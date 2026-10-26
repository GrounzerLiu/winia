use super::*;

// ═══════════════════════════════════════════════════════
// LazyListPolicy — the measure policy
// ═══════════════════════════════════════════════════════

/// The lazy list's measure pass: place the registered children against the anchor, write the
/// measured sizes back into the cache, and report the viewport extent.
///
/// Axis-agnostic — `A: LazyAxis` decides which way is "main" (LazyColumn and LazyRow share this).
pub(crate) struct LazyListPolicy<A: LazyAxis> {
    /// Which way is "main" — the only thing the two list kinds differ by.
    pub axis: PhantomData<A>,
    /// Measured item sizes, shared with `build` (which prices unmeasured items from it).
    pub cache: crate::runtime::state::State<ItemHeightCache>,
    /// The viewport extent. Written here, read by the NEXT `build` to choose its window.
    pub viewport: crate::runtime::state::State<f32>,
    /// Total content extent (measured + estimated + content padding) — the scroll clamp's input.
    pub content_height: crate::runtime::state::Backchannel<f32>,
    /// How far a fling may travel; `f32::MAX` until the first measure, a real `0` when the content fits.
    pub fling_limit: crate::runtime::state::Backchannel<f32>,
    /// Compose's `isScrollInProgress`: the same flag the drag and fling paths write, shared with the
    /// `ScrollState` the list assembles.
    pub is_scroll_in_progress: crate::runtime::state::State<bool>,
    /// Space between consecutive items, along the main axis.
    pub spacing: f32,
    /// How many items the data holds; the composition window is a sub-range of it.
    pub total: usize,
    /// Main-axis content padding `(before, after)` — vertical for a column, horizontal for a row.
    pub content_padding: (f32, f32),
    /// Cross-axis content padding `(before, after)` — shrinks the space an item may use.
    pub cross_padding: (f32, f32),
    /// Registration order → global item index (sticky items are appended last, so they paint on top).
    pub globals: Vec<usize>,
    /// Global item index → item key, extracted from the intervals during `build`. `measure` writes
    /// the same view into the cache, so an item's identity keeps its height across data changes.
    pub keys: Vec<u64>,
    /// The same keys as a set: `finish_frame` asks "is this key still in the data?" once per
    /// off-screen state, so a linear scan of `keys` would be O(offscreen * total) per layout.
    pub key_set: std::collections::HashSet<u64>,
    /// Child indices of the sticky items in registration order (global index ascending).
    pub sticky_children: Vec<usize>,
    /// The global index of the sticky header currently PINNED, found by walking back at `build`;
    /// while pinned the anchor is `(pin, 0)`.
    pub pin: Option<usize>,
    /// Reverse layout (`reverseLayout`): a placement's content coordinate is
    /// `content_h - mirrored - item_size` (index 0 sits at the content's bottom).
    pub reverse: bool,
    /// Keys that are new to the DATA and have not been placed yet. A key that is new to the composed
    /// window because the user scrolled is NOT in here, so it does not fade in: fading it makes a
    /// scroll look like a strobe (measured: a jump to offset 4000 composed keys 35..39 at alpha 0).
    /// Each key is removed on its first place, so a fade starts once even across the several compose
    /// passes a frame may take.
    pub pending_new: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    /// The resolved layout direction. A horizontal list mirrors its main axis under RTL (Compose's
    /// LazyRow starts at the right), and BOTH axes mirror the CROSS axis: `content_padding_cross`'s
    /// "before" is the start edge, which is the right one in RTL.
    pub rtl: bool,
    /// The `reverse_layout` the caller asked for, before the direction is folded in — reported in the
    /// layout info, where a caller wants the parameter and not only the effective mirror.
    pub reverse_requested: bool,
    /// Compose's `LazyListState.layoutInfo`: written at the end of every measure pass.
    pub layout_info: crate::runtime::state::State<std::sync::Arc<super::LazyListLayoutInfo>>,
    /// Paging snap (from `snap_paging`): the step is written back only when every measured item
    /// measures at the viewport's main size, and `None` (no snapping) otherwise — with items of
    /// unequal size there is no snap position to speak of.
    pub snap_paging: bool,
    /// Every item fills the viewport along the main axis (`fillParentMax*`), from `fill_items`.
    pub fill_items: bool,
    /// Pin every item's main size (`PageSize.Fixed`), taking precedence over `fill_items`.
    pub item_main_size: Option<f32>,
    /// Where inside an item a snapping list rests (`SnapPosition`), from `snap_offset`.
    pub snap_offset: f32,
    /// The release threshold written into the snap record (`SnapInfo::positional_threshold`).
    pub snap_positional_threshold: f32,
    /// The settle spring written into the snap record (`SnapInfo::settle`).
    pub snap_animation_spec: crate::animation::SpringSpec,
    /// The page ceiling written into the snap record (`SnapInfo::max_pages_per_fling`).
    pub snap_max_pages: usize,
    /// Items composed beyond the viewport on each side (`beyondViewportPageCount`).
    pub beyond_bounds: usize,
    pub state: LazyListState,  // derived anchor, written back after the measure (exact values)
}

