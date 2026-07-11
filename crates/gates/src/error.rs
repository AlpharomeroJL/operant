//! Typed evaluation errors. The evaluator never panics on adapter-supplied data;
//! every malformed node surfaces as one of these.

use thiserror::Error;

/// An error raised while evaluating a gate predicate.
///
/// Evaluation is total over well-typed ASTs: a `Fail` result is not an error.
/// These variants only cover a structurally malformed predicate (a shape the
/// frozen contract does not permit).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GateError {
    /// A predicate node was not a JSON object with an `op` or `kind`.
    #[error("gate node is not an operator or query object: {0}")]
    NotANode(String),

    /// The `op` string is not one of the nine gate operators.
    #[error("unknown gate operator: {0}")]
    UnknownOp(String),

    /// The `kind` string is not a known query/value kind.
    #[error("unknown query kind: {0}")]
    UnknownKind(String),

    /// A required field was missing from an operator or query node.
    #[error("operator `{op}` is missing required field `{field}`")]
    MissingField {
        /// The operator or kind whose field is missing.
        op: String,
        /// The missing field name.
        field: &'static str,
    },

    /// A regex string failed to compile.
    #[error("invalid regex `{pattern}`: {reason}")]
    Regex {
        /// The offending pattern (already anchored).
        pattern: String,
        /// The underlying compile error text.
        reason: String,
    },

    /// A value was not coercible to the numeric type an operator requires.
    #[error("expected a number for operator `{op}`, found {found}")]
    NotNumeric {
        /// The operator that needed a number.
        op: String,
        /// A short description of what was found.
        found: String,
    },
}
