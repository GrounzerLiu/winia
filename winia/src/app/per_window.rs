use super::*;

// ── PerWindow ──

pub(crate) struct PerWindow {
    pub(crate) composer: Composer,
    pub(crate) skia_window: Option<SkiaWindow>,
    pub(crate) width: f32, pub(crate) height: f32,
    pub(crate) scale_factor: f64,
    pub(crate) focused_id: Option<u64>,
    pub(crate) content: Box<dyn Fn(&mut ComposeCtx)>,
    /// Window-size state, split by scheduling semantic (see docs/state-handles.md):
    /// - `window_size_state`: Reactive — resize path `set()` notifies, driving
    ///   dependents (suite scaffolds reading `window_size()`) dirty for recompose.
    /// - `window_size_backchannel`: per-frame sync without notify (replaces the
    ///   old silent write on the shared State — same value, Backchannel write).
    pub(crate) window_size_state: std::cell::RefCell<Option<crate::runtime::state::State<(f32, f32)>>>,
    pub(crate) window_size_backchannel: std::cell::RefCell<Option<crate::runtime::state::Backchannel<(f32, f32)>>>,
    pub(crate) on_close: Option<Box<dyn FnMut() + Send>>,
    pub(crate) created_id: Option<u64>,
    /// Stable key this window saves its saveable state under (see `PendingWindow::saveable_key`).
    pub(crate) saveable_key: Option<u64>,

    /// 焦点节点的 slot_key
    pub(crate) focused_slot_key: Option<u64>,
    /// 当前窗口修饰键状态（ModifiersChanged 按 WindowId 维护）
    pub(crate) modifiers: winit::keyboard::ModifiersState,
    /// 当前悬停节点的 slot_key（指针移入/移出时发射 Hover Enter/Exit）
    /// 当前 hover 的 hoverable 节点 slot 集合（**支持嵌套**——Tooltip 锚点
    /// 容器与内部 Button 等可同时 hover；修复前只存最内层 → 嵌套 hoverable
    /// 外层收不到 Enter（Tooltip 锚点挂 hoverable 时内部 Button 抢走事件））
    pub(crate) hovered_slots: std::collections::HashSet<u64>,
    /// The cursor currently applied to the window — `set_cursor` is a platform call, so it is
    /// only made when the hovered node asks for something different.
    pub(crate) cursor_icon: crate::input::PointerIcon,
    /// 当前按下交互（clickable 绑定源 + 按下节点 slot——Up/越界 slop 时释放）
    pub(crate) pressed_interaction: Option<(u64, crate::interaction::MutableInteractionSource)>,
    /// 已发射 Focus 的节点 slot（focus 变化时对旧节点补发 Unfocus）
    pub(crate) focused_interaction_slot: Option<u64>,
    /// What this window draws with, and what it has already drawn with (see `WindowThemeState`).
    pub(crate) theme: WindowThemeState,
    /// The frame's clock: when it last rendered and last asked to, how fast it should be
    /// going, and whether it has given up (see `FrameClock`).
    pub(crate) clock: FrameClock,
    /// The pointer and gesture state: what is down, what a gesture locked onto, and the
    /// sessions it opened (see `GestureState` — fourteen fields that used to be here).
    pub(crate) input: GestureState,
    /// The wheel's page-step gate: how much travel the current gesture has banked toward the next
    /// page, per window (see `WheelPager`).
    pub(crate) wheel: WheelPager,
    /// The overlay host: the layers, and the interaction state that only makes sense against
    /// them (see `OverlayHost` — the group that used to be ten fields here).
    pub(crate) overlay: OverlayHost,
}


