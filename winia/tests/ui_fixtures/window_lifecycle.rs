//! Window lifecycle fixture for `window_lifecycle_test.rs`.
//!
//! One scenario per process, selected with `WINDOW_FIXTURE_MODE`. The fixture writes observations to
//! `WINDOW_FIXTURE_OUT`; a few scenarios intentionally keep running so the test can observe that a
//! vetoed close or a resident policy did not end the process.

use std::cell::Cell;
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use winia::prelude::*;

static FIRST_WINDOW: AtomicBool = AtomicBool::new(true);

/// Every observation is appended. A scenario runs one process against a fresh output file, so an
/// append is also its first write; using a truncating write somewhere would silently erase the
/// markers another scope already recorded.
fn append_out(text: &str) {
    if let Ok(path) = std::env::var("WINDOW_FIXTURE_OUT") {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(text.as_bytes());
        }
    }
}

pub fn main() {
    let mode = std::env::var("WINDOW_FIXTURE_MODE").unwrap_or_default();
    if mode == "persist"
        || mode == "persist-again"
        || mode == "persist-clear"
        || mode == "persist-initial-destroy"
        || mode == "geometry-save"
        || mode == "geometry-restore"
    {
        // Persistence is opt-in; the test points this at a scratch directory.
        if let Ok(dir) = std::env::var("WINDOW_FIXTURE_STATE") {
            winia::runtime::saveable::persist::set_store(Some(
                winia::runtime::saveable::persist::PersistStore::open(dir),
            ));
        }
    }
    winia::run_app!(move |ctx| {
        if mode == "destroy-stay" || mode == "pending-child-close" {
            ctx.set_last_window_policy(LastWindowPolicy::StayResident);
        }
        window(ctx, &mode);
    });
}

fn window(ctx: &mut ComposeCtx, mode: &str) {
    let mode = mode.to_owned();
    let mode_for_handler = mode.clone();
    let mode_for_close = mode.clone();
    let mode_for_on_close = mode.clone();
    let declare_second = ctx.remember(|| false);
    let trigger_second = declare_second.clone();
    let title = if mode == "empty-title" {
        ""
    } else {
        "window fixture"
    };
    let mut builder = Window::new()
        .title(title)
        .size(400.0, 300.0)
        .key("fixture-persist");
    if mode == "initial-state" || mode == "initial-position-reset" {
        builder = builder
            .position(WindowPosition::At(120.0, 80.0))
            .visible(false);
    }
    if mode == "initial-placement" {
        builder = builder.placement(WindowPlacement::Maximized).visible(false);
    }
    if mode == "window-properties" {
        builder = builder
            .always_on_top(true)
            .transparent(true)
            .decorations(false)
            .minimum_size(320.0, 240.0)
            .maximum_size(900.0, 700.0);
    }
    let declared_id = builder
        .on_close_requested(move |id, reason| {
            append_out(&format!("requested={reason:?}\n"));
            if mode_for_handler == "close-accept" {
                id.destroy();
            }
        })
        .on_close(move || {
            if mode_for_on_close == "initial-destroy"
                || mode_for_on_close == "persist-initial-destroy"
            {
                append_out("initial-on-close\n");
            }
            if mode_for_on_close == "existing-command-new-root" {
                append_out("first-closed\n");
            }
        })
        .build(ctx, move |ctx| {
            let id = ctx.window_id();
            match mode_for_close.as_str() {
                "size" => size_scenario(ctx, id),
                "close-veto" | "close-accept" => close_scenario(ctx, id),
                "policy-exit" | "policy-stay" => policy_scenario(ctx, id, &mode_for_close),
                "destroy-stay" => destroy_stay_scenario(ctx, id),
                "nested-close" => nested_close_scenario(ctx, id),
                "sibling-close" => sibling_close_scenario(ctx),
                "content-update" => content_update_scenario(ctx),
                "pending-child-close" => pending_child_close_scenario(ctx, id),
                "empty-title" => empty_title_scenario(ctx, id),
                "initial-commands" => initial_commands_scenario(ctx, id),
                "initial-destroy" | "persist-initial-destroy" => initial_destroy_scenario(ctx, id),
                "reactive-title" => reactive_title_scenario(ctx, id),
                "reactive-size" => reactive_size_scenario(ctx, id),
                "initial-state" | "initial-placement" | "initial-position-reset" => {
                    initial_state_scenario(ctx, id)
                }
                "insets-share" => insets_share_scenario(ctx, id),
                "owner-query" => owner_query_scenario(ctx, id),
                "window-properties" => window_properties_scenario(ctx, id),
                "existing-command-new-root" => {
                    let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
                    if !once.get() {
                        once.set(true);
                        let trigger = trigger_second.clone();
                        winia::effect::side_effect(ctx, move || trigger.set(true));
                    }
                    Text::new("first").build(ctx);
                }
                "persist" | "persist-again" => persist_scenario(ctx, id),
                "persist-clear" => persist_clear_scenario(ctx, id),
                "geometry-save" | "geometry-restore" => geometry_scenario(ctx, id, &mode_for_close),
                other => {
                    append_out(&format!("unknown-mode={other}\n"));
                    std::process::exit(2);
                }
            }
        });
    if mode == "empty-title" {
        append_out(&format!("declared-title={}\n", declared_id.state().title()));
    }
    if mode == "initial-position-reset" {
        declared_id.state().set_position(WindowPosition::Default);
    }
    if mode == "initial-commands" {
        declared_id.state().set_title("early title");
        declared_id.state().set_size(640.0, 480.0);
        declared_id.state().set_visible(false);
    }
    if mode == "initial-destroy" || mode == "persist-initial-destroy" {
        declared_id.destroy();
    }
    if mode == "existing-command-new-root" && declare_second.get() {
        // Same root recomposition: the existing window's command is submitted first, then a new
        // root window is declared. Its first frame must not jump ahead of the older command.
        declared_id.destroy();
        Window::new().title("second root").build(ctx, |ctx| {
            append_out("second-composed\n");
            Text::new("second").build(ctx);
            std::process::exit(0);
        });
    }
}

