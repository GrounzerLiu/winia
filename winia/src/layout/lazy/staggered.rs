//! `LazyStaggeredGrid` — the waterfall grid (Compose's `LazyStaggeredGrid`).
//!
//! A staggered grid has no lines: every item takes its own main-axis size and goes into the lane whose
//! cursor is lowest, so the lanes drift apart. `layout::staggered` owns that rule, this owns the
//! container.
//!
//! # How it windows
//!
//! The grid is a lazy container in the lists' sense: it attaches `A::scroll` and `lazy_scroll`, so the
//! engine takes the content extent from the modifier instead of from the composed children's bottoms,
//! and the policy owns the child constraints and the scroll clamp. The window itself comes from a
//! shared height cache: the build prices every item (measured last frame, or an estimate), assigns the
//! lanes from those prices, and composes only the items whose extent intersects
//! `[offset - beyond, offset + viewport + beyond]`. The policy then measures that window for real,
//! writes the sizes back and reports the extent over the WHOLE data set, so the scroll range is right
//! even though only a slice is composed.
//!
//! # v1 scope
//!
//! - `GridCells::Fixed` only (Adaptive needs the cross viewport before the first measure).
//! - No item placement animation and no sticky headers, as Compose's staggered grids.
//! - A data change does not re-anchor by KEY: the scroll position is a plain pixel offset, so an
//!   insertion above the viewport moves the content instead of keeping the reading position (the lazy
//!   lists' anchor correction needs a per-item "first visible key", which a per-LANE waterfall does not
//!   have — Compose resolves it with `LazyStaggeredGridLaneInfo`, and `layout::staggered::lane_anchors`
//!   is the piece that would feed it).

use super::{GridCells, ItemHeightCache, LazyAxis};
use crate::layout::axis::Axis as _;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement};
use crate::layout::staggered::assign_lanes;
use crate::layout::container::build_container;
use crate::layout::Constraints;
use crate::modifier::{Modifier, ScrollState};
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::Backchannel;
use crate::unit::{Offset, Size};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

/// The main size an unmeasured item is priced at.
const ESTIMATE: f32 = 120.0;

/// What the build and the policy share between frames.
#[derive(Debug)]
struct Shared {
    heights: ItemHeightCache,
    /// The main viewport from the last measure (0 before the first one).
    viewport: f32,
}

struct StaggeredPolicy<A: LazyAxis> {
    lanes: usize,
    spacing: f32,
    cross_spacing: f32,
    cross_before: f32,
    cross_after: f32,
    /// The data index of each composed child, in composition order.
    window: Vec<usize>,
    /// The lane each composed child goes into.
    lanes_of: Vec<usize>,
    /// Every lane's cursor at the first composed item of that lane.
    start_cursors: Vec<f32>,
    /// The main-axis content padding used by this frame (`content_padding`).
    main_before: f32,
    /// The content extent over the whole data set.
    extent: f32,
    shared: Arc<Mutex<Shared>>,
    content_height: Backchannel<f32>,
    /// The scroll position: the measure clamps its offset and writes its fling limit.
    state: ScrollState,
    marker: PhantomData<A>,
}

impl<A: LazyAxis> std::fmt::Debug for StaggeredPolicy<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaggeredPolicy")
            .field("lanes", &self.lanes)
            .field("window", &self.window.len())
            .finish()
    }
}

