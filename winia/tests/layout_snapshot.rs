//! 布局快照测试 — 不渲染像素，直接验证 LayoutNode 树结构。
//!
//! compose + layout 后检查节点的 position、size、children 关系。

use winia::core::composer::Composer;
use winia::layout::constraints::Constraints;
use winia::modifier::Modifier;
use winia::ui::{Text, Column, Row};

#[test]
fn leaf_node_has_correct_size() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(100.0, 50.0));
        ctx.end_node();
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

    let root = composer.layout_root().expect("root exists");
    assert_eq!(root.measured_size.width, 100.0);
    assert_eq!(root.measured_size.height, 50.0);
    assert_eq!(root.position.x, 0.0);
    assert_eq!(root.position.y, 0.0);
    assert!(root.children.is_empty());
}

#[test]
fn column_has_two_children() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(200.0, 30.0));
            ctx.end_node();
            let k2 = ctx.next_key();
            ctx.start_leaf(k2, Modifier::new().size(200.0, 20.0));
            ctx.end_node();
        });
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));

    let root = composer.layout_root().expect("root exists");
    assert_eq!(root.children.len(), 2, "Column should have 2 children");
}

#[test]
fn column_children_stacked_vertically() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(200.0, 30.0));
            ctx.end_node();
            let k2 = ctx.next_key();
            ctx.start_leaf(k2, Modifier::new().size(200.0, 20.0));
            ctx.end_node();
        });
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));

    let root = composer.layout_root().expect("root exists");
    let child0 = &root.children[0];
    let child1 = &root.children[1];

    // 第一个子节点 y=0
    assert_eq!(child0.position.y, 0.0);
    assert_eq!(child0.measured_size.height, 30.0);

    // 第二个子节点 y = 第一个的高度（垂直堆叠）
    assert_eq!(child1.position.y, 30.0);
    assert_eq!(child1.measured_size.height, 20.0);
}

#[test]
fn row_children_arranged_horizontally() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Row::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().size(50.0, 40.0));
            ctx.end_node();
            let k2 = ctx.next_key();
            ctx.start_leaf(k2, Modifier::new().size(70.0, 40.0));
            ctx.end_node();
        });
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 40.0));

    let root = composer.layout_root().expect("root exists");
    let child0 = &root.children[0];
    let child1 = &root.children[1];

    assert_eq!(child0.position.x, 0.0);
    assert_eq!(child0.measured_size.width, 50.0);

    assert_eq!(child1.position.x, 50.0); // 紧接第一个之后
    assert_eq!(child1.measured_size.width, 70.0);
}

#[test]
fn padding_offsets_children() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new()
            .modifier(Modifier::new().padding(10.0))
            .build(ctx, |ctx| {
                let k1 = ctx.next_key();
                ctx.start_leaf(k1, Modifier::new().size(100.0, 20.0));
                ctx.end_node();
            });
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

    let root = composer.layout_root().expect("root exists");
    let child = &root.children[0];

    // padding 10 应该让子节点偏移 (10, 10)
    assert_eq!(child.position.x, 10.0, "padding should offset x");
    assert_eq!(child.position.y, 10.0, "padding should offset y");
}

#[test]
fn fill_max_width_expands_to_parent() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().build(ctx, |ctx| {
            let k1 = ctx.next_key();
            ctx.start_leaf(k1, Modifier::new().fill_max_width().size(Dimension::Auto, 20.0));
            ctx.end_node();
        });
    });

    use winia::modifier::Dimension;
    composer.layout(Constraints::new(0.0, 300.0, 0.0, 100.0));

    let root = composer.layout_root().expect("root exists");
    let child = &root.children[0];
    assert_eq!(child.measured_size.width, 300.0, "fill_max_width child should fill parent width");
}

#[test]
fn text_leaf_has_measured_size() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Text::new("Hello").font_size(20.0).build(ctx);
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 100.0));

    let root = composer.layout_root().expect("root exists");
    assert!(root.measured_size.width > 0.0, "text should have non-zero width");
    assert!(root.measured_size.height > 0.0, "text should have non-zero height");
}

#[test]
fn nested_column_in_row() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Row::new().build(ctx, |ctx| {
            Column::new().modifier(Modifier::new().size(80.0, 60.0)).build(ctx, |ctx| {
                let k1 = ctx.next_key();
                ctx.start_leaf(k1, Modifier::new().size(80.0, 30.0));
                ctx.end_node();
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().size(80.0, 30.0));
                ctx.end_node();
            });
            // 第二个 Column
            Column::new().modifier(Modifier::new().fill_max_width().fill_max_height()).build(ctx, |ctx| {
                let k3 = ctx.next_key();
                ctx.start_leaf(k3, Modifier::new().fill_max_width().fill_max_height());
                ctx.end_node();
            });
        });
    });
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 60.0));

    let root = composer.layout_root().expect("root exists");
    assert_eq!(root.children.len(), 2, "Row should have 2 Column children");

    // 第一个 Column 宽度 80
    let col0 = &root.children[0];
    assert_eq!(col0.measured_size.width, 80.0);
    assert_eq!(col0.children.len(), 2);

    // 第二个 Column 填满剩余宽度 120
    let col1 = &root.children[1];
    assert_eq!(col1.measured_size.width, 120.0);
    assert_eq!(col1.children.len(), 1);
}