fn initial_commands_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let state = id.state();
    append_out(&format!(
        "initial-title={}\ninitial-size={:?}\ninitial-visible={}\ninitial-realized={}\n",
        state.title(),
        state.size(),
        state.visible(),
        id.is_realized(),
    ));
    std::process::exit(0);
}

fn initial_destroy_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    append_out("initial-content-composed\n");
    Text::new("unexpected").build(ctx);
    let _ = id;
}

fn reactive_title_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let title = id.state().title();
    if title == "reactive title" {
        append_out("reactive-title=reactive title\n");
        std::process::exit(0);
    }
    let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !once.get() {
        once.set(true);
        winia::effect::side_effect(ctx, move || id.state().set_title("reactive title"));
    }
    Text::new("title").build(ctx);
}

fn reactive_size_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let size = id.state().size();
    if (size.0 - 560.0).abs() < 1.0 && (size.1 - 360.0).abs() < 1.0 {
        append_out("reactive-size=(560.0, 360.0)\n");
        std::process::exit(0);
    }
    let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !once.get() {
        once.set(true);
        winia::effect::side_effect(ctx, move || id.state().set_size(560.0, 360.0));
    }
    Text::new("size").build(ctx);
}

fn initial_state_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let state = id.state();
    append_out(&format!(
        "builder-position={:?}\nbuilder-placement={:?}\nbuilder-visible={}\n",
        state.position(),
        state.placement(),
        state.visible(),
    ));
    std::process::exit(0);
}

fn insets_share_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let external = id.insets();
    let content = ctx.window_insets();
    let expected = winia::window::insets::Insets::new(1.0, 2.0, 3.0, 4.0);
    external.set_status_bar_insets(expected);
    append_out(&format!(
        "insets-shared={}\nstatus={:?}\n",
        external.current() == content.current(),
        content.current().status_bars,
    ));
    std::process::exit(0);
}

fn owner_query_scenario(ctx: &mut ComposeCtx, parent: WindowId) {
    append_out(&format!("root-owner={:?}\n", parent.owner()));
    Window::new().title("owned child").build(ctx, move |ctx| {
        let child = ctx.window_id();
        append_out(&format!("child-owner={:?}\n", child.owner()));
        Text::new("child").build(ctx);
        std::process::exit(0);
    });
}

fn window_properties_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let state = id.state();
    append_out(&format!(
        "initial-top={}\ninitial-transparent={}\ninitial-decorations={}\ninitial-min={:?}\ninitial-max={:?}\n",
        state.always_on_top(),
        state.transparent(),
        state.decorations(),
        state.minimum_size(),
        state.maximum_size(),
    ));
    let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !once.get() {
        once.set(true);
        winia::effect::side_effect(ctx, move || {
            id.state().set_always_on_top(false);
            id.state().set_minimum_size(Some((280.0, 220.0)));
            id.state().set_maximum_size(Some((1000.0, 800.0)));
            id.activate();
        });
    }
    Text::new("properties").build(ctx);
}

