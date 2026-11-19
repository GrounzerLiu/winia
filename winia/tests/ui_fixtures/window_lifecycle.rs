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

fn write_out(text: &str) {
    if let Ok(path) = std::env::var("WINDOW_FIXTURE_OUT") {
        let _ = std::fs::write(path, text);
    }
}

fn append_out(text: &str) {
    if let Ok(path) = std::env::var("WINDOW_FIXTURE_OUT") {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(text.as_bytes());
        }
    }
}

pub fn main() {
    let mode = std::env::var("WINDOW_FIXTURE_MODE").unwrap_or_default();
    if mode == "persist" || mode == "persist-again" {
        // Persistence is opt-in; the test points this at a scratch directory.
        if let Ok(dir) = std::env::var("WINDOW_FIXTURE_STATE") {
            winia::runtime::saveable::persist::set_store(Some(
                winia::runtime::saveable::persist::PersistStore::open(dir),
            ));
        }
    }
    winia::run_app!(move |ctx| {
        if mode == "destroy-stay" {
            ctx.set_last_window_policy(LastWindowPolicy::StayResident);
        }
        window(ctx, &mode);
    });
}

fn window(ctx: &mut ComposeCtx, mode: &str) {
    let mode = mode.to_owned();
    let mode_for_handler = mode.clone();
    let mode_for_close = mode.clone();
    Window::new()
        .title("window fixture")
        .size(400.0, 300.0)
        .key("fixture-persist")
        .on_close_requested(move |id, reason| {
            write_out(&format!("requested={reason:?}\n"));
            if mode_for_handler == "close-accept" {
                id.destroy();
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
                "persist" | "persist-again" => persist_scenario(ctx, id),
                other => {
                    write_out(&format!("unknown-mode={other}\n"));
                    std::process::exit(2);
                }
            }
        });
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
        write_out(&format!("state={size:?}\nlayout={layout:?}\n"));
        std::process::exit(0);
    }
    Text::new("size").build(ctx);
}

fn close_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let requested = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !requested.get() {
        requested.set(true);
        winia::effect::side_effect(ctx, move || {
            write_out("closing\n");
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
            write_out("destroying\n");
            id.destroy();
        });
    }
    Text::new("policy").build(ctx);
}

/// Destroy the window while its declaration still composes. The declaration must not mint a
/// replacement on the next frame.
fn destroy_stay_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let first = FIRST_WINDOW.swap(false, Ordering::Relaxed);
    write_out(if first { "created\n" } else { "recreated\n" });
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
    append_out(&format!("parent={}\n", if open.get() { "open" } else { "closed" }));
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
    Text::new(if open.get() { "declared" } else { "not declared" }).build(ctx);
}

fn persist_scenario(ctx: &mut ComposeCtx, id: WindowId) {
    let counter = winia::runtime::saveable::remember_saveable::<i64>(ctx, "counter", || 0);
    let written = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !written.get() {
        written.set(true);
        let value = counter.get();
        write_out(&format!("counter={value}\n"));
        let target = counter.clone();
        winia::effect::side_effect(ctx, move || {
            target.set(value + 1);
            id.destroy();
        });
    }
    Text::new("persist").build(ctx);
}
