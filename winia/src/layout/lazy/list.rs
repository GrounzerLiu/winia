use super::*;

// ═══════════════════════════════════════════════════════

/// A lazily built list (Compose's `LazyColumn` / `LazyRow`), whose axis the type parameter picks.
///
/// Only the visible items are composed and measured (plus the prefetch window either side). `items` takes
/// a stable key factory, so a scroll position survives an insert or a removal earlier in the data.
pub struct LazyList<A: LazyAxis> {
    pub(crate) axis: PhantomData<A>,
    pub(crate) state: Option<LazyListState>,
    pub(crate) spacing: f32,
    pub(crate) modifier: Modifier,
    pub(crate) intervals: IntervalList,
    /// Main-axis content padding `(before, after)` — vertical = top/bottom, horizontal = start/end
    pub(crate) content_padding: (f32, f32),
    /// Cross-axis content padding `(before, after)` — vertical = start/end, horizontal = top/bottom
    pub(crate) cross_padding: (f32, f32),
    /// Reverse layout (Compose's `reverseLayout`): the content is laid out from the far end of the main
    /// axis — index 0 at the bottom/right, offset 0 showing the list's start (item 0 at the viewport's
    /// far edge), and the scroll direction stays the same (offset grows toward the list's end).
    pub(crate) reverse: bool,
    /// Paging snap (Compose's `snapFlingBehavior`): every item exactly fills the viewport along the
    /// main axis, and a finished fling settles onto the nearest item boundary.
    ///
    /// Only meaningful for a PAGED list — which is what the date picker's months are (`fillParentMaxWidth`
    /// items), the same shape material3 reaches with `LazyRow` + `Box(fillParentMaxWidth())`
    /// (`DatePicker.kt:1722-1750`). Items of unequal main size turn it off instead: there is no snap
    /// position to speak of.
    pub(crate) snap_paging: bool,
    /// Every item fills the viewport along the main axis (`fillParentMaxWidth` / `fillParentMaxHeight`).
    /// Off by default, where items wrap to their content.
    pub(crate) fill_items: bool,
    /// Pin every item's main size to this instead of the viewport (`Pager` with `PageSize.Fixed`). A
    /// fixed page does not fill the viewport, so the snap step is this size plus the spacing.
    pub(crate) item_main_size: Option<f32>,
    /// How many items beyond the viewport the composition window covers on each side
    /// (`beyondViewportPageCount`). Defaults to `LAZY_BEYOND_BOUNDS`.
    pub(crate) beyond_bounds: usize,
    /// Whether the list reacts to wheel/drag input. `false` keeps programmatic scrolling working.
    pub(crate) user_scroll: bool,
    /// Where inside an item the list comes to rest when `snap_paging` is on: `0.0` is its leading edge,
    /// `(viewport - item) / 2.0` centres a fixed page, `viewport - item` puts it at the trailing edge.
    pub(crate) snap_offset: f32,
    /// How far into the page a release has to be before it carries on to the next one, in `0.0..=1.0`
    /// (Compose's `snapPositionalThreshold`); written back into the snap spec.
    pub(crate) snap_positional_threshold: f32,
    /// The spring a paging settle runs on (Compose's `snapAnimationSpec`); written back into the snap
    /// spec, so the wheel's page step and a fling settle with the same curve.
    pub(crate) snap_animation_spec: crate::animation::SpringSpec,
    /// How many pages one fling may carry (Compose's `PagerSnapDistance.atMost`), at least one.
    pub(crate) snap_max_pages: usize,
    /// The fling strategy the assembled scroll state runs with (`None` = the built-in snapping one).
    pub(crate) fling_behavior: Option<std::sync::Arc<dyn crate::fling::FlingBehavior>>,
}

/// A vertical lazy list (Compose's `LazyColumn`).
pub type LazyColumn = LazyList<crate::layout::axis::VerticalAxis>;
/// A horizontal lazy list (Compose's `LazyRow`)
pub type LazyRow = LazyList<crate::layout::axis::HorizontalAxis>;

impl LazyList<crate::layout::axis::VerticalAxis> {
    /// A vertical list (LazyColumn)
    pub fn new() -> Self { Self::new_list() }
}

impl LazyList<crate::layout::axis::HorizontalAxis> {
    /// A horizontal list (LazyRow)
    pub fn new() -> Self { Self::new_list() }
}

impl<A: LazyAxis> LazyList<A> {
    pub(crate) fn new_list() -> Self {
        Self {
            axis: PhantomData,
            state: None,
            spacing: 0.0,
            modifier: Modifier::new(),
            intervals: IntervalList::new(),
            content_padding: (0.0, 0.0),
            cross_padding: (0.0, 0.0),
            reverse: false,
            snap_paging: false,
            fill_items: false,
            item_main_size: None,
            beyond_bounds: LAZY_BEYOND_BOUNDS,
            user_scroll: true,
            snap_offset: 0.0,
            snap_positional_threshold: 0.5,
            snap_animation_spec: crate::fling::default_snap_settle(),
            snap_max_pages: 1,
            fling_behavior: None,
        }
    }

