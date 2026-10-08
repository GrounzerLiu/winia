//! Row 布局 — 水平排列子节点（对齐 Compose Row）
//!
//! 主轴 = 水平（宽度），交叉轴 = 垂直（高度）
//!
//! 特性:
//! - per-child 对齐: `Modifier::align_self(Alignment::Center)` 覆盖 Row 默认对齐
//! - weight 权重: `Modifier::weight(2.0)` 按比例分配剩余宽度
//! - spacing: 子节点间固定间距
//!
//! 实现委托到 `flex::measure_flex::<HorizontalAxis>()`。

use super::constraints::Constraints;
use crate::unit::Size;
use super::flex;
use super::node::*;

/// Row 布局策略
#[derive(Debug, Clone)]
pub struct RowLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub spacing: f32,
    pub direction: LayoutDirection,
}

impl RowLayout {
    pub fn new() -> Self {
        RowLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for RowLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for RowLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        flex::measure_flex::<super::axis::HorizontalAxis>(
            self.arrangement,
            self.alignment,
            self.spacing,
            self.direction,
            nodes,
            policies,
            children,
            &constraints,
        )
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }

    // ── Intrinsic measurement ──
    //
    // Row is the horizontal half of Compose's `IntrinsicMeasureBlocks` (RowColumnImpl.kt:261-369):
    // a WIDTH query is the main axis and prices weighted children by weight unit, a HEIGHT query is
    // the cross axis and first resolves how much main-axis room each child gets.