impl PerWindow {
    pub(crate) fn new(content: Box<dyn Fn(&mut ComposeCtx)>, width: f32, height: f32, theme: crate::theme::ThemeColors) -> Self {
        // A window built without a `Window` node: its palette is whatever the caller passed, so the intent
        // is that palette (nothing to follow), and its type scale is the default.
        let theme = WindowThemeState::from_theme(theme);
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content, window_size_state: std::cell::RefCell::new(None), window_size_backchannel: std::cell::RefCell::new(None), on_close: None, created_id: None, saveable_key: None, theme, focused_slot_key: None, input: GestureState::default(), wheel: WheelPager::default(), overlay: OverlayHost::default(), clock: FrameClock::default(), modifiers: Default::default(), hovered_slots: std::collections::HashSet::new(), cursor_icon: crate::input::PointerIcon::DEFAULT, pressed_interaction: None, focused_interaction_slot: None }
    }
    pub(crate) fn created_id(&self) -> Option<u64> { self.created_id }

    /// 焦点同步：focused_slot_key 变化时，对旧焦点节点补发 Unfocus、
    /// 新焦点节点补发 Focus（布局后调用——焦点标志已随重组刷新）。
    pub(crate) fn sync_focus_interaction(&mut self) {        let new_slot = self.focused_slot_key;
        if new_slot == self.focused_interaction_slot {
            return;
        }
        if let Some(old) = self.focused_interaction_slot.take() {
            let nodes = self.composer.arena_nodes();
            if let Some(r) = self.composer.layout_root_idx() {
                if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, old) {
                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                        if let Some(src) = nodes[idx].modifier.focusable_interaction() {
                            src.emit_unfocus();
                        }
                    }
                }
            }
        }
        if let Some(new) = new_slot {
            let nodes = self.composer.arena_nodes();
            if let Some(r) = self.composer.layout_root_idx() {
                if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, new) {
                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                        if let Some(src) = nodes[idx].modifier.focusable_interaction() {
                            src.emit_focus();
                            self.focused_interaction_slot = Some(new);
                        }
                    }
                }
            }
        }
    }

    /// Overlay focus interaction sync (mirrors sync_focus_interaction against the
    /// overlay arenas): without it, overlay inputs never report is_focused() —
    /// no cursor, no blink, no focused colors. Tracks (overlay id, slot); called
    /// after layout_overlays (slot resolution needs overlay layout done).
    /// Deliver `on_focus_changed` callbacks after this frame's focus sync (main tree + overlays).
    pub(crate) fn dispatch_focus_changed(&mut self) {
        if let Some(root) = self.composer.layout_root_idx() {
            crate::layout::node::dispatch_focus_changed(self.composer.arena_nodes_mut(), root);
        }
        for ov in self.overlay.layers.iter_mut() {
            if let Some(root) = ov.composer.layout_root_idx() {
                crate::layout::node::dispatch_focus_changed(ov.composer.arena_nodes_mut(), root);
            }
        }
    }

    pub(crate) fn sync_overlay_focus_interaction(&mut self) {
        let cur: Option<(u64, u64)> = (0..self.overlay.layers.len()).rev().find_map(|i| {
            let ov = &self.overlay.layers[i];
            if ov.closing {
                return None;
            }
            ov.focused_slot_key.map(|s| (ov.id, s))
        });
        if cur == self.overlay.focused_interaction {
            return;
        }
        if let Some((old_id, old_slot)) = self.overlay.focused_interaction.take() {
            if let Some(ov) = self.overlay.layers.iter().find(|o| o.id == old_id) {
                let nodes = ov.composer.arena_nodes();
                if let Some(r) = ov.composer.layout_root_idx() {
                    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, old_slot) {
                        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                            if let Some(src) = nodes[idx].modifier.focusable_interaction() {
                                src.emit_unfocus();
                            }
                        }
                    }
                }
            }
        }
        if let Some((id, slot)) = cur {
            if let Some(ov) = self.overlay.layers.iter().find(|o| o.id == id) {
                let nodes = ov.composer.arena_nodes();
                if let Some(r) = ov.composer.layout_root_idx() {
                    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
                        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                            if let Some(src) = nodes[idx].modifier.focusable_interaction() {
                                src.emit_focus();
                                self.overlay.focused_interaction = Some((id, slot));
                            }
                        }
                    }
                }
            }
        }
    }

    /// 按指定焦点节点同步 IME 开关（Tab/方向键/Escape/焦点请求共用）——
    /// 框架只做机械转发：节点声明了 ime_callback 才开启，否则关闭。
    pub(crate) fn apply_ime_for_focus(&self, fid: Option<u64>) {
        self.apply_ime_for_focus_in(None, fid);
    }

    /// IME for a node in a SPECIFIC arena (`None` = the main tree).
    ///
    /// The arena has to be named: node ids are unique across arenas, so an overlay node id resolved
    /// against the main arena finds nothing and yields "no IME" — measured as the arrow keys turning
    /// the IME off while moving focus inside a dialog's text field.
    pub(crate) fn apply_ime_for_focus_in(&self, overlay: Option<usize>, fid: Option<u64>) {
        let root = match overlay {
            Some(i) => self.overlay.layers.get(i).and_then(|ov| ov.composer.layout_root_idx()),
            None => self.composer.layout_root_idx(),
        };
        let wants_ime = match (root, fid) {
            (Some(r), Some(fid)) => {
                let nodes = match overlay {
                    Some(i) => self.overlay.layers[i].composer.arena_nodes(),
                    None => self.composer.arena_nodes(),
                };
                crate::layout::node::find_node_by_id(nodes, r, fid)
                    .map(|idx| node_or_descendant_wants_ime(nodes, idx))
                    .unwrap_or(false)
            }
            _ => false,
        };
        if let Some(ref sw) = self.skia_window {
            set_ime_enabled(&sw, wants_ime);
        }
    }

    /// 重新查询窗口所在显示器的刷新率并更新帧间隔（跨屏跟随）。
    /// Moved/ScaleFactorChanged 高频触发——300ms 去抖；查询失败保留旧值
    /// （避免瞬时失败把高刷错误降级成 60fps）。
    pub(crate) fn refresh_frame_interval(&mut self) {
        const RECHECK_MIN: std::time::Duration = std::time::Duration::from_millis(300);
        let now = std::time::Instant::now();
        if now.duration_since(self.clock.last_refresh_check) < RECHECK_MIN {
            return;
        }
        self.clock.last_refresh_check = now;
        let Some(ref sw) = self.skia_window else { return; };
        let Some(interval) = current_frame_interval(sw) else { return; };
        if interval != self.clock.frame_interval {
            debug_log!("[refresh] frame_interval {:?} -> {:?}", self.clock.frame_interval, interval);
            self.clock.frame_interval = interval;
            if let Some(ref sw) = self.skia_window { sw.request_redraw(); }
        }
    }

    /// 补发延迟的 tap（Compose：onDoubleTap 存在时 onTap 延迟到双击窗口结束）。
    /// 超时或按下其他节点时调用——按当前布局树换算组件本地坐标。
    pub(crate) fn fire_pending_tap(&mut self, t: crate::input::gesture::PendingTap) {
        let action = crate::input::gesture::GestureAction::Tap(t.pos);
        // A deferred tap belongs to the arena its node was pressed in (main tree or that popup's
        // layer), and its `pos` is layer-local for a popup — the space the tracker recorded it in.
        match t.overlay_id {
            None => {
                let nodes = self.composer.arena_nodes();
                let Some(r) = self.composer.layout_root_idx() else { return; };
                fire_gesture_action(nodes, r, t.slot_key, action);
            }
            Some(id) => {
                let Some(i) = self.overlay.layers.iter().position(|o| o.id == id) else { return; };
                let ov = &self.overlay.layers[i];
                let nodes = ov.composer.arena_nodes();
                let Some(r) = ov.composer.layout_root_idx() else { return; };
                fire_gesture_action(nodes, r, t.slot_key, action);
            }
        }
        if let Some(ref sw) = self.skia_window { sw.request_redraw(); }
    }


    /// Escape: close the topmost overlay, or — with no overlay open — drop the page's focus.
    ///
    /// The one implementation, called by the real `KeyboardInput` path and by the debug server's
    /// `k Escape`: this branch used to exist twice, and the copy kept the old "always clear focus"
    /// rule, so a keyboard round trip driven over the debug server measured behaviour the framework
    /// no longer had.
    ///
    /// Closing an overlay does NOT touch the page's focus: the page keeps it while the overlay owns
    /// the keyboard (see `claim_keyboard_for_overlay`), so a keyboard user who opened a panel and
    /// dismissed it is still where they were instead of having to tab back. Clearing the focus is
    /// what Escape does when there is nothing to close (the collapsed field loses its ring).
    pub(crate) fn escape_key(&mut self) -> bool {
        if let Some(i) = self.overlay.layers.iter().rposition(|o| !o.closing) {
            // A dismissal is a REQUEST: the component hides itself in `on_dismiss` and flips the
            // caller's `visible`. With no handler nobody can complete it, and starting one anyway
            // faded the overlay out only to have the deadline hand it back — the key is left to the
            // overlay's own handlers instead (a component that wants Escape can still see it).
            if self.overlay.layers[i].on_dismiss.is_none() {
                return false;
            }
            // `dismissOnBackPress = false` SWALLOWS the key. This is a deliberate divergence, and the
            // reason is structural rather than a quote: Compose runs a dialog in its OWN window, so
            // with the flag false its `onKeyUp` simply falls through to `super` (`BasicEdgeToEdge
            // Dialog.android.kt:227,235`) and there is no page behind to reach anyway. winia draws the
            // dialog as an overlay over the live page, so letting the key through would hand an Escape
            // the dialog deliberately kept to the page under its scrim — where it would clear that
            // page's focus. (Note that `AlertDialog` does not even go through
            // `BasicEdgeToEdgeDialog`: it reaches `androidx.compose.ui.window.Dialog` via
            // `DefaultBasicAlertDialogOverride`, `AlertDialog.kt:165-172`.)
            //
            // Ordering matters and is not an accident: with no `on_dismiss_request` there is nothing
            // for a dismissal to ask, so the handler check above wins and the key is left alone even
            // when this flag is off.
            if !self.overlay.layers[i].dismiss_on_back_press {
                return true;
            }
            let id = self.overlay.layers[i].id;
            // A content that owns its exit gets the request and nothing else: the bottom sheet slides down
            // first and only then flips the caller's `visible`, which stops composing the overlay and
            // removes it. Starting the host's own exit fade here made the panel fade out where it stood
            // (the overlap the bottom sheet's module doc recorded) — Compose has no flag for this because
            // `onDismissRequest` is always the caller's to complete.
            if self.overlay.layers[i].dismiss_defers_to_content {
                if let Some(cb) = self.overlay.layers[i].on_dismiss.clone() {
                    cb();
                }
                return true;
            }
            begin_overlay_close(self, id);
            return true;
        }
        if self.focused_id.is_some() {
            if let Some(r) = self.composer.layout_root_idx() {
                crate::layout::node::clear_focus(self.composer.arena_nodes_mut(), r);
            }
            self.focused_id = None;
            self.focused_slot_key = None;
            // Escape 清焦后关闭输入法（避免 IME 残留开启）
            self.apply_ime_for_focus(None);
            return true;
        }
        false
    }

    /// Tab / Shift+Tab: move focus one step inside whichever arena owns the keyboard.
    ///
    /// The arena is chosen by [`keyboard_scope`] — an overlay that already holds focus keeps it, else
    /// the topmost overlay that declares itself a focus scope takes the key, else the main tree. This
    /// is the ONE implementation: the real `KeyboardInput` path and the debug server's `k Tab` both
    /// call it, because a second copy of the rule silently goes stale (it did: the debug copy kept
    /// walking the main tree after this rule changed, so a UI test driving Tab over the debug server
    /// measured the copy, not the framework).
    /// Restore the focus that `focused_slot_key` identifies after a recomposition.
    ///
    /// A node that can no longer take focus (or disappeared) must also have the arena flags cleared:
    /// setting only `focused_id`/the slot to None leaves the tree with a focus ring and a
    /// semantics "focused" node that the window does not know about.
    pub(crate) fn restore_focus_by_slot_key(&mut self) {
        let Some(slot_key) = self.focused_slot_key else { return };
        let Some(r) = self.composer.layout_root_idx() else { return };
        let nodes = self.composer.arena_nodes_mut();
        let restored = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot_key)
            .map(|new_id| (new_id, crate::layout::node::focus_node_by_id(nodes, r, new_id)));
        match restored {
            Some((new_id, true)) => {
                self.focused_id = Some(new_id);
                debug_log!("[focus] restored by slot_key id={}", new_id);
            }
            _ => {
                crate::layout::node::clear_focus(nodes, r);
                self.focused_id = None;
                self.focused_slot_key = None;
            }
        }
    }

    pub(crate) fn tab_move_focus(&mut self, shift: bool) {
        if let Some(i) = keyboard_scope(self) {
            let (new_id, new_slot, want_ime) = {
                let ov = &mut self.overlay.layers[i];
                match ov.composer.layout_root_idx() {
                    Some(r) => {
                        let nodes = ov.composer.arena_nodes_mut();
                        if shift { focus_prev(nodes, r); } else { focus_next(nodes, r); }
                        let fid = crate::layout::node::get_focus_id(nodes, r);
                        let (slot, want) = fid
                            .and_then(|f| crate::layout::node::find_node_by_id(nodes, r, f))
                            .map(|idx| (nodes[idx].slot_key, node_or_descendant_wants_ime(nodes, idx)))
                            .unwrap_or((0, false));
                        (fid, fid.map(|_| slot), want)
                    }
                    None => (None, None, false),
                }
            };
            self.overlay.layers[i].focused_id = new_id;
            self.overlay.layers[i].focused_slot_key = new_slot;
            if let Some(ref sw) = self.skia_window {
                set_ime_enabled(&sw, want_ime);
            }
        } else {
            let (new_id, new_slot) = self.composer.layout_root_idx().map(|r| {
                let nodes = self.composer.arena_nodes_mut();
                if shift { focus_prev(nodes, r); } else { focus_next(nodes, r); }
                let id = crate::layout::node::get_focus_id(nodes, r);
                let slot = id.and_then(|fid| crate::layout::node::find_node_by_id(nodes, r, fid).map(|idx| nodes[idx].slot_key));
                (id, slot)
            }).unwrap_or((None, None));
            self.focused_id = new_id;
            self.focused_slot_key = new_slot;
            self.apply_ime_for_focus(new_id);
        }
    }

    /// Focus one node id from UIA / any non-keyboard entry point: same bookkeeping as Tab (the
    /// single-focus invariant, `focused_id`/`focused_slot_key` and IME), and the target's arena is
    /// resolved (main first, then overlays topmost-first) instead of always assuming the page.
    pub(crate) fn focus_by_node_id(&mut self, node_id: u64) -> bool {
        if let Some(root) = self.composer.layout_root_idx() {
            let (ok, slot) = {
                let nodes = self.composer.arena_nodes_mut();
                let ok = crate::layout::node::focus_node_by_id(nodes, root, node_id);
                let slot = if ok {
                    crate::layout::node::find_node_by_id(nodes, root, node_id)
                        .map(|idx| nodes[idx].slot_key)
                } else {
                    None
                };
                (ok, slot)
            };
            if ok {
                self.focused_id = Some(node_id);
                self.focused_slot_key = slot;
                for ov in self.overlay.layers.iter_mut() {
                    if ov.focused_id.is_some() {
                        if let Some(r) = ov.composer.layout_root_idx() {
                            crate::layout::node::clear_focus(ov.composer.arena_nodes_mut(), r);
                        }
                        ov.focused_id = None;
                        ov.focused_slot_key = None;
                    }
                }
                self.apply_ime_for_focus(Some(node_id));
                return true;
            }
        }
        for i in (0..self.overlay.layers.len()).rev() {
            let Some(root) = self.overlay.layers[i].composer.layout_root_idx() else {
                continue;
            };
            let (ok, slot, want_ime) = {
                let ov = &mut self.overlay.layers[i];
                let nodes = ov.composer.arena_nodes_mut();
                let ok = crate::layout::node::focus_node_by_id(nodes, root, node_id);
                let idx = if ok {
                    crate::layout::node::find_node_by_id(nodes, root, node_id)
                } else {
                    None
                };
                let slot = idx.map(|j| nodes[j].slot_key);
                let want = idx
                    .map(|j| node_or_descendant_wants_ime(nodes, j))
                    .unwrap_or(false);
                (ok, slot, want)
            };
            if ok {
                self.overlay.layers[i].focused_id = Some(node_id);
                self.overlay.layers[i].focused_slot_key = slot;
                if let Some(r) = self.composer.layout_root_idx() {
                    crate::layout::node::clear_focus(self.composer.arena_nodes_mut(), r);
                }
                self.focused_id = None;
                self.focused_slot_key = None;
                for j in 0..self.overlay.layers.len() {
                    if j == i || self.overlay.layers[j].focused_id.is_none() {
                        continue;
                    }
                    if let Some(r) = self.overlay.layers[j].composer.layout_root_idx() {
                        crate::layout::node::clear_focus(
                            self.overlay.layers[j].composer.arena_nodes_mut(),
                            r,
                        );
                    }
                    self.overlay.layers[j].focused_id = None;
                    self.overlay.layers[j].focused_slot_key = None;
                }
                if let Some(ref sw) = self.skia_window {
                    set_ime_enabled(sw, want_ime);
                }
                return true;
            }
        }
        false
    }

    /// 方向键移动焦点（对标 Compose Desktop 方向导航）——
    /// 候选 = 目标方向半平面内的可聚焦节点，得分 = 方向距离 + 垂直偏离×2
    ///
    /// The arena is the same one Tab uses ([`keyboard_scope`]): while an overlay owns the keyboard —
    /// a modal one, or any overlay something inside was clicked into — the arrows move inside it too.
    /// Letting them walk the page behind a dialog would be the same focus leak Tab had.
    pub(crate) fn focus_move_directional(&mut self, dir: FocusDir) -> bool {
        let overlay = keyboard_scope(self);
        let (current, root) = match overlay {
            Some(i) => (self.overlay.layers[i].focused_id, self.overlay.layers[i].composer.layout_root_idx()),
            None => (self.focused_id, self.composer.layout_root_idx()),
        };
        let (Some(fid), Some(r)) = (current, root) else { return false };

        let target = {
            let nodes = match overlay {
                Some(i) => self.overlay.layers[i].composer.arena_nodes(),
                None => self.composer.arena_nodes(),
            };
            let candidates = crate::layout::node::collect_focus_candidates(nodes, r);
            let Some((_, cur_cx, cur_cy)) = candidates.iter().find(|(id, _, _)| *id == fid) else {
                return false;
            };
            let mut best: Option<(f32, u64)> = None;
            for &(id, cx, cy) in &candidates {
                if id == fid { continue; }
                let (dx, dy) = (cx - cur_cx, cy - cur_cy);
                let in_dir = match dir {
                    FocusDir::Right => dx > 0.0,
                    FocusDir::Left => dx < 0.0,
                    FocusDir::Down => dy > 0.0,
                    FocusDir::Up => dy < 0.0,
                };
                if !in_dir { continue; }
                let (main, cross) = match dir {
                    FocusDir::Right | FocusDir::Left => (dx.abs(), dy.abs()),
                    FocusDir::Down | FocusDir::Up => (dy.abs(), dx.abs()),
                };
                let score = main + cross * 2.0;
                if best.map_or(true, |(s, _)| score < s) {
                    best = Some((score, id));
                }
            }
            best.map(|(_, id)| id)
        };
        let Some(target) = target else { return false };

        let (nodes, root) = match overlay {
            Some(i) => (self.overlay.layers[i].composer.arena_nodes_mut(), r),
            None => (self.composer.arena_nodes_mut(), r),
        };
        crate::layout::node::clear_focus(nodes, root);
        if crate::layout::node::set_focus_by_id(nodes, root, target) {
            let slot = crate::layout::node::find_node_by_id(nodes, root, target)
                .map(|idx| nodes[idx].slot_key);
            match overlay {
                Some(i) => {
                    self.overlay.layers[i].focused_id = Some(target);
                    self.overlay.layers[i].focused_slot_key = slot;
                }
                None => {
                    self.focused_id = Some(target);
                    self.focused_slot_key = slot;
                }
            }
            // IME 按组件声明（方向键聚焦文本组件时开启输入法）—— resolved in the arena the focus
            // actually moved in (see `apply_ime_for_focus_in`).
            self.apply_ime_for_focus_in(overlay, Some(target));
            return true;
        }
        false
    }

    /// Compose until nothing is left to compose, restoring focus across each pass. Returns whether any
    /// pass actually composed.
    ///
    /// The loop exists because a notification can arrive DURING composition (a tokio task's state write,
    /// an animation tick), and the frame has to consume those rather than lose them. Extracted from
    /// `recompose_layout_render` so the same handler can run it again after `layout()` — see the
    /// same-frame convergence there.
    pub(crate) fn recompose_until_stable(&mut self) -> bool {
        let mut any_composed = false;
        loop {
            let did_compose = self.composer.recompose(|ctx| {
                // Window-size states: Reactive (notifying, for dependents) +
                // Backchannel (per-frame sync without notify). First frame
                // creates both via remember under a stable ctx.key.
                let slot = &self.window_size_state;
                let bslot = &self.window_size_backchannel;
                let existing = slot.borrow().clone();
                let size_state = existing.unwrap_or_else(|| {
                    ctx.key("winia_window_size_state", |ctx| {
                        let s = ctx.remember(|| (self.width, self.height));
                        *slot.borrow_mut() = Some(s.clone());
                        *bslot.borrow_mut() = Some(s.as_backchannel());
                        s
                    })
                });
                if let Some(b) = bslot.borrow().as_ref() {
                    crate::layout::adaptive::sync_window_size_state(b, (self.width, self.height));
                }
                crate::layout::adaptive::set_window_size_state(size_state);
                (self.content)(ctx);
            });
            any_composed |= did_compose;
            self.restore_focus_by_slot_key();
            // 如果在 compose 期间又有新 notify 入队，需要再处理一次
            if !did_compose && !self.composer.has_pending_states() {
                break;
            }
        }
        any_composed
    }

    /// 增量重组 → 恢复焦点 → 布局 → 渲染（供 RedrawRequested 使用）
    /// 循环消费 notify 队列直到稳定，避免 tokio task 的并发通知丢失。
    pub(crate) fn recompose_layout_render(&mut self, window_id: WindowId, after_draw: impl FnOnce(&[LayoutNode], usize, &mut skia_safe::Surface)) {
        let _focus_window = self.composer.focus_window(window_id.into_raw() as u64);
        // 帧时钟按窗口分发：ComposeCtx::frame_clock() 用它把等待者绑到本窗口的帧上。
        self.composer.set_window_id(window_id.into_raw() as u64);
        // vsync 研究：渲染帧计数（每秒渲染次数——Fifo 下应 ~60）
        self.clock.frame_counter += 1;
        debug_log!("[fps] render#{} compose#{} pending={}", self.clock.frame_counter, self.composer.compose_count(), self.composer.pending_state_count());
        // anim-trace frame barrier: stamps the frame number and a wall-clock timestamp, samples every
        // scene published this frame, and flushes the previous frame's records.
        //
        // With `anim-trace` off this is a no-op. With it ON it always runs — the ring that serves the
        // debug server's `tr` command is always fed — and only the FILE sink is gated on WINIA_ANIM_TRACE,
        // so a build with the feature and no env var records into memory and writes nothing.
        crate::anim_trace::begin_frame(
            self.clock.frame_counter,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
        );
        // 临时：窗口节点数（诊断主窗口塌缩）
        // 提供当前窗口 Density（从 scale_factor）——覆盖 compose + layout + draw 全程，
        // 保证 Dimension::Px / TextUnit::Px 在布局/渲染期使用窗口 sf 而非 standard(1.0)
        let density = crate::unit::Density::from_density(self.scale_factor as f32);
        crate::runtime::density::with_density(density, || {
        // 更新当前 Composer 的 adaptive context；不再覆盖 thread-local singleton。
        self.composer.set_adaptive_window_size(self.width, self.height);
        // 循环 compose 直到没有新的 pending state——处理并发 task 在 compose 期间
        // 完成的 case（第二个 notify 的 state 在第一次 compose 之后才入队）
        // The theme is brought up to date HERE, immediately before the frame's recomposition: the marking
        // it does is what makes the next `recompose` re-run the content closure, so it has to land in the
        // same call as the compose. Doing it from the render handler instead consumed the change on a
        // frame whose compose had already happened, and the tree kept the old colors.
        self.refresh_theme();
        // Start the frame with no stale request: a request raised on a frame whose convergence hit its
        // pass cap would otherwise be consumed by this frame's loop BEFORE its own layout ran, costing a
        // compose the frame did not need.
        let _ = crate::runtime::composer::take_compose_after_layout();
        // 循环 compose 直到没有新的 pending state
        let mut any_composed = self.recompose_until_stable();
        self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
        // A layout pass can find that this frame's composition cannot fill the space it was measured in:
        // `LazyColumn` composes its window in `build` from the viewport the PREVIOUS measure wrote, so on
        // the frame the viewport grows the window is short and the new bottom is empty until the next
        // frame. Only a compose can add the items, so the policy that noticed asks for one — measured on
        // a viewport growing 400 -> 2500: the resize frame's bottom was empty, and covered once this loop
        // consumes the request (`docs/lazy-column.md`).
        //
        // The trigger is the REQUEST, not "composition is pending": a measure also writes back derived
        // values (the anchor), which are pending on nearly every scrolling frame, and converging on those
        // doubled the frame's compose+layout while scrolling (measured: 2 passes on 4 of 5 scroll
        // frames). The cap keeps a policy that asks on every pass from spinning the frame.
        let mut frame_passes: u8 = 1;
        for _ in 0..8 {
            if !crate::runtime::composer::take_compose_after_layout() {
                break;
            }
            any_composed |= self.recompose_until_stable();
            self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
            frame_passes += 1;
        }
        // Publish how many compose+layout rounds THIS frame took: the difference between one frame that
        // caught up with its own measurement (2) and the frame-by-frame lag it replaced (two frames of 1
        // each) is invisible in the tree, and it is exactly the property the convergence exists for.
        crate::debug::update_frame_passes(window_id.into_raw() as u64, frame_passes);
        // Shared-element flights (Phase 2): fill ends, start flights, rewrite
        // per-frame visual snapshots, reap completed flights. Render reads
        // plain f32 snapshots — never subscribes (zero-recomposition rule).
        self.composer.poll_shared_flights();
        // 焦点交互同步（Focus/Unfocus 发射——focus 标志已随重组刷新）
        self.sync_focus_interaction();

        // 顶层弹出层：同步（按 id 匹配保留 State）+ compose + layout + 定位
        // ⚠ recomposed 标志：recompose 跳过的帧（无 pending State 变化）不能执行
        // retain 删除——take_overlays 为空会误删仍开着的 overlay（dialog_open 未变），
        // 导致 overlay 消失但状态残留 → 下次点击 toggle 错乱（"点两次才开"）
        sync_overlays(self, any_composed);
        layout_overlays(self);
        // A modal overlay owns the keyboard from the frame it appears on and until it closes. The
        // arenas have to exist first, which is why this runs after `layout_overlays`.
        claim_keyboard_for_overlay(self);
        // Overlay focus interaction sync (needs overlay layout for slot resolve).
        self.sync_overlay_focus_interaction();
        self.dispatch_focus_changed();
        // Cross-composer Tier1 flights (Phase 4): match stashed sources with
        // overlay counterparts, drive active Tier1, free unmatched stashes.
        // Main composer first by convention (Tier1 flights live in its map).
        {
            let mut all: Vec<&mut crate::runtime::composer::Composer> = Vec::with_capacity(1 + self.overlay.layers.len());
            all.push(&mut self.composer);
            for ov in self.overlay.layers.iter_mut() {
                all.push(&mut ov.composer);
            }
            crate::runtime::composer::Composer::poll_cross_flights(&mut all);
        }
        // A flight that attached its layout override for the FIRST time this frame left the
        // entering end's parent stale WITHIN the frame: the override can only be attached by
        // the polls above — resolving a flight needs the target's measured rect — so the
        // layout that ran before them reported the target's NATURAL size to the parent, and
        // the rows below a starting hero dipped and snapped back on the next frame (seen in
        // `shared_transition_image_demo`). One extra pass per composer, taken once, makes the
        // frame coherent; this is winia's stand-in for Compose's lookahead pass, and it costs
        // a second measurement only on flight-start frames (everything else folds).
        if self.composer.take_layout_override_fresh() {
            self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
        }
        for ov in self.overlay.layers.iter_mut() {
            if ov.composer.take_layout_override_fresh() {
                ov.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
            }
        }

        let bg = self.theme.applied.colors.background;

        if let Some(root_idx) = self.composer.layout_root_idx() {
            let nodes = self.composer.arena_nodes();
            if let Some(ref mut sw) = self.skia_window {
                let sf = self.scale_factor as f32;
                let debug_window_id = window_id.into_raw() as u64;
                if crate::debug::screenshot_requested(debug_window_id) {
                    request_capture();
                }
                sw.draw(|surface| {
                    // Draw order: main tree → overlays → main transition layer.
                    // Overlays always sit above the tree (an open dialog must
                    // not be covered by the base screen, in either mode), and
                    // while a cross-composer flight runs the main layer lands
                    // above both so the ghost flies over modal scrims
                    // (Compose zIndexInOverlay).
                    // NOTE: `render_overlays` expects the DEVICE context — it
                    // applies `scale` itself — so it must not run inside the
                    // tree's `canvas.scale(sf)`, or a HiDPI window would draw
                    // every overlay at sf² and at an sf²-offset origin.
                    let cross_active = self.composer.has_cross_flights();
                    let canvas = surface.canvas();
                    canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
                    canvas.save();
                    canvas.scale((sf, sf));
                    render::render(nodes, root_idx, canvas);
                    if !cross_active {
                        // Shared-element transition layer: detached sources AND
                        // elevated (render_in_overlay) endpoints, drawn rootless
                        // at their absolute positions, so they escape every
                        // ancestor clip / layer transform the tree walk is
                        // still under.
                        self.composer.render_layer(canvas);
                    }
                    canvas.restore();
                    render_overlays(&self.overlay.layers, canvas, sf, (self.width, self.height));
                    if cross_active {
                        canvas.save();
                        canvas.scale((sf, sf));
                        self.composer.render_layer(canvas);
                        canvas.restore();
                    }
                    after_draw(nodes, root_idx, surface);
                });
                // 截图读回在 flush 之后（skiwin draw 内）——保证真实呈现帧
                if crate::debug::screenshot_requested(debug_window_id) {
                    if let Some((w2, h2, pixels)) = take_capture() {
                        crate::debug::update_pixels(debug_window_id, &pixels, w2, h2);
                    }
                    crate::debug::screenshot_done(debug_window_id);
                }
            }
        }
        });
    }
}


