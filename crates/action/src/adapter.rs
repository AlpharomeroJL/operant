//! Adapter registration framework: an [`Adapter`] registers a namespace, a
//! JSON schema per verb, a risk class per verb, and an idempotency hint.
//! The executor validates `adapter_call` params against that schema before
//! dispatch (`docs/specs/action.md`).
//!
//! Real adapters (shell/PowerShell, filesystem, Office COM, email,
//! OCR/PDF, browser, MCP client) land in L2B. This crate owns only the
//! framework: registration, schema validation, and the resolution-order
//! helper (`docs/ARCHITECTURE.md` C4: "Resolution order enforced in code:
//! adapter beats UIA beats vision").

use std::collections::HashMap;

use operant_ir::{Grounding, RiskClass};
use thiserror::Error;

/// How safe it is for the executor to retry a verb call after a failure or
/// timeout without asking anyone first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idempotency {
    /// Retrying is always safe, e.g. `fs.read`.
    Idempotent,
    /// Retrying may duplicate a side effect, e.g. `email.send`.
    NotIdempotent,
    /// Not established for this verb; callers should treat it like
    /// `NotIdempotent`.
    Unknown,
}

/// One verb an [`Adapter`] exposes under its namespace.
pub struct VerbSpec {
    pub name: String,
    /// JSON Schema (draft 2020-12, matching the dialect used under
    /// `contracts/`) that a call's `args` object must satisfy.
    pub args_schema: serde_json::Value,
    pub risk_class: RiskClass,
    pub idempotency: Idempotency,
}

impl VerbSpec {
    pub fn new(
        name: impl Into<String>,
        args_schema: serde_json::Value,
        risk_class: RiskClass,
        idempotency: Idempotency,
    ) -> Self {
        Self {
            name: name.into(),
            args_schema,
            risk_class,
            idempotency,
        }
    }
}

/// A namespace of `adapter_call` verbs, e.g. `fs`, `shell`, `excel`.
///
/// Implementors own their verb metadata (typically a `Vec<VerbSpec>` built
/// once in `new()`) so [`Adapter::verbs`] can hand back a borrowed slice.
pub trait Adapter: Send + Sync {
    /// The `namespace` an `adapter_call` action must set to reach this
    /// adapter, e.g. `"fs"` for `{ "namespace": "fs", "verb": "read", ... }`.
    fn namespace(&self) -> &str;

    /// Every verb this adapter supports.
    fn verbs(&self) -> &[VerbSpec];

    /// Perform the call. The executor guarantees `args` already passed
    /// schema validation for this verb before calling this method.
    fn call(&self, verb: &str, args: &serde_json::Value)
        -> Result<serde_json::Value, AdapterError>;

    /// Look up one verb's spec by name.
    fn verb(&self, verb: &str) -> Option<&VerbSpec> {
        self.verbs().iter().find(|v| v.name == verb)
    }
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter namespace `{0}` is not registered")]
    NamespaceNotRegistered(String),
    #[error("adapter `{namespace}` has no verb `{verb}`")]
    VerbNotRegistered { namespace: String, verb: String },
    #[error("adapter_call params for `{namespace}.{verb}` failed schema validation: {errors:?}")]
    SchemaValidation {
        namespace: String,
        verb: String,
        errors: Vec<String>,
    },
    #[error("adapter `{namespace}.{verb}` call failed: {message}")]
    CallFailed {
        namespace: String,
        verb: String,
        message: String,
    },
}