    /// Main-axis content padding (Compose's `contentPadding`): top/bottom for a vertical list,
    /// start/end for a horizontal one. The content is placed from `before`; at either end of the
    /// scroll the content stops at the padding rather than against the viewport edge; a sticky header
    /// pins at `before`; the total content size is `before + items + after` (`max_offset` includes it).
    pub fn content_padding(mut self, before: f32, after: f32) -> Self {
        // Negative padding would let items grow OUTSIDE the viewport and make `max_off` negative;
        // Compose's `PaddingValues` allows it, but the lazy arithmetic here does not, so it clamps.
        self.content_padding = (before.max(0.0), after.max(0.0));
        self
    }

    /// Cross-axis content padding (start/end for a vertical list): shrinks the width an item may use
    /// and places items from `before` (the cross axis of Compose's `contentPadding`).
    pub fn content_padding_cross(mut self, before: f32, after: f32) -> Self {
        self.cross_padding = (before.max(0.0), after.max(0.0));
        self
    }

    /// Reverse layout (Compose's `reverseLayout`): index 0 sits at the far end of the main axis (the
    /// bottom for a LazyColumn), offset 0 showing item 0 there, and the scroll direction stays the
    /// same (offset grows toward the list's end). Typical use: a chat list with the newest at the end.
    pub fn reverse_layout(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    /// Inject an external scroll state (kept across recompositions; remembered internally if absent)
    pub fn state(mut self, state: LazyListState) -> Self {
        self.state = Some(state);
        self
    }

    /// Every item fills the viewport along the main axis — the effect of `fillParentMaxWidth` /
    /// `fillParentMaxHeight`, and the half of material3's month list that reads
    /// `Box(Modifier.fillParentMaxWidth())` (`DatePicker.kt:1735`).
    ///
    /// Off by default, where the main axis is unbounded and an item wraps to its content.
    pub fn fill_items(mut self, enabled: bool) -> Self {
        self.fill_items = enabled;
        self
    }

    /// Paging snap (Compose's `snapFlingBehavior`, which material3 reaches through
    /// `DatePickerDefaults.rememberSnapFlingBehavior`): every item fills the viewport along the main
    /// axis, and a finished fling settles onto the nearest item boundary — so a fast swipe stops on a
    /// whole month instead of between two.
    ///
    /// Pair it with [`Self::fill_items`]: the measure pass writes the step back only when every measured
    /// item really does measure at the viewport's size, and turns snapping off otherwise rather than
    /// snapping to a step that means nothing.
    pub fn snap_paging(mut self, enabled: bool) -> Self {
        self.snap_paging = enabled;
        self
    }

    /// How far into a page a release has to be before it carries on to the next one, in `0.0..=1.0`
    /// (Compose's `snapPositionalThreshold`, `0.5` by default). Only consulted under the fling floor.
    pub fn snap_positional_threshold(mut self, threshold: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&threshold) && threshold.is_finite(),
            "snap_positional_threshold must be within 0.0..=1.0, got {threshold}"
        );
        self.snap_positional_threshold = if threshold.is_finite() { threshold.clamp(0.0, 1.0) } else { 0.5 };
        self
    }

    /// The spring a paging settle runs on (Compose's `snapAnimationSpec`). The default is material3's
    /// `DefaultEffects` spring; the wheel's page step uses the same spec, so the two settles of one
    /// pager cannot drift apart.
    pub fn snap_animation_spec(mut self, spec: crate::animation::SpringSpec) -> Self {
        self.snap_animation_spec = spec;
        self
    }

    /// How many pages one fling may carry, at least one (Compose's `PagerSnapDistance.atMost`). The
    /// default `1` is Compose's own: one gesture moves at most one page.
    pub fn snap_max_pages(mut self, pages: usize) -> Self {
        self.snap_max_pages = pages.max(1);
        self
    }

    /// The strategy this list flings with (see [`crate::fling`]). `None` keeps the built-in one: a
    /// paging list settles on a boundary, anything else decays.
    pub fn fling_behavior(
        mut self,
        behavior: std::sync::Arc<dyn crate::fling::FlingBehavior>,
    ) -> Self {
        self.fling_behavior = Some(behavior);
        self
    }

    /// Pin every item's main size instead of filling the viewport (`PageSize.Fixed`).
    pub fn item_main_size(mut self, size: f32) -> Self {
        // Only a finite, positive page size is meaningful; anything else is ignored rather than
        // pushed into the item constraints (a NaN there poisons every measurement).
        self.item_main_size = (size.is_finite() && size > 0.0).then_some(size);
        self
    }

    /// How many items beyond the viewport the composition window covers (`beyondViewportPageCount`).
    pub fn beyond_bounds(mut self, items: usize) -> Self {
        self.beyond_bounds = items;
        self
    }

    /// Whether the list reacts to wheel and drag input; `false` keeps programmatic scrolling working.
    pub fn user_scroll(mut self, enabled: bool) -> Self {
        self.user_scroll = enabled;
        self
    }

    /// Where inside an item a snapping list comes to rest (`SnapPosition`).
    pub fn snap_offset(mut self, offset: f32) -> Self {
        self.snap_offset = if offset.is_finite() { offset.max(0.0) } else { 0.0 };
        self
    }

    /// Spacing between items, along the main axis
    pub fn spacing(mut self, s: f32) -> Self {
        // Compose requires a non-negative spacing; a negative one would make `prefix_height`
        // non-monotonic and the anchor recovery land on the wrong item, so it clamps to zero.
        self.spacing = if s.is_finite() { s.max(0.0) } else { 0.0 };
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// One fixed item (Compose's `item(key, content)`)
    pub fn item(mut self, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, None, c, false);
        self
    }

    /// A single item with an explicit key — Compose's `item(key = …) { }`.
    pub fn item_keyed(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c, false);
        self
    }

    /// `count` items with a key factory — Compose's
    /// `items(count, key: (index) -> Any, itemContent)`.
    pub fn items(
        mut self,
        count: usize,
        key: impl Fn(usize) -> u64 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        let k = Arc::new(key);
        let c = Arc::new(content);
        self.intervals.add(count, Some(k), c, false);
        self
    }

    /// `count` items without keys; the position acts as the key.
    pub fn items_plain(
        mut self,
        count: usize,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        let c = Arc::new(content);
        self.intervals.add(count, None, c, false);
        self
    }

    /// Iterate a data list — Compose's
    /// `items(items, key: (item) -> Any, itemContent)`. This is the entry point for STABLE-KEY
    /// iteration: keys come from the data (an id), so inserting or removing items at the head keeps
    /// the scroll position on the same item.
    /// Takes the list as an `Arc<Vec<T>>` or anything that converts into one — notably a
    /// [`crate::StateList`] snapshot, so a list that observes its own mutations feeds the lazy list
    /// without copying its elements:
    ///
    /// ```ignore
    /// LazyColumn::new()
    ///     .items_from(rows.snapshot(), |row| row.id, |ctx, _i, row| { /* ... */ })
    /// ```
    pub fn items_from<T: Send + Sync + 'static>(
        mut self,
        list: impl Into<Arc<Vec<T>>>,
        key: impl Fn(&T) -> u64 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize, &T) + Send + Sync + 'static,
    ) -> Self {
        let list = list.into();
        let k = Arc::new(key);
        let c = Arc::new(content);
        let lk = list.clone();
        let lc = list.clone();
        let n = list.len();
        let kk: Arc<dyn Fn(usize) -> u64 + Send + Sync> = Arc::new(move |g: usize| {
            k(&lk[g])
        });
        let cc: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(move |ctx, g| {
            c(ctx, g, &lc[g]);
        });
        self.intervals.add(n, Some(kk), cc, false);
        self
    }

    /// A sticky-header section — Compose's `stickyHeader(key, content)`. While scrolling it stays
    /// pinned at the main axis's start edge and the content slides underneath; the next sticky
    /// header pushes the previous one out. The key must be unique across the whole list.
    pub fn sticky_header(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c, true);
        self
    }
}