impl<A: LazyAxis> std::fmt::Debug for LazyListPolicy<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LazyListPolicy")
    }
}

impl<A: LazyAxis> LazyListPolicy<A> {
    /// Build the Compose-shaped layout snapshot for this pass.
    ///
    /// Item offsets are VIEWPORT coordinates, which is the arena position shifted by the render
    /// translate: `-offset` for a straight list, `-(content - viewport - offset)` for a mirrored one
    /// (`reverseLayout`, or RTL on a row). Sticky items are in `placements` too — their pinned
    /// position is what a caller wants to see — so the list is sorted by index afterwards, because
    /// the child order is "ordinary items, then sticky".
    fn layout_info_snapshot(
        &self,
        placements: &[crate::layout::node::Placement],
        measured: &[(f32, f32)],
        content_h: f32,
        vh: f32,
        cross_extent: f32,
    ) -> super::LazyListLayoutInfo {
        let offset = self.state.offset.get();
        let shift = if self.reverse {
            content_h - vh - offset
        } else {
            offset
        };
        let viewport_start = -self.content_padding.0;
        let viewport_end = vh + self.content_padding.1;
        let mut visible = Vec::new();
        for (i, placement) in placements.iter().enumerate() {
            let size = measured.get(i).map(|(main, _)| *main).unwrap_or(0.0);
            // The placement's position IS a Size here: main axis in one component, cross in the other.
            let main = A::main_size(crate::unit::Size::new(
                placement.position.x,
                placement.position.y,
            ));
            let start = main - shift;
            // Intersecting the viewport (Compose's `minMainAxisOffset`/`maxMainAxisOffset` check);
            // the items composed by `beyond_bounds` but outside it are NOT visible.
            if start < viewport_end && start + size > viewport_start {
                let global = self.globals.get(i).copied().unwrap_or(i);
                visible.push(super::LazyListItemInfo {
                    index: global,
                    key: self.keys.get(global).copied().unwrap_or(global as u64),
                    offset: start,
                    size,
                });
            }
        }
        visible.sort_by_key(|item| item.index);
        let node_size = A::size(vh, cross_extent);
        super::LazyListLayoutInfo {
            visible_items: visible,
            viewport_start_offset: viewport_start,
            viewport_end_offset: viewport_end,
            total_items: self.total,
            viewport_size: (node_size.width, node_size.height),
            vertical: A::VERTICAL,
            reverse_layout: self.reverse_requested,
            rtl: self.rtl,
            before_content_padding: self.content_padding.0,
            after_content_padding: self.content_padding.1,
            spacing: self.spacing,
        }
    }

}
impl<A: LazyAxis> crate::layout::node::MeasurePolicy for LazyListPolicy<A> {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::node::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::constraints::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::node::Placement>) {
        // Viewport main extent: a bounded constraint is used directly (and written back to the
        // cache); an unbounded one — the parent drives its content, and a Column with no fixed
        // height hands its children `f32::MAX` — falls back to the cached value, because an
        // f32::MAX viewport makes the scroll clamp zero the offset. (`is_finite()` is TRUE for
        // f32::MAX, so the check has to be the framework's `< f32::MAX`.)
        let main_max = A::main_max(&constraints);
        let vh = if main_max < f32::MAX && main_max > 0.0 {
            // NOT a silent set: a viewport change (a resize) has to notify `build`, so the
            // window is re-composed against the new viewport. A stable value is de-duplicated by
            // PartialEq and notifies nobody, so this cannot oscillate.
            self.viewport.set(main_max);
            main_max
        } else {
            self.viewport.get()
        };

