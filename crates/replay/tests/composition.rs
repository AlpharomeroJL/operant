//! Composition (C22 / FR-O5): a `callWorkflow` step lets a compiled workflow
//! call another as a step.
//!
//! Every fixture here is a minimal, hand-authored `Manifest`/`Action` value
//! (no compiler in the loop), so each test exercises exactly the composition
//! executor's contract: load the callee, intersect grants, refuse before
//! running if the intersection cannot cover the callee's declared needs, and
//! never let the callee reach past the intersection once it does run.

use std::collections::BTreeMap;

use operant_action::SynthCall;
use operant_gates::EvalContext;
use operant_ir::{
    Action, ActionKind, Capabilities, Coords, DslRef, Grounding, Manifest, Pace, Retry, RiskClass,
    Target, WindowMatch,
};
use operant_replay::compose::{
    as_call_workflow, call_workflow_action, CompositionError, CompositionExecutor, MapLoader,
};
use operant_replay::{CompiledWorkflow, Replayer};

fn capabilities(apps: &[&str], network: bool, risk_ceiling: RiskClass) -> Capabilities {
    Capabilities {
        apps: apps.iter().map(|s| s.to_string()).collect(),
        paths: Vec::new(),
        network,
        risk_ceiling,
    }
}

fn manifest(name: &str, caps: Capabilities, steps: Vec<String>) -> Manifest {
    Manifest {
        v: 1,
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("{name} fixture"),
        step_summary: steps,
        inputs_schema: serde_json::json!({}),
        capabilities: caps,
        gates: Vec::new(),
        min_operant_version: None,
        source_run_id: None,
        dsl: DslRef {
            path: format!("inline://{name}"),
            hash: "0".repeat(64),
        },
        signature: None,
    }
}

fn window(process: &str) -> WindowMatch {
    WindowMatch {
        process: Some(process.to_string()),
        title_pattern: None,
    }
}

fn click(id: &str, process: &str, risk_class: RiskClass) -> Action {
    Action {
        v: 1,
        id: id.to_string(),
        kind: ActionKind::Click,
        intent: None,
        target: Some(Target {
            window: Some(window(process)),
            selectors: Vec::new(),
            anchor: None,
            coords_last_known: Some(Coords {
                x: 1.0,
                y: 1.0,
                monitor: None,
                dpi_scale: None,
            }),
        }),
        params: serde_json::Map::new(),
        pace: Pace::Instant,
        risk_class,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Retry::default(),
    }
}

fn compiled(name: &str, caps: Capabilities, actions: Vec<Action>, steps: Vec<String>) -> CompiledWorkflow {
    CompiledWorkflow {
        manifest: manifest(name, caps, steps),
        actions,
    }
}

#[test]
fn broad_parent_calling_narrow_child_runs_at_the_intersection() {
    let child = compiled(
        "child-notes",
        capabilities(&["notepad.exe"], false, RiskClass::Write),
        vec![click("c1", "notepad.exe", RiskClass::Write)],
        vec!["click notepad".to_string()],
    );

    let parent_action = call_workflow_action("call-child", "child-notes", BTreeMap::new(), RiskClass::Read);
    let parent = compiled(
        "parent-broad",
        capabilities(&["notepad.exe", "chrome.exe"], true, RiskClass::Destructive),
        vec![parent_action.clone()],
        vec!["call child-notes".to_string()],
    );

    let step = as_call_workflow(&parent_action).expect("recognized as callWorkflow");
    let loader = MapLoader::new().with("child-notes", child);
    let replayer = Replayer::with_mock();
    let executor = CompositionExecutor::new(&replayer);

    let (report, node) = executor
        .call(&parent, &step, &loader, &EvalContext::new(), &EvalContext::new())
        .expect("composition succeeds");

    assert_eq!(report.steps_executed, 1);
    // The effective set is the child's own (narrower) declaration, not the
    // parent's broader grants: chrome.exe, network, and Destructive never
    // reach the child.
    assert_eq!(
        node.effective_capabilities,
        capabilities(&["notepad.exe"], false, RiskClass::Write)
    );
    assert_eq!(node.child_name, "child-notes");
    assert_eq!(node.parent_name, "parent-broad");

    // focus + click: the child's one step actually dispatched.
    assert_eq!(replayer.synthesizer().calls().len(), 2);
    assert!(matches!(
        replayer.synthesizer().calls()[1],
        SynthCall::ClickPoint(_)
    ));
}

