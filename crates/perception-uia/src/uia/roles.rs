//! ControlType -> Role map (`docs/specs/perception.md`: "ControlType
//! mapped to a fixed enum"). One direction only: UIA's
//! `UIA_CONTROLTYPE_ID` into the schema's closed `Role` enum
//! (`contracts/perception_snapshot.schema.json`). Anything this list does
//! not name (including control types Microsoft adds after this was
//! written) falls back to `Role::Unknown` rather than failing the walk.

use operant_ir::snapshot::Role;
use windows::Win32::UI::Accessibility::*;

// windows-rs names these `UIA_XxxControlTypeId` (mixed case, mirroring the
// underlying Win32 identifiers) rather than Rust's SCREAMING_SNAKE_CASE
// convention for constants; matching on them as patterns is correct
// (they are `pub const UIA_CONTROLTYPE_ID` values, not fresh bindings) but
// trips the stylistic lint below.
#[allow(non_upper_case_globals)]
pub fn control_type_to_role(ct: UIA_CONTROLTYPE_ID) -> Role {
    match ct {
        UIA_ButtonControlTypeId => Role::Button,
        UIA_SplitButtonControlTypeId => Role::Button,
        UIA_CheckBoxControlTypeId => Role::Checkbox,
        UIA_RadioButtonControlTypeId => Role::Radio,
        UIA_ComboBoxControlTypeId => Role::Combobox,
        UIA_EditControlTypeId => Role::Edit,
        UIA_TextControlTypeId => Role::Text,
        UIA_ListControlTypeId => Role::List,
        UIA_ListItemControlTypeId => Role::Listitem,
        UIA_TreeControlTypeId => Role::Tree,
        UIA_TreeItemControlTypeId => Role::Treeitem,
        UIA_MenuControlTypeId => Role::Menu,
        UIA_MenuBarControlTypeId => Role::Menubar,
        UIA_MenuItemControlTypeId => Role::Menuitem,
        UIA_TabControlTypeId => Role::Tab,
        UIA_TabItemControlTypeId => Role::Tabitem,
        UIA_ToolBarControlTypeId => Role::Toolbar,
        UIA_AppBarControlTypeId => Role::Toolbar,
        UIA_TableControlTypeId => Role::Table,
        UIA_DataGridControlTypeId => Role::Table,
        UIA_DataItemControlTypeId => Role::Row,
        UIA_HeaderControlTypeId => Role::Header,
        UIA_HeaderItemControlTypeId => Role::Cell,
        UIA_HyperlinkControlTypeId => Role::Hyperlink,
        UIA_ImageControlTypeId => Role::Image,
        UIA_SliderControlTypeId => Role::Slider,
        UIA_ThumbControlTypeId => Role::Slider,
        UIA_SpinnerControlTypeId => Role::Spinner,
        UIA_ProgressBarControlTypeId => Role::Progressbar,
        UIA_ScrollBarControlTypeId => Role::Scrollbar,
        UIA_StatusBarControlTypeId => Role::Statusbar,
        UIA_TitleBarControlTypeId => Role::Titlebar,
        UIA_DocumentControlTypeId => Role::Document,
        UIA_GroupControlTypeId => Role::Group,
        UIA_SeparatorControlTypeId => Role::Separator,
        UIA_ToolTipControlTypeId => Role::Tooltip,
        UIA_WindowControlTypeId => Role::Window,
        UIA_PaneControlTypeId => Role::Pane,
        UIA_CustomControlTypeId => Role::Custom,
        UIA_SemanticZoomControlTypeId => Role::Custom,
        UIA_CalendarControlTypeId => Role::Custom,
        _ => Role::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_control_types() {
        assert_eq!(control_type_to_role(UIA_ButtonControlTypeId), Role::Button);
        assert_eq!(control_type_to_role(UIA_EditControlTypeId), Role::Edit);
        assert_eq!(control_type_to_role(UIA_WindowControlTypeId), Role::Window);
        assert_eq!(
            control_type_to_role(UIA_MenuBarControlTypeId),
            Role::Menubar
        );
    }

    #[test]
    fn unknown_control_type_falls_back() {
        assert_eq!(
            control_type_to_role(UIA_CONTROLTYPE_ID(999_999)),
            Role::Unknown
        );
    }
}
