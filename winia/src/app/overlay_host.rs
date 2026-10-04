//! The overlay host: the windows a `Popup`, `Dialog` or `DropdownMenu` composes into.
//!
//! Split out of `app.rs`. An overlay is a second composer and a second tree that the frame lays
//! out, renders and hit-tests on top of the page — it also takes the keyboard while it is modal.

use super::*;

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
        if let Some(ref sw) = pw.skia_window { sw.set_ime_allowed(false); }
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
        sw.set_ime_allowed(false);
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
pub(crate) fn layout_overlays(pw: &mut PerWindow) {
    // material3's `matchAnchorWidth` FORCES a menu's width to its anchor's (`minWidth = maxWidth =
    // menuWidth` in `exposedDropdownSize`), so those widths are needed at MEASURE time — before this pass
    // reaches the positioning code. Resolved up front into owned values: the loop below holds
    // `&mut pw.overlay.layers` and mutates `pw.composer`, so no borrow of the arena can stay alive across it.
    let anchor_widths: Vec<f32> = {
        let nodes = pw.composer.arena_nodes();
        let root = pw.composer.layout_root_idx();
        pw.overlay.layers
            .iter()
            .map(|ov| {
                if !ov.match_anchor_width {
                    return 0.0;
                }
                let rect = ov
                    .anchor_slot
                    .and_then(|slot| root.and_then(|r| overlay_anchor_rect(nodes, r, slot)));
                rect.map(|(_, _, w, _)| w).unwrap_or(0.0)
            })
            .collect()
    };
    for (overlay_index, ov) in pw.overlay.layers.iter_mut().enumerate() {
        // 关闭中：仍需 recompose/layout 以驱动 BottomSheet 的 slide（offset 动画）
        // —— Dialog 等静态内容重组无副作用；冻结仅针对交互（hit_overlay 已跳过 closing）
        // 之前 `if closing { continue; }` 导致 hide() 的 offset 动画不被布局，面板
        // 卡在 401 仅 fade，视觉上“淡出而非下滑”。
        if ov.closing {
            // 仍执行 recompose/layout，但不处理输入（hit_overlay 已过滤 closing）
        }
        // ⚠ 重放 CompositionLocal 快照（主树捕获时的主题/方向/排版）——
        // overlay 独立 Composer 在 provides 弹栈后 recompose，需快照继承。
        let snap = ov.local_snapshot.clone();
        // Same same-frame contract as the main tree (a measure whose window cannot fill the viewport it
        // is measured in asks for one more compose). Consuming the request HERE, right after this
        // overlay's own layout, is what scopes it to this composer: an overlay that asks nothing leaves
        // the flag clear for the next one, so nothing is consumed on another tree's behalf.
        for _ in 0..8 {
            crate::runtime::composition_local::with_snapshot(&snap, || {
                ov.composer.recompose(|ctx| (ov.content)(ctx));
            });
            // An overlay that fits itself around its anchor also keeps material3's
            // `MenuVerticalMargin` (48dp) clear of the top and bottom window edges, so a menu that ends
            // up as tall as the space allows still reads as a panel floating over the page instead of a
            // full-bleed column. The margin is a MEASURE constraint here and a placement clamp below:
            // without it a 30-item menu took the whole window height and sat flush against both edges.
            let constraints = if ov.fit_around_anchor {
                // material3's `matchAnchorWidth` FORCES the width (`minWidth = maxWidth = menuWidth` in
                // `exposedDropdownSize`), so the anchor's rect is needed at measure time — before the
                // positioning pass — and the anchor lives in the main tree, already laid out.
                let anchor_width = anchor_widths[overlay_index];
                let margin = crate::components::dropdown_menu::MENU_VERTICAL_MARGIN;
                crate::layout::Constraints::new(
                    anchor_width,
                    if ov.match_anchor_width { anchor_width } else { pw.width },
                    margin,
                    (pw.height - margin * 2.0).max(0.0),
                )
            } else {
                crate::layout::Constraints::new(0.0, pw.width, 0.0, pw.height)
            };
            ov.composer.layout(constraints);
            if !crate::runtime::composer::take_compose_after_layout() {
                break;
            }
        }
    }
    // 定位（需主树锚点位置——在 draw 前算）
    let nodes = pw.composer.arena_nodes();
    let root = pw.composer.layout_root_idx();
    let (w, h) = (pw.width, pw.height);
    for ov in &mut pw.overlay.layers {
        let size = ov.composer.layout_root()
            .map(|r| (r.measured_size.width, r.measured_size.height))
            .unwrap_or((0.0, 0.0));
        // 锚点位置（主树）
        let anchor = ov.anchor_slot.and_then(|s| root.and_then(|r| {
            crate::layout::node::find_node_id_by_slot_key(nodes, r, s)
        })).and_then(|nid| root.map(|r| {
            node_abs_position(nodes, r, nid)
        }));
        let anchor_size = ov.anchor_slot.and_then(|s| root.and_then(|r| {
            crate::layout::node::find_node_id_by_slot_key(nodes, r, s)
        })).and_then(|nid| root.and_then(|r| {
            crate::layout::node::find_node_by_id(nodes, r, nid).map(|i| nodes[i].measured_size)
        }));
        let anchored = anchor.is_some() && anchor_size.is_some();
        let (ax, ay, aw, ah) = match (anchor, anchor_size) {
            (Some((x, y)), Some(s)) => (x, y, s.width, s.height),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        use crate::overlay::PopupPosition as P;
        let pos = match ov.position {
            // 窗口对齐（无锚点）
            P::Center => ((w - size.0) / 2.0, (h - size.1) / 2.0),
            P::TopLeft => (0.0, 0.0),
            P::TopCenter => ((w - size.0) / 2.0, 0.0),
            P::TopRight => (w - size.0, 0.0),
            P::BottomLeft => (0.0, h - size.1),
            P::BottomCenter => ((w - size.0) / 2.0, h - size.1),
            P::BottomRight => (w - size.0, h - size.1),
        };
        // 有锚点（且在主树中找到）时：按位置相对锚点（Bottom* = 锚点下方，
        // Top* = 锚点上方）；锚点缺失/未物化（scope）回退窗口对齐——避免 (0,0)
        let pos = if anchored && ov.fit_around_anchor {
            // material3's candidate lists, in `overlay::dropdown_menu_position`: below the anchor, above it,
            // centred on its top edge, then pinned to the nearer window edge — each taken only if the menu
            // fits inside `MenuVerticalMargin` (48dp). winia's placement used to do the first alone, which is
            // why a long menu hung off the bottom edge with its last rows unreachable.
            crate::components::dropdown_menu::dropdown_menu_position((ax, ay, aw, ah), size, (w, h))
        } else if anchored {
            match ov.position {
                P::BottomLeft => (ax, ay + ah),
                P::BottomCenter => (ax + (aw - size.0) / 2.0, ay + ah),
                P::BottomRight => (ax + aw - size.0, ay + ah),
                P::TopLeft => (ax, ay - size.1),
                P::TopCenter => (ax + (aw - size.0) / 2.0, ay - size.1),
                P::TopRight => (ax + aw - size.0, ay - size.1),
                P::Center => ((w - size.0) / 2.0, (h - size.1) / 2.0),
            }
        } else { pos };
        // Anchor slide: the panel starts at the anchor's corner and ends at the window's, both axes moving
        // together — Compose's `(lerp(collapsedBounds.left, offsetX, progress),
        // lerp(collapsedBounds.top, offsetY, progress))` with both offsets 0. Only this pass knows the
        // anchor's coordinates, so the interpolation happens here; the caller supplies the progress reader.
        let pos = if let (Some(slide), true) = (&ov.anchor_slide, anchored) {
            let p = slide.progress();
            (
                crate::overlay::anchor_slide_lerp(ax, p),
                crate::overlay::anchor_slide_lerp(ay, p),
            )
        } else {
            pos
        };
        ov.screen_pos = (pos.0 + ov.offset.0, pos.1 + ov.offset.1);
        // The anchor rect, for the render's transform origin (see `OverlayWindow::anchor_rect`).
        ov.anchor_rect = anchored.then_some((ax, ay, aw, ah));
        // Flight coordinate frame (Phase 4 Tier1): overlay canvas renders
        // translated by screen_pos — visuals store window-minus-origin.
        ov.composer.screen_origin = ov.screen_pos;
    }
    // Shared-element Tier0 polls (mirrors the main-tree post-layout poll —
    // overlay composers drive their own flights; Tier1 spans composers and
    // is driven by cross-poll in the main flow below).
    for ov in &mut pw.overlay.layers {
        ov.composer.poll_shared_flights();
    }
    // Overlay focus restore across recompositions (mirrors the main-tree
    // focused_slot_key restore in recompose_layout_render): arena node ids are
    // rebuilt, slot keys are stable.
    for ov in &mut pw.overlay.layers {
        let Some(slot) = ov.focused_slot_key else { continue; };
        let Some(r) = ov.composer.layout_root_idx() else { continue; };
        let nodes = ov.composer.arena_nodes_mut();
        if let Some(new_id) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
            crate::layout::node::clear_focus(nodes, r);
            crate::layout::node::set_focus_by_id(nodes, r, new_id);
            ov.focused_id = Some(new_id);
        } else {
            ov.focused_id = None;
            ov.focused_slot_key = None;
        }
    }
}
/// overlay 命中测试——返回 (overlay 索引, 本地坐标)——从最上层（最后一个）往下
pub(crate) fn hit_overlay(pw: &PerWindow, scene_pos: (f32, f32)) -> Option<(usize, (f32, f32))> {
    for i in (0..pw.overlay.layers.len()).rev() {
        let ov = &pw.overlay.layers[i];
        // 关闭中（退出动画播放）：不响应交互——命中视同穿透（下层/主树）
        if ov.closing {
            continue;
        }
        let local = (scene_pos.0 - ov.screen_pos.0, scene_pos.1 - ov.screen_pos.1);
        if let Some(r) = ov.composer.layout_root_idx() {
            let nodes = ov.composer.arena_nodes();
            if !hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1).is_empty() {
                return Some((i, local));
            }
        }
    }
    None
}
/// overlay 渲染（主树之后——上层；模态先画遮罩）
pub(crate) fn render_overlays(overlays: &[OverlayWindow], canvas: &skia_safe::Canvas, scale: f32, window: (f32, f32)) {
    for ov in overlays {
        // 显示进度 → (scale, alpha, dy, reveal)：progress State 驱动（peek——
        // 渲染期零重组）。打开：progress 0→1，apply() 正向；关闭（closing）：
        // progress 1→0，apply_exit() 反向。dy = 高度倍数位移；reveal = 揭示
        // 高度倍数（顶部展开用）。
        let (anim_scale, anim_alpha, anim_dy, anim_reveal) = match (&ov.progress, ov.closing, &ov.enter_anim, &ov.exit_anim) {
            (Some(p), false, Some(spec), _) => spec.apply(p.peek()),       // 进入
            (Some(p), true, _, Some(spec)) => spec.apply_exit(p.peek()),   // 退出
            _ => (1.0, 1.0, 0.0, 1.0),                                     // 无动画
        };
        // 模态遮罩（淡入淡出——跟随内容 alpha）
        if ov.modal {
            let mut mask = skia_safe::Paint::default();
            mask.set_color(skia_safe::Color::from_argb((110.0 * anim_alpha) as u8, 0, 0, 0));
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(0.0, 0.0, window.0 * scale, window.1 * scale),
                &mask,
            );
        }
        let Some(r) = ov.composer.layout_root_idx() else { continue; };
        let nodes = ov.composer.arena_nodes();
        canvas.save();
        canvas.translate((ov.screen_pos.0 * scale, ov.screen_pos.1 * scale));
        // 进入/退出动画：围绕 overlay 中心缩放 + 内容淡入淡出 + 垂直滑入。
        // ⚠ scale 在 translate 之后——先定位再缩放（缩放中心 = overlay 左上角 +
        // 内容半尺寸，即内容中心）；slide 在缩放之后、内容绘制之前（布局空间位移）
        let size = ov.composer.layout_root()
            .map(|r| (r.measured_size.width, r.measured_size.height))
            .unwrap_or((0.0, 0.0));
        // Slide/揭示期间裁到内容框（对齐上游下拉 `.clip(dropdownShape)` 与
        // 展开揭示）——否则滑入起点整个面板压住 bar。clip 在 settled bounds 上；
        // 动画结束内容恰好落回框内 → 无跳变。reveal<1 时按揭示高度裁。
        // (anim_dy != 0.0) 或 (anim_reveal < 1.0) 时裁剪。
        // The clip height comes from `overlay_reveal_clip_height`, which also keeps "no clip" (`None`)
        // distinct from "clipped to zero" — the distinction the render trace needs.
        let clip_h = crate::overlay::overlay_reveal_clip_height(size.1, anim_dy, anim_reveal)
            .unwrap_or(-1.0);
        // Render-time trace: the overlay's reveal/alpha/offset exist only here, at paint time — the debug
        // server's tree reports POST-LAYOUT sizes, so a reveal (a clip) was invisible to every other
        // channel (`reveal` and `dy` are fractions; `clip_h` is what is actually applied).
        if crate::anim_trace::enabled() {
            let mut r = crate::anim_trace::TraceRecord::render(ov.id);
            r.phase = Some(if ov.closing { "closing" } else { "opening" });
            r.alpha = Some(anim_alpha);
            r.clip = Some(clip_h >= 0.0);
            // `painted.h` is the height actually drawn: `clip_h` when revealing, and the FULL height when
            // the animation is settled (`clip_h == -1` means "no clip", not "zero height" — reporting 0
            // there made a fully open panel look empty).
            let painted_h = if clip_h >= 0.0 { clip_h } else { size.1 };
            r.painted = Some(crate::anim_trace::TraceRect {
                x: ov.screen_pos.0,
                y: ov.screen_pos.1,
                w: size.0,
                h: painted_h,
            });
            r.detail = Some(format!(
                "reveal={anim_reveal:.4} dy={anim_dy:.4} scale={anim_scale:.4} layout_h={:.1}",
                size.1
            ));
            crate::anim_trace::record(r);
        }
        if clip_h >= 0.0 {
            // clip_rect is affected by the current matrix (screen_pos translate
            // applied above, no content scale yet) — coordinates are device px,
            // so multiply layout units by scale (same as the scrim rect above).
            canvas.clip_rect(
                skia_safe::Rect::from_xywh(0.0, 0.0, size.0 * scale, clip_h * scale),
                None,
                Some(false),
            );
        }
        if anim_scale != 1.0 {
            // Scale around the pivot the animation asks for: material3's menus grow out of their anchor
            // (`anchor_pivot`, the origin `calculateTransformOrigin` picks), everything else keeps the
            // content centre it has always used.
            let (px_frac, py_frac) = match (ov.closing, &ov.enter_anim, &ov.exit_anim) {
                (false, Some(spec), _) if spec.anchor_pivot => ov
                    .anchor_rect
                    .map(|anchor| {
                        crate::overlay::overlay_transform_origin(
                            anchor,
                            (ov.screen_pos.0, ov.screen_pos.1, size.0, size.1),
                        )
                    })
                    .unwrap_or((0.5, 0.5)),
                (true, _, Some(spec)) if spec.anchor_pivot => ov
                    .anchor_rect
                    .map(|anchor| {
                        crate::overlay::overlay_transform_origin(
                            anchor,
                            (ov.screen_pos.0, ov.screen_pos.1, size.0, size.1),
                        )
                    })
                    .unwrap_or((0.5, 0.5)),
                _ => (0.5, 0.5),
            };
            canvas.translate(((px_frac * size.0) * scale, (py_frac * size.1) * scale));
            canvas.scale((anim_scale, anim_scale));
            canvas.translate((-(px_frac * size.0) * scale, -(py_frac * size.1) * scale));
        }
        if anim_dy != 0.0 {
            canvas.translate((0.0, anim_dy * size.1 * scale));
        }
        if anim_alpha < 1.0 {
            // 内容淡入/淡出：整体 alpha 层（save_layer_alpha_f——Skia 层叠 alpha）
            canvas.save_layer_alpha_f(None, anim_alpha);
        }
        // overlay 内容与主树一致按 scale 绘制（坐标均为逻辑单位）——
        // 缺省会导致内容以 1x 绘制：可见位置/大小与命中测试（逻辑坐标）错位
        canvas.scale((scale, scale));
        render::render(nodes, r, canvas);
        // Transition layer (sources + elevated endpoints), overlay-local coords.
        ov.composer.render_layer(canvas);
        if anim_alpha < 1.0 {
            canvas.restore();
        }
        canvas.restore();
    }
}
/// overlay 拖拽结束（含嵌套滚动 fling）。overlay 内可滚列表的 fling 走 overlay Composer。
pub(crate) fn overlay_drag_up(pw: &mut PerWindow) -> bool {
    let mut handled = false;
    if let Some(ds) = pw.overlay.drag_scroll.take() {
        // 复用主树 drag_scroll_up 逻辑：样本估速 → overlay 的 dispatch_nested_scroll_fling
        //（此前为空壳——注释"由 overlay 内 NestedScrollConnection on_post_fling 兜底"未接通，
        // 导致松手后列表不 fling、sheet 不 settle → "松手没有动画"）
        let (vx, vy) = (ds.velocity_x(), ds.velocity_y());
        // target 反查：ds.slot 是 down 时 overlay composer 的 scroll 节点 slot_key
        let target: Option<usize> = (|| {
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            let ov = pw.overlay.layers.get(ov_idx)?;
            let r = ov.composer.layout_root_idx()?;
            let nodes = ov.composer.arena_nodes();
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, ds.slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[overlay-drag-up] slot={:?} target={:?} v=({},{})", ds.slot, target, vx, vy);
        }
        let Some(idx) = target else { handled = true; return handled; };
        let Some(ov) = pw.overlay.layers.last_mut() else { handled = true; return handled; };
        let Some(r) = ov.composer.layout_root_idx() else { handled = true; return handled; };
        // 手指速度 → 滚动速度（内容速度 = -手指速度），走 nested scroll pre/post fling 链
        let velocity = crate::nested_scroll::ScrollVelocity { x: -vx, y: -vy };
        let _ = dispatch_nested_scroll_fling(ov.composer.arena_nodes_mut(), r, idx, velocity);
        handled = true;
    }
    let Some((idx, slot, _down)) = pw.overlay.drag.take() else { return handled; };
    let started = pw.overlay.drag_started;
    pw.overlay.drag_started = false;
    pw.overlay.drag_last = None;
    if !started {
        return handled;
    }
    if let Some(ov) = pw.overlay.layers.get(idx) {
        let nodes = ov.composer.arena_nodes();
        if let Some(r) = ov.composer.layout_root_idx() {
            fire_gesture_action(nodes, r, slot,
                crate::input::gesture::GestureAction::DragEnd);
        }
    }
    true
}
pub(crate) fn exec_overlay_click(pw: &mut PerWindow) -> bool {
    let Some((idx, local, _nid)) = pw.overlay.click.take() else { return false; };
    let Some(ov) = pw.overlay.layers.get(idx) else { return false; };
    // 关闭中（down 后外部 dismiss 等）→ 不 fire click（避免对已关闭的
    // overlay 误触发——极窄边界但语义正确）
    if ov.closing {
        return false;
    }
    let Some(r) = ov.composer.layout_root_idx() else { return false; };
    let nodes = ov.composer.arena_nodes();
    let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
    // 沿路径找 clickable（最内层优先）
    let r = fire_click_along_path(nodes, &path);
    r
}
/// 指针按下：先测 overlay（最上层）——命中 → 记录点击目标；外部 → dismiss
pub(crate) fn overlay_down(pw: &mut PerWindow, scene_pos: (f32, f32), kind: crate::input::PointerKind) -> bool {
    if pw.overlay.layers.is_empty() {
        return false;
    }
    if let Some((i, local)) = hit_overlay(pw, scene_pos) {
        // 命中 overlay 内容——记录点击目标（v1：仅 clickable——up 时执行）
        // ⚠ click_passthrough（Tooltip）：命中浮层但**放行主树**——浮层盖住
        // 锚点（锚点上方 tooltip 与锚点本身重叠）时点击锚点仍生效（否则
        // tooltip 挡住锚点按钮 → 外部 visible 控制关不了）
        let ov = &mut pw.overlay.layers[i];
        if ov.click_passthrough {
            return false;
        }
        // overlay：按 drag 相对 scroll 的深度判定（对齐主树 child_drag「最内层手势优先」：
        // app.rs:2922-2926）——内层可拖组件（slider/switch）优先于滚动；外层面板 on_drag 让位
        // 于内层滚动（列表滚动优先，到边后由 SheetNested.on_post_scroll 折叠 sheet）。
        // path 下标从根到叶递增；rev() 找到的第一个命中即最内层（下标最大）。
        // - drag 更深（组件在 scroll 内部，didx_last > scroll_idx 且更内层）→ 拖拽组件优先
        // - 否则若有 scroll → 滚动优先（列表滚；面板 on_drag 让位）
        // - 否则若有 drag → 面板 on_drag 拖 sheet（背景/文字/空白）
        {
            let nodes = ov.composer.arena_nodes();
            if let Some(r) = ov.composer.layout_root_idx() {
                let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
                let scroll_idx = path_scroll_idx(nodes, &path);
                let drag_idx = path_drag_idx(nodes, &path);
                let inner_comp_drag = inner_component_drag(drag_idx, scroll_idx);
                // The axis arbitration in `gesture_move` is main-tree only: an overlay drag runs
                // through `overlay_drag` (its own session, which fires the callbacks itself), not
                // through the tracker, so there is nothing to hand over here.
                pw.input.axis = None;
                pw.input.scroll_slot = None;
                if inner_comp_drag {
                    if let Some(didx) = drag_idx {
                        pw.overlay.drag = Some((i, nodes[didx].slot_key, scene_pos));
                        pw.overlay.drag_origin = ov.screen_pos;
                        pw.overlay.drag_started = false;
                        pw.overlay.drag_last = None;
                        pw.overlay.drag_scroll = None;
                    }
                } else if let Some(t) = scroll_idx {
                    // 列表区（面板 on_drag 让位）：内容滚动优先
                    pw.overlay.drag = None;
                    pw.overlay.drag_started = false;
                    pw.overlay.drag_last = None;
                    pw.overlay.drag_scroll = Some(DragScroll::new(nodes[t].slot_key, scene_pos));
                } else if let Some(didx) = drag_idx {
                    // 非滚动区：fallback 到面板 on_drag（背景/文字拖 sheet）
                    pw.overlay.drag = Some((i, nodes[didx].slot_key, scene_pos));
                    pw.overlay.drag_origin = ov.screen_pos;
                    pw.overlay.drag_started = false;
                    pw.overlay.drag_last = None;
                    pw.overlay.drag_scroll = None;
                } else {
                    pw.overlay.drag = None;
                    pw.overlay.drag_started = false;
                    pw.overlay.drag_last = None;
                    pw.overlay.drag_scroll = None;
                }
            }
        }
        // 发射 Press（按下波纹——对标 Compose PressInteraction.Press；
        // ripple 渲染已支持 overlay，缺的只是事件触发）
        {
            // ⚠ 找不到 clickable_interaction 时**跳过 Press 但不阻断**——
            // 否则普通 clickable（无 interaction source，如 ModalBottomSheet
            // Scrim 层）的点击永远到不了 overlay_click → 点击丢失。
            let nodes = ov.composer.arena_nodes();
            if let Some(r) = ov.composer.layout_root_idx() {
                let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
                if let Some(&idx) = path.iter().rev().find(|&&i| {
                    nodes[i].modifier.clickable_interaction().is_some()
                        || nodes[i].modifier.node_click_interaction().is_some()
                }) {
                    if let Some(src) = nodes[idx]
                        .modifier
                        .clickable_interaction()
                        .cloned()
                        .or_else(|| nodes[idx].modifier.node_click_interaction())
                    {
                        // 波纹中心 = 节点本地坐标（overlay 内无滚动/变换——直接换算）
                        let local_press = crate::layout::node::scene_to_node_local(nodes, &path, idx, local.0, local.1);
                        src.emit_press_at(local_press);
                        ov.pressed_interaction = Some((nodes[idx].slot_key, src.clone()));
                    }
                }
            }
        }
        // Press gesture — the same dispatch the main tree does from `gesture_down`. The overlay
        // path used to fire only `on_click` (on up, through `fire_click_along_path`), so a
        // component whose reaction lives on `on_press` was dead inside a popup: a popup
        // `TextField` focuses through `on_press → FocusRequester::request_focus`, which is why
        // this handler used to need a focus rule of its own. With the gesture dispatched, popup
        // content reacts like main-tree content and that rule is gone (the focus lands one frame
        // later, through the ordinary focus-request queue — see `take_focus_requests`).
        //
        // The tracker goes with it, so the rest of the tap family (`on_tap` / `on_double_tap` /
        // `on_long_press`) fires in a popup too; `gesture_move` / `gesture_up` route their actions
        // back into this arena (`pw.input.arena`). A target WITH drag gestures is tracked as well:
        // the overlay drag session for that same node is stood down below, because both dispatchers
        // would otherwise fire its `on_drag_*` callbacks for one gesture.
        {
            // Immutable scope: resolve the target and fire the Press, then leave the overlay borrow
            // before the tracker bookkeeping below needs `pw` mutably.
            let target = {
                let ov = &pw.overlay.layers[i];
                let nodes = ov.composer.arena_nodes();
                match ov.composer.layout_root_idx() {
                    None => None,
                    Some(r) => {
                        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
                        press_gesture_target(nodes, &path).map(|(nid, slot, has_drag)| {
                            fire_gesture_action(nodes, r, slot, crate::input::gesture::GestureAction::Press(local));
                            (nid, slot, has_drag, ov.id)
                        })
                    }
                }
            };
            if let Some((nid, slot, has_drag, ov_id)) = target {
                if has_drag {
                    // The tracker owns this node's drag as well as its tap family: the arbitration
                    // above gave the same node to `pw.overlay.drag`, and both dispatchers would fire
                    // `on_drag_start` / `on_drag` / `on_drag_end` for one gesture. Stand the overlay
                    // session down instead — the tracker routes the drag through
                    // `fire_in_gesture_arena` into this same arena, so nothing is lost, and the node
                    // gains its `on_tap` / `on_double_tap` / `on_long_press` (the tap family of a
                    // popup drag target used to be unreachable, `Slider` included).
                    if pw.overlay.drag.map(|(idx, key, _)| idx == i && key == slot).unwrap_or(false) {
                        pw.overlay.drag = None;
                        pw.overlay.drag_started = false;
                        pw.overlay.drag_last = None;
                    }
                    // A scroll session belongs to a DIFFERENT node (the arbitration picks scroll when
                    // the drag is its ancestor); leave it alone.
                }
                // Same bookkeeping as the main tree for both kinds of target: a deferred tap on this
                // node is due, or this press is its double-tap candidate.
                process_pending_taps_on_down(pw, nid);
                let ctx = pw.input.tap_ctx.take()
                    .filter(|(n, _, _)| *n == nid)
                    .map(|(_, t, p)| (t, p));
                pw.input.tracker = Some(crate::input::gesture::GestureTracker::new(nid, local, has_drag, ctx));
                pw.input.node = Some(nid);
                pw.input.slot = Some(slot);
                pw.input.arena = Some(ov_id);
                pw.input.axis = None;
                pw.input.scroll_slot = None;
                pw.input.arena_origin = pw
                    .overlay
                    .layers
                    .iter()
                    .find(|o| o.id == ov_id)
                    .map(|o| o.screen_pos)
                    .unwrap_or((0.0, 0.0));
            }
        }
        // Overlay pointer dispatch + caret placement — the main-tree sequence from
        // `handle_pointer_down`: Down goes to the overlay arena for its `on_ptr` handlers, and a
        // tap places the text caret from the grapheme anchor (tap-to-place).
        //
        // Focus is deliberately NOT set here. A component that wants the keyboard asks for it,
        // and the press gesture above just gave it that chance (the popup `TextField` container
        // does `on_press → FocusRequester::request_focus`), so a tap on a popup button leaves the
        // field beside it alone — the rule Compose's `Clickable` follows by never calling
        // `requestFocus` for a click. This handler used to focus the deepest focusable node on the
        // hit path (the debug-click rule, see `consume_debug_events`), which stole the keyboard
        // from a field as soon as any button in the same popup was tapped. Drag-select inside
        // overlay inputs is v1-out.
        {
            let ptr_ev = crate::input::PointerEvent {
                event_type: crate::input::PointerEventType::Down,
                position: (0.0, 0.0),
                scene_position: local,
                kind: kind.clone(),
                is_alt_pressed: pw.modifiers.alt_key(),
                is_ctrl_pressed: pw.modifiers.control_key(),
                is_shift_pressed: pw.modifiers.shift_key(),
                is_meta_pressed: pw.modifiers.meta_key(),
            };
            // Read phase (immutable): hit path + caret anchor.
            // (Root is guaranteed by hit_overlay above; the None arm only
            // satisfies the compiler and still falls through to click recording.)
            let (path, caret): (Vec<usize>, Option<(usize, usize)>) = match {
                let ov = &pw.overlay.layers[i];
                let nodes = ov.composer.arena_nodes();
                // Owned roots: the scrutinee borrow ends here, but the arm
                // below needs them (and `ov` there is the outer `&mut` binding).
                let troots: Vec<usize> = ov.composer.transition_roots().to_vec();
                match ov.composer.layout_root_idx() {
                    None => None,
                    Some(r) => Some((nodes, r, troots)),
                }
            } {
                None => (Vec::new(), None),
                Some((nodes, r, troots)) => {
                let path = hit_test_with_flights(nodes, r, &troots, local.0, local.1);
                let caret = (|| {
                    let &innermost = path.last()?;
                    let anchor_node = find_anchor_text_node(nodes, &path, innermost)?;
                    let borrow = nodes[anchor_node].cached_paragraph.try_borrow().ok()?;
                    let para = borrow.as_ref()?;
                    let (ax, ay) = node_abs_position(nodes, r, nodes[anchor_node].id);
                    let (pad_s, pad_t, pad_e, _) = nodes[anchor_node].modifier.get_padding_sides();
                    let pad_x = if nodes[anchor_node].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                    let tl = crate::text::TextLayout::new(para, 0);
                    let hit = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(local.0 - ax - pad_x, local.1 - ay - pad_t));
                    let edit = crate::text::field::offset_mapping_for_node(nodes, r, anchor_node)
                        .map(|m| m.transformed_to_original(hit))
                        .unwrap_or(hit);
                    Some((anchor_node, edit))
                })();
                (path, caret)
                }
            };
            // Pointer dispatch (immutable arena borrow, ends immediately).
            if !path.is_empty() {
                let ov = &pw.overlay.layers[i];
                if let Some(r) = ov.composer.layout_root_idx() {
                    let nodes = ov.composer.arena_nodes();
                    dispatch_ptr_event(nodes, r, &path, &ptr_ev, local, None);
                }
            }
            if let Some((aidx, aoff)) = caret {
                let nodes = pw.overlay.layers[i].composer.arena_nodes();
                if let Some(n) = nodes.get(aidx) {
                    n.cursor_index.set(aoff);
                    if let Some(cb) = n.cursor_callback.borrow_mut().as_mut() {
                        cb(aoff);
                    }
                }
            }
        }
        // (ov borrow ended at the Press block above; re-index here.)
        let nid = pw.overlay.layers[i].composer.layout_root_idx().and_then(|r| {
            let nodes = pw.overlay.layers[i].composer.arena_nodes();
            hit_test_with_flights(nodes, r, pw.overlay.layers[i].composer.transition_roots(), local.0, local.1).last().map(|&idx| nodes[idx].id)
        });
        pw.overlay.click = Some((i, local, nid.unwrap_or(0)));
        return true; // 事件消费——不进主树
    }
    // Outside press: an overlay that dismisses on an outside press closes; an overlay that does
    // not still CONSUMES the press when it is modal (its scrim blocks what is behind it).
    //
    // ⚠ `dismiss_on_outside == false` has to be honoured for a modal overlay too: `modal` alone
    // says "blocks the content behind", not "closes on any press". Compose's
    // `DialogProperties(dismissOnClickOutside = false)` and `PopupProperties.dismissOnClickOutside`
    // keep the overlay open, and callers pass the flag for exactly that — Nav3's
    // `dialogProperties` (`nav.rs`) and `AlertDialog`'s own builder. Treating `modal` as
    // "dismiss" made those flags silent no-ops.
    // ⚠ 同步移除（不等 recompose）——否则残留 overlay 会吞掉关闭后
    // 紧接着的点击（用户"点两次才打开"）且多渲染一帧（视觉闪烁）
    for i in (0..pw.overlay.layers.len()).rev() {
        // A closing overlay is on its way out: it must not be dismissed again and must not swallow
        // the press — `hit_overlay` treats it as transparent for the same reason. Without this a
        // dialog that was just closed (Escape, a button, an outside press) eats the next press for
        // the length of its exit animation, which is the "click twice to open" complaint.
        if pw.overlay.layers[i].closing {
            continue;
        }
        let dismiss = pw.overlay.layers[i].dismiss_on_outside;
        if !dismiss && !pw.overlay.layers[i].modal {
            continue;
        }
        let passthrough = pw.overlay.layers[i].click_passthrough;
        let id = pw.overlay.layers[i].id;
        if dismiss {
            // 启动退出动画（不复位——动画完成后移除；passthrough Tooltip 的
            // on_dismiss 同步置 visible=false → 组合期记录 active=false 走
            // begin_overlay_close；这里先直接触发——两者幂等（closing 防重入））
            begin_overlay_close(pw, id);
        }
        // ⚠ Tooltip（passthrough）：dismiss 后**放行主树**——点击不消费
        // （否则点按钮第一次只关 tooltip、按钮收不到——需点两次）
        if passthrough {
            continue;
        }
        return true;
    }
    false
}
/// overlay 内 hover 更新（overlay composer + 本地坐标）
pub(crate) fn overlay_update_hover(ov: &mut OverlayWindow, local: (f32, f32)) {
    let hit: Vec<(u64, crate::interaction::MutableInteractionSource)> = {
        let nodes = ov.composer.arena_nodes();
        let Some(r) = ov.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
        path.iter()
            .filter(|&&i| nodes[i].modifier.has_hoverable())
            .filter_map(|&i| nodes[i].modifier.hoverable_interaction().map(|s| (nodes[i].slot_key, s.clone())))
            .collect()
    };
    let hit_slots: std::collections::HashSet<u64> = hit.iter().map(|(s, _)| *s).collect();
    for (slot, src) in &hit {
        if ov.hovered_slots.insert(*slot) {
            src.emit_hover_enter();
        }
    }
    let gone: Vec<u64> = ov.hovered_slots.iter().copied().filter(|s| !hit_slots.contains(s)).collect();
    for slot in gone {
        ov.hovered_slots.remove(&slot);
        overlay_exit_hover_at(ov, slot);
    }
}
/// 对 overlay 内指定 slot 的节点补发 Hover Exit
pub(crate) fn overlay_exit_hover_at(ov: &mut OverlayWindow, slot: u64) {
    let nodes = ov.composer.arena_nodes();
    let Some(r) = ov.composer.layout_root_idx() else { return; };
    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
            if let Some(src) = nodes[idx].modifier.hoverable_interaction() {
                src.emit_hover_exit();
            }
        }
    }
}
// ── 顶层弹出层（Popup/Dialog/DropdownMenu） ──