        // Measure each child: the cross axis inherits the parent's max less the cross-axis
        // `contentPadding` (Compose: an item's usable width is the viewport minus that padding), and
        // the main axis is unbounded so the item wraps to its content — passing the parent's max
        // (= the viewport) there would inflate every item to fill it. `fill_items` opts back into
        // exactly that inflation, which is what `fillParentMaxWidth` asks for and what makes a snap
        // position an item boundary.
        let (cb, ca) = self.cross_padding;
        let (pad_before, pad_after) = self.content_padding;
        let mut child_constraints = A::child_constraints(constraints);
        let cross_max = A::cross_max(&child_constraints);
        child_constraints = A::with_cross_max(child_constraints, (cross_max - cb - ca).max(0.0));
        // A pager's Fill pages occupy the usable viewport between the main-axis content padding,
        // matching Compose's `mainAxisAvailableSize`. Ordinary lazy items remain wrap-content.
        let uniform_page_size = if let Some(size) = self.item_main_size {
            Some(size)
        } else if self.fill_items && vh > 0.0 {
            Some((vh - pad_before - pad_after).max(0.0))
        } else {
            None
        };
        if let Some(size) = uniform_page_size {
            child_constraints = A::with_main_exact(child_constraints, size);
        }
        let mut placements = Vec::with_capacity(children.len());
        let mut measured: Vec<(f32, f32)> = Vec::with_capacity(children.len());
        for &c in children.iter() {
            let (size, _) = crate::layout::node::measure_node(nodes, policies, c, child_constraints);
            measured.push((A::main_size(size), A::cross_size(size)));
        }

        // Global index mapping: the registration order runs start..end. `build` hands the policy
        // only the anchor's first index, so the children's global order comes from `globals`
        // (build passes (start, end) to the policy) and is written back here positionally.
        let mut cache = self.cache.get();
        // `fill_items` pins every item to the viewport, so unmeasured items can be priced exactly. It
        // has to be declared, or the flat estimate is used and every position outside the measured
        // window — and so every fling that leaves it — is computed from the wrong size.
        let uniform = uniform_page_size;
        if cache.uniform != uniform {
            cache.set_uniform(uniform);
        }
        for (i, (h, _)) in measured.iter().enumerate() {
            let global = self.globals[i];
            cache.record(global, *h);
            // Also record the KEY view (`keys[global]` is that item's key): an item's identity
            // keeps its height across data changes, and `rebase` migrates it to the new index.
            if let Some(&k) = self.keys.get(global) {
                cache.record_keyed(k, *h);
            }
        }
        // Total content extent: measured items plus the estimate for unmeasured ones — this is
        // what `apply_scroll_delta` derives `max_offset` from. It includes the main-axis content
        // padding, matching Compose: max_offset = before + content + after - viewport.
        // The snap offset is a PLACEMENT shift (Compose shifts the page, not the scroll target).
        // Price spacing only BETWEEN items, then add enough trailing room for the final snap position
        // to reach the same visual alignment as every earlier page.
        let snap_shift = self.snap_offset.max(0.0);
        let mut content_h = pad_before + pad_after + snap_shift;
        for g in 0..self.total {
            // `coverage_height`, like `prefix_height` and the window walk: `max_off` clamps an offset that
            // `prefix_height` produced, so the two have to price unmeasured items the same way or the end
            // of the list disagrees with itself by a little on every frame.
            content_h += cache.coverage_height(g);
            if g + 1 < self.total {
                content_h += self.spacing;
            }
        }
        if self.snap_paging {
            if let Some(page_size) = uniform_page_size.filter(|size| *size > 0.0) {
                let trailing_snap_room =
                    (vh - (pad_before + pad_after + snap_shift + page_size)).max(0.0);
                content_h += trailing_snap_room;
            }
        }
        self.content_height.set(content_h);
        // Out-of-range clamp: `scroll_to_item` carries no `total`, so a programmatic scroll past
        // the end is pulled back here (Compose clamps the scroll position during measure too).
        let max_off = (content_h - vh).max(0.0);

