//! LazyColumn / LazyRow 组件 — 对标 Compose foundation lazy `LazyColumn` / `LazyRow`
//!
//! 核心机制（对齐 Compose lazy 架构）：
//! - **锚点滚动模型**：`LazyListState.first_visible_index + first_visible_offset`
//!   而非像素 offset——滚动位置由"第一个可见项的索引 + 其偏移"表达；
//! - **稳定 key**：`items()` 接受 `key` 工厂（数据 id 等稳定值）。插入/删除
//!   列表前部项后，按 key 找回原 firstVisibleItem 保持滚动位置（Compose
//!   `LazyListScrollPosition.updateScrollPositionIfTheFirstItemWasMoved` 语义）；
//! - **区间内容**：`item/items` 注册为 IntervalList（count + key 工厂 + content
//!   闭包），全局 index 经区间定位转局部 index（Compose `LazyListIntervalContent`）；
//! - **懒测量**：组合期按高度缓存预估可见范围 [start, end]，只注册/组合可见项；
//!   测量期真实排布并把高度写回缓存（未缓存项用默认高度预估，偏差触发补注册）。
//!
//! ⚠ 与 Compose 差异：winia 组合为命令式（build 直接注册节点），无 Compose 的
//! LazyLayout 测量期组合——这里用"组合期预估 + 测量期校正"两阶段模型。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::layout::constraints::Constraints;
use crate::modifier::Modifier;
use std::marker::PhantomData;
use std::sync::Arc;

/// 未测量项的预估高度（首次进入视口时用，测量后写回缓存）
pub const LAZY_ITEM_ESTIMATED_HEIGHT: f32 = 48.0;
/// 超出视口后仍注册的额外项数（上下各预取，减少滚动时补注册抖动）
pub const LAZY_BEYOND_BOUNDS: usize = 4;

/// The lazy list's main axis. Sealed, like [`Axis`](super::axis::Axis): the two markers it carries are
/// the whole set.
///
/// The questions both container families ask — main and cross extents, `size`, `point` — are on
/// [`Axis`](super::axis::Axis); this adds what only a lazy list needs.
pub trait LazyAxis: super::axis::Axis {
    /// A child's constraints: the cross axis inherits the parent's max, the main axis is unbounded
    /// (wrap content).
    #[doc(hidden)]
    fn child_constraints(c: Constraints) -> Constraints;
    /// Replace the cross-axis max (`contentPadding` shrinks the item's usable width).
    #[doc(hidden)]
    fn with_cross_max(c: Constraints, v: f32) -> Constraints;
    /// Pin the main axis's min AND max to `v` — what `fillParentMaxWidth` / `fillParentMaxHeight` do.
    ///
    /// Off by default: the unbounded main axis is what lets an item wrap to its content (see
    /// [`Self::child_constraints`]). A PAGED list wants the opposite — each item exactly fills the
    /// viewport, which is what makes a snap position an item boundary.
    #[doc(hidden)]
    fn with_main_exact(c: Constraints, v: f32) -> Constraints;
    /// The scroll modifier, per axis (vertical / horizontal)
    #[doc(hidden)]
    fn scroll(state: crate::modifier::ScrollState) -> Modifier;
    /// Whether this axis scrolls vertically — the direction an item's placement animates in.
    #[doc(hidden)]
    const VERTICAL: bool;
}

impl LazyAxis for crate::layout::axis::VerticalAxis {
    const VERTICAL: bool = true;
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: f32::MAX }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: v, min_height: 0.0, max_height: c.max_height }
    }
    fn with_main_exact(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: c.min_width, max_width: c.max_width, min_height: v, max_height: v }
    }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().vertical_scroll(state)
    }
}

impl LazyAxis for crate::layout::axis::HorizontalAxis {
    const VERTICAL: bool = false;
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: f32::MAX, min_height: 0.0, max_height: c.max_height }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: v }
    }
    fn with_main_exact(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: v, max_width: v, min_height: c.min_height, max_height: c.max_height }
    }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().horizontal_scroll(state)
    }
}

/// Per-item placement animations, keyed by the item key. Shared by every layout of the list.
#[derive(Debug, Default)]
pub(crate) struct ItemAnimations {
    /// Incremented once per `place`; an item seen on the PREVIOUS frame animates its move, an item
    /// that was away resets instead of springing from a stale slot.
    pub frame: u64,
    pub items: std::collections::HashMap<u64, ItemAnimation>,
}

