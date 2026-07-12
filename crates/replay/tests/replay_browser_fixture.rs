//! End-to-end for the wasm playground's own fixture (`site/playground/`):
//! replay the compiled workflow checked in at
//! `site/playground/fixtures/compiled_workflow.json` against
//! `site/playground/fixtures/webapp.html` (a checked-in copy of
//! `contracts/fixtures/webapp/index.html`) through
//! [`Replayer::with_adapters`] and a [`FixtureBrowser`], the exact same
//! call graph `crates/replay/src/wasm.rs`'s `replay_fixture` drives from
//! JS. Runs natively so a failure here is a fast `cargo test`, not a
//! headless-browser round trip; `e2e/harness`'s Playwright spec covers the
//! browser side of the same fixture pair separately.

use std::collections::BTreeMap;
use std::sync::Arc;

use operant_action::adapters::browser::{BrowserAdapter, FixtureBrowser};
use operant_action::{AdapterRegistry, MockSynthesizer};
use operant_gates::EvalContext;
use operant_ir::GateResult;
use operant_replay::{CompiledWorkflow, Replayer};

const WORKFLOW_JSON: &str = include_str!("../../../site/playground/fixtures/compiled_workflow.json");
const PAGE_HTML: &str = include_str!("../../../site/playground/fixtures/webapp.html");

#[test]
fn replays_the_playground_fixture_through_the_browser_adapter() {
    let workflow: CompiledWorkflow =
        serde_json::from_str(WORKFLOW_JSON).expect("playground fixture workflow parses");

    let browser = Arc::new(FixtureBrowser::new());
    browser
        .attach_html(PAGE_HTML)
        .expect("attach the playground's checked-in webapp fixture");

    let mut adapters = AdapterRegistry::new();
    adapters.register(Box::new(BrowserAdapter::new(browser)));

    let replayer = Replayer::with_adapters(MockSynthesizer::new(), adapters);
    let ctx = EvalContext::new();
    let report = replayer
        .replay_compiled(&workflow, &BTreeMap::new(), &ctx, &ctx)
        .expect("the playground fixture replays cleanly");

    // s1 (type customer), s1-wait, s2 (type amount), s2-wait, s3 (click
    // save), s4 (adapter_call assert): every action but a literal
    // `ActionKind::Assert` gets dispatched, and this fixture has none (its
    // postcondition is an `adapter_call` to the `browser` namespace's own
    // `assert` verb, so it runs for real rather than being skipped as a
    // gate).
    assert_eq!(report.steps_executed, 6);
    assert!(report.pre.iter().all(|r| *r == GateResult::Pass));
    assert!(report.post.iter().all(|r| *r == GateResult::Pass));
}

#[test]
fn a_renamed_save_button_breaks_replay_instead_of_silently_passing() {
    // The drift fixture (`contracts/fixtures/webapp/drift.html`) renames
    // `#save-btn`'s id to `#store-btn` and its label to "Store invoice"
    // (`crates/action/src/adapters/browser/fixture.rs`'s own
    // `drift_variant_renames_the_button_and_the_digest_changes` test pins
    // this down). s3's `#save-btn` selector no longer resolves against the
    // drifted page, so replay must fail at the click step rather than
    // silently reporting success: a demo that "replays clean" against a
    // page that changed underneath it would be worse than no demo at all.
    let workflow: CompiledWorkflow =
        serde_json::from_str(WORKFLOW_JSON).expect("playground fixture workflow parses");

    let drift_html = include_str!("../../../contracts/fixtures/webapp/drift.html");
    let browser = Arc::new(FixtureBrowser::new());
    browser
        .attach_html(drift_html)
        .expect("attach the drifted webapp fixture");

    let mut adapters = AdapterRegistry::new();
    adapters.register(Box::new(BrowserAdapter::new(browser)));

    let replayer = Replayer::with_adapters(MockSynthesizer::new(), adapters);
    let ctx = EvalContext::new();
    let err = replayer
        .replay_compiled(&workflow, &BTreeMap::new(), &ctx, &ctx)
        .expect_err("the renamed button id must break the click step's selector");
    let message = err.to_string();
    assert!(
        message.contains("browser.click"),
        "expected a `browser.click` adapter failure, got: {message}"
    );
}