        // Programmatic jump: consume `jump_request` (after `max_off`, because an animated target
        // needs the clamp). The pixel offset is derived from the WRITTEN-BACK cache (measured
        // sizes plus estimates) — the same `prefix_height` the anchor resolution and the placement
        // use — so a round trip is exact: jumping to 500 lands at 500 instead of drifting by the
        // estimate error (the old implementation landed at 505 for 500 x 48 against 47.5). The
        // formula is the same under `reverseLayout` (offset = before + prefix + offset), where it
        // means the item's BOTTOM hugs the viewport bottom — isomorphic in mirrored coordinates.
        if let Some((req_idx, req_off, animate)) = self.state.jump_request.get() {
            let idx = req_idx.min(self.total.saturating_sub(1));
            let target = pad_before + prefix_height(&cache, idx, self.spacing) + req_off;
            self.state.jump_request.set(None);
            if animate {
                // Animated scroll (Compose's `animateScrollToItem` — a spring to the target).
                // `push_animatable` replaces a running animation on the same state; retargeting
                // carries the velocity over.
                //
                // The flag goes UP for the length of the spring, not down. It is the same state the
                // drag and fling paths write (`build` hands this clone to the list's ScrollState), and
                // Compose's guard on a programmatic jump — `!monthsListState.isScrollInProgress` before
                // `scrollToItem` (`DatePicker.kt:1548-1553`) — depends on it being set while this runs.
                // Clearing it here left that guard inert for the month arrows: an outside change landing
                // mid-animation would `scroll_to_item` and `cancel_animation` straight through the motion
                // the user just asked for.
                //
                // The variant whose early exits all report completion is deliberate: the plain
                // `push_animatable_with_done` has TWO silent early exits — already at the target
                // (`animation.rs:475-480`), and same-state-same-target dedup when the identical motion is
                // already running (`animation.rs:491-494`) — and neither fires `done`, which would leave
                // the flag set with no animation to clear it. The velocity variant reports completion in
                // both (`animation.rs:195-198`, `:212-215`). Velocity 0 is right for a jump — nothing was
                // flung.
                let in_progress = self.is_scroll_in_progress.clone();
                in_progress.set(true);
                crate::animation::push_animatable_with_velocity_and_done(
                    self.state.offset.clone(),
                    target.clamp(0.0, max_off),
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
                    0.0,
                    move || {
                        in_progress.set(false);
                    },
                );
            } else {
                // Immediate jump: cancel a running fling and clear the scrolling flag.
                //
                // CLAMPED like the animated path: `scroll_to_item(0, -200)` or an index past the end
                // produced a target outside `[0, max_off]`, and the pixel offset is what every later
                // frame derives the anchor from — a negative one put the content above the viewport and
                // an overshooting one left the list unable to scroll back.
                crate::animation::cancel_animation(&self.state.offset);
                self.is_scroll_in_progress.set(false);
                self.state.offset.set(target.clamp(0.0, max_off));
            }
        }
        // Fling limit written back — both the input path's `ScrollState::fling` and the
        // programmatic `LazyListState::fling` read it.
        self.fling_limit.set(max_off);
        self.state.fling_limit.set(max_off);
        // The viewport is what a pager divides a pixel offset by; write it beside the limit.
        if self.state.viewport_main.peek() != vh {
            self.state.viewport_main.set(vh);
        }
        // Paging snap step, written back so a finished fling knows where to land. Only given when
        // every measured item is exactly the viewport's main size: `measured` covers every item in
        // this frame's composition window, so unequal items are caught and snapping turns itself off
        // rather than snapping to a step that means nothing. Fewer than two items is no snapping
        // either — there is nothing to page to, and nothing should move.
        // A paged list rests on an item boundary; a FIXED-size page rests wherever its size says, and
        // the spacing between items is part of the distance to the next one.
        let page_size = uniform_page_size.unwrap_or(vh);
        let snap = if self.snap_paging
            && self.total >= 2
            && uniform_page_size.is_some()
            && page_size > 0.0
            && measured.iter().all(|(size, _)| (size - page_size).abs() < 0.5)
        {
            Some(crate::fling::SnapInfo {
                step: page_size + self.spacing,
                // Page k is placed after `pad_before`, but its scroll snap position is `k * step`;
                // the leading padding belongs to layout, not to the scroll state's snap anchor.
                anchor: 0.0,
                positional_threshold: self.snap_positional_threshold,
                settle: self.snap_animation_spec.clone(),
                max_pages_per_fling: self.snap_max_pages,
                // Compose's `MinFlingVelocityDp = 400.dp`, in px at the density this list lays out
                // under — below it a fling settles back on the page it started from.
                min_fling_velocity: 400.0 * crate::runtime::density::current_density().density,
            })
        } else {
            None
        };
        if self.state.snap.peek() != snap {
            self.state.snap.set(snap);
        }
        let clamped = self.state.offset.get().clamp(0.0, max_off);
        if clamped != self.state.offset.get() {
            self.state.offset.set(clamped);
        }