// ═══════════════════════════════════════════════════════
// Height cache + visible-range arithmetic
// ═══════════════════════════════════════════════════════

/// Per-item height cache (global index → measured height); unmeasured items are priced with the
/// estimate.
///
/// Public because external code may want the measured heights (custom scroll logic).
/// `scroll_to_item` does NOT need it: the anchor is authoritative and the measure pass derives
/// pixels from the cache by itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemHeightCache {
    /// The INDEX view (index → height), used by the linear scans (`prefix_height`, the anchor
    /// recovery, `visible_range`) and by the out-of-range estimate. It goes stale when items are
    /// inserted or reordered at the head; [`ItemHeightCache::rebase`] repairs it.
    pub heights: Vec<f32>,
    /// The KEY view (item key → height): keeps "identity → height" across data changes, so an
    /// item's height survives index shifts. Private; written by `record_keyed` and read back by
    /// `rebase`, which migrates it into the index view.
    pub(crate) keyed: std::collections::HashMap<u64, f32>,
    /// The last few measured heights, a ring — the sample behind [`ItemHeightCache::coverage_height`]'s
    /// median. Not the `keyed` view: this is about the rows being shown NOW, not about identity.
    pub(crate) recent: Vec<f32>,
    /// The item size when the list GUARANTEES it — `fill_items` pins every item to the viewport, so the
    /// size is known analytically and a measured item is not needed to price the rest.
    ///
    /// This is not a nicety. Without it the 2412 month pages of a date picker are priced at the flat
    /// `LAZY_ITEM_ESTIMATED_HEIGHT` until each is measured, and since the real page is several times
    /// that, every position past the measured window — and so every fling that leaves it — lands
    /// somewhere meaningless. Measured: with the estimate in place a flick moved the picker six months
    /// in one gesture and a plain drag moved it backwards.
    pub(crate) uniform: Option<f32>,
}

impl ItemHeightCache {
    pub fn new() -> Self { Self { heights: Vec::new(), keyed: std::collections::HashMap::new(), recent: Vec::new(), uniform: None } }

    /// Declare that every item measures `size` along the main axis, so unmeasured indices can be
    /// priced exactly instead of estimated. `None` restores the estimating behaviour.
    pub fn set_uniform(&mut self, size: Option<f32>) { self.uniform = size; }
}

