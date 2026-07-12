//! Playwright spec importer (X9): a small, hand-rolled parser for the basic
//! subset of a Playwright test file this build supports (`goto`, `click`,
//! `fill`, `expect(...).toHaveValue(...)`/`.toHaveText(...)`), not a full
//! Playwright AST implementation.
//!
//! Each recognized statement is replayed against a [`FixtureBrowser`]
//! attached to the target page, through [`Browser::act`], the exact call
//! the `browser` namespace adapter (`crates/action/src/adapters/browser`,
//! L9A, merged) already uses to turn a DOM interaction into Action IR. The
//! resulting actions are assembled into an `operant_compiler::Trajectory`
//! (the compiler's input shape) and run through `operant_compiler::compile`
//! (L8A, merged) to emit a workflow skeleton, exactly the same pipeline a
//! live recording session feeds.
//!
//! Anything this basic parser cannot map onto goto/click/fill/expect (a
//! desktop-only concern the import cannot infer, such as checking a
//! downloaded file, or simply a Playwright call this parser does not
//! recognize) becomes a `TODO` marker step in the emitted workflow: a
//! harmless `wait` action whose intent names the unmapped source line, so
//! the import never silently drops a step or crashes on one.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use operant_action::adapters::browser::{Browser, BrowserAct, BrowserError, FixtureBrowser};
use operant_compiler::{compile, CompileError, Compilation, RunMeta, Trajectory, TrajectoryStep};
use operant_ir::{Action, ActionKind, Element, Grounding, Pace, RiskClass, Retry, Selector};
use serde_json::{json, Map, Value};
use thiserror::Error;

/// One recognized (or unrecognized) statement in the spec, in source order.
#[derive(Debug, Clone, PartialEq)]
enum ParsedStep {
    Goto(String),
    Click(String),
    Fill(String, String),
    ExpectValue(String, String),
    ExpectText(String, String),
    /// A `page.*` or `expect(...)` statement this basic parser does not
    /// recognize. Carries the original (trimmed) source line verbatim so
    /// the emitted TODO step names exactly what a human needs to look at.
    Todo(String),
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("no recognizable Playwright steps found in the spec")]
    Empty,
    #[error("replaying `{line}` against the fixture browser failed: {source}")]
    Browser {
        line: String,
        #[source]
        source: BrowserError,
    },
    #[error(transparent)]
    Compile(#[from] CompileError),
}

/// Everything one `import` call produces: the compiled workflow, plus a
/// plain-English note for every step this parser could not map (empty when
/// every statement in the spec mapped cleanly).
pub struct ImportOutcome {
    pub compilation: Compilation,
    pub todo_notes: Vec<String>,
}

