//! Cached one-round-trip subtree walk (`docs/specs/perception.md`): build
//! a `CacheRequest` naming every property and pattern needed, call
//! `BuildUpdatedCache` ONCE on the root, then walk `GetCachedChildren`
//! entirely off already-marshaled data. The recursive walk never touches a
//! `Current*` property: that would mean a cross-process call per property
//! per element, exactly what the spec forbids ("never per-property calls
//! in a loop").
//!
//! Individual node failures (a COM call erroring on one element) are
//! skipped rather than aborting the whole walk: a single flaky element
//! should not blank out an otherwise-good snapshot.

use operant_ir::snapshot::{Bounds, Element};
use windows::Win32::UI::Accessibility::*;

use super::roles::control_type_to_role;

/// Cap from `docs/specs/perception.md`: "cap realization at 500 items per
/// snapshot with a truncation flag."
const REALIZATION_CAP: usize = 500;

pub struct WalkOutcome {
    pub elements: Vec<Element>,
    pub truncated: bool,
}

/// Elements/patterns cached in one round trip per subtree. Value, Toggle,
/// Invoke, Scroll, and VirtualizedItem availability are read back through
/// `GetCachedPatternAs`, so their pattern ids are cached too, not just
/// their properties.
pub fn build_cache_request(
    automation: &IUIAutomation,
) -> windows::core::Result<IUIAutomationCacheRequest> {
    let req = unsafe { automation.CreateCacheRequest() }?;
    unsafe {
        req.AddProperty(UIA_NamePropertyId)?;
        req.AddProperty(UIA_ControlTypePropertyId)?;
        req.AddProperty(UIA_AutomationIdPropertyId)?;
        req.AddProperty(UIA_BoundingRectanglePropertyId)?;
        req.AddProperty(UIA_IsEnabledPropertyId)?;
        req.AddProperty(UIA_IsOffscreenPropertyId)?;
        req.AddProperty(UIA_IsPasswordPropertyId)?;
        req.AddPattern(UIA_ValuePatternId)?;
        req.AddPattern(UIA_TogglePatternId)?;
        req.AddPattern(UIA_InvokePatternId)?;
        req.AddPattern(UIA_ScrollPatternId)?;
        req.AddPattern(UIA_VirtualizedItemPatternId)?;
        req.SetTreeScope(TreeScope_Subtree)?;
        let true_condition = automation.CreateTrueCondition()?;
        req.SetTreeFilter(&true_condition)?;
    }
    Ok(req)
}

