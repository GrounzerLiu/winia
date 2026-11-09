//! 渲染管线 — 遍历 LayoutNode 树绘制到 Skia Canvas
//!
//! 单阶段深度遍历：节点级修饰符（阴影/背景/边框/文本/子节点）按序绘制；
//! BackdropBlur 在节点自身内容绘制前即时 snapshot→blur→画回（见
//! `draw_backdrop_blur`）——语义对齐 Compose：只模糊"位于其下"的内容。

use crate::animation::visibility::{ExpandFrom, ExpandFromH, SlideDirection, SlideOffset};
use crate::graphics::skia_color;
use crate::graphics::{DecodedIcon, IconSource, IconSpec, decoded_icon};
use crate::layout::LayoutDirection;
use crate::layout::node::{LayoutNode, PaintDisposition};
use crate::modifier::ModifierElement;
use crate::transition::TransitionRole;
use skia_safe::image_filters;
use skia_safe::sampling_options::{FilterMode, MipmapMode};
use skia_safe::svg;
use skia_safe::{BlendMode, Canvas, Color4f, IRect, Paint, RRect, Rect, SamplingOptions};
use std::cell::RefCell;

// ── 入口 ──

pub fn render(nodes: &[LayoutNode], root_idx: usize, canvas: &Canvas) {
    render_with_menu(nodes, root_idx, canvas, None, (0.0, 0.0));
}

/// A frame that may have a selection context menu open. The window is only needed to clamp the menu
/// inside it; a test or an overlay render passes `(0.0, 0.0)` and gets the unclamped box.
pub fn render_with_menu(
    nodes: &[LayoutNode],
    root_idx: usize,
    canvas: &Canvas,
    menu: Option<&crate::app::gesture::SelectionMenu>,
    window: (f32, f32),
) {
    render_pass1(nodes, root_idx, root_idx, canvas, 0.0, 0.0, false, false);
    // Selection handles last, so a neighbouring node can never paint over a grab area.
    selection_handles::draw(nodes, root_idx, canvas);
    selection_menu::draw(menu, window, canvas);
}

/// Draw one node as a **transition-layer root** at an explicit absolute
/// position, on a canvas that carries no ancestor clips or layer transforms.
///
/// This is the Compose `renderInOverlayDuringTransition` elevation: the node
/// keeps its place in the tree (layout, state and hit testing are untouched)
/// and is merely re-drawn from the root of the canvas by
/// `Composer::render_layer`, which is `pub(crate)` — see
/// `docs/shared-element-transition.md` §3.6.
/// `root_idx` stays the REAL tree root — the text-field helpers inside
/// `render_pass1` resolve colours and offset mappings by walking up to it, so
/// passing the drawn node as the root would cut that walk short.
pub fn render_node_at(
    nodes: &[LayoutNode],
    root_idx: usize,
    idx: usize,
    canvas: &Canvas,
    abs_x: f32,
    abs_y: f32,
) {
    let node = &nodes[idx];
    // `render_pass1` adds `parent_*` to the node's own (parent-relative)
    // position — invert that so the node lands exactly on the absolute point.
    render_pass1(
        nodes,
        root_idx,
        idx,
        canvas,
        abs_x - node.position.x,
        abs_y - node.position.y,
        true,
        // Everything BELOW this entry is painted on a canvas that carries no
        // ancestor translate, so descendants must not add a frozen scroll sum.
        true,
    );
}

mod backdrop;
mod helpers;
mod paint;
mod pass;
mod selection_handles;
mod selection_menu;
pub(crate) use selection_handles::HANDLE_RADIUS as HANDLE_GRAB_RADIUS;
pub(crate) use selection_menu::{
    ITEMS as SELECTION_MENU_ITEMS, PADDING as SELECTION_MENU_PADDING,
    ROW_HEIGHT as SELECTION_MENU_ROW_HEIGHT, menu_rect as selection_menu_rect,
    row_at as selection_menu_row_at,
};
mod text_field;

use pass::render_pass1;

pub use helpers::draw_background_for_node;
pub(crate) use text_field::draw_focus;

#[cfg(test)]
mod tests;