        // Exact anchor write-back: derive `first_visible_index` / `first_visible_offset` from the
        // real heights (already in the cache). `build` worked from the ESTIMATES; this is the
        // precise value, mirroring Compose updating from the measure result.
        //
        // While pinned the anchor is `(pin, 0)` — Compose's `firstVisibleItemIndex` IS the pinned
        // header — otherwise the natural anchor (recovered from the pixel offset).
        let offset = self.state.offset.get();
        let (real_first, real_off) = match self.pin {
            Some(p) if offset >= pad_before + prefix_height(&cache, p, self.spacing) => (p, 0.0),
            _ => anchor_from_offset(&cache, offset, self.spacing, pad_before),
        };
        self.state.first_visible_index.set(real_first);
        self.state.first_visible_offset.set(real_off);

        // Same-frame window coverage. `build` picked the window from the viewport the PREVIOUS measure
        // wrote, and THIS measure just learned the real one — on the frame the viewport grows, that
        // window is short and the new bottom has nothing in it. No measurement can conjure the missing
        // items (they were never composed), so ask the frame for one more compose instead of drawing an
        // empty region and converging on the next frame (`docs/lazy-column.md`).
        //
        // The condition is "the window I was given does not cover what the real viewport needs", which
        // is exactly the case that leaves a hole on screen. It is deliberately NOT "the viewport
        // changed": a frame where the viewport is unchanged and the window still covers asks for
        // nothing, so a scrolling frame stays at one compose+layout pass.
        {
            let (need_start, need_end) =
                visible_range(&cache, real_first, real_off, vh, self.spacing, self.total, self.beyond_bounds);
            // `globals` is the composed window in registration order (sticky items are appended last,
            // so min/max — not first/last — give the range).
            let composed_start = self.globals.iter().copied().min().unwrap_or(usize::MAX);
            let composed_end = self.globals.iter().copied().max().map_or(0, |m| m + 1);
            if need_end > composed_end || need_start < composed_start {
                crate::runtime::composer::request_compose_after_layout();
            }
        }

        // Anchor placement in CONTENT coordinates (main axis = the item's accumulated position in
        // the content, WITHOUT the offset) — scrolling is the framework's `translate(-offset)`. A
        // placement that carried the offset too would shift twice (measured: after scrolling 3000
        // the content left the viewport entirely).
        //
        // The content starts at the `before` content padding (Compose's `contentPadding`).
        //
        // Sticky pass (Compose's `LazyListMeasure`):
        //   forward  final(i) = max(C(i) - offset, before)  — scrolled past, the header pins at `before`
        //   backward final(prev) = min(final(prev), final(next) - h(prev))
        //            — the previous header slides out while the next is within h(prev) of the top
        // The placement coordinate is `final + offset`; ordinary items keep their natural position.
        // Cross-axis extent, and where the content starts on it.
        //
        // A lazy list FILLS its cross axis (that is what `fill_max_*` asks for), but an unbounded
        // parent hands over the framework's `f32::MAX` sentinel — reporting it makes the node 3.4e38
        // wide and poisons everything downstream (the scroll container's content width, hit testing,
        // a scrollbar's geometry). Compose has no equivalent: its `Constraints.Infinity` is an int
        // sentinel that never reaches a size computation. So an unbounded cross axis reports the
        // WIDEST CHILD instead, floored by the incoming minimum and widened by the cross content
        // padding, which is the same box a filling child would occupy.
        let cross_max = A::cross_max(&constraints);
        let widest_cross = measured.iter().map(|(_, cross)| *cross).fold(0.0f32, f32::max);
        let cross_extent = if cross_max < f32::MAX {
            cross_max
        } else {
            (widest_cross + cb + ca).max(A::cross_min(&constraints))
        };

