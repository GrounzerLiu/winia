    use super::*;
    use crate::modifier::Modifier;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::theme::{ThemeColors, WiniaTheme};

    /// FlowRow 组合链路集成：content 闭包 4 个固定叶，容器宽 100 → 换行。
    #[test]
    fn flow_row_composes_and_wraps() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                FlowRow::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for _ in 0..4 {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, Modifier::new().size(30.0, 20.0));
                            ctx.end_node();
                        }
                    });
            });
        });
        composer.layout(Constraints::new(100.0, 100.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // 递归找 4 子节点（FlowRow 容器——WiniaTheme scope 无节点，不假设层级）
        fn find_4(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<usize> {
            if nodes[idx].children.len() == 4 { return Some(idx); }
            for &c in &nodes[idx].children {
                if let Some(f) = find_4(nodes, c) { return Some(f); }
            }
            None
        }
        let flow_idx = find_4(nodes, root).expect("应有 4 子 FlowRow 节点");
        let ys: Vec<f32> = nodes[flow_idx].children.iter()
            .map(|&c| nodes[c].position.y)
            .collect();
        assert_eq!(ys, vec![0.0, 0.0, 0.0, 20.0], "前 3 个首行，第 4 个换行，ys={ys:?}");
    }

    /// FlowColumn 组合链路集成：容器高 100 → 换列。
    #[test]
    fn flow_column_composes_and_wraps() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                FlowColumn::new()
                    .modifier(Modifier::new().fill_max_height())
                    .build(ctx, |ctx| {
                        for _ in 0..4 {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, Modifier::new().size(20.0, 30.0));
                            ctx.end_node();
                        }
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 600.0, 100.0, 100.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        fn find_4(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<usize> {
            if nodes[idx].children.len() == 4 { return Some(idx); }
            for &c in &nodes[idx].children {
                if let Some(f) = find_4(nodes, c) { return Some(f); }
            }
            None
        }
        let flow_idx = find_4(nodes, root).expect("应有 4 子 FlowColumn 节点");
        let xs: Vec<f32> = nodes[flow_idx].children.iter()
            .map(|&c| nodes[c].position.x)
            .collect();
        assert_eq!(xs, vec![0.0, 0.0, 0.0, 20.0], "前 3 个首列，第 4 个换列，xs={xs:?}");
    }
