//! The five compiler passes.
//!
//! 1. normalize   drop retry-superseded steps and the steps a human correction
//!    superseded, keeping the corrected branch.
//! 2. parameterize turn shape-matching literals (date, currency, file path,
//!    email, url) into typed inputs and rewrite the step text as a template.
//! 3. selectorize  order every step's selectors by stability score, keeping all.
//! 4. waits/asserts insert a wait after each step that changed the snapshot
//!    digest, and surface the outcome-bearing assert as a postcondition.
//! 5. emit         (in [`crate::emit`]) render the TypeScript DSL and manifest.

use std::collections::HashSet;
use std::sync::OnceLock;

use operant_ir::{Action, ActionKind, Grounding, Pace, Retry, RiskClass, Target, WindowMatch};
use regex::Regex;
use serde_json::{json, Value};

use crate::trajectory::OUTCOME_RETRY_SUPERSEDED;

/// The plain-English text a synthesized wait step carries, in the manifest
/// step summary and as its Action IR intent.
pub const WAIT_INTENT: &str = "Wait for the screen to update";

/// One step as it flows through the passes: the Action IR plus the digest and
/// outcome metadata pass 4 needs.
#[derive(Debug, Clone)]
pub struct WorkStep {
    pub action: Action,
    pub digest_before: Option<String>,
    pub digest_after: Option<String>,
    pub outcome_bearing: bool,
}

impl WorkStep {
    pub fn new(
        action: Action,
        digest_before: Option<String>,
        digest_after: Option<String>,
        outcome_bearing: bool,
    ) -> Self {
        Self {
            action,
            digest_before,
            digest_after,
            outcome_bearing,
        }
    }
}

/// The kind of typed input a literal was recognized as (pass 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Date,
    Currency { cents: bool },
    FilePath,
    Email,
    Url,
}

impl InputKind {
    /// The default input name for this kind when no meaningful label precedes
    /// the literal in the text.
    fn base_name(self) -> &'static str {
        match self {
            InputKind::Date => "date",
            InputKind::Currency { .. } => "amount",
            InputKind::FilePath => "path",
            InputKind::Email => "email",
            InputKind::Url => "url",
        }
    }

    /// The `@operant/sdk` `input.<ctor>` builder that constructs this input.
    pub fn ts_ctor(self) -> &'static str {
        match self {
            InputKind::Date => "date",
            InputKind::Currency { .. } => "currency",
            InputKind::FilePath => "filePath",
            InputKind::Email => "email",
            InputKind::Url => "url",
        }
    }
}

/// A typed input inferred by pass 2, in text order.
#[derive(Debug, Clone)]
pub struct InputDef {
    pub name: String,
    pub kind: InputKind,
    pub default: String,
    pub title: String,
}

impl InputDef {
    /// The JSON Schema fragment for this input, as it appears under
    /// `inputs_schema.properties[name]` in the manifest.
    pub fn schema(&self) -> Value {
        match self.kind {
            InputKind::Date => json!({
                "type": "string",
                "format": "date",
                "default": self.default,
                "title": self.title,
            }),
            InputKind::Currency { cents } => {
                let pattern = if cents { r"^\d+\.\d{2}$" } else { r"^\d+$" };
                json!({
                    "type": "string",
                    "pattern": pattern,
                    "default": self.default,
                    "title": self.title,
                })
            }
            InputKind::FilePath => json!({
                "type": "string",
                "default": self.default,
                "title": self.title,
            }),
            InputKind::Email => json!({
                "type": "string",
                "format": "email",
                "default": self.default,
                "title": self.title,
            }),
            InputKind::Url => json!({
                "type": "string",
                "format": "uri",
                "default": self.default,
                "title": self.title,
            }),
        }
    }
}

// ---- Pass 1: normalize ------------------------------------------------------

/// One row fed into pass 1: a step plus the two fields that decide whether it
/// survives normalization (its own outcome, and any seq it supersedes).
pub struct NormRow {
    pub seq: u32,
    pub outcome: Option<String>,
    pub supersedes_seq: Option<u32>,
    pub step: WorkStep,
}

/// Drop retry-superseded steps and any step a human correction superseded,
/// keeping the corrected branch. A step that is both (the corrected-away step
/// is marked `retry_superseded` and named by a correction) drops once.
pub fn normalize(rows: Vec<NormRow>) -> Vec<WorkStep> {
    let superseded: HashSet<u32> = rows.iter().filter_map(|r| r.supersedes_seq).collect();
    rows.into_iter()
        .filter(|r| r.outcome.as_deref() != Some(OUTCOME_RETRY_SUPERSEDED))
        .filter(|r| !superseded.contains(&r.seq))
        .map(|r| r.step)
        .collect()
}

