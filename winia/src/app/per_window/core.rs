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
    pub(crate) pressed_interaction: Option<(u64, crate::input::interaction::MutableInteractionSource)>,
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
        crate::debug::animation::begin_frame(
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

