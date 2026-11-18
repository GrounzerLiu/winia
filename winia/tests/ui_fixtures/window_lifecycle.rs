//! Window lifecycle fixture for `window_lifecycle_test.rs`.
//!
//! One scenario per process, selected with `WINDOW_FIXTURE_MODE`. The fixture writes observations to
//! `WINDOW_FIXTURE_OUT`; a few scenarios intentionally keep running so the test can observe that a
//! vetoed close or a resident policy did not end the process.

use std::cell::Cell;
use std::rc::Rc;
use winia::prelude::*;

fn write_out(text: &str) {
    if let Ok(path) = std::env::var("WINDOW_FIXTURE_OUT") {
        let _ = std::fs::write(path, text);
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