/// One animated item's current paint offset and the slot it occupied last frame.
#[derive(Debug)]
pub(crate) struct ItemAnimation {
    pub offset: crate::runtime::state::State<f32>,
    pub last_main: f32,
    pub last_frame: u64,
}

/// How many placement passes a key survives after it leaves the window before its animation state
/// is dropped. The map is keyed by item key, so a feed of ever-new keys would otherwise grow without
/// bound; an item that comes back after this window starts from a fresh slot anyway.
const ITEM_ANIMATION_RETENTION_FRAMES: u64 = 240;

fn retain_recent_items(anims: &mut ItemAnimations, frame: u64) {
    anims
        .items
        .retain(|_, item| frame.saturating_sub(item.last_frame) <= ITEM_ANIMATION_RETENTION_FRAMES);
}

/// Lazy 列表滚动状态（winia 像素模型 + 派生锚点）
///
/// 滚动输入走 winia 现有通道：`ScrollState.offset`（像素总偏移，挂 vertical_scroll）。
/// `first_visible_index/offset` 是派生锚点（对齐 Compose 的锚点模型语义，供外部读取）；
/// `last_known_first_key` 记录第一个可见项 key——数据前部增删后按 key 校正偏移。
#[derive(Debug, Clone)]
pub struct LazyListState {
    /// 像素滚动偏移（挂 vertical_scroll modifier——滚动输入通道）
    pub offset: crate::runtime::state::State<f32>,
    /// 最近已知的第一个可见项 key（数据变化后按 key 校正位置）
    pub(crate) last_known_first_key: crate::runtime::state::State<Option<u64>>,
    /// 上次 build 见到的 total——**状态级**守卫（非组合级 remember）：
    /// 外部持有 state 跨 Composer 复用时，组合级 remember 会每帧误触发 key 校正
    /// （实测：第二次 render 把 offset=2000 拉回 0）。放这里与 LazyListState
    /// 同生命周期，只有数据真的变化（total 变）才校正。
    pub(crate) known_total: crate::runtime::state::State<usize>,
    /// 程序化跳转请求 (index, offset-in-item, animate)——锚点权威（对齐 Compose
    /// `requestPositionAndForgetLastKnownKey`：scroll position 就是锚点，
    /// 测量从锚点开始组合；像素 offset 由测量期从缓存推导，不做反推）。
    /// 首次测量消费后清空。animate = 动画滚动（spring，对齐 animateScrollToItem）。
    pub(crate) jump_request: crate::runtime::state::State<Option<(usize, f32, bool)>>,
    /// How far a fling may travel: written back during measure as `content height - viewport height`,
    /// or `f32::MAX` while that is still unknown. A REAL `0` means the content fits and the list cannot
    /// move — see `ScrollState::fling_limit`, which had the same "0 means unknown" conflation.
    ///
    /// 同 crate 的 scrollbar.rs 可见（scrollbar 侧读，content = limit + viewport）。
    pub(crate) fling_limit: crate::runtime::state::Backchannel<f32>,
    /// The viewport's main-axis size from the last measure: what a pager needs to turn a pixel offset
    /// into a page fraction. `0.0` until the first measure.
    ///
    /// A `State`, not a backchannel: the first measure is where a pager learns its page size, and the
    /// pager's own build does not run again just because the viewport appeared - a derived value that
    /// reads this has to be woken by the write.
    pub(crate) viewport_main: crate::runtime::state::State<f32>,
    /// 滚动活动脉冲（P1-3：边界滚轮点亮用——与 ScrollState.scroll_pulse 同语义；
    /// lazy 的 ScrollState 是 build 期拼装（offset/is_scrolling/fling_limit 三
    /// clone），pulse 必须挂在这里才跨帧稳定；拼装时 clone 进去；同 crate
    /// 的 scrollbar.rs 可见）。
    pub(crate) scroll_pulse: crate::runtime::state::State<u64>,
    /// Whether a drag or a fling is in progress — Compose's `LazyListState.isScrollInProgress`. This is
    /// not a flag of its own: `build` hands this same state to the `ScrollState` the list assembles as
    /// its `is_scroll_in_progress` (`:983`, `:1014`), so it is whatever the drag and fling paths last
    /// wrote. The getter exists because a caller has to be able to ASK, which is Compose's
    /// `!isScrollInProgress` guard on `LaunchedEffect(monthIndex)` (`DatePicker.kt:1548-1553`) — without
    /// it a sync that scrolls the list cancels the gesture that was already moving it.
    pub(crate) is_scrolling: crate::runtime::state::State<bool>,
    /// The paging snap configuration, written back by the measure pass when `snap_paging` is on and
    /// every measured item fills the viewport; `None` means no snapping. Lives beside `fling_limit` for
    /// the same reason — the ScrollState `build` assembles is shared with the measure policy, and this
    /// is the one that tells a finished fling where to land.
    pub(crate) snap: crate::runtime::state::Backchannel<Option<crate::fling::SnapInfo>>,
    /// The in-flight wheel page target, shared by every ScrollState assembled for this list.
    /// Recomposition must not erase it while the spring is still moving.
    pub(crate) step_goal: std::sync::Arc<std::sync::Mutex<Option<f32>>>,
    /// Per-item placement animations (keyed by item key), shared with the measure policy.
    pub(crate) item_animations: std::sync::Arc<std::sync::Mutex<ItemAnimations>>,
    /// 派生：第一个可见项索引（每次 build 后更新）
    pub first_visible_index: crate::runtime::state::State<usize>,
    /// 派生：第一个可见项的偏移（正 = 该项向上滚出多少）
    pub first_visible_offset: crate::runtime::state::State<f32>,
}

