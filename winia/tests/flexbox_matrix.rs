//! FlexBox across a matrix of parameter combinations.
//!
//! The unit tests in the module pin one behaviour each; this file sweeps the combinations and asserts
//! INVARIANTS every correct flex layout satisfies, so a combination nobody wrote a case for still
//! fails loudly. A failing case prints the whole configuration, which is what makes it reproducible.

use winia::layout::{
    Constraints, FlexAlignContent, FlexAlignItems, FlexAlignSelf, FlexBasis, FlexBoxConfig,
    FlexBoxLayout, FlexDirection, FlexItemConfig, FlexJustifyContent, FlexWrap, LayoutNode,
    MeasurePolicy,
};
use winia::modifier::Modifier;
use winia::unit::Size;

/// A small deterministic pattern of item sizes and flex configs.
fn item(index: usize, pattern: usize) -> (LayoutNode, FlexItemConfig) {
    let (width, height) = match pattern {
        0 => (40.0, 20.0),
        1 => (96.0, 24.0 + (index % 3) as f32 * 16.0),
        _ => (60.0 + (index % 4) as f32 * 30.0, 20.0 + (index % 2) as f32 * 40.0),
    };
    let mut config = FlexItemConfig::new();
    config = match index % 3 {
        0 => config.basis_dp(width).grow(1.0),
        1 => config.basis(FlexBasis::Percent(0.2)).shrink(1.0),
        _ => config,
    };
    if index % 5 == 4 {
        config = config.align_self(FlexAlignSelf::Center);
    }
    let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
    node.measured_size = Size::new(width, height);
    (node, config)
}

fn item_node(index: usize, pattern: usize) -> LayoutNode {
    let (node, config) = item(index, pattern);
    LayoutNode::leaf(Modifier::new().size(
        node.measured_size.width,
        node.measured_size.height,
    )
    .flex(config))
}

#[derive(Clone, Copy)]
struct Case {
    direction: FlexDirection,
    wrap: FlexWrap,
    justify: FlexJustifyContent,
    align_items: FlexAlignItems,
    align_content: FlexAlignContent,
    count: usize,
    pattern: usize,
    constraints: Constraints,
}

impl std::fmt::Display for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ {:?} {:?} {:?} {:?} {:?} count={} pattern={} constraints=({}..{}, {}..{}) }}",
            self.direction,
            self.wrap,
            self.justify,
            self.align_items,
            self.align_content,
            self.count,
            self.pattern,
            self.constraints.min_width,
            self.constraints.max_width,
            self.constraints.min_height,
            self.constraints.max_height,
        )
    }
}

fn run(case: &Case) -> (Size, Vec<(f32, f32, f32, f32)>) {
    let mut nodes: Vec<LayoutNode> = (0..case.count)
        .map(|index| item_node(index, case.pattern))
        .collect();
    let children: Vec<usize> = (0..nodes.len()).collect();
    let mut config = FlexBoxConfig::new()
        .direction(case.direction)
        .wrap(case.wrap)
        .justify_content(case.justify)
        .align_items(case.align_items)
        .align_content(case.align_content)
        .gap(8.0);
    if case.wrap != FlexWrap::NoWrap && case.count > 2 {
        config = config.max_items_in_each_line(0);
    }
    let policy = FlexBoxLayout::new(config);
    let (size, placements) = policy.measure(&mut nodes, &[], &children, case.constraints);
    policy.place(&mut nodes, &children, &placements);
    // The placement must agree with what the child actually measured.
    for (index, child) in children.iter().enumerate() {
        let measured = nodes[*child].measured_size;
        let placed = placements[index].size;
        assert!(
            (measured.width - placed.width).abs() < 0.01 && (measured.height - placed.height).abs() < 0.01,
            "placement disagrees with the measured child for {case}: child {index} measured {measured:?} placed {placed:?}"
        );
    }
    let rects = placements
        .iter()
        .map(|placement| {
            (
                placement.position.x,
                placement.position.y,
                placement.size.width,
                placement.size.height,
            )
        })
        .collect();
    (size, rects)
}

