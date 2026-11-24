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

        // Consume the pending queue: create the windows.
        while self.pending_content.len() > 0 {
            if crate::debug::is_shutdown() {
                self.pending_content.clear();
                break;
            }
            let pending = self.pending_content.remove(0);
            self.open_window(event_loop, pending);
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

    pub(crate) fn open_window(&mut self, event_loop: &dyn ActiveEventLoop, pending: PendingWindow) {
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
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
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
        // A window re-opened with the same key picks up what the last one saved. `created_id` is
        // deliberately NOT a fallback: it is minted fresh for every window, so using it would key
        // saved state by "the Nth window ever opened" and quietly restore the wrong data. A window
        // without a key keeps its state in memory only.
        let memory_key = pending.saveable_key;
        let persist_key = pending.persist_key;
        if memory_key.is_some() || persist_key.is_some() {
            // In-memory first (a window re-opened in this process), then whatever a previous RUN
            // left on disk under the APPLICATION's key. Both are best-effort: a missing or corrupt
            // file is just an empty map.
            let restored = memory_key
                .and_then(|key| self.saveable.remove(&key))
                .or_else(|| persist_key.and_then(crate::runtime::saveable::persist::load_saved))
                .unwrap_or_default();
            pw.saveable_key = memory_key;
            pw.persist_key = persist_key;
            pw.composer.set_saveable_registry(
                crate::runtime::saveable::SaveableStateRegistry::new(restored),
            );
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
        if let Some(created_id) = pw.created_id {
            pw.composer.set_created_id(created_id);
        }
        // The declaration minted the id before the platform created anything; this is the frame the
        // window becomes real, so the live state adopts what the platform actually chose.
        if let Some(created_id) = pw.created_id {
            crate::app::window_state::realize(
                crate::app::window_state::WindowId::new(created_id),
                (pw.width, pw.height),
                &title,
            );
        }
        // The first compose+layout+draw gets a Density too (so Px units are right from frame one).
        let density = crate::runtime::density::density_with_system_font_scale(sf as f32);
        crate::runtime::density::with_density(density, || {
            // The first frame also gets the window size injected (adaptive components are correct immediately) and the reactive State attached.
            pw.composer
                .set_adaptive_window_size(pending.width, pending.height);
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
            pw.composer
                .layout(Constraints::new(0.0, pending.width, 0.0, pending.height));
            let bg = pw.theme.applied.colors.background;
            if let Some(root_idx) = pw.composer.layout_root_idx() {
                let nodes = pw.composer.arena_nodes();
                if let Some(ref mut sw) = pw.skia_window {
                    let sf2 = sf as f32;
                    sw.draw(|surface| {
                        let canvas = surface.canvas();
                        canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
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