impl OverlayWindow {
    pub(crate) fn new_with_composer(desc: crate::overlay::OverlayDesc, composer: Composer) -> Self {
        Self {
            id: desc.id,
            composer,
            anchor_slot: desc.anchor_slot,
            position: desc.position,
            offset: desc.offset,
            dismiss_on_back_press: desc.dismiss_on_back_press,
            anchor_slide: desc.anchor_slide,
            modal: desc.modal,
            dismiss_on_outside: desc.dismiss_on_outside,
            click_passthrough: desc.click_passthrough,
            fit_around_anchor: desc.fit_around_anchor,
            match_anchor_width: desc.match_anchor_width,
            on_dismiss: desc.on_dismiss,
            content: desc.content,
            local_snapshot: desc.local_snapshot,
            screen_pos: (0.0, 0.0),
            anchor_rect: None,
            hovered_slots: std::collections::HashSet::new(),
            pressed_interaction: None,
            // progress is driven only when an enter or exit spec exists (0->1
            // enter, 1->0 exit); otherwise None means always visible (instant).
            progress: (desc.enter_anim.is_some() || desc.exit_anim.is_some())
                .then(|| crate::runtime::state::Animating::new(0.0)),
            enter_anim: desc.enter_anim,
            exit_anim: desc.exit_anim,
            closing: false,
            closing_since: None,
            focused_id: None,
            focused_slot_key: None,
            focus_scope: desc.focus_scope,
            claimed_keyboard: false,
        }
    }

