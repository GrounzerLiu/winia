//! The text field's container payload — what the `Modifier` chain carries and the renderer reads.
//!
//! `ModifierElement::TextFieldVisual` holds a [`TextFieldVisual`] payload, so the variant, the slot
//! roles, the colour set and the three render-time queries all have to be visible below the
//! component. The component keeps `TextField`, `TextFieldDefaults` and the constructors that turn a
//! `WiniaTheme` into a `TextFieldColors` — the design-system half.

use crate::modifier::{ModifierElement};
use crate::graphics::{Color};

/// 容器变体（对齐 material3 TextField（Filled）/ OutlinedTextField）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFieldVariant {
    /// 填充容器 + 底部指示线（M3 FilledTextField——top 4dp 圆角）
    Filled,
    /// 无填充 + 边框（M3 OutlinedTextField——四角 4dp 圆角）
    Outlined,
}

/// M3 TextField 状态色集合（对齐 `TextFieldDefaults.colors` 的默认值；
/// 状态优先级 disabled > error > focused > unfocused）
#[derive(Debug, Clone)]
pub struct TextFieldColors {
    pub text: crate::graphics::Color,
    pub disabled_text: crate::graphics::Color,
    /// 容器背景（Filled 用；Outlined 为透明）
    pub container: crate::graphics::Color,
    /// 光标（正常；error 时用 error_cursor）
    pub cursor: crate::graphics::Color,
    pub error_cursor: crate::graphics::Color,
    /// 指示线/边框
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
    /// 支持文本
    pub supporting: crate::graphics::Color,
    pub disabled_supporting: crate::graphics::Color,
    pub error_supporting: crate::graphics::Color,
    /// 前置图标（focused/unfocused onSurfaceVariant、disabled 38%、error 不变）
    pub leading_icon_focused: crate::graphics::Color,
    pub leading_icon_disabled: crate::graphics::Color,
    /// 后置图标（trailing——error 态 error 色）
    pub trailing_icon_focused: crate::graphics::Color,
    pub trailing_icon_disabled: crate::graphics::Color,
    pub trailing_icon_error: crate::graphics::Color,
    /// 前后缀文本（onSurfaceVariant、disabled 38%）
    pub affix: crate::graphics::Color,
    pub disabled_affix: crate::graphics::Color,
}

/// TextField 容器子节点角色（text-field-v2 容器化——TextFieldLayout
/// policy 按角色布局；构建顺序固定：leading → label → placeholder →
/// prefix → input → suffix → trailing，缺省跳过）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextFieldSlotRole {
    /// 前置图标（M3 leadingIcon——12dp 边距垂直居中，与文本 16dp）
    Leading,
    /// 悬浮/展开 label（位置动画由 policy 插值）
    Label,
    /// 占位文本（输入位；仅空内容时构建）
    Placeholder,
    /// 前缀（文本起点前）
    Prefix,
    /// 输入区（文本/光标/选区/IME——唯一可交互节点）
    Input,
    /// 后缀（右对齐）
    Suffix,
    /// 后置图标（右 12dp 垂直居中）
    Trailing,
}

/// 从节点（TextField 输入 leaf）向上找 OffsetMapping。
///
/// ⚠ TextFieldVisual（含 offset_mapping）挂在**容器** modifier 上
/// （text_field_visual），输入 leaf 无此元素——点击定位/拖拽/IME 区域/
/// 渲染光标从 leaf 查找 TextFieldVisual 会得到 None → 显示偏移直接当
/// 编辑偏移写 selection（掩码字符 '•' 3 字节 → replace_range 越界 panic）
/// 或光标画错位置。沿 parent_id 链向上找第一个 TextFieldVisual
/// （裸 TextField 无 variant → TextFieldOffsetMapping 元素）。
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

/// 从节点（TextField 输入 leaf）向上找容器 TextFieldVisual 的光标色。
///
/// ⚠ cursor_color 在容器 TextFieldVisual（组合期解析 primary/error），
/// 输入 leaf 无此元素——渲染光标从 leaf 查找会回退文本色（非 M3 光标色）。
/// 与 offset_mapping_for_node 同路径沿 parent 链向上找。
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
