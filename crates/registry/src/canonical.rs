//! Canonical JSON: recursively sorted keys, no whitespace, UTF-8. Must match
//! `contracts/fixtures/generate.mjs`'s `canonicalJson` byte for byte, since
//! that script is the source of truth for every signature in the fixtures:
//!
//! ```js
//! export function canonicalJson(value) {
//!   if (value === null || typeof value !== "object") return JSON.stringify(value);
//!   if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
//!   const keys = Object.keys(value).sort();
//!   return "{" + keys.map((k) => JSON.stringify(k) + ":" + canonicalJson(value[k])).join(",") + "}";
//! }
//! ```

pub fn to_canonical_json(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        serde_json::Value::Number(n) => out.push_str(&n.to_string()),
        serde_json::Value::String(s) => {
            out.push_str(&serde_json::to_string(s).expect("string encoding never fails"));
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("string encoding never fails"));
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
    }
}

/// Returns a clone of `value` with the top-level `signature` key removed, the
/// exact input `generate.mjs` signs: canonical JSON of the manifest minus its
/// own signature block.
pub fn without_signature(value: &serde_json::Value) -> serde_json::Value {
    let mut v = value.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.remove("signature");
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_keys_and_strips_whitespace() {
        let v = json!({"b": 1, "a": [1, 2, {"z": true, "y": null}]});
        assert_eq!(
            to_canonical_json(&v),
            r#"{"a":[1,2,{"y":null,"z":true}],"b":1}"#
        );
    }

    #[test]
    fn escapes_strings_like_json_stringify() {
        let v = json!({"k": "line\nbreak \"quote\""});
        assert_eq!(to_canonical_json(&v), r#"{"k":"line\nbreak \"quote\""}"#);
    }

    #[test]
    fn drops_only_the_signature_key() {
        let v = json!({"a": 1, "signature": {"sig": "x"}});
        assert_eq!(without_signature(&v), json!({"a": 1}));
    }
}