/// Import one Playwright spec's source text into a compiled workflow.
///
/// `spec_dir` is the directory the spec file lives in; a relative `goto`
/// target resolves against it (see [`resolve_goto_target`]), matching how a
/// browser resolves a relative navigation against the page it is loaded
/// from.
pub fn import(spec_text: &str, spec_dir: &Path) -> Result<ImportOutcome, ImportError> {
    let parsed = parse(spec_text);
    if parsed.steps.is_empty() {
        return Err(ImportError::Empty);
    }

    let browser = FixtureBrowser::new();
    let mut trajectory_steps: Vec<TrajectoryStep> = Vec::new();
    let mut todo_notes: Vec<String> = Vec::new();
    let mut last_assert_idx: Option<usize> = None;
    let mut seq: u32 = 0;

    for step in &parsed.steps {
        match step {
            ParsedStep::Goto(target) => {
                let resolved = resolve_goto_target(target, spec_dir);
                browser
                    .attach(&resolved)
                    .map_err(|source| ImportError::Browser {
                        line: format!("page.goto({target:?})"),
                        source,
                    })?;
                // `goto` sets up the session; it has no Action IR shape of
                // its own (nothing a replay executor would dispatch), so no
                // trajectory step is pushed for it.
            }
            ParsedStep::Click(selector) => {
                seq += 1;
                let id = format!("import-step-{seq}");
                let label = element_label(&browser, selector);
                let (mut action, before, after) =
                    act(&browser, &id, ActionKind::Click, selector, Map::new()).map_err(
                        |source| ImportError::Browser {
                            line: format!("page.click({selector:?})"),
                            source,
                        },
                    )?;
                action.intent = Some(format!("Click {label}"));
                trajectory_steps.push(browser_step(seq, action, before, after, false));
            }
            ParsedStep::Fill(selector, text) => {
                seq += 1;
                let id = format!("import-step-{seq}");
                let label = element_label(&browser, selector);
                let mut params = Map::new();
                params.insert("text".to_string(), Value::String(text.clone()));
                let (mut action, before, after) =
                    act(&browser, &id, ActionKind::Type, selector, params).map_err(|source| {
                        ImportError::Browser {
                            line: format!("page.fill({selector:?}, {text:?})"),
                            source,
                        }
                    })?;
                action.intent = Some(format!("Fill {label}"));
                trajectory_steps.push(browser_step(seq, action, before, after, false));
            }
            ParsedStep::ExpectValue(selector, expected) => {
                seq += 1;
                let id = format!("import-step-{seq}");
                let label = element_label(&browser, selector);
                let element = find_element(&browser, selector);
                let mut params = Map::new();
                params.insert("expect_value".to_string(), Value::String(expected.clone()));
                let (mut action, before, after) =
                    act(&browser, &id, ActionKind::Assert, selector, params).map_err(
                        |source| ImportError::Browser {
                            line: format!(
                                "expect(page.locator({selector:?})).toHaveValue({expected:?})"
                            ),
                            source,
                        },
                    )?;
                action.intent = Some(format!("Check {label}"));
                if let Some(el) = &element {
                    action
                        .params
                        .insert("expr".to_string(), value_equals_expr(el, expected));
                }
                trajectory_steps.push(browser_step(seq, action, before, after, false));
                last_assert_idx = Some(trajectory_steps.len() - 1);
            }
            ParsedStep::ExpectText(selector, expected) => {
                seq += 1;
                let id = format!("import-step-{seq}");
                let label = element_label(&browser, selector);
                let element = find_element(&browser, selector);
                let mut params = Map::new();
                params.insert("expect_name".to_string(), Value::String(expected.clone()));
                let (mut action, before, after) =
                    act(&browser, &id, ActionKind::Assert, selector, params).map_err(
                        |source| ImportError::Browser {
                            line: format!(
                                "expect(page.locator({selector:?})).toHaveText({expected:?})"
                            ),
                            source,
                        },
                    )?;
                action.intent = Some(format!("Check {label}"));
                if let Some(el) = &element {
                    action
                        .params
                        .insert("expr".to_string(), named_exists_expr(el, expected));
                }
                trajectory_steps.push(browser_step(seq, action, before, after, false));
                last_assert_idx = Some(trajectory_steps.len() - 1);
            }
            ParsedStep::Todo(line) => {
                seq += 1;
                todo_notes.push(line.clone());
                trajectory_steps.push(browser_step(
                    seq,
                    todo_action(seq, line),
                    None,
                    None,
                    false,
                ));
            }
        }
    }

    // The final assert (if any) is the outcome-bearing step: pass 4
    // (`operant_compiler`) surfaces it as the workflow postcondition,
    // exactly like the last assert in a recorded trajectory.
    if let Some(idx) = last_assert_idx {
        trajectory_steps[idx].outcome_bearing = true;
    }

    let run_id = format!("import-playwright-{:016x}", spec_hash(spec_text));
    let goal = parsed
        .goal
        .unwrap_or_else(|| "Imported Playwright spec".to_string());
    let traj = Trajectory {
        v: 1,
        description: Some(
            "Imported from a Playwright spec by `operant import playwright` (X9).".to_string(),
        ),
        run: RunMeta {
            id: run_id.clone(),
            goal,
            mode: Some("import".to_string()),
            status: Some("completed".to_string()),
        },
        steps: trajectory_steps,
    };

    let compilation = compile(&traj)?;
    Ok(ImportOutcome {
        compilation,
        todo_notes,
    })
}

/// Resolve a `goto` argument to a path [`FixtureBrowser::attach`] can open:
/// a `file://` URL has its scheme stripped, an absolute path is used
/// verbatim, and a relative path resolves against the spec's own directory
/// (the fixture spec under `contracts/fixtures/playwright/` reaches the
/// webapp fixture this way, e.g. `../webapp/index.html`).
fn resolve_goto_target(raw: &str, spec_dir: &Path) -> String {
    if let Some(path) = raw.strip_prefix("file://") {
        return path.to_string();
    }
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        return raw.to_string();
    }
    spec_dir.join(candidate).to_string_lossy().into_owned()
}