impl PerWindow {
    pub(crate) fn refresh_theme(&mut self) -> bool {
        if !self.theme.cell.refresh(&mut self.theme.applied) {
            return false;
        }
        // The window's own snapshot (the surface clear color) is `theme_applied.colors`, refreshed above;
        // the tree has to run again for the new values to reach the components that resolved them.
        self.composer.mark_content_dirty();
        for ov in &mut self.overlay.layers {
            ov.composer.mark_content_dirty();
        }
        if let Some(ref sw) = self.skia_window {
            sw.request_redraw();
        }
        true
    }
}


/// 沿命中路径从内到外触发第一个 on_click——返回是否触发。
/// 主树 click 与 overlay 点击共用（消除重复）
/// 双轨（exp/modifier-node）：枚举 Clickable 优先（旧行为），无枚举时回退
/// 开放 ClickNode（第三方自定义点击无需改核心）。
pub(crate) fn fire_click_along_path(nodes: &[crate::layout::node::LayoutNode], path: &[usize]) -> bool {
    for &i in path.iter().rev() {
        if let Some(cb) = nodes[i].modifier.on_click() {
            (cb)();
            return true;
        }
        if let Some(node_cb) = nodes[i].modifier.node_click() {
            node_cb.on_click();
            return true;
        }
    }
    false
}