    /// Undo a close in progress: this overlay is visible again, so its show-progress goes back to 1.
    ///
    /// The exit tween drove it to 0 and nothing else brings it back — the enter animation is pushed
    /// only when an overlay is CREATED or when its animation specs flip None<->Some. Without this the
    /// overlay is drawn at alpha 0 while `hit_overlay` (which never looks at alpha) keeps swallowing
    /// clicks in its rect and `keyboard_scope` keeps granting it the keyboard: an invisible panel you
    /// cannot get rid of, reached by closing and letting the caller re-declare the overlay.
    pub(crate) fn resume_after_close(&mut self) {
        self.closing = false;
        self.closing_since = None;
        // It is the keyboard's owner again, so the page's suspended focus has to be suspended afresh
        // (the release already handed it back).
        self.claimed_keyboard = false;
        let Some(progress) = self.progress.clone() else { return };
        match &self.enter_anim {
            Some(spec) => {
                crate::animation::push_animatable_handle(progress, 1.0, spec.animation_spec());
            }
            // No enter animation to replay: the render path reads progress for the exit spec too, so
            // a stale 0 would keep it hidden — show it outright.
            None => progress.as_raw().set_backchannel(1.0),
        }
    }

    fn update(&mut self, desc: crate::overlay::OverlayDesc) {
        // A desc only reaches this method while its overlay is still DECLARED (a closing overlay is not
        // re-registered), so the caller bringing one back — open, close, open again inside the exit fade,
        // where the id is reused — has to cancel the pending close: leaving `closing` set kept the overlay
        // un-interactive (`hit_overlay` skips closing ones) and let the fade remove it a moment after it
        // was reopened, which showed as a panel flashing.
        let was_closing = self.closing;
        self.closing = false;
        self.closing_since = None;
        self.anchor_slot = desc.anchor_slot;
        self.position = desc.position;
        self.anchor_slide = desc.anchor_slide;
        self.offset = desc.offset;
        self.modal = desc.modal;
        self.focus_scope = desc.focus_scope;
        self.dismiss_on_outside = desc.dismiss_on_outside;
        self.dismiss_on_back_press = desc.dismiss_on_back_press;
        self.click_passthrough = desc.click_passthrough;
        self.fit_around_anchor = desc.fit_around_anchor;
        self.match_anchor_width = desc.match_anchor_width;
        self.on_dismiss = desc.on_dismiss;
        self.content = desc.content;
        self.local_snapshot = desc.local_snapshot;
        // Animation spec update (when a reused overlay's specs change) — rebuild
        // progress on None<->Some flip (None->Some: new driver; Some->None:
        // leftover State retired — render falls to the `_` branch, always visible,
        // and close removes instantly with has_exit=false).
        let old_progress = self.progress.clone();
        let spec_flipped = desc.enter_anim.is_some() != self.enter_anim.is_some()
            || desc.exit_anim.is_some() != self.exit_anim.is_some();
        self.enter_anim = desc.enter_anim;
        self.exit_anim = desc.exit_anim;
        if spec_flipped {
            // Cancel any in-flight animation on the old progress handle
            // (otherwise it lingers as an orphan until timeout — P1-4).
            if let Some(old) = old_progress {
                crate::animation::cancel_animation_by_id(old.state_id());
            }
            self.progress = (self.enter_anim.is_some() || self.exit_anim.is_some())
                .then(|| crate::runtime::state::Animating::new(0.0));
            // Update is the reuse path — overlay is already visible; progress=0
            // would render as apply(0) hidden. If the new spec has no enter
            // animation, keep it fully visible (Backchannel-equivalent direct
            // write on the Animating handle — same stored value); otherwise the
            // caller will push the 0->1 enter animation.
            if let Some(p) = self.progress.as_ref() {
                if self.enter_anim.is_none() {
                    p.as_raw().set_backchannel(1.0);
                }
            }
        }
        // The caller brought back an overlay that was fading out (open, close, open inside the exit):
        // the fade has to be undone, not just the flag.
        if was_closing {
            self.resume_after_close();
        }
    }
}

