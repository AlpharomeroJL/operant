//! [`FixtureBrowser`]: reads a fixture webapp HTML file straight off disk
//! and answers [`super::Browser`] calls deterministically off the parsed
//! element table -- no real browser, DOM, or network involved. Always
//! built (no `cdp` feature needed), mirroring
//! `operant-perception-uia::fixture::FixturePerceiver`'s precedent for
//! C2 (`crates/perception-uia/src/fixture.rs`): "Always built... so
//! every lane -- and `cargo test`... -- gets a working [backend]
//! headless."

use std::path::{Path, PathBuf};

use operant_ir::snapshot::{Bounds, Element, Role, Snapshot, SnapshotSource, WindowInfo};
use operant_ir::{
    Action, ActionKind, Grounding, NameRoleSeg, OrdinalSeg, Pace, Retry, RiskClass, Selector,
    Target,
};
use parking_lot::Mutex;

use super::digest::compute_digest;
use super::html::{extract_page, PageFields};
use super::{Browser, BrowserAct, BrowserError};

/// `window.process` every [`FixtureBrowser`] snapshot reports, matching
/// the process key `operant-perception-uia`'s own drift fixture timeline
/// already uses for this same fixture webapp
/// (`crates/perception-uia/src/fixture.rs` tests,
/// `contracts/fixtures/drift_renamed_button/{before,after}.json`).
const PROCESS: &str = "fixture-webapp";

/// Fixed layout constants matching
/// `contracts/fixtures/drift_renamed_button/before.json` exactly (there
/// is no real layout engine here, so positions are assigned by
/// structural order: document, then each tracked input top to bottom,
/// then the submit button, then the results list).
const DOC_W: f64 = 1024.0;
const DOC_H: f64 = 768.0;
const FIRST_INPUT_Y: f64 = 120.0;
const INPUT_ROW_H: f64 = 50.0;
const INPUT_H: f64 = 32.0;
const BUTTON_GAP: f64 = 50.0;
const BUTTON_W: f64 = 140.0;
const BUTTON_H: f64 = 36.0;
const LIST_GAP: f64 = 70.0;
const LIST_W: f64 = 500.0;
const LIST_H: f64 = 300.0;
const FIELD_X: f64 = 40.0;
const FIELD_W: f64 = 300.0;
const MONITOR: &str = "MON1";

#[derive(Debug, Clone, Default)]
struct PageState {
    title: String,
    elements: Vec<Element>,
}

/// A [`Browser`] backed by a parsed fixture HTML file
/// (`contracts/fixtures/webapp/index.html` / `drift.html`). `attach`
/// parses the file's form inputs, submit button, and aria-labelled list
/// into a flat element table shaped like
/// `contracts/fixtures/drift_renamed_button/before.json` (minus its
/// placeholder digest and `source: "fixture"`, since this backend
/// legitimately observed the page itself: `source: "browser"`);
/// `snapshot` and `act` answer purely off that table.
#[derive(Default)]
pub struct FixtureBrowser {
    state: Mutex<Option<PageState>>,
}

impl FixtureBrowser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach from HTML text already in memory rather than a file on
    /// disk. [`Browser::attach`] (the trait entry point) is the
    /// disk-path convenience built on top of this.
    pub fn attach_html(&self, html: &str) -> Result<(), BrowserError> {
        let fields = extract_page(html);
        *self.state.lock() = Some(PageState {
            title: fields.title.clone(),
            elements: build_elements(&fields),
        });
        Ok(())
    }

    /// `contracts/fixtures/webapp` relative to the workspace root,
    /// resolved via `CARGO_MANIFEST_DIR` so it works regardless of the
    /// process's current directory (mirrors
    /// `FixtureMailStore::open_default_fixtures`,
    /// `crates/action/src/adapters/email/store.rs`).
    pub fn fixtures_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/fixtures/webapp")
    }
}

impl Browser for FixtureBrowser {
    fn attach(&self, target: &str) -> Result<(), BrowserError> {
        let raw = std::fs::read_to_string(target).map_err(|source| BrowserError::Io {
            path: target.to_string(),
            source,
        })?;
        self.attach_html(&raw)
    }

    fn snapshot(&self) -> Result<Snapshot, BrowserError> {
        let guard = self.state.lock();
        let page = guard.as_ref().ok_or(BrowserError::NotAttached)?;
        Ok(build_snapshot(page))
    }