/// 按下：命中路径最内层 clickable 绑定的交互源 → 发射 Press
/// （对标 Compose clickable 的 PressInteraction.Press）
pub(crate) fn press_interaction_down(pw: &mut PerWindow, path: &[usize], scene_pos: (f32, f32)) {
    let (slot, idx, src) = {
        let nodes = pw.composer.arena_nodes();
        let Some(&idx) = path.iter().rev().find(|&&i| {
            nodes[i].modifier.clickable_interaction().is_some()
                || nodes[i].modifier.node_click_interaction().is_some()
        }) else {
            return;
        };
        let src = match nodes[idx]
            .modifier
            .clickable_interaction()
            .cloned()
            .or_else(|| nodes[idx].modifier.node_click_interaction())
        {
            Some(s) => s,
            None => return,
        };
        (nodes[idx].slot_key, idx, src)
    };
    // 场景坐标 → 节点本地坐标（对标 Compose PressInteraction.Press.pressPosition）。
    // 波纹中心必须存本地坐标：绘制时加回布局原点，滚动/变换后波纹跟随节点；
    // 若存场景坐标，按下后滚动/动画会脱离按钮。
    let local = {
        let nodes = pw.composer.arena_nodes();
        let clickable_local =
            crate::layout::node::scene_to_node_local(nodes, path, idx, scene_pos.0, scene_pos.1);
        // Ripple 与 clickable 不同节点时（如 Switch 的 Handle 容器）：
        // 按压坐标要换算到 Ripple 节点本地空间，否则波纹锚点偏移。
        // 本地坐标差 = 两节点绝对位置差（无滚动/变换时等价于 scene 换算）。
        let ripple_adjust = pw.composer.layout_root_idx().and_then(|r| {
            // 优先在命中路径内定位（同一 source 复用时避免锚点错位）；Switch 的
            // ripple 挂在 Handle 子节点上，点击轨道空白处时不在 path 内——回退
            // 全树搜索。已知限制：同一 source 复用于多个组件时全树搜索可能取到
            // 错误节点（当前 Switch 均为每组件独立 source，不受影响）。
            let ridx = path
                .iter()
                .copied()
                .find(|&i| {
                    nodes[i]
                        .modifier
                        .ripple_interaction()
                        .map(|s| s == &src)
                        .unwrap_or(false)
                })
                .or_else(|| {
                    nodes.iter().position(|n| {
                        n.modifier
                            .ripple_interaction()
                            .map(|s| s == &src)
                            .unwrap_or(false)
                    })
                })?;
            if ridx == idx {
                return None;
            }
            let (ax, ay) = node_abs_position(nodes, r, nodes[idx].id);
            let (rx, ry) = node_abs_position(nodes, r, nodes[ridx].id);
            Some((ax - rx, ay - ry))
        });
        match ripple_adjust {
            Some((dx, dy)) => (clickable_local.0 + dx, clickable_local.1 + dy),
            None => clickable_local,
        }
    };
    src.emit_press_at(local);
    pw.pressed_interaction = Some((slot, src));
}

/// 释放/取消按下交互（Up 或越界 slop）——对标 PressInteraction.Release/Cancel
pub(crate) fn release_pressed_interaction(pw: &mut PerWindow) {
    if let Some((_, src)) = pw.pressed_interaction.take() {
        src.emit_release();
    }
    // overlay 的按下交互同步释放（overlay 独立 composer——独立 pressed 状态）
    for ov in &mut pw.overlay.layers {
        if let Some((_, src)) = ov.pressed_interaction.take() {
            src.emit_release();
        }
    }
}