// ---- Pass 2: parameterize ---------------------------------------------------

/// Rewrite every `type` step's text, turning shape-matching literals into typed
/// inputs. Returns the inputs in first-seen order.
pub fn parameterize(steps: &mut [WorkStep]) -> Vec<InputDef> {
    let mut inputs: Vec<InputDef> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for step in steps.iter_mut() {
        if step.action.kind != ActionKind::Type {
            continue;
        }
        let Some(text) = step.action.params.get("text").and_then(Value::as_str) else {
            continue;
        };
        let (template, found) = parameterize_text(text, &mut used);
        if found.is_empty() {
            continue;
        }
        step.action
            .params
            .insert("text".to_string(), Value::String(template));
        inputs.extend(found);
    }
    inputs
}

struct RawMatch {
    kind: InputKind,
    default: String,
    match_start: usize,
    replace_start: usize,
    replace_end: usize,
}

/// Turn a literal string into a template plus the inputs it yielded.
fn parameterize_text(text: &str, used: &mut HashSet<String>) -> (String, Vec<InputDef>) {
    let mut raw = detect_literals(text);
    // Earliest first; on a tie prefer the longer span so an outer url wins over
    // an inner date.
    raw.sort_by(|a, b| {
        a.replace_start
            .cmp(&b.replace_start)
            .then((b.replace_end - b.replace_start).cmp(&(a.replace_end - a.replace_start)))
    });

    let mut accepted: Vec<RawMatch> = Vec::new();
    let mut last_end = 0usize;
    for m in raw {
        if m.replace_start < last_end {
            continue; // overlaps an already-accepted literal
        }
        last_end = m.replace_end;
        accepted.push(m);
    }

    let mut inputs: Vec<InputDef> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for m in &accepted {
        let name = pick_name(text, m, used);
        used.insert(name.clone());
        out.push_str(&text[cursor..m.replace_start]);
        out.push('{');
        out.push_str(&name);
        out.push('}');
        cursor = m.replace_end;
        inputs.push(InputDef {
            title: title_for(&name),
            name,
            kind: m.kind,
            default: m.default.clone(),
        });
    }
    out.push_str(&text[cursor..]);
    (out, inputs)
}

fn re(pat: &str) -> Regex {
    Regex::new(pat).expect("compiler regex is valid")
}

