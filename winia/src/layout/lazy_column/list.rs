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

/// 垂直懒列表（对标 Compose `LazyColumn`）
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
        self.content_padding = (before, after);
        self
    }

    /// Cross-axis content padding (start/end for a vertical list): shrinks the width an item may use
    /// and places items from `before` (the cross axis of Compose's `contentPadding`).
    pub fn content_padding_cross(mut self, before: f32, after: f32) -> Self {
        self.cross_padding = (before, after);
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
        self.snap_positional_threshold = threshold;
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
        self.item_main_size = Some(size);
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
        self.snap_offset = offset;
        self
    }

    /// Spacing between items, along the main axis
    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
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

    /// 带 key 的单一项
    pub fn item_keyed(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c, false);
        self
    }

    /// count 个项 + key 工厂（对标 `items(count, key: (index) -> Any, itemContent)`）
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

    /// count 个无 key 项（位置即 key）
    pub fn items_plain(
        mut self,
        count: usize,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        let c = Arc::new(content);
        self.intervals.add(count, None, c, false);
        self
    }

    /// 从数据列表迭代（对标 `items(items, key: (item) -> Any, itemContent)`）——
    /// **用户要求的"稳定 key 迭代"入口**：key 从数据 id 提取，列表前部增删
    /// 后滚动位置按 key 保持。
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

    /// sticky header 区间（对标 Compose `stickyHeader(key, content)`）——
    /// 滚动时钉在视口主轴起始端，内容从它下面滑过；下一个 sticky header
    /// 到来时把前一个推上去。key 必须全列表唯一。
    pub fn sticky_header(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c, true);
        self
    }
}

// ═══════════════════════════════════════════════════════
// 高度缓存 + 可见范围计算
// ═══════════════════════════════════════════════════════

/// 每项高度缓存（全局 index → 测量高度）；未测项用预估
///
/// 公开：可用于外部读取实测高度（如自定义滚动逻辑）；`scroll_to_item`
/// 不需要它（锚点权威，测量期自动用缓存推导像素）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemHeightCache {
    /// index 视图（index → 高度）：供连续扫描（prefix/anchor/visible_range）
    /// 与越界预估使用；数据前部增删/重排后此视图会错位，由 [`ItemHeightCache::rebase`] 校正。
    pub heights: Vec<f32>,
    /// key 视图（item key → 高度）：跨数据变化保持"项身份 → 高度"——
    /// 数据增删/重排后，同一 key 的项高度不因 index 平移而丢失。私有，
    /// 由 `record_keyed` 写入、`rebase` 读回迁移到 index 视图。
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
        // 未测（缺失或 0）→ 预估；记录过的 >0 高度直接返回
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

    /// 按 item key 记录高度（写入 key 视图）。测量侧在写 index 视图的同时
    /// 调用，使跨数据变化时项身份 → 高度 的关系得以保留。
    pub fn record_keyed(&mut self, key: u64, h: f32) {
        self.keyed.insert(key, h);
    }

    /// 按 item key 读高度（无记录 → None）。跨数据变化时用于在迁移前确认
    /// 某项是否有已知高度。
    pub fn height_keyed(&self, key: u64) -> Option<f32> {
        self.keyed.get(&key).copied()
    }

    /// 数据变化后的校正：把 key 视图中仍存在的项高度迁移到当前 index 视图
    /// 的正确位置（`keys[i]` = 当前数据第 i 项的 key）。
    ///
    /// - 同 key 项：从 `keyed` 迁移到 `heights[新 index]`（数据增删/重排后
    ///   高度跟随项身份，不因位置漂移而丢失）。
    /// - 无 key 记录的项：`heights` 置 0 → 走预估/重测。
    /// - 数据变短：清空超出 `keys.len()` 的残留高度。
    ///
    /// 幂等：key 没变的项高度保持，key 变化的项由新 key 的 `keyed` 决定。
    /// 可见项随后由测量重新填充（Enter 路径 record 覆盖），此处只保证
    /// 不可见项的缓存高度在数据变化后仍与正确项对应。
    pub fn rebase(&mut self, keys: &[u64]) {
        self.heights.clear();
        self.heights.resize(keys.len(), 0.0);
        for (i, &k) in keys.iter().enumerate() {
            if let Some(&h) = self.keyed.get(&k) {
                self.heights[i] = h;
            }
        }
        // 清理已从数据中消失的 key 记录（防 keyed 无限膨胀；项后续可能
        // 复用新 key 重建——旧记录无意义）。用 HashSet 加速 contain 查询，
        // 避免 `keys.contains` 对每个 keyed 键做 O(n) 线性扫描（O(n²)）。
        let live: std::collections::HashSet<u64> = keys.iter().copied().collect();
        self.keyed.retain(|k, _| live.contains(k));
    }
}