impl LazyListState {
    pub fn new() -> Self {
        Self {
            offset: crate::runtime::state::State::new(0.0),
            last_known_first_key: crate::runtime::state::State::new(None),
            known_total: crate::runtime::state::State::new(usize::MAX),
            jump_request: crate::runtime::state::State::new(None),
            fling_limit: crate::runtime::state::Backchannel::new(f32::MAX),
            viewport_main: crate::runtime::state::State::new(0.0),
            scroll_pulse: crate::runtime::state::State::new(0),
            snap: crate::runtime::state::Backchannel::new(None),
            step_goal: std::sync::Arc::new(std::sync::Mutex::new(None)),
            item_animations: std::sync::Arc::new(std::sync::Mutex::new(ItemAnimations::default())),
            is_scrolling: crate::runtime::state::State::new(false),
            first_visible_index: crate::runtime::state::State::new(0),
            first_visible_offset: crate::runtime::state::State::new(0.0),
        }
    }

    /// 当前第一个可见项索引
    pub fn first_visible(&self) -> usize { self.first_visible_index.get() }

    /// 当前第一个可见项偏移
    pub fn offset(&self) -> f32 { self.offset.get() }

    /// How far the first visible item has been scrolled past, in pixels (`0.0` = its leading edge is
    /// at the viewport edge).
    pub fn first_visible_offset(&self) -> f32 { self.first_visible_offset.get() }

    /// The viewport size along the main axis, as of the last measure (`0.0` before the first one).
    ///
    /// A tracked read: a caller deriving from it (a pager's page size) is woken when the first measure
    /// resolves it, which a `peek` would miss - the pager's build does not re-run by itself.
    pub fn viewport_main_size(&self) -> f32 { self.viewport_main.get() }

    /// Whether there is content past the last visible item (Compose
    /// `LazyListState.canScrollForward`). Asked from the pixel offset against the measured limit, so
    /// it answers what a scrollbar would: is the list already resting at its end?
    ///
    /// ⚠ The sentinel is `f32::MAX`, not infinity — see the same warning on `fling_limit` at measure
    /// time — so `is_finite()` is TRUE before the first measure and the naive test answered "yes, you
    /// can scroll" on a list nobody has laid out yet. Compose's `canScrollForward` starts at `false`
    /// (`LazyListState.kt:474`).
    pub fn can_scroll_forward(&self) -> bool {
        let limit = self.fling_limit.peek();
        limit < f32::MAX && self.offset.get() < limit - 0.5
    }

    /// Whether there is content before the first visible item (Compose
    /// `LazyListState.canScrollBackward`).
    pub fn can_scroll_backward(&self) -> bool { self.offset.get() > 0.5 }

    /// [`can_scroll_forward`](Self::can_scroll_forward) as a `State` that notifies only when the
    /// ANSWER changes.
    ///
    /// Calling `can_scroll_forward()` during composition makes the caller recompose on every scroll
    /// pixel — a chevron that only cares "am I at the end" is rebuilt 60 times a second to be told
    /// "no" again. Reading this handle recomposes it when the list reaches its end and when it leaves
    /// it, which is the only information the caller actually uses.
    #[crate::composable]
    pub fn remember_can_scroll_forward(
        &self,
        ctx: &mut crate::runtime::composer::ComposeCtx,
    ) -> crate::runtime::state::State<bool> {
        let source = self.clone();
        crate::runtime::flow::remember_derived_state(ctx, move || source.can_scroll_forward())
    }

