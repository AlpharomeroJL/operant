//! Runtime values in the gate predicate language.
//!
//! The language is small and dynamically typed: selectors resolve to strings,
//! numbers, booleans, or arrays; operators consume and produce these.

use serde_json::Value as Json;

/// A value produced by evaluating a gate sub-expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    /// Absent / not found.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number (all JSON numbers collapse to f64).
    Num(f64),
    /// A string.
    Str(String),
    /// An ordered list (adapter array projections, element sets).
    Array(Vec<Val>),
    /// An opaque object kept as JSON (adapter results, elements).
    Object(serde_json::Map<String, Json>),
}

impl Val {
    /// Lift a raw JSON value into a [`Val`].
    pub fn from_json(j: &Json) -> Val {
        match j {
            Json::Null => Val::Null,
            Json::Bool(b) => Val::Bool(*b),
            Json::Number(n) => Val::Num(n.as_f64().unwrap_or(0.0)),
            Json::String(s) => Val::Str(s.clone()),
            Json::Array(a) => Val::Array(a.iter().map(Val::from_json).collect()),
            Json::Object(o) => Val::Object(o.clone()),
        }
    }

    /// Boolean interpretation used when a value lands in a boolean position.
    ///
    /// Operators normally return [`Val::Bool`] directly; this fallback lets a
    /// bare numeric/string value still resolve a gate to pass/fail without a panic.
    pub fn truthy(&self) -> bool {
        match self {
            Val::Bool(b) => *b,
            Val::Num(n) => *n != 0.0,
            Val::Str(s) => !s.is_empty(),
            Val::Array(a) => !a.is_empty(),
            Val::Object(_) => true,
            Val::Null => false,
        }
    }

    /// True when the value is anything other than [`Val::Null`]. Used by `exists`
    /// over adapter results and element values.
    pub fn present(&self) -> bool {
        !matches!(self, Val::Null)
    }

    /// Best-effort numeric coercion (numbers, numeric strings, booleans as 0/1).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            Val::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Val::Str(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }
    }

    /// The string view, if this value is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// Structural equality across the value lattice, with lenient numeric/string
/// coercion so a literal `0` compares equal to a numeric-string `"0"`.
pub fn val_equals(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Num(x), Val::Num(y)) => (x - y).abs() < 1e-9,
        (Val::Str(x), Val::Str(y)) => x == y,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Null, Val::Null) => true,
        (Val::Array(x), Val::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| val_equals(p, q))
        }
        (Val::Num(x), Val::Str(y)) | (Val::Str(y), Val::Num(x)) => {
            y.trim().parse::<f64>().map(|yy| (x - yy).abs() < 1e-9).unwrap_or(false)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness_and_presence() {
        assert!(Val::Bool(true).truthy());
        assert!(!Val::Bool(false).truthy());
        assert!(Val::Num(3.0).truthy());
        assert!(!Val::Num(0.0).truthy());
        assert!(!Val::Null.present());
        assert!(Val::Str(String::new()).present());
    }

    #[test]
    fn equality_coerces_numeric_strings() {
        assert!(val_equals(&Val::Num(0.0), &Val::Str("0".into())));
        assert!(val_equals(&Val::Str("notepad.exe".into()), &Val::Str("notepad.exe".into())));
        assert!(!val_equals(&Val::Num(1.0), &Val::Str("two".into())));
        assert!(val_equals(&Val::Bool(true), &Val::Bool(true)));
    }
}