    fn act(&self, act: &BrowserAct) -> Result<Action, BrowserError> {
        if !matches!(
            act.kind,
            ActionKind::Click | ActionKind::Type | ActionKind::Assert
        ) {
            return Err(BrowserError::UnsupportedKind(act.kind));
        }

        let mut guard = self.state.lock();
        let page = guard.as_mut().ok_or(BrowserError::NotAttached)?;
        let idx =
            resolve_selector(&page.elements, &act.selector).ok_or(BrowserError::SelectorMiss)?;

        match act.kind {
            ActionKind::Type => {
                let text = act
                    .params
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or(BrowserError::MissingArg("text"))?;
                page.elements[idx].value = Some(text.to_string());
            }
            ActionKind::Click => {
                if !page.elements[idx].patterns.iter().any(|p| p == "invoke") {
                    return Err(BrowserError::NotInvokable);
                }
            }
            ActionKind::Assert => {
                let element = &page.elements[idx];
                if let Some(expected) = act.params.get("expect_name").and_then(|v| v.as_str()) {
                    if element.name != expected {
                        return Err(BrowserError::AssertionFailed(format!(
                            "expected name `{expected}`, got `{}`",
                            element.name
                        )));
                    }
                }
                if let Some(expected) = act.params.get("expect_value").and_then(|v| v.as_str()) {
                    let actual = element.value.as_deref().unwrap_or("");
                    if actual != expected {
                        return Err(BrowserError::AssertionFailed(format!(
                            "expected value `{expected}`, got `{actual}`"
                        )));
                    }
                }
            }
            _ => unreachable!("kind checked above"),
        }

        record_action(act, &page.elements[idx])
    }
}

/// Find the element whose selector list contains `selector` verbatim (any
/// kind: automation id, css, name/role path, or ordinal path all work,
/// same as a real selector-chain resolve, just without a layout engine
/// behind it).
fn resolve_selector(elements: &[Element], selector: &Selector) -> Option<usize> {
    elements.iter().position(|e| e.selectors.contains(selector))
}

/// Build the Action IR record of one DOM interaction: a css-first
/// `target.selectors` (`docs/ARCHITECTURE.md` C5: "DOM actions emitted as
/// Action IR"), falling back to the element's automation id, then
/// whatever else it has.
fn record_action(act: &BrowserAct, element: &Element) -> Result<Action, BrowserError> {
    let mut selectors = Vec::new();
    if let Some(css) = element
        .selectors
        .iter()
        .find(|s| matches!(s, Selector::Css { .. }))
    {
        selectors.push(css.clone());
    }
    if let Some(auto) = element
        .selectors
        .iter()
        .find(|s| matches!(s, Selector::AutomationId { .. }))
    {
        selectors.push(auto.clone());
    }
    if selectors.is_empty() {
        selectors = element.selectors.clone();
    }

    let risk_class = match act.kind {
        ActionKind::Click | ActionKind::Type => RiskClass::Write,
        _ => RiskClass::Read,
    };

    Ok(Action {
        v: 1,
        id: act.id.clone(),
        kind: act.kind,
        intent: None,
        target: Some(Target {
            window: None,
            selectors,
            anchor: None,
            coords_last_known: None,
        }),
        params: act.params.clone(),
        pace: Pace::Instant,
        risk_class,
        irreversible: false,
        grounding: Grounding::Adapter,
        timeout_ms: 5000,
        retry: Retry::default(),
    })
}