#[test]
fn parent_only_capability_is_refused_inside_the_child() {
    // The child's manifest only declares needing notepad.exe, but one of its
    // own steps (compiler drift, or a hand authored DSL) reaches for
    // chrome.exe: an app only the PARENT grants. Composition must not trust
    // the manifest summary alone. Even though the parent has chrome, the
    // child never declared it, so the effective set excludes it and the step
    // is refused rather than silently allowed through the parent's broader
    // grant.
    let child = compiled(
        "child-reaches-too-far",
        capabilities(&["notepad.exe"], false, RiskClass::Write),
        vec![
            click("c1", "notepad.exe", RiskClass::Write),
            click("c2", "chrome.exe", RiskClass::Write),
        ],
        vec!["click notepad".to_string(), "click chrome".to_string()],
    );

    let parent_action = call_workflow_action(
        "call-child",
        "child-reaches-too-far",
        BTreeMap::new(),
        RiskClass::Read,
    );
    let parent = compiled(
        "parent-broad",
        capabilities(&["notepad.exe", "chrome.exe"], true, RiskClass::Destructive),
        vec![parent_action.clone()],
        vec!["call child".to_string()],
    );

    let step = as_call_workflow(&parent_action).unwrap();
    let loader = MapLoader::new().with("child-reaches-too-far", child);
    let replayer = Replayer::with_mock();
    let executor = CompositionExecutor::new(&replayer);

    let err = executor
        .call(&parent, &step, &loader, &EvalContext::new(), &EvalContext::new())
        .expect_err("must refuse: chrome.exe is outside the effective set");
    assert!(matches!(err, CompositionError::ExceedsEffective { .. }));
    assert!(err.to_string().contains("chrome.exe"));

    // Refused before anything ran: not even the first, valid step dispatched.
    assert_eq!(replayer.synthesizer().calls().len(), 0);
}

#[test]
fn refuses_at_load_when_the_child_needs_more_than_the_parent_grants() {
    let child = compiled(
        "child-needs-destructive",
        capabilities(&["notepad.exe"], false, RiskClass::Destructive),
        vec![click("c1", "notepad.exe", RiskClass::Destructive)],
        vec!["delete file".to_string()],
    );

    let parent_action = call_workflow_action(
        "call-child",
        "child-needs-destructive",
        BTreeMap::new(),
        RiskClass::Read,
    );
    let parent = compiled(
        "parent-narrow",
        capabilities(&["notepad.exe"], false, RiskClass::Write), // no Destructive
        vec![parent_action.clone()],
        vec!["call child".to_string()],
    );

    let step = as_call_workflow(&parent_action).unwrap();
    let loader = MapLoader::new().with("child-needs-destructive", child);
    let replayer = Replayer::with_mock();
    let executor = CompositionExecutor::new(&replayer);

    let err = executor
        .call(&parent, &step, &loader, &EvalContext::new(), &EvalContext::new())
        .expect_err("must refuse before running: the parent's risk ceiling is below the child's need");
    assert!(matches!(err, CompositionError::InsufficientCapabilities { .. }));
    assert!(err.to_string().contains("risk ceiling"));

    // Refused at load: no step of the child ever dispatched.
    assert_eq!(replayer.synthesizer().calls().len(), 0);
}

#[test]
fn unknown_callee_is_refused_before_running() {
    let parent_action = call_workflow_action("call-child", "does-not-exist", BTreeMap::new(), RiskClass::Read);
    let parent = compiled(
        "parent",
        capabilities(&["notepad.exe"], false, RiskClass::Write),
        vec![parent_action.clone()],
        vec!["call missing child".to_string()],
    );

    let step = as_call_workflow(&parent_action).unwrap();
    let loader = MapLoader::new(); // nothing registered
    let replayer = Replayer::with_mock();
    let executor = CompositionExecutor::new(&replayer);

    let err = executor
        .call(&parent, &step, &loader, &EvalContext::new(), &EvalContext::new())
        .expect_err("unknown callee must be refused");
    assert!(matches!(err, CompositionError::UnknownCallee { .. }));
    assert_eq!(replayer.synthesizer().calls().len(), 0);
}

#[test]
fn fixture_composition_loads_and_replays_a_child_with_inputs() {
    let mut type_action = click("c2", "notepad.exe", RiskClass::Write);
    type_action.kind = ActionKind::Type;
    type_action.target = None;
    type_action
        .params
        .insert("text".to_string(), serde_json::Value::String("hello {name}".to_string()));

    let child = compiled(
        "child-greeting",
        capabilities(&["notepad.exe"], false, RiskClass::Write),
        vec![click("c1", "notepad.exe", RiskClass::Write), type_action],
        vec!["click".to_string(), "type greeting".to_string()],
    );

    let mut call_inputs = BTreeMap::new();
    call_inputs.insert("name".to_string(), "Operant".to_string());
    let parent_action = call_workflow_action("call-child", "child-greeting", call_inputs, RiskClass::Read);
    let parent = compiled(
        "parent",
        capabilities(&["notepad.exe", "chrome.exe"], true, RiskClass::Destructive),
        vec![parent_action.clone()],
        vec!["call child-greeting".to_string()],
    );

    let step = as_call_workflow(&parent_action).expect("recognized as callWorkflow");
    let loader = MapLoader::new().with("child-greeting", child);
    let replayer = Replayer::with_mock();
    let executor = CompositionExecutor::new(&replayer);

    let (report, node) = executor
        .call(&parent, &step, &loader, &EvalContext::new(), &EvalContext::new())
        .expect("composed replay succeeds");

    assert_eq!(report.steps_executed, 2);
    assert_eq!(
        node.child_step_summary,
        vec!["click".to_string(), "type greeting".to_string()]
    );
    assert!(replayer
        .synthesizer()
        .calls()
        .contains(&SynthCall::TypeText("hello Operant".to_string())));
}