impl ItemHeightCache {
    pub fn height(&self, index: usize) -> f32 {
        // Unmeasured (absent or 0) → the estimate; a recorded height above zero is returned as is.
        self.heights
            .get(index)
            .copied()
            .filter(|&h| h > 0.0)
            .unwrap_or_else(|| self.uniform.unwrap_or(LAZY_ITEM_ESTIMATED_HEIGHT))
    }

    /// The height to assume for an item that has NOT been measured, when deciding how much to COMPOSE
    /// (`visible_range`). `height` above is the flat estimate and stays as it is — the anchor math and
    /// the placements read it, and those must not start guessing differently.
    ///
    /// Coverage needs the error in the SAFE direction. A window sized on a height larger than the real
    /// items comes up short of the viewport, and a short window is what asks for a second compose+layout
    /// pass: measured on a cold jump (`lazy_column_demo`, jump to index 500, release) at ~1.5 ms of the
    /// frame's 7.0 ms. So an unmeasured item is assumed to be as tall as the MEDIAN of the heights around
    /// it, which is what it is for a list whose rows are uniform — the case that pays. Clamped to
    /// `[estimate/2, estimate]`: never taller than the flat estimate (that would only make windows longer
    /// than they need to be, since being wrong upward shrinks them) and never below half of it, so a list
    /// of stray 1px separators cannot blow a window up by an unbounded factor.
    ///
    /// This only changes behaviour where the window walk reaches unmeasured items: a warm scroll frame
    /// walks entirely over measured ones, and an empty cache has no median to use.
    pub fn coverage_height(&self, index: usize) -> f32 {
        if let Some(h) = self.heights.get(index).copied().filter(|&h| h > 0.0) {
            return h;
        }
        // A list that guarantees its item size (`fill_items`) is priced exactly — the median below is
        // a guess, and the guarantee makes the guess unnecessary.
        if let Some(size) = self.uniform {
            return size;
        }
        match self.recent_median() {
            Some(m) => m.clamp(LAZY_ITEM_ESTIMATED_HEIGHT * 0.5, LAZY_ITEM_ESTIMATED_HEIGHT),
            None => LAZY_ITEM_ESTIMATED_HEIGHT,
        }
    }

    /// Median of the last few measured heights, or `None` when nothing has been measured.
    pub(crate) fn recent_median(&self) -> Option<f32> {
        if self.recent.is_empty() {
            return None;
        }
        let mut v = self.recent.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(v[v.len() / 2])
    }

    /// Note a height for [`Self::coverage_height`]'s median. A ring, so the estimate follows the rows the
    /// list is actually showing instead of everything it has ever measured.
    pub(crate) fn note_recent(&mut self, h: f32) {
        const RECENT_KEPT: usize = 32;
        if !(h > 0.0) {
            return;
        }
        if self.recent.len() == RECENT_KEPT {
            self.recent.remove(0);
        }
        self.recent.push(h);
    }

    pub fn record(&mut self, index: usize, h: f32) {
        if self.heights.len() <= index { self.heights.resize(index + 1, 0.0); }
        self.heights[index] = h;
        self.note_recent(h);
    }

    /// Record a height by item key (writes the KEY view). The measure pass calls this alongside the
    /// index write, which is what keeps "identity → height" across data changes.
    pub fn record_keyed(&mut self, key: u64, h: f32) {
        self.keyed.insert(key, h);
    }

    /// Read a height by item key (`None` when unrecorded) — used before a rebase to ask whether an
    /// item already has a known height.
    pub fn height_keyed(&self, key: u64) -> Option<f32> {
        self.keyed.get(&key).copied()
    }

    /// Repair the cache after the data changed: migrate the heights of the items that still exist
    /// into the current index view (`keys[i]` is the current data's key for index `i`).
    ///
    /// - Same key: the height moves from `keyed` to `heights[new index]`, so it follows the item's
    ///   identity instead of being lost to the index shift.
    /// - No key record: `heights` stays 0 and the item is estimated or measured again.
    /// - Shorter data: heights past `keys.len()` are dropped.
    ///
    /// Idempotent: an item whose key did not change keeps its height; a changed key takes whatever
    /// `keyed` holds for the new key. Visible items are re-recorded by the next measure pass, so this
    /// only has to keep the OFF-SCREEN heights associated with the right items.
    pub fn rebase(&mut self, keys: &[u64]) {
        self.heights.clear();
        self.heights.resize(keys.len(), 0.0);
        for (i, &k) in keys.iter().enumerate() {
            if let Some(&h) = self.keyed.get(&k) {
                self.heights[i] = h;
            }
        }
        // Drop the key records of items that left the data (otherwise `keyed` grows without bound;
        // a rebuilt item takes a new key, so the stale record is worthless). The HashSet makes the
        // containment check O(1) — a linear scan per key would make this O(n²).
        let live: std::collections::HashSet<u64> = keys.iter().copied().collect();
        self.keyed.retain(|k, _| live.contains(k));
    }
}