/// 主树 compose 后同步 overlay：按 id 匹配（保留 State）——新增/更新/删除。
/// 删除依据 = 组合期显式记录的 active=false（Popup/Dialog/DropdownMenu build
/// 总执行时 record_overlay_active）——主动关闭（visible/expanded=false）→ 删；
/// 注册方 Skip（build 未执行 → 本帧无记录）→ 保留（闪烁根因：旧 retain 把
/// Skip 帧误判为主动关闭）。
pub(crate) fn sync_overlays(pw: &mut PerWindow, _recomposed: bool) {
    let descs = pw.composer.take_overlays();
    for desc in descs {
        if let Some(ov) = pw.overlay.layers.iter_mut().find(|o| o.id == desc.id) {
            // 复用：规格翻转（None↔Some）时 update 重建 progress——新规格有
            // enter 动画则 push 0→1（否则 update 内已 Backchannel 写 1.0 保持显示）
            let had_enter = ov.enter_anim.is_some();
            ov.update(desc);
            // ⚠ The caller just recomposed and handed us a NEW content closure, so the overlay's whole
            // content has to run again. An overlay is a separate composer: nothing inside it can see
            // that its closure was replaced, and a group re-enters only for state it read or
            // parameters it declared — so content built from values the caller captured (a filtered
            // list, a formatted string) kept what the FIRST closure captured. Marking the root alone
            // is not enough: the caller's lambda usually sits inside wrapper groups of its own (a
            // `Column`, a `Surface`), which declare nothing and would skip. Measured on
            // `search_bar_demo`: the fullscreen panel and the docked dropdown both stayed on their
            // first rows while the input field updated.
            //
            // Cost is bounded by how often the caller's group runs, not by this call: a screen whose
            // main tree composes every frame re-runs the overlay content every frame too, while one
            // that idles skips both composers. Nothing leaks across frames (`Slot::dirty` is consumed
            // by the pass that reads it), a `LazyColumn` keeps its scroll position, and a closing
            // overlay is not re-registered at all.
            ov.composer.mark_content_dirty();
            let now_has_enter = ov.enter_anim.is_some();
            if !had_enter && now_has_enter {
                if let Some(p) = ov.progress.clone() {
                    let spec = ov.enter_anim.as_ref().unwrap();
                    crate::animation::push_animatable_handle(
                        p, 1.0,
                        spec.animation_spec(),
                    );
                }
            }
        } else {
            // 新 overlay：创建 + 启动进入动画（progress 0→1；无 enter 规格 =
            // 瞬时显示——但 exit 有规格时 progress 直接置 1.0，退出才能 1→0）
            // 共享主窗 AdaptiveContext，确保 overlay 读 window_size/density 与主窗一致
            let mut composer = Composer::new();
            composer.adaptive = pw.composer.adaptive.clone();
            let enter_anim = desc.enter_anim.clone();
            pw.overlay.layers.push(OverlayWindow::new_with_composer(desc, composer));
            if let Some(p) = pw.overlay.layers.last().and_then(|o| o.progress.clone()) {
                if let Some(spec) = &enter_anim {
                    crate::animation::push_animatable_handle(
                        p, 1.0,
                        spec.animation_spec(),
                    );
                } else {
                    // 无进入动画（但 exit 有）：直接完整显示（progress=1）
                    p.as_raw().set_backchannel(1.0);
                }
            }
        }
    }
    // 组合期记录 active=false 的 overlay → 主动关闭 → 启动退出动画
    // （先触发 on_dismiss；动画完成由 finish_closing_overlays 移除——不复位
    // 则保留渲染播放退出动画）
    let to_close: Vec<u64> = pw.composer.overlay_active.iter()
        .filter(|(_, active)| !**active)
        .map(|(&id, _)| id)
        .collect();
    for id in &to_close {
        begin_overlay_close(pw, *id);
    }
    // 退出动画完成检测：closing 且 progress≈0（或无动画规格）→ 真正移除
    finish_closing_overlays(pw);
}