/// Walk `cached_root` (already the result of one `BuildUpdatedCache`) into
/// the flat, parent-indexed element list the schema wants. `monitor` is
/// the opaque monitor handle string stamped onto every element's bounds
/// (`docs/specs/perception.md`: "Multi-monitor: all coordinates carry the
/// monitor handle").
pub fn walk_subtree(
    cached_root: &IUIAutomationElement,
    cache_request: &IUIAutomationCacheRequest,
    monitor: &str,
) -> WalkOutcome {
    let mut elements = Vec::new();
    let mut realized = 0usize;
    let mut truncated = false;
    push_node(
        cached_root,
        cache_request,
        None,
        monitor,
        &mut elements,
        &mut realized,
        &mut truncated,
    );
    WalkOutcome {
        elements,
        truncated,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_node(
    node: &IUIAutomationElement,
    cache_request: &IUIAutomationCacheRequest,
    parent: Option<u32>,
    monitor: &str,
    out: &mut Vec<Element>,
    realized: &mut usize,
    truncated: &mut bool,
) {
    let idx = out.len() as u32;
    let Some((element, is_virtualized)) = element_from_cached(node, idx, parent, monitor) else {
        return; // could not even read the basics off this node; skip it
    };
    out.push(element);

    if is_virtualized {
        // Realize-on-demand: a virtualized item's children are not in the
        // subtree cache until UIA is asked to materialize them, which
        // needs a fresh per-node BuildUpdatedCache (the one-round-trip
        // guarantee is necessarily per already-realized subtree, not
        // across a realization).
        if *realized >= REALIZATION_CAP {
            *truncated = true;
            return;
        }
        *realized += 1;
        if let Some(fresh) = realize_and_recache(node, cache_request) {
            push_children(
                &fresh,
                cache_request,
                idx,
                monitor,
                out,
                realized,
                truncated,
            );
        }
        return;
    }

    push_children(node, cache_request, idx, monitor, out, realized, truncated);
}

#[allow(clippy::too_many_arguments)]
fn push_children(
    node: &IUIAutomationElement,
    cache_request: &IUIAutomationCacheRequest,
    parent: u32,
    monitor: &str,
    out: &mut Vec<Element>,
    realized: &mut usize,
    truncated: &mut bool,
) {
    let Ok(children) = (unsafe { node.GetCachedChildren() }) else {
        return; // leaf, or descendants were excluded from this node's cache
    };
    let count = unsafe { children.Length() }.unwrap_or(0);
    for i in 0..count {
        let Ok(child) = (unsafe { children.GetElement(i) }) else {
            continue;
        };
        push_node(
            &child,
            cache_request,
            Some(parent),
            monitor,
            out,
            realized,
            truncated,
        );
    }
}

fn realize_and_recache(
    node: &IUIAutomationElement,
    cache_request: &IUIAutomationCacheRequest,
) -> Option<IUIAutomationElement> {
    let pattern = unsafe {
        node.GetCachedPatternAs::<IUIAutomationVirtualizedItemPattern>(UIA_VirtualizedItemPatternId)
    }
    .ok()?;
    unsafe { pattern.Realize() }.ok()?;
    unsafe { node.BuildUpdatedCache(cache_request) }.ok()
}

fn element_from_cached(
    node: &IUIAutomationElement,
    idx: u32,
    parent: Option<u32>,
    monitor: &str,
) -> Option<(Element, bool)> {
    // ControlType is the one field with no sane fallback: if this cache
    // read fails, the node was not usefully cached at all.
    let control_type = unsafe { node.CachedControlType() }.ok()?;
    let role = control_type_to_role(control_type);

    let name = unsafe { node.CachedName() }
        .map(|b| b.to_string())
        .unwrap_or_default();
    let automation_id = unsafe { node.CachedAutomationId() }
        .ok()
        .map(|b| b.to_string())
        .filter(|s| !s.is_empty());
    let rect = unsafe { node.CachedBoundingRectangle() }.unwrap_or_default();
    let bounds = Some(Bounds {
        x: rect.left as f64,
        y: rect.top as f64,
        w: (rect.right - rect.left) as f64,
        h: (rect.bottom - rect.top) as f64,
        monitor: Some(monitor.to_string()),
    });
    let enabled = unsafe { node.CachedIsEnabled() }
        .map(|b| b.as_bool())
        .unwrap_or(true);
    let offscreen = unsafe { node.CachedIsOffscreen() }
        .map(|b| b.as_bool())
        .unwrap_or(false);
    let is_password = unsafe { node.CachedIsPassword() }
        .map(|b| b.as_bool())
        .unwrap_or(false);

    let mut patterns = Vec::new();
    let mut value = None;
    if let Ok(p) =
        unsafe { node.GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
    {
        patterns.push("value".to_string());
        value = unsafe { p.CachedValue() }.ok().map(|b| b.to_string());
    }
    if unsafe { node.GetCachedPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) }.is_ok()
    {
        patterns.push("toggle".to_string());
    }
    if unsafe { node.GetCachedPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }.is_ok()
    {
        patterns.push("invoke".to_string());
    }
    if unsafe { node.GetCachedPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId) }.is_ok()
    {
        patterns.push("scroll".to_string());
    }
    let is_virtualized = unsafe {
        node.GetCachedPatternAs::<IUIAutomationVirtualizedItemPattern>(UIA_VirtualizedItemPatternId)
    }
    .is_ok();
    if is_virtualized {
        patterns.push("virtualized".to_string());
    }

    Some((
        Element {
            idx,
            parent,
            role,
            name,
            value,
            automation_id,
            bounds,
            enabled,
            offscreen,
            is_password,
            patterns,
            selectors: Vec::new(), // filled in by `crate::selectors` once the full tree is known
        },
        is_virtualized,
    ))
}