    /// [`can_scroll_backward`](Self::can_scroll_backward) as a `State` that notifies only on a flip.
    #[crate::composable]
    pub fn remember_can_scroll_backward(
        &self,
        ctx: &mut crate::runtime::composer::ComposeCtx,
    ) -> crate::runtime::state::State<bool> {
        let source = self.clone();
        crate::runtime::flow::remember_derived_state(ctx, move || source.can_scroll_backward())
    }
    /// Whether a drag or a fling is in progress — Compose's `LazyListState.isScrollInProgress`, which
    /// callers use to stay off a list that is already moving.
    pub fn is_scrolling(&self) -> bool { self.is_scrolling.get() }

    /// 立即滚动到指定索引（项顶部对齐视口顶部，可带偏移）。
    ///
    /// 签名对齐 Compose `scrollToItem(index, scrollOffset = 0)`：不需要高度
    /// 缓存/间距——跳转请求由测量期消费：先按锚点组合窗口，再用写回后的高度
    /// 缓存推导像素 offset（与放置/锚点解析共用同一 prefix 函数，round-trip
    /// 精确——不会出现预估 48 vs 实测 47.5 的累积偏差导致落点漂移）。
    /// 越界 clamp 在 measure 期（本方法不知道 total）。
    pub fn scroll_to_item(&self, index: usize, scroll_offset: f32) {
        self.jump_request.set(Some((index, scroll_offset, false)));
    }

    /// 动画滚动到指定索引（对齐 Compose `animateScrollToItem(index, scrollOffset)`）。
    /// 与 `scroll_to_item` 同一锚点权威：请求由测量期消费并推导像素目标，
    /// 用 Spring 动画（默认阻尼 1.0 / 刚度 200，收敛 ~300-400ms）从当前 offset
    /// 平滑滚动到目标；目标 clamp 到 [0, max_offset]。
    pub fn animate_scroll_to_item(&self, index: usize, scroll_offset: f32) {
        self.jump_request.set(Some((index, scroll_offset, true)));
    }

    /// Start a fling on this list from code (the gesture path calls
    /// `ScrollState::fling_with_boundary` on the state the list assembles and does not come here).
    ///
    /// It runs the SAME strategy path, so a paging list settles on a page boundary instead of decaying
    /// across one, and the plan-based velocity floor applies. A custom `fling_behavior` is a component
    /// parameter and is not reachable from this entry point - it runs the built-in strategy.
    ///
    /// Unlike the old decay-only version, this now writes `is_scrolling` (the state the assembled
    /// `ScrollState` shares as `is_scroll_in_progress`), so a caller observing that flag sees the
    /// direct fling like a gesture one.
    pub fn fling(&self, velocity: f32) {
        if !velocity.is_finite() {
            return;
        }
        // The SAME path the gesture uses: the scroll state is assembled around this list's shared
        // states and asks its fling strategy, so a paging list settles on a page boundary instead of
        // decaying across one, and the plan-based velocity floor applies. (A CUSTOM `fling_behavior` is
        // a component parameter — the pager hands it to the state it assembles each frame — so this
        // entry point runs the built-in strategy; it is the list-level escape hatch, not the component
        // path.)
        let state = crate::modifier::ScrollState {
            offset: self.offset.clone(),
            is_scroll_in_progress: self.is_scrolling.clone(),
            fling_limit: self.fling_limit.clone(),
            scroll_pulse: self.scroll_pulse.clone(),
            snap: self.snap.clone(),
            step_goal: self.step_goal.clone(),
            fling_behavior: None,
        };
        state.fling(velocity);
    }
}