impl<A: LazyAxis> MeasurePolicy for StaggeredPolicy<A> {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let cross_max = A::cross_max(&constraints);
        let main_max = A::main_max(&constraints);
        let viewport = if main_max < f32::MAX { main_max } else { 0.0 };
        let lanes = self.lanes.max(1);
        let usable = (cross_max
            - self.cross_before
            - self.cross_after
            - self.cross_spacing * lanes.saturating_sub(1) as f32)
            .max(0.0);
        let cell = usable / lanes as f32;
        let mut cursors = self.start_cursors.clone();
        cursors.resize(lanes, 0.0);
        let mut placements = Vec::with_capacity(children.len());
        let mut measured: Vec<(usize, f32)> = Vec::with_capacity(children.len());
        for (slot, &child) in children.iter().enumerate() {
            let cc = if A::VERTICAL {
                Constraints::new(0.0, cell, 0.0, f32::MAX)
            } else {
                Constraints::new(0.0, f32::MAX, 0.0, cell)
            };
            let (size, _) = crate::layout::measure_node(nodes, policies, child, cc);
            let lane = self.lanes_of.get(slot).copied().unwrap_or(0).min(lanes - 1);
            let main = self.main_before + cursors[lane];
            let main_size = if A::VERTICAL { size.height } else { size.width };
            cursors[lane] = main + main_size + self.spacing;
            let cross = self.cross_before + lane as f32 * (cell + self.cross_spacing);
            let position = if A::VERTICAL {
                Offset::new(cross, main)
            } else {
                Offset::new(main, cross)
            };
            placements.push(Placement { size, position });
            if let Some(&index) = self.window.get(slot) {
                measured.push((index, main_size));
            }
        }
        {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.viewport = viewport;
            for (index, main_size) in measured {
                shared.heights.record_keyed(index as u64, main_size);
            }
        }
        // The scroll range covers the whole data set, not the composed window — and the offset is
        // CLAMPED into it, the way the lazy list's policy does: without the clamp a programmatic
        // `offset.set(10000)` stayed 10000 (measured), leaving the grid composing nothing forever.
        let max_off = (self.extent - viewport).max(0.0);
        self.content_height.set(self.extent);
        // The INPUT path (drag, wheel, fling) reads `ScrollState::fling_limit`, not a channel of our
        // own: writing a private Backchannel left the real limit untouched.
        self.state.fling_limit.set(max_off);
        if viewport > 0.0 {
            let current = self.state.offset.peek();
            if current > max_off {
                self.state.offset.set(max_off);
            }
        }
        let size = if A::VERTICAL {
            Size::new(cross_max, viewport.max(constraints.min_height))
        } else {
            Size::new(viewport.max(constraints.min_width), cross_max)
        };
        (size, placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &child) in children.iter().enumerate() {
            if let Some(placement) = placements.get(i) {
                nodes[child].position = placement.position;
                nodes[child].measured_size = placement.size;
            }
        }
    }
}

/// A staggered (waterfall) grid — Compose's `LazyStaggeredGrid`. The axis is the type parameter.
pub struct LazyStaggeredGrid<A: LazyAxis> {
    cells: GridCells,
    spacing: f32,
    cross_spacing: f32,
    content_padding: (f32, f32),
    content_padding_cross: (f32, f32),
    beyond_bounds: f32,
    state: Option<ScrollState>,
    modifier: Modifier,
    count: usize,
    key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>,
    content: Option<Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>>,
    marker: PhantomData<A>,
}

/// The vertical staggered grid.
pub type LazyVerticalStaggeredGrid = LazyStaggeredGrid<crate::layout::axis::VerticalAxis>;
/// The horizontal staggered grid. RTL mirrors it through the scroll modifier, as the other lazy
/// containers do.
pub type LazyHorizontalStaggeredGrid = LazyStaggeredGrid<crate::layout::axis::HorizontalAxis>;

impl<A: LazyAxis> Default for LazyStaggeredGrid<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: LazyAxis> LazyStaggeredGrid<A> {
    pub fn new() -> Self {
        LazyStaggeredGrid {
            cells: GridCells::Fixed(2),
            spacing: 0.0,
            cross_spacing: 0.0,
            content_padding: (0.0, 0.0),
            content_padding_cross: (0.0, 0.0),
            beyond_bounds: 600.0,
            state: None,
            modifier: Modifier::new(),
            count: 0,
            key: None,
            content: None,
            marker: PhantomData,
        }
    }

    /// How many lanes the waterfall has (`GridCells::Fixed`).
    pub fn lanes(mut self, cells: GridCells) -> Self {
        self.cells = cells;
        self
    }

    /// The gap between two items of the same lane.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    /// The gap between lanes.
    pub fn cross_spacing(mut self, spacing: f32) -> Self {
        self.cross_spacing = spacing.max(0.0);
        self
    }

    pub fn content_padding(mut self, before: f32, after: f32) -> Self {
        self.content_padding = (before.max(0.0), after.max(0.0));
        self
    }

