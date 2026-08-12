//! keyed_stmt! + composable_keyed 宏冒烟测试——展开/运行不 panic
use winia::core::composer::Composer;
use winia::prelude::*;

#[composable_keyed]
fn light_ui(ctx: &mut ComposeCtx, flag: bool) {
    // remember 被编译期替换（scope base——不注入语句也可用）
    let count = ctx.remember(|| 0);
    if flag {
        keyed_stmt!({ Text::new("x").build(ctx); });
    }
    let _ = count.get();
}

#[test]
fn composable_keyed_runs() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    let mut composer = Composer::new();
    for _ in 0..2 {
        composer.compose(|ctx| {
            light_ui(ctx, true);
        });
    }
    composer.layout(winia::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    let root = composer.layout_root_idx().unwrap();
    // keyed_stmt 内的 Text 物化并测量（layout 后 size 非零）
    let n = &composer.arena_nodes()[root];
    assert!(n.measured_size.width > 0.0 && n.measured_size.height > 0.0,
        "keyed_stmt 内组件应物化并测量（size={:?}）", n.measured_size);
}

#[composable_keyed]
fn keyed_returns(ctx: &mut ComposeCtx) -> i32 {
    let v = ctx.remember(|| 42);
    v.get()
}

#[test]
fn composable_keyed_supports_return() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    let mut composer = Composer::new();
    let mut out = 0;
    composer.compose(|ctx| {
        out = keyed_returns(ctx);
    });
    assert_eq!(out, 42, "composable_keyed 支持返回值");
}