/// 累计高度：index 之前各项高度和（含间距）
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

/// 由像素偏移定位第一个可见项（锚点）
///
/// 未测项按预估高度继续推算（而非在已知范围末尾截断）——否则滚动超出已测范围时
/// 锚点错误退回 0（实测：offset=2000 空缓存时恒返回 (0,0)，滚动失效）。
/// 已知高度部分线性扫描，超出部分用除法直接估算（O(1) 防大 offset 循环）。
///
/// `coverage_height` for the same reason as [`prefix_height`]: this is the inverse of it, and the two are
/// only inverses if they price an unmeasured item the same way. The extrapolation past the recorded part
/// uses that same value as its step, for the same reason.
pub(crate) fn anchor_from_offset(cache: &ItemHeightCache, offset: f32, spacing: f32, before: f32) -> (usize, f32) {
    // 内容坐标含 before padding：项 i 顶 = before + prefix(i)
    let known = cache.heights.len();
    let mut acc = before;
    for i in 0..known {
        let h = cache.coverage_height(i) + spacing;
        if acc + h > offset {
            return (i, offset - acc);
        }
        acc += h;
    }
    // 超出已知范围：按预估高度除法估算剩余项数
    let step = cache.coverage_height(known) + spacing;
    if step <= 0.0 { return (known.saturating_sub(1), 0.0); }
    let remaining = offset - acc;
    let extra = (remaining / step).floor() as usize;
    let idx = known + extra;
    (idx, remaining - extra as f32 * step)
}

/// 由锚点 + 视口高计算可见范围（含预取窗）
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
    let end = (end + beyond).min(total);
    (start, end)
}

// ═══════════════════════════════════════════════════════
// build — 组合期注册可见项
// ═══════════════════════════════════════════════════════

impl<A: LazyAxis> LazyList<A> {
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let state = match self.state {
            Some(s) => s,
            None => ctx.remember(|| LazyListState::new()).get(),
        };
        ctx.changed(&state.offset);
        // 派生锚点依赖：policy 测量后 set 精确值 → 触发重组收敛（build 用预估高度算的
        // 锚点与真实值不同时，下一帧以真实缓存重算；相同则无变化不重组）
        ctx.changed(&state.first_visible_index);
        ctx.changed(&state.first_visible_offset);
        let key = ctx.next_key();

        // 内容注册表（重建——数据变化反映）
        let mut intervals = self.intervals;
        intervals.rebuild();
        let total = intervals.total();

        // 跨帧 remember：高度缓存 / 视口高 / 滚动中标记 / fling 极限
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
        // 数据 key 序列签名（方案 A：检测数据变化——total 变或同 total 重排/
        // 替换。签名变化 → 高度缓存按 item key 迁移到正确 index，避免 index
        // 平移导致旧高度错位）
        let data_sig = ctx.remember(|| 0u64);

        // 当前数据全局 index → item key（供 policy 写 key 视图 + 数据变化迁移）。
        // 无 key 工厂的段（items_plain）用索引自身作 key——同 key 段内容替换
        // 时无法区分（固有局限，见文档）；有 key 段（items_from/item_keyed）
        // 身份精确追踪。
        let keys: Vec<u64> = (0..total).map(|g| intervals.key_of(g).unwrap_or(g as u64)).collect();
        let key_set: std::collections::HashSet<u64> = keys.iter().copied().collect();
        // 数据签名（FNV-1a 折叠 key 序列）——检测数据变化：total 变或同 total
        // 重排/替换（key 序列变）。变化时按 item key 迁移高度缓存。
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

