//! keyed_stmt! + composable_keyed 宏冒烟测试——展开/运行不 panic。
//!
//! ⚠ 定稿契约（key-system-design.md §9：编译期替换机制已砍）：
//! `#[composable_keyed]` 只注入 RAII scope guard，**不做** remember/next_key
//! 编译期替换——裸 ctx 调用必须用 `keyed_stmt!` 标记获得语句级稳定 base，
//! 否则运行期 fail-fast panic（见下方 negative test）。
use winia::core::composer::Composer;
use winia::prelude::*;

#[composable_keyed]
fn light_ui(ctx: &mut ComposeCtx, flag: bool) {
    // 定稿契约：remember 也需 keyed_stmt! 标记（语句 id = 调用位置哈希）
    let count = keyed_stmt!({ ctx.remember(|| 0) });
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
    let v = keyed_stmt!({ ctx.remember(|| 42) });
    v.get()
}

/// 定稿 fail-fast 契约：composable_keyed 内裸 remember（无 keyed_stmt!/ctx.key）
/// 必须 panic——静默降级会重新引入跨语句 key 漂移（设计文档 §1.2 根因）。
#[test]
fn composable_keyed_bare_remember_fails_fast() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    #[composable_keyed]
    fn bare(ctx: &mut ComposeCtx) {
        let _ = ctx.remember(|| 0);
    }
    let result = catch_unwind(AssertUnwindSafe(move || {
        let mut composer = Composer::new();
        composer.compose(|ctx| bare(ctx));
    }));
    assert!(result.is_err(), "裸 remember 应 fail-fast panic");
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
