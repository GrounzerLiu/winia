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
use crate::layout::{Constraints, LayoutDirection};
use crate::modifier::{Modifier, ScrollState};
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::Backchannel;
use crate::unit::{Offset, Size};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

/// The main size an unmeasured item is priced at.
const ESTIMATE: f32 = 120.0;
/// What the window assumes the viewport is before the first measure has written one back.
const FIRST_FRAME_VIEWPORT: f32 = 800.0;

/// What the build and the policy share between frames.
#[derive(Debug)]
struct Shared {
    heights: ItemHeightCache,
}

struct StaggeredPolicy<A: LazyAxis> {
    lanes: usize,
    spacing: f32,
    cross_spacing: f32,
    cross_before: f32,
    cross_after: f32,
    /// The data index of each composed child, in composition order.
    window: Vec<usize>,
    /// The KEY of each composed child: the height cache is keyed by key, not by index.
    keys: Vec<u64>,
    /// The lane each composed child goes into.
    lanes_of: Vec<usize>,
    /// Every lane's cursor at the first composed item of that lane.
    start_cursors: Vec<f32>,
    /// The main-axis content padding used by this frame (`content_padding`).
    main_before: f32,
    /// Whether the layout direction mirrors the CROSS axis (lane 0 on the right for RTL).
    rtl: bool,
    /// The cross extent the lanes are laid out in, for that mirror.
    cross_extent: f32,
    /// The content extent over the whole data set.
    extent: f32,
    shared: Arc<Mutex<Shared>>,
    content_height: Backchannel<f32>,
    /// The viewports, written for the next build (and to wake it: they are States).
    viewport_main: crate::runtime::state::State<f32>,
    viewport_cross: crate::runtime::state::State<f32>,
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
        // An unbounded main axis (a wrap-content parent) falls back to the LAST viewport, as the lazy
        // list does: reporting 0 collapsed the grid and emptied the window.
        // Both axes have to survive a WRAP-CONTENT parent, which hands f32::MAX (that is the framework's
        // unbounded sentinel as well as the scroll one). Main: the last measured viewport, else the same
        // default the build uses. Cross: the last measured cross extent, else a sane lane width — an
        // f32::MAX cross extent used to poison the reported size, the hit test and the clip.
        let viewport = if main_max < f32::MAX {
            main_max
        } else {
            let cached = self.viewport_main.peek();
            if cached > 0.0 { cached } else { FIRST_FRAME_VIEWPORT }
        };
        let cross_extent = if cross_max < f32::MAX {
            cross_max
        } else {
            let cached = self.viewport_cross.peek();
            if cached > 0.0 { cached } else { self.lanes.max(1) as f32 * 200.0 }.max(1.0)
        };
        let lanes = self.lanes.max(1);
        let usable = (cross_extent
            - self.cross_before
            - self.cross_after
            - self.cross_spacing * lanes.saturating_sub(1) as f32)
            .max(0.0);
        let cell = usable / lanes as f32;
        let mut cursors = self.start_cursors.clone();
        cursors.resize(lanes, 0.0);
        let mut placements = Vec::with_capacity(children.len());
        let mut measured: Vec<(u64, f32)> = Vec::with_capacity(children.len());
        for (slot, &child) in children.iter().enumerate() {
            let cc = if A::VERTICAL {
                Constraints::new(0.0, cell, 0.0, f32::MAX)
            } else {
                Constraints::new(0.0, f32::MAX, 0.0, cell)
            };
            let (size, _) = crate::layout::measure_node(nodes, policies, child, cc);
            let lane = self.lanes_of.get(slot).copied().unwrap_or(0).min(lanes - 1);
            // The cursors stay in CONTENT coordinates; the padding is added to the placement only, or
            // every item after the first in a lane would be padded again (30 dp padding, item 2 at 104
            // instead of 74 — found by the review).
            let main = cursors[lane];
            let main_size = if A::VERTICAL { size.height } else { size.width };
            cursors[lane] = main + main_size + self.spacing;
            let lane_offset = lane as f32 * (cell + self.cross_spacing);
            // Only a VERTICAL waterfall mirrors its CROSS axis under RTL (lane 0 on the right); a
            // horizontal one mirrors its main axis, which the scroll modifier does. The item keeps its
            // own cross size, so the RTL anchor is the cell's far edge minus that size.
            let cross_size = if A::VERTICAL { size.width } else { size.height };
            let cross = if self.rtl && A::VERTICAL {
                cross_extent - self.cross_after - lane_offset - cross_size
            } else {
                self.cross_before + lane_offset
            };
            let position = if A::VERTICAL {
                Offset::new(cross, self.main_before + main)
            } else {
                Offset::new(self.main_before + main, cross)
            };
            placements.push(Placement { size, position });
            if let Some(&key) = self.keys.get(slot) {
                measured.push((key, main_size));
            }
        }
        {
            if self.viewport_main.peek() != viewport {
                self.viewport_main.set(viewport);
            }
            let cross = cross_extent;
            if self.viewport_cross.peek() != cross {
                self.viewport_cross.set(cross);
            }
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            for (key, main_size) in measured {
                shared.heights.record_keyed(key, main_size);
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
            let clamped = current.clamp(0.0, max_off);
            if clamped != current {
                self.state.offset.set(clamped);
            }
        }
        // The window came from ESTIMATES: when the measured block stops short of the viewport's far
        // edge, ask for one more compose — the lazy list's convergence, and the only thing that fills
        // the strip an estimate left empty on a static screen.
        let offset = self.state.offset.peek();
        if viewport > 0.0 && self.extent > offset + viewport {
            let target = offset + viewport;
            let covered = cursors.iter().copied().all(|cursor| cursor >= target);
            if !covered {
                crate::runtime::composer::request_compose_after_layout();
            }
        }
        let size = if A::VERTICAL {
            Size::new(cross_extent, viewport.max(constraints.min_height))
        } else {
            Size::new(viewport.max(constraints.min_width), cross_extent)
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
        // The container OWNS this state, so it is a plain remember (docs/state-handles.md 7.4); the
        // scroll modifier and the policy of every frame share the same handles.
        let shared = ctx
            .remember(|| Arc::new(Mutex::new(Shared { heights: ItemHeightCache::new() })))
            .peek();
        // The viewports are STATES, not Mutex fields: the policy writes them during measure and the
        // build READS them, so the write wakes this group. A Mutex field changes nothing the Skip
        // decision compares, and the recomputed window/lane count/extent were silently discarded
        // (that was the root cause of both ignored tests).
        let viewport_main = ctx.remember(|| crate::runtime::state::State::new(0.0f32)).peek();
        let viewport_cross = ctx.remember(|| crate::runtime::state::State::new(0.0f32)).peek();
        let cached_cross = viewport_cross.get();
        let lanes = match self.cells {
            GridCells::Fixed(cells) => cells.max(1),
            // The same two-phase rule as the main viewport: the cross extent is known after the first
            // measure, so the first frame composes one lane and the next corrects it.
            GridCells::Adaptive(min_size) => {
                let min_size = min_size.max(1.0);
                let fit =
                    ((cached_cross + self.cross_spacing) / (min_size + self.cross_spacing)).floor();
                (fit as usize).max(1)
            }
        };
        let content_height = ctx.remember_backchannel(|| 0.0f32);
        // A state the caller did not supply has to be REMEMBERED, or every re-entry builds a fresh one
        // at offset 0 and the grid snaps to the top on any recomposition (docs/state-handles.md 7.4).
        let state = self
            .state
            .clone()
            .unwrap_or_else(|| ctx.remember(ScrollState::new).get());

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
        let viewport = viewport_main.get();
        let offset = state.offset.get();
        let content_offset = (offset - self.content_padding.0).max(0.0);
        // Before the first measure there is no viewport to window against: compose a generous default
        // instead of a zero-height strip, or the bottom of the grid stays empty until a scroll forces
        // another build (measured on the demo: ~450 dp of content for a 600 dp viewport).
        let viewport = if viewport > 0.0 { viewport } else { FIRST_FRAME_VIEWPORT };
        let window_start = (content_offset - self.beyond_bounds).max(0.0);
        let window_end = content_offset + viewport + self.beyond_bounds;
        let mut window: Vec<usize> = Vec::new();
        let mut keys: Vec<u64> = Vec::new();
        let mut lanes_of: Vec<usize> = Vec::new();
        let mut start_cursors = vec![f32::MAX; lanes];
        for (index, slot) in slots.iter().enumerate() {
            let top = slot.main_start;
            let bottom = top + heights[index];
            if bottom < window_start || top > window_end {
                continue;
            }
            window.push(index);
            keys.push(key(index));
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
        ctx.changed(&self.count);
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

        let rtl = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current())
            == LayoutDirection::Rtl;
        let policy = StaggeredPolicy::<A> {
            lanes,
            spacing: self.spacing,
            cross_spacing: self.cross_spacing,
            cross_before: self.content_padding_cross.0,
            cross_after: self.content_padding_cross.1,
            main_before: self.content_padding.0,
            rtl,
            cross_extent: cached_cross,
            window: window.clone(),
            keys,
            lanes_of,
            start_cursors,
            extent,
            shared,
            content_height: content_height.clone(),
            viewport_main: viewport_main.clone(),
            viewport_cross: viewport_cross.clone(),
            state: state.clone(),
            marker: PhantomData,
        };
        // A horizontal waterfall mirrors its MAIN axis under RTL, exactly as LazyRow does; a vertical
        // one keeps its main axis and mirrors the cross axis inside the policy.
        let modifier = Modifier::new()
            .lazy_scroll(content_height)
            .lazy_scroll_reverse(rtl && !A::VERTICAL)
            .then(A::scroll(state.clone()))
            .then(self.modifier);
        build_container(ctx, modifier, policy, |ctx| {
            for index in window {
                // A keyed group per item: the window shifts by one when the offset moves, and without
                // the key the item at loop position 0 would keep the previous item's slot (its
                // remembered values, effects and focus), instead of following its data key.
                let item_key = key(index).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                let group = ctx.next_key() ^ item_key;
                ctx.key(group, |ctx| content(ctx, index));
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





    /// The height cache is keyed by the item's KEY: with keys that are not indices, a cache written by
    /// index never hit and every item stayed priced at the estimate (so the extent, and the scroll
    /// range, were wrong).
    #[test]

    fn a_key_that_is_not_an_index_still_feeds_the_cache() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer, state: &ScrollState| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyVerticalStaggeredGrid::new()
                    .lanes(GridCells::Fixed(2))
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(200.0))
                    .items_with(
                        3,
                        // Keys far from the indices, as a data id would be.
                        |index| 1000 + index as u64 * 10,
                        |ctx, index| {
                            let height = if index == 0 { 1000.0 } else { 10.0 };
                            Stack::new()
                                .modifier(Modifier::new().size(10.0, height))
                                .build(ctx, |_| {});
                        },
                    )
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        for _ in 0..3 {
            frame(&mut composer, &state);
        }
        // The 1000 dp item makes the content 1000 tall, so the scroll range is 800.
        let limit = state.fling_limit.peek();
        assert!(
            (limit - 800.0).abs() <= 1.0,
            "the measured height reaches the extent through the key: {limit}"
        );
    }

    /// `GridCells::Adaptive` counts the lanes from the cross extent the last measure wrote back, so the
    /// first frame composes one lane and the next corrects it.
    #[test]

    fn adaptive_lanes_come_from_the_measured_cross_extent() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer, state: &ScrollState| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyVerticalStaggeredGrid::new()
                    .lanes(GridCells::Adaptive(90.0))
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(200.0))
                    .items(6, |ctx, index| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .size(10.0, 40.0)
                                    .test_tag(format!("adaptive-{index}")),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        for _ in 0..3 {
            frame(&mut composer, &state);
        }
        let root = composer.layout_root().unwrap();
        let items: Vec<usize> = root.children.clone();
        assert!(items.len() >= 3, "six items compose: {items:?}");
        let first_three: Vec<(f32, f32)> = items
            .iter()
            .take(3)
            .map(|child| {
                let node = &composer.arena_nodes()[*child];
                (node.position.x, node.position.y)
            })
            .collect();
        assert!(
            first_three.iter().all(|(_, y)| (*y - first_three[0].1).abs() < 0.5),
            "a 300 dp grid at 90 dp per cell fits three lanes: {first_three:?}"
        );
        assert!(
            first_three[1].0 > first_three[0].0 && first_three[2].0 > first_three[1].0,
            "and the lanes run across: {first_three:?}"
        );
    }


    /// A WRAP-CONTENT parent hands both axes f32::MAX. The grid must still report a usable size and
    /// compose its window instead of collapsing to zero or poisoning the parent with the sentinel.
    #[test]
    fn an_unbounded_parent_still_measures_a_usable_grid() {
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
                    .spacing(8.0)
                    .state(s)
                    .items(6, |ctx, index| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .size(10.0, 40.0)
                                    .test_tag(format!("free-{index}")),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::UNBOUNDED);
        }
        let root = composer.layout_root().unwrap();
        assert!(root.measured_size.height > 0.0, "a usable height: {:?}", root.measured_size);
        assert!(
            root.measured_size.width < 100_000.0,
            "and the cross sentinel never reaches the parent: {:?}",
            root.measured_size
        );
        assert!(!root.children.is_empty(), "the window composes under an unbounded parent");
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
