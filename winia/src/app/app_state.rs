use super::*;

// ── AppState ──

pub(crate) struct AppState {
    /// 已创建的窗口
    pub(crate) windows: HashMap<WindowId, PerWindow>,
    /// Saveable values of windows that have closed, keyed by `created_id`: a window recreated with
    /// the same id picks its state back up.
    pub(crate) saveable: HashMap<u64, HashMap<String, crate::runtime::saveable::SavedValue>>,
    /// 待创建的窗口（由 Window composable 排队）
    pub(crate) pending_content: Vec<PendingWindow>,
    /// 父窗口 ID（用于 is_parent 判断，不依赖 HashMap 顺序）
    pub(crate) parent_window_id: Option<WindowId>,
    /// 初始化回调（仅首次调用，用于声明式创建主窗口）
    pub(crate) init: Option<Box<dyn FnOnce(&mut ComposeCtx)>>,
    /// 上轮动画是否活跃（停止时强制终帧渲染）
    pub(crate) was_animating: bool,
}

impl ApplicationHandler for AppState {
        fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        // Wait + request_redraw 自驱动动画（避免 Poll↔Wait 切换竞态丢帧）
        event_loop.set_control_flow(ControlFlow::Wait);
        // 每轮推进动画（与窗口解耦，多窗口/子窗口动画均正确推进）
        // 动画（水波纹/状态层过渡也注册在全局动画列表——自动驱动重绘）
        let animating = crate::animation::update_animations();
        if animating {
            // 动画活跃：WaitUntil 定时唤醒（对齐刷新率）保证每帧唤醒（不冻结），
            // request 节流（距上次渲染 >= 帧间隔）限制 WM_PAINT 生成频率——
            // 修复 request 每轮发送 → WM_PAINT 消息唤醒 Wait 的 55k/s 自驱动空转
            let interval = self.windows.values().map(|pw| pw.clock.frame_interval).min()
                .unwrap_or(std::time::Duration::from_millis(16));
            event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + interval));
            let now = std::time::Instant::now();
            for pw in self.windows.values_mut() {
                if now.duration_since(pw.clock.last_request_time) >= pw.clock.frame_interval {
                    pw.clock.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
        } else if self.was_animating {
            // 动画刚停止：强制终帧渲染（最后一次 set 的 request 可能被跳过）
            for pw in self.windows.values_mut() {
                if let Some(ref sw) = pw.skia_window {
                    pw.clock.force_redraw = true; // 仅可渲染窗口设（避免 None 残留）
                    sw.request_redraw();
                }
            }
        }
        // 兜底：pending 存在（State 已变化待重组——点击/异步回调）或渲染欠账
        // （force_redraw——帧节流跳过的更新）时请求重绘。节流命中不丢弃：
        // WaitUntil 到下一个可用时刻重试，保证一次性更新不卡到外部事件
        let now = std::time::Instant::now();
        let mut retry_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            // 渲染欠账按 last_render 对齐（force_redraw 由帧节流跳过时设置）
            if pw.clock.force_redraw {
                if now.duration_since(pw.clock.last_render_time) >= pw.clock.frame_interval {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    let d = pw.clock.last_render_time + pw.clock.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
            // pending state 按 last_request 节流（动画持续 pending 时每帧至多一次）
            if pw.composer.has_pending_states() {
                if now.duration_since(pw.clock.last_request_time) >= pw.clock.frame_interval {
                    pw.clock.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    let d = pw.clock.last_request_time + pw.clock.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
        }
        if let Some(d) = retry_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // 延迟 tap 超时补发（Compose detectTapGestures：onDoubleTap 存在时
        // onTap 延迟触发）——超时立即补发；未超时用 WaitUntil 定时唤醒
        // （idle 时也能在双击窗口结束后补发，不依赖下一次输入事件）
        let now = std::time::Instant::now();
        let mut next_tap_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            let mut kept = Vec::new();
            for t in std::mem::take(&mut pw.input.pending_taps) {
                if t.deadline <= now {
                    pw.fire_pending_tap(t);
                } else {
                    let d = t.deadline;
                    kept.push(t);
                    next_tap_deadline = Some(match next_tap_deadline {
                        Some(e) => e.min(d),
                        None => d,
                    });
                }
            }
            pw.input.pending_taps = kept;
        }
        if let Some(d) = next_tap_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // Long press fires AT its deadline, while the finger is still down (Compose
        // `detectTapGestures`: `onLongPress` arrives during the press, not on release). The tracker
        // is polled here, and the `WaitUntil` below is what wakes an idle window at that moment.
        //
        // Nothing is duplicated for it: the gesture, its slot and its arena are already on the
        // window, so the deadline only has to be remembered until it passes.
        let mut next_long_press: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            let fired = pw.input.tracker.as_mut().and_then(|tracker| tracker.poll_long_press(now));
            if let Some(action) = fired {
                let (slot, arena) = (pw.input.slot, pw.input.arena);
                if let Some(slot) = slot {
                    let fired_callback = fire_in_gesture_arena(pw, arena, slot, action);
                    if fired_callback {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                }
            }
            if let Some(deadline) = pw.input.tracker.as_ref().and_then(|tracker| tracker.long_press_deadline()) {
                next_long_press = Some(match next_long_press {
                    Some(existing) => existing.min(deadline),
                    None => deadline,
                });
            }
        }
        if let Some(d) = next_long_press {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // DevTools 事件兜底消费：只遍历有 queued events 的窗口，避免每轮
        // 事件批次空转全部窗口；legacy target 0 只会匹配 parent。
        let targets = debug::queued_event_targets();
        if !targets.is_empty() {
            let windows: Vec<WindowId> = self.windows.keys().copied().collect();
            for wid in windows {
                if targets.contains(&(wid.into_raw() as u64)) {
                    self.consume_debug_events(wid);
                }
            }
        }
        // Actions the accessibility bridge queued: UIA calls a provider on its own schedule, so the
        // provider only records what was asked for and this — the frame loop — performs it, running
        // the same callback a real click or Tab would.
        if crate::semantics::has_actions() {
            let windows: Vec<WindowId> = self.windows.keys().copied().collect();
            for wid in windows {
                self.consume_ui_actions(wid);
            }
        }
        self.was_animating = animating;
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        AppState::process_pending_windows(self, event_loop);
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
                // 如果 debug server 请求关闭，退出事件循环
        if debug::is_shutdown() {
            event_loop.exit();
            return;
        }
        AppState::process_pending_windows(self, event_loop);
        // 消费 pending close（on_remove 推入，compose 末尾也消费一次）
        crate::app::window::Window::process_detached(
            &mut self.windows,
            event_loop,
            &|| debug::force_shutdown(),
            &mut self.saveable,
        );
        // Redraw requests raised from OUTSIDE the window they concern: a `Window` node publishing a new
        // theme intent runs in its DECLARING tree, which leaves the window's own composer with nothing
        // pending — without this its frame would never be scheduled and the new theme would sit unseen.
        for created_id in take_redraw_requests() {
            for pw in self.windows.values() {
                if pw.created_id == Some(created_id) {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
        }
        // Debug 工具有 pending 请求时唤醒窗口（截图/模拟事件需要 RedrawRequested）
        if debug::has_pending() {
            for pw in self.windows.values() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
        // The mode moved (from an application or a background thread): every window has to look at it, and
        // a window whose content an application composed itself has no dependency to be woken by.
        if crate::theme::take_theme_redraw_all() {
            for pw in self.windows.values() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
        // 异步 State 变更唤醒事件循环后需要 request_redraw（节流：距上次 request
        // 够帧间隔才发——动画每帧 notify 触发 proxy_wake_up，无节流会渲染风暴）
        let now = std::time::Instant::now();
        let mut retry_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            if pw.composer.has_pending_states() {
                if now.duration_since(pw.clock.last_request_time) >= pw.clock.frame_interval {
                    pw.clock.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    // 节流命中不丢弃：到期重试（一次性更新不卡到外部事件）
                    let d = pw.clock.last_request_time + pw.clock.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
        }
        if let Some(d) = retry_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_parent = self.parent_window_id.map(|p| p == window_id).unwrap_or(false);
        let closing_last = self.windows.len() == 1;
        // winit reports the system theme per WINDOW (winit-win32 raises it from `WM_SETTINGCHANGE`), while
        // the change itself is process-wide: record it, and schedule the frame of every window — a window
        // that is not redrawn never notices (each one re-resolves against the epoch in its own frame).
        if let WindowEvent::ThemeChanged(theme) = &event {
            crate::theme::note_platform_theme(matches!(theme, winit::window::Theme::Dark));
            for pw in self.windows.values_mut() {
                if let Some(ref sw) = pw.skia_window {
                    sw.request_redraw();
                }
            }
            return;
        }
        let Some(pw) = self.windows.get_mut(&window_id) else { return };

        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let mut wheel = match delta {
                    // LineDelta's 20 is logical px per line, matching the scroll offsets below. One
                    // notch is discrete, so it pages as one (`WheelKind::Notch`).
                    winit::event::MouseScrollDelta::LineDelta(x, y) => WheelDelta {
                        dx: x * 20.0,
                        dy: y * 20.0,
                        kind: wheel_kind_for_lines(x, y),
                    },
                    // `PixelDelta` is a PHYSICAL position (winit), and everything downstream is
                    // logical, so it converts like the pointer positions do. Without this a
                    // touchpad scroll moved `density` times too far on a HiDPI display. Only
                    // platforms that emit PixelDelta (macOS/Linux touchpads) reached it — the
                    // win32 backend sends LineDelta — so the conversion is unverified here.
                    //
                    // Its events are continuous, so `WheelPager` decides when they add up to a page.
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        let lp = p.to_logical::<f32>(pw.scale_factor);
                        WheelDelta { dx: lp.x, dy: lp.y, kind: WheelKind::Pixel }
                    }
                    // `MouseScrollDelta` is `#[non_exhaustive]` in winit 0.31 (it was marked so in
                    // 0.31.0-beta.3), so a future variant must not break this match: an unknown
                    // delta scrolls nothing rather than being guessed at.
                    _ => WheelDelta { dx: 0.0, dy: 0.0, kind: WheelKind::Notch },
                };
                // Shift + 垂直滚轮 → 转为水平滚动（兼容 LazyRow 等横向容器；
                // 多数系统不会自动把 Shift+wheel 翻译成 dx，这里显式处理）
                let (dx, dy) = scroll_delta_with_shift(wheel.dx, wheel.dy, pw.modifiers.shift_key());
                wheel.dx = dx;
                wheel.dy = dy;
                if dx != 0.0 || dy != 0.0 {
                    // Overlays float ABOVE the main tree and own separate arenas, so a wheel over one has
                    // to scroll ITS container — a menu's list — rather than whatever the main tree has
                    // under the same point. This is the routing pointer events already use (`hit_overlay`),
                    // and without it a scrollable menu could not be scrolled at all: the main-tree search
                    // below cannot see into another arena (measured: the last item stayed at y=920 in a
                    // 520px window no matter how much the wheel moved).
                    let overlay_target = pw.input.last_pos.and_then(|(px, py)| {
                        let (i, (lx, ly)) = hit_overlay(pw, (px, py))?;
                        let ov = &pw.overlay.layers[i];
                        let r = ov.composer.layout_root_idx()?;
                        let nodes = ov.composer.arena_nodes();
                        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), lx, ly);
                        // The overlay's own arena, the main tree's rule: a menu's list pages exactly
                        // like a pager in the main tree, and a horizontal pager inside the menu takes
                        // the vertical wheel too.
                        wheel_scroll_target(nodes, &path, wheel.dx, wheel.dy).map(|t| (i, t, r))
                    });
                    let now = std::time::Instant::now();
                    let density = crate::unit::Density::from_density(pw.scale_factor as f32);
                    if let Some((i, target, r)) = overlay_target {
                        let ov = &mut pw.overlay.layers[i];
                        apply_wheel_to_target(
                            ov.composer.arena_nodes_mut(),
                            r,
                            target,
                            OVERLAY_WHEEL_SPACE_BASE + i as u64,
                            wheel,
                            &mut pw.wheel,
                            density,
                            now,
                        );
                    } else if let Some(root_idx) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes();
                        // §3.7 修复：优先按鼠标位置 hit-test，在命中路径上找滚动
                        // 目标——两个并排滚动区域只滚鼠标悬停的那一个（旧实现
                        // find_scroll_target 按反向 child 顺序盲找，可能滚错）。
                        // 用最后 PointerMoved 位置（winit 0.31 MouseWheel 事件
                        // 不携带 cursor position）。
                        // ⚠ 真实滚轮**没有**盲找兜底：鼠标不在任何滚动节点上时
                        // 不滚动（否则鼠标在空白处滚轮也会带动列表/Shift+横滚，
                        // 用户实测 bug）。`None`（启动后无移动/已离开窗口）也
                        // 不滚——盲找只留给 DebugEvent::Scroll（测试注入无坐标，
                        // 见 consume_debug_events）。
                        let target = pw.input.last_pos.and_then(|(px, py)| {
                            let path = crate::layout::node::hit_test_with_flights(nodes, root_idx, pw.composer.transition_roots(), px, py);
                            // 命中路径从根到叶——从内向外找第一个轴匹配的 scroll 节点
                            wheel_scroll_target(nodes, &path, wheel.dx, wheel.dy)
                        });
                        if let Some(target) = target {
                            // Paging, pixel scrolling and the edge pulse all live in the shared
                            // helper; see `wheel_page_step` and `apply_wheel_to_target`.
                            apply_wheel_to_target(
                                pw.composer.arena_nodes_mut(),
                                root_idx,
                                target,
                                MAIN_TREE_WHEEL_SPACE,
                                wheel,
                                &mut pw.wheel,
                                density,
                                now,
                            );
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            WindowEvent::CloseRequested => {
                if let Some(ref mut cb) = pw.on_close { cb(); }
                // Snapshot saveable state while the composition is still alive to provide it.
                if let Some(key) = pw.saveable_key {
                    if let Some(registry) = pw.composer.saveable_registry() {
                        let saved = registry.perform_save();
                        if !saved.is_empty() {
                            crate::runtime::saveable::persist::save_saved(key, &saved);
                            self.saveable.insert(key, saved);
                        }
                    }
                }
                if let Some(ref sw) = pw.skia_window {
                    crate::accessibility::uninstall(&**sw, window_id.into_raw() as u64);
                }
                self.windows.remove(&window_id);
                // 窗口没了，它的帧时钟一并丢掉（否则每开一个窗口注册表就多一份）。
                crate::effect::forget_window(window_id.into_raw() as u64);
                // 清理 debug 树条目（窗口关闭后不再渲染——残留会让 UI 测试误判）
                debug::remove_tree(window_id.into_raw() as u64);
                // 通知其他窗口重绘（状态可能已变化）
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if closing_last || is_parent {
                    debug::force_shutdown();
                    event_loop.exit();
                }
            }
            WindowEvent::Destroyed => {
                if let Some(cid) = pw.created_id { crate::app::window::CREATED.lock().unwrap().remove(&cid); }
                if let Some(ref sw) = pw.skia_window {
                    crate::accessibility::uninstall(&**sw, window_id.into_raw() as u64);
                }
                self.windows.remove(&window_id);
                debug::remove_tree(window_id.into_raw() as u64);
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            // 跨屏移动：Moved 高频触发（拖动过程）——内部 300ms 去抖
            WindowEvent::Moved { .. } => {
                pw.refresh_frame_interval();
            }
            WindowEvent::ThemeChanged(_) => {
                // Handled before the per-window lookup — see the top of this function: the report is
                // process-wide, so one window's event has to schedule every window's frame.
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                pw.scale_factor = scale_factor;
                // 换显示器可能伴随缩放变化——顺带刷新帧间隔（去抖在方法内）
                pw.refresh_frame_interval();
                // Phase 4.1：scale 变化必须触发重组——TextUnit::Px/Dimension::Px
                // 在组合期用 current_density() 转换写入 modifier，若 build 不重跑
                // 则 px 值保留旧 density 结果（文本/尺寸不随 DPI 缩放）。
                // 全量重组（needs_recomposition=true）——DPI 变化罕见，代价可接受。
                pw.composer.request_recomposition(0);
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::PointerButton { position, state, button, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                // Down/Up 也刷新指针位置（点击后不移动直接滚轮时，
                // last_pointer_pos 否则还是旧值——命中目标错位）
                pw.input.last_pos = Some(scene_pos);
                let event_type = if state.is_pressed() {
                    crate::input::PointerEventType::Down
                } else {
                    crate::input::PointerEventType::Up
                };
                if state.is_pressed() {
                    // ── Down：指针按下核心（共享——真实/Debug 防分叉）──
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, scene_pos, crate::input::PointerKind::from_button_source(&button), &modifiers, true);
                }
                // ── Up：Compose 风格 click 检测（仅释放时——Down 保留
                // pointer_down_state 供拖动选择；无条件执行会 Down 后立即 take
                // 掉 state → 拖动无法选择）──
                // ⚠ 必须位于 `if state.is_pressed()`（Down 块）之外：Up 事件（Released）
                // 不进入 Down 块——嵌套在块内的 click 检测对 Up 事件永不执行
                // （曾导致真实点击永远无效，而 [sel-up-clean]（块外）正常打印）
                if !state.is_pressed() {
                    // 显式请求重绘：on_click 内的 State set 走 wake_up 链路（异步），
                    // 若无 pending 检查兜底会漏刷新（用户看到 count 不变）
                    if detect_click(pw, scene_pos) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 点击执行（down 命中 overlay 时记录——up 触发；
                    // 主树 detect_click 因 down 短路未记录 pointer_down_state 而空转）
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 拖拽结束（up）
                    if overlay_drag_up(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 手势 up 判定（tap/double-tap/long-press/drag-end）
                    if gesture_up(pw, scene_pos) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 拖拽滚动结束：速度足够 → fling（惯性滚动）
                    drag_scroll_up(pw);
                    // 释放按下交互（Compose Release 语义——clickable 按下态结束）
                    release_pressed_interaction(pw);
                }
                // ── 指针事件分发（Up 时先分发后清除 capture）──
                let nodes = pw.composer.arena_nodes();
                if let Some(r) = pw.composer.layout_root_idx() {
                    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
                    let ptr_ev = crate::input::PointerEvent {
                        event_type,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: crate::input::PointerKind::from_button_source(&button),
                        is_alt_pressed: pw.modifiers.alt_key(),
                        is_ctrl_pressed: pw.modifiers.control_key(),
                        is_shift_pressed: pw.modifiers.shift_key(),
                        is_meta_pressed: pw.modifiers.meta_key(),
                    };
                    pw.input.last_kind = ptr_ev.kind.clone();
                    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
                }
                // Up 后清除 capture + 通知选区变化
                if !state.is_pressed() {
                    // Compose 方式：从拖拽节点 slot_key 取 registrar 直接 fire
                    if let Some(slot) = pw.input.down_slot {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                                    if let Some(reg) = nodes[idx].registrar.borrow().as_ref() {
                                        reg.fire_on_change();
                                    }
                                }
                            }
                        }
                    }
                    pw.input.down_slot = None;
                    // 防御：take 可能未执行（click 检测分支外的边界）——强制清 state
                    pw.input.down = None;
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                event_loop.set_control_flow(ControlFlow::Poll);
                let nodes = pw.composer.arena_nodes();
                if let Some(r) = pw.composer.layout_root_idx() {
                    let fid = crate::layout::node::get_focus_id(nodes, r);
                    let slot = fid.and_then(|id| crate::layout::node::find_node_by_id(nodes, r, id).map(|idx| nodes[idx].slot_key));
                    pw.focused_id = fid;
                    pw.focused_slot_key = slot;
                }
                if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
            }
            WindowEvent::PointerMoved { position, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                // 记录最后指针位置（MouseWheel 命中滚动目标用——§3.7；
                // winit 0.31 MouseWheel 事件不带 cursor position）
                pw.input.last_pos = Some(scene_pos);
                // 指针移动核心（共享——真实/Debug 防分叉；Debug 路径此前缺
                // x_off 对齐偏移——Center/Right 对齐文本选择错位，合并修复）
                let modifiers = pw.modifiers;
                let consumed = handle_pointer_move(pw, scene_pos, pw.input.last_kind.clone(), &modifiers);
                // 消费（on_pointer_event 可能更新 State）或按下拖动选区时请求重绘；
                // 未消费的悬停移动不唤醒事件循环（避免每帧白醒）
                if consumed || pw.input.down.is_some() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            // 指针离开窗口：清所有 hover（对每个 hoverable 补发 Exit）+
            // 清指针位置（否则离开窗口后滚轮仍用旧坐标命中滚动容器，
            // 鼠标已不在上面却还能滚动——用户实测 bug）
            WindowEvent::PointerLeft { .. } => {
                let olds: Vec<u64> = pw.hovered_slots.drain().collect();
                for old in olds {
                    exit_hover_at(pw, old);
                }
                pw.input.last_pos = None;
                update_cursor(pw, crate::input::PointerIcon::DEFAULT);
            }
            WindowEvent::ModifiersChanged(m) => {
                pw.modifiers = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event_type = if event.state.is_pressed() {
                    crate::input::KbEventType::KeyDown
                } else {
                    crate::input::KbEventType::KeyUp
                };
                let ke = crate::input::KbEvent {
                    key: event.logical_key.clone(),
                    event_type,
                    is_alt_pressed: pw.modifiers.alt_key(),
                    is_ctrl_pressed: pw.modifiers.control_key(),
                    is_shift_pressed: pw.modifiers.shift_key(),
                    is_meta_pressed: pw.modifiers.meta_key(),
                    repeat: event.repeat,
                };
                let mut consumed = false;
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {
                    consumed = pw.escape_key();
                }
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    pw.tab_move_focus(pw.modifiers.shift_key());
                    consumed = true;
                }
                if !consumed {
                    consumed = dispatch_key_to_overlay(pw, &ke);
                }
                if !consumed {
                    // The page is out of reach while a focus-scope overlay is up: it keeps its focused
                    // node (and its ring comes back when the overlay closes), but keys it would have
                    // handled — typing into a field behind the scrim — stop here instead.
                    if !focus_scope_is_open(pw) {
                        consumed = dispatch_key_to_focus(pw, &ke);
                    }
                }
                // 方向键焦点导航（对标 Compose Desktop arrow-key navigation）——
                // 在目标方向半平面内选"方向距离 + 垂直偏离×2"最小的可聚焦节点
                if !consumed && event.state.is_pressed() && !event.repeat {
                    let dir = match &event.logical_key {
                        Key::Named(NamedKey::ArrowRight) => Some(FocusDir::Right),
                        Key::Named(NamedKey::ArrowLeft) => Some(FocusDir::Left),
                        Key::Named(NamedKey::ArrowDown) => Some(FocusDir::Down),
                        Key::Named(NamedKey::ArrowUp) => Some(FocusDir::Up),
                        _ => None,
                    };
                    if let Some(dir) = dir {
                        if pw.focus_move_directional(dir) {
                            consumed = true;
                        }
                    }
                }
                if consumed {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
            WindowEvent::Ime(ime) => {
                use winit::event::Ime;
                match ime {
                    Ime::Preedit(text, cursor) => {
                        // Focused overlay first (independent composer — main-tree
                        // lookup can never reach overlay nodes), then main tree.
                        // ⚠ TextField 容器化：焦点在容器、ime_callback 在输入 leaf——
                        // 只查焦点节点自身则 Preedit 永远到不了（预输入不显示）。
                        // 从焦点节点向下找第一个 ime_callback。
                        let overlay_target: Option<(usize, usize)> = (0..pw.overlay.layers.len()).rev().find_map(|i| {
                            let ov = &pw.overlay.layers[i];
                            if ov.closing { return None; }
                            let fid = ov.focused_id?;
                            let nodes = ov.composer.arena_nodes();
                            let r = ov.composer.layout_root_idx()?;
                            let idx = crate::layout::node::find_node_by_id(nodes, r, fid)?;
                            let ime_idx = find_descendant_ime_callback(nodes, idx)?;
                            Some((i, ime_idx))
                        });
                        if let Some((i, ime_idx)) = overlay_target {
                            let nodes = pw.overlay.layers[i].composer.arena_nodes();
                            if let Some(cb) = nodes[ime_idx].ime_callback.borrow_mut().as_mut() {
                                cb(&text, cursor);
                            }
                        } else if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                    if let Some(ime_idx) = find_descendant_ime_callback(nodes, idx) {
                                        if let Some(cb) = nodes[ime_idx].ime_callback.borrow_mut().as_mut() {
                                            cb(&text, cursor);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    Ime::Commit(text) => {
                        // IME 提交文本——派发给聚焦节点的 on_key_event 以 Character 形式.
                        // Focused overlay first, then main tree (same routing rule).
                        let overlay_target: Option<usize> = (0..pw.overlay.layers.len()).rev().find(|&i| {
                            let ov = &pw.overlay.layers[i];
                            if ov.closing { return false; }
                            ov.focused_id.and_then(|fid| {
                                ov.composer.layout_root_idx().and_then(|r| {
                                    crate::layout::node::find_node_by_id(ov.composer.arena_nodes(), r, fid)
                                })
                            }).is_some()
                        });
                        if let Some(i) = overlay_target {
                            // 逐字符发送 (overlay arena — bubble-only, mirroring main)
                            for ch in text.chars() {
                                let s = ch.to_string();
                                let ke = crate::input::KbEvent {
                                    key: winit::keyboard::Key::Character(s.clone().into()),
                                    event_type: crate::input::KbEventType::KeyDown,
                                    is_alt_pressed: false, is_ctrl_pressed: false,
                                    is_shift_pressed: false, is_meta_pressed: false,
                                    repeat: false,
                                };
                                let ov = &pw.overlay.layers[i];
                                if let Some(fid) = ov.focused_id {
                                    let nodes = ov.composer.arena_nodes();
                                    if let Some(r) = ov.composer.layout_root_idx() {
                                        let mut path: Vec<usize> = Vec::new();
                                        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                            path.push(idx);
                                            let mut pid = nodes[idx].parent_id;
                                            while let Some(id) = pid {
                                                if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                                                    path.push(anc); pid = nodes[anc].parent_id;
                                                } else { break; }
                                            }
                                            path.reverse();
                                        }
                                        for &ni in path.iter().rev() {
                                            let mut consumed = false;
                                            for el in nodes[ni].modifier.elements().iter().rev() {
                                                if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                                    if handler(&ke) { consumed = true; break; }
                                                }
                                            }
                                            if consumed { break; }
                                        }
                                    }
                                }
                            }
                            if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        } else if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                // 逐字符发送
                                for ch in text.chars() {
                                    let s = ch.to_string();
                                    let ke = crate::input::KbEvent {
                                        key: winit::keyboard::Key::Character(s.clone().into()),
                                        event_type: crate::input::KbEventType::KeyDown,
                                        is_alt_pressed: false, is_ctrl_pressed: false,
                                        is_shift_pressed: false, is_meta_pressed: false,
                                        repeat: false,
                                    };
                                    let mut path: Vec<usize> = Vec::new();
                                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                        path.push(idx);
                                        let mut pid = nodes[idx].parent_id;
                                        while let Some(id) = pid {
                                            if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                                                path.push(anc); pid = nodes[anc].parent_id;
                                            } else { break; }
                                        }
                                        path.reverse();
                                    }
                                    for &ni in path.iter().rev() {
                                        let mut consumed = false;
                                        for el in nodes[ni].modifier.elements().iter().rev() {
                                            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                                if handler(&ke) { consumed = true; break; }
                                            }
                                        }
                                        if consumed { break; }
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                            }
                        }
                    }
                    Ime::Enabled | Ime::Disabled => {}
                    _ => {}
                }
            }
            WindowEvent::SurfaceResized(s) => {
                // 直接采纳事件尺寸。旧版对 >50% 变化做忽略（防 winit #2094 陈旧
                // 事件）——但最大化必然 >50%，整个最大化路径被吞（尺寸/布局全部
                // 卡旧值）。陈旧事件改由 RedrawRequested 的每帧 inner_size 校准自愈。
                let l = s.to_logical::<f32>(pw.scale_factor);
                pw.width = l.width; pw.height = l.height;
                if let Some(ref mut sw) = pw.skia_window { sw.resize(); }
                // 不设 force_redraw：拖拽时 SurfaceResized 以鼠标速率（~125Hz）到达，
                // 无条件渲染会绕过帧节流（实测 3.6ms/帧）。帧节流的"渲染欠账"机制
                // （跳过时自设 force_redraw）保证更新不丢；request_redraw 保持链路
                // （OS 拖拽期间持续 WM_PAINT + 自驱，帧距 ≥16ms）
                // 尺寸 State set() 通知——依赖方（NavigationSuiteScaffold 等
                // 读 window_size() 的 slot）标记 dirty，增量重组切换形态
                if let Some(s) = pw.window_size_state.borrow().as_ref() {
                    s.set((l.width, l.height));
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                // 帧时钟 tick：全局的（with_frame_nanos，任意窗口的帧）和本窗口的
                // （ctx.frame_clock()，只被自己的帧唤醒）。
                crate::effect::frame_tick();
                crate::effect::FrameClock::for_window(window_id.into_raw() as u64).tick();
                // 尺寸自愈校准：以 winit 实际 surface 尺寸为准。winit #2094 类
                // 陈旧事件（最大化时旧尺寸事件可能后到）由此在下一帧纠正——
                // 偏差 >0.5 逻辑像素即更新 pw + 通知尺寸 State（套件形态随之刷新）
                if let Some(ref sw) = pw.skia_window {
                    let phys = sw.surface_size();
                    let l = phys.to_logical::<f32>(pw.scale_factor);
                    if (l.width - pw.width).abs() > 0.5 || (l.height - pw.height).abs() > 0.5 {
                        pw.width = l.width;
                        pw.height = l.height;
                        if let Some(s) = pw.window_size_state.borrow().as_ref() {
                            s.set((l.width, l.height));
                        }
                    }
                }
                // 动画推进（冗余于 new_events——Windows 拖拽 resize 是模态循环，
                // new_events/AboutToWait 被阻塞不触发；WM_PAINT 驱动的 RedrawRequested
                // 是拖拽中唯一持续到达的事件——在此 tick 才有"边拖边动"的动画。
                // 与 new_events 双 tick 无害：Animatable 按实际 dt 推进）
                crate::animation::update_animations();
                // 消费焦点请求（在 compose 前处理，避免丢失）
                let _focus_window = pw.composer.focus_window(window_id.into_raw() as u64);
                for id in pw.composer.take_focus_requests() {
                    // Main tree first; on miss try overlays topmost-first (overlay
                    // inputs request focus via on_press exactly like main-tree ones,
                    // but the old code only resolved requests against the main arena,
                    // silently dropping every overlay focus request).
                    let mut main_hit = false;
                    // While a focus scope owns the keyboard the page is out of reach, and that has to
                    // hold for a focus REQUEST too — not only for keys. A component asking for focus
                    // behind a scrim would take it (and turn its IME back on) while the modal is up.
                    if !focus_scope_is_open(pw) {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            let nodes = pw.composer.arena_nodes_mut();
                            if crate::layout::node::focus_by_id(nodes, r, id) {
                                pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                                main_hit = true;
                            }
                        }
                    }
                    if main_hit {
                        // Single-focus invariant: main focus clears overlay foci.
                        for ov in pw.overlay.layers.iter_mut() {
                            if ov.focused_id.is_some() {
                                if let Some(r) = ov.composer.layout_root_idx() {
                                    crate::layout::node::clear_focus(ov.composer.arena_nodes_mut(), r);
                                }
                                ov.focused_id = None;
                                ov.focused_slot_key = None;
                            }
                        }
                    } else {
                        for i in (0..pw.overlay.layers.len()).rev() {
                            let hit = {
                                let ov = &mut pw.overlay.layers[i];
                                if ov.closing {
                                    false
                                } else if let Some(r) = ov.composer.layout_root_idx() {
                                    crate::layout::node::focus_by_id(ov.composer.arena_nodes_mut(), r, id)
                                } else {
                                    false
                                }
                            };
                            if hit {
                                let (fid, fslot, want) = {
                                    let ov = &pw.overlay.layers[i];
                                    let nodes = ov.composer.arena_nodes();
                                    let fid = ov.composer.layout_root_idx()
                                        .and_then(|r| crate::layout::node::get_focus_id(nodes, r));
                                    let (fslot, want) = fid.and_then(|f| {
                                        crate::layout::node::find_node_by_id(nodes, ov.composer.layout_root_idx()?, f)
                                    }).map(|found| {
                                        (nodes[found].slot_key, node_or_descendant_wants_ime(nodes, found))
                                    }).unwrap_or((0, false));
                                    (fid, fslot, want)
                                };
                                pw.overlay.layers[i].focused_id = fid;
                                pw.overlay.layers[i].focused_slot_key = fid.map(|_| fslot);
                                // Single-focus invariant: overlay focus clears main
                                // focus and sibling overlay foci.
                                if pw.focused_id.is_some() {
                                    if let Some(r) = pw.composer.layout_root_idx() {
                                        crate::layout::node::clear_focus(pw.composer.arena_nodes_mut(), r);
                                    }
                                    pw.focused_id = None;
                                    pw.focused_slot_key = None;
                                }
                                for (j, other) in pw.overlay.layers.iter_mut().enumerate() {
                                    if j != i && other.focused_id.is_some() {
                                        if let Some(r) = other.composer.layout_root_idx() {
                                            crate::layout::node::clear_focus(other.composer.arena_nodes_mut(), r);
                                        }
                                        other.focused_id = None;
                                        other.focused_slot_key = None;
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window {
                                    set_ime_enabled(&sw, want);
                                }
                                break;
                            }
                        }
                    }
                    // 单独查 slot_key（避免与 root 的 borrow 冲突）
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(found) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                pw.focused_slot_key = Some(nodes[found].slot_key);
                                // IME 按组件声明开关：文本组件（TextField）在组合期
                                // set_current_node_ime_callback 主动声明 IME 需求；
                                // 框架只做机械转发，不判断组件类型。Button 等未声明
                                // ime_callback 的 focusable 聚焦时不开启输入法。
                                // ⚠ TextField 容器化：焦点在容器、ime_callback 在输入
                                // leaf——须向下找子树（node_or_descendant_wants_ime）
                                let wants_ime = node_or_descendant_wants_ime(nodes, found);
                                if let Some(ref sw) = pw.skia_window {
                                    set_ime_enabled(&sw, wants_ime);
                                }
                            }
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                // 增量重组 → 布局 → 渲染
                // 帧率限制：Windows 上 acquire_next_image 不阻塞 vsync（实测 1-2ms），
                // 无节流会 ~1300fps 渲染风暴（present fence 只等 GPU 提交不等显示刷新）。
                // 距上次渲染 <16ms（~60fps）跳过——动画值下轮渲染时取最新（不丢帧）。
                let now = std::time::Instant::now();
                if pw.clock.render_disabled {
                    // 崩溃边界（P3-3）：连续 panic 后停更——保留最后画面
                    return;
                }
                if !pw.clock.force_redraw && now.duration_since(pw.clock.last_render_time) < pw.clock.frame_interval {
                    // 帧节流命中：不丢弃本次更新——记渲染欠账（force_redraw），
                    // 下一个可用帧由 new_events 补发 request_redraw。原实现直接
                    // 跳过：一次性状态变更（如双击延迟 tap）会卡到外部事件才刷新
                    pw.clock.force_redraw = true;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(pw.clock.last_render_time + pw.clock.frame_interval));
                } else {
                pw.clock.last_render_time = now;
                // 崩溃边界（P3-3）：compose/layout/draw 任一段 panic（用户 content 代码 /
                // skia 异常）不崩窗口——捕获后跳过本帧（保留上帧画面），下帧正常重试。
                // 连续 panic 计数防风暴：超过阈值打印错误并停更（不再自旋）。
                // 渲染路径无 unsafe（P2-3 后）——catch_unwind 后继续用 self 仅"逻辑不一致"
                // 非内存不安全；slot/arena 每帧从 root 重建结构，panic 中断的半状态下帧自愈。
                let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let wid = window_id.into_raw() as u64;
                    // Built only when something consumes it (the debug server or the accessibility
                    // bridge): it is a full tree walk per frame, and `publishing_enabled` is a const,
                    // so a plain build never pays for it.
                    let mut main_nodes = Vec::new();
                    pw.recompose_layout_render(window_id, |nodes, root_idx, _surface| {
                        debug::update_tree(wid, &debug::build_tree_json(nodes, root_idx));
                        if crate::semantics::publishing_enabled() {
                            main_nodes = crate::semantics::semantics_tree(nodes, root_idx);
                        }
                    });
                    // overlay 独立 Composer 的 arena 同样进调试树（modal/popup 可观测；
                    // 每帧整体替换——overlay 关闭后条目自动消失）。z 序 = pw.overlay.layers
                    // 栈序，与 render_overlays 绘制顺序一致。
                    let ov_trees: Vec<(u64, (f32, f32), String)> = pw.overlay.layers.iter().filter_map(|ov| {
                        ov.composer.layout_root_idx()
                            .map(|r| (ov.id, ov.screen_pos, debug::build_tree_json(ov.composer.arena_nodes(), r)))
                    }).collect();
                    debug::set_overlay_trees(wid, ov_trees);
                    if crate::semantics::publishing_enabled() {
                        // The overlays' arenas are walked too, so a modal's contents are in the tree a
                        // screen reader descends. Each keeps its screen origin: its tree is in the
                        // overlay's own coordinates.
                        let overlays = pw.overlay.layers.iter().filter_map(|ov| {
                            ov.composer.layout_root_idx().map(|r| crate::semantics::OverlaySemantics {
                                id: ov.id,
                                origin: ov.screen_pos,
                                nodes: crate::semantics::semantics_tree(ov.composer.arena_nodes(), r),
                            })
                        }).collect();
                        // The two numbers a platform bridge needs to turn logical bounds into
                        // physical ones. Where the window sits on screen is the bridge's own business
                        // — it has the handle, and the answer changes whenever the user moves it.
                        let snapshot = crate::semantics::WindowSemantics {
                            main: main_nodes,
                            overlays,
                            window_size: (pw.width, pw.height),
                            scale_factor: pw.scale_factor as f32,
                        };
                        // Tell the platform bridge what changed since the last frame — it raises the
                        // structure/focus notifications its clients listen for, and does nothing at
                        // all when no client is listening (feature `accessibility`; a no-op otherwise).
                        crate::accessibility::notify(wid, &snapshot);
                        crate::semantics::publish(wid, snapshot);
                    }
                }));
                match panic_result {
                    Ok(()) => {
                        pw.clock.force_redraw = false; // 渲染成功后才清除强制帧（中途异常保留）
                        pw.clock.consecutive_panics = 0;
                        // 动画自驱动兜底：渲染中注册的动画依赖 wake_up 启动下一轮，
                        // 但 Windows 上从事件处理内调用 EventLoopProxy::wake_up 偶发丢失
                        // （winit 已知竞态）→ 动画冻结在起始值，直到下一个外部事件
                        // （鼠标移动）才用大 dt 一次收敛。显式 request_redraw 保证
                        // 动画帧持续推进，不依赖那次可能丢失的唤醒。
                        if crate::animation::is_animating() {
                            if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        }
                    }
                    Err(e) => {
                        pw.clock.consecutive_panics += 1;
                        let msg = if let Some(s) = e.downcast_ref::<&str>() { (*s).to_string() }
                                  else if let Some(s) = e.downcast_ref::<String>() { s.clone() }
                                  else { "unknown panic".to_string() };
                        eprintln!("[render-panic] 第 {} 次连续 panic（本帧已跳过，上帧画面保留）: {}", pw.clock.consecutive_panics, msg);
                        if pw.clock.consecutive_panics >= 30 {
                            eprintln!("[render-panic] 连续 30 次 panic——停止本窗口渲染（避免 panic 风暴）");
                            pw.clock.render_disabled = true;
                        }
                        // force_redraw 保持 true——下帧继续尝试（若未停更）
                    }
                }
                // IME 光标区域更新（输入法候选框跟随光标位置）
                if let Some(ref sw) = pw.skia_window {
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            // ⚠ TextField 容器化：焦点在容器、paragraph/cursor 在输入
                            // leaf——须向下找 ime_callback 节点（find_descendant_ime_callback）；
                            // 只查焦点节点则容器无 paragraph → 候选框区域从不更新
                            let pidx = crate::layout::node::find_node_by_id(nodes, r, fid)
                                .and_then(|idx| find_descendant_ime_callback(nodes, idx));
                            if let Some(pidx) = pidx {
                                if let Ok(borrow) = nodes[pidx].cached_paragraph.try_borrow() {
                                    if let Some(p) = borrow.as_ref() {
                                        let abs = node_abs_position(nodes, r, nodes[pidx].id);
                                        // 内容区偏移（渲染侧 content_x = 节点原点 +
                                        // padding——IME 区域须与其对称，否则预选
                                        // 窗口整体偏 padding 偏移）
                                        let (pad_s, pad_t, pad_e, _) = nodes[pidx].modifier.get_padding_sides();
                                        let pad_x = if nodes[pidx].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                                        // 空文本：无 glyph 可定位——IME 区域放内容
                                        // 起点（行高近似；与渲染端空文本光标一致）
                                        let empty = nodes[pidx].modifier.content_len() == 0;
                                        // 光标索引是编辑偏移——经映射转显示偏移
                                        // （TextFieldVisual 挂在容器——向上找）
                                        let caret_idx = crate::text::field::offset_mapping_for_node(nodes, r, pidx)
                                            .map(|m| m.original_to_transformed(nodes[pidx].cursor_index.get()))
                                            .unwrap_or_else(|| nodes[pidx].cursor_index.get());
                                        let (cx, cy, ch) = if empty {
                                            (0.0, 0.0, 20.0)
                                        } else {
                                            let tl = crate::text::TextLayout::new(
                                                p,
                                                p.paragraph_byte_to_real_indices.len(),
                                            );
                                            match tl.get_cursor_position(caret_idx) {
                                                Some(v) => v,
                                                None => (0.0, 0.0, 20.0),
                                            }
                                        };
                                        let align = nodes[pidx].modifier.align().unwrap_or(crate::text::TextAlign::Left);
                                        // CONTENT box: the painter uses it, so the IME
                                        // candidate window must too — under a flight layout
                                        // override `measured_size` is the size the PARENT was
                                        // told (review R3-F6).
                                        let node_w = nodes[pidx].content_box().width;
                                        let intrinsic_w = p.max_intrinsic_width();
                                        // ⚠ 空文本：渲染端光标画在内容起点（左对齐，
                                        // 不随 align 偏移）——IME 区域须一致（否则
                                        // Center/Right 对齐时空文本输入候选框在容器
                                        // 中央而非光标处）
                                        let x = if empty {
                                            (abs.0 + pad_x) as f64
                                        } else {
                                            let x_off = match align {
                                                crate::text::TextAlign::Left | crate::text::TextAlign::Justify => abs.0,
                                                crate::text::TextAlign::Center => abs.0 + (node_w - intrinsic_w).max(0.0) / 2.0,
                                                crate::text::TextAlign::Right => abs.0 + (node_w - intrinsic_w).max(0.0),
                                            };
                                            (x_off + pad_x + cx) as f64
                                        };
                                        let y = (abs.1 + pad_t + cy) as f64;
                                        let _ = sw.request_ime_update(
                                            winit::window::ImeRequest::Update(
                                                winit::window::ImeRequestData::default()
                                                    .with_cursor_area(
                                                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(x, y)),
                                                        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(2.0, ch as f64)),
                                                    )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                } // end 帧率限制 else（渲染 + IME 同步）
                // 检查 compose 后是否有待关闭窗口
                if pw.composer.pending_window_close_id().is_some() {
                    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
                }
                // DevTools 事件消费（仅父窗口）——new_events 也兜底调用（见下）
                if is_parent {
                    self.consume_debug_events(window_id);
                }
            }
            _ => {}
        }
    }
}

impl AppState {
    /// 消费 DevTools 注入事件（点击/按键/滚动/拖拽等——UI 测试 + WS 调试）。
    /// 在 window_event（RedrawRequested）与 new_events（兜底）两处调用：
    /// 多窗口下主窗口可能在后台——RedrawRequested 不来时事件卡队列，
    /// Perform the actions a platform bridge (or anything else off the UI thread) queued for this
    /// window. Invoke runs the node's real click callback; Focus moves the ring to the node.
    pub(crate) fn consume_ui_actions(&mut self, window_id: WindowId) {
        let wid = window_id.into_raw() as u64;
        let actions = crate::semantics::take_actions(wid);
        if actions.is_empty() {
            return;
        }
        let Some(pw) = self.windows.get_mut(&window_id) else { return };
        for action in actions {
            match action {
                crate::semantics::UiAction::Invoke(node_id) => {
                    // The arena is borrowed immutably and the callback only sets state, which is what
                    // a real click does too.
                    let nodes = pw.composer.arena_nodes();
                    if !crate::semantics::click_node_by_id(nodes, node_id) {
                        log::debug!("[accessibility] invoke: node {node_id} has no click action");
                    }
                }
                crate::semantics::UiAction::Focus(node_id) => {
                    if !pw.focus_by_node_id(node_id) {
                        log::debug!("[accessibility] focus: node {node_id} cannot take focus");
                    }
                }
            }
        }
        if let Some(ref sw) = pw.skia_window {
            sw.request_redraw();
        }
        debug::wake();
    }

    /// new_events 每轮事件批次必然执行——保证注入事件不丢失。
    pub(crate) fn consume_debug_events(&mut self, window_id: WindowId) {
        let Some(pw) = self.windows.get_mut(&window_id) else { return };
        let mut handled = false;
        for evt in debug::take_queued_events(window_id.into_raw() as u64) {
            match evt {
                debug::DebugEvent::Click { x, y } => {
                    // overlay 优先（对齐真实指针路径 handle_pointer_down）：
                    // 命中 overlay → 消费（不进主树）；外部点击 → dismiss。
                    // ⚠ Click 是合成单事件（非 down/up 分离）——overlay 命中后
                    // 必须立即执行点击（真实路径 down 记录 + up 触发；这里
                    // down 记录后直接 exec，否则 overlay 按钮永远点不动）
                    let click_kind = pw.input.last_kind.clone();
                    if overlay_down(pw, (x, y), click_kind) {
                        // 命中 overlay：立即执行点击（合成单事件——down 记录 +
                        // 立即 up 触发；真实路径由 PointerUp 事件触发）
                        exec_overlay_click(pw);
                        // The synthetic click is down + up in one event, so it has to close the
                        // gesture the down opened, exactly as the real up path does
                        // (`exec_overlay_click` → `gesture_up`): otherwise a tap in a popup would
                        // leave its tracker (and any deferred tap) behind, and `on_tap` would never
                        // fire for a harness click.
                        gesture_up(pw, (x, y));
                        // 释放 overlay 的 press（合成事件无真实 up——否则按下
                        // 波纹/按压态卡死到 overlay 关闭）
                        release_pressed_interaction(pw);
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        handled = true;
                        continue;
                    }
                    // 只读阶段：hit_test + click 检测（arena 借用在块尾结束）
                    let (fid, sk, _path_len, _click_handled) = {
                        let arena = pw.composer.arena_nodes();
                        let root_idx = pw.composer.layout_root_idx();
                        if let Some(r) = root_idx {
                            let path = hit_test_with_flights(arena, r, pw.composer.transition_roots(), x, y);
                            // debug 点击聚焦：path 中最深的 focusable 节点
                            // （text-field-v2 容器化后焦点/交互在容器——空字段
                            // 输入子节点 0 宽点不中；旧逻辑只看 ime_callback
                            // 已失效）；兼容声明 IME 的节点（旧 leaf 语义）
                            // NOTE: deliberately BROADER than the real overlay path
                            // (`overlay_down`, which focuses only a field that holds an input):
                            // a synthesized click has to be able to put focus anywhere, because
                            // that is how a following `k <char>` finds a target. Do not narrow
                            // this to match — see `clicking_an_overlay_button_does_not_steal_focus`.
                            let (fid, sk) = path.iter().rev().find_map(|&i| {
                                let has_focusable = arena[i].modifier.elements().iter()
                                    .any(|el| matches!(el, crate::modifier::ModifierElement::Focusable { .. }));
                                let wants_ime = arena[i].ime_callback.borrow().is_some();
                                (has_focusable || wants_ime).then_some((arena[i].id, arena[i].slot_key))
                            }).unwrap_or((0, 0));
                            let path_len = path.len();
                            // Click 检测（P2-1：复用 fire_click_along_path，
                            // 与主树/overlay 同语义，防内联重复漂移）
                            let click_handled = fire_click_along_path(arena, &path);
                            if click_handled {
                                handled = true;
                            }
                            (fid, sk, path_len, click_handled)
                        } else { (0, 0, 0, false) }
                    };
                    if fid != 0 {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            let nodes = pw.composer.arena_nodes_mut();
                            crate::layout::node::clear_focus(nodes, r);
                            crate::layout::node::set_focus_by_id(nodes, r, fid);
                        }
                        pw.focused_id = Some(fid);
                        pw.focused_slot_key = Some(sk);
                        if let Some(ref sw) = pw.skia_window { set_ime_enabled(&sw, true); }
                    }
                    debug_log!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, _path_len, pw.scale_factor);
                    debug_log!("[debug-click] handled={} pos=({:.0},{:.0})", _click_handled, x, y);
                }
                debug::DebugEvent::Key { key } => {
                    if key == "Escape" {
                        // The same helper the real key path calls — see `PerWindow::escape_key`.
                        pw.escape_key();
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    } else if key == "Tab" {
                        // The SAME helper the real key path calls: this branch used to keep its own
                        // copy of the traversal rule, which silently went stale the moment the rule
                        // changed (it still walked the main tree while an overlay owned the keyboard,
                        // and a UI test driving Tab over the debug server would have measured that
                        // copy instead of the framework). Shift is not expressible in `k <key>`.
                        pw.tab_move_focus(false);
                    } else if let Some(k) = parse_debug_key(&key) {
                        // 任意按键：复用真实键盘派发路径（Preview/Bubble/激活）——
                        // WS 可模拟字符输入/删除/方向键。修饰键默认无（Ctrl 等
                        // 组合暂不支持——如需可扩展 KbEvent 修饰字段）
                        let ke = crate::input::KbEvent {
                            key: k,
                            event_type: crate::input::KbEventType::KeyDown,
                            is_alt_pressed: pw.modifiers.alt_key(),
                            is_ctrl_pressed: pw.modifiers.control_key(),
                            is_shift_pressed: pw.modifiers.shift_key(),
                            is_meta_pressed: pw.modifiers.meta_key(),
                            repeat: false,
                        };
                        if !dispatch_key_to_overlay(pw, &ke) && !focus_scope_is_open(pw) {
                            // Same gate as the real key path: a focus-scope overlay owns the keyboard,
                            // so a key it does not consume does not reach the page behind it either.
                            dispatch_key_to_focus(pw, &ke);
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    handled = true;
                }
                debug::DebugEvent::FocusNext => {
                    // The real traversal rule, not a copy: this arm used to walk the main tree only.
                    pw.tab_move_focus(false);
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    handled = true;
                }
                debug::DebugEvent::RequestFocus { id } => {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes_mut();
                        if crate::layout::node::focus_by_id(nodes, r, id) {
                            pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                            pw.focused_slot_key = pw.focused_id.and_then(|fid| crate::layout::node::find_node_by_id(nodes, r, fid).map(|idx| nodes[idx].slot_key));
                            pw.apply_ime_for_focus(pw.focused_id);
                            handled = true;
                        }
                    }
                    if !handled {
                        // Main tree missed — try overlays topmost-first (same
                        // single-focus clearing as the frame request path).
                        for i in (0..pw.overlay.layers.len()).rev() {
                            let hit = {
                                let ov = &mut pw.overlay.layers[i];
                                if ov.closing {
                                    false
                                } else if let Some(r) = ov.composer.layout_root_idx() {
                                    crate::layout::node::focus_by_id(ov.composer.arena_nodes_mut(), r, id)
                                } else {
                                    false
                                }
                            };
                            if hit {
                                let (fid, fslot, want) = {
                                    let ov = &pw.overlay.layers[i];
                                    let nodes = ov.composer.arena_nodes();
                                    let r = ov.composer.layout_root_idx();
                                    let fid = r.and_then(|r| crate::layout::node::get_focus_id(nodes, r));
                                    let (fslot, want) = fid.and_then(|f| {
                                        r.and_then(|r| crate::layout::node::find_node_by_id(nodes, r, f))
                                    }).map(|found| {
                                        (nodes[found].slot_key, node_or_descendant_wants_ime(nodes, found))
                                    }).unwrap_or((0, false));
                                    (fid, fslot, want)
                                };
                                pw.overlay.layers[i].focused_id = fid;
                                pw.overlay.layers[i].focused_slot_key = fid.map(|_| fslot);
                                if pw.focused_id.is_some() {
                                    if let Some(r) = pw.composer.layout_root_idx() {
                                        crate::layout::node::clear_focus(pw.composer.arena_nodes_mut(), r);
                                    }
                                    pw.focused_id = None;
                                    pw.focused_slot_key = None;
                                }
                                for (j, other) in pw.overlay.layers.iter_mut().enumerate() {
                                    if j != i && other.focused_id.is_some() {
                                        if let Some(r) = other.composer.layout_root_idx() {
                                            crate::layout::node::clear_focus(other.composer.arena_nodes_mut(), r);
                                        }
                                        other.focused_id = None;
                                        other.focused_slot_key = None;
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window {
                                    set_ime_enabled(&sw, want);
                                }
                                handled = true;
                                break;
                            }
                        }
                    }
                }
                debug::DebugEvent::PointerDown { x, y } => {
                    // 模拟指针按下：与真实 PointerButton Down 共用核心
                    // （with_focus=false——调试路径不做光标/聚焦）
                    pw.input.last_pos = Some((x, y));
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, (x, y), pw.input.last_kind.clone(), &modifiers, false);
                    handled = true;
                }
                debug::DebugEvent::PointerMove { x, y } => {
                    // 模拟拖动选择：与真实 PointerMoved 共用核心（含 x_off 对齐偏移）
                    pw.input.last_pos = Some((x, y));
                    let modifiers = pw.modifiers;
                    handle_pointer_move(pw, (x, y), pw.input.last_kind.clone(), &modifiers);
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                debug::DebugEvent::PointerUp { x, y } => {
                    // 模拟释放：先走真实 Up 的 click 检测（验证真实链路）
                    // 与真实路径（PointerButton Up 分支）一致：on_click 内
                    // State set 后必须 request_redraw——否则依赖 wake_up 异步
                    // 链（偶发丢失 → 用户看到 count 不刷新）
                    if detect_click(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 点击执行（与真实路径一致）
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 拖拽结束（与真实路径一致）
                    if overlay_drag_up(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 手势 up 判定（与真实路径一致）
                    if gesture_up(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 拖拽滚动结束：速度足够 → fling（与真实路径一致）
                    drag_scroll_up(pw);
                    // 释放按下交互（与真实路径一致）
                    release_pressed_interaction(pw);
                    // 通知选区变化 + 清理
                    if let Some(slot) = pw.input.down_slot {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                                    if let Some(reg) = nodes[idx].registrar.borrow().as_ref() {
                                        reg.fire_on_change();
                                    }
                                }
                            }
                        }
                    }
                    pw.input.down_slot = None;
                    pw.input.down = None;
                    handled = true;
                }
                debug::DebugEvent::Scroll { dx, dy } => {
                    // An injected wheel follows the same precedence as a real one: an overlay under the
                    // pointer first (a test moves the pointer with `m x y`), then the main tree's blind
                    // search — which is what makes a scrollable menu testable at all, since the blind
                    // search only ever sees the main tree's arena.
                    let overlay_target = pw.input.last_pos.and_then(|(px, py)| {
                        let (i, (lx, ly)) = hit_overlay(pw, (px, py))?;
                        let ov = &pw.overlay.layers[i];
                        let r = ov.composer.layout_root_idx()?;
                        let nodes = ov.composer.arena_nodes();
                        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), lx, ly);
                        wheel_scroll_target(nodes, &path, dx, dy).map(|t| (i, t, r))
                    });
                    // An injected wheel is one notch — `s` in the debug protocol — so it pages at once
                    // instead of banking travel the way a touchpad stream does.
                    let wheel = WheelDelta { dx, dy, kind: WheelKind::Notch };
                    let now = std::time::Instant::now();
                    let density = crate::unit::Density::from_density(pw.scale_factor as f32);
                    if let Some((i, target, r)) = overlay_target {
                        let ov = &mut pw.overlay.layers[i];
                        let outcome = apply_wheel_to_target(
                            ov.composer.arena_nodes_mut(),
                            r,
                            target,
                            OVERLAY_WHEEL_SPACE_BASE + i as u64,
                            wheel,
                            &mut pw.wheel,
                            density,
                            now,
                        );
                        handled = outcome.handled();
                    } else if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes();
                        // An injected wheel follows the REAL one when the test moved the pointer:
                        // hit-test at that position and take the first axis-matching node on the path
                        // (including a horizontal-only container, which takes the vertical wheel).
                        // Without the pointer it falls back to the blind search, which is what a test
                        // that never moves the mouse has to use.
                        let by_pointer = pw.input.last_pos.and_then(|(px, py)| {
                            let path = crate::layout::node::hit_test_with_flights(nodes, r, pw.composer.transition_roots(), px, py);
                            wheel_scroll_target(nodes, &path, dx, dy)
                        });
                        if let Some(target) = by_pointer.or_else(|| find_scroll_target(nodes, r, dx, dy)) {
                            // Same helper as the real wheel, so the two paths cannot drift again.
                            let outcome = apply_wheel_to_target(
                                pw.composer.arena_nodes_mut(),
                                r,
                                target,
                                MAIN_TREE_WHEEL_SPACE,
                                wheel,
                                &mut pw.wheel,
                                density,
                                now,
                            );
                            handled = outcome.handled();
                        }
                    }
                }
                debug::DebugEvent::Resize { w, h } => {
                    // 真实 resize：request_inner_size → WM_SIZE → SurfaceResized
                    // 事件自然回流——与拖拽/最大化完全同通路（pw 字段与尺寸 State
                    // 均由事件处理器统一更新，避免与 surface_size 自愈互相打架）
                    if let Some(ref sw) = pw.skia_window {
                        let _ = sw.request_surface_size(
                            winit::dpi::LogicalSize::new(w as f64, h as f64).into(),
                        );
                    }
                    handled = true;
                }
                _ => {}
            }
        }
        if handled { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
        if debug::has_pending() { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
    }
}

impl AppState {
    /// 消费 `app::open_window` 排队的窗口请求 + 首次初始化
    pub(crate) fn process_pending_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        // 首次：运行 init 回调收集主窗口创建请求
        if self.windows.is_empty() {
            if let Some(init) = self.init.take() {
                let mut composer = Composer::new();
                composer.compose(|ctx| init(ctx));
                // 临时 composer：compose 内部已 take_deps（注册到其 slot_deps）——
                // 此处再 take 是防御性空操作（缓冲已空），确保 DEP_MODE 复位
                crate::runtime::state::take_deps();
                // The temporary initialization Composer is not an active
                // window owner; its lifecycle state is dropped with it.
            }
        }

        for item in take_pending_windows() {
            self.pending_content.push(item);
        }

        // 消费 pending：创建窗口
        while self.pending_content.len() > 0 {
            // drain-like: pop from front
            let pending = self.pending_content.remove(0);
            self.open_window(event_loop, pending);
        }
    }

    pub(crate) fn open_window(&mut self, event_loop: &dyn ActiveEventLoop, pending: PendingWindow) {
        let mut a = winit::window::WindowAttributes::default();
        a.title = if pending.title.is_empty() { "Winia".into() } else { pending.title.clone() };
        a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(pending.width as f64, pending.height as f64)));
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
        let window_id = w.id();
        let sf = w.scale_factor();
        // 帧间隔对齐屏幕刷新率（refresh_rate_millihertz：60000 = 60Hz）——须在 w 移入前获取
        let mhz_opt = w.current_monitor()
            .and_then(|m| m.current_video_mode())
            .and_then(|v| v.refresh_rate_millihertz())
            .map(|m| m.get());
        let frame_interval = mhz_opt
            .map(|mhz| std::time::Duration::from_nanos(1_000_000_000_000 / mhz as u64))
            .unwrap_or(std::time::Duration::from_millis(16));
        // Which refresh rate the window was actually created under, and the interval derived from it:
        // the number every pacing decision below uses, and until now it was never printed (a 300 Hz
        // panel and a 60 Hz one differ by 5x here, so "how many frames did that animation cost" cannot
        // be read without it).
        debug_log!(
            "[refresh] create: monitor={}mHz interval={:?}",
            mhz_opt.map(|m| m.to_string()).unwrap_or_else(|| "unknown".into()),
            frame_interval
        );
        let skia_window = SkiaWindow::new(event_loop, w);
        // Publish this window to the OS accessibility layer (feature `accessibility`; a no-op
        // otherwise, and unconditional at the call site so nothing here needs feature gates).
        crate::accessibility::install(&*skia_window, window_id.into_raw() as u64);
        let content = pending.content.unwrap_or_else(|| Box::new(|_| {}));
        // How this window resolves its theme. A `Window` node passes the cell it shares with its declaring
        // tree; a window opened directly follows the system (`Auto`), which is what an unspecified theme
        // means everywhere else in the API.
        let theme = pending
            .theme
            .unwrap_or_else(|| crate::theme::WindowTheme::new(crate::theme::ThemeSpec::Auto));
        // The ONE place a window's content is wrapped in its theme: the declaring tree publishes the cell
        // before the first frame, and every frame after that composes under it. (`Window::build` used to do
        // this, which left windows opened through the public API unwrapped — their theme then only reached
        // the surface clear colour.)
        let content = {
            let theme = theme.clone();
            Box::new(move |ctx: &mut ComposeCtx| theme.provide(ctx, |ctx| content(ctx)))
                as Box<dyn Fn(&mut ComposeCtx)>
        };
        let mut pw = PerWindow::new(content, pending.width, pending.height, theme.colors());
        // What the window has drawn is what the cell says right now — its first frame is about to compose
        // with exactly this, and a default guess here would both re-run that frame for nothing and miss a
        // publish that changed a custom type scale back to the default.
        pw.theme.applied = theme.applied();
        pw.theme.cell = theme;
        pw.on_close = pending.on_close;
        pw.created_id = pending.created_id;
        // A window re-opened at the same declaration site picks up what the last one saved. The key
        // is the caller's stable one; `created_id` is a fallback for openers that have nothing better,
        // and it cannot survive a reopen on its own.
        let saveable_key = pending.saveable_key.or(pending.created_id);
        if let Some(key) = saveable_key {
            // In-memory first (a window re-opened in this process), then whatever a previous RUN
            // left on disk. Both are best-effort: a missing or corrupt file is just an empty map.
            let restored = self
                .saveable
                .remove(&key)
                .or_else(|| crate::runtime::saveable::persist::load_saved(key))
                .unwrap_or_default();
            pw.saveable_key = Some(key);
            pw.composer
                .set_saveable_registry(crate::runtime::saveable::SaveableStateRegistry::new(restored));
        }
        pw.scale_factor = sf;
        pw.skia_window = Some(skia_window);
        pw.clock.frame_interval = frame_interval;
        // FocusRequester created by this window's initial composition is bound
        // to this WindowId, just like later redraw compositions.
        let _focus_window = pw.composer.focus_window(window_id.into_raw() as u64);
        // The initial composition below runs BEFORE the first redraw, so the window id has to be set
        // here as well: an effect that captures ctx.frame_clock() on that pass would otherwise bind
        // to window 0 (which never ticks) and never be re-run.
        pw.composer.set_window_id(window_id.into_raw() as u64);
        // 首次 compose+layout+draw 也提供 Density（Px 单位首帧即正确）
        let density = crate::unit::Density::from_density(sf as f32);
        crate::runtime::density::with_density(density, || {
        // 首帧同样注入窗口尺寸（自适应组件首帧即正确形态）+ 挂载响应式 State
        pw.composer.set_adaptive_window_size(pending.width, pending.height);
        let slot = &pw.window_size_state;
        let (w0, h0) = (pending.width, pending.height);
        pw.composer.compose(|ctx| {
            let existing = slot.borrow().clone();
            let size_state = existing.unwrap_or_else(|| {
                ctx.key("winia_window_size_state", |ctx| {
                    let s = ctx.remember(|| (w0, h0));
                    *slot.borrow_mut() = Some(s.clone());
                    *pw.window_size_backchannel.borrow_mut() = Some(s.as_backchannel());
                    s
                })
            });
            if let Some(b) = pw.window_size_backchannel.borrow().as_ref() {
                crate::layout::adaptive::sync_window_size_state(b, (w0, h0));
            }
            crate::layout::adaptive::set_window_size_state(size_state);
            (pw.content)(ctx);
        });
        pw.composer.layout(Constraints::new(0.0, pending.width, 0.0, pending.height));
        let bg = pw.theme.applied.colors.background;
        if let Some(root_idx) = pw.composer.layout_root_idx() {
            let nodes = pw.composer.arena_nodes();
            if let Some(ref mut sw) = pw.skia_window {
                let sf2 = sf as f32;
                sw.draw(|surface| {
                    let canvas = surface.canvas(); canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
                    canvas.save(); canvas.scale((sf2, sf2)); render::render(nodes, root_idx, canvas); pw.composer.render_layer(canvas); canvas.restore();
                });
                sw.set_visible(true);
            }
        }
        });
        self.windows.insert(window_id, pw);
        if self.parent_window_id.is_none() {
            self.parent_window_id = Some(window_id);
            crate::debug::set_legacy_target(window_id.into_raw() as u64);
        }
    }
}