fn build_elements(fields: &PageFields) -> Vec<Element> {
    let doc_name = fields.title.clone();
    let mut elements = vec![Element {
        idx: 0,
        parent: None,
        role: Role::Document,
        name: doc_name.clone(),
        value: None,
        automation_id: None,
        bounds: Some(Bounds {
            x: 0.0,
            y: 0.0,
            w: DOC_W,
            h: DOC_H,
            monitor: Some(MONITOR.into()),
        }),
        enabled: true,
        offscreen: false,
        is_password: false,
        patterns: vec![],
        selectors: vec![Selector::NameRolePath {
            path: vec![NameRoleSeg {
                role: "document".into(),
                name: doc_name.clone(),
            }],
        }],
    }];

    let mut y = FIRST_INPUT_Y;
    for input in &fields.inputs {
        let idx = elements.len() as u32;
        elements.push(Element {
            idx,
            parent: Some(0),
            role: Role::Edit,
            name: input.label.clone(),
            value: Some(String::new()),
            automation_id: Some(input.id.clone()),
            bounds: Some(Bounds {
                x: FIELD_X,
                y,
                w: FIELD_W,
                h: INPUT_H,
                monitor: Some(MONITOR.into()),
            }),
            enabled: true,
            offscreen: false,
            is_password: input.input_type == "password",
            patterns: vec!["value".into()],
            selectors: vec![
                Selector::AutomationId {
                    value: input.id.clone(),
                },
                Selector::Css {
                    value: format!("#{}", input.id),
                },
                Selector::NameRolePath {
                    path: vec![
                        NameRoleSeg {
                            role: "document".into(),
                            name: doc_name.clone(),
                        },
                        NameRoleSeg {
                            role: "edit".into(),
                            name: input.label.clone(),
                        },
                    ],
                },
            ],
        });
        y += INPUT_ROW_H;
    }

    if let Some(button) = &fields.button {
        let idx = elements.len() as u32;
        let button_y = y - INPUT_ROW_H + BUTTON_GAP;
        elements.push(Element {
            idx,
            parent: Some(0),
            role: Role::Button,
            name: button.label.clone(),
            value: None,
            automation_id: Some(button.id.clone()),
            bounds: Some(Bounds {
                x: FIELD_X,
                y: button_y,
                w: BUTTON_W,
                h: BUTTON_H,
                monitor: Some(MONITOR.into()),
            }),
            enabled: true,
            offscreen: false,
            is_password: false,
            patterns: vec!["invoke".into()],
            selectors: vec![
                Selector::AutomationId {
                    value: button.id.clone(),
                },
                Selector::Css {
                    value: format!("#{}", button.id),
                },
                Selector::NameRolePath {
                    path: vec![
                        NameRoleSeg {
                            role: "document".into(),
                            name: doc_name.clone(),
                        },
                        NameRoleSeg {
                            role: "button".into(),
                            name: button.label.clone(),
                        },
                    ],
                },
                Selector::OrdinalPath {
                    path: vec![
                        OrdinalSeg {
                            role: "document".into(),
                            ordinal: 0,
                        },
                        OrdinalSeg {
                            role: "button".into(),
                            ordinal: 0,
                        },
                    ],
                },
            ],
        });

        if let Some(list) = &fields.list {
            let idx = elements.len() as u32;
            elements.push(Element {
                idx,
                parent: Some(0),
                role: Role::List,
                name: list.label.clone(),
                value: None,
                automation_id: Some(list.id.clone()),
                bounds: Some(Bounds {
                    x: FIELD_X,
                    y: button_y + LIST_GAP,
                    w: LIST_W,
                    h: LIST_H,
                    monitor: Some(MONITOR.into()),
                }),
                enabled: true,
                offscreen: false,
                is_password: false,
                patterns: vec![],
                selectors: vec![
                    Selector::AutomationId {
                        value: list.id.clone(),
                    },
                    Selector::Css {
                        value: format!("#{}", list.id),
                    },
                    Selector::NameRolePath {
                        path: vec![
                            NameRoleSeg {
                                role: "document".into(),
                                name: doc_name.clone(),
                            },
                            NameRoleSeg {
                                role: "list".into(),
                                name: list.label.clone(),
                            },
                        ],
                    },
                ],
            });
        }
    } else if let Some(list) = &fields.list {
        // No button but a list: still emit it, anchored under the last input.
        let idx = elements.len() as u32;
        elements.push(Element {
            idx,
            parent: Some(0),
            role: Role::List,
            name: list.label.clone(),
            value: None,
            automation_id: Some(list.id.clone()),
            bounds: Some(Bounds {
                x: FIELD_X,
                y,
                w: LIST_W,
                h: LIST_H,
                monitor: Some(MONITOR.into()),
            }),
            enabled: true,
            offscreen: false,
            is_password: false,
            patterns: vec![],
            selectors: vec![
                Selector::AutomationId {
                    value: list.id.clone(),
                },
                Selector::Css {
                    value: format!("#{}", list.id),
                },
                Selector::NameRolePath {
                    path: vec![
                        NameRoleSeg {
                            role: "document".into(),
                            name: doc_name.clone(),
                        },
                        NameRoleSeg {
                            role: "list".into(),
                            name: list.label.clone(),
                        },
                    ],
                },
            ],
        });
    }

    elements
}

