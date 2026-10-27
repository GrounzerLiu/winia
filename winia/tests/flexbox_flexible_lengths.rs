//! The two flexible-length freeze loops, pinned with exact numbers: CSS 9.7 4.d/4.e and the growth
//! clamp that Compose omits.

use winia::layout::{
    Constraints, FlexBoxConfig, FlexBoxLayout, FlexItemConfig, LayoutNode, MeasurePolicy,
};
use winia::modifier::Modifier;
use winia::unit::Size;

fn measure_two(config: FlexBoxConfig, a: (f32, f32), b: (f32, f32), base: f32) -> (Size, Vec<Size>) {
    let mut nodes = vec![
        LayoutNode::leaf(
            Modifier::new()
                .size(a.0, a.1)
                .flex(FlexItemConfig::new().basis_dp(base)),
        ),
        LayoutNode::leaf(
            Modifier::new()
                .size(b.0, b.1)
                .flex(FlexItemConfig::new().basis_dp(base)),
        ),
    ];
    let policy = FlexBoxLayout::new(config);
    let children = vec![0usize, 1];
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 150.0, 0.0, 200.0),
    );
    (
        size,
        placements
            .iter()
            .map(|placement| placement.size)
            .collect(),
    )
}

/// Shrinking: the item whose share would fall below its min-content size freezes there and the rest
/// is taken from the other one (Compose resolveFlexibleLengths shrink loop, FlexBox.kt:612-676).
#[test]
fn a_shrinking_item_freezes_at_its_min_content_and_the_other_takes_the_rest() {
    let (size, sizes) = measure_two(FlexBoxConfig::new(), (90.0, 20.0), (40.0, 20.0), 100.0);
    assert_eq!(size.width, 150.0);
    assert_eq!(
        sizes[0].width, 90.0,
        "the first item cannot go below its 90 dp content"
    );
    assert_eq!(sizes[1].width, 60.0, "the second absorbs the rest");
}

/// Growing past a min-content floor: winia freezes the item at its floor and hands the remaining
/// space to the others (CSS 9.7 4.d/4.e).
///
/// Compose's grow pass has no such clamp: it would give BOTH items 150 and squash the first below
/// its own content, so this pins the deliberate deviation recorded in docs/flexbox.md.
#[test]
fn growing_past_a_min_content_floor_redistributes_instead_of_squashing() {
    let mut nodes = vec![
        LayoutNode::leaf(
            Modifier::new()
                .size(200.0, 20.0)
                .flex(FlexItemConfig::new().basis_dp(50.0).grow(1.0)),
        ),
        LayoutNode::leaf(
            Modifier::new()
                .size(40.0, 20.0)
                .flex(FlexItemConfig::new().basis_dp(50.0).grow(1.0)),
        ),
    ];
    let policy = FlexBoxLayout::new(FlexBoxConfig::new());
    let children = vec![0usize, 1];
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 300.0, 0.0, 200.0),
    );
    assert_eq!(size.width, 300.0);
    assert_eq!(placements[0].size.width, 200.0, "the floored item keeps 200");
    assert_eq!(placements[1].size.width, 100.0, "the other item takes what is left");
}