#[cfg(test)]
mod overlay_close_tests {
    use super::{closing_overlay_is_done, CLOSING_DEADLINE};
    use super::OverlayWindow;
    use crate::app::PerWindow;
    use crate::runtime::composer::Composer;
    use crate::overlay::{OverlayAnimSpec, OverlayDesc, PopupPosition};
    use crate::theme::ThemeColors;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A closing overlay is dropped when its fade finished — and, failing that, past the deadline. The
    /// case in between is the reason the deadline exists: a tween that never reports done kept the overlay
    /// (and its modal scrim) rendering forever, which a user hit as a stuck dim layer over the page with
    /// the sheet already gone.
    #[test]
    fn a_closing_overlay_waits_for_its_fade_but_not_forever() {
        // The deadline has to stay in the same order of magnitude as the exit specs (a few hundred
        // milliseconds): the test below derives its instants from it, so a constant of minutes would pass
        // the test and still leave a stuck scrim on screen for minutes.
        assert!(
            CLOSING_DEADLINE <= Duration::from_millis(2000),
            "CLOSING_DEADLINE is a safety net, not a grace period: {CLOSING_DEADLINE:?}"
        );
        let now = Instant::now();
        assert!(closing_overlay_is_done(None, None, now), "no progress channel: nothing to wait for");
        assert!(closing_overlay_is_done(Some(0.0), None, now), "the fade reached its end");
        assert!(
            !closing_overlay_is_done(Some(1.0), Some(now), now),
            "a fade in flight is not dropped"
        );
        assert!(
            !closing_overlay_is_done(Some(1.0), Some(now - CLOSING_DEADLINE / 2), now),
            "…and not before its time"
        );
        assert!(
            closing_overlay_is_done(Some(1.0), Some(now - CLOSING_DEADLINE - Duration::from_millis(1)), now),
            "…and it is dropped once the deadline passes"
        );
    }

