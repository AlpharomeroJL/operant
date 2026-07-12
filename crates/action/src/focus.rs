//! Window matching for focus resolution (engine fix E1).
//!
//! The IR's [`WindowMatch::title_pattern`] is a REGEX (e.g. `.* - Notepad`).
//! The old real backend handed that string LITERALLY to `FindWindowW`, which
//! does exact-title matching, so a pattern never matched a real window and
//! live focus (and therefore live replay) always failed on its own canonical
//! Notepad fixture. The fix enumerates the live top-level windows and
//! regex-matches each title, also honoring [`WindowMatch::process`].
//!
//! This module is OS-free and ALWAYS compiled (no `real-input` gate) on
//! purpose: the regex/process matching contract is the part with real logic,
//! so it is unit tested by the default headless build. The Windows backend
//! (`real_win`, behind `real-input`) is a thin shell that reads each live
//! window's title/process via `EnumWindows`/`GetWindowTextW` and feeds them to
//! [`pick_window`]; it adds no matching logic of its own.

use operant_ir::WindowMatch;
use regex::Regex;

use crate::synth::SynthesizerError;

/// One enumerated top-level window, reduced to what focus matching needs.
///
/// `process` is the image basename (e.g. `notepad.exe`), or `None` when the
/// owning process could not be resolved (an elevated window the current
/// process cannot open, say). `hwnd` is the opaque OS handle the real backend
/// focuses once a candidate wins; it is unused by the pure matcher and by the
/// default build, hence the field-level allow below.
#[derive(Debug, Clone)]
pub(crate) struct WindowCandidate {
    #[allow(dead_code)] // read only by the `real-input` backend, not the matcher
    pub hwnd: isize,
    pub title: String,
    pub process: Option<String>,
    pub visible: bool,
}

/// Does `candidate` satisfy every constraint `want` specifies?
///
/// `title_pattern` is matched as an unanchored REGEX (this is the E1 fix; it
/// used to be compared for exact string equality by `FindWindowW`). `process`
/// is compared case-insensitively against the candidate's image basename. A
/// [`WindowMatch`] that specifies neither field cannot identify a window and
/// matches nothing. A malformed `title_pattern` regex is a hard, typed error
/// rather than a silent non-match, so a bad workflow fails loudly.
pub(crate) fn window_matches(
    candidate: &WindowCandidate,
    want: &WindowMatch,
) -> Result<bool, SynthesizerError> {
    if let Some(pattern) = want.title_pattern.as_deref() {
        let re = compile_title_regex(pattern)?;
        if !re.is_match(&candidate.title) {
            return Ok(false);
        }
    }
    if let Some(process) = want.process.as_deref() {
        match candidate.process.as_deref() {
            Some(found) if found.eq_ignore_ascii_case(process) => {}
            _ => return Ok(false),
        }
    }
    // Neither constraint given => nothing to match on.
    Ok(want.title_pattern.is_some() || want.process.is_some())
}

fn compile_title_regex(pattern: &str) -> Result<Regex, SynthesizerError> {
    Regex::new(pattern).map_err(|e| {
        SynthesizerError::Focus(format!("invalid title_pattern regex `{pattern}`: {e}"))
    })
}