/// The accumulated extent before `index` (heights plus spacing).
///
/// Uses `coverage_height` — the same function the window walk uses — because the conversions that trade
/// between an index and a pixel offset have to agree on what an unmeasured item is worth, or they do not
/// invert each other: `scroll_to_item` computes a target with this and the anchor is resolved with
/// `anchor_from_offset`, and a mismatch between the two makes a jump land somewhere else and costs an
/// extra compose+layout round to correct (`docs/frame-cost-probe-round.md` 4b). For MEASURED items the
/// two functions are identical, so this only changes what the estimate says about the rest.
pub(crate) fn prefix_height(cache: &ItemHeightCache, index: usize, spacing: f32) -> f32 {
    let mut acc = 0.0;
    for i in 0..index {
        acc += cache.coverage_height(i) + spacing;
    }
    acc
}

/// Resolve the first visible item (the anchor) from a pixel offset.
///
/// Unmeasured items are extrapolated with the estimate instead of stopping at the end of the
/// recorded range — truncating there made the anchor fall back to 0 once the scroll left the
/// measured window (measured: an empty cache returned (0,0) for offset 2000 and the list stopped
/// scrolling). The recorded part is scanned linearly; the rest is a single division, so a huge
/// offset costs O(1).
///
/// `coverage_height` for the same reason as [`prefix_height`]: this is the inverse of it, and the two are
/// only inverses if they price an unmeasured item the same way. The extrapolation past the recorded part
/// uses that same value as its step, for the same reason.
pub(crate) fn anchor_from_offset(cache: &ItemHeightCache, offset: f32, spacing: f32, before: f32) -> (usize, f32) {
    // Content coordinates include the `before` padding: item i's top is `before + prefix(i)`.
    let known = cache.heights.len();
    let mut acc = before;
    for i in 0..known {
        let h = cache.coverage_height(i) + spacing;
        if acc + h > offset {
            return (i, offset - acc);
        }
        acc += h;
    }
    // Past the recorded range: divide by the estimate to count the remaining items.
    let step = cache.coverage_height(known) + spacing;
    if step <= 0.0 { return (known.saturating_sub(1), 0.0); }
    let remaining = offset - acc;
    let extra = (remaining / step).floor() as usize;
    let idx = known + extra;
    (idx, remaining - extra as f32 * step)
}

/// The visible range from the anchor and the viewport extent (beyond-bounds window included).
pub(crate) fn visible_range(
    cache: &ItemHeightCache,
    first_index: usize,
    first_offset: f32,
    viewport_h: f32,
    spacing: f32,
    total: usize,
    beyond: usize,
) -> (usize, usize) {
    let start = first_index.saturating_sub(beyond);
    let mut end = first_index;
    let mut acc = first_offset;
    while end < total && acc < viewport_h {
        // `coverage_height`, not `height`: this walk decides WHICH items get composed, and over-stating an
        // unmeasured item makes the window come up short (see `ItemHeightCache::coverage_height`). The
        // anchor math (`prefix_height`, the offset conversion) keeps the plain estimate.
        acc += cache.coverage_height(end) + spacing;
        end += 1;
    }
    // `saturating_add`: `beyond_bounds` is a public knob, and `end + beyond` overflows (panicking in
    // debug) for a large enough value, which a caller tuning a huge prefetch would hit.
    let end = end.saturating_add(beyond).min(total);
    (start, end)
}

// ═══════════════════════════════════════════════════════
// build — register the visible items during composition
// ═══════════════════════════════════════════════════════

impl<A: LazyAxis> LazyList<A> {
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let state = match self.state {
            Some(s) => s,
            None => ctx.remember(|| LazyListState::new()).get(),
        };
        ctx.changed(&state.offset);
        // The derived anchor is a dependency: the policy writes the exact value after measuring, which
        // asks `build` to run again — the next frame recomputes from the real cache. When the estimate
        // already matched, nothing changes and no recomposition happens.
        ctx.changed(&state.first_visible_index);
        ctx.changed(&state.first_visible_offset);
        let key = ctx.next_key();

        // The interval registry, rebuilt so a data change is visible to this pass.
        let mut intervals = self.intervals;
        intervals.rebuild();
        let total = intervals.total();