/// Call [`Browser::act`], bracketed by a snapshot before and after so the
/// resulting trajectory step carries the digests pass 4 needs to decide
/// whether a wait belongs after it.
fn act(
    browser: &FixtureBrowser,
    id: &str,
    kind: ActionKind,
    selector: &str,
    params: Map<String, Value>,
) -> Result<(Action, Option<String>, Option<String>), BrowserError> {
    let before = browser.snapshot().ok().map(|s| s.digest);
    let action = browser.act(&BrowserAct {
        id: id.to_string(),
        kind,
        selector: Selector::Css {
            value: selector.to_string(),
        },
        params,
    })?;
    let after = browser.snapshot().ok().map(|s| s.digest);
    Ok((action, before, after))
}

fn browser_step(
    seq: u32,
    action: Action,
    before: Option<String>,
    after: Option<String>,
    outcome_bearing: bool,
) -> TrajectoryStep {
    TrajectoryStep {
        seq,
        action,
        snapshot_digest_before: before,
        snapshot_digest_after: after,
        outcome: Some("ok".to_string()),
        ms: None,
        note: None,
        human_correction: None,
        outcome_bearing,
    }
}

/// The element the fixture browser's current page has under `selector`, if
/// any, used to build the compiled assert's gate `expr` (the css selector
/// itself is not addressable by `operant_gates`, which reads role/name off
/// the perception snapshot the same way UIA-grounded asserts do).
fn find_element(browser: &FixtureBrowser, selector: &str) -> Option<Element> {
    let snap = browser.snapshot().ok()?;
    let css = Selector::Css {
        value: selector.to_string(),
    };
    snap.elements.into_iter().find(|e| e.selectors.contains(&css))
}

/// A human-readable name for a step's intent: the target element's
/// accessible name when the fixture page has one, else the raw css
/// selector, so a compiled step always reads as something more useful than
/// a blank line in `manifest.step_summary` and the emitted `workflow.ts`
/// comment.
fn element_label(browser: &FixtureBrowser, selector: &str) -> String {
    find_element(browser, selector)
        .map(|e| e.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| selector.to_string())
}

fn role_str(role: operant_ir::Role) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// `expect(...).toHaveValue(expected)` becomes an `equals` gate over the
/// element's `snapshot_element_value`, the same query kind
/// `notepad-invoice-note`'s own final assert uses.
fn value_equals_expr(el: &Element, expected: &str) -> Value {
    json!({
        "op": "equals",
        "left": {
            "kind": "snapshot_element_value",
            "role": role_str(el.role),
            "name": el.name,
        },
        "right": { "kind": "literal", "value": expected }
    })
}

/// `expect(...).toHaveText(expected)` becomes an `exists` gate for an
/// element of the same role bearing `expected` as its accessible name: the
/// gate language addresses an element's value, not its rendered text, so
/// existence-by-name is the closest available check for a text assertion.
fn named_exists_expr(el: &Element, expected: &str) -> Value {
    json!({
        "op": "exists",
        "query": {
            "kind": "snapshot_element",
            "role": role_str(el.role),
            "name": expected,
        }
    })
}

/// The placeholder step an unmapped statement becomes: a no-op `wait` (the
/// only Action IR kind with no side effect) whose intent names exactly what
/// could not be imported, so it shows up in the manifest step summary and
/// the emitted `workflow.ts` for a human to fill in by hand.
fn todo_action(seq: u32, source_line: &str) -> Action {
    let mut params = Map::new();
    params.insert("todo".to_string(), Value::Bool(true));
    params.insert(
        "source_line".to_string(),
        Value::String(source_line.to_string()),
    );
    Action {
        v: 1,
        id: format!("import-step-{seq}-todo"),
        kind: ActionKind::Wait,
        intent: Some(format!(
            "TODO: could not import `{source_line}`; fill in the equivalent step by hand"
        )),
        target: None,
        params,
        pace: Pace::Instant,
        risk_class: RiskClass::Read,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Retry {
            attempts: 0,
            backoff_ms: 0,
        },
    }
}

fn spec_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

// ---- parsing -----------------------------------------------------------

struct ParsedSpec {
    steps: Vec<ParsedStep>,
    goal: Option<String>,
}

fn parse(spec_text: &str) -> ParsedSpec {
    let mut goal = None;
    let mut steps = Vec::new();
    for raw_line in spec_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if goal.is_none() {
            if let Some(g) = extract_test_title(line) {
                goal = Some(g);
            }
        }
        let Some(stmt) = line.strip_prefix("await ") else {
            continue;
        };
        let stmt = stmt.trim_end_matches(';').trim();
        if let Some(step) = parse_statement(stmt) {
            steps.push(step);
        }
    }
    ParsedSpec { steps, goal }
}

