//! Perception Snapshot: normalized accessibility snapshot.
//! Mirrors `contracts/perception_snapshot.schema.json`.

use serde::{Deserialize, Serialize};

use crate::Selector;

fn default_v() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default = "default_v")]
    pub v: u32,
    pub source: SnapshotSource,
    pub window: WindowInfo,
    pub digest: String,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_ms: Option<u64>,
    pub elements: Vec<Element>,
}

impl Snapshot {
    /// Find the first element matching a role and name.
    pub fn find(&self, role: Role, name: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.role == role && e.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotSource {
    Uia,
    Browser,
    Fixture,
    AxStub,
    AtspiStub,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hwnd: Option<String>,
    pub process: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    #[serde(default = "one")]
    pub dpi_scale: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub idx: u32,
    pub parent: Option<u32>,
    pub role: Role,
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub automation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub offscreen: bool,
    #[serde(default)]
    pub is_password: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<Selector>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

/// Fixed role enum. ControlType maps into this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Window,
    Pane,
    Button,
    Edit,
    Text,
    Checkbox,
    Radio,
    Combobox,
    List,
    Listitem,
    Tree,
    Treeitem,
    Menu,
    Menuitem,
    Menubar,
    Tab,
    Tabitem,
    Toolbar,
    Table,
    Row,
    Cell,
    Header,
    Link,
    Image,
    Slider,
    Progressbar,
    Scrollbar,
    Statusbar,
    Titlebar,
    Document,
    Group,
    Separator,
    Tooltip,
    Hyperlink,
    Spinner,
    Custom,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrips_fixture() {
        let raw = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
        let snap: Snapshot = serde_json::from_str(raw).expect("fixture parses");
        assert_eq!(snap.window.process, "notepad.exe");
        let doc = snap.find(Role::Document, "Text editor").expect("editor present");
        assert_eq!(doc.automation_id.as_deref(), Some("RichEditD2DPT"));
        // Re-serialize and re-parse for round-trip stability.
        let back = serde_json::to_string(&snap).unwrap();
        let reparsed: Snapshot = serde_json::from_str(&back).unwrap();
        assert_eq!(snap, reparsed);
    }
}