        // Remembered across frames: the height cache, the viewport extent, the scrolling flag, the
        // fling limit.
        let cache = ctx.remember(ItemHeightCache::default);
        let viewport = ctx.remember(|| 600.0f32);
        let is_scrolling = state.is_scrolling.clone();
        let content_height = ctx.remember_backchannel(|| 0.0f32);
        let fling_limit = ctx.remember_backchannel(|| f32::MAX);
        let snap_paging = self.snap_paging;
        ctx.changed(&snap_paging);
        let fill_items = self.fill_items;
        ctx.changed(&fill_items);
        // The knobs added for a pager are parameters of this container too: a Clean slot compares
        // exactly what is declared here, so an undeclared one would silently keep the old behaviour
        // (`item_main_size` / `snap_offset` change where content lands, `beyond_bounds` how much of it
        // composes, `user_scroll` whether input reaches it).
        let item_main_size = self.item_main_size;
        ctx.changed(&item_main_size);
        let snap_offset = self.snap_offset;
        ctx.changed(&snap_offset);
        // The settle curve and the release threshold are parameters too: they reach the snap spec the
        // measure writes back, so a Clean slot that skipped them would keep the previous feel.
        let snap_positional_threshold = self.snap_positional_threshold;
        ctx.changed(&snap_positional_threshold);
        let snap_animation_spec = self.snap_animation_spec.clone();
        ctx.changed(&snap_animation_spec);
        let snap_max_pages = self.snap_max_pages;
        ctx.changed(&snap_max_pages);
        // The strategy has no value equality worth comparing; its IDENTITY is the parameter. Swapping
        // the object at one call site re-enters, handing the same one down does not.
        let fling_behavior_id = self
            .fling_behavior
            .as_ref()
            .map(|behavior| std::sync::Arc::as_ptr(behavior) as *const () as usize);
        ctx.changed(&fling_behavior_id);
        let beyond_bounds = self.beyond_bounds;
        ctx.changed(&beyond_bounds);
        let user_scroll = self.user_scroll;
        ctx.changed(&user_scroll);
        // Spacing, padding and the item count are parameters too: they decide where content lands and
        // how much of it exists. Only `snap_paging`/`fill_items` used to be declared, so changing the
        // spacing on a Clean slot kept the old placement forever.
        ctx.changed(&self.spacing);
        ctx.changed(&self.content_padding);
        ctx.changed(&self.cross_padding);
        ctx.changed(&total);
        ctx.changed(&self.reverse);
        // A signature of the data's key sequence: it changes when the total changes, or when the same
        // total is reordered or replaced. On a change the height cache migrates by item key to the
        // right index, so an index shift cannot leave heights on the wrong items.
        let data_sig = ctx.remember(|| 0u64);

        // Global index → item key for the current data (feeds the policy's key view and the migration).
        // A section without a key factory (`items_plain`) uses the index itself as the key, so replacing
        // the content of such a section is invisible — an inherent limitation, documented; sections
        // WITH keys (`items_from`, `item_keyed`) track identity exactly.
        let keys: Vec<u64> = (0..total).map(|g| intervals.key_of(g).unwrap_or(g as u64)).collect();
        let key_set: std::collections::HashSet<u64> = keys.iter().copied().collect();
        // The data signature (FNV-1a over the key sequence) detects a data change: a different total,
        // or the same total reordered or replaced (the key sequence changed). On a change the height
        // cache migrates by item key.
        let mut sig = 0xcbf29ce484222325u64;
        for &k in &keys { sig ^= k; sig = sig.wrapping_mul(0x100000001b3); }
        if data_sig.get() != sig {
            data_sig.set(sig);
            let mut c = cache.get();
            c.rebase(&keys);
            cache.set(c);
        }
        // The key sequence is a PARAMETER of this container, and it has to be declared: a same-count
        // replacement or reorder changes it, and the group must re-enter - `intervals.rebuild()` and the
        // height-cache rebase above live in the body, so a Skip would leave the rows and the cache on
        // the previous data. (The item closures themselves are registered inside the body and cannot be
        // compared, which is why it is the keys that are declared.) This became load-bearing only once
        // the `LazyScroll` element could compare equal: before that every lazy list re-entered each
        // frame by accident, and an undeclared data change was simply invisible.
        ctx.changed(&sig);

        // Key correction: after items were inserted or removed at the head, `last_known_first_key`
        // finds the item that used to be first visible.
        //
        // Only when the TOTAL changed (items added or removed): during ordinary scrolling the anchor
        // item changing is the result of the user's scroll and must never be corrected back (measured:
        // correcting every frame pulled the scroll position back to 0).
        //
        // And strictly AFTER the rebase: the offset calculation uses `prefix_height` over the height
        // cache MIGRATED BY KEY — before the rebase the index view still describes the old data, so the
        // prefix would be wrong and the offset misplaced.
        let known_total = state.known_total.clone();
        let total_changed = known_total.get() != total;
        if total_changed {
            known_total.set(total);
            if let Some(last_key) = state.last_known_first_key.get() {
                let cache_ref = cache.get();
                if let Some(new_index) = intervals.index_of_key(last_key) {
                    let new_offset = self.content_padding.0 + prefix_height(&cache_ref, new_index, self.spacing);
                    state.offset.set(new_offset);
                }
            }
        }