        // key 校正：数据前部增删后，用 last_known_first_key 找回原 first visible 项。
        // ⚠ 仅当 total 变化（数据增删）时校正——正常滚动时锚点项变化是用户滚动
        // 的结果，绝不能校正回原位置（实测：每帧校正会把滚动拉回 0）。
        // ⚠ 必须在 rebase 之后：offset 校正用 prefix_height 依赖**按 key 迁移后**
        // 的高度缓存（rebase 前 index 视图还是旧数据，prefix 算错 → offset 错位）。
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
        // 锚点：跳转请求权威（对齐 Compose——requestPosition 后直接从请求的
        // index 开始组合，不经过像素反推）；否则由像素 offset 反推
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
                // ⚠ 锚点 index 必须 clamp 到 total 内：offset 超界时 anchor 按预估
                // 高度外推（可远超 total）→ visible_range 的 end<start → 组合窗溢出
                let (fi, fo) = anchor_from_offset(&cache_ref, offset, self.spacing, pad_before);
                (fi.min(total.saturating_sub(1)), fo)
            }
        };
        // 记录锚点项 key（供下次数据变化校正）——派生锚点由测量期 policy 写回精确值
        if let Some(k) = intervals.key_of(first_index) {
            state.last_known_first_key.set(Some(k));
        }

        // 组合期窗口高度：读上一帧 measure 回写的真实视口（方向一——build 感知
        // 真实视口，resize 后按新视口组合足够项）。此前用固定 2000，resize 放大
        // 超过 2000 时 build 组合不足、底部可见项缺失。
        // ⚠ 约束振荡：fill_max_height 时 viewport 由外部约束决定（稳定，不振荡）；
        // wrap-content 时 measure 用 `< f32::MAX` 判定回退缓存（不写 ∞），build 读
        // 缓存稳定。故 build 读 viewport 安全。下限 1.0 防 0/负值。
        let viewport_h = viewport.get().max(1.0);
        let (mut start, end) = visible_range(
            &cache_ref, first_index, first_item_offset,
            viewport_h, self.spacing, total, self.beyond_bounds,
        );

        // sticky header 回溯（对齐 Compose：钉住的 header 即使自然位置远在视口
        // 上方也必须留在窗口内）：
        // - pin  = 最后一个 C(i) ≤ offset 的 sticky header（锚点上方第一个 sticky，
        //   深滚动时回溯距离 = 当前 section 长度；无 sticky 列表时跳过）
        // - prev = pin 上方一个 sticky header，仅当其底部仍低于视口顶
        //   （C(prev)+h(prev) > offset，即正处于被推出的过渡期）才纳入窗口
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
                // prev：pin 上方最近的 sticky；走到底部 ≤ 视口顶即停（更上方不可见）
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

        // 挂自定义测量策略（真实测量 + 写回高度 + 放置 + 视口回写）
        let scroll = crate::modifier::ScrollState {
            offset: state.offset.clone(),
            is_scroll_in_progress: is_scrolling.clone(),
            fling_limit: fling_limit.clone(),
            // pulse 接通：LazyListState.scroll_pulse 跨帧稳定（与 offset 同
            // 生命周期），拼装 clone 进去——分发层自增落在同一 State 上，
            // scrollbar 侧 pulse 点亮对 lazy 同样生效。
            scroll_pulse: state.scroll_pulse.clone(),
            snap: state.snap.clone(),
            // Keep the in-flight page target with the list, not with this per-build adapter.
            step_goal: state.step_goal.clone(),
            fling_behavior: self.fling_behavior.clone(),
        };
        // 注册顺序：普通项在前、sticky header 在后（子节点渲染顺序 = 注册顺序，
        // 后者画在最上层——钉住的 header 需盖住从它下面滑过的内容）。
        // globals[i] = 第 i 个子节点的全局 index（policy 放置/写回用，不能再
        // 用 start+i 位置映射）
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
            reverse: self.reverse,
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
            .lazy_scroll_reverse(self.reverse);
        let m = m
            .then(self.modifier)
            .then(Modifier::new().with_item_animation_host(state.item_animations.clone()));
        // 注册顺序（policy 移动进 group 后不可再读——先取出）
        let child_globals = policy.globals.clone();
        // ⚠ item 子节点必须在 start_restartable_group **之后**注册（挂到
        // LazyColumn 节点下）——组合顺序决定父子关系
        match ctx.start_restartable_group(key, m, policy) {
            GroupStatus::Skip => {
                // Skip：复用上帧子树——但可见范围可能变了，无法原地改子节点数；
                // 依赖下一帧 Enter。winia Skip 仅当所有依赖未变时命中（offset 变了
                // 会 Enter），此处安全。
            }
            GroupStatus::Enter => {
                // 注册可见项子节点（按 globals 顺序——sticky 在后）：slot key
                // 混合 item key（跨帧复用/回收）
                const MIX: u64 = 0x9E37_79B9_7F4A_7C15;
                for g in child_globals {
                    let item_key = intervals.key_of(g).unwrap_or(g as u64);
                    let ik = ctx.next_key() ^ (item_key.wrapping_mul(MIX));
                    if let Some((iv_idx, _local)) = intervals.locate(g) {
                        let content = intervals.intervals[iv_idx].content.clone();
                        match ctx.start_restartable_group(ik, Modifier::new(), BoxLayout::new()) {
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

