//! The text field's container payload — what the `Modifier` chain carries and the renderer reads.
//!
//! `ModifierElement::TextFieldVisual` holds a `TextFieldVisual` payload, so the variant, the slot
//! roles, the colour set and the three render-time queries all have to be visible below the
//! component. The component keeps `TextField`, `TextFieldDefaults` and the constructors that turn a
//! `WiniaTheme` into a `TextFieldColors` — the design-system half.


/// Container shape — material3's `TextField` (filled) or `OutlinedTextField`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFieldVariant {
    /// A filled container with a bottom indicator (M3 `FilledTextField`, top corners 4 dp).
    Filled,
    /// No fill, an outline instead (M3 `OutlinedTextField`, all corners 4 dp).
    Outlined,
}

/// The M3 text-field state colours, matching `TextFieldDefaults.colors`.
///
/// Precedence: disabled > error > focused > unfocused.
#[derive(Debug, Clone)]
pub struct TextFieldColors {
    pub text: crate::graphics::Color,
    pub disabled_text: crate::graphics::Color,
    /// Container fill (used by `Filled`; `Outlined` stays transparent).
    pub container: crate::graphics::Color,
    /// The caret, when the field is not in error (see `error_cursor`).
    pub cursor: crate::graphics::Color,
    pub error_cursor: crate::graphics::Color,
    /// The bottom indicator or the outline.
    pub indicator_focused: crate::graphics::Color,
    pub indicator_unfocused: crate::graphics::Color,
    pub indicator_disabled: crate::graphics::Color,
    pub indicator_error: crate::graphics::Color,
    /// label
    pub label_focused: crate::graphics::Color,
    pub label_unfocused: crate::graphics::Color,
    pub label_disabled: crate::graphics::Color,
    pub label_error: crate::graphics::Color,
    /// placeholder
    pub placeholder: crate::graphics::Color,
    pub disabled_placeholder: crate::graphics::Color,
    /// Supporting text.
    pub supporting: crate::graphics::Color,
    pub disabled_supporting: crate::graphics::Color,
    pub error_supporting: crate::graphics::Color,
    /// The leading icon (onSurfaceVariant when idle, 38% when disabled, unchanged in error).
    pub leading_icon_focused: crate::graphics::Color,
    pub leading_icon_disabled: crate::graphics::Color,
    /// The trailing icon, which turns error-coloured in the error state.
    pub trailing_icon_focused: crate::graphics::Color,
    pub trailing_icon_disabled: crate::graphics::Color,
    pub trailing_icon_error: crate::graphics::Color,
    /// Prefix and suffix text (onSurfaceVariant, 38% when disabled).
    pub affix: crate::graphics::Color,
    pub disabled_affix: crate::graphics::Color,
}

/// What a child of the text-field container is for. `TextFieldLayout`'s policy places each role,
/// and the build order is fixed: leading -> label -> placeholder -> prefix -> input -> suffix ->
/// trailing, skipping whatever a given field does not carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextFieldSlotRole {
    /// The leading icon (M3's `leadingIcon`: 12 dp in, vertically centred, 16 dp from the text).
    Leading,
    /// The floating/expanded label; the policy interpolates its position.
    Label,
    /// Placeholder text, built only while the value is empty.
    Placeholder,
    /// The prefix, drawn before the text starts.
    Prefix,
    /// The input area — text, caret, selection and IME. The only interactive child.
    Input,
    /// The suffix, right aligned.
    Suffix,
    /// The trailing icon (12 dp from the right, vertically centred).
    Trailing,
}

/// Walk up from a text-field input leaf to its `OffsetMapping`.
///
/// The `TextFieldVisual` element (which carries `offset_mapping`) sits on the CONTAINER's
/// modifier, not on the input leaf. Asking the leaf alone answers `None`, and the caller then
/// treats a DISPLAY offset as an EDITING offset: it writes it into the selection (where a 3-byte
/// mask character makes `replace_range` panic past the end) or draws the caret in the wrong
/// place. So this follows the `parent_id` chain up to the first `TextFieldVisual`; a bare
/// `TextField` with no variant carries a `TextFieldOffsetMapping` element instead.
pub(crate) fn offset_mapping_for_node(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    idx: usize,
) -> Option<std::sync::Arc<dyn crate::text::transformation::OffsetMapping>> {
    use crate::modifier::ModifierElement;
    let mut cur = Some(idx);
    while let Some(i) = cur {
        for el in nodes[i].modifier.elements() {
            match el {
                ModifierElement::TextFieldVisual { offset_mapping, .. } if offset_mapping.is_some() => {
                    return offset_mapping.clone();
                }
                ModifierElement::TextFieldOffsetMapping { offset_mapping } => {
                    return Some(offset_mapping.clone());
                }
                _ => {}
            }
        }
        cur = nodes[i].parent_id
            .and_then(|pid| crate::layout::node::find_node_by_id(nodes, root, pid));
    }
    None
}

/// Walk up from a text-field input leaf to the container's caret colour.
///
/// `cursor_color` lives on the container's `TextFieldVisual`, where the theme resolves primary vs
/// error at compose time. The input leaf carries nothing, so looking there alone falls back to the
/// text colour — not the M3 caret. Same walk as [`offset_mapping_for_node`].
pub(crate) fn text_field_visual_color(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    idx: usize,
) -> Option<crate::graphics::Color> {
    use crate::modifier::ModifierElement;
    let mut cur = Some(idx);
    while let Some(i) = cur {
        for el in nodes[i].modifier.elements() {
            if let ModifierElement::TextFieldVisual { cursor_color, .. } = el {
                return Some(*cursor_color);
            }
        }
        cur = nodes[i].parent_id
            .and_then(|pid| crate::layout::node::find_node_by_id(nodes, root, pid));
    }
    None
}

/// Whether a text field's node should draw a caret, along the same parent chain [`text_field_visual_color`]
/// walks: material3's `showCursor = enabled && !readOnly && …`
/// (`foundation/text/CoreTextField.kt`). A read-only field keeps its focus — it stays selectable and
/// copyable — but shows no caret, where winia drew one whenever the field had focus.
///
/// Defaults to `true` when no visual element is found, which is the bare (variant-less) field: it has no
/// `readOnly` to consult, and has always drawn its caret.
pub(crate) fn text_field_show_cursor(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    idx: usize,
) -> bool {
    use crate::modifier::ModifierElement;
    let mut cur = Some(idx);
    while let Some(i) = cur {
        for el in nodes[i].modifier.elements() {
            if let ModifierElement::TextFieldVisual { enabled, read_only, .. } = el {
                return *enabled && !*read_only;
            }
        }
        cur = nodes[i].parent_id
            .and_then(|pid| crate::layout::node::find_node_by_id(nodes, root, pid));
    }
    true
}
