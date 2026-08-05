//! UI 测试 fixture：多级 if/else 结构切换（3 态循环）。
//! 场景：`switch` 按钮点击 → level 1 → 2 → other → 1 …循环；每态节点数不同；
//! 重复进入同一状态时节点数一致（结构稳定、key 不漂移）。
//!
//! 由 `[[bin]]` 注册编译为独立 exe，测试通过 stdin/stdout 管道驱动。

use winia::prelude::*;

#[composable]
fn ui(ctx: &mut ComposeCtx) {
    let level = ctx.remember(|| 1i32);
    let clicks = ctx.remember(|| 0i32);
    let n = clicks.get();

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new().on_click({ let s = level.clone(); move || { s.update(|v| { *v = (*v % 3) + 1; }); } })
                .modifier(Modifier::new().size(110.0, 28.0))
                .build(ctx, |ctx| { Text::new("switch").font_size(12.0).build(ctx); });

            if level.get() == 1 {
                // 分支 1：嵌套子分支（even/odd）——节点数最多
                Text::new(format!("level 1 — clicks={n}")).font_size(12.0).build(ctx);
                if n % 2 == 0 {
                    Text::new("  → even").font_size(12.0).build(ctx);
                } else {
                    Text::new("  → odd").font_size(12.0).build(ctx);
                    if n > 2 {
                        Text::new("    → >2").font_size(12.0).build(ctx);
                    } else {
                        Text::new("    → ≤2").font_size(12.0).build(ctx);
                    }
                }
            } else if level.get() == 2 {
                // 分支 2：中间复杂度
                Text::new(format!("level 2 — clicks={n}")).font_size(12.0).build(ctx);
                Button::new()
                    .modifier(Modifier::new().size(100.0, 28.0))
                    .build(ctx, |ctx| { Text::new("+3").font_size(12.0).build(ctx); });
            } else {
                // 分支 3：最简
                Text::new("level other").font_size(12.0).build(ctx);
            }
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(400.0, 500.0)
            .title("ui fixture: nest")
            .build(ctx, |ctx| { ui(ctx); });
    });
}