impl Default for LazyListState {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════
// IntervalList — 区间内容（对齐 Compose LazyListIntervalContent）
// ═══════════════════════════════════════════════════════

/// 一个区间：count 个连续项共享 key 工厂与内容闭包
pub(crate) struct Interval {
    pub count: usize,
    pub key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>,
    pub content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>,
    /// sticky header 区间（对标 Compose `stickyHeader`——滚动时钉在视口顶）
    pub is_sticky_header: bool,
}

impl Interval {
    /// 计算区间内全局 index 对应的 key（无工厂时用索引自身——位置即 key）
    pub fn key_of(&self, global_index: usize) -> u64 {
        match &self.key {
            Some(f) => f(global_index),
            None => global_index as u64,
        }
    }
}

/// 区间列表：全局 index 定位到 (interval, local_index)
pub(crate) struct IntervalList {
    pub intervals: Vec<Interval>,
    /// 每段起始全局 index（惰性构建缓存）
    starts: Vec<usize>,
    total: usize,
    /// key → 全局 index 映射（rebuild 时构建；无 key 工厂的段用索引自身）
    key_index: std::collections::HashMap<u64, usize>,
    /// 是否存在 sticky header 区间（rebuild 时构建——build 回溯据此跳过）
    has_sticky: bool,
}

impl IntervalList {
    pub fn new() -> Self {
        Self {
            intervals: Vec::new(), starts: Vec::new(), total: 0,
            key_index: std::collections::HashMap::new(),
            has_sticky: false,
        }
    }

    pub fn add(&mut self, count: usize, key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>, content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>, sticky: bool) {
        self.intervals.push(Interval { count, key, content, is_sticky_header: sticky });
    }

    /// 全局 index 是否为 sticky header（区间标志——O(区间数)）
    pub fn is_sticky(&self, global: usize) -> bool {
        self.locate(global)
            .map(|(iv, _)| self.intervals[iv].is_sticky_header)
            .unwrap_or(false)
    }

    /// 列表是否含 sticky header（rebuild 缓存）
    pub fn has_sticky_headers(&self) -> bool { self.has_sticky }

    /// 重建每段起始索引与总数，并构建 key → index 映射
    pub fn rebuild(&mut self) {
        self.starts.clear();
        self.total = 0;
        self.has_sticky = false;
        for iv in &self.intervals {
            self.starts.push(self.total);
            self.total += iv.count;
            self.has_sticky |= iv.is_sticky_header;
        }
        // key 映射（key 必须全列表唯一——对齐 Compose：重复 key 抛异常，
        // LazyLayout 中 key 冲突会导致状态错乱，官方约束 key 唯一）
        self.key_index.clear();
        for g in 0..self.total {
            let k = self.key_of_unchecked(g);
            if let Some(prev) = self.key_index.insert(k, g) {
                panic!(
                    "LazyColumn/LazyRow: duplicate key {k} — global index {g} collides with {prev}.                      Keys must be unique across the whole list (Compose throws IllegalStateException)"
                );
            }
        }
    }

    pub fn total(&self) -> usize { self.total }

    /// 全局 index → (interval 索引, 段内局部 index)
    pub fn locate(&self, global: usize) -> Option<(usize, usize)> {
        if global >= self.total { return None; }
        // 线性查找（区间数通常少；可改二分）
        for i in 0..self.intervals.len() {
            let start = self.starts[i];
            let count = self.intervals[i].count;
            if global >= start && global < start + count {
                return Some((i, global - start));
            }
        }
        None
    }

    /// 全局 index → key（无工厂段 = 索引自身；越界返回自身——调用方需先查 total）
    fn key_of_unchecked(&self, global: usize) -> u64 {
        if global >= self.total { return global as u64; }
        for i in 0..self.intervals.len() {
            let start = self.starts[i];
            let count = self.intervals[i].count;
            if global >= start && global < start + count {
                return self.intervals[i].key_of(global);
            }
        }
        global as u64
    }

    /// 全局 index → key
    pub fn key_of(&self, global: usize) -> Option<u64> {
        if global >= self.total { return None; }
        Some(self.key_of_unchecked(global))
    }

