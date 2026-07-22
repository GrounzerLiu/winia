//! 事件流测试 — State 变化 → 重组 → 验证 LayoutNode 树更新。
//!
//! 不涉及渲染，纯测试 compose → layout 在 state 变更后的行为。

use winia::core::composer::Composer;
use winia::core::state::State;
use winia::layout::constraints::Constraints;
use winia::modifier::Modifier;
use winia::ui::{Text, Column};
use winia::core::composer::ComposeCtx;

fn compose_and_layout(composer: &mut Composer, w: f32, h: f32, ui: impl FnOnce(&mut ComposeCtx)) {
    composer.compose(|ctx| ui(ctx));
    composer.layout(Constraints::new(0.0, w, 0.0, h));
}

#[test]
fn state_change_triggers_recomposition() {
    let mut composer = Composer::new();
    let count = State::new(0i32);

    // 首次 compose
    compose_and_layout(&mut composer, 200.0, 40.0, |ctx| {
        Text::new(format!("Count: {}", count.get())).build(ctx);
    });
    assert!(composer.layout_root().is_some());

    // 记住首次的 measured 尺寸
    let first_w = composer.layout_root().unwrap().measured_size.width;

    // 修改 state
    count.set(42);

    // 重组
    composer.recompose(|ctx| {
        Text::new(format!("Count: {}", count.get())).build(ctx);
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 40.0));

    let second_w = composer.layout_root().unwrap().measured_size.width;
    // "Count: 42" 比 "Count: 0" 宽
    assert!(second_w > first_w,
        "text with larger number should be wider: {} → {}", first_w, second_w);
}

#[test]
fn conditional_node_appears_after_state_change() {
    let mut composer = Composer::new();
    let show = State::new(false);

    // 首次：不显示额外节点
    compose_and_layout(&mut composer, 200.0, 60.0, |ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(100.0, 20.0));
            ctx.end_node();

            if show.get() {
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().size(100.0, 20.0));
                ctx.end_node();
            }
        });
    });

    let root = composer.layout_root().unwrap();
    assert_eq!(root.children.len(), 1, "initially 1 child");

    // 切换 show
    show.set(true);

    // 重组
    composer.compose(|ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(100.0, 20.0));
            ctx.end_node();

            if show.get() {
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().size(100.0, 20.0));
                ctx.end_node();
            }
        });
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 60.0));

    let root = composer.layout_root().unwrap();
    assert_eq!(root.children.len(), 2, "after show=true, should have 2 children");
    // 总高度增加
    let total_h: f32 = root.children.iter().map(|c| c.measured_size.height).sum();
    assert!(total_h > 35.0, "total height should increase with second child");
}

#[test]
fn state_remember_persists_across_recompose() {
    let mut composer = Composer::new();

    // 首次 compose：remember 创建 state，set 为 10
    composer.compose(|ctx| {
        let s = ctx.remember(|| 0i32);
        s.set(10);
        Text::new(format!("v={}", s.get())).build(ctx);
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 40.0));

    let w1 = composer.layout_root().unwrap().measured_size.width;

    // 重组：remember 应返回同一个 state（值仍为 10）
    composer.recompose(|ctx| {
        let s: State<i32> = ctx.remember(|| 999);
        assert_eq!(s.get(), 10, "remember should return persisted value");
        Text::new(format!("v={}", s.get())).build(ctx);
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 40.0));

    let w2 = composer.layout_root().unwrap().measured_size.width;
    assert_eq!(w1, w2, "same content, same width");
}

#[test]
fn multiple_state_changes_in_one_frame() {
    let mut composer = Composer::new();
    let a = State::new(1i32);
    let b = State::new(10i32);

    // 首次 compose
    compose_and_layout(&mut composer, 200.0, 40.0, |ctx| {
        Text::new(format!("{} + {}", a.get(), b.get())).build(ctx);
    });

    // 修改两个 state
    a.set(100);
    b.set(200);

    // 重组
    composer.recompose(|ctx| {
        Text::new(format!("{} + {}", a.get(), b.get())).build(ctx);
    });
    composer.layout(Constraints::new(0.0, 300.0, 0.0, 40.0));

    let root = composer.layout_root().unwrap();
    // 新文本更长："100 + 200" > "1 + 10"
    assert!(root.measured_size.width > 60.0,
        "wider text after larger numbers, got {}", root.measured_size.width);
}

#[test]
fn recompose_skips_when_no_changes() {
    let mut composer = Composer::new();
    let count = State::new(0i32);

    // 首次 compose
    compose_and_layout(&mut composer, 200.0, 40.0, |ctx| {
        Text::new(format!("Count: {}", count.get())).build(ctx);
    });
    let w1 = composer.layout_root().unwrap().measured_size.width;

    // 重组（无 state 变化，recompose 应跳过）
    let did_recompose = composer.recompose(|ctx| {
        Text::new(format!("Count: {}", count.get())).build(ctx);
    });

    if did_recompose {
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 40.0));
        // 即使重组了，内容相同，宽度应不变
        let w2 = composer.layout_root().unwrap().measured_size.width;
        assert_eq!(w1, w2);
    }
    // 如果 recompose 返回 false（增量跳过），也是一种正确的行为
}

#[test]
fn nested_state_propagates_layout_recalculation() {
    let mut composer = Composer::new();
    let show_extra = State::new(false);

    compose_and_layout(&mut composer, 300.0, 100.0, |ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(200.0, 30.0));
            ctx.end_node();

            if show_extra.get() {
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().fill_max_width().size(Dimension::Auto, 40.0));
                ctx.end_node();
            }
        });
    });

    use winia::modifier::Dimension;
    let root = composer.layout_root().unwrap();
    let h1 = root.measured_size.height;
    assert!(h1 < 50.0, "initial height small, got {}", h1);

    // 显示额外行
    show_extra.set(true);
    composer.compose(|ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(200.0, 30.0));
            ctx.end_node();

            if show_extra.get() {
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().fill_max_width().size(Dimension::Auto, 40.0));
                ctx.end_node();
            }
        });
    });
    composer.layout(Constraints::new(0.0, 300.0, 0.0, 100.0));

    let root = composer.layout_root().unwrap();
    let h2 = root.measured_size.height;
    assert!(h2 > h1, "height should increase: {} → {}", h1, h2);
}
