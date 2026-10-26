//! Lane placement for a staggered grid — the core of Compose's `LazyStaggeredGrid`.
//!
//! A staggered grid (a waterfall) has no lines: every item takes its own main-axis size and goes into
//! the lane whose cursor is smallest, so the lanes drift apart and the content extent is the deepest
//! lane. That single rule is what this module owns; the lazy window, the anchor and the scroll stay the
//! lazy list's job (which is why the grid's own build/policy wiring is a separate piece — the list's
//! build is 465 lines and its policy 412, and neither can express a lane cursor).
//!
//! Kept separate from the container so the rule can be tested on its own, the way `measure_flex` is.

/// Where one item sits: its lane and its main-axis start inside the content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaneSlot {
    pub lane: usize,
    pub main_start: f32,
}

/// Assigns `sizes` to `lane_count` lanes, shortest lane first — Compose's
/// `LazyStaggeredGridMeasure` lane loop.
///
/// `spacing` is the gap BETWEEN two items of the same lane (the main-axis arrangement); the lanes
/// themselves are separated on the cross axis by the caller. Returns the per-item slots and the
/// content's main extent (the deepest lane's cursor minus the trailing spacing).
pub fn assign_lanes(lane_count: usize, sizes: &[f32], spacing: f32) -> (Vec<LaneSlot>, f32) {
    let lanes = lane_count.max(1);
    let spacing = spacing.max(0.0);
    let mut cursors = vec![0.0f32; lanes];
    let mut slots = Vec::with_capacity(sizes.len());
    for &size in sizes {
        // The SHORTEST lane wins; ties go to the leftmost, so the assignment is deterministic.
        let mut best = 0usize;
        for lane in 1..lanes {
            if cursors[lane] < cursors[best] {
                best = lane;
            }
        }
        let start = cursors[best];
        slots.push(LaneSlot {
            lane: best,
            main_start: start,
        });
        cursors[best] = start + size.max(0.0) + spacing;
    }
    let deepest = cursors.iter().copied().fold(0.0f32, f32::max);
    let extent = if sizes.is_empty() {
        0.0
    } else {
        (deepest - spacing).max(0.0)
    };
    (slots, extent)
}

/// Compose keeps one scroll position per lane because the lanes drift: this returns, per lane, the item
/// COVERING the offset — the last one that starts at or above it (`None` when the lane does not reach).
pub fn lane_anchors(slots: &[LaneSlot], lane_count: usize, offset: f32) -> Vec<Option<usize>> {
    // A slot whose lane is past `lane_count` widens the result instead of panicking: the caller's lane
    // count and the slots' lanes can disagree while a grid is being reconfigured.
    let lanes = slots
        .iter()
        .map(|slot| slot.lane + 1)
        .chain(std::iter::once(lane_count.max(1)))
        .max()
        .unwrap_or(1);
    let mut anchors = vec![None; lanes];
    for (index, slot) in slots.iter().enumerate() {
        // The lanes drift, so each keeps its own reading position: the item COVERING the offset, which
        // is the last one that starts at or above it (a lane with no item that far is `None`).
        if slot.main_start <= offset {
            anchors[slot.lane] = Some(index);
        }
    }
    anchors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_item_takes_the_shortest_lane() {
        let (slots, extent) = assign_lanes(2, &[10.0, 30.0, 20.0, 5.0], 0.0);
        let shape: Vec<(usize, f32)> = slots.iter().map(|s| (s.lane, s.main_start)).collect();
        assert_eq!(
            shape,
            vec![(0, 0.0), (1, 0.0), (0, 10.0), (0, 30.0)],
            "item 2 goes under item 0 (lane 0 is shorter), and item 3 breaks the 30/30 tie to the left"
        );
        assert_eq!(extent, 35.0, "lane 0 ends at 35, lane 1 at 30");
    }

    #[test]
    fn the_spacing_counts_between_items_of_one_lane_only() {
        let (slots, extent) = assign_lanes(2, &[10.0, 10.0, 10.0], 4.0);
        assert_eq!(slots[2].main_start, 14.0, "item 2 follows item 0 with one gap");
        assert_eq!(extent, 24.0, "lane 0 holds two items: 10 + 4 + 10, and the trailing gap does not count");
    }

    #[test]
    fn ties_go_to_the_leftmost_lane() {
        let (slots, _) = assign_lanes(3, &[5.0, 5.0, 5.0], 0.0);
        assert_eq!(slots.iter().map(|s| s.lane).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn one_lane_is_a_plain_column() {
        let (slots, extent) = assign_lanes(1, &[10.0, 20.0], 2.0);
        assert_eq!(slots[1].main_start, 12.0);
        assert_eq!(extent, 32.0);
    }

    #[test]
    fn an_empty_grid_is_zero() {
        let (slots, extent) = assign_lanes(2, &[], 8.0);
        assert!(slots.is_empty());
        assert_eq!(extent, 0.0);
    }

    #[test]
    fn lane_anchors_are_the_first_item_at_or_above_the_offset() {
        let (slots, _) = assign_lanes(2, &[10.0, 10.0, 10.0, 10.0], 0.0);
        // lane 0 holds items 0 and 2 (at 0 and 10), lane 1 items 1 and 3.
        let anchors = lane_anchors(&slots, 2, 5.0);
        assert_eq!(anchors, vec![Some(0), Some(1)], "both lanes still show their first item");
        let anchors = lane_anchors(&slots, 2, 10.0);
        assert_eq!(anchors, vec![Some(2), Some(3)], "both lanes have scrolled to their second item");
    }
}
