use super::*;

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