        let offset = state.offset.get();
        let cache_ref = cache.get();
        // The anchor: a jump request is authoritative (Compose composes from the requested index
        // directly, without going through the pixel conversion); otherwise it is recovered from the
        // pixel offset.
        let pad_before = self.content_padding.0;
        let (first_index, first_item_offset) = match state.jump_request.get() {
            Some((idx, off, _)) => {
                // A jump whose target lies PAST the end of the content is clamped by the measure, which is
                // the only side that knows `max_off` from real heights. A window composed at the
                // un-clamped position is then a window the measure moves out from under itself, and that
                // mismatch is what makes the frame of such a jump take a second compose+layout pass —
                // measured in `docs/frame-cost-probe-round.md` 4b (a jump to the end of a 1000-row list:
                // 2203 µs against 1000 for the same jump inside the range). Clamp here first, with the
                // `max_off` the last measure wrote, so the window opens where the measure will leave the
                // list.
                //
                // In-range jumps keep the exact requested anchor: the conversion only runs when the clamp
                // actually moved the target, which is what preserves the documented round-trip precision
                // (a jump to 500 lands at 500, not at whatever the estimate-based conversion returns).
                let last_max_off = fling_limit.get();
                // `0` (the content fits, nothing to clamp) and the not-yet-measured sentinel both take the
                // unclamped branch.
                if !(last_max_off > 0.0 && last_max_off < f32::MAX) {
                    (idx.min(total), off)
                } else {
                    let target = pad_before + prefix_height(&cache_ref, idx.min(total), self.spacing) + off;
                    let clamped = target.clamp(0.0, last_max_off);
                    if clamped == target {
                        (idx.min(total), off)
                    } else {
                        anchor_from_offset(&cache_ref, clamped, self.spacing, pad_before)
                    }
                }
            }
            None => {
                // The anchor index MUST be clamped into `0..total`: past the end the anchor
                // extrapolates with the estimate (far beyond the total), which would make
                // `visible_range` return end < start and the composition window overflow.
                let (fi, fo) = anchor_from_offset(&cache_ref, offset, self.spacing, pad_before);
                (fi.min(total.saturating_sub(1)), fo)
            }
        };
        // Record the anchor item's key (the next data change corrects against it). The exact anchor is
        // written back by the policy after the measure.
        if let Some(k) = intervals.key_of(first_index) {
            state.last_known_first_key.set(Some(k));
        }

        // Window extent during composition: the real viewport the previous measure wrote back, so
        // `build` sees the true size and composes enough items after a resize. (It used to be a fixed
        // 2000, and a viewport grown past that left the bottom of the list uncomposed.)
        //
        // No oscillation: under `fill_max_height` the viewport comes from the outer constraints and is
        // stable; under wrap-content the measure falls back to the cache when the axis is unbounded (it
        // does not write infinity), so reading it here is safe. The 1.0 floor guards against 0/negative.
        let viewport_h = viewport.get().max(1.0);
        let (mut start, end) = visible_range(
            &cache_ref, first_index, first_item_offset,
            viewport_h, self.spacing, total, self.beyond_bounds,
        );

        // Sticky-header walk-back (Compose keeps a pinned header in the window even when its natural
        // position is far above the viewport):
        // - pin  = the last sticky header whose C(i) <= offset — the first sticky above the anchor; the
        //   walk-back distance is the current section's length (no sticky list → skipped)
        // - prev = the sticky header above `pin`, included only while its bottom is still below the
        //   viewport top (C(prev) + h(prev) > offset, i.e. it is in the middle of being pushed out)
        let mut pin: Option<usize> = None;
        let mut prev_pin: Option<usize> = None;
        if intervals.has_sticky_headers() {
            let c_anchor = pad_before + prefix_height(&cache_ref, first_index, self.spacing);
            if intervals.is_sticky(first_index) && c_anchor <= offset + 0.01 {
                pin = Some(first_index);
            } else {
                let mut c = c_anchor;
                let mut i = first_index;
                while i > 0 {
                    i -= 1;
                    // `coverage_height`: these walks compare against `prefix_height` and the anchor's own
                    // content position, so they have to price an unmeasured item the same way it does.
                    c -= cache_ref.coverage_height(i) + self.spacing;
                    if intervals.is_sticky(i) && c <= offset + 0.01 {
                        pin = Some(i);
                        break;
                    }
                }
            }
            if let Some(p) = pin {
                // prev: the nearest sticky above `pin`; walk up until its bottom is at or above the
                // viewport top (anything further up is not visible).
                let mut c2 = pad_before + prefix_height(&cache_ref, p, self.spacing);
                let mut j = p;
                while j > 0 {
                    j -= 1;
                    c2 -= cache_ref.coverage_height(j) + self.spacing;
                    if intervals.is_sticky(j) {
                        if c2 + cache_ref.coverage_height(j) > offset {
                            prev_pin = Some(j);
                        }
                        break;
                    }
                }
            }
        }
        if let Some(p) = pin {
            start = start.min(p);
        }
        if let Some(pp) = prev_pin {
            start = start.min(pp);
        }

