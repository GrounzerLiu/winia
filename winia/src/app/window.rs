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
    persist_key: Option<u64>,
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
            persist_key: None,
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

    pub fn on_close(mut self, f: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }

    /// A stable key for this window's saved state.
    ///
    /// Without one the window keeps its state in memory only: closing and re-opening it starts
    /// empty. The key has to be unique per window and stable across runs; it is hashed into the
    /// saveable store, so renaming it discards whatever the old name saved.
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
            let removed = CREATED.lock().unwrap().remove(&created_id);
            if !removed {
                continue;
            }
            if let Some(mut pw) = windows.remove(&window_id) {
                pw.finalize(window_id, saveable);
            }
            Self::close_owned_children(windows, saveable, created_id);
        }
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
            // Publish the live state now: the declaration owns the id before the platform window
            // exists, so `WindowState` is usable from the very first composition. `realize` fills
            // in the values the platform actually chose.
            window_state::declare(
                WindowId::new(id),
                WindowSeed {
                    size: (w, h),
                    title: self.spec.title.clone(),
                    resizable: self.options.resizable,
                    visible: true,
                    placement: WindowPlacement::Windowed,
                    on_close_requested: self.on_close_requested,
                },
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
                self.spec.title.clone(),
                self.options,
                Some(Box::new(move |ctx| {
                    sub_window_content(ctx, &content);
                })),
                wrapped,
                Some(id),
                Some(key),
                self.persist_key,
                Some(ctx.window_id().raw()),
                Some(theme),
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
