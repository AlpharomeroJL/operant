//! Element digest (C6): the ONLY perception representation the planner is
//! ever allowed to see. Built from a `Snapshot`'s normalized element tree;
//! never touches pixels, screenshots, or vision-sidecar output.
//! `docs/ARCHITECTURE.md` C6: "goal -> perceive -> element digest (never
//! raw pixels to the planner) -> plan".

use operant_ir::{Element, Snapshot};

/// A planner-safe textual summary of one perception snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementDigest {
    pub window_process: String,
    pub window_title: String,
    pub snapshot_digest: String,
    pub element_count: usize,
    pub truncated: bool,
    lines: Vec<String>,
}

impl ElementDigest {
    /// Build a digest from a snapshot. Pure data transform: role, name,
    /// automation id, value, and enabled/disabled state per element.
    /// Nothing image-shaped is derived or carried, which is what makes the
    /// "never raw pixels to the planner" rule enforceable in code rather
    /// than just in a comment: there is no field here that could hold one.
    pub fn build(snapshot: &Snapshot) -> Self {
        let lines = snapshot.elements.iter().map(describe_element).collect();
        ElementDigest {
            window_process: snapshot.window.process.clone(),
            window_title: snapshot.window.title.clone(),
            snapshot_digest: snapshot.digest.clone(),
            element_count: snapshot.elements.len(),
            truncated: snapshot.truncated,
            lines,
        }
    }

    /// Render as plain text for a `CompletionRequest`'s message content.
    pub fn to_prompt_text(&self) -> String {
        let mut out = format!(
            "window: \"{}\" ({}){}, digest {}\n",
            self.window_title,
            self.window_process,
            if self.truncated { ", truncated" } else { "" },
            self.snapshot_digest,
        );
        for line in &self.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

fn describe_element(e: &Element) -> String {
    let role = role_str(e);
    let mut parts = vec![format!("[{}] {} \"{}\"", e.idx, role, e.name)];
    if let Some(aid) = &e.automation_id {
        parts.push(format!("id={aid}"));
    }
    if let Some(val) = e.value.as_deref() {
        if !val.is_empty() {
            parts.push(format!("value={val:?}"));
        }
    }
    if !e.enabled {
        parts.push("disabled".to_string());
    }
    format!("  {}", parts.join(" "))
}

fn role_str(e: &Element) -> String {
    serde_json::to_value(e.role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notepad_snapshot() -> Snapshot {
        let raw = include_str!("../../../../contracts/fixtures/snapshot_notepad.json");
        serde_json::from_str(raw).expect("shared notepad fixture parses")
    }

    #[test]
    fn build_lists_every_element_and_never_touches_pixels() {
        let snap = notepad_snapshot();
        let digest = ElementDigest::build(&snap);
        assert_eq!(digest.window_process, "notepad.exe");
        assert_eq!(digest.element_count, snap.elements.len());

        let text = digest.to_prompt_text();
        assert!(text.contains("Text editor"));
        assert!(text.contains("RichEditD2DPT"));
        // The digest is plain text; nothing image-shaped ever appears in it.
        assert!(!text.to_lowercase().contains("png"));
        assert!(!text.to_lowercase().contains("base64"));
    }

    #[test]
    fn disabled_elements_are_flagged_in_the_text() {
        let mut snap = notepad_snapshot();
        snap.elements[0].enabled = false;
        let digest = ElementDigest::build(&snap);
        assert!(digest.to_prompt_text().contains("disabled"));
    }

    #[test]
    fn truncated_flag_surfaces_in_the_header_line() {
        let mut snap = notepad_snapshot();
        snap.truncated = true;
        let digest = ElementDigest::build(&snap);
        assert!(digest.truncated);
        assert!(digest.to_prompt_text().contains("truncated"));
    }
}
