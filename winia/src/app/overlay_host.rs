//! The overlay host: the windows a `Popup`, `Dialog` or `DropdownMenu` composes into.
//!
//! Split out of `app.rs`. An overlay is a second composer and a second tree that the frame lays
//! out, renders and hit-tests on top of the page — it also takes the keyboard while it is modal.

use super::*;
use super::set_ime_enabled;

/// 顶层弹出层实例——独立 Composer 组合单元（State 跨帧保持），
/// 渲染定位在主树之上（模态遮罩 + 内容）
/// The window's overlay host: the layers themselves, and the interaction state that only makes
/// sense against them.
///
/// These fields were `PerWindow`'s, and every one of them had to be widened so this module could
/// read it — which is the measure of a group that wanted a struct. The drag session, the click
/// target, the frozen arena origins and the focus the host suspends are all about *an overlay*, not
/// about the window; what stays on `PerWindow` is the frame.
pub(crate) struct OverlayHost {
    /// 顶层弹出层（独立组合单元——渲染在主树之上）
    pub(crate) layers: Vec<OverlayWindow>,
    /// overlay 点击目标（down 命中 overlay 记录——up 执行 click；v1 仅 clickable）
    pub(crate) click: Option<(usize, (f32, f32), u64)>,
    /// overlay 拖拽会话（down 命中 overlay 且有 on_drag 的节点时建立——
    /// (overlay index, 节点 slot_key, 拖拽起点 scene)）。overlay 是独立
    /// composer，拖拽走 overlay 内容节点的 on_drag/on_drag_end。
    pub(crate) drag: Option<(usize, u64, (f32, f32))>,
    /// The overlay's screen origin frozen when the drag started — the absolute `pos` handed to
    /// `on_drag_start` / `on_drag` is arena-local, and an overlay that moves during the drag must not
    /// inject its own motion into it (the deltas are scene-space and unaffected).
    pub(crate) drag_origin: (f32, f32),
    /// overlay 拖拽是否已越过 slop 触发 DragStart
    pub(crate) drag_started: bool,
    /// overlay 拖拽上一次 move 位置（增量计算用）
    pub(crate) drag_last: Option<(f32, f32)>,
    /// overlay 内的可滚动容器（独立 Composer——嵌套滚动用，列表到顶下拉拖 Sheet）
    pub(crate) drag_scroll: Option<DragScroll>,
    /// Overlay focus interaction target (overlay id, slot) — overlay inputs report `is_focused()`
    /// only after `emit_focus` in their own arena.
    pub(crate) focused_interaction: Option<(u64, u64)>,
    /// The PAGE's focus while a focus-scope overlay owns the keyboard: a slot key, not a flag on the
    /// tree. Exactly one layer shows a focus ring (the keyboard owner), and the ones below remember
    /// theirs here — see `claim_keyboard_for_overlay` / `release_keyboard_to_lower_layer`.
    pub(crate) suspended_focus_slot: Option<u64>,
}

impl Default for OverlayHost {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            click: None,
            drag: None,
            drag_origin: (0.0, 0.0),
            drag_started: false,
            drag_last: None,
            drag_scroll: None,
            focused_interaction: None,
            suspended_focus_slot: None,
        }
    }
}

