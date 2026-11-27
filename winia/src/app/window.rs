use crate::app;
use crate::app::window_state::{self, WindowId, WindowPlacement, WindowSeed};
use crate::composable;
use crate::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Key salt for the Window placeholder leaf / created_id (the golden ratio, keeping it in a separate
/// key space from content nodes' next_key).
const WINDOW_KEY_SALT: u64 = 0x9E37_79B9_7F4A_7C15;

/// Sub-window content wrapper. It is #[composable] because framework-internal composition points
/// follow the stable-key rules too: app.rs's process_pending_windows composes this closure directly
/// with an empty STMT_STACK, and without the macro's scope it would trip the stable-key panic.
#[composable]
fn sub_window_content(ctx: &mut ComposeCtx, content: &impl Fn(&mut ComposeCtx)) {
    // Each window owns its inset controller: the custom decoration (or the platform backend) writes
    // the window's status-bar/navigation-bar/IME insets into it, and the inset-padding modifiers read
    // them at measure time.
    let insets = ctx.window_insets();
    crate::window::insets::LOCAL_WINDOW_INSETS.provides(insets, || {
        // The content fills the window with no default padding (full-screen pages such as Scaffold
        // need edge-to-edge).
        Column::new().build(ctx, |ctx| {
            content(ctx);
        });
    });
}

/// The set of window IDs created so far, globally.
pub(crate) static CREATED: LazyLock<Mutex<HashSet<u64>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// IDs whose platform window was destroyed while their declaration still composes.
///
/// Without this, `Window::build` sees the id missing from `CREATED` and mints a fresh window on the
/// very next frame, so `id.destroy()` would be undone before the application could observe it.
static DESTROYED: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

pub(crate) fn mark_destroyed(id: u64) {
    DESTROYED.lock().unwrap().insert(id);
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The declarative window builder.
pub struct Window {
    spec: WindowSpec,
    options: crate::app::WindowOptions,
    on_close: Option<Box<dyn FnMut() + Send>>,
    on_close_requested: Option<std::rc::Rc<dyn Fn(WindowId, window_state::CloseReason)>>,
    on_event: Option<std::rc::Rc<dyn Fn(WindowId, window_state::WindowEvent)>>,
    persist_key: Option<u64>,
    position: window_state::WindowPosition,
    placement: window_state::WindowPlacement,
    visible: bool,
}

/// The values a `Window` declaration carries. Renamed from the old `WindowState`, whose name now
/// belongs to the live state handle (`crate::app::window_state`).
pub struct WindowSpec {
    pub width: f32,
    pub height: f32,
    pub title: String,
}

impl Default for WindowSpec {
    fn default() -> Self {
        WindowSpec {
            width: 300.0,
            height: 200.0,
            title: String::new(),
        }
    }
}

impl Window {
    pub fn new() -> Self {
        Window {
            spec: WindowSpec::default(),
            options: crate::app::WindowOptions::default(),
            on_close: None,
            on_close_requested: None,
            on_event: None,
            persist_key: None,
            position: window_state::WindowPosition::Default,
            placement: window_state::WindowPlacement::Windowed,
            visible: true,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.spec.title = title.into();
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.spec.width = width;
        self.spec.height = height;
        self
    }

    /// Initial outer position in logical desktop coordinates. Unsupported platforms keep the
    /// platform-chosen position.
    pub fn position(mut self, position: window_state::WindowPosition) -> Self {
        self.position = position;
        self
    }

    /// Initial platform placement.
    pub fn placement(mut self, placement: window_state::WindowPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Whether the window should be revealed after its first drawn frame.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Whether the OS may resize the window (drag its edges).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.options.resizable = resizable;
        self
    }

    /// Whether the window keeps its title bar and border.
    pub fn decorations(mut self, decorations: bool) -> Self {
        self.options.decorations = decorations;
        self
    }

    /// Keep the window above normal windows when the platform supports it.
    pub fn always_on_top(mut self, always_on_top: bool) -> Self {
        self.options.always_on_top = always_on_top;
        self
    }

    /// Windows only: hide the window's taskbar button.
    ///
    /// This is a creation-time style choice. Other platforms accept and ignore the request;
    /// [`WindowState::skip_taskbar`](crate::app::window_state::WindowState::skip_taskbar) reports the
    /// normalized value.
    pub fn skip_taskbar(mut self, skip_taskbar: bool) -> Self {
        self.options.skip_taskbar = skip_taskbar;
        self
    }

    /// Windows only: use the `WS_EX_TOOLWINDOW` tool-window style.
    ///
    /// Tool windows are excluded from the ordinary taskbar and Alt-Tab flow. This is a creation-time
    /// style choice; other platforms accept and ignore it.
    pub fn tool_window(mut self, tool_window: bool) -> Self {
        self.options.tool_window = tool_window;
        self
    }

    /// Windows only: add `WS_EX_NOACTIVATE`.
    ///
    /// The window can be shown and clicked without becoming the foreground application. This also
    /// prevents ordinary keyboard focus, so enable it only for auxiliary surfaces. Other platforms
    /// accept and ignore the request.
    pub fn no_activate(mut self, no_activate: bool) -> Self {
        self.options.no_activate = no_activate;
        self
    }

    /// Windows only: register an application desktop toolbar. Other platforms keep a normal window.
    ///
    /// App-bar windows are always on top, skip the taskbar, use the tool-window style, are not
    /// resizable, and have no minimum/maximum size constraints. The shell owns their geometry, so
    /// conflicting runtime geometry, placement, stacking, decoration, and size writes are ignored.
    /// `no_activate` remains an independent choice. Unsupported platforms keep a normal window and
    /// report [`AppBarStatus::Unsupported`].
    pub fn app_bar(mut self, options: window_state::AppBarOptions) -> Self {
        self.options.app_bar = Some(options);
        self
    }

    /// Create a window whose surface may contain alpha.
    ///
    /// This is independent of [`decorations`](Self::decorations): a transparent window keeps its
    /// title bar unless the app also asks for `decorations(false)`. Platforms without transparency
    /// support drop the request, so [`WindowState::transparent`](crate::app::window_state::WindowState::transparent)
    /// then reports `false`.
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.options.transparent = transparent;
        self
    }

    /// Minimum inner size in logical pixels.
    pub fn minimum_size(mut self, width: f32, height: f32) -> Self {
        self.options.minimum_size = Some((width, height));
        self
    }

    /// Maximum inner size in logical pixels.
    pub fn maximum_size(mut self, width: f32, height: f32) -> Self {
        self.options.maximum_size = Some((width, height));
        self
    }

    pub fn on_close(mut self, f: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }

    /// A stable key for this window's saved state.
    ///
    /// Without one the window keeps its state in memory only: closing and re-opening it starts
    /// empty. The key has to be unique per window and stable across runs; it is hashed into the
    /// saveable store and also names the window-geometry record, so renaming it discards both.
    pub fn key(mut self, key: &str) -> Self {
        self.persist_key = Some(window_state::persist_key(key));
        self
    }

    /// Register a close-request notification.
    ///
    /// A notification, not a decision: agree by hiding the window
    /// (`state().set_visible(false)`), destroying it (`destroy()`), or dropping the declaration.
    /// Doing nothing keeps the window open. Without a handler, closing the window closes it.
    pub fn on_close_requested(
        mut self,
        f: impl Fn(WindowId, window_state::CloseReason) + 'static,
    ) -> Self {
        self.on_close_requested = Some(std::rc::Rc::new(f));
        self
    }

    /// Register a semantic window-event callback.
    ///
    /// The callback runs on the UI thread after the corresponding state has been updated. Only
    /// changed observations are delivered; close requests use [`Window::on_close_requested`].
    pub fn on_event(mut self, f: impl Fn(WindowId, window_state::WindowEvent) + 'static) -> Self {
        self.on_event = Some(std::rc::Rc::new(f));
        self
    }

    /// Called at the end of compose to check whether a window asked to close.
    /// Close one window whose declaration no longer exists in `owner`.
    pub(crate) fn process_pending_remove(
        windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
        event_loop: &dyn winit::event_loop::ActiveEventLoop,
        force_shutdown: &dyn Fn(),
        saveable: &mut std::collections::HashMap<
            u64,
            std::collections::HashMap<String, crate::runtime::saveable::SavedValue>,
        >,
        id: u64,
    ) {
        Self::close_detached_window(windows, event_loop, force_shutdown, id, saveable);
    }

    /// Finalize every window declared inside the window identified by `owner` (its platform
    /// `created_id`), recursively. Child declarations are removed from `CREATED` first so the
    /// gesture is a declaration removal, not a `destroy()` that leaves a tombstone.
    pub(crate) fn close_owned_children(
        windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
        saveable: &mut std::collections::HashMap<
            u64,
            std::collections::HashMap<String, crate::runtime::saveable::SavedValue>,
        >,
        owner: u64,
    ) {
        let children: Vec<(winit::window::WindowId, u64)> = windows
            .iter()
            .filter(|(_, pw)| pw.owner == Some(owner))
            .filter_map(|(window_id, pw)| pw.created_id.map(|id| (*window_id, id)))
            .collect();
        for (window_id, created_id) in children {
            let _ = CREATED.lock().unwrap().remove(&created_id);
            if let Some(mut pw) = windows.remove(&window_id) {
                pw.finalize(window_id, saveable);
            }
            // Recurse even when CREATED no longer contains the child: a half-torn-down child must
            // not strand its grandchildren.
            Self::close_owned_children(windows, saveable, created_id);
        }
        // Children still waiting in GLOBAL_PENDING have no PerWindow to find above.
        Self::cancel_pending_subtree(owner);
    }

    /// Remove a not-yet-created window and every pending descendant from the global creation queue.
    pub(crate) fn cancel_pending_subtree(root: u64) {
        let mut doomed = HashSet::from([root]);
        loop {
            let mut canceled = {
                let mut pending = crate::app::GLOBAL_PENDING.lock().unwrap();
                let mut kept = Vec::with_capacity(pending.len());
                let mut canceled: Vec<crate::app::PendingWindow> = Vec::new();
                for item in pending.drain(..) {
                    let remove = item.created_id.is_some_and(|id| doomed.contains(&id))
                        || item.owner.is_some_and(|id| doomed.contains(&id));
                    if remove {
                        if let Some(id) = item.created_id {
                            doomed.insert(id);
                        }
                        canceled.push(item);
                    } else {
                        kept.push(item);
                    }
                }
                *pending = kept;
                canceled
            };
            if canceled.is_empty() {
                break;
            }
            // A never-realized window still has an after-the-fact callback. Run it outside the
            // global lock while its declaration cell exists, matching finalize's ordering.
            canceled.sort_by_key(|item| item.created_id.unwrap_or(u64::MAX));
            for item in canceled {
                if let Some(mut on_close) = item.on_close {
                    on_close();
                }
            }
            // Rescan: a callback may have queued another descendant under the canceled owner.
        }
        for id in doomed {
            let _ = CREATED.lock().unwrap().remove(&id);
            window_state::forget(WindowId::new(id));
        }
    }

    /// Root declarations can disappear before they ever enter AppState::windows.
    pub(crate) fn stale_pending_root_ids(active: &HashSet<u64>) -> Vec<u64> {
        crate::app::GLOBAL_PENDING
            .lock()
            .unwrap()
            .iter()
            .filter(|item| item.owner == Some(0))
            .filter_map(|item| item.created_id)
            .filter(|id| !active.contains(id))
            .collect()
    }

    pub(crate) fn process_detached(
        windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
        event_loop: &dyn winit::event_loop::ActiveEventLoop,
        force_shutdown: &dyn Fn(),
        saveable: &mut std::collections::HashMap<
            u64,
            std::collections::HashMap<String, crate::runtime::saveable::SavedValue>,
        >,
    ) {
        let pending: Vec<(winit::window::WindowId, Vec<u64>)> = windows
            .iter()
            .filter_map(|(window_id, pw)| {
                let ids = pw.composer.pending_window_close_ids();
                (!ids.is_empty()).then_some((*window_id, ids))
            })
            .collect();
        for (owner_window_id, ids) in pending {
            let Some(pw) = windows.get(&owner_window_id) else {
                continue;
            };
            // Reset BEFORE closing, not after: an `on_close` handler that registers another close on
            // the same composer must survive this pass and land in the next one. Clearing afterwards
            // would silently drop it.
            pw.composer.reset_pending_window_remove();
            for wid in ids {
                Self::close_detached_window(windows, event_loop, force_shutdown, wid, saveable);
            }
        }
    }

    fn close_detached_window(
        windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
        event_loop: &dyn winit::event_loop::ActiveEventLoop,
        force_shutdown: &dyn Fn(),
        wid: u64,
        saveable: &mut std::collections::HashMap<
            u64,
            std::collections::HashMap<String, crate::runtime::saveable::SavedValue>,
        >,
    ) {
        // One lock for the gate and the removal: a declaration that is already gone (its
        // `Window::build` never ran, or something else finalized it first) must not close anything.
        if !CREATED.lock().unwrap().remove(&wid) {
            return;
        }
        let to_close: Vec<winit::window::WindowId> = windows
            .iter()
            .filter(|(_, pw)| pw.created_id() == Some(wid))
            .map(|(wid2, _)| *wid2)
            .collect();
        for w in to_close {
            if let Some(mut pw) = windows.remove(&w) {
                pw.finalize(w, saveable);
            }
        }
        // Finalize first so on_close can still read WindowState; then cancel any creation that
        // never reached PerWindow and drop the declaration cell if it is still present.
        Self::cancel_pending_subtree(wid);
        Self::close_owned_children(windows, saveable, wid);
        for pw in windows.values_mut() {
            if let Some(ref sw) = pw.skia_window {
                sw.request_redraw();
            }
            pw.composer.mark_content_dirty();
        }
        // Declaration removal is a teardown like any other: the last-window policy decides whether
        // the process ends, and a resident application keeps running with no window.
        let exit = matches!(
            window_state::last_window_policy(),
            window_state::LastWindowPolicy::Exit
        );
        if windows.is_empty() && exit {
            force_shutdown();
            event_loop.exit();
        }
    }

    #[composable]
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        content: impl Fn(&mut ComposeCtx) + Send + 'static,
    ) -> WindowId {
        // The placeholder leaf and created_id share one salted key, for two reasons: (1) it keeps them
        // in a separate key space from content nodes, so a structural change cannot silently reuse an
        // old slot; (2) it is per-Window (remember_at_key(u64::MAX) would make several Windows share
        // one created_id, so the second Window would never be created).
        let key = ctx.next_key().wrapping_add(WINDOW_KEY_SALT);
        let created_id = ctx.remember_at_key(key, || 0u64);
        let _wid = created_id.get();

        // The lifecycle callback reads the LATEST id at fire time (a window is created asynchronously,
        // so the id at declaration time is 0). The request is stored on this Composer's lifecycle
        // context so a different window cannot consume it.
        //
        // It used to ride on a placeholder leaf slot created just to carry an `on_remove`; the
        // callback now lives in the slot itself (`effect::on_forget`), so nothing is materialized.
        // The salted key above is still needed for `created_id`, but the callback no longer shares it.
        let cid = created_id.clone();
        let lifecycle = ctx.window_lifecycle();
        let remove_lifecycle = lifecycle.clone();
        crate::effect::on_forget(ctx, move || {
            let wid = cid.get();
            if wid != 0 {
                // The declaration is gone, so its tombstone must not leak into a later window that
                // happens to reuse the same slot.
                DESTROYED.lock().unwrap().remove(&wid);
                if CREATED.lock().unwrap().contains(&wid) {
                    remove_lifecycle.set_pending_remove(wid);
                }
            }
        });

        // The theme this window resolves against, REMEMBERED across frames: the declaring tree re-runs
        // every frame and may switch which theme node wraps this window — palette, typography and direction
        // alike — and the window has to follow that (a sub-window whose tree flipped from light to dark kept
        // the palette it started with, and a custom type scale survived exactly one frame). The cell is
        // shared with the window's own composer — see `theme::WindowTheme`.
        let theme = ctx.remember_at_key(key.wrapping_add(1), || {
            crate::theme::WindowTheme::new(crate::theme::current_theme_spec())
        });
        let theme = theme.get();
        if theme.publish(
            crate::theme::current_theme_spec(),
            crate::theme::WiniaTheme::typography(),
            crate::layout::direction::current(),
        ) {
            // Something moved in THIS composer, which leaves the window's own composer with nothing
            // pending: ask the loop to schedule its frame.
            let id = created_id.get();
            if id != 0 {
                app::request_redraw_created(id);
            }
        }

        let wid = created_id.get();
        if wid != 0 {
            window_state::set_event_handler(WindowId::new(wid), self.on_event.clone());
        }
        let destroyed = wid != 0 && DESTROYED.lock().unwrap().contains(&wid);
        // A destroyed window keeps its declaration but not its platform window; only a fresh
        // declaration (a new created_id) may create one again.
        let need_new = wid == 0 || (!destroyed && !CREATED.lock().unwrap().contains(&wid));

        if need_new {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            created_id.as_raw().set_backchannel(id); // Backchannel: window-creation marker must not notify (would avalanche keys / collapse the tree mid-compose)
            CREATED.lock().unwrap().insert(id);

            let w = self.spec.width;
            let h = self.spec.height;
            let title = if self.spec.title.is_empty() {
                "Winia".to_owned()
            } else {
                self.spec.title.clone()
            };
            // Publish the live state now: the declaration owns the id before the platform window
            // exists, so `WindowState` is usable from the very first composition. `realize` fills
            // in the values the platform actually chose.
            let caps = WindowCapabilities::platform();
            let mut options = self.options;
            options.normalize(caps);
            let position = if caps.position {
                self.position
            } else {
                window_state::WindowPosition::Default
            };
            let placement = if caps.placement {
                self.placement
            } else {
                WindowPlacement::Windowed
            };
            let app_bar = caps.app_bar && options.app_bar.is_some();
            let placement = if app_bar {
                WindowPlacement::Windowed
            } else {
                placement
            };
            let position = if app_bar {
                window_state::WindowPosition::Default
            } else {
                position
            };
            let owner_raw = ctx.window_id().raw();
            let owner = (owner_raw != 0).then_some(WindowId::new(owner_raw));
            window_state::declare_owned(
                WindowId::new(id),
                WindowSeed {
                    size: (w, h),
                    title: title.clone(),
                    resizable: options.resizable,
                    always_on_top: options.always_on_top,
                    transparent: options.transparent,
                    skip_taskbar: options.skip_taskbar,
                    tool_window: options.tool_window,
                    no_activate: options.no_activate,
                    app_bar: options.app_bar,
                    decorations: options.decorations,
                    minimum_size: options.minimum_size,
                    maximum_size: options.maximum_size,
                    visible: self.visible,
                    placement,
                    position,
                    on_close_requested: self.on_close_requested,
                    on_event: self.on_event,
                },
                owner,
            );
            let mut on_close = self.on_close;
            let id_close = id;
            let wrapped: Option<Box<dyn FnMut() + Send>> = Some(Box::new(move || {
                CREATED.lock().unwrap().remove(&id_close);
                if let Some(ref mut f) = on_close {
                    f();
                }
            }));
            // The content closure carries the CELL (a live handle), not a palette: a captured palette pinned
            // the window to its startup colors — the app's own theme switch flipped the surface behind the
            // tree while every component kept composing with the old colors. `app::open_window` wraps the
            // content in it, so this stays a plain closure.
            // The saveable key is the DECLARATION SITE's salted key, not the freshly minted window id:
            // a re-opened window has to be matched against the site that declared it.
            app::open_window_with_keys_owned(
                w,
                h,
                title,
                options,
                None,
                wrapped,
                Some(id),
                Some(key),
                self.persist_key,
                position,
                placement,
                self.visible,
                Some(ctx.window_id().raw()),
                Some(theme),
            );
        }

        // Publish the latest closure on every recomposition. The initial creation takes it from the
        // cell in open_window; later frames replace PerWindow.content at the frame boundary.
        let content_id = created_id.get();
        if content_id != 0 && !DESTROYED.lock().unwrap().contains(&content_id) {
            window_state::set_pending_content(
                WindowId::new(content_id),
                Box::new(move |ctx| sub_window_content(ctx, &content)),
            );
        }

        ctx.end_node();

        // After end_node: mark that this window's declaration was rebuilt this pass.
        lifecycle.mark_rebuilt(created_id.get());

        // The handle is valid from the first frame: the id was minted above, and `is_realized()`
        // stays false until the platform creates the window.
        WindowId::new(created_id.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::window_state::WindowCapabilities;

    #[test]
    fn transparent_builder_is_order_independent() {
        let caps = WindowCapabilities {
            position: true,
            placement: true,
            multi_window: true,
            native_close: true,
            activate: true,
            always_on_top: true,
            transparent: true,
            skip_taskbar: true,
            tool_window: true,
            no_activate: true,
            app_bar: true,
            decorations: true,
            size_constraints: true,
            native_popup: true,
        };
        let mut transparent_first = Window::new().transparent(true).decorations(true).options;
        let mut decorations_first = Window::new().decorations(true).transparent(true).options;
        transparent_first.normalize(caps);
        decorations_first.normalize(caps);

        for options in [transparent_first, decorations_first] {
            assert!(options.transparent);
            assert!(options.decorations);
        }
    }
}