/// Registered adapters, keyed by namespace. The executor validates every
/// `adapter_call` action's params against the target verb's schema before
/// dispatching to it (see [`AdapterRegistry::call`]).
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: HashMap<String, Box<dyn Adapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an adapter under its own declared namespace. Registering a
    /// second adapter under the same namespace replaces the first.
    pub fn register(&mut self, adapter: Box<dyn Adapter>) {
        self.adapters
            .insert(adapter.namespace().to_string(), adapter);
    }

    pub fn get(&self, namespace: &str) -> Option<&dyn Adapter> {
        self.adapters.get(namespace).map(|a| a.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    fn lookup(
        &self,
        namespace: &str,
        verb: &str,
    ) -> Result<(&dyn Adapter, &VerbSpec), AdapterError> {
        let adapter = self
            .get(namespace)
            .ok_or_else(|| AdapterError::NamespaceNotRegistered(namespace.to_string()))?;
        let spec = adapter
            .verb(verb)
            .ok_or_else(|| AdapterError::VerbNotRegistered {
                namespace: namespace.to_string(),
                verb: verb.to_string(),
            })?;
        Ok((adapter, spec))
    }

    /// Validate `args` against the verb's JSON schema without calling it.
    /// This is what the executor runs before every `adapter_call` dispatch.
    pub fn validate(
        &self,
        namespace: &str,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<(), AdapterError> {
        let (_, spec) = self.lookup(namespace, verb)?;
        validate_args(namespace, verb, &spec.args_schema, args)
    }

    /// Validate then call. Returns the adapter's raw JSON result.
    pub fn call(
        &self,
        namespace: &str,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, AdapterError> {
        let (adapter, spec) = self.lookup(namespace, verb)?;
        validate_args(namespace, verb, &spec.args_schema, args)?;
        adapter.call(verb, args)
    }
}

fn validate_args(
    namespace: &str,
    verb: &str,
    schema: &serde_json::Value,
    args: &serde_json::Value,
) -> Result<(), AdapterError> {
    let compiled =
        jsonschema::JSONSchema::compile(schema).map_err(|e| AdapterError::SchemaValidation {
            namespace: namespace.to_string(),
            verb: verb.to_string(),
            errors: vec![format!("adapter registered an invalid schema: {e}")],
        })?;
    if let Err(errors) = compiled.validate(args) {
        let errors = errors.map(|e| e.to_string()).collect();
        return Err(AdapterError::SchemaValidation {
            namespace: namespace.to_string(),
            verb: verb.to_string(),
            errors,
        });
    }
    Ok(())
}

/// Enforce adapter > UIA > vision: given the groundings actually available
/// for a step, pick the highest-priority one. Returns `None` when
/// `available` is empty.
///
/// This is a pure priority pick; supplying the candidate set for a given
/// step (what an adapter can actually reach, whether UIA resolved a
/// selector, whether a vision anchor matched) is a perception/orchestrator
/// concern (C2/C3/C6) layered on top.
pub fn resolve_strategy(available: &[Grounding]) -> Option<Grounding> {
    [Grounding::Adapter, Grounding::Uia, Grounding::Vision]
        .into_iter()
        .find(|g| available.contains(g))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct EchoAdapter {
        verbs: Vec<VerbSpec>,
    }

    impl EchoAdapter {
        fn new() -> Self {
            Self {
                verbs: vec![VerbSpec::new(
                    "read",
                    json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": { "type": "string", "minLength": 1 } },
                        "additionalProperties": false
                    }),
                    RiskClass::Read,
                    Idempotency::Idempotent,
                )],
            }
        }
    }

    impl Adapter for EchoAdapter {
        fn namespace(&self) -> &str {
            "test"
        }

        fn verbs(&self) -> &[VerbSpec] {
            &self.verbs
        }

        fn call(
            &self,
            verb: &str,
            args: &serde_json::Value,
        ) -> Result<serde_json::Value, AdapterError> {
            Ok(json!({ "verb": verb, "echo": args }))
        }
    }

    fn registry() -> AdapterRegistry {
        let mut reg = AdapterRegistry::new();
        reg.register(Box::new(EchoAdapter::new()));
        reg
    }

    #[test]
    fn validate_accepts_a_good_payload() {
        let reg = registry();
        assert!(reg
            .validate("test", "read", &json!({ "path": "C:/tmp/x.txt" }))
            .is_ok());
    }

    #[test]
    fn validate_rejects_a_bad_payload() {
        let reg = registry();
        // Missing the required `path` field.
        let err = reg.validate("test", "read", &json!({})).unwrap_err();
        assert!(
            matches!(err, AdapterError::SchemaValidation { .. }),
            "got {err:?}"
        );

        // Wrong type for `path`.
        let err = reg
            .validate("test", "read", &json!({ "path": 42 }))
            .unwrap_err();
        assert!(
            matches!(err, AdapterError::SchemaValidation { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn call_runs_the_adapter_after_validating() {
        let reg = registry();
        let result = reg
            .call("test", "read", &json!({ "path": "a.txt" }))
            .unwrap();
        assert_eq!(
            result,
            json!({ "verb": "read", "echo": { "path": "a.txt" } })
        );
    }

    #[test]
    fn call_rejects_bad_payload_before_dispatch() {
        let reg = registry();
        let err = reg.call("test", "read", &json!({})).unwrap_err();
        assert!(matches!(err, AdapterError::SchemaValidation { .. }));
    }

    #[test]
    fn unknown_namespace_and_verb_are_typed_errors() {
        let reg = registry();
        assert!(matches!(
            reg.validate("nope", "read", &json!({})),
            Err(AdapterError::NamespaceNotRegistered(ns)) if ns == "nope"
        ));
        assert!(matches!(
            reg.validate("test", "nope", &json!({})),
            Err(AdapterError::VerbNotRegistered { .. })
        ));
    }

    #[test]
    fn resolve_strategy_prefers_adapter_then_uia_then_vision() {
        assert_eq!(
            resolve_strategy(&[Grounding::Vision, Grounding::Uia, Grounding::Adapter]),
            Some(Grounding::Adapter)
        );
        assert_eq!(
            resolve_strategy(&[Grounding::Vision, Grounding::Uia]),
            Some(Grounding::Uia)
        );
        assert_eq!(
            resolve_strategy(&[Grounding::Vision]),
            Some(Grounding::Vision)
        );
        assert_eq!(resolve_strategy(&[]), None);
    }
}
