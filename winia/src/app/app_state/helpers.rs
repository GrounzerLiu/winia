use super::*;

impl AppState {
    /// Drain the window requests queued by `app::open_window`, plus the root composition.
    pub(crate) fn process_pending_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        if crate::debug::is_shutdown() {
            return;
        }
        if self.root.is_none() {
            let mut composer = Composer::new();
            #[cfg(target_os = "android")]
            composer.set_android_app(self.platform.android_app.clone());
            if let Some(init) = self.init.as_ref() {
                composer.compose(|ctx| init(ctx));
            }
            self.root = Some(composer);
        } else if let Some(init) = self.init.as_ref() {
            if let Some(root) = self.root.as_mut() {
                root.recompose(|ctx| init(ctx));
            }
        }
        // `compose` already took the deps into its slot_deps; this clears any leftover buffer.
        crate::runtime::state::take_deps();

        // Window-content declarations can disappear before their pending platform window exists.
        crate::app::window::Window::process_detached(
            &mut self.windows,
            event_loop,
            &|| crate::debug::force_shutdown(),
            &mut self.saveable,
        );
        if crate::debug::is_shutdown() {
            return;
        }

        // A root declaration that disappeared closes its own window. Doing this by active-id diff
        // (rather than the slot forget callback) works even when several root declarations share
        // one composer: a still-composed sibling must not keep the removed one alive.
        if let Some(active) = self.root.as_ref().map(|root| root.active_window_ids()) {
            for id in crate::app::window::Window::stale_pending_root_ids(&active) {
                crate::app::window::Window::cancel_pending_subtree(id);
            }
            let stale: Vec<u64> = self
                .windows
                .values()
                .filter(|pw| pw.owner == Some(0))
                .filter_map(|pw| pw.created_id)
                .filter(|id| !active.contains(id))
                .collect();
            for id in stale {
                crate::app::window::Window::process_pending_remove(
                    &mut self.windows,
                    event_loop,
                    &|| crate::debug::force_shutdown(),
                    &mut self.saveable,
                    id,
                );
            }
        }

        for item in take_pending_windows() {
            self.pending_content.push(item);
        }