    /// An overlay that stops closing has to be VISIBLE again, not merely non-closing: the exit tween
    /// left its show-progress at 0, and that number is what the render path draws with (while the hit
    /// test never looks at it). Left there, the overlay is an invisible panel that still swallows
    /// clicks in its rect and still owns the keyboard — the state a close-then-reopen used to land in,
    /// and the state the deadline's "hand it back its interactivity" arm would have landed in too.
    #[test]
    fn an_overlay_that_stops_closing_is_shown_again() {
        let desc = |enter: Option<OverlayAnimSpec>, exit: Option<OverlayAnimSpec>| OverlayDesc {
            id: 7,
            anchor_slot: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            click_passthrough: false,
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: None,
            enter_anim: enter,
            exit_anim: exit,
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };
        let fade = || OverlayAnimSpec::fade_only(Duration::from_millis(50));

        // Nothing to animate back up (no enter spec): the value must land on 1 outright.
        let mut ov = OverlayWindow::new_with_composer(desc(None, Some(fade())), Composer::new());
        let progress = ov.progress.clone().expect("the exit spec gives it a progress channel");
        progress.as_raw().set_backchannel(0.0);
        ov.closing = true;
        ov.closing_since = Some(Instant::now());
        ov.resume_after_close();
        assert!(!ov.closing, "resuming undoes the close");
        assert_eq!(
            ov.progress.as_ref().map(|p| p.peek()),
            Some(1.0),
            "with no enter animation the panel is shown outright, not left at the exit tween's 0"
        );

        // With one, the 0->1 enter is scheduled again — the push is the observable proof.
        let _serial = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut ov = OverlayWindow::new_with_composer(desc(Some(fade()), Some(fade())), Composer::new());
        let progress = ov.progress.clone().expect("an animated overlay has a progress channel");
        progress.as_raw().set_backchannel(0.0);
        ov.closing = true;
        ov.closing_since = Some(Instant::now());
        ov.resume_after_close();
        assert!(
            crate::animation::has_animation_for_state(progress.state_id()),
            "resuming an overlay that was fading out schedules its enter animation again"
        );
        crate::animation::clear_all_animations();
    }