        let mut fin: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for &ci in &self.sticky_children {
            let v = pad_before + prefix_height(&cache, self.globals[ci], self.spacing) - offset;
            fin.insert(ci, v.max(pad_before));
        }
        for k in (1..self.sticky_children.len()).rev() {
            let prev = self.sticky_children[k - 1];
            let next = self.sticky_children[k];
            let nf = fin[&next];
            let pf = fin[&prev];
            let h_prev = measured[prev].0;
            fin.insert(prev, pf.min(nf - h_prev));
        }
        let mut main_pos = 0.0f32;
        let mut main_init = false;
        for (i, _c) in children.iter().enumerate() {
            let (h, w) = measured[i];
            let pos = match fin.get(&i) {
                Some(f) => *f + offset,
                None => {
                    if !main_init {
                        main_pos = pad_before + prefix_height(&cache, self.globals[i], self.spacing);
                        main_init = true;
                    }
                    // The snap offset moves every item forward, which is what makes a page smaller
                    // than the viewport rest where `SnapPosition` asks (Compose shifts the page in
                    // layout, it does not skew the snap positions).
                    let p = main_pos + snap_shift;
                    main_pos += h + self.spacing;
                    p
                }
            };
            // Reverse layout: mirrored coordinates (accumulated from the bottom) → content
            // coordinate = content_h - pos - h (index 0 sits at the content's bottom; anchors,
            // visible ranges and the pin walk keep their formulas in mirrored coordinates).
            let content_pos = if self.reverse { content_h - pos - h } else { pos };
            placements.push(crate::layout::node::Placement {
                // The cross axis starts at `cross_before` — the START padding, so under RTL the item
                // is mirrored to the far edge (Compose's `contentPadding` start/end follow the layout
                // direction; a narrow item hugs the start edge, a filling one covers the padded box).
                // Only a VERTICAL list mirrors its cross axis under RTL — there the cross axis is
                // horizontal, so the "start" padding belongs on the right edge. A horizontal list's
                // cross axis is vertical: RTL never flips it (the MAIN axis mirroring above already
                // accounts for the direction).
                position: A::point(
                    content_pos,
                    if self.rtl && A::VERTICAL {
                        (cross_extent - cb - w).max(0.0)
                    } else {
                        cb
                    },
                ),
                size: A::size(h, w),
            });
        }

        self.cache.set(cache);
        // Compose's `layoutInfo`: what this pass actually placed, in viewport coordinates. Written
        // before the policy returns so a reader woken by it sees the same numbers the frame draws.
        let info = self.layout_info_snapshot(&placements, &measured, content_h, vh, cross_extent);
        self.layout_info.set(std::sync::Arc::new(info));
        (A::size(vh, cross_extent), placements)
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[crate::layout::node::Placement]) {
        // Consumed once per layout pass: the offset change the anchor correction made, so the host can
        // rebase its per-item references instead of animating rows that only moved with the viewport.
        let anchor_shift = {
            let mut shift = self
                .state
                .anchor_shift
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let taken = *shift;
            *shift = 0.0;
            taken
        };
        let main_shift = if A::VERTICAL {
            crate::unit::Offset::new(0.0, anchor_shift)
        } else {
            crate::unit::Offset::new(anchor_shift, 0.0)
        };
        self.state.item_animations.begin_frame(main_shift);
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
            nodes[c].item_paint = None;
            let global = self.globals.get(i).copied().unwrap_or(i);
            let key = self.keys.get(global).copied().unwrap_or(global as u64);
            // The marker sits on the item's ROOT content; the lazy list registers a wrapper group
            // node per item, so the animation belongs to the wrapper and the marker is its direct
            // content child. A deeper marker belongs to a nested item-aware layout and must not
            // animate the whole outer item.
            let Some(spec) = crate::layout::item_animation::wrapper_item_spec(nodes, c) else {
                self.state.item_animations.retire(key);
                continue;
            };
            let position = nodes[c].position;
            // Consumed exactly once: an item that is first composed in a later pass of the same frame
            // still fades in, and one that was already placed does not start a second fade.
            let new_in_data = self
                .pending_new
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&key);
            self.state
                .item_animations
                .place(nodes, c, key, position, &spec, new_in_data);
        }
        self.state.item_animations.finish_frame(Some(&self.key_set));
    }
}