/// 悬停更新：**路径上所有 hoverable** 节点进入/离开 → 发射 Hover Enter/Exit
/// （对标 Compose hoverable：每个 hoverable 独立收到 Enter/Exit；修复前只
/// 发射最内层——嵌套 hoverable（如 Tooltip 锚点容器 + 内部 Button）外层
/// 收不到 Enter → Tooltip 不显示）。节点移除时自动补 Exit。
/// The cursor a hit path asks for: the deepest `pointer_hover_icon`, unless an ancestor with
/// `override_descendants` wins — Compose Desktop's `PointerHoverIconModifierNode` rule.
pub(crate) fn pointer_icon_for_path(
    nodes: &[LayoutNode],
    path: &[usize],
) -> crate::input::PointerIcon {
    let mut nearest: Option<crate::input::PointerIcon> = None;
    // Leaf first: the innermost icon is the nearest one, and the first override wins over
    // everything below it.
    for &idx in path.iter().rev() {
        for el in nodes[idx].modifier.elements().iter().rev() {
            if let crate::modifier::ModifierElement::PointerHoverIcon { icon, override_descendants } = el {
                if *override_descendants {
                    return *icon;
                }
                if nearest.is_none() {
                    nearest = Some(*icon);
                }
            }
        }
    }
    nearest.unwrap_or_default()
}

/// Apply a cursor to the window, skipping the platform call when it is already the current one.
pub(crate) fn update_cursor(pw: &mut PerWindow, icon: crate::input::PointerIcon) {
    if pw.cursor_icon == icon {
        return;
    }
    pw.cursor_icon = icon;
    if let Some(sw) = pw.skia_window.as_ref() {
        sw.set_cursor(winit::cursor::Cursor::Icon(icon.cursor_icon()));
    }
}

pub(crate) fn update_hover(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // overlay 优先：指针在 overlay 内容上 → 只更新 overlay 的 hover，
    // 同时主树 + **其他 overlay** 全部退出 hover（overlay 盖住下层——
    // 对齐真实指针层叠；堆叠 overlay 时下层按钮 hover 不残留）
    if let Some((i, local)) = hit_overlay(pw, scene_pos) {
        let ov = &mut pw.overlay.layers[i];
        overlay_update_hover(ov, local);
        // 主树 hover 全清（指针在 overlay 上——主树不可见）
        let olds: Vec<u64> = pw.hovered_slots.drain().collect();
        for slot in olds {
            exit_hover_at(pw, slot);
        }
        // 其他 overlay（下层）hover 全清
        for j in 0..pw.overlay.layers.len() {
            if j == i { continue; }
            let olds: Vec<u64> = pw.overlay.layers[j].hovered_slots.drain().collect();
            for slot in olds {
                overlay_exit_hover_at(&mut pw.overlay.layers[j], slot);
            }
        }
        let icon = {
            let ov = &pw.overlay.layers[i];
            match ov.composer.layout_root_idx() {
                Some(root) => {
                    let nodes = ov.composer.arena_nodes();
                    let path = hit_test_with_flights(
                        nodes,
                        root,
                        ov.composer.transition_roots(),
                        local.0,
                        local.1,
                    );
                    pointer_icon_for_path(nodes, &path)
                }
                None => crate::input::PointerIcon::DEFAULT,
            }
        };
        update_cursor(pw, icon);
        return;
    }
    // 未命中任何 overlay：主树 hover 更新 + 所有 overlay hover 全清
    // （指针离开 overlay——overlay 的 hover 状态清理）
    for ov in &mut pw.overlay.layers {
        let olds: Vec<u64> = ov.hovered_slots.drain().collect();
        for slot in olds {
            overlay_exit_hover_at(ov, slot);
        }
    }
    // 当前路径上所有 hoverable 的 (slot, interaction)
    let (hit, icon): (Vec<(u64, crate::interaction::MutableInteractionSource)>, crate::input::PointerIcon) = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        let hit = path.iter()
            .filter(|&&i| nodes[i].modifier.has_hoverable())
            .filter_map(|&i| nodes[i].modifier.hoverable_interaction().map(|s| (nodes[i].slot_key, s.clone())))
            .collect();
        (hit, pointer_icon_for_path(nodes, &path))
    };
    update_cursor(pw, icon);
    let hit_slots: std::collections::HashSet<u64> = hit.iter().map(|(s, _)| *s).collect();
    // 仍在 hover 的：跳过（已 enter）
    // 新进入：emit enter + 记录
    for (slot, src) in &hit {
        if pw.hovered_slots.insert(*slot) {
            src.emit_hover_enter();
        }
    }
    // 已退出：补 Exit + 移除
    let gone: Vec<u64> = pw.hovered_slots.iter().copied().filter(|s| !hit_slots.contains(s)).collect();
    for slot in gone {
        pw.hovered_slots.remove(&slot);
        exit_hover_at(pw, slot);
    }
}


/// 对指定 slot 的节点补发 Hover Exit（节点已移除则跳过——hover 状态自然清理）
pub(crate) fn exit_hover_at(pw: &mut PerWindow, slot: u64) {
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
            if let Some(src) = nodes[idx].modifier.hoverable_interaction() {
                src.emit_hover_exit();
            }
        }
    }
}

/// Compose 风格 click 检测：Down 记录（pointer_down_state）、Up 释放时触发 on_click。
/// 真实 PointerButton Up 与 debug 模拟（d/u）共用——消除平行实现并可模拟验证。
pub(crate) fn detect_click(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    const CLICK_SLOP: f32 = 18.0;
    const CLICK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
    let Some(down) = pw.input.down.take() else {
        return false;
    };
    // Down 时所在节点的 slot_key（跨重组稳定——Down 与 Up 之间可能发生重组
    // （如聚焦触发），arena 节点重建 → node_id 变化 → 旧 id 匹配必然失败；
    // slot_key 按组合位置稳定，不受重组影响）
    let down_slot = pw.input.down_slot;
    let dx = scene_pos.0 - down.position.0;
    let dy = scene_pos.1 - down.position.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let in_time = down.time.elapsed() < CLICK_TIMEOUT;
    if dist <= CLICK_SLOP && in_time {
        let nodes = pw.composer.arena_nodes();
        if let Some(r) = pw.composer.layout_root_idx() {
            let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
            let hit = down_slot
                .map(|slot| path.iter().any(|&i| nodes[i].slot_key == slot))
                .unwrap_or_else(|| path.iter().any(|&i| nodes[i].id == down.node_id));
            if hit {
                // 只在相同节点触发 click（从内到外找第一个 on_click）
                return fire_click_along_path(nodes, &path);
            }
        }
    }
    false
}

/// 键盘事件派发到焦点路径（Preview: root→focused；Bubble: focused→root，
/// 对齐 onPreviewKeyEvent/onKeyEvent）+ 聚焦组件激活（Enter/Space 触发
/// onClick——仅聚焦节点自身的 clickable）。
///
/// 真实 KeyboardInput 与 debug 模拟共用（防行为分叉）：Escape/Tab 等
/// 框架级按键由调用方前置处理（聚焦导航/清焦），不进入本函数。
pub(crate) fn dispatch_key_to_focus(pw: &PerWindow, ke: &crate::input::KbEvent) -> bool {
    let Some(fid) = pw.focused_id else { return false };
    let Some(r) = pw.composer.layout_root_idx() else { return false };
    dispatch_key_in_arena(pw.composer.arena_nodes(), r, fid, ke)
}

/// Dispatch a key event to the topmost focused overlay first (overlays own
/// independent composers — the main-tree focus path can never reach them).
/// Falls through (returns false) when no overlay has focus, so callers can
/// continue with the main tree.
pub(crate) fn dispatch_key_to_overlay(pw: &PerWindow, ke: &crate::input::KbEvent) -> bool {
    // Only the arena that OWNS the keyboard gets the key. Walking "the first overlay that has focus"
    // would hand keys to a layer below the modal that is actually on top (an overlay keeps a
    // remembered focus now, see `claim_keyboard_for_overlay`), and falling through to lower surfaces
    // would double-handle a key the owner already acted on.
    let Some(i) = keyboard_scope(pw) else { return false };
    let ov = &pw.overlay.layers[i];
    let Some(fid) = ov.focused_id else { return false };
    let Some(r) = ov.composer.layout_root_idx() else { return false };
    // The owner did not consume — stop here.
    dispatch_key_in_arena(ov.composer.arena_nodes(), r, fid, ke)
}

/// Core key dispatch against one arena: focus path Preview (root→focused) +
/// Bubble (focused→root) + focused-node activation (Enter/Space → onClick).
/// Shared by the main tree and overlay composers (identical semantics).
pub(crate) fn dispatch_key_in_arena(
    nodes: &[crate::layout::node::LayoutNode],
    r: usize,
    fid: u64,
    ke: &crate::input::KbEvent,
) -> bool {
    // 收集焦点路径：root → ... → focused
    let mut path: Vec<usize> = Vec::new();
    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
        path.push(idx);
        let mut pid = nodes[idx].parent_id;
        while let Some(id) = pid {
            if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                path.push(anc);
                pid = nodes[anc].parent_id;
            } else { break; }
        }
        path.reverse(); // path[0] == root, path[last] == focused
    }
    // Preview: root → focused（对齐 onPreviewKeyEvent）
    // 双轨（exp/modifier-node）：枚举 on_pre_key 先行，同节点 key node 紧随。
    for &ni in &path {
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {
                if handler(ke) { return true; }
            }
        }
        for k in nodes[ni].modifier.key_nodes() {
            if k.on_pre(ke) { return true; }
        }
    }
    // Bubble: focused → root（对齐 onKeyEvent）
    for &ni in path.iter().rev() {
        for el in nodes[ni].modifier.elements().iter().rev() {
            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                if handler(ke) { return true; }
            }
        }
        for k in nodes[ni].modifier.key_nodes() {
            if k.on_event(ke) { return true; }
        }
    }
    // 聚焦组件的键盘激活（对标 Compose clickable：聚焦时按 Enter/Space
    // 触发 onClick——仅聚焦节点自身的 clickable 响应，不向祖先冒泡）
    let is_activate = matches!(&ke.key, winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter))
        || matches!(&ke.key, winit::keyboard::Key::Character(c) if c == " ");
    if ke.event_type == crate::input::KbEventType::KeyDown && !ke.repeat && is_activate {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
            if let Some(on_click) = nodes[idx].modifier.on_click() {
                on_click();
                return true;
            }
            // 双轨：node 点击同样响应键盘激活
            if let Some(node_cb) = nodes[idx].modifier.node_click() {
                node_cb.on_click();
                return true;
            }
        }
    }
    false
}