fn size_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let started = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !started.get() {
        started.set(true);
        winia::effect::side_effect(ctx, move || id.state().set_size(520.0, 333.0));
    }
    // A tracked read: the platform resize dirties this content, so the check below runs after the
    // echo. Reading only `WindowState` would be a plain cell read and never schedule a pass.
    let layout = winia::layout::window_size();
    let size = id.state().size();
    if (size.0 - 520.0).abs() < 1.0 && (size.1 - 333.0).abs() < 1.0 {
        append_out(&format!("state={size:?}\nlayout={layout:?}\n"));
        std::process::exit(0);
    }
    Text::new("size").build(ctx);
}

fn close_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let requested = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !requested.get() {
        requested.set(true);
        winia::effect::side_effect(ctx, move || {
            append_out("closing\n");
            id.close();
        });
    }
    Text::new("close").build(ctx);
}

fn policy_scenario(ctx: &mut ComposeCtx, id: WindowId, mode: &str) {
    if mode == "policy-stay" {
        ctx.set_last_window_policy(LastWindowPolicy::StayResident);
    }
    let destroyed = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !destroyed.get() {
        destroyed.set(true);
        winia::effect::side_effect(ctx, move || {
            append_out("destroying\n");
            id.destroy();
        });
    }
    Text::new("policy").build(ctx);
}

/// Destroy the window while its declaration still composes. The declaration must not mint a
/// replacement on the next frame.
fn destroy_stay_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let first = FIRST_WINDOW.swap(false, Ordering::Relaxed);
    append_out(if first { "created\n" } else { "recreated\n" });
    let destroyed = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !destroyed.get() {
        destroyed.set(true);
        winia::effect::side_effect(ctx, move || id.destroy());
    }
    Text::new("destroy").build(ctx);
}

/// A child window declared inside a parent's content. The parent waits until the child has
/// realized, then destroys itself; the child must be finalized by the owner cascade.
fn nested_close_scenario(ctx: &mut ComposeCtx, parent: WindowId) {
    let child_ready = ctx.remember(|| false);
    let child_ready_for_child = child_ready.clone();
    Window::new()
        .title("nested child")
        .size(260.0, 180.0)
        .on_close(|| append_out("child-closed\n"))
        .build(ctx, move |ctx| {
            let ready = child_ready_for_child.clone();
            let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
            if !once.get() {
                once.set(true);
                winia::effect::side_effect(ctx, move || ready.set(true));
            }
            Text::new("child").build(ctx);
        });

    if child_ready.get() {
        let destroyed = ctx.remember(|| Rc::new(Cell::new(false))).get();
        if !destroyed.get() {
            destroyed.set(true);
            winia::effect::side_effect(ctx, move || parent.destroy());
        }
    }
    Text::new("parent").build(ctx);
}

/// The window API demo's shape: the primary window's content reads a `State<bool>` and declares a
/// secondary window behind it. The secondary window closes itself (the same write its Close button
/// makes), and the primary window must re-render without any pointer input.
fn sibling_close_scenario(ctx: &mut ComposeCtx) {
    let open = ctx.remember(|| true);
    // The label sits in its own group and reads nothing reactive: a recompose triggered by the
    // shared `State` skips it (its parameters did not change), so only a whole-content rebuild
    // reaches it. That is the mechanism this scenario exists to pin.
    let seq = ctx.remember(|| Rc::new(Cell::new(0usize))).get();
    let seen = ctx.remember(|| Rc::new(Cell::new(usize::MAX))).get();
    Column::new().build(ctx, |ctx| {
        let count = ctx.windows().len();
        if seen.get() != count {
            seen.set(count);
            let n = seq.get() + 1;
            seq.set(n);
            append_out(&format!("seq={n} windows={count}\n"));
        }
        Text::new(format!("windows {count}")).build(ctx);
    });
    append_out(&format!(
        "parent={}\n",
        if open.get() { "open" } else { "closed" }
    ));
    if open.get() {
        let open_for_child = open.clone();
        Window::new()
            .title("sibling")
            .size(260.0, 180.0)
            .build(ctx, move |ctx| {
                let open = open_for_child.clone();
                append_out("child=composed\n");
                let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
                if !once.get() {
                    once.set(true);
                    // The write a Close button handler makes: outside compose, on the UI thread.
                    winia::effect::side_effect(ctx, move || open.set(false));
                }
                Text::new("child").build(ctx);
            });
    } else {
        // Reaching this line at all means the primary window re-ran after the sibling closed.
        append_out("parent-saw-closed\n");
    }
    Text::new(if open.get() {
        "declared"
    } else {
        "not declared"
    })
    .build(ctx);
}