fn detect_literals(text: &str) -> Vec<RawMatch> {
    static URL: OnceLock<Regex> = OnceLock::new();
    static EMAIL: OnceLock<Regex> = OnceLock::new();
    static PATH: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    static CURRENCY: OnceLock<Regex> = OnceLock::new();

    let url = URL.get_or_init(|| re(r#"https?://[^\s"'<>]+"#));
    let email = EMAIL.get_or_init(|| re(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"));
    let path = PATH.get_or_init(|| re(r#"(?:[A-Za-z]:\\[^\s"'<>|]+|(?:/[A-Za-z0-9._-]+){2,})"#));
    let date = DATE.get_or_init(|| re(r"\d{4}-\d{2}-\d{2}"));
    let currency = CURRENCY.get_or_init(|| re(r"\$(\d+(?:\.\d{2})?)"));

    let mut out = Vec::new();
    for m in url.find_iter(text) {
        out.push(RawMatch {
            kind: InputKind::Url,
            default: m.as_str().to_string(),
            match_start: m.start(),
            replace_start: m.start(),
            replace_end: m.end(),
        });
    }
    for m in email.find_iter(text) {
        out.push(RawMatch {
            kind: InputKind::Email,
            default: m.as_str().to_string(),
            match_start: m.start(),
            replace_start: m.start(),
            replace_end: m.end(),
        });
    }
    for m in path.find_iter(text) {
        out.push(RawMatch {
            kind: InputKind::FilePath,
            default: m.as_str().to_string(),
            match_start: m.start(),
            replace_start: m.start(),
            replace_end: m.end(),
        });
    }
    for m in date.find_iter(text) {
        out.push(RawMatch {
            kind: InputKind::Date,
            default: m.as_str().to_string(),
            match_start: m.start(),
            replace_start: m.start(),
            replace_end: m.end(),
        });
    }
    for cap in currency.captures_iter(text) {
        let whole = cap.get(0).unwrap();
        let num = cap.get(1).unwrap();
        out.push(RawMatch {
            kind: InputKind::Currency {
                cents: num.as_str().contains('.'),
            },
            default: num.as_str().to_string(),
            match_start: whole.start(),
            replace_start: num.start(),
            replace_end: num.end(),
        });
    }
    out
}

/// Words that are too generic to prefix an input name with. A meaningful noun
/// like "invoice" becomes `invoice_date`; a bare quantifier like "total" falls
/// back to the type's base name (`amount`).
fn is_generic_word(w: &str) -> bool {
    const GENERIC: &[&str] = &[
        "total", "subtotal", "grand", "sum", "amount", "value", "number", "no", "the", "a", "an",
        "of", "is", "was", "were", "be", "for", "to", "at", "on", "in", "and", "or", "it", "its",
        "this", "that", "with", "by", "from", "as",
    ];
    GENERIC.contains(&w)
}

/// Name an inferred input from the word immediately preceding its literal.
fn pick_name(text: &str, m: &RawMatch, used: &HashSet<String>) -> String {
    static WORD: OnceLock<Regex> = OnceLock::new();
    let word = WORD.get_or_init(|| re(r"([A-Za-z][A-Za-z0-9]*)\s*$"));
    let base = m.kind.base_name();

    let prefix = word
        .captures(&text[..m.match_start])
        .and_then(|c| c.get(1))
        .map(|w| w.as_str().to_ascii_lowercase())
        .filter(|w| w.len() >= 3 && !is_generic_word(w) && w != base);

    let candidate = match prefix {
        Some(w) => format!("{w}_{base}"),
        None => base.to_string(),
    };
    dedupe(candidate, used)
}

fn dedupe(mut name: String, used: &HashSet<String>) -> String {
    if !used.contains(&name) {
        return name;
    }
    let stem = name.clone();
    let mut n = 2;
    loop {
        name = format!("{stem}_{n}");
        if !used.contains(&name) {
            return name;
        }
        n += 1;
    }
}

/// Title-case a snake-case input name: `invoice_date` becomes `Invoice date`.
fn title_for(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

// ---- Pass 3: selectorize ----------------------------------------------------

/// Order each step's selectors by stability score (descending), keeping all.
/// The order is stable so equally-scored selectors keep their recorded order.
pub fn selectorize(steps: &mut [WorkStep]) {
    for step in steps.iter_mut() {
        if let Some(target) = step.action.target.as_mut() {
            // Stable sort by descending score: equal-score selectors keep their
            // recorded order.
            target
                .selectors
                .sort_by_key(|s| std::cmp::Reverse(s.score()));
        }
    }
}

// ---- Pass 4: waits and asserts ---------------------------------------------

/// Insert a wait after every step whose action changed the snapshot digest.
/// The outcome-bearing assert is left in place and its predicate is returned so
/// the emitter can also bind it as the workflow postcondition.
pub fn waits_and_asserts(steps: Vec<WorkStep>) -> (Vec<Action>, Option<Value>) {
    let mut out: Vec<Action> = Vec::new();
    let mut post_expr: Option<Value> = None;

    for step in steps {
        let changed = digest_changed(&step);
        let is_assert = step.action.kind == ActionKind::Assert;

        if step.outcome_bearing && is_assert {
            if let Some(expr) = step.action.params.get("expr") {
                post_expr = Some(expr.clone());
            }
        }

        let window = step.action.target.as_ref().and_then(|t| t.window.clone());
        let wait_id = format!("{}-wait", step.action.id);

        out.push(step.action);

        // A wait guards the screen settling after a mutation; an assert reads
        // state and never needs one.
        if changed && !is_assert {
            out.push(make_wait(&wait_id, window));
        }
    }

    (out, post_expr)
}

fn digest_changed(step: &WorkStep) -> bool {
    match (&step.digest_before, &step.digest_after) {
        (Some(b), Some(a)) => b != a,
        _ => false,
    }
}

fn make_wait(id: &str, window: Option<WindowMatch>) -> Action {
    Action {
        v: 1,
        id: id.to_string(),
        kind: ActionKind::Wait,
        intent: Some(WAIT_INTENT.to_string()),
        target: Some(Target {
            window,
            ..Default::default()
        }),
        params: serde_json::Map::new(),
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

// ---- capability roll-up -----------------------------------------------------

/// The union of application ids the final steps drive, in first-seen order.
pub fn union_apps(actions: &[Action]) -> Vec<String> {
    let mut apps: Vec<String> = Vec::new();
    for a in actions {
        if let Some(proc) = a
            .target
            .as_ref()
            .and_then(|t| t.window.as_ref())
            .and_then(|w| w.process.as_ref())
        {
            if !apps.iter().any(|p| p == proc) {
                apps.push(proc.clone());
            }
        }
    }
    apps
}

/// The highest risk class across the final steps (the workflow risk ceiling).
pub fn risk_ceiling(actions: &[Action]) -> RiskClass {
    actions
        .iter()
        .map(|a| a.risk_class)
        .max()
        .unwrap_or(RiskClass::Read)
}