        // Create every native window hidden. The root recomposition above may also have queued
        // commands for already-live windows, so drain every window before any first frame runs.
        let mut initial_windows = Vec::new();
        let mut initial_ids = std::collections::HashSet::new();
        while self.pending_content.len() > 0 {
            if crate::debug::is_shutdown() {
                self.pending_content.clear();
                break;
            }
            let pending = self.pending_content.remove(0);
            if let Some((window_id, public_id)) = self.create_hidden_window(event_loop, pending) {
                initial_ids.insert(window_id);
                initial_windows.push((window_id, public_id));
            }
        }
        let mut batch = Vec::new();
        for (winit_id, pw) in &self.windows {
            if let Some(raw) = pw.created_id {
                for queued in crate::app::window_state::take_queued_commands(
                    crate::app::window_state::WindowId::new(raw),
                ) {
                    batch.push((queued.sequence, *winit_id, raw, queued.command));
                }
            }
        }
        batch.sort_by_key(|(sequence, _, _, _)| *sequence);
        let batch = defer_initial_visibility(batch, &initial_ids);
        self.apply_command_batch(event_loop, batch);
        // Capture visibility after pre-realization commands but before initial content runs.
        // Commands emitted by that first composition belong to the next frame.
        let initial_visibility: Vec<_> = initial_windows
            .into_iter()
            .map(|(window_id, public_id)| {
                let visible = public_id
                    .map(crate::app::window_state::visible_now)
                    .unwrap_or(true);
                (window_id, visible)
            })
            .collect();
        for (window_id, visible) in initial_visibility {
            self.finish_initial_window(window_id, visible);
        }
        // Initial compositions run above and can queue declaration removals. Reconcile them before
        // returning to either proxy_wake_up or can_create_surfaces.
        crate::app::window::Window::process_detached(
            &mut self.windows,
            event_loop,
            &|| crate::debug::force_shutdown(),
            &mut self.saveable,
        );
        self.refresh_parent_window_id();
    }

    fn create_hidden_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        pending: PendingWindow,
    ) -> Option<(
        winit::window::WindowId,
        Option<crate::app::window_state::WindowId>,
    )> {
        let mut a = winit::window::WindowAttributes::default();
        a.window_icon = crate::assets::window_icon();
        let title = if pending.title.is_empty() {
            "Winia".to_owned()
        } else {
            pending.title.clone()
        };
        a.title = title.clone();
        a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
            pending.width as f64,
            pending.height as f64,
        )));
        a.resizable = pending.options.resizable;
        a.decorations = pending.options.decorations;
        a.window_level = if pending.options.always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        };
        a.transparent = pending.options.transparent;
        if let Some((width, height)) = pending.options.minimum_size {
            a.min_surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                width as f64,
                height as f64,
            )));
        }
        if let Some((width, height)) = pending.options.maximum_size {
            a.max_surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                width as f64,
                height as f64,
            )));
        }
        let initial_position = pending
            .created_id
            .map(|raw| {
                crate::app::window_state::requested_position(
                    crate::app::window_state::WindowId::new(raw),
                )
            })
            .unwrap_or(pending.position);
        a.position = match initial_position {
            crate::app::window_state::WindowPosition::At(x, y) => Some(
                winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(x as f64, y as f64)),
            ),
            crate::app::window_state::WindowPosition::Default => None,
        };
        a.maximized = pending.placement == crate::app::window_state::WindowPlacement::Maximized;
        a.fullscreen = if pending.placement == crate::app::window_state::WindowPlacement::Fullscreen
        {
            Some(winit::monitor::Fullscreen::Borderless(None))
        } else {
            None
        };
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
        if pending.placement == crate::app::window_state::WindowPlacement::Minimized {
            let _ = winit::window::Window::set_minimized(&**w, true);
        }
        let window_id = w.id();
        let sf = w.scale_factor();
        // Match the frame interval to the screen refresh rate (refresh_rate_millihertz: 60000 = 60 Hz) — must be read before `w` is moved.
        let mhz_opt = w
            .current_monitor()
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
            mhz_opt
                .map(|m| m.to_string())
                .unwrap_or_else(|| "unknown".into()),
            frame_interval
        );
        let skia_window = SkiaWindow::new(event_loop, w);
        // Publish this window to the OS accessibility layer (feature `accessibility`; a no-op
        // otherwise, and unconditional at the call site so nothing here needs feature gates).
        crate::accessibility::install(&*skia_window, window_id.into_raw() as u64);
        let content = pending
            .created_id
            .map(crate::app::window_state::WindowId::new)
            .and_then(crate::app::window_state::take_pending_content)
            .or(pending.content)
            .unwrap_or_else(|| Box::new(|_| {}));
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
        pw.transparent = pending.options.transparent;
        if let Some(raw) = pending.created_id {
            let insets = crate::app::window_state::WindowId::new(raw).insets();
            pw.insets = insets.clone();
            pw.composer.set_window_insets(insets);
        }
        #[cfg(target_os = "android")]
        {
            pw.composer
                .set_android_app(self.platform.android_app.clone());
            pw.sync_android_insets();
        }
        // What the window has drawn is what the cell says right now — its first frame is about to compose
        // with exactly this, and a default guess here would both re-run that frame for nothing and miss a
        // publish that changed a custom type scale back to the default.
        pw.theme.applied = theme.applied();
        pw.theme.cell = theme;
        pw.on_close = pending.on_close;
        pw.created_id = pending.created_id;
        pw.owner = pending.owner;
        // Saveable restore belongs to the first content composition, not native creation. A
        // window destroyed by a pre-realization command never composed content and must not publish
        // an empty snapshot over the previous run's saved state.
        pw.saveable_key = pending.saveable_key;
        pw.persist_key = pending.persist_key;
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
        if let Some(created_id) = pw.created_id {
            pw.composer.set_created_id(created_id);
        }
        self.windows.insert(window_id, pw);
        if self.parent_window_id.is_none() {
            self.parent_window_id = Some(window_id);
            crate::debug::set_legacy_target(window_id.into_raw() as u64);
        }
        Some((
            window_id,
            pending
                .created_id
                .map(crate::app::window_state::WindowId::new),
        ))
    }

    /// Compose and reveal a hidden window only after its pre-realization commands have been applied.
    fn finish_initial_window(&mut self, window_id: winit::window::WindowId, visible: bool) {
        let Some(mut pw) = self.windows.remove(&window_id) else {
            return;
        };
        let sf = pw.scale_factor;
        let public_id = pw.created_id.map(crate::app::window_state::WindowId::new);
        if let Some(ref skia) = pw.skia_window {
            let size = skia.surface_size().to_logical::<f32>(sf);
            pw.width = size.width;
            pw.height = size.height;
            if let Some(public_id) = public_id {
                crate::app::window_state::realize(public_id, (pw.width, pw.height));
                crate::app::window_state::observe_platform_state(&**skia, public_id, sf);
            }
        }
        if pw.saveable_key.is_some() || pw.persist_key.is_some() {
            let restored = pw
                .saveable_key
                .and_then(|key| self.saveable.remove(&key))
                .or_else(|| {
                    pw.persist_key
                        .and_then(crate::runtime::saveable::persist::load_saved)
                })
                .unwrap_or_default();
            pw.composer.set_saveable_registry(
                crate::runtime::saveable::SaveableStateRegistry::new(restored),
            );
        }
        let density = crate::runtime::density::density_with_system_font_scale(sf as f32);
        crate::runtime::density::with_density(density, || {
            pw.composer.set_adaptive_window_size(pw.width, pw.height);
            let slot = &pw.window_size_state;
            let (w0, h0) = (pw.width, pw.height);
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
            pw.composer
                .layout(Constraints::new(0.0, pw.width, 0.0, pw.height));
            let bg = pw.theme.applied.colors.background;
            if let Some(root_idx) = pw.composer.layout_root_idx() {
                let nodes = pw.composer.arena_nodes();
                if let Some(ref mut sw) = pw.skia_window {
                    let sf2 = sf as f32;
                    sw.draw(|surface| {
                        let canvas = surface.canvas();
                        let clear = if pw.transparent {
                            skia_safe::Color::TRANSPARENT
                        } else {
                            skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b)
                        };
                        canvas.clear(clear);
                        canvas.save();
                        canvas.scale((sf2, sf2));
                        render::render_with_menu(
                            nodes,
                            root_idx,
                            canvas,
                            pw.input.selection_menu.as_ref(),
                            (pw.width, pw.height),
                        );
                        pw.composer.render_layer(canvas);
                        canvas.restore();
                    });
                    if visible {
                        sw.set_visible(true);
                    }
                }
            }
        });
        self.windows.insert(window_id, pw);
    }

    /// Keep the debug/legacy parent bound to a window that still exists.
    pub(crate) fn refresh_parent_window_id(&mut self) {
        let replacement = Self::replacement_parent_window_id(self.parent_window_id, &self.windows);
        if replacement != self.parent_window_id {
            self.parent_window_id = replacement;
            if let Some(id) = replacement {
                crate::debug::set_legacy_target(id.into_raw() as u64);
            }
        }
    }

    fn replacement_parent_window_id(
        parent: Option<WindowId>,
        windows: &std::collections::HashMap<WindowId, crate::app::PerWindow>,
    ) -> Option<WindowId> {
        if parent.is_some_and(|id| !windows.contains_key(&id)) {
            windows
                .iter()
                .filter(|(_, pw)| pw.owner.is_none() || pw.owner == Some(0))
                .min_by_key(|(_, pw)| pw.creation_order)
                .map(|(window_id, _)| *window_id)
        } else {
            parent
        }
    }
}