pub(crate) struct OverlayWindow {
    pub(crate) id: u64,
    pub(crate) composer: crate::runtime::composer::Composer,
    pub(crate) anchor_slot: Option<u64>,
    /// The direction `*Start`/`*End` positions resolve against, when the caller supplied one at compose time
    /// (see `OverlayDesc::direction`). `None` falls back to the anchor node's direction, else the root's.
    pub(crate) direction: Option<crate::layout::LayoutDirection>,
    pub(crate) position: crate::overlay::PopupPosition,
    pub(crate) offset: (f32, f32),
    /// Grow the panel out of its anchor along both axes (see `OverlayDesc::anchor_slide`).
    pub(crate) anchor_slide: Option<crate::overlay::AnchorSlide>,
    pub(crate) modal: bool,
    pub(crate) dismiss_on_outside: bool,
    pub(crate) click_passthrough: bool,
    /// Fit this overlay inside the window around its anchor (see `OverlayDesc::fit_around_anchor`).
    pub(crate) fit_around_anchor: bool,
    /// Take exactly the anchor's width (see `OverlayDesc::match_anchor_width`).
    pub(crate) match_anchor_width: bool,
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) content: Box<dyn Fn(&mut ComposeCtx)>,
    /// 注册时（主树 provides 内）捕获的 CompositionLocal 快照——recompose
    /// 时重放（overlay 独立 Composer 继承主树主题/方向/排版）
    pub(crate) local_snapshot: crate::runtime::composition_local::LocalSnapshot,
    /// 渲染/命中用的屏幕位置（逻辑坐标——每帧布局后更新）
    pub(crate) screen_pos: (f32, f32),
    /// The anchor's window rect `(x, y, w, h)`, resolved each layout pass (the only pass that knows it).
    /// Kept for the render: an animation that grows out of the anchor needs both rects, and material3's
    /// menus compute their transform origin from exactly these two (`calculateTransformOrigin`).
    pub(crate) anchor_rect: Option<(f32, f32, f32, f32)>,
    /// 该 overlay 内当前 hover 的 hoverable slot 集合（独立于主树——
    /// overlay 是独立 composer，slot 与主树可能重复）
    pub(crate) hovered_slots: std::collections::HashSet<u64>,
    /// 该 overlay 内当前按下 的 interaction source（Press 波纹——up/取消时释放）
    pub(crate) pressed_interaction: Option<(u64, crate::interaction::MutableInteractionSource)>,
    /// 显示进度（1=完全显示，0=隐藏）——进入/退出动画统一驱动：
    /// 打开 push_animatable(progress, 1.0)（0→1），关闭 push(progress, 0.0)
    /// （1→0）；渲染期 peek 计算 scale/alpha。None=无动画（恒 1）
    pub(crate) progress: Option<crate::runtime::state::Animating<f32>>,
    /// 进入动画规格（None = 瞬时——Popup/DropdownMenu 默认）
    pub(crate) enter_anim: Option<crate::overlay::OverlayAnimSpec>,
    /// 退出动画规格（None = 瞬时消失）
    pub(crate) exit_anim: Option<crate::overlay::OverlayAnimSpec>,
    /// Closing in progress (exit animation playing — kept rendered until done;
    /// no interaction meanwhile)
    pub(crate) closing: bool,
    /// When `closing` was set. The exit animation is expected to finish long before
    /// [`CLOSING_DEADLINE`]; if it never reports done (an interrupted or never-advanced tween), a
    /// closing overlay would otherwise keep rendering its modal scrim forever — which is exactly what a
    /// user saw: the sheet gone, the dim layer still over the page.
    pub(crate) closing_since: Option<std::time::Instant>,
    /// Focused node id inside this overlay's own arena (overlay keyboards route
    /// here — the main-tree focus path can never reach overlay nodes).
    /// None = no focus in this overlay.
    pub(crate) focused_id: Option<u64>,
    /// Slot key of the focused node (stable across recompositions — focus is
    /// restored by slot_key after layout, mirroring the main tree).
    pub(crate) focused_slot_key: Option<u64>,
    /// Whether this overlay owns the keyboard while it is open (see
    /// [`crate::overlay::OverlayDesc::focus_scope`]).
    pub(crate) focus_scope: bool,
    /// Whether Escape closes this overlay (Compose's `DialogProperties.dismissOnBackPress`). Copied
    /// from [`crate::overlay::OverlayDesc::dismiss_on_back_press`] and read by [`PerWindow::escape_key`].
    pub(crate) dismiss_on_back_press: bool,
    /// Whether the CONTENT completes its own removal: a dismissal request calls `on_dismiss` and leaves the
    /// overlay open (the component animates out and stops composing it). Copied from
    /// [`crate::overlay::OverlayDesc::dismiss_defers_to_content`].
    pub(crate) dismiss_defers_to_content: bool,
    /// Has this overlay already suspended the page's focus? Claiming is a ONE-TIME transition, and
    /// this is what keeps it one: re-running it every frame walked the whole main tree and called
    /// into the IME on every frame of a modal's life.
    pub(crate) claimed_keyboard: bool,
}
/// 启动 overlay 关闭（退出动画）：标记 closing + 触发 on_dismiss + 驱动
/// progress 1→0（有退出规格）——动画完成前保留渲染（对齐 Compose
/// AnimatedVisibility exit 语义）。无退出规格 → 立即移除。
pub(crate) fn begin_overlay_close(pw: &mut PerWindow, id: u64) {
    let Some(idx) = pw.overlay.layers.iter().position(|o| o.id == id) else { return };
    // 已在关闭中（重复关闭请求）→ 跳过（防重入：动画重启/on_dismiss 重复）
    if pw.overlay.layers[idx].closing {
        return;
    }
    pw.overlay.layers[idx].closing = true;
    pw.overlay.layers[idx].closing_since = Some(std::time::Instant::now());
    if let Some(cb) = pw.overlay.layers[idx].on_dismiss.take() {
        (cb)();
    }
    // Closing surface loses focus (its nodes are about to be destroyed).
    // IME follows: off when nothing else holds focus. Both the cache AND the tree flags go: the
    // overlay keeps rendering through its fade, and a flag left set draws a focus ring on a panel
    // that is on its way out — next to the ring the layer below just got back (caught by the
    // one-ring assertion in `tab_owns_the_keyboard_inside_a_modal_overlay_and_the_page_gets_it_back`).
    pw.overlay.layers[idx].focused_id = None;
    pw.overlay.layers[idx].focused_slot_key = None;
    if let Some(r) = pw.overlay.layers[idx].composer.layout_root_idx() {
        crate::layout::node::clear_focus(pw.overlay.layers[idx].composer.arena_nodes_mut(), r);
    }
    // A focus-scope overlay took the keyboard when it opened; closing it hands the keyboard to the
    // layer below (an overlay that remembers a focus, else the page) — a keyboard user keeps standing
    // where they were instead of starting over.
    release_keyboard_to_lower_layer(pw);
    if pw.focused_id.is_none() && !pw.overlay.layers.iter().any(|o| !o.closing && o.focused_id.is_some()) {
        if let Some(ref sw) = pw.skia_window { set_ime_enabled(&sw, false); }
    }
    cleanup_overlay_interactions(&mut pw.overlay.layers[idx]);
    // 启动退出动画（progress 1→0）；无退出规格 → 立即移除（不推无用动画——
    // 否则对即将 drop 的孤儿 State 浪费 200ms 动画）
    let has_exit = pw.overlay.layers[idx].exit_anim.is_some();
    if has_exit {
        if let Some(p) = pw.overlay.layers[idx].progress.clone() {
            let spec = pw.overlay.layers[idx].exit_anim.as_ref().unwrap();
            crate::animation::push_animatable_handle(
                p, 0.0,
                spec.animation_spec(),
            );
        }
    }
    // An overlay the caller STILL declares is not removed here. `ModalBottomSheet` is the case that
    // proved it: it fades out on its own 200 ms exit spec AND slides out through its `SheetState`,
    // and it reports the dismissal back to the caller only after that slide settles
    // (`shown && settled == Hidden`). Removing the overlay as soon as its fade finished would cut the
    // composition that observer runs in — the caller's `visible` then stayed TRUE for good, so the
    // sheet could never be reopened and the next recomposition rebuilt it (off-screen, with its
    // scrim) from the still-true declaration. `finish_closing_overlays` removes it once the caller
    // stops declaring it, and the fade path below is for overlays nobody re-declares.
    let still_declared = pw.composer.overlay_active.get(&id).copied().unwrap_or(false);
    if !has_exit && !still_declared {
        pw.overlay.layers.remove(idx);
    }
}
/// How long a closing overlay may stay in the list before it is removed regardless of its exit
/// animation. Every exit spec is a few hundred milliseconds; a tween that never reports done (an
/// interrupted or never-advanced one) used to keep the overlay — and its modal scrim — rendering for the
/// rest of the process, which is a stuck dim layer over the window that the user cannot get rid of.
pub(crate) const CLOSING_DEADLINE: std::time::Duration = std::time::Duration::from_millis(1000);
/// Which arena owns the keyboard right now: `Some(i)` = that overlay's own composition,
/// `None` = the window's main tree.
///
/// In two steps:
///
/// 1. The topmost open overlay that declares itself a focus scope takes the keys. It wins outright:
///    asking "who already holds focus" first would hand the keys to an overlay BELOW a modal that
///    still remembers a focused node — measured, with a dialog over the settings sheet: Tab cycled
///    the sheet underneath.
/// 2. With no focus scope up, an overlay something was clicked into keeps the keys (a popup, a
///    tooltip the user is interacting with), else they belong to the main tree.
///
/// The focus scope does NOT have to contain anything focusable. Skipping such a layer would make it
/// fall through to the page behind its scrim, which is the wrong answer for a modal: a dialog with
/// nothing focusable simply has nowhere to put focus, and Tab does nothing rather than reaching
/// behind a scrim for a target. (`focus_scope_is_open` shares that judgement for the key
/// fall-through.)
pub(crate) fn keyboard_scope(pw: &PerWindow) -> Option<usize> {
    if let Some(i) = (0..pw.overlay.layers.len()).rev().find(|&i| overlay_owns_keyboard(&pw.overlay.layers[i])) {
        return Some(i);
    }
    (0..pw.overlay.layers.len())
        .rev()
        .find(|&i| !pw.overlay.layers[i].closing && pw.overlay.layers[i].focused_id.is_some())
}
/// Whether this overlay owns the keyboard: it declares itself a focus scope, has not started
/// closing, and still has a size.
fn overlay_owns_keyboard(ov: &OverlayWindow) -> bool {
    !ov.closing && ov.focus_scope && overlay_has_size(ov)
}
fn overlay_has_size(ov: &OverlayWindow) -> bool {
    ov.composer.layout_root()
        .map(|r| r.measured_size.width > 0.0 && r.measured_size.height > 0.0)
        .unwrap_or(false)
}
/// Is a focus-scope overlay open right now (closing ones do not count)? While one is, the page is
/// out of reach — for keys the overlay does not consume, and for focus requests that would otherwise
/// land on a node behind the scrim.
///
/// Deliberately NOT `keyboard_scope(pw).is_none()`: a focus scope with nothing focusable makes that
/// expression `None` too, which let characters through to the page — measured against this very
/// gate before it was split.
pub(crate) fn focus_scope_is_open(pw: &PerWindow) -> bool {
    pw.overlay.layers.iter().any(|ov| !ov.closing && ov.focus_scope && overlay_has_size(ov))
}
/// Give the keyboard to a focus-scope overlay on the frame it appears: from here on Tab and the
/// arrow keys work inside it and the page's keys have nowhere to go.
///
/// The page KEEPS its focused node. It is not the keyboard target any more, and it draws no ring
/// while the overlay covers it, but nothing forgets it either — closing the overlay puts the
/// keyboard user back where they were, without a fresh Tab round.
///
/// It deliberately does NOT focus anything inside the overlay. A focus ring appearing on its own the
/// moment a panel opens reads as a bug — nothing the user did put it there — so the first Tab is what
/// moves focus in, and until then the panel simply has no focus. (Compose focuses a dialog's first
/// focusable on open; this framework shows the keyboard user the same thing with one keypress, without
/// the ring that a mouse user would see for no reason.)
///
/// Runs right after the overlays have been laid out, because the choice needs their arenas: on the
/// frame an overlay is created, the arena only exists once `layout_overlays` has run.
///
/// Exactly ONE layer draws a focus ring — the one that owns the keyboard — and the layers below keep
/// their focus as a SLOT KEY with no flag on the tree. That is what makes a modal read correctly
/// (nothing highlighted behind the scrim) while a keyboard user still comes back to where they were
/// (see `release_keyboard_to_lower_layer`).
pub(crate) fn claim_keyboard_for_overlay(pw: &mut PerWindow) {
    let scope = (0..pw.overlay.layers.len()).rev().find(|&i| {
        !pw.overlay.layers[i].claimed_keyboard
            && pw.overlay.layers[i].focused_id.is_none()
            && overlay_owns_keyboard(&pw.overlay.layers[i])
    });
    let Some(i) = scope else { return };
    pw.overlay.layers[i].claimed_keyboard = true;

    // Suspend the page's focus instead of dropping it. Read from the ARENA, not from
    // `pw.focused_slot_key`: a click focuses a node through `focus_by_id` and leaves the cached slot
    // key alone, so that cache read as `None` while the page did have focus (measured: dismissing the
    // sheet used to restore nothing at all).
    if pw.overlay.suspended_focus_slot.is_none() {
        let page_focus = pw.composer.layout_root_idx().and_then(|r| {
            let nodes = pw.composer.arena_nodes();
            let id = crate::layout::node::get_focus_id(nodes, r)?;
            crate::layout::node::find_node_by_id(nodes, r, id).map(|idx| nodes[idx].slot_key)
        });
        pw.overlay.suspended_focus_slot = page_focus.or(pw.focused_slot_key);
    }
    if let Some(r) = pw.composer.layout_root_idx() {
        crate::layout::node::clear_focus(pw.composer.arena_nodes_mut(), r);
    }
    pw.focused_id = None;
    pw.focused_slot_key = None;
    // Overlays BELOW the owner lose their ring too, and keep their memory in their own slot keys.
    for j in 0..pw.overlay.layers.len() {
        if j == i || pw.overlay.layers[j].focused_id.is_none() {
            continue;
        }
        if let Some(r) = pw.overlay.layers[j].composer.layout_root_idx() {
            crate::layout::node::clear_focus(pw.overlay.layers[j].composer.arena_nodes_mut(), r);
        }
    }
    if let Some(ref sw) = pw.skia_window {
        // Nothing in the panel is focused until the user tabs into it, so the IME has no target yet.
        set_ime_enabled(&sw, false);
    }
}
/// Hand the keyboard back when the last focus-scope overlay stops owning it: the nearest layer below
/// that remembers a focus takes the ring back (an overlay, else the page).
///
/// This reverses `claim_keyboard_for_overlay`: the memory was kept as a slot key, so restoring is
/// re-marking that node. Non-destructive by construction — it only ever writes into a layer that has
/// no focus of its own right now.
pub(crate) fn release_keyboard_to_lower_layer(pw: &mut PerWindow) {
    if pw.overlay.layers.iter().any(|o| !o.closing && o.focus_scope && overlay_has_size(o)) {
        return; // still covered: a focus scope that is still open keeps the keyboard
    }
    for i in (0..pw.overlay.layers.len()).rev() {
        let remembered = {
            let ov = &pw.overlay.layers[i];
            if ov.closing {
                None
            } else {
                match (ov.composer.layout_root_idx(), ov.focused_slot_key) {
                    (Some(r), Some(slot)) => Some((r, slot)),
                    _ => None,
                }
            }
        };
        let Some((r, slot)) = remembered else { continue };
        let nodes = pw.overlay.layers[i].composer.arena_nodes_mut();
        if let Some(id) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
            crate::layout::node::clear_focus(nodes, r);
            if crate::layout::node::set_focus_by_id(nodes, r, id) {
                pw.overlay.layers[i].focused_id = Some(id);
                return;
            }
        }
    }
    let Some(slot) = pw.overlay.suspended_focus_slot.take() else { return };
    let Some(r) = pw.composer.layout_root_idx() else { return };
    let nodes = pw.composer.arena_nodes_mut();
    if let Some(id) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        crate::layout::node::clear_focus(nodes, r);
        if crate::layout::node::set_focus_by_id(nodes, r, id) {
            pw.focused_id = Some(id);
            pw.focused_slot_key = Some(slot);
            pw.apply_ime_for_focus(Some(id));
        }
    }
}
/// Close bookkeeping: drop a closing overlay whose fade is over AND whose caller has stopped
/// declaring it, and hand a still-declared one back its interactivity if the close never concludes.
///
/// The declaration is what decides. A component can keep its overlay alive through a close — the
/// modal sheet slides out with its own `SheetState` and reports the dismissal to the caller only
/// after the slide settles — so removing it on the strength of "no exit animation" cut the
/// composition that observer lives in, froze the caller's `visible` at true for good, and let the
/// next recomposition rebuild the panel off-screen with its scrim (measured: sheet unreachable
/// afterwards, dim layer permanent).
pub(crate) fn finish_closing_overlays(pw: &mut PerWindow) {
    let now = std::time::Instant::now();
    // Ids the caller still declares this frame (`overlay_active` is written during compose).
    let declared: Vec<u64> = pw.composer.overlay_active
        .iter()
        .filter(|(_, active)| **active)
        .map(|(&id, _)| id)
        .collect();
    pw.overlay.layers.retain_mut(|ov| {
        if !ov.closing { return true; }
        let is_declared = declared.contains(&ov.id);
        // 无 progress（无动画）不应到这里（begin 已移除）；有则等动画完成
        let progress = ov.progress.as_ref().map(|p| p.peek());
        let done = closing_overlay_is_done(progress, ov.closing_since, now);
        if is_declared {
            // The fade finished (or timed out) and the caller still wants this overlay: it is not
            // going away, so give it back rather than leaving a panel the user can see and not touch.
            if done && closing_overlay_timed_out(ov.closing_since, now) {
                ov.resume_after_close();
            }
            return true;
        }
        if !done {
            return true;
        }
        if let Some(p) = ov.progress.as_ref() {
            // A fade that never reported done: drop it and stop driving the state (it is about to be
            // dropped, and a stray animation would keep writing to it). This cut IS visible — the fade
            // was still somewhere between 1 and 0 — which is the price of the deadline; it only happens
            // when the animation system already failed.
            crate::animation::cancel_animation_by_id(p.state_id());
        }
        false
    });
}
/// Whether a close has been pending past [`CLOSING_DEADLINE`] — the safety net for a tween that never
/// reports done, and the trigger for handing a still-declared overlay back its interactivity.
fn closing_overlay_timed_out(closing_since: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    closing_since.is_some_and(|since| now.duration_since(since) >= CLOSING_DEADLINE)
}
/// Whether a closing overlay may be dropped: its fade finished, or it has been closing past
/// [`CLOSING_DEADLINE`]. The deadline is the safety net for a tween that never reports done — without it
/// the overlay stayed in the list forever, rendering its modal scrim over a page the user could no longer
/// dim away (the sheet itself had already slid out of view).
pub(crate) fn closing_overlay_is_done(
    progress: Option<f32>,
    closing_since: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    match progress {
        // No progress channel at all — nothing to wait for (`begin_overlay_close` removes those at once).
        None => true,
        Some(p) if p < 0.001 => true,
        Some(_) => closing_since.is_some_and(|since| now.duration_since(since) >= CLOSING_DEADLINE),
    }
}