    /// `HorizontalMinWidth`: `intrinsicMainAxisSize(measurables, minIntrinsicWidth(h), availableHeight)`
    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        flex::flex_intrinsic_main(ctx, children, IntrinsicQuery::MinWidth, height, self.spacing)
    }

    /// `HorizontalMaxWidth`
    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        flex::flex_intrinsic_main(ctx, children, IntrinsicQuery::MaxWidth, height, self.spacing)
    }

    /// `HorizontalMinHeight`: `intrinsicCrossAxisSize(measurables, maxIntrinsicWidth, minIntrinsicHeight, availableWidth)`
    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        flex::flex_intrinsic_cross(
            ctx,
            children,
            IntrinsicQuery::MaxWidth,
            IntrinsicQuery::MinHeight,
            width,
            self.spacing,
        )
    }

    /// `HorizontalMaxHeight`
    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        flex::flex_intrinsic_cross(
            ctx,
            children,
            IntrinsicQuery::MaxWidth,
            IntrinsicQuery::MaxHeight,
            width,
            self.spacing,
        )
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(width: f32, height: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
        node.measured_size = Size::new(width, height);
        node
    }

    #[test]
    fn test_row_simple() {
        let row = RowLayout::new();
        let mut nodes = vec![make_leaf(10.0, 100.0), make_leaf(30.0, 80.0), make_leaf(20.0, 120.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 60.0); // 10+30+20
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 10.0);
        assert_eq!(placements[2].position.x, 40.0);
    }

    #[test]
    fn test_row_spacing() {
        let row = RowLayout::new().spacing(5.0);
        let mut nodes = vec![make_leaf(20.0, 100.0), make_leaf(30.0, 80.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 55.0); // 20+5+30
        assert_eq!(placements[1].position.x, 25.0);
    }

    #[test]
    fn test_row_center_alignment() {
        let row = RowLayout::new().alignment(Alignment::Center);
        let mut nodes = vec![make_leaf(50.0, 20.0), make_leaf(50.0, 100.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(placements[0].position.y, (100.0 - 20.0) / 2.0);
        assert_eq!(placements[1].position.y, 0.0);
    }

    #[test]
    fn test_row_rtl_mirror() {
        // RTL：子节点从右到左排列（第一个子在最右）
        let row = RowLayout::new().direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(10.0, 100.0), make_leaf(30.0, 80.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 40.0);
        // 容器宽 40：第一个子（10 宽）在最右 → x = 30
        assert_eq!(placements[0].position.x, 30.0);
        assert_eq!(placements[1].position.x, 0.0);
    }

    #[test]
    fn test_row_rtl_uses_own_width_not_constraint() {
        // 回归：wrap Row 的 RTL 镜像必须用行自身宽度（此前误用 incoming
        // max——子节点被推到约束宽度，wrap 行内容消失/错位）
        let row = RowLayout::new().direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(20.0, 50.0), make_leaf(30.0, 50.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) = row.measure(&mut nodes, &[], &children, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].position.x, 30.0, "第一个子（20 宽）在最右");
        assert_eq!(placements[1].position.x, 0.0);
    }

    #[test]
    fn test_row_rtl_space_between_mirrors_full_width() {
        // SpaceBetween 分配的是容器多余的主轴空间，而宽度由约束给出（min = max = 100），不是这个排列撑
        // 出来的——这正是 Compose 的行为：`Row(SpaceBetween)` 要靠 `fillMaxWidth()` 才有宽度可分配。
        // 对齐前后的差别见 `flex.rs` 的 `measured_main`。
        let row = RowLayout::new()
            .direction(LayoutDirection::Rtl)
            .arrangement(Arrangement::SpaceBetween);
        let mut nodes = vec![make_leaf(20.0, 50.0), make_leaf(30.0, 50.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(100.0, 100.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 100.0, "宽度来自约束");
        // LTR 位置：0 / 70（间距 50）；RTL 镜像后：80 / 0
        assert_eq!(placements[0].position.x, 80.0, "第一个子在最右");
        assert_eq!(placements[1].position.x, 0.0, "第二个子在最左");
    }

    /// Two baselines on one line — the case Compose's `alignBy` exists for, a small label next to
    /// something bigger.
    ///
    /// Measured both ways: WITHOUT the modifier the two texts sit on the row's top edge and their
    /// baselines differ by the difference in their font sizes; with `align_by_baseline` on the larger
    /// one the lines coincide. The control is what makes the assertion mean something — a row that
    /// placed both texts identically would satisfy the aligned case vacuously.
    #[test]
    fn align_by_baseline_puts_two_sizes_on_one_line() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::layout::components::Row;
        use crate::components::text::Text;

        let lines = |aligned: bool| -> (f32, f32) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Row::new().build(ctx, |ctx| {
                    let small = Text::new("small").font_size(12.0);
                    let big = Text::new("BIG").font_size(28.0);
                    if aligned {
                        small.modifier(Modifier::new().align_by_baseline()).build(ctx);
                        big.modifier(Modifier::new().align_by_baseline()).build(ctx);
                    } else {
                        small.build(ctx);
                        big.build(ctx);
                    }
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let nodes = composer.arena_nodes();
            let texts: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].has_text_content).collect();
            assert_eq!(texts.len(), 2, "both texts composed");
            let line_of = |i: usize| {
                nodes[i].position.y
                    + nodes[i]
                        .alignment_line(AlignmentLine::FIRST_BASELINE)
                        .expect("a text leaf publishes its first baseline")
            };
            (line_of(texts[0]), line_of(texts[1]))
        };

        let (small, big) = lines(false);
        assert!(
            (small - big).abs() > 1.0,
            "the control is vacuous unless the two baselines differ without it: {small} vs {big}"
        );

        let (small, big) = lines(true);
        assert_eq!(
            small, big,
            "with `align_by_baseline` the smaller text's baseline lands on the bigger one's"
        );
    }

    /// A line is INHERITED: a container reports the lines its children report, shifted into its own
    /// coordinates and merged — Compose's rule (`ui/layout/AlignmentLine.kt:60-66`). Without it a Row
    /// could only see a DIRECT child's line, so `align_by_baseline` on a Column wrapping text found
    /// nothing and fell back to the Column's top edge.
    ///
    /// Measured both ways like the test above: the control (no modifier on the Column) has the inner
    /// text's baseline somewhere else entirely, and the assertion only means something because of it.
    #[test]
    fn a_baseline_inside_a_child_is_visible_to_the_row() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::layout::components::{Column, Row};
        use crate::components::text::Text;
        use crate::layout::node::LayoutNode;

        /// Every text's baseline in the ROOT's coordinates, walking down so a nested text is measured
        /// where it is actually drawn.
        fn absolute_baselines(composer: &crate::runtime::composer::Composer) -> Vec<f32> {
            fn walk(nodes: &[LayoutNode], idx: usize, y: f32, out: &mut Vec<f32>) {
                let y = y + nodes[idx].position.y;
                if nodes[idx].has_text_content {
                    if let Some(line) = nodes[idx].alignment_line(AlignmentLine::FIRST_BASELINE) {
                        out.push(y + line);
                    }
                }
                for &c in &nodes[idx].children {
                    walk(nodes, c, y, out);
                }
            }
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            if let Some(root) = composer.layout_root_idx() {
                walk(nodes, root, 0.0, &mut out);
            }
            out.sort_by(|a, b| a.partial_cmp(b).unwrap());
            out
        }

        let baselines = |aligned: bool| -> Vec<f32> {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Row::new().build(ctx, |ctx| {
                    // The small text is one level down, inside a Column. BOTH children carry the
                    // modifier: a line-aligned child is placed by ITS line, so a single aligned child
                    // among unaligned ones has nothing to meet — the lesson the test above records.
                    // What is under test here is that the COLUMN's line is the inner text's.
                    let column = Column::new();
                    let column = if aligned {
                        column.modifier(Modifier::new().align_by_baseline())
                    } else {
                        column
                    };
                    column.build(ctx, |ctx| {
                        Text::new("small").font_size(12.0).build(ctx);
                    });
                    let big = Text::new("BIG").font_size(28.0);
                    if aligned {
                        big.modifier(Modifier::new().align_by_baseline()).build(ctx);
                    } else {
                        big.build(ctx);
                    }
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            absolute_baselines(&composer)
        };

        let control = baselines(false);
        assert_eq!(control.len(), 2, "both texts composed");
        assert!(
            (control[0] - control[1]).abs() > 1.0,
            "the control is vacuous unless the two baselines differ without it: {control:?}"
        );

        let aligned = baselines(true);
        assert_eq!(
            aligned[0], aligned[1],
            "the row aligns by the baseline INSIDE the column, not by the column's top edge"
        );
    }

    /// `Modifier::padding_from_baseline` puts the BASELINE where the caller asks, not the box's edge —
    /// Compose's `Modifier.paddingFrom` (`foundation/layout/AlignmentLine.kt:65`), whose measure is
    /// `paddingBefore = (before - line).coerceIn(0, axisMax - axis)`.
    ///
    /// Measured both ways, because a box that simply grew would satisfy nothing: the control (no
    /// modifier) is the text's natural size, and with `top = 40` the box must be taller and the
    /// baseline must sit exactly 40 from its top. The baseline's own distance from the text's top is
    /// unchanged — the text did not move inside its own box; the box grew around it.
    #[test]
    fn padding_from_baseline_puts_the_baseline_where_it_was_asked() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::components::text::Text;

        let measure = |padded: bool| -> (f32, f32, f32) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                let text = Text::new("Label").font_size(16.0);
                let text = if padded {
                    text.modifier(Modifier::new().padding_from_baseline(Some(40.0), None))
                } else {
                    text
                };
                text.build(ctx);
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("laid out");
            let node = &nodes[root];
            let line = node
                .alignment_line(AlignmentLine::FIRST_BASELINE)
                .expect("the text reports its baseline");
            (node.measured_size.height, line, node.position.y)
        };

        let (natural_h, natural_line, _) = measure(false);
        let (padded_h, padded_line, _) = measure(true);
        assert!(
            natural_line < 40.0,
            "the control must be a case the padding actually changes (baseline {natural_line})"
        );
        assert_eq!(
            padded_line, 40.0,
            "the baseline sits 40 from the box's top"
        );
        assert_eq!(
            padded_h,
            natural_h - natural_line + 40.0,
            "and the box grew by exactly the difference: {natural_h} (baseline {natural_line}) -> {padded_h}"
        );
    }

    /// The line a `paddingFrom` reads is the node's OWN reported one, which for a container is the
    /// inherited line of its content — so the padding can be asked of a Column and still land on the
    /// text inside it.
    #[test]
    fn padding_from_baseline_reads_an_inherited_baseline() {
        use crate::layout::AlignmentLine;
        use crate::layout::components::Column;
        use crate::modifier::Modifier;
        use crate::components::text::Text;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Column::new()
                .modifier(Modifier::new().padding_from_baseline(Some(30.0), None))
                .build(ctx, |ctx| {
                    Text::new("inner").font_size(14.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let column = composer.layout_root_idx().expect("laid out");
        let text = nodes[column].children[0];
        let text_line = nodes[text]
            .alignment_line(AlignmentLine::FIRST_BASELINE)
            .expect("the text reports its baseline");
        let absolute = nodes[text].position.y + text_line;
        assert_eq!(
            absolute, 30.0,
            "the Column's padding is computed against the text's inherited baseline"
        );
        assert_eq!(
            nodes[column].alignment_line(AlignmentLine::FIRST_BASELINE),
            Some(30.0),
            "and the Column's own line moved with the padding, so an outer row still sees it"
        );
    }

    /// `after` with no `before`: the content is placed against the FAR edge instead — Compose's
    /// `size - paddingAfter - axis` branch (`AlignmentLine.kt:348-354`), and here the line is
    /// `LastBaseline`, whose merger is `::max`.
    #[test]
    fn padding_from_the_last_baseline_places_by_the_bottom() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::components::text::Text;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("one two three four five six seven eight")
                .font_size(14.0)
                // Narrow enough to wrap, which is what makes First and Last differ.
                .modifier(
                    Modifier::new()
                        .width(60.0)
                        .padding_from(AlignmentLine::LAST_BASELINE, None, Some(12.0)),
                )
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("laid out");
        let node = &nodes[root];
        let first = node
            .alignment_line(AlignmentLine::FIRST_BASELINE)
            .expect("first baseline");
        let last = node
            .alignment_line(AlignmentLine::LAST_BASELINE)
            .expect("last baseline");
        assert!(last > first, "the text wrapped: {first} vs {last}");
        assert_eq!(
            node.measured_size.height - last,
            12.0,
            "the LAST baseline sits 12 above the box's bottom"
        );
    }

    /// The incoming maximum caps the padding, and `before` takes what there is — Compose's contract
    /// (`foundation/layout/AlignmentLine.kt:44-49`: "when the max constraints do not allow this,
    /// satisfying the `before` requirement will have priority over `after`"), which falls out of its
    /// two `coerceIn`s.
    #[test]
    fn padding_from_gives_the_maximum_to_before() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::components::text::Text;

        let measure = |padded: bool| -> (f32, f32) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                let text = Text::new("Label").font_size(16.0);
                let text = if padded {
                    text.modifier(Modifier::new().padding_from_baseline(
                        Some(40.0),
                        Some(40.0),
                    ))
                } else {
                    text
                };
                text.build(ctx);
            });
            // 30 of height in total: 40 above the baseline and 40 below it cannot both fit. (The
            // first version used 25 and 25, which DID fit — measured, and the test passed the branch
            // it was meant to exercise without ever entering it.)
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 30.0));
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("laid out");
            (
                nodes[root].measured_size.height,
                nodes[root]
                    .alignment_line(AlignmentLine::FIRST_BASELINE)
                    .expect("baseline"),
            )
        };

        let (natural_h, natural_line) = measure(false);
        let (padded_h, padded_line) = measure(true);
        assert_eq!(padded_h, 30.0, "the box stops at the incoming maximum");
        assert_eq!(
            padded_line,
            30.0 - natural_h + natural_line,
            "…and the content is pushed down as far as the space allows, so `after` got nothing"
        );
    }

    /// A minimum larger than the padded content is satisfied, and the content still sits at `before`
    /// — Compose's "position the content to satisfy the `before` requirement if specified"
    /// (`foundation/layout/AlignmentLine.kt:50-55`).
    #[test]
    fn padding_from_satisfies_a_minimum_without_moving_the_line() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::components::text::Text;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("Label")
                .font_size(16.0)
                // Larger than the natural baseline (~13 for 16 pt), or there would be no padding to
                // place at all — the first version asked for 10 and measured the untouched 12.9.
                .modifier(Modifier::new().padding_from_baseline(Some(40.0), None))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 300.0, 100.0, 300.0));
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("laid out");
        assert_eq!(
            nodes[root].measured_size.height, 100.0,
            "the minimum wins over the padded content"
        );
        assert_eq!(
            nodes[root]
                .alignment_line(AlignmentLine::FIRST_BASELINE)
                .expect("baseline"),
            40.0,
            "and the line is still 40 from the top, not centred in the leftover space"
        );

        // The control: without the modifier the text fills the min-height on its own, which is why
        // the relaxation above has to happen BEFORE the content is measured — measured with the
        // padding added on top of that 100, the box came out 122.73 tall.
        let mut plain = crate::runtime::composer::Composer::new();
        plain.compose(|ctx| {
            Text::new("Label").font_size(16.0).build(ctx);
        });
        plain.layout(Constraints::new(0.0, 300.0, 100.0, 300.0));
        let plain_nodes = plain.arena_nodes();
        let plain_root = plain.layout_root_idx().expect("laid out");
        assert_eq!(
            plain_nodes[plain_root].measured_size.height, 100.0,
            "the natural size under this constraint IS the minimum"
        );
    }

    /// A line-aligned child whose text WRAPS below the line makes the row taller than any child in
    /// it: Compose sizes the cross axis to `beforeCrossAxisAlignmentLine +
    /// afterCrossAxisAlignmentLine`, not to the tallest child (`RowColumnMeasurePolicy.kt:253-259`),
    /// because a line pinned at `before` needs room underneath for whatever hangs below it.
    ///
    /// Here the small text wraps to several lines, so its first baseline sits high while its box runs
    /// well below the big text's.
    #[test]
    fn a_wrapped_child_below_the_line_makes_the_row_taller_than_its_tallest_child() {
        use crate::layout::AlignmentLine;
        use crate::modifier::Modifier;
        use crate::layout::components::Row;
        use crate::components::text::Text;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Row::new().build(ctx, |ctx| {
                Text::new("BIG")
                    .font_size(28.0)
                    .modifier(Modifier::new().align_by_baseline())
                    .build(ctx);
                Text::new("a small text that wraps")
                    .font_size(12.0)
                    // Narrow enough that the label needs several lines.
                    .modifier(Modifier::new().width(40.0).align_by_baseline())
                    .build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let nodes = composer.arena_nodes();
        let texts: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].has_text_content).collect();
        assert_eq!(texts.len(), 2, "both texts composed");
        let line_of = |i: usize| {
            nodes[i].position.y
                + nodes[i]
                    .alignment_line(AlignmentLine::FIRST_BASELINE)
                    .expect("a text leaf publishes its first baseline")
        };
        assert_eq!(
            line_of(texts[0]),
            line_of(texts[1]),
            "the two baselines still land together"
        );
        let tallest = texts
            .iter()
            .map(|&t| nodes[t].measured_size.height)
            .fold(0.0f32, f32::max);
        let row_height = nodes[composer.layout_root_idx().unwrap()].measured_size.height;
        assert!(
            row_height > tallest,
            "the row must make room for what hangs below the line: row {row_height} against the \
             tallest child {tallest}"
        );
    }

    /// Both of Compose's text baselines are published, and a container MERGES its children's values
    /// through each line's own merger: `FirstBaseline` is `::min` and `LastBaseline` is
    /// `::max` (`ui/layout/AlignmentLine.kt:94-103`).
    ///
    /// A wrapped text is what separates them: its first baseline sits on the first line, its last on
    /// the final one. The Column around it then reports the minimum for the first line and the
    /// maximum for the last — the same values, shifted into the Column's coordinates.
    #[test]
    fn a_container_merges_its_childrens_baselines() {
        use crate::layout::AlignmentLine;
        use crate::layout::components::Column;
        use crate::components::text::Text;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Column::new().build(ctx, |ctx| {
                // Narrow enough to wrap into several lines.
                Text::new("a small text that wraps").font_size(12.0).modifier(crate::modifier::Modifier::new().width(40.0)).build(ctx);
                Text::new("second").font_size(12.0).build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let nodes = composer.arena_nodes();
        let column = composer.layout_root_idx().expect("laid out");
        let texts: Vec<usize> = (0..nodes.len())
            .filter(|&i| nodes[i].has_text_content)
            .collect();
        assert_eq!(texts.len(), 2, "both texts composed");

        let wrapped = texts[0];
        let first = nodes[wrapped]
            .alignment_line(AlignmentLine::FIRST_BASELINE)
            .expect("the wrapped text reports its first baseline");
        let last = nodes[wrapped]
            .alignment_line(AlignmentLine::LAST_BASELINE)
            .expect("and its last");
        assert!(
            last > first,
            "a wrapped text's last baseline is below its first: {first} vs {last}"
        );

        // The Column merged: FIRST is the minimum over its children (shifted by their positions),
        // LAST the maximum.
        let column_first = nodes[column]
            .alignment_line(AlignmentLine::FIRST_BASELINE)
            .expect("the column inherits the line");
        let column_last = nodes[column]
            .alignment_line(AlignmentLine::LAST_BASELINE)
            .expect("and the last one");
        let expected_first = nodes[wrapped].position.y + first;
        let expected_last = texts
            .iter()
            .map(|&t| {
                nodes[t].position.y
                    + nodes[t]
                        .alignment_line(AlignmentLine::LAST_BASELINE)
                        .expect("both texts report a last baseline")
            })
            .fold(f32::MIN, f32::max);
        assert_eq!(
            column_first, expected_first,
            "FirstBaseline merges with ::min — the first line of the first child"
        );
        assert_eq!(
            column_last, expected_last,
            "LastBaseline merges with ::max — the last line of the lowest child"
        );
    }
}