/// 解析 debug `k` 命令的按键字符串为 winit Key（单字符 → Character；
/// 特殊名 → NamedKey）。无法解析返回 None（忽略）。
pub(crate) fn parse_debug_key(s: &str) -> Option<winit::keyboard::Key> {
    use winit::keyboard::{Key, NamedKey};
    match s {
        "Backspace" => Some(Key::Named(NamedKey::Backspace)),
        "Delete" => Some(Key::Named(NamedKey::Delete)),
        "Enter" => Some(Key::Named(NamedKey::Enter)),
        "Escape" => Some(Key::Named(NamedKey::Escape)),
        "ArrowLeft" => Some(Key::Named(NamedKey::ArrowLeft)),
        "ArrowRight" => Some(Key::Named(NamedKey::ArrowRight)),
        "ArrowUp" => Some(Key::Named(NamedKey::ArrowUp)),
        "ArrowDown" => Some(Key::Named(NamedKey::ArrowDown)),
        // The spacebar. winit 0.31 (the version in Cargo.lock) has no `NamedKey::Space`, so a spacebar
        // arrives as `Key::Character(" ")` — the same spelling winia's own activation path checks
        // (`app.rs:4552`), and the one components reading it as activation accept (`overlay.rs`, an
        // editable menu anchor). This name exists so a test can press it at all.
        "Space" => Some(Key::Character(" ".into())),
        "Home" => Some(Key::Named(NamedKey::Home)),
        "End" => Some(Key::Named(NamedKey::End)),
        _ if s.chars().count() == 1 => Some(Key::Character(s.to_string().into())),
        _ => None,
    }
}

/// 查找 grapheme anchor 定位用的文本节点（有 cached_paragraph 的节点）。
///
/// 策略分两步：
/// 1. **向上**：沿命中路径从 innermost 起找第一个有 paragraph 的节点——
///    命中文本本身（输入 leaf / label / 输出 Text）时直接命中。
/// 2. **向下**（向上失败，且 innermost 是 TextField 容器）：点击容器空白
///    （文本右侧/padding/后缀区）时输入 leaf 不在命中路径——DFS 容器子树
///    找第一个「有 paragraph + 有 registrar」的后代（TextField 输入 leaf；
///    label/placeholder 有 para 无 reg，排除）。
///
/// ⚠ 向下分支必须**限定 TextField 容器**（modifier 含 TextFieldVisual；
/// 输入 leaf 无此标记）：非 TextField 空白（页面空隙/Column 空白/
/// SelectionContainer 空白）绝不向下找——否则 DFS 会误命中子树中任意
/// 「有 para+reg」的文本（如远处 TextField 的输入 leaf），从空白按下的
/// 拖动会错误选中/错绑 anchor。
///
/// 返回 None = 点击处无可定位文本（空区域点击 → 不设光标/不开始拖选）。
pub(crate) fn find_anchor_text_node(nodes: &[LayoutNode], path: &[usize], innermost: usize) -> Option<usize> {
    if let Some(&i) = path.iter().rev().find(|&&i| nodes[i].cached_paragraph.borrow().is_some()) {
        return Some(i);
    }
    let is_tf_container = nodes[innermost].modifier.elements().iter()
        .any(|el| matches!(el, crate::modifier::ModifierElement::TextFieldVisual { .. }));
    if !is_tf_container {
        return None;
    }
    // 向下找：TextField 容器子树中的输入 leaf（para + registrar 双条件）
    pub(crate) fn dfs(nodes: &[LayoutNode], idx: usize) -> Option<usize> {
        if nodes[idx].cached_paragraph.borrow().is_some() && nodes[idx].registrar.borrow().is_some() {
            return Some(idx);
        }
        for &c in &nodes[idx].children {
            if let Some(f) = dfs(nodes, c) { return Some(f); }
        }
        None
    }
    dfs(nodes, innermost)
}

/// 节点或其子树中是否有节点声明 IME 需求（ime_callback 非空）。
///
/// ⚠ TextField 容器化：焦点/键盘在容器（focusable 挂在容器 modifier），而
/// ime_callback 在**输入 leaf**（set_current_node_ime_callback）——只查焦点
/// 节点自身则 TextField 聚焦时 IME 永不开启。DFS 向下找子树（与
/// find_anchor_text_node 同策略）；Button 等无 ime_callback 的 focusable
/// 子树不匹配 → 聚焦不开启输入法。
pub(crate) fn node_or_descendant_wants_ime(nodes: &[LayoutNode], idx: usize) -> bool {
    if nodes[idx].ime_callback.borrow().is_some() {
        return true;
    }
    for &c in &nodes[idx].children {
        if node_or_descendant_wants_ime(nodes, c) {
            return true;
        }
    }
    false
}

/// 从节点子树中找第一个声明 ime_callback 的节点（TextField 输入 leaf）。
///
/// ⚠ 与 node_or_descendant_wants_ime 同根因：焦点在容器、ime_callback 在
/// 输入 leaf——Preedit 事件须派发到实际持有 ime_callback 的节点。
pub(crate) fn find_descendant_ime_callback(nodes: &[LayoutNode], idx: usize) -> Option<usize> {
    if nodes[idx].ime_callback.borrow().is_some() {
        return Some(idx);
    }
    for &c in &nodes[idx].children {
        if let Some(f) = find_descendant_ime_callback(nodes, c) {
            return Some(f);
        }
    }
    None
}

/// 指针按下核心（真实 PointerButton 与 debug 模拟共用——防行为分叉）。
///
/// 统一：hit_test → 清除旧选区 → grapheme anchor 定位 → PtrDownState 记录 →
/// 光标设置/自动聚焦（仅真实路径 `with_focus`）→ on_pointer_event 分发。
/// 返回 true 表示命中节点并已处理。
pub(crate) fn handle_pointer_down(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::input::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
    with_focus: bool,
) -> bool {
    // 顶层弹出层优先：命中 overlay → 记录点击目标（事件不进主树）；
    // 外部点击 → dismiss（模态/可关闭）
    if overlay_down(pw, scene_pos, kind.clone()) {
        return true;
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
    let Some(&innermost) = path.last() else { return false; };

    // 清除旧的选区（新点击开始）
    {
        if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
            reg.clear_selection();
        }
    }
    // grapheme anchor 定位。
    // ⚠ TextField 容器化后：paragraph 只缓存在**输入 leaf**，容器节点无
    // cached_paragraph——点击容器空白命中容器 → innermost 无 para。由
    // find_anchor_text_node 处理：向上找有 paragraph 的祖先（命中文本本身），
    // 失败且命中 TextField 容器时向下找子树中的输入 leaf。
    let anchor_node = find_anchor_text_node(nodes, &path, innermost);
    let anchor = anchor_node.and_then(|ai| {
        let borrow = nodes[ai].cached_paragraph.try_borrow().ok()?;
        borrow.as_ref().map(|para| {
            let (ax, ay) = node_abs_position(nodes, r, nodes[ai].id);
            // 段落局部坐标须扣除节点 padding（渲染侧文本画在 content 区 =
            // 节点原点 + padding——不扣则点击位置整体偏 padding 偏移，
            // 光标跳错位置）
            let (pad_s, pad_t, pad_e, _) = nodes[ai].modifier.get_padding_sides();
            let pad_x = if nodes[ai].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
            let tl = crate::text::TextLayout::new(para, 0);
            let hit = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(scene_pos.0 - ax - pad_x, scene_pos.1 - ay - pad_t));
            // 定位结果是显示文本偏移（paragraph = 显示文本）——经 OffsetMapping
            // 转回编辑偏移（密码掩码/格式化输入）。
            // ⚠ TextFieldVisual 挂在**容器**——须向上找（offset_mapping_for_node）
            crate::text::field::offset_mapping_for_node(nodes, r, ai)
                .map(|m| m.transformed_to_original(hit))
                .unwrap_or(hit)
        })
    });
    // reg 只用 anchor 节点自己的 registrar（不 fallback active_registrar）——
    // 不可选节点（输出 Text 等未注册）的 node.registrar 为 None，
    // fallback 会取到全局残留（如 Container B 的）→ anchor_registrar
    // 错绑 B → 拖动到 B 时 same_reg=true → 混合偏移 → B 被选
    let own_reg = anchor_node.and_then(|ai| nodes[ai].registrar.borrow().as_ref().cloned());
    let anchor_global = anchor.and_then(|a| {
        anchor_node.and_then(|ai| {
            own_reg.as_ref()
                .and_then(|reg| reg.segment_info(nodes[ai].slot_key).map(|(off, _)| off + a))
        })
    });
    pw.input.down = Some(PtrDownState {
        node_id: nodes[innermost].id,
        position: scene_pos,
        time: std::time::Instant::now(),
        selection_anchor: anchor_global,
        anchor_registrar: own_reg, // None（不可选节点）→ 无 anchor 容器
    });
    pw.input.down_slot = Some(nodes[innermost].slot_key);

    if with_focus {
        // 设置 TextField 光标位置 + selection 更新回调（仅真实路径）。
        // ⚠ 点击聚焦由组件自己决定（TextField 内部 requestFocus）——
        // 框架层不自动聚焦（对标 Compose：clickable/focusable 点击不请求焦点，
        // 焦点由 Tab 导航或显式 requestFocus 获得）。
        // 光标设置在 anchor 节点（输入 leaf）上——光标绘制/回调都在 leaf
        if let (Some(a), Some(ai)) = (anchor, anchor_node) {
            nodes[ai].cursor_index.set(a);
            if let Some(cb) = nodes[ai].cursor_callback.borrow_mut().as_mut() {
                cb(a);
            }
        }
    }

    // 按下交互（clickable 绑定源——Compose Press 语义；置于 nodes 借用结束后）
    press_interaction_down(pw, &path, scene_pos);

    // 手势入口（on_press 立即触发；后续 move/up 由 gesture_node 路由）——
    // 置于 with_focus 块后（nodes 借用结束，避免与 pw mut 冲突）
    gesture_down(pw, scene_pos);

    // 拖拽滚动目标：按下点向上找最近滚动容器。文本选择/组件 drag 手势优先
    // （拖选文本/组件拖拽不滚动——对齐 Compose 最内层 pointerInput 消费）
    pw.input.drag_scroll = (|| {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return None };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[drag-down] scene=({},{}) path_len={} path={:?}",
                scene_pos.0, scene_pos.1, path.len(),
                path.iter().map(|&i| (i, nodes[i].slot_key, nodes[i].modifier.vertical_scroll_state().is_some())).collect::<Vec<_>>());
            for &i in &path {
                let n = &nodes[i];
                eprintln!("  [drag-node] idx={} pos=({},{}) size=({},{}) vp_h={} scroll={}",
                    i, n.position.x, n.position.y, n.measured_size.width, n.measured_size.height,
                    n.scroll_viewport_height,
                    n.modifier.vertical_scroll_state().is_some());
            }
        }
        let Some(&_innermost) = path.last() else { return None };
        let selecting = pw.input.down.as_ref()
            .map(|s| s.selection_anchor.is_some())
            .unwrap_or(false);
        // 内层手势优先，但**滚动优先于外层面板拖拽**（对齐 Compose：BottomSheet 面板
        // 整面板可拖，但列表区按下时内容滚动优先——面板 on_drag 是外层 fallback）。
        // child_drag 仅在「drag 节点比 scroll 节点更深」（内层组件如 slider/switch）
        // 时才成立；drag 是 scroll 祖先（面板）→ 滚动优先，不放行 child_drag。
        let drag_idx = path_drag_idx(nodes, &path);
        let scroll_idx = path_scroll_idx(nodes, &path);
        let child_drag = inner_component_drag(drag_idx, scroll_idx);
        // An inner drag component and the scroll it sits in both want this gesture; which one gets it
        // is decided by the finger's dominant axis in `gesture_move`, so record the scroll as the
        // fallback owner here. `None` when the component stands alone (it owns every direction), and
        // `None` while a text selection is in progress: the selection keeps the pointer (it is
        // extended from every move below), so the list must not start scrolling under the finger.
        pw.input.scroll_slot = if child_drag && !selecting {
            scroll_idx.map(|i| nodes[i].slot_key)
        } else {
            None
        };
        if selecting || child_drag {
            None
        } else {
            scroll_idx.map(|i| DragScroll::new(nodes[i].slot_key, scene_pos))
        }
    })();

    // 分发 on_pointer_event（Down）
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::input::PointerEvent {
        event_type: crate::input::PointerEventType::Down,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.input.last_kind = ptr_ev.kind.clone();
    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
    true
}