/// overlay 移除前清理交互状态（hover 补 Exit + press 释放）——
/// overlay 删除后其节点销毁，交互 source 悬空 → 必须在移除前发射
fn cleanup_overlay_interactions(ov: &mut OverlayWindow) {
    let olds: Vec<u64> = ov.hovered_slots.drain().collect();
    for slot in olds {
        overlay_exit_hover_at(ov, slot);
    }
    if let Some((_, src)) = ov.pressed_interaction.take() {
        src.emit_release();
    }
}
/// The window rect `(x, y, w, h)` of an overlay's anchor, resolved from the MAIN tree (already laid out).
///
/// Shared by the two passes that need it: the measure pass, for an overlay that matches its anchor's width
/// (material3's `matchAnchorWidth`, which forces `minWidth = maxWidth = menuWidth`), and the positioning
/// pass, which places the overlay against it. Takes the arena rather than the window so callers can hold
/// their own borrows.
pub(crate) fn overlay_anchor_rect(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    slot: u64,
) -> Option<(f32, f32, f32, f32)> {
    let id = crate::layout::node::find_node_id_by_slot_key(nodes, root, slot)?;
    let index = crate::layout::node::find_node_by_id(nodes, root, id)?;
    let (x, y) = node_abs_position(nodes, root, id);
    let size = nodes[index].measured_size;
    Some((x, y, size.width, size.height))
}

/// overlay compose + layout（独立组合单元——约束为窗口尺寸），并计算屏幕定位

mod input;
mod placement;
mod render;
mod window;

pub(crate) use input::*;
pub(crate) use placement::*;
pub(crate) use render::*;
pub(crate) use window::*;

#[cfg(test)]
mod tests;