/// Visibility for a window that has not drawn its first frame is represented by its tracked
/// request snapshot, not by an early native set_visible call.
fn defer_initial_visibility(
    batch: Vec<(
        u64,
        winit::window::WindowId,
        u64,
        crate::app::window_state::Command,
    )>,
    initial_ids: &std::collections::HashSet<winit::window::WindowId>,
) -> Vec<(
    u64,
    winit::window::WindowId,
    u64,
    crate::app::window_state::Command,
)> {
    batch
        .into_iter()
        .filter(|(_, window_id, _, command)| {
            !(initial_ids.contains(window_id)
                && matches!(command, crate::app::window_state::Command::Visible(_)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn test_window_id(raw: u64) -> winit::window::WindowId {
        unsafe { std::mem::transmute::<usize, winit::window::WindowId>(raw as usize) }
    }

    #[cfg(not(windows))]
    fn test_window_id(raw: u64) -> winit::window::WindowId {
        winit::window::WindowId::from(raw)
    }

    fn window_with_owner(
        created_id: Option<u64>,
        creation_order: u64,
        owner: Option<u64>,
    ) -> PerWindow {
        let mut pw = PerWindow::new(
            Box::new(|_| {}),
            100.0,
            100.0,
            crate::theme::ThemeColors::default_light(),
        );
        pw.created_id = created_id;
        pw.owner = owner;
        pw.creation_order = creation_order;
        pw
    }

    #[test]
    fn initial_visibility_commands_wait_for_the_first_draw() {
        let initial = test_window_id(1);
        let existing = test_window_id(2);
        let batch = vec![
            (
                1,
                initial,
                10,
                crate::app::window_state::Command::Visible(true),
            ),
            (
                2,
                existing,
                20,
                crate::app::window_state::Command::Visible(false),
            ),
            (
                3,
                initial,
                10,
                crate::app::window_state::Command::Title("early".into()),
            ),
        ];
        let planned = defer_initial_visibility(batch, &std::collections::HashSet::from([initial]));
        assert_eq!(planned.len(), 2);
        assert_eq!(planned[0].1, existing);
        assert_eq!(planned[1].1, initial);
        assert!(matches!(
            planned[1].3,
            crate::app::window_state::Command::Title(_)
        ));
    }

    #[test]
    fn closing_the_parent_rebinds_to_the_earliest_remaining_root() {
        let first = test_window_id(1);
        let later_root = test_window_id(2);
        let child = test_window_id(3);
        let earliest_root = test_window_id(4);
        let platform_root = test_window_id(5);
        let mut windows = std::collections::HashMap::new();
        windows.insert(child, window_with_owner(Some(30), 30, Some(20)));
        windows.insert(later_root, window_with_owner(Some(20), 20, Some(0)));
        windows.insert(earliest_root, window_with_owner(Some(10), 10, Some(0)));
        windows.insert(platform_root, window_with_owner(None, 5, None));

        assert_eq!(
            AppState::replacement_parent_window_id(Some(first), &windows),
            Some(platform_root),
            "a child must never become the legacy parent"
        );
        assert_eq!(
            AppState::replacement_parent_window_id(Some(earliest_root), &windows),
            Some(earliest_root)
        );
        assert_eq!(
            AppState::replacement_parent_window_id(Some(first), &std::collections::HashMap::new()),
            None
        );
    }
}