        // Install the custom measure policy: real measurement, height write-back, placement, and the
        // viewport write-back.
        let scroll = crate::modifier::ScrollState {
            offset: state.offset.clone(),
            is_scroll_in_progress: is_scrolling.clone(),
            fling_limit: fling_limit.clone(),
            // Pulse wired through: `LazyListState.scroll_pulse` is stable across frames (same lifetime
            // as `offset`), and the assembled state clones it, so the dispatch layer increments the same
            // State and a scrollbar's pulse lights up for a lazy list too.
            scroll_pulse: state.scroll_pulse.clone(),
            snap: state.snap.clone(),
            // Keep the in-flight page target with the list, not with this per-build adapter.
            step_goal: state.step_goal.clone(),
            fling_behavior: self.fling_behavior.clone(),
        };
        // Registration order: ordinary items first, sticky headers last (a child's paint order is its
        // registration order, so the sticky one paints on top and covers the content sliding under it).
        // `globals[i]` is the global index of child `i`, which the policy places and writes back with —
        // a positional `start + i` mapping no longer holds.
        let mut globals: Vec<usize> = Vec::with_capacity(end - start);
        let mut sticky_children: Vec<usize> = Vec::new();
        for g in start..end {
            if !intervals.is_sticky(g) {
                globals.push(g);
            }
        }
        for g in start..end {
            if intervals.is_sticky(g) {
                sticky_children.push(globals.len());
                globals.push(g);
            }
        }
        // The direction the list lays out under. A `layout_direction` on the modifier wins over the
        // ambient local, exactly as `Row`/`Column` resolve it (`layout/components.rs`).
        let rtl = self
            .modifier
            .get_layout_direction()
            .unwrap_or_else(crate::layout::direction::current)
            == crate::layout::LayoutDirection::Rtl;
        // What the CONTENT coordinates are mirrored by. `reverseLayout` flips the main axis; on a
        // HORIZONTAL list RTL flips it too, because Compose's LazyRow starts at the right edge in RTL.
        // The policy mirrors the item positions with it and the scroll modifier mirrors the translate
        // with it, exactly as they already do for `reverseLayout` alone — the two have to agree, or the
        // content is drawn shifted by one viewport.
        let mirror = self.reverse ^ (!A::VERTICAL && rtl);
        let policy = LazyListPolicy::<A> {
            axis: PhantomData,
            cache: cache.clone(),
            viewport: viewport.clone(),
            content_height: content_height.clone(),
            fling_limit: fling_limit.clone(),
            is_scroll_in_progress: is_scrolling.clone(),
            spacing: self.spacing,
            total,
            content_padding: self.content_padding,
            cross_padding: self.cross_padding,
            globals,
            keys,
            key_set,
            sticky_children,
            pin,
            reverse: mirror,
            rtl,
            reverse_requested: self.reverse,
            layout_info: state.layout_info.clone(),
            snap_paging,
            fill_items,
            item_main_size: self.item_main_size,
            snap_offset: self.snap_offset,
            snap_positional_threshold: self.snap_positional_threshold,
            snap_animation_spec: self.snap_animation_spec.clone(),
            snap_max_pages: self.snap_max_pages,
            beyond_bounds: self.beyond_bounds,
            state: state.clone(),
        };
        // `userScroll(false)` drops the wheel/drag channel but keeps the offset itself, so
        // `scroll_to_page` and an external animation still move the list.
        let user_scroll = self.user_scroll;
        let mut base = Modifier::new().fill_max_width().fill_max_height();
        if user_scroll {
            base = base.then(A::scroll(scroll));
        }
        let m = base
            .lazy_scroll(content_height.clone())
            // The engine's mirror flag for a scroll container: `reverseLayout` flips the main axis,
            // and on a HORIZONTAL list RTL does too (Compose's LazyRow starts at the right in RTL),
            // so the flag is the XOR of the two. A vertical list ignores the direction here — its
            // cross axis is mirrored in the policy instead.
            .lazy_scroll_reverse(mirror);
        let m = m
            .then(self.modifier)
            .then(Modifier::new().with_item_animation_host(state.item_animations.clone()))
            // Accessibility: the list is a COLLECTION of `total` items, and each item below declares
            // its own position in it. Without both, a reader walks the visible rows as an anonymous
            // stack instead of announcing "item 3 of 200" (Compose's `LazyListSemantics`).
            .then(Modifier::new().semantics(
                crate::semantics::SemanticsConfig::new().collection_info(if A::VERTICAL {
                    crate::semantics::CollectionInfo::rows(total)
                } else {
                    crate::semantics::CollectionInfo::columns(total)
                }),
            ));
        // The registration order, taken out before the policy moves into the group (unreadable after).
        let child_globals = policy.globals.clone();
        // The item children MUST be registered AFTER `start_restartable_group` (so they attach under
        // the LazyColumn node) — composition order decides the parent/child relation.
        match ctx.start_restartable_group(key, m, policy) {
            GroupStatus::Skip => {
                // Skip: the previous frame's subtree is reused — but the visible range may have
                // changed and the child count cannot be edited in place, so the next frame's Enter does
                // it. A Skip only happens when every dependency is unchanged (a moved offset enters),
                // so this is safe.
            }
            GroupStatus::Enter => {
                // Register the visible item children in `globals` order (sticky last). The slot key
                // mixes in the item key, so a node is reused or recycled across frames by identity.
                const MIX: u64 = 0x9E37_79B9_7F4A_7C15;
                for g in child_globals {
                    let item_key = intervals.key_of(g).unwrap_or(g as u64);
                    let ik = ctx.next_key() ^ (item_key.wrapping_mul(MIX));
                    if let Some((iv_idx, _local)) = intervals.locate(g) {
                        let content = intervals.intervals[iv_idx].content.clone();
                        // The item's position in the collection — a reader says "3 of 200" from these
                        // two declarations (the count on the list node above, the index here).
                        let item_modifier = Modifier::new().semantics(
                            crate::semantics::SemanticsConfig::new().collection_item_info(
                                crate::semantics::CollectionItemInfo::new(g),
                            ),
                        );
                        match ctx.start_restartable_group(ik, item_modifier, BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                content(ctx, g);
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
            }
        }
        ctx.end_restartable_group();
    }
}