/// 指针移动核心（真实 PointerMoved 与 debug 模拟共用——防行为分叉）。
///
/// 统一：拖拽选区（18px slop + x_off 对齐偏移 + grapheme 定位 + compute_selection）
/// + on_pointer_event 分发。返回是否消费（选择更新或 handler 消费）。
pub(crate) fn handle_pointer_move(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::input::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
) -> bool {
    // 手势驱动（drag capture：tracker 存在即路由——指针移出组件仍接收）
    // ⚠ 必须在 nodes 借用之前（gesture_move 内部自取 nodes/pw mut）
    let mut handled = false;
    if pw.input.node.is_some() && gesture_move(pw, scene_pos) {
        handled = true;
    }
    // overlay 点击 slop 取消：按下 overlay 后拖出 18px → 取消 click（对齐
    // 主树 detect_click 的 slop 判定；down 位置 = overlay screen_pos + local）
    if pw.overlay.click.is_some() {
        let cancel = match pw.overlay.click {
            Some((idx, local, _)) => {
                match pw.overlay.layers.get(idx) {
                    Some(ov) => {
                        let down_screen = (ov.screen_pos.0 + local.0, ov.screen_pos.1 + local.1);
                        let dx = scene_pos.0 - down_screen.0;
                        let dy = scene_pos.1 - down_screen.1;
                        (dx * dx + dy * dy).sqrt() > 18.0
                    }
                    None => false,
                }
            }
            None => false,
        };
        if cancel {
            pw.overlay.click = None;
            release_pressed_interaction(pw);
        }
    }
    if let Some((idx, slot, down_pos)) = pw.overlay.drag {
        let (dx, dy) = (scene_pos.0 - down_pos.0, scene_pos.1 - down_pos.1);
        if !pw.overlay.drag_started {
            if dx * dx + dy * dy >= 8.0 * 8.0 {
                pw.overlay.drag_started = true;
                pw.overlay.drag_last = Some(scene_pos);
                if let Some(ov) = pw.overlay.layers.get(idx) {
                    let nodes = ov.composer.arena_nodes();
                    if let Some(r) = ov.composer.layout_root_idx() {
                        // ⚠ The overlay arena is LAYER-local, and a gesture callback receives
                        // node-local coordinates (`fire_gesture_action` subtracts the node's
                        // position inside the arena it was given). So the absolute position has to
                        // be converted here — passing `scene_pos` shifted it by the popup's screen
                        // origin, which is why dragging a Slider inside a popup landed on the wrong
                        // value (deltas are unaffected: the layer offset cancels in a difference).
                        let local = (
                            scene_pos.0 - pw.overlay.drag_origin.0,
                            scene_pos.1 - pw.overlay.drag_origin.1,
                        );
                        fire_gesture_action(nodes, r, slot,
                            crate::input::gesture::GestureAction::DragStart(local));
                        fire_gesture_action(nodes, r, slot,
                            crate::input::gesture::GestureAction::DragMove(local, (dx, dy)));
                    }
                }
                handled = true;
            }
        } else {
            let last = pw.overlay.drag_last.unwrap_or(down_pos);
            let inc = (scene_pos.0 - last.0, scene_pos.1 - last.1);
            pw.overlay.drag_last = Some(scene_pos);
            if let Some(ov) = pw.overlay.layers.get(idx) {
                let nodes = ov.composer.arena_nodes();
                if let Some(r) = ov.composer.layout_root_idx() {
                    let local = (
                        scene_pos.0 - pw.overlay.drag_origin.0,
                        scene_pos.1 - pw.overlay.drag_origin.1,
                    );
                    fire_gesture_action(nodes, r, slot,
                        crate::input::gesture::GestureAction::DragMove(local, inc));
                }
            }
            handled = true;
        }
    }
    // overlay 内拖拽滚动（独立 Composer——BottomSheet 内 LazyColumn）
    // 列表滚动走 overlay Composer 的 dispatch_nested_scroll_delta，Sheet 通过 panel 上的
    // SheetNested（nested_scroll）吃剩余 available。此分支仅消费列表，能否拖 Sheet 取决于 nested。
    if pw.overlay.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            let slot = pw.overlay.drag_scroll.as_ref().unwrap().slot;
            let ov = pw.overlay.layers.get(ov_idx)?;
            let r = ov.composer.layout_root_idx()?;
            let nodes = ov.composer.arena_nodes();
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.overlay.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            let (ax, ay) = {
                let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
                let ov = &pw.overlay.layers[ov_idx];
                let nodes = ov.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() { (0.0, dy) }
                else if nodes[idx].modifier.horizontal_scroll_state().is_some() { (dx, 0.0) }
                else { (0.0, 0.0) }
            };
            if ax != 0.0 || ay != 0.0 {
                let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
                if let Some(ov) = pw.overlay.layers.get_mut(ov_idx) {
                    if let Some(root) = ov.composer.layout_root_idx() {
                        let density = crate::unit::Density::from_density(pw.scale_factor as f32);
                        let consumed = dispatch_nested_scroll_delta(ov.composer.arena_nodes_mut(), root, idx, crate::nested_scroll::ScrollDelta::new(ax, ay), crate::nested_scroll::NestedScrollSource::Drag, density);
                        if consumed.x != 0.0 || consumed.y != 0.0 {
                            handled = true;
                        }
                    }
                }
            }
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            if let Some(ov) = pw.overlay.layers.get(ov_idx) {
                let nodes = ov.composer.arena_nodes();
                if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() { ss.is_scroll_in_progress.set(true); }
                else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() { ss.is_scroll_in_progress.set(true); }
            }
        }
    }
    // 拖拽滚动：内容跟随指针 + 记录速度样本（松手 fling 用）。放在手势/文本
    // 选择之前——但按下时已排除组件 drag 手势与文本选择，此处无冲突
    if pw.input.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let nodes = pw.composer.arena_nodes();
            let Some(r) = pw.composer.layout_root_idx() else { return None };
            let slot = pw.input.drag_scroll.as_ref().unwrap().slot;
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.input.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                let nodes = pw.composer.arena_nodes();
                eprintln!("[drag-move] target_idx={} scroll_v={} scroll_h={} slot={:?}",
                    idx,
                    nodes[idx].modifier.vertical_scroll_state().is_some(),
                    nodes[idx].modifier.horizontal_scroll_state().is_some(),
                    pw.input.drag_scroll.as_ref().map(|d| d.slot));
            }
            // 轴感知：垂直容器吃 dy，水平容器吃 dx（apply_scroll_delta 按节点轴取）
            let (ax, ay) = {
                let nodes = pw.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() {
                    (0.0, dy)
                } else if nodes[idx].modifier.horizontal_scroll_state().is_some() {
                    (dx, 0.0)
                } else {
                    (0.0, 0.0)
                }
            };
            if ax != 0.0 || ay != 0.0 {
                let root = pw.composer.layout_root_idx();
                let consumed = root.map(|root| dispatch_nested_scroll_delta(
                    pw.composer.arena_nodes_mut(),
                    root,
                    idx,
                    crate::nested_scroll::ScrollDelta::new(ax, ay),
                    crate::nested_scroll::NestedScrollSource::Drag,
                    crate::unit::Density::from_density(pw.scale_factor as f32),
                )).unwrap_or(crate::nested_scroll::ScrollDelta::ZERO);
                handled = consumed.x != 0.0 || consumed.y != 0.0;
            }
            // 拖拽中标记（apply_scroll_delta 内部取消 fling 时置 false——这里覆盖）
            let nodes = pw.composer.arena_nodes();
            if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            } else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            }
        }
    }
    // 悬停更新（自身 hit test——不依赖下方 nodes 借用）
    update_hover(pw, scene_pos);
    // 越界 slop：按下交互取消（Compose：press 超过 touch slop → Cancel）
    if pw.input.down.is_some() {
        let down = pw.input.down.as_ref().unwrap();
        let dx = scene_pos.0 - down.position.0;
        let dy = scene_pos.1 - down.position.1;
        if (dx * dx + dy * dy).sqrt() > 18.0 {
            release_pressed_interaction(pw);
            // overlay 点击同步取消（对齐主树 detect_click 的 slop 判定——
            // 拖出 slop 后 up 不应 fire overlay click）
            pw.overlay.click = None;
        }
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);

    // 拖拽选中文本
    if pw.input.down.is_some() {
        if let Some(&innermost) = path.last() {
            let down = pw.input.down.as_ref().unwrap();
            let dx = scene_pos.0 - down.position.0;
            let dy = scene_pos.1 - down.position.1;
            const CLICK_SLOP: f32 = 18.0;
            // 文本选择：按下在可选中文本（anchor 有值）→ **任意移动即更新
            // 选区**（单字母 ~7px < CLICK_SLOP——slop 门槛导致无法选中单
            // 字母/小范围文本；对齐 Compose：按下即进入拖选）。其余场景
            // 保持 slop 判定（clickable/拖拽手势）
            let text_selection = down.selection_anchor.is_some();
            if text_selection || (dx * dx + dy * dy).sqrt() > CLICK_SLOP {
                let (abs_x, abs_y) = node_abs_position(nodes, r, nodes[innermost].id);
                if let Ok(borrow) = nodes[innermost].cached_paragraph.try_borrow() {
                    if let Some(para) = borrow.as_ref() {
                        // 对齐偏移（匹配渲染侧 x_off）——绘制用内容盒，选区高亮须一致
                        //（飞行中 measured_size 是父级被告知的尺寸，review R3-F6）
                        let node_w = nodes[innermost].content_box().width;
                        let align = nodes[innermost].modifier.align().unwrap_or(crate::text::TextAlign::Left);
                        // The slack rule has to be the PAINT paths' rule, not a copy of the old
                        // one: a WRAPPED paragraph was laid out inside its box, so a negative
                        // slack would push it out of it — but an UNWRAPPED one
                        // (`soft_wrap(false)`, laid out at its intrinsic width) can be WIDER than
                        // the box, and then centring or right-aligning has to shift it the other
                        // way. With the clamped form here, a selection on an overflowing label
                        // was skewed by half the overflow (`render.rs` uses the conditional one
                        // at both of its sites).
                        let soft_wrap = nodes[innermost]
                            .modifier
                            .elements()
                            .iter()
                            .find_map(|el| match el {
                                crate::modifier::ModifierElement::TextContent { soft_wrap, .. } => Some(*soft_wrap),
                                _ => None,
                            })
                            .unwrap_or(true);
                        let slack = if soft_wrap {
                            (node_w - para.max_intrinsic_width()).max(0.0)
                        } else {
                            node_w - para.max_intrinsic_width()
                        };
                        let x_off = match align {
                            crate::text::TextAlign::Center => abs_x + slack / 2.0,
                            crate::text::TextAlign::Right => abs_x + slack,
                            _ => abs_x,
                        };
                        // 段落局部坐标：扣节点 padding（与渲染侧 content 区一致——
                        // 否则拖动选区整体偏 padding 偏移）
                        let (pad_s, pad_t, pad_e, _) = nodes[innermost].modifier.get_padding_sides();
                        let pad_x = if nodes[innermost].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                        let tl = crate::text::TextLayout::new(para, 0);
                        {
                            let down = pw.input.down.as_ref().unwrap();
                            // 不可选节点（未注册到任何 SelectionContainer）→ 不更新选择
                            // （用 if let 包裹而非 else return——return 会跳过 dispatch_ptr_event/request_redraw）
                            if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
                                let current_index = tl.get_closest_grapheme_cluster_cluster_at(
                                    skia_safe::Point::new(scene_pos.0 - x_off - pad_x, scene_pos.1 - abs_y - pad_t));
                                // 显示偏移 → 编辑偏移（密码掩码/格式化输入；
                                // TextFieldVisual 在容器——向上找）
                                let current_index = crate::text::field::offset_mapping_for_node(nodes, r, innermost)
                                    .map(|m| m.transformed_to_original(current_index))
                                    .unwrap_or(current_index);
                                let cur_off = reg.segment_info(nodes[innermost].slot_key).map(|(off, _)| off);
                                if let Some((target, s, e)) = crate::components::selection_container::compute_selection(
                                    down.anchor_registrar.as_ref(), down.selection_anchor,
                                    &reg, cur_off, current_index,
                                    scene_pos.1, down.position.1, abs_y,
                                ) {
                                    // 范围是编辑偏移（anchor/current 已转回）——
                                    // reg 空间 = 显示偏移，写入选区前转换
                                    let (ts, te) = crate::text::field::offset_mapping_for_node(nodes, r, innermost)
                                        .map(|m| (m.original_to_transformed(s), m.original_to_transformed(e)))
                                        .unwrap_or((s, e));
                                    target.set_selection(ts, te);
                                    handled = true;
                                }
                            } // end if let Some(reg)
                        }
                    }
                }
            }
        }
    }

    // 分发 on_pointer_event（悬停 Move 也可能更新 State）
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::input::PointerEvent {
        event_type: crate::input::PointerEventType::Move,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.input.last_kind = ptr_ev.kind.clone();
    let consumed = dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
    handled || consumed
}