/// The first quoted string literal right after a `test(` prefix on one
/// line, used as the workflow goal. Deliberately does not try to find the
/// whole `test(...)` call's matching close paren (it spans many lines, an
/// arrow function body): the title is always the call's first argument.
fn extract_test_title(line: &str) -> Option<String> {
    let rest = line.strip_prefix("test(")?;
    first_quoted_literal(rest)
}

fn first_quoted_literal(s: &str) -> Option<String> {
    let s = s.trim_start();
    let mut chars = s.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    for (i, c) in chars {
        if c == quote {
            return Some(s[quote.len_utf8()..i].to_string());
        }
    }
    None
}

/// Dispatch one `await`-stripped statement to the step it names, or
/// [`ParsedStep::Todo`] for a `page.*`/`expect(...)` shape this parser does
/// not recognize. Anything that is not a `page.*` or `expect(...)` call at
/// all (test scaffolding: `mkdir`, `browser.close()`, ...) is not a
/// workflow action and is ignored rather than flagged.
fn parse_statement(stmt: &str) -> Option<ParsedStep> {
    if let Some((inner, _)) = strip_call(stmt, "page.goto(") {
        return Some(match first_str_arg(inner) {
            Some(url) => ParsedStep::Goto(url),
            None => ParsedStep::Todo(stmt.to_string()),
        });
    }
    if let Some((inner, _)) = strip_call(stmt, "page.click(") {
        return Some(match first_str_arg(inner) {
            Some(sel) => ParsedStep::Click(sel),
            None => ParsedStep::Todo(stmt.to_string()),
        });
    }
    if let Some((inner, _)) = strip_call(stmt, "page.fill(") {
        let args = str_args(inner);
        return Some(if args.len() >= 2 {
            ParsedStep::Fill(args[0].clone(), args[1].clone())
        } else {
            ParsedStep::Todo(stmt.to_string())
        });
    }
    if stmt.starts_with("expect(") {
        return Some(parse_expect(stmt).unwrap_or_else(|| ParsedStep::Todo(stmt.to_string())));
    }
    if stmt.starts_with("page.") {
        return Some(ParsedStep::Todo(stmt.to_string()));
    }
    None
}

fn parse_expect(stmt: &str) -> Option<ParsedStep> {
    let (locator_call, tail) = strip_call(stmt, "expect(")?;
    let (loc_inner, _) = strip_call(locator_call, "page.locator(")?;
    let selector = first_str_arg(loc_inner)?;
    let tail = tail.trim();
    if let Some((inner, _)) = strip_call(tail, ".toHaveValue(") {
        return Some(ParsedStep::ExpectValue(selector, first_str_arg(inner)?));
    }
    if let Some((inner, _)) = strip_call(tail, ".toHaveText(") {
        return Some(ParsedStep::ExpectText(selector, first_str_arg(inner)?));
    }
    None
}

/// If `stmt` starts with `marker` (a call opener like `"page.click("`, with
/// the marker's own opening paren already included), return the substring
/// between that paren and its match, plus everything after the matching
/// close paren.
fn strip_call<'a>(stmt: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let rest = stmt.strip_prefix(marker)?;
    let end = matching_paren_end(rest)?;
    Some((&rest[..end], &rest[end + 1..]))
}