    /// key → 全局 index（rebuild 构建的 HashMap 缓存——O(1)，对齐 Compose
    /// NearestRangeKeyIndexMap 的目的；大数据量下无线性扫描）
    pub fn index_of_key(&self, key: u64) -> Option<usize> {
        self.key_index.get(&key).copied()
    }
}

// ═══════════════════════════════════════════════════════
// LazyColumn — 公开组件
// ═══════════════════════════════════════════════════════

/// A lazily built list (Compose's `LazyColumn` / `LazyRow`), whose axis the type parameter picks.
///
/// Only the visible items are composed and measured (plus the prefetch window either side). `items` takes
/// a stable key factory, so a scroll position survives an insert or a removal earlier in the data.
pub struct LazyList<A: LazyAxis> {
    axis: PhantomData<A>,
    state: Option<LazyListState>,
    spacing: f32,
    modifier: Modifier,
    intervals: IntervalList,
    /// Main-axis content padding `(before, after)` — vertical = top/bottom, horizontal = start/end
    content_padding: (f32, f32),
    /// Cross-axis content padding `(before, after)` — vertical = start/end, horizontal = top/bottom
    cross_padding: (f32, f32),
    /// Reverse layout (Compose's `reverseLayout`): the content is laid out from the far end of the main
    /// axis — index 0 at the bottom/right, offset 0 showing the list's start (item 0 at the viewport's
    /// far edge), and the scroll direction stays the same (offset grows toward the list's end).
    reverse: bool,
    /// Paging snap (Compose's `snapFlingBehavior`): every item exactly fills the viewport along the
    /// main axis, and a finished fling settles onto the nearest item boundary.
    ///
    /// Only meaningful for a PAGED list — which is what the date picker's months are (`fillParentMaxWidth`
    /// items), the same shape material3 reaches with `LazyRow` + `Box(fillParentMaxWidth())`
    /// (`DatePicker.kt:1722-1750`). Items of unequal main size turn it off instead: there is no snap
    /// position to speak of.
    snap_paging: bool,
    /// Every item fills the viewport along the main axis (`fillParentMaxWidth` / `fillParentMaxHeight`).
    /// Off by default, where items wrap to their content.
    fill_items: bool,
    /// Pin every item's main size to this instead of the viewport (`Pager` with `PageSize.Fixed`). A
    /// fixed page does not fill the viewport, so the snap step is this size plus the spacing.
    item_main_size: Option<f32>,
    /// How many items beyond the viewport the composition window covers on each side
    /// (`beyondViewportPageCount`). Defaults to `LAZY_BEYOND_BOUNDS`.
    beyond_bounds: usize,
    /// Whether the list reacts to wheel/drag input. `false` keeps programmatic scrolling working.
    user_scroll: bool,
    /// Where inside an item the list comes to rest when `snap_paging` is on: `0.0` is its leading edge,
    /// `(viewport - item) / 2.0` centres a fixed page, `viewport - item` puts it at the trailing edge.
    snap_offset: f32,
    /// How far into the page a release has to be before it carries on to the next one, in `0.0..=1.0`
    /// (Compose's `snapPositionalThreshold`); written back into the snap spec.
    snap_positional_threshold: f32,
    /// The spring a paging settle runs on (Compose's `snapAnimationSpec`); written back into the snap
    /// spec, so the wheel's page step and a fling settle with the same curve.
    snap_animation_spec: crate::animation::SpringSpec,
    /// How many pages one fling may carry (Compose's `PagerSnapDistance.atMost`), at least one.
    snap_max_pages: usize,
    /// The fling strategy the assembled scroll state runs with (`None` = the built-in snapping one).
    fling_behavior: Option<std::sync::Arc<dyn crate::fling::FlingBehavior>>,
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
    keyed: std::collections::HashMap<u64, f32>,
    /// The last few measured heights, a ring — the sample behind [`ItemHeightCache::coverage_height`]'s
    /// median. Not the `keyed` view: this is about the rows being shown NOW, not about identity.
    recent: Vec<f32>,
    /// The item size when the list GUARANTEES it — `fill_items` pins every item to the viewport, so the
    /// size is known analytically and a measured item is not needed to price the rest.
    ///
    /// This is not a nicety. Without it the 2412 month pages of a date picker are priced at the flat
    /// `LAZY_ITEM_ESTIMATED_HEIGHT` until each is measured, and since the real page is several times
    /// that, every position past the measured window — and so every fling that leaves it — lands
    /// somewhere meaningless. Measured: with the estimate in place a flick moved the picker six months
    /// in one gesture and a plain drag moved it backwards.
    uniform: Option<f32>,
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
    fn recent_median(&self) -> Option<f32> {
        if self.recent.is_empty() {
            return None;
        }
        let mut v = self.recent.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(v[v.len() / 2])
    }