/// 分发指针事件到 hit_test 路径（pre: outer→inner, bubble: inner→outer）
/// 计算节点在布局树中的绝对位置（从根累加 position）。
///
/// ⚠ 与 `hit_test`/`scene_to_node_local` 同空间：累加时**减去祖先 scroll 偏移**
/// （渲染时滚动容器 `canvas.translate(-offset)` → 命中测试的 scene 坐标就是
/// 减过滚动量的"滚动画布坐标"）。此前纯累加 layout position，滚动容器内
/// 节点绝对 y 被滚动量污染 → 段落局部坐标（scene - abs）偏负 → skia 最近
/// glyph 恒为第一行——多行 TextField 点击/拖拽只能定位到第一行。
pub(crate) fn node_abs_position(nodes: &[LayoutNode], root: usize, id: u64) -> (f32, f32) {
    pub(crate) fn walk(nodes: &[LayoutNode], idx: usize, target: u64, abs_x: f32, abs_y: f32) -> Option<(f32, f32)> {
        let node = &nodes[idx];
        let nx = abs_x + node.position.x;
        let ny = abs_y + node.position.y;
        if node.id == target { return Some((nx, ny)); }
        // 命中测试同路径：子节点坐标 = 本节点坐标 - 本节点 scroll 偏移
        let (dx, dy) = crate::layout::node::scroll_offset_for_node(node);
        for &c in &node.children {
            if let Some(r) = walk(nodes, c, target, nx - dx, ny - dy) { return Some(r); }
        }
        None
    }
    walk(nodes, root, id, 0.0, 0.0).unwrap_or((0.0, 0.0))
}

pub(crate) fn dispatch_ptr_event(
    nodes: &[LayoutNode],
    root: usize,
    path: &[usize],
    event: &crate::input::PointerEvent,
    scene_pos: (f32, f32),
    captured_id: Option<u64>,
) -> bool {
    // 如果指针被一个节点捕获（Down 后未释放），用 root 查找节点并构建祖先链
    let captured_path: Vec<usize> = if let Some(cid) = captured_id {
        let mut ancestors: Vec<usize> = Vec::new();
        if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, root, cid).and_then(|nid| crate::layout::node::find_node_by_id(nodes, root, nid)) {
            ancestors.push(nid);
            let mut pid = nodes[nid].parent_id;
            while let Some(id) = pid {
                if let Some(anc) = crate::layout::node::find_node_by_id(nodes, root, id) {
                    ancestors.push(anc);
                    pid = nodes[anc].parent_id;
                } else { break; }
            }
            ancestors.reverse(); // root → ... → captured
            ancestors
        } else {
            path.to_vec()
        }
    } else { path.to_vec() };
    let use_path = &captured_path;
    // 计算路径累积偏移（每个节点的 position 是相对于父节点的偏移）。
    // 与 hit_test_recursive 一致：节点 i 的视觉坐标 = Σposition[0..=i]
    // − Σscroll[0..i-1]（祖先的 scroll offset；节点自身 offset 只影响子节点，
    // 不影响自身左上角——见 node.rs hit_test_recursive 的 child_px/nx 逻辑）。
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let mut scroll_x = 0.0f32;
    let mut scroll_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = use_path.iter().map(|&i| {
        abs_x += nodes[i].position.x;
        abs_y += nodes[i].position.y;
        // 当前节点坐标 = 累积位置 − 祖先 scroll 累积（自身 offset 稍后累加）
        let pos = (abs_x - scroll_x, abs_y - scroll_y);
        let (sdx, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[i]);
        scroll_x += sdx;
        scroll_y += sdy;
        pos
    }).collect();

    // on_pre_ptr: outer → inner
    // 双轨（exp/modifier-node）：枚举 on_pre_ptr 先行，同节点 node.on_pre 紧随。
    for (i, &ni) in use_path.iter().enumerate() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_pre_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
        for p in nodes[ni].modifier.pointer_nodes() {
            if p.on_pre(&ev) { return true; }
        }
    }

    // on_ptr: inner → outer
    for (i, &ni) in use_path.iter().enumerate().rev() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
        for p in nodes[ni].modifier.pointer_nodes() {
            if p.on_event(&ev) { return true; }
        }
    }
    false
}

/// request 节流判断（纯函数——边界单测）：
/// 距上次 request >= 帧间隔才允许请求（WM_PAINT 生成频率受控为刷新率）。
/// 独立于渲染节流（last_render_time）——避免 WM_PAINT 晚于 request（ε>0）导致
/// 定时器唤醒时 now-last_render = I-ε < I 恒拦截 → 渲染频率减半（2I 间隔）。
#[allow(dead_code)] // the tests in this file call it
pub(crate) fn should_request_redraw(last_request: std::time::Instant, now: std::time::Instant, interval: std::time::Duration) -> bool {
    now.duration_since(last_request) >= interval
}