fn build_snapshot(page: &PageState) -> Snapshot {
    Snapshot {
        v: 1,
        source: SnapshotSource::Browser,
        window: WindowInfo {
            hwnd: None,
            process: PROCESS.to_string(),
            title: page.title.clone(),
            monitor: Some(MONITOR.into()),
            dpi_scale: 1.0,
        },
        digest: compute_digest(&page.elements),
        truncated: false,
        captured_ms: None,
        elements: page.elements.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_html() -> String {
        std::fs::read_to_string(FixtureBrowser::fixtures_dir().join("index.html")).unwrap()
    }

    fn drift_html() -> String {
        std::fs::read_to_string(FixtureBrowser::fixtures_dir().join("drift.html")).unwrap()
    }

    #[test]
    fn snapshot_matches_the_before_json_element_shape() {
        let fx = FixtureBrowser::new();
        fx.attach_html(&index_html()).unwrap();
        let snap = fx.snapshot().unwrap();

        assert_eq!(snap.source, SnapshotSource::Browser);
        assert_eq!(snap.window.process, "fixture-webapp");
        assert_eq!(snap.window.title, "Operant Fixture Invoices");
        assert_eq!(
            snap.elements.len(),
            5,
            "document, customer, amount, button, list"
        );

        let doc = &snap.elements[0];
        assert_eq!(doc.role, Role::Document);
        assert_eq!(doc.parent, None);

        let customer = &snap.elements[1];
        assert_eq!(customer.role, Role::Edit);
        assert_eq!(customer.name, "Customer");
        assert_eq!(customer.automation_id.as_deref(), Some("customer"));
        assert_eq!(customer.parent, Some(0));
        assert!(customer.selectors.contains(&Selector::Css {
            value: "#customer".into()
        }));
        assert!(customer.selectors.contains(&Selector::AutomationId {
            value: "customer".into()
        }));

        let amount = &snap.elements[2];
        assert_eq!(amount.name, "Amount");
        assert_eq!(amount.automation_id.as_deref(), Some("amount"));

        let button = &snap.elements[3];
        assert_eq!(button.role, Role::Button);
        assert_eq!(button.name, "Save invoice");
        assert_eq!(button.automation_id.as_deref(), Some("save-btn"));
        assert_eq!(button.patterns, vec!["invoke".to_string()]);
        assert!(button.selectors.contains(&Selector::Css {
            value: "#save-btn".into()
        }));

        let list = &snap.elements[4];
        assert_eq!(list.role, Role::List);
        assert_eq!(list.name, "Saved invoices");
        assert_eq!(list.automation_id.as_deref(), Some("invoice-list"));

        // No `date` input tracked, matching
        // contracts/fixtures/drift_renamed_button/before.json's shape.
        assert!(snap
            .elements
            .iter()
            .all(|e| e.automation_id.as_deref() != Some("date")));
    }

    #[test]
    fn drift_variant_renames_the_button_and_the_digest_changes() {
        let index = FixtureBrowser::new();
        index.attach_html(&index_html()).unwrap();
        let before = index.snapshot().unwrap();

        let drift = FixtureBrowser::new();
        drift.attach_html(&drift_html()).unwrap();
        let after = drift.snapshot().unwrap();

        // Everything upstream of the button is untouched.
        assert_eq!(before.elements[0].name, after.elements[0].name);
        assert_eq!(before.elements[1], after.elements[1]);
        assert_eq!(before.elements[2], after.elements[2]);

        let before_btn = &before.elements[3];
        let after_btn = &after.elements[3];
        assert_eq!(before_btn.name, "Save invoice");
        assert_eq!(after_btn.name, "Store invoice");
        assert_eq!(before_btn.automation_id.as_deref(), Some("save-btn"));
        assert_eq!(after_btn.automation_id.as_deref(), Some("store-btn"));

        // The list survives the rename untouched too.
        assert_eq!(before.elements[4], after.elements[4]);

        assert_ne!(
            before.digest, after.digest,
            "the diff must be detectable via the digest"
        );
    }

    #[test]
    fn click_type_assert_sequence_emits_the_expected_action_ir() {
        let fx = FixtureBrowser::new();
        fx.attach_html(&index_html()).unwrap();

        let mut type_params = serde_json::Map::new();
        type_params.insert("text".into(), serde_json::json!("Acme Corp"));
        let type_action = fx
            .act(&BrowserAct {
                id: "step-1".into(),
                kind: ActionKind::Type,
                selector: Selector::Css {
                    value: "#customer".into(),
                },
                params: type_params,
            })
            .unwrap();
        assert_eq!(type_action.kind, ActionKind::Type);
        assert_eq!(type_action.risk_class, RiskClass::Write);
        assert_eq!(type_action.grounding, Grounding::Adapter);
        let target = type_action.target.expect("target");
        assert_eq!(
            target.selectors[0],
            Selector::Css {
                value: "#customer".into()
            }
        );
        assert_eq!(type_action.params.get("text").unwrap(), "Acme Corp");

        // The typed value is now live in the page: a fresh snapshot sees it.
        let snap = fx.snapshot().unwrap();
        assert_eq!(snap.elements[1].value.as_deref(), Some("Acme Corp"));

        let click_action = fx
            .act(&BrowserAct {
                id: "step-2".into(),
                kind: ActionKind::Click,
                selector: Selector::Css {
                    value: "#save-btn".into(),
                },
                params: serde_json::Map::new(),
            })
            .unwrap();
        assert_eq!(click_action.kind, ActionKind::Click);
        assert_eq!(click_action.risk_class, RiskClass::Write);
        assert_eq!(
            click_action.target.unwrap().selectors[0],
            Selector::Css {
                value: "#save-btn".into()
            }
        );

        let mut assert_params = serde_json::Map::new();
        assert_params.insert("expect_name".into(), serde_json::json!("Save invoice"));
        let assert_action = fx
            .act(&BrowserAct {
                id: "step-3".into(),
                kind: ActionKind::Assert,
                selector: Selector::Css {
                    value: "#save-btn".into(),
                },
                params: assert_params,
            })
            .unwrap();
        assert_eq!(assert_action.kind, ActionKind::Assert);
        assert_eq!(assert_action.risk_class, RiskClass::Read);
    }

    #[test]
    fn assert_mismatch_is_a_typed_failure() {
        let fx = FixtureBrowser::new();
        fx.attach_html(&index_html()).unwrap();
        let mut params = serde_json::Map::new();
        params.insert("expect_name".into(), serde_json::json!("Nope"));
        let err = fx
            .act(&BrowserAct {
                id: "a1".into(),
                kind: ActionKind::Assert,
                selector: Selector::Css {
                    value: "#save-btn".into(),
                },
                params,
            })
            .unwrap_err();
        assert!(matches!(err, BrowserError::AssertionFailed(_)));
    }

    #[test]
    fn act_before_attach_is_not_attached() {
        let fx = FixtureBrowser::new();
        let err = fx
            .act(&BrowserAct {
                id: "a1".into(),
                kind: ActionKind::Click,
                selector: Selector::Css {
                    value: "#save-btn".into(),
                },
                params: serde_json::Map::new(),
            })
            .unwrap_err();
        assert!(matches!(err, BrowserError::NotAttached));

        let err = fx.snapshot().unwrap_err();
        assert!(matches!(err, BrowserError::NotAttached));
    }

    #[test]
    fn unknown_selector_is_a_selector_miss() {
        let fx = FixtureBrowser::new();
        fx.attach_html(&index_html()).unwrap();
        let err = fx
            .act(&BrowserAct {
                id: "a1".into(),
                kind: ActionKind::Click,
                selector: Selector::Css {
                    value: "#does-not-exist".into(),
                },
                params: serde_json::Map::new(),
            })
            .unwrap_err();
        assert!(matches!(err, BrowserError::SelectorMiss));
    }

    #[test]
    fn click_on_a_non_invokable_element_is_refused() {
        let fx = FixtureBrowser::new();
        fx.attach_html(&index_html()).unwrap();
        let err = fx
            .act(&BrowserAct {
                id: "a1".into(),
                kind: ActionKind::Click,
                selector: Selector::Css {
                    value: "#customer".into(),
                },
                params: serde_json::Map::new(),
            })
            .unwrap_err();
        assert!(matches!(err, BrowserError::NotInvokable));
    }

    #[test]
    fn attach_reads_a_real_file_from_disk() {
        let fx = FixtureBrowser::new();
        fx.attach(
            FixtureBrowser::fixtures_dir()
                .join("index.html")
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(fx.snapshot().unwrap().elements.len(), 5);
    }

    #[test]
    fn attach_unknown_path_is_a_typed_io_error() {
        let fx = FixtureBrowser::new();
        let err = fx.attach("Z:/definitely/not/a/real/path.html").unwrap_err();
        assert!(matches!(err, BrowserError::Io { .. }));
    }
}
