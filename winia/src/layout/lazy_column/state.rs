use super::*;

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
    /// Per-item placement/fade animations (keyed by item key), shared with the measure policy.
    pub(crate) item_animations: crate::layout::item_animation::ItemAnimationHost,
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
            item_animations: crate::layout::item_animation::ItemAnimationHost::default(),
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