    /// `DialogProperties.dismissOnBackPress = false` still SWALLOWS Escape — the page behind the scrim
    /// must not react to a key the dialog kept — but does not close the dialog. That swallow is winia's
    /// own, not Compose's: see the note on [`PerWindow::escape_key`].
    #[test]
    fn escape_respects_dismiss_on_back_press() {
        let desc = |dismiss_on_back_press: bool| OverlayDesc {
            id: 9,
            anchor_slot: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press,
            click_passthrough: false,
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: Some(Arc::new(|| {})),
            enter_anim: None,
            exit_anim: Some(OverlayAnimSpec::fade_only(Duration::from_millis(50))),
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };

        // True (the default): the escape starts the close.
        let light = ThemeColors::default_light();
        let mut kept = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        kept.overlay.layers
            .push(OverlayWindow::new_with_composer(desc(true), Composer::new()));
        assert!(kept.escape_key(), "escape is consumed");
        assert!(
            kept.overlay.layers[0].closing,
            "and it closes the dialog that opted in"
        );

        // False: consumed, but nothing closes.
        let light = ThemeColors::default_light();
        let mut stubborn = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        stubborn
            .overlay
            .layers
            .push(OverlayWindow::new_with_composer(desc(false), Composer::new()));
        assert!(
            stubborn.escape_key(),
            "escape is still SWALLOWED so the page behind does not see it"
        );
        assert!(
            !stubborn.overlay.layers[0].closing,
            "but the dialog that opted out stays open"
        );
    }
}