#[test]
fn the_matrix_respects_the_layout_invariants() {
    let directions = [
        FlexDirection::Row,
        FlexDirection::RowReverse,
        FlexDirection::Column,
        FlexDirection::ColumnReverse,
    ];
    let wraps = [FlexWrap::NoWrap, FlexWrap::Wrap, FlexWrap::WrapReverse];
    let justifies = [
        FlexJustifyContent::Start,
        FlexJustifyContent::End,
        FlexJustifyContent::Center,
        FlexJustifyContent::SpaceBetween,
        FlexJustifyContent::SpaceAround,
        FlexJustifyContent::SpaceEvenly,
    ];
    let aligns = [
        FlexAlignItems::Start,
        FlexAlignItems::End,
        FlexAlignItems::Center,
        FlexAlignItems::Stretch,
        FlexAlignItems::Baseline,
    ];
    let contents = [
        FlexAlignContent::Start,
        FlexAlignContent::End,
        FlexAlignContent::Center,
        FlexAlignContent::Stretch,
        FlexAlignContent::SpaceBetween,
        FlexAlignContent::SpaceAround,
    ];
    let constraint_sets = [
        Constraints::new(300.0, 300.0, 200.0, 200.0),
        Constraints::new(0.0, 240.0, 0.0, 160.0),
        Constraints::new(0.0, 600.0, 0.0, 400.0),
        Constraints::new(200.0, 200.0, 60.0, 60.0),
    ];

    let mut checked = 0usize;
    for &direction in &directions {
        for &wrap in &wraps {
            for &justify in &justifies {
                for &align_items in &aligns {
                    for &align_content in &contents {
                        for &count in &[1usize, 2, 3, 5] {
                            for &pattern in &[0usize, 1, 2] {
                                for &constraints in &constraint_sets {
                                    let case = Case {
                                        direction,
                                        wrap,
                                        justify,
                                        align_items,
                                        align_content,
                                        count,
                                        pattern,
                                        constraints,
                                    };
                                    let (size, rects) = run(&case);
                                    checked += 1;

                                    // 1. A finite, non-negative container inside the incoming range.
                                    assert!(
                                        size.width.is_finite()
                                            && size.height.is_finite()
                                            && size.width >= 0.0
                                            && size.height >= 0.0,
                                        "non-finite or negative container size for {case}: {size:?}"
                                    );
                                    assert!(
                                        size.width >= constraints.min_width - 0.01
                                            && size.height >= constraints.min_height - 0.01,
                                        "container below the incoming minimum for {case}: {size:?}"
                                    );
                                    assert!(
                                        size.width <= constraints.max_width + 0.01
                                            && size.height <= constraints.max_height + 0.01,
                                        "container above the incoming maximum for {case}: {size:?}"
                                    );

                                    // 2. Finite, non-negative placements.
                                    for (index, rect) in rects.iter().enumerate() {
                                        assert!(
                                            rect.0.is_finite()
                                                && rect.1.is_finite()
                                                && rect.2.is_finite()
                                                && rect.3.is_finite()
                                                && rect.2 >= 0.0
                                                && rect.3 >= 0.0,
                                            "non-finite or negative placement for {case}: item {index} {rect:?}"
                                        );
                                    }

                                    // 3. Determinism.
                                    let (size_again, rects_again) = run(&case);
                                    assert_eq!(
                                        size, size_again,
                                        "the same inputs must produce the same size for {case}"
                                    );
                                    assert_eq!(
                                        rects, rects_again,
                                        "the same inputs must produce the same placements for {case}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 10_000, "the matrix ran {checked} cases");
}

/// Behavioural invariants on top of the shape checks: order, no overlap, the exact justify/align
/// arithmetic, and the mirror relations that tie the reverse and RTL directions to the forward one.
#[test]
fn the_matrix_respects_the_behavioural_invariants() {
    let constraint_sets = [
        Constraints::new(320.0, 320.0, 200.0, 200.0),
        Constraints::new(0.0, 260.0, 0.0, 180.0),
        Constraints::new(0.0, 700.0, 0.0, 500.0),
    ];
    let mut checked = 0usize;

    for &constraints in &constraint_sets {
        for &wrap in &[FlexWrap::NoWrap, FlexWrap::Wrap] {
            for &justify in &[
                FlexJustifyContent::Start,
                FlexJustifyContent::End,
                FlexJustifyContent::Center,
                FlexJustifyContent::SpaceBetween,
            ] {
                for &align in &[
                    FlexAlignItems::Start,
                    FlexAlignItems::End,
                    FlexAlignItems::Center,
                ] {
                    for &count in &[1usize, 2, 3, 5] {
                        let base = Case {
                            direction: FlexDirection::Row,
                            wrap,
                            justify,
                            align_items: align,
                            align_content: FlexAlignContent::Start,
                            count,
                            pattern: 0,
                            constraints,
                        };
                        let (size, rects) = run(&base);
                        checked += 1;
                        let horizontal = true;
                        let main_size = size.width;
                        let cross_size = size.height;

                        // Order: with justify Start and no reverse the items run forward.
                        if justify == FlexJustifyContent::Start {
                            for pair in rects.windows(2) {
                                if (pair[0].1 - pair[1].1).abs() < 0.5 {
                                    assert!(
                                        pair[1].0 >= pair[0].0 - 0.01,
                                        "Start must keep the item order for {base}: {rects:?}"
                                    );
                                }
                            }
                        }

                        // No overlap of items on the same line (all pattern-0 items shrink).
                        for a in 0..rects.len() {
                            for b in (a + 1)..rects.len() {
                                let (ax, ay, aw, ah) = rects[a];
                                let (bx, by, bw, bh) = rects[b];
                                if (ay - by).abs() > 0.5 {
                                    continue;
                                }
                                let overlap = ax < bx + bw - 0.01 && bx < ax + aw - 0.01;
                                assert!(
                                    !overlap,
                                    "two items of one line overlap for {base}: {a} {rects:?}"
                                );
                            }
                        }

                        // Only one line when wrap is off: every item's main extent is inside the
                        // container (the line fit).
                        if wrap == FlexWrap::NoWrap {
                            for (index, rect) in rects.iter().enumerate() {
                                assert!(
                                    rect.0 >= -0.01 && rect.0 + rect.2 <= main_size + 0.01,
                                    "an item of the single line leaves the container for {base}: item {index} {rects:?} size {size:?}"
                                );
                            }
                        }

                        // Justify exactness on a single line.
                        if wrap == FlexWrap::NoWrap && count >= 2 {
                            let first = rects[0];
                            let last = *rects.last().unwrap();
                            match justify {
                                FlexJustifyContent::Start => {
                                    assert!(first.0.abs() < 0.01, "Start starts at 0 for {base}: {rects:?}")
                                }
                                FlexJustifyContent::End => {
                                    assert!(
                                        (last.0 + last.2 - main_size).abs() < 0.01,
                                        "End packs the last item against the end for {base}: {rects:?} size {size:?}"
                                    )
                                }
                                FlexJustifyContent::Center => {
                                    let left = first.0;
                                    let right = main_size - (last.0 + last.2);
                                    assert!(
                                        (left - right).abs() < 0.01,
                                        "Center leaves equal margins for {base}: {left} vs {right} {rects:?}"
                                    )
                                }
                                FlexJustifyContent::SpaceBetween => {
                                    assert!(
                                        first.0.abs() < 0.01
                                            && (last.0 + last.2 - main_size).abs() < 0.01,
                                        "SpaceBetween pins both ends for {base}: {rects:?} size {size:?}"
                                    )
                                }
                                _ => {}
                            }
                        }

                        // Align exactness on a single line: the cross extent is the line's extent,
                        // which is the container's when wrap is off.
                        if wrap == FlexWrap::NoWrap {
                            let line_cross = cross_size;
                            for (index, rect) in rects.iter().enumerate() {
                                // The pattern gives every fifth item its own align-self, which the
                                // container's align-items must not override.
                                if index % 5 == 4 {
                                    continue;
                                }
                                let (position, extent) = (rect.1, rect.3);
                                let expected = match align {
                                    FlexAlignItems::Start => 0.0,
                                    FlexAlignItems::End => line_cross - extent,
                                    FlexAlignItems::Center => (line_cross - extent) / 2.0,
                                    _ => position,
                                };
                                assert!(
                                    (position - expected).abs() < 0.5,
                                    "align {align:?} places item {index} at {position}, expected {expected} for {base}: {rects:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 100, "the behavioural matrix ran {checked} cases");
}

/// The mirror relation: reversing the main axis (or flipping RTL) must produce the mirrored geometry.
#[test]
fn the_matrix_mirrors_with_the_reverse_and_rtl_directions() {
    let constraints = Constraints::new(320.0, 320.0, 200.0, 200.0);
    for &justify in &[
        FlexJustifyContent::Start,
        FlexJustifyContent::End,
        FlexJustifyContent::Center,
    ] {
        for &count in &[1usize, 2, 3] {
            let forward = Case {
                direction: FlexDirection::Row,
                wrap: FlexWrap::NoWrap,
                justify,
                align_items: FlexAlignItems::Start,
                align_content: FlexAlignContent::Start,
                count,
                pattern: 0,
                constraints,
            };
            let mut reversed = forward;
            reversed.direction = FlexDirection::RowReverse;
            let (forward_size, forward_rects) = run(&forward);
            let (reverse_size, reverse_rects) = run(&reversed);
            assert!(
                (forward_size.width - reverse_size.width).abs() < 0.01,
                "a reverse direction does not change the container for {forward}: {forward_size:?} {reverse_size:?}"
            );
            let mut expected: Vec<f32> = forward_rects
                .iter()
                .map(|rect| forward_size.width - rect.0 - rect.2)
                .collect();
            let mut actual: Vec<f32> = reverse_rects.iter().map(|rect| rect.0).collect();
            expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
            actual.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for (a, b) in expected.iter().zip(actual.iter()) {
                assert!(
                    (a - b).abs() < 0.5,
                    "RowReverse must mirror the geometry for {forward}: expected {expected:?} got {actual:?} ({forward_rects:?} / {reverse_rects:?})"
                );
            }
        }
    }
}

/// The same behavioural invariants on the COLUMN axis, plus the unbounded axis the earlier sweep never
/// used — an axis with no maximum is where the basis / percent / justify arithmetic changes shape.
#[test]
fn the_matrix_holds_on_the_column_axis_and_unbounded_axes() {
    let constraint_sets = [
        Constraints::new(200.0, 200.0, 320.0, 320.0),
        Constraints::new(0.0, 160.0, 0.0, 240.0),
        Constraints::new(0.0, 200.0, 0.0, f32::MAX),
        Constraints::new(0.0, f32::MAX, 0.0, 240.0),
    ];
    let mut checked = 0usize;
    for &constraints in &constraint_sets {
        for &justify in &[
            FlexJustifyContent::Start,
            FlexJustifyContent::End,
            FlexJustifyContent::Center,
            FlexJustifyContent::SpaceBetween,
        ] {
            for &align in &[
                FlexAlignItems::Start,
                FlexAlignItems::End,
                FlexAlignItems::Center,
            ] {
                for &count in &[1usize, 2, 3] {
                    let case = Case {
                        direction: FlexDirection::Column,
                        wrap: FlexWrap::NoWrap,
                        justify,
                        align_items: align,
                        align_content: FlexAlignContent::Start,
                        count,
                        pattern: 0,
                        constraints,
                    };
                    let (size, rects) = run(&case);
                    checked += 1;
                    assert!(size.width.is_finite() && size.height.is_finite(), "finite for {case}: {size:?}");
                    assert!(
                        size.width >= constraints.min_width - 0.01 && size.height >= constraints.min_height - 0.01,
                        "floored by the minimums for {case}: {size:?}"
                    );
                    if constraints.max_width < f32::MAX {
                        assert!(size.width <= constraints.max_width + 0.01, "capped on width for {case}: {size:?}");
                    }
                    if constraints.max_height < f32::MAX {
                        assert!(size.height <= constraints.max_height + 0.01, "capped on height for {case}: {size:?}");
                    }
                    // Order down the column.
                    if justify == FlexJustifyContent::Start {
                        for pair in rects.windows(2) {
                            if (pair[0].0 - pair[1].0).abs() < 0.5 {
                                assert!(pair[1].1 >= pair[0].1 - 0.01, "column order for {case}: {rects:?}");
                            }
                        }
                    }
                    // No overlap within the line.
                    for a in 0..rects.len() {
                        for b in (a + 1)..rects.len() {
                            let (ax, ay, aw, ah) = rects[a];
                            let (bx, by, bw, bh) = rects[b];
                            let overlap = ax < bx + bw - 0.01
                                && bx < ax + aw - 0.01
                                && ay < by + bh - 0.01
                                && by < ay + ah - 0.01;
                            assert!(!overlap, "column items overlap for {case}: {rects:?}");
                        }
                    }
                    // Justify exactness down the column (only when the axis is bounded and the
                    // content fits: an unbounded column is as tall as its content, so Start is 0
                    // anyway and End/Center have no leftover).
                    if constraints.max_height < f32::MAX && count >= 2 {
                        let first = rects[0];
                        let last = *rects.last().unwrap();
                        let main_size = size.height;
                        match justify {
                            FlexJustifyContent::Start => {
                                assert!(first.1.abs() < 0.01, "column Start at 0 for {case}: {rects:?}")
                            }
                            FlexJustifyContent::End => {
                                assert!(
                                    (last.1 + last.3 - main_size).abs() < 0.01,
                                    "column End packs to the bottom for {case}: {rects:?} size {size:?}"
                                )
                            }
                            FlexJustifyContent::Center => {
                                let top = first.1;
                                let bottom = main_size - (last.1 + last.3);
                                assert!(
                                    (top - bottom).abs() < 0.01,
                                    "column Center margins {top} vs {bottom} for {case}: {rects:?}"
                                )
                            }
                            FlexJustifyContent::SpaceBetween => {
                                assert!(
                                    first.1.abs() < 0.01 && (last.1 + last.3 - main_size).abs() < 0.01,
                                    "column SpaceBetween pins both ends for {case}: {rects:?} size {size:?}"
                                )
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 100, "the column matrix ran {checked} cases");
}

/// The lines themselves: align-content positions them across the cross axis, and WrapReverse flips the
/// order. Every line is one item here, so the line index is the item index.
#[test]
fn the_matrix_distributes_the_wrapped_lines() {
    let constraints = Constraints::new(0.0, 150.0, 240.0, 240.0);
    // (rows; the column variant is below)
    for &align_content in &[
        FlexAlignContent::Start,
        FlexAlignContent::End,
        FlexAlignContent::Center,
        FlexAlignContent::Stretch,
        FlexAlignContent::SpaceBetween,
        FlexAlignContent::SpaceAround,
    ] {
        for &wrap in &[FlexWrap::Wrap, FlexWrap::WrapReverse] {
            let case = Case {
                direction: FlexDirection::Row,
                wrap,
                justify: FlexJustifyContent::Start,
                align_items: FlexAlignItems::Start,
                align_content,
                count: 3,
                pattern: 0,
                constraints,
            };
            let (size, rects) = run(&case);
            let mut ys: Vec<f32> = rects.iter().map(|rect| rect.1).collect();
            assert!(
                ys.iter().all(|y| y.is_finite()),
                "finite line positions for {case}: {rects:?}"
            );
            // The items are 20 tall; three lines plus two 8 dp gaps take 76 of 240, so 164 is free.
            let free = 240.0 - (3.0 * 20.0 + 2.0 * 8.0);
            let mut sorted = ys.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let (expected_top, expected_pitch) = match align_content {
                // Start packs the lines at the cross-start; WrapReverse starts at the cross-END, so
                // the block sits at the far side.
                FlexAlignContent::Start => {
                    let top = if wrap == FlexWrap::WrapReverse { free } else { 0.0 };
                    (top, 28.0)
                }
                FlexAlignContent::End => {
                    let top = if wrap == FlexWrap::WrapReverse { 0.0 } else { free };
                    (top, 28.0)
                }
                FlexAlignContent::Center => (free / 2.0, 28.0),
                // Stretch shares the leftover of 240 - 60 - 16 over the three lines, so each line
                // grows to 74.67 and the pitch is that plus the gap.
                FlexAlignContent::Stretch => (0.0, (240.0 - 2.0 * 8.0) / 3.0 + 8.0),
                // Space-between spans the whole cross extent, so its block starts at 0 in both wrap
                // modes; wrap-reverse only flips which LINE is at the bottom.
                FlexAlignContent::SpaceBetween => (0.0, 20.0 + 8.0 + free / 2.0),
                FlexAlignContent::SpaceAround => (free / 6.0, 20.0 + 8.0 + free / 3.0),
            };
            assert!(
                (sorted[0] - expected_top).abs() < 0.5,
                "align-content {align_content:?} on {wrap:?}: the block starts at {expected_top}, got {:?} ({rects:?})",
                sorted[0]
            );
            for pair in sorted.windows(2) {
                assert!(
                    (pair[1] - pair[0] - expected_pitch).abs() < 0.5,
                    "align-content {align_content:?} on {wrap:?}: line pitch {expected_pitch}, got {:?} ({rects:?})",
                    pair[1] - pair[0]
                );
            }
            assert!(
                sorted[2] + 20.0 <= 240.0 + 0.01,
                "align-content {align_content:?} on {wrap:?}: the block leaves the container ({rects:?})"
            );
            // The line ORDER follows the wrap mode: Wrap stacks downward, WrapReverse upward.
            if wrap == FlexWrap::Wrap {
                assert!(
                    ys.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
                    "Wrap stacks the lines downward for {case}: {rects:?}"
                );
            } else {
                assert!(
                    ys.windows(2).all(|pair| pair[1] <= pair[0] + 0.01),
                    "WrapReverse stacks the lines upward for {case}: {rects:?}"
                );
            }
        }
    }
}

/// RTL, the line cap, order and nesting — the parameters the sweeps above do not vary.
#[test]
fn the_matrix_covers_rtl_the_line_cap_order_and_nesting() {
    // RTL mirrors the main axis of a row, and only x.
    for &justify in &[FlexJustifyContent::Start, FlexJustifyContent::End] {
        let constraints = Constraints::new(300.0, 300.0, 100.0, 100.0);
        let forward = Case {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            justify,
            align_items: FlexAlignItems::Start,
            align_content: FlexAlignContent::Start,
            count: 2,
            pattern: 0,
            constraints,
        };
        let (size, ltr) = run(&forward);
        let rtl = run_rtl(&forward);
        let mut expected: Vec<f32> = ltr.iter().map(|rect| size.width - rect.0 - rect.2).collect();
        let mut actual: Vec<f32> = rtl.iter().map(|rect| rect.0).collect();
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        actual.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(expected, actual, "RTL mirrors the row for {forward}");
        // The cross axis must not move.
        assert!(
            ltr.iter().zip(rtl.iter()).all(|(a, b)| (a.1 - b.1).abs() < 0.01 && (a.3 - b.3).abs() < 0.01),
            "RTL must leave the cross axis alone for {forward}: {ltr:?} {rtl:?}"
        );
    }

    // max_items_in_each_line caps each line even when the items would fit.
    let mut nodes: Vec<LayoutNode> = (0..5).map(|index| item_node(index, 0)).collect();
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlexBoxLayout::new(
        FlexBoxConfig::new().wrap(FlexWrap::Wrap).max_items_in_each_line(2),
    );
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 600.0, 0.0, 400.0),
    );
    let mut per_line: Vec<f32> = placements.iter().map(|placement| placement.position.y).collect();
    per_line.dedup();
    assert_eq!(per_line.len(), 3, "five items with a cap of two make three lines: {per_line:?}");

    // order re-sorts the items before placement.
    let mut ordered: Vec<LayoutNode> = vec![
        LayoutNode::leaf(Modifier::new().size(20.0, 20.0).flex(FlexItemConfig::new().order(2))),
        LayoutNode::leaf(Modifier::new().size(20.0, 20.0).flex(FlexItemConfig::new().order(0))),
        LayoutNode::leaf(Modifier::new().size(20.0, 20.0).flex(FlexItemConfig::new().order(1))),
    ];
    let children: Vec<usize> = (0..ordered.len()).collect();
    let policy = FlexBoxLayout::new(FlexBoxConfig::new());
    let (_, placements) = policy.measure(
        &mut ordered,
        &[],
        &children,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    let xs: Vec<f32> = placements.iter().map(|placement| placement.position.x).collect();
    assert!(
        xs[1] < xs[2] && xs[2] < xs[0],
        "the items are placed in order 0, 1, 2: {xs:?}"
    );

    // A FlexBox inside a FlexBox: the outer one lays out the inner one as a single item.
    let inner = LayoutNode::leaf(Modifier::new().size(80.0, 30.0));
    let mut outer_nodes = vec![inner, LayoutNode::leaf(Modifier::new().size(40.0, 20.0))];
    let outer_children: Vec<usize> = (0..outer_nodes.len()).collect();
    let policy = FlexBoxLayout::new(FlexBoxConfig::new().gap(10.0));
    let (size, placements) = policy.measure(
        &mut outer_nodes,
        &[],
        &outer_children,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size, Size::new(130.0, 30.0), "the outer line is the two items plus the gap");
    assert_eq!(placements[1].position.x, 90.0);
}

fn run_rtl(case: &Case) -> Vec<(f32, f32, f32, f32)> {
    let mut nodes: Vec<LayoutNode> = (0..case.count)
        .map(|index| item_node(index, case.pattern))
        .collect();
    let children: Vec<usize> = (0..nodes.len()).collect();
    let config = FlexBoxConfig::new()
        .direction(case.direction)
        .wrap(case.wrap)
        .justify_content(case.justify)
        .align_items(case.align_items)
        .align_content(case.align_content)
        .gap(8.0);
    let policy = FlexBoxLayout::new(config).direction(winia::layout::LayoutDirection::Rtl);
    let (_, placements) = policy.measure(&mut nodes, &[], &children, case.constraints);
    placements
        .iter()
        .map(|placement| {
            (
                placement.position.x,
                placement.position.y,
                placement.size.width,
                placement.size.height,
            )
        })
        .collect()
}

/// Uniform items, so the arithmetic is exact: equal grow factors split a line evenly, and a NoWrap
/// line with room to grow fills the container exactly.
#[test]
fn the_matrix_splits_a_line_evenly_between_equal_items() {
    for &count in &[1usize, 2, 3, 4, 7] {
        for &justify in &[FlexJustifyContent::Start, FlexJustifyContent::Center] {
            for &(width, min_width) in &[(0.0f32, 300.0f32), (300.0, 300.0), (0.0, 500.0)] {
                let mut nodes: Vec<LayoutNode> = (0..count)
                    // No fixed WIDTH: a fixed main size sets the min-content floor, and an item
                    // cannot grow below it (the earlier case measured 40, not 36, for exactly that
                    // reason).
                    .map(|_| LayoutNode::leaf(Modifier::new().height(20.0).flex(FlexItemConfig::new().basis_dp(20.0).grow(1.0))))
                    .collect();
                let children: Vec<usize> = (0..nodes.len()).collect();
                let policy = FlexBoxLayout::new(
                    FlexBoxConfig::new().justify_content(justify).gap(8.0),
                );
                let constraints = Constraints::new(min_width, width.max(min_width), 0.0, 200.0);
                let (size, placements) = policy.measure(&mut nodes, &[], &children, constraints);
                let widths: Vec<f32> = placements.iter().map(|placement| placement.size.width).collect();
                let gaps = 8.0 * (count as f32 - 1.0);
                let container_main = size.width.max(min_width);
                let expected = (container_main - gaps) / count as f32;
                for (index, width) in widths.iter().enumerate() {
                    assert!(
                        (width - expected).abs() < 0.5,
                        "equal grow factors split the line: count {count} item {index} is {width}, expected {expected} ({widths:?}, container {container_main})"
                    );
                }
                let total: f32 = widths.iter().sum();
                assert!(
                    (total + gaps - container_main).abs() < 0.5,
                    "the grown line fills the container: count {count} total {total} + gaps {gaps} vs {container_main} ({widths:?})"
                );
            }
        }
    }
}

/// With wrap and align-items: stretch every item of a line takes THAT line's cross extent, so items
/// sharing a cross start share a height, and two lines of different heights stay different.
#[test]
fn the_matrix_stretches_each_line_to_its_own_extent() {
    let mut nodes: Vec<LayoutNode> = vec![
        // Line one: two items 30 and 50 tall.
        LayoutNode::leaf(Modifier::new().size(90.0, 30.0)),
        LayoutNode::leaf(Modifier::new().size(90.0, 50.0)),
        // Line two: two items 20 tall. 90 dp each, so only two fit a 200 dp line.
        LayoutNode::leaf(Modifier::new().size(90.0, 20.0)),
        LayoutNode::leaf(Modifier::new().size(90.0, 20.0)),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlexBoxLayout::new(
        FlexBoxConfig::new()
            .wrap(FlexWrap::Wrap)
            .align_items(FlexAlignItems::Stretch)
            .gap(4.0),
    );
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 200.0, 0.0, 300.0),
    );
    let heights: Vec<f32> = placements.iter().map(|placement| placement.size.height).collect();
    assert!(
        (heights[0] - 50.0).abs() < 0.5 && (heights[1] - 50.0).abs() < 0.5,
        "the first line stretches to its tallest item: {heights:?}"
    );
    assert!(
        (heights[2] - 20.0).abs() < 0.5 && (heights[3] - 20.0).abs() < 0.5,
        "the second line stretches to its own tallest item: {heights:?}"
    );
    assert!(
        (size.height - (50.0 + 4.0 + 20.0)).abs() < 0.5,
        "the container is the two lines plus the gap: {size:?}"
    );
    let ys: Vec<f32> = placements.iter().map(|placement| placement.position.y).collect();
    assert!(
        (ys[2] - 54.0).abs() < 0.5 && (ys[3] - 54.0).abs() < 0.5,
        "the second line starts below the first: {ys:?}"
    );
}

/// align-content on a COLUMN: the lines stack across the width, so the cross arithmetic moves to x.
#[test]
fn the_matrix_distributes_the_wrapped_lines_on_the_column_axis() {
    for &align_content in &[
        FlexAlignContent::Start,
        FlexAlignContent::End,
        FlexAlignContent::Center,
        FlexAlignContent::SpaceBetween,
    ] {
        for &wrap in &[FlexWrap::Wrap, FlexWrap::WrapReverse] {
            let mut nodes: Vec<LayoutNode> = (0..3)
                .map(|index| {
                    let (_, config) = item(index, 0);
                    let mut node =
                        LayoutNode::leaf(Modifier::new().size(40.0, 20.0).flex(config));
                    node.measured_size = Size::new(40.0, 20.0);
                    node
                })
                .collect();
            let children: Vec<usize> = (0..nodes.len()).collect();
            let policy = FlexBoxLayout::new(
                FlexBoxConfig::new()
                    .direction(FlexDirection::Column)
                    .wrap(wrap)
                    .max_items_in_each_line(1)
                    .align_content(align_content)
                    .gap(8.0),
            );
            let (size, placements) = policy.measure(
                &mut nodes,
                &[],
                &children,
                Constraints::new(240.0, 240.0, 0.0, 200.0),
            );
            let xs: Vec<f32> = placements.iter().map(|placement| placement.position.x).collect();
            let free = 240.0 - (3.0 * 40.0 + 2.0 * 8.0);
            let mut sorted = xs.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let expected_top = match align_content {
                FlexAlignContent::Start => {
                    if wrap == FlexWrap::WrapReverse {
                        free
                    } else {
                        0.0
                    }
                }
                FlexAlignContent::End => {
                    if wrap == FlexWrap::WrapReverse {
                        0.0
                    } else {
                        free
                    }
                }
                FlexAlignContent::Center => free / 2.0,
                _ => 0.0,
            };
            assert!(
                (sorted[0] - expected_top).abs() < 0.5,
                "column align-content {align_content:?} on {wrap:?}: block starts at {expected_top}, got {sorted:?} ({xs:?})"
            );
            assert!(
                sorted[2] + 40.0 <= 240.0 + 0.01,
                "column align-content {align_content:?} on {wrap:?}: the block leaves the container ({xs:?})"
            );
            assert_eq!(size.width, 240.0, "the column keeps its bounded width");
            // WrapReverse flips which line is at the cross-start.
            if wrap == FlexWrap::Wrap {
                assert!(
                    xs.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
                    "a column Wrap stacks the lines rightward: {xs:?}"
                );
            } else {
                assert!(
                    xs.windows(2).all(|pair| pair[1] <= pair[0] + 0.01),
                    "a column WrapReverse stacks the lines leftward: {xs:?}"
                );
            }
        }
    }
}