    /// Note a height for [`Self::coverage_height`]'s median. A ring, so the estimate follows the rows the
    /// list is actually showing instead of everything it has ever measured.
    fn note_recent(&mut self, h: f32) {
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
        let m = m.then(self.modifier);
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

// ═══════════════════════════════════════════════════════
// LazyListPolicy — 测量策略
// ═══════════════════════════════════════════════════════

/// 懒列表测量：把已注册子节点按锚点排布，真实高度写回缓存，视口高回写。
/// 轴无关（`A: LazyAxis` 决定主轴方向——LazyColumn/LazyRow 共用）。
pub(crate) struct LazyListPolicy<A: LazyAxis> {
    pub axis: PhantomData<A>,
    pub cache: crate::runtime::state::State<ItemHeightCache>,
    pub viewport: crate::runtime::state::State<f32>,
    pub content_height: crate::runtime::state::Backchannel<f32>,
    pub fling_limit: crate::runtime::state::Backchannel<f32>,
    pub is_scroll_in_progress: crate::runtime::state::State<bool>,
    pub spacing: f32,
    pub total: usize,
    /// 主轴内容内边距 (before, after)
    pub content_padding: (f32, f32),
    /// 交叉轴内容内边距 (before, after)
    pub cross_padding: (f32, f32),
    /// 注册顺序 → 全局 index（sticky 项排在最后——画在最上层）
    pub globals: Vec<usize>,
    /// 当前数据全局 index → item key（build 期从 intervals 提取；
    /// measure 用其写 cache 的 key 视图——跨数据变化保持项身份高度）
    pub keys: Vec<u64>,
    /// 注册序中 sticky 子节点的 child 下标（全局升序）
    pub sticky_children: Vec<usize>,
    /// 钉住的 sticky header 全局 index（build 期回溯；钉住时锚点 = (pin, 0)）
    pub pin: Option<usize>,
    /// 反向布局（reverseLayout）：放置内容坐标 = content_h - 镜像位置 - 项高
    pub reverse: bool,
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
    pub state: LazyListState,  // 派生锚点回写（测量后精确值）
}

impl<A: LazyAxis> std::fmt::Debug for LazyListPolicy<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LazyListPolicy")
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
        // 视口主轴尺寸：有限约束直接用（回写缓存）；无界（父内容驱动——Column 无
        // 固定高度时给子节点 f32::MAX；⚠ is_finite() 对 f32::MAX 也返回 true，
        // 必须用框架惯例 `< f32::MAX` 判定）回退缓存值，避免视口
        // 无限膨胀（实测：f32::MAX 视口会让 clamp 把 offset 清零）
        let main_max = A::main_max(&constraints);
        let vh = if main_max < f32::MAX && main_max > 0.0 {
            // 非 silent set：viewport 变化（resize）→ 通知 build 重组，用新视口
            // 补组合可见项（方向一）。值稳定时 PartialEq 去重不通知（无振荡）。
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

        // 全局 index 映射：注册顺序从 start 到 end，start = first_index - 预取数
        // 但 build 只给了 policy first_index（锚点）——子节点全局序需要从 placement 反推：
        // 实际实现：build 把 (start, end) 传给 policy，这里按序写回
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
            // 同步写 key 视图（keys[global] = 该全局项的 item key）——跨数据
            // 变化保持"项身份 → 高度"，rebase 据此迁移到正确的 index
            if let Some(&k) = self.keys.get(global) {
                cache.record_keyed(k, *h);
            }
        }
        // 内容总高：注册项实测 + 未注册项预估（供 apply_scroll_delta 算 max_offset）；
        // 含 主轴 contentPadding（对齐 Compose：max_offset = before + 内容 + after - 视口）
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

        // 越界 clamp：scroll_to_item 无 total 信息，程序化滚动超出内容边界时
        // 在这里收回到末尾（对齐 Compose：scroll position 在 measure 期 clamp）
        let max_off = (content_h - vh).max(0.0);

        // 程序化跳转：消费 jump_request（放在 max_off 之后——动画目标需要 clamp）。
        // 像素 offset 用**写回后的缓存**推导（实测项真实高度 + 未测项预估）——
        // 与锚点解析/放置共用同一 prefix_height，round-trip 精确：跳 500 就是
        // 500，不会因预估 vs 实测高度差累积漂移（实测：旧实现 500×48 vs 47.5
        // → 落到 505）。reverseLayout 下公式相同（offset = before + prefix + 偏移：
        // 语义变为项**底**贴视口底——镜像坐标中与正向同构）。
        if let Some((req_idx, req_off, animate)) = self.state.jump_request.get() {
            let idx = req_idx.min(self.total.saturating_sub(1));
            let target = pad_before + prefix_height(&cache, idx, self.spacing) + req_off;
            self.state.jump_request.set(None);
            if animate {
                // 动画滚动（对齐 Compose animateScrollToItem——spring 收敛）；
                // push_animatable 内部处理同 state 动画替换（retarget 继承速度）
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
                // 立即跳转：取消进行中的 fling + 结束滚动中标记
                crate::animation::cancel_animation(&self.state.offset);
                self.is_scroll_in_progress.set(false);
                self.state.offset.set(target);
            }
        }
        // fling 极限回写（输入路径 ScrollState::fling + 程序化 LazyListState::fling）
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

        // 精确锚点回写：用真实高度（cache 已写回）反推 first_visible_index/offset——
        // build 期用的是预估高度，这里给出精确值（对齐 Compose 从 measure result 更新）
        let offset = self.state.offset.get();
        // 钉住时锚点 = (pin, 0)（对齐 Compose：firstVisibleItemIndex 就是钉住的
        // header）；否则自然锚点（像素反推）
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

        // 锚点排布：**内容坐标**（主轴 = 项在内容中的累计位置，不含 offset）——
        // 滚动由框架 scroll translate(-offset) 处理。若 placement 也含 offset 会
        // 双重偏移（实测：滚动 3000 后内容完全滚出视口）。
        // 内容起点 = before 内容内边距（对齐 Compose contentPadding）。
        // sticky pass（对齐 Compose LazyListMeasure）：
        //   正向 final(i) = max(C(i) - offset, before)   —— 滚过顶钉在 before 处
        //   反向 final(prev) = min(final(prev), final(next) - h(prev))
        //     —— 下一个 header 距顶 h(prev) 内时前一个开始滑出
        // 放置内容坐标 = final + offset；普通项保持自然位置（累计）
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
            // 反向布局：镜像坐标（从底向上累计）→ 内容坐标 = content_h - pos - h
            // （index 0 在内容底部；锚点/可见范围/pin 回溯在镜像坐标中公式不变）
            let content_pos = if self.reverse { content_h - pos - h } else { pos };
            placements.push(crate::layout::node::Placement {
                // 交叉轴从 cross_before 处开始（contentPadding 交叉轴语义）
                position: A::point(content_pos, cb),
                size: A::size(h, w),
            });
        }