    pub fn content_padding_cross(mut self, before: f32, after: f32) -> Self {
        self.content_padding_cross = (before.max(0.0), after.max(0.0));
        self
    }

    /// How far past the viewport items stay composed, in logical pixels.
    pub fn beyond_bounds(mut self, pixels: f32) -> Self {
        self.beyond_bounds = pixels.max(0.0);
        self
    }

    pub fn state(mut self, state: ScrollState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    pub fn items(
        self,
        count: usize,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        self.items_with(count, |index| index as u64, content)
    }

    pub fn items_with(
        mut self,
        count: usize,
        key: impl Fn(usize) -> u64 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        self.count = count;
        self.key = Some(Arc::new(key));
        self.content = Some(Arc::new(content));
        self
    }

    pub fn items_from<T: Send + Sync + 'static>(
        self,
        list: impl Into<Arc<Vec<T>>>,
        key: impl Fn(&T) -> u64 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize, &T) + Send + Sync + 'static,
    ) -> Self {
        let list: Arc<Vec<T>> = list.into();
        let count = list.len();
        let (k, c) = (Arc::new(key), Arc::new(content));
        let (lk, lc) = (list.clone(), list.clone());
        self.items_with(
            count,
            move |index| k(&lk[index]),
            move |ctx, index| c(ctx, index, &lc[index]),
        )
    }

    /// Assigns the lanes from the height cache, composes the window the scroll offset selects, and
    /// hands both to the policy.
    pub fn build(self, ctx: &mut ComposeCtx) {
        let (Some(content), Some(key)) = (self.content, self.key) else {
            return;
        };
        let lanes = match self.cells {
            GridCells::Fixed(cells) => cells.max(1),
            GridCells::Adaptive(_) => 1,
        };
        // The container OWNS this state, so it is a plain remember (docs/state-handles.md 7.4); the
        // scroll modifier and the policy of every frame share the same handles.
        let shared = ctx
            .remember(|| {
                Arc::new(Mutex::new(Shared {
                    heights: ItemHeightCache::new(),
                    viewport: 0.0,
                }))
            })
            .peek();
        let content_height = ctx.remember_backchannel(|| 0.0f32);
        let state = self.state.clone().unwrap_or_else(ScrollState::new);

        // Price every item, then assign the lanes from the prices. An unmeasured item is priced at the
        // AVERAGE of the measured ones rather than a fixed constant: the extent (and so the scroll
        // range) then converges on the first frame's measurements instead of over-shooting per item
        // (measured: 200 forty-dp items reported a 11552 dp extent against a real 4792 with a flat 120).
        let heights: Vec<f32> = {
            let shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            let mut measured_sum = 0.0f32;
            let mut measured_count = 0usize;
            let mut priced: Vec<Option<f32>> = Vec::with_capacity(self.count);
            for index in 0..self.count {
                let height = shared
                    .heights
                    .height_keyed(key(index))
                    .filter(|height| *height > 0.0);
                if let Some(height) = height {
                    measured_sum += height;
                    measured_count += 1;
                }
                priced.push(height);
            }
            let estimate = if measured_count > 0 {
                measured_sum / measured_count as f32
            } else {
                ESTIMATE
            };
            priced
                .into_iter()
                .map(|height| height.unwrap_or(estimate))
                .collect()
        };
        let (slots, deepest) = assign_lanes(lanes, &heights, self.spacing);
        let extent = self.content_padding.0 + deepest + self.content_padding.1;
        let viewport = {
            let shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.viewport
        };
        let offset = state.offset.get();
        let content_offset = (offset - self.content_padding.0).max(0.0);
        let window_start = (content_offset - self.beyond_bounds).max(0.0);
        let window_end = content_offset + viewport + self.beyond_bounds;
        let mut window: Vec<usize> = Vec::new();
        let mut lanes_of: Vec<usize> = Vec::new();
        let mut start_cursors = vec![f32::MAX; lanes];
        for (index, slot) in slots.iter().enumerate() {
            let top = slot.main_start;
            let bottom = top + heights[index];
            if bottom < window_start || top > window_end {
                continue;
            }
            window.push(index);
            lanes_of.push(slot.lane);
            if start_cursors[slot.lane] == f32::MAX {
                start_cursors[slot.lane] = top;
            }
        }
        let start_cursors: Vec<f32> = start_cursors
            .into_iter()
            .map(|cursor| if cursor == f32::MAX { 0.0 } else { cursor })
            .collect();

        // The container's parameters and the DATA SIGNATURE (the window's keys), declared before the
        // group starts: without them a data change at a stable offset Skips the group and the previous
        // frame's children stay on screen (measured: shrinking four items to one kept all four).
        ctx.changed(&self.cells);
        ctx.changed(&self.spacing);
        ctx.changed(&self.cross_spacing);
        ctx.changed(&self.content_padding);
        ctx.changed(&self.content_padding_cross);
        let mut signature = 0u64;
        for index in &window {
            signature = signature.wrapping_mul(31).wrapping_add(key(*index));
        }
        ctx.changed(&signature);

        let policy = StaggeredPolicy::<A> {
            lanes,
            spacing: self.spacing,
            cross_spacing: self.cross_spacing,
            cross_before: self.content_padding_cross.0,
            cross_after: self.content_padding_cross.1,
            main_before: self.content_padding.0,
            window: window.clone(),
            lanes_of,
            start_cursors,
            extent,
            shared,
            content_height: content_height.clone(),
            state: state.clone(),
            marker: PhantomData,
        };
        let modifier = Modifier::new()
            .lazy_scroll(content_height)
            .then(A::scroll(state.clone()))
            .then(self.modifier);
        build_container(ctx, modifier, policy, |ctx| {
            for index in window {
                content(ctx, index);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Stack, Constraints};
    use crate::runtime::composer::Composer;

    fn grid_frame(composer: &mut Composer, state: &ScrollState, count: usize, height: f32) {
        let state = state.clone();
        composer.compose(crate::compose!(|ctx| {
            LazyVerticalStaggeredGrid::new()
                .lanes(GridCells::Fixed(2))
                .spacing(4.0)
                .cross_spacing(4.0)
                .state(state)
                .modifier(Modifier::new().fill_max_width().height(200.0))
                .items(count, move |ctx, index| {
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .size(10.0, height)
                                .test_tag(format!("lane-item-{index}")),
                        )
                        .build(ctx, |_| {});
                })
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
    }

    /// Each item goes into the shortest lane, the lanes are one cell wide plus the cross gap, and an
    /// item's main offset is its lane's cursor.
    #[test]
    fn a_staggered_grid_places_items_into_the_shortest_lane() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        let mut composer = Composer::new();
        grid_frame(&mut composer, &state, 4, 40.0);
        grid_frame(&mut composer, &state, 4, 40.0);

        let root = composer.layout_root().unwrap();
        let items: Vec<usize> = root.children.clone();
        assert_eq!(items.len(), 4, "all four items fit the 200 dp viewport");
        // The items keep their own 10 dp width (a cell places them, it does not stretch them), so the
        // CELL is read off the second lane's x offset: (300 - 4) / 2 + 4.
        assert_eq!(composer.arena_nodes()[items[0]].position, Offset::new(0.0, 0.0));
        assert_eq!(
            composer.arena_nodes()[items[1]].position,
            Offset::new(152.0, 0.0),
            "the second item starts lane 1"
        );
        assert_eq!(
            composer.arena_nodes()[items[2]].position,
            Offset::new(0.0, 44.0),
            "the third follows the first in lane 0: 40 + 4"
        );
        assert_eq!(
            composer.arena_nodes()[items[3]].position,
            Offset::new(152.0, 44.0),
            "and the fourth follows the second"
        );
    }




    /// The main-axis content padding moves the first item down and counts towards the extent (it used
    /// to be stored and never applied).
    #[test]
    fn the_main_content_padding_offsets_the_items() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        let mut composer = Composer::new();
        for _ in 0..2 {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyVerticalStaggeredGrid::new()
                    .lanes(GridCells::Fixed(2))
                    .content_padding(30.0, 10.0)
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(200.0))
                    .items(2, |ctx, index| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .size(10.0, 40.0)
                                    .test_tag(format!("pad-item-{index}")),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        }
        let root = composer.layout_root().unwrap();
        let items: Vec<usize> = root.children.clone();
        assert_eq!(items.len(), 2);
        assert_eq!(
            composer.arena_nodes()[items[0]].position.y, 30.0,
            "the content starts 30 dp in"
        );
    }

    /// The offset is clamped into the range the extent allows. Without the clamp a programmatic
    /// `offset.set(10000)` stayed 10000 and the grid composed nothing, forever (measured).
    #[test]
    fn the_offset_is_clamped_into_the_scroll_range() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        // Content that FITS: nothing may scroll.
        let state = ScrollState::new();
        let mut composer = Composer::new();
        grid_frame(&mut composer, &state, 4, 40.0);
        grid_frame(&mut composer, &state, 4, 40.0);
        state.offset.set(10000.0);
        grid_frame(&mut composer, &state, 4, 40.0);
        assert_eq!(state.offset.get(), 0.0, "a grid whose content fits cannot scroll");

        // A tall grid: the clamp is extent - viewport. 200 items of 40 dp in two lanes with the
        // helper's 4 dp spacing are (100 * 44) - 4 = 4396, so 4396 - 200 = 4196.
        let state = ScrollState::new();
        let mut composer = Composer::new();
        for _ in 0..3 {
            grid_frame(&mut composer, &state, 200, 40.0);
        }
        state.offset.set(999_999.0);
        grid_frame(&mut composer, &state, 200, 40.0);
        grid_frame(&mut composer, &state, 200, 40.0);
        let clamped = state.offset.get();
        assert!(
            (clamped - 4196.0).abs() <= 2.0,
            "the offset clamps to the deepest lane minus the viewport: {clamped}"
        );
        assert!(
            (state.fling_limit.peek() - 4196.0).abs() <= 2.0,
            "and the INPUT path's fling limit is that same range: {}",
            state.fling_limit.peek()
        );
    }

    /// A DATA change has to re-enter the container: the group compares parameters and a data signature,
    /// so dropping items at a stable offset does not leave last frame's children behind (measured:
    /// shrinking four items to one kept all four nodes until the signature was declared).
    #[test]
    fn a_data_change_re_enters_the_container() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        let mut composer = Composer::new();
        grid_frame(&mut composer, &state, 4, 40.0);
        grid_frame(&mut composer, &state, 4, 40.0);
        assert_eq!(
            composer.layout_root().unwrap().children.len(),
            4,
            "four items compose"
        );
        grid_frame(&mut composer, &state, 1, 40.0);
        grid_frame(&mut composer, &state, 1, 40.0);
        assert_eq!(
            composer.layout_root().unwrap().children.len(),
            1,
            "and shrinking to one leaves exactly one composed child"
        );
    }

    /// The window follows the scroll offset: only the items near it compose, while the content extent
    /// (and so the scroll range) still covers the whole data set.
    #[test]
    fn the_window_follows_the_scroll_offset() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        let mut composer = Composer::new();
        // 200 items of 40 dp in two lanes: 100 per lane, so the content is ~4400 dp tall.
        grid_frame(&mut composer, &state, 200, 40.0);
        grid_frame(&mut composer, &state, 200, 40.0);
        let root = composer.layout_root().unwrap();
        let first_window = root.children.len();
        assert!(
            first_window < 200,
            "the first frame only composes a window, not all 200 items: {first_window}"
        );

        state.offset.set(2000.0);
        grid_frame(&mut composer, &state, 200, 40.0);
        grid_frame(&mut composer, &state, 200, 40.0);
        let root = composer.layout_root().unwrap();
        // Positions are CONTENT coordinates, so "above the viewport" is "smaller than the offset".
        let first_y = root
            .children
            .iter()
            .map(|child| composer.arena_nodes()[*child].position.y)
            .fold(f32::MAX, f32::min);
        assert!(
            first_y < 2000.0,
            "the composed window starts above the 2000 dp offset: {first_y}"
        );
        assert!(
            first_y > 1200.0,
            "and not far above it (the 600 dp overscan): {first_y}"
        );
    }
}
