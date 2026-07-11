//! `Role` <-> schema string helper shared by selector-chain matching
//! (`topology.rs`) and, behind `real-uia`, selector construction
//! (`selectors.rs`). Reads the string back out through the same serde
//! derive `operant_ir::Role` uses (`#[serde(rename_all = "lowercase")]`)
//! so this can never drift from the wire representation in
//! `contracts/perception_snapshot.schema.json`.

use operant_ir::snapshot::Role;

pub fn role_str(role: Role) -> String {
    match serde_json::to_value(role) {
        Ok(serde_json::Value::String(s)) => s,
        _ => unreachable!("Role serializes to a plain lowercase string"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_schema_role_strings() {
        assert_eq!(role_str(Role::Window), "window");
        assert_eq!(role_str(Role::Listitem), "listitem");
        assert_eq!(role_str(Role::Menubar), "menubar");
        assert_eq!(role_str(Role::Unknown), "unknown");
    }
}