/// The declaration recomposes with a new closure capturing a plain value. The child must replace
/// its old PerWindow.content rather than keeping the first closure forever.
fn content_update_scenario(ctx: &mut ComposeCtx) {
    let version = ctx.remember(|| 0u32);
    let child_ready = ctx.remember(|| false);
    let ready_for_child = child_ready.clone();
    let value = version.get();
    Window::new().title("content child").build(ctx, move |ctx| {
        append_out(&format!("content={value}\n"));
        if value == 1 {
            std::process::exit(0);
        }
        let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
        if !once.get() {
            once.set(true);
            let ready = ready_for_child.clone();
            winia::effect::side_effect(ctx, move || ready.set(true));
        }
        Text::new("child").build(ctx);
    });
    if child_ready.get() && version.get() == 0 {
        let version = version.clone();
        winia::effect::side_effect(ctx, move || version.set(1));
    }
}

/// Destroy the parent in the same composition that declares a child. The child is still in
/// GLOBAL_PENDING and must be canceled instead of opening as an orphan.
fn pending_child_close_scenario(ctx: &mut ComposeCtx, parent: WindowId) {
    Window::new()
        .title("pending orphan")
        .on_close(|| {
            append_out("pending-child-closed\n");
            // Re-enter the public creation API from the callback. Cancellation must release the
            // global pending lock before invoking user code.
            winia::app::open_window_with_title(
                120.0,
                90.0,
                "reentrant".to_owned(),
                Some(Box::new(|ctx| {
                    append_out("reentrant-composed\n");
                    Text::new("reentrant").build(ctx);
                })),
                None,
                None,
                None,
            );
        })
        .build(ctx, |ctx| {
            append_out("pending-child-composed\n");
            Text::new("orphan").build(ctx);
        });
    let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !once.get() {
        once.set(true);
        winia::effect::side_effect(ctx, move || parent.destroy());
    }
}

fn empty_title_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    append_out(&format!("title={}\n", id.state().title()));
    std::process::exit(0);
}

/// Close a keyed window without registering any saveable provider. The empty snapshot must replace
/// the previous run's disk snapshot.
fn persist_clear_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let once = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !once.get() {
        once.set(true);
        winia::effect::side_effect(ctx, move || id.destroy());
    }
    Text::new("clear").build(ctx);
}

fn geometry_scenario(ctx: &mut ComposeCtx, id: WindowId, mode: &str) {
    let state = id.state();
    let position_supported = id.capabilities().position;
    if mode == "geometry-restore" {
        append_out(&format!(
            "restored-size={:?}\nrestored-position={:?}\nrestored-position-supported={}\n",
            state.size(),
            state.position(),
            position_supported,
        ));
        std::process::exit(0);
    }

    let requested = ctx.remember(|| false);
    if !requested.get() {
        requested.set(true);
        state.set_size(640.0, 480.0);
        if position_supported {
            state.set_position(WindowPosition::At(120.0, 80.0));
        }
    }
    let finished = ctx.remember(|| false);
    if !finished.get()
        && state.size() == (640.0, 480.0)
        && (!position_supported || state.position() == WindowPosition::At(120.0, 80.0))
    {
        finished.set(true);
        append_out("geometry-saved\n");
        winia::effect::side_effect(ctx, move || id.destroy());
    }
    Text::new("geometry").build(ctx);
}

fn persist_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let counter = winia::runtime::saveable::remember_saveable::<i64>(ctx, "counter", || 0);
    let written = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !written.get() {
        written.set(true);
        let value = counter.get();
        append_out(&format!("counter={value}\n"));
        let target = counter.clone();
        winia::effect::side_effect(ctx, move || {
            target.set(value + 1);
            id.destroy();
        });
    }
    Text::new("persist").build(ctx);
}