        self.cache.set(cache);
        (A::size(vh, A::cross_max(&constraints)), placements)
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[crate::layout::node::Placement]) {
        /// Whether any node in this item's subtree asks for placement animation (`animateItem`).
        fn subtree_has_animate_item(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> bool {
            if nodes[idx].modifier.has_animate_item() {
                return true;
            }
            nodes[idx]
                .children
                .iter()
                .any(|&c| subtree_has_animate_item(nodes, c))
        }
        // Placement animations (Compose's `animateItem`): an item whose key occupied a different
        // main-axis slot on the PREVIOUS frame starts at the old slot and springs to the new one.
        // Items that were away (not placed last frame) reset instead of springing from a stale slot.
        // The map is keyed by the item key, so the animation survives the node recycling the lazy
        // window does.
        let mut anims = self
            .state
            .item_animations
            .lock()
            .expect("lazy item animations");
        anims.frame += 1;
        let frame = anims.frame;
        for (i, &c) in children.iter().enumerate() {
            // The marker sits on the item's ROOT content, and the lazy list registers a wrapper
            // group node per item — so the animation belongs to the wrapper and the marker is looked
            // for in its subtree.
            let animate = subtree_has_animate_item(nodes, c);
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
            nodes[c].item_anim = None;
            if !animate {
                continue;
            }
            let global = self.globals.get(i).copied().unwrap_or(i);
            let key = self.keys.get(global).copied().unwrap_or(global as u64);
            let main = A::main_size(crate::unit::Size::new(
                nodes[c].position.x,
                nodes[c].position.y,
            ));
            let item = anims.items.entry(key).or_insert_with(|| ItemAnimation {
                offset: crate::runtime::state::State::new(0.0),
                last_main: main,
                last_frame: frame,
            });
            if item.last_frame + 1 == frame {
                let delta = item.last_main - main;
                if delta.abs() > 0.5 {
                    crate::animation::cancel_animation(&item.offset);
                    item.offset.set(delta);
                    crate::animation::push_animatable_restart(
                        item.offset.clone(),
                        0.0,
                        crate::animation::AnimationSpec::Spring(
                            crate::animation::SpringSpec::default(),
                        ),
                    );
                }
            } else {
                // Away for a frame (or new): no stale spring.
                item.offset.set(0.0);
            }
            item.last_main = main;
            item.last_frame = frame;
            nodes[c].item_anim = Some((item.offset.clone(), A::VERTICAL));
        }
        retain_recent_items(&mut anims, frame);
    }
}


#[cfg(test)]
mod tests;