/// Pick the best window matching `want` from the enumerated `candidates`.
///
/// A visible match always wins over a non-visible one; among visible matches
/// the first in enumeration order is chosen (`EnumWindows` returns windows in
/// top-down Z-order, so this prefers the foreground-most match). Returns
/// `Ok(None)` when nothing matches, and propagates a malformed-regex error.
pub(crate) fn pick_window<'a>(
    candidates: &'a [WindowCandidate],
    want: &WindowMatch,
) -> Result<Option<&'a WindowCandidate>, SynthesizerError> {
    let mut fallback: Option<&WindowCandidate> = None;
    for candidate in candidates {
        if window_matches(candidate, want)? {
            if candidate.visible {
                return Ok(Some(candidate));
            }
            if fallback.is_none() {
                fallback = Some(candidate);
            }
        }
    }
    Ok(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(title: &str, process: Option<&str>, visible: bool) -> WindowCandidate {
        WindowCandidate {
            hwnd: 1,
            title: title.to_string(),
            process: process.map(str::to_string),
            visible,
        }
    }

    #[test]
    fn regex_title_pattern_resolves_the_matching_window() {
        // The canonical IR pattern the OLD literal `FindWindowW` never matched:
        // there is no window literally titled ".* - Notepad", so exact matching
        // resolved nothing. Regex matching resolves the real Notepad window.
        let want = WindowMatch {
            process: None,
            title_pattern: Some(r".* - Notepad".to_string()),
        };
        let candidates = vec![
            cand("Program Manager", Some("explorer.exe"), true),
            cand("Untitled - Notepad", Some("notepad.exe"), true),
        ];
        let picked = pick_window(&candidates, &want)
            .unwrap()
            .expect("a regex title_pattern must now resolve a window");
        assert_eq!(picked.title, "Untitled - Notepad");
    }

    #[test]
    fn pattern_is_a_regex_not_a_literal() {
        // Proof the fix treats the pattern as a regex: a window whose title is
        // not literally the pattern string still resolves through the regex.
        let want = WindowMatch {
            process: None,
            title_pattern: Some(r".* - Notepad".to_string()),
        };
        let only = vec![cand("Invoices - Notepad", Some("notepad.exe"), true)];
        assert!(
            pick_window(&only, &want).unwrap().is_some(),
            "regex `.* - Notepad` must match `Invoices - Notepad`"
        );
    }

    #[test]
    fn process_constraint_is_honored_alongside_the_title() {
        let want = WindowMatch {
            process: Some("notepad.exe".to_string()),
            title_pattern: Some(r".* - Notepad".to_string()),
        };
        // Matching title but wrong process => not a match.
        let wrong_process = vec![cand("Draft - Notepad", Some("wordpad.exe"), true)];
        assert!(pick_window(&wrong_process, &want).unwrap().is_none());
        // Matching title AND process => a match.
        let right = vec![cand("Draft - Notepad", Some("notepad.exe"), true)];
        assert!(pick_window(&right, &want).unwrap().is_some());
    }

    #[test]
    fn process_only_match_is_case_insensitive() {
        let want = WindowMatch {
            process: Some("Notepad.EXE".to_string()),
            title_pattern: None,
        };
        let candidates = vec![cand("Untitled - Notepad", Some("notepad.exe"), true)];
        assert!(pick_window(&candidates, &want).unwrap().is_some());
    }

    #[test]
    fn a_visible_match_is_preferred_over_a_hidden_one() {
        let want = WindowMatch {
            process: None,
            title_pattern: Some(r".* - Notepad".to_string()),
        };
        let candidates = vec![
            cand("Hidden - Notepad", Some("notepad.exe"), false),
            cand("Shown - Notepad", Some("notepad.exe"), true),
        ];
        assert_eq!(
            pick_window(&candidates, &want).unwrap().unwrap().title,
            "Shown - Notepad"
        );
    }

    #[test]
    fn an_empty_window_match_resolves_nothing() {
        let want = WindowMatch {
            process: None,
            title_pattern: None,
        };
        let candidates = vec![cand("Untitled - Notepad", Some("notepad.exe"), true)];
        assert!(pick_window(&candidates, &want).unwrap().is_none());
    }

    #[test]
    fn a_malformed_regex_is_a_typed_focus_error() {
        let want = WindowMatch {
            process: None,
            title_pattern: Some("(unterminated".to_string()),
        };
        let candidates = vec![cand("anything", None, true)];
        assert!(matches!(
            pick_window(&candidates, &want),
            Err(SynthesizerError::Focus(_))
        ));
    }
}