/// Byte offset of the `)` that closes the paren already consumed by the
/// caller (depth starts at 1), skipping over parens inside quoted strings.
/// `None` when the statement's close paren is not on this line (an
/// unsupported multi-line call).
fn matching_paren_end(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_str: Option<char> = None;
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        if let Some(q) = in_str {
            if c == '\\' {
                chars.next();
                continue;
            }
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => in_str = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a call's argument list on top-level commas (not inside a quoted
/// string or nested brackets) and unquote each plain string-literal
/// argument. A non-string-literal argument (an object, a variable) is
/// dropped rather than guessed at; callers that need every argument to be a
/// string literal treat a short result as "unsupported".
fn str_args(inner: &str) -> Vec<String> {
    let mut raw_args: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut start = 0usize;
    let mut chars = inner.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if let Some(q) = in_str {
            if c == '\\' {
                chars.next();
                continue;
            }
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => in_str = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                raw_args.push(inner[start..i].to_string());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    let tail = inner[start..].to_string();
    if !tail.trim().is_empty() || !raw_args.is_empty() {
        raw_args.push(tail);
    }
    raw_args
        .into_iter()
        .filter_map(|a| unquote(a.trim()))
        .collect()
}

fn first_str_arg(inner: &str) -> Option<String> {
    str_args(inner).into_iter().next()
}

fn unquote(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'\'' || first == b'"' || first == b'`') && first == last {
            return Some(s[1..s.len() - 1].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_spec_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../contracts/fixtures/playwright/save_invoice.spec.ts")
    }

    fn fixture_spec_text() -> String {
        std::fs::read_to_string(fixture_spec_path()).expect("fixture spec reads")
    }

    #[test]
    fn parses_goto_fill_fill_click_expect_from_the_fixture_spec() {
        let parsed = parse(&fixture_spec_text());
        assert_eq!(
            parsed.goal.as_deref(),
            Some("fills the invoice form and saves it")
        );
        assert_eq!(
            parsed.steps,
            vec![
                ParsedStep::Goto("../webapp/index.html".to_string()),
                ParsedStep::Fill("#customer".to_string(), "Acme Corp".to_string()),
                ParsedStep::Fill("#amount".to_string(), "142.50".to_string()),
                ParsedStep::Click("#save-btn".to_string()),
                ParsedStep::ExpectValue("#customer".to_string(), "Acme Corp".to_string()),
            ]
        );
    }

    #[test]
    fn imports_the_fixture_spec_into_a_compiled_workflow() {
        let outcome = import(&fixture_spec_text(), fixture_spec_path().parent().unwrap())
            .expect("fixture spec imports");
        assert!(outcome.todo_notes.is_empty());

        let actions = &outcome.compilation.workflow.actions;
        let kinds: Vec<ActionKind> = actions.iter().map(|a| a.kind).collect();
        // A `wait` follows each `fill`: typing into `#customer`/`#amount`
        // changes the element's value, so its snapshot digest changes and
        // pass 4 inserts a settle wait, exactly like a desktop `type` step
        // that changes what is on screen. `click` on `#save-btn` mutates
        // nothing in the fixture, so none follows it.
        assert_eq!(
            kinds,
            vec![
                ActionKind::Type,
                ActionKind::Wait,
                ActionKind::Type,
                ActionKind::Wait,
                ActionKind::Click,
                ActionKind::Assert,
            ]
        );

        let post = outcome
            .compilation
            .workflow
            .manifest
            .gates
            .iter()
            .find(|g| serde_json::to_value(g.kind).unwrap() == "post")
            .expect("the final expect becomes the postcondition gate");
        assert_eq!(post.expr["op"], "equals");
    }

    #[test]
    fn an_unmapped_statement_becomes_a_todo_step_not_a_crash() {
        let spec = r#"
import { test, expect } from '@playwright/test';

test('checks a download', async ({ page }) => {
  await page.goto('../webapp/index.html');
  await page.click('#save-btn');
  await page.waitForEvent('download');
  await expect(page.locator('#save-btn')).toBeVisible();
});
"#;
        let dir = fixture_spec_path();
        let dir = dir.parent().unwrap();
        let outcome = import(spec, dir).expect("import does not crash on an unmapped statement");
        assert_eq!(outcome.todo_notes.len(), 2);
        assert!(outcome.todo_notes[0].contains("waitForEvent"));
        assert!(outcome.todo_notes[1].contains("toBeVisible"));

        let intents: Vec<String> = outcome
            .compilation
            .workflow
            .actions
            .iter()
            .filter_map(|a| a.intent.clone())
            .collect();
        assert!(intents.iter().any(|i| i.starts_with("TODO:")));
    }

    #[test]
    fn replaying_the_compiled_web_steps_reaches_the_same_end_state_the_spec_asserts() {
        let outcome = import(&fixture_spec_text(), fixture_spec_path().parent().unwrap())
            .expect("fixture spec imports");

        // A fresh browser, a fresh attach: nothing carried over from import
        // time except the compiled actions themselves.
        let fresh = FixtureBrowser::new();
        let index_path = fixture_spec_path()
            .parent()
            .unwrap()
            .join("../webapp/index.html");
        fresh
            .attach(index_path.to_str().unwrap())
            .expect("fresh attach");

        for action in &outcome.compilation.workflow.actions {
            let Some(css) = action
                .target
                .as_ref()
                .and_then(|t| t.selectors.iter().find(|s| matches!(s, Selector::Css { .. })))
                .cloned()
            else {
                continue; // the wait/todo steps carry no selector to replay
            };
            let replayed = fresh.act(&BrowserAct {
                id: action.id.clone(),
                kind: action.kind,
                selector: css,
                params: action.params.clone(),
            });
            assert!(
                replayed.is_ok(),
                "replaying step `{}` ({:?}) failed: {:?}",
                action.id,
                action.kind,
                replayed.err()
            );
        }
    }
}
