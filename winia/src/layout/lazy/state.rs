use super::*;

/// A lazy list's scroll state: winia's pixel model with Compose's anchor model on top.
///
/// Scrolling goes through the existing channel — `ScrollState.offset` (a total pixel offset, attached
/// with `vertical_scroll`/`horizontal_scroll`). `first_visible_index` / `first_visible_offset` are the
/// DERIVED anchor (Compose's semantics, exposed for callers), and `last_known_first_key` remembers the
/// first visible item's key, so inserting or removing items at the head can correct the offset by key.
#[derive(Debug, Clone)]
pub struct LazyListState {
    /// The pixel scroll offset (attached with the scroll modifier — the input channel).
    pub offset: crate::runtime::state::State<f32>,
    /// The last known key of the first visible item (a data change corrects the position by it).
    pub(crate) last_known_first_key: crate::runtime::state::State<Option<u64>>,
    /// The total the last `build` saw — a STATE-level guard, deliberately not a composition remember:
    /// when a caller holds the state across composers, a remembered flag fired the key correction every
    /// frame (measured: the second render pulled offset 2000 back to 0). Living here it shares
    /// `LazyListState`'s lifetime, so only a real data change (a different total) corrects.
    pub(crate) known_total: crate::runtime::state::State<usize>,
    /// A programmatic jump request, `(index, offset-in-item, animate)`. The anchor is authoritative
    /// (Compose's `requestPositionAndForgetLastKnownKey`: the scroll position IS the anchor, composition
    /// starts there, and the pixel offset is derived from the cache during measure — never the other way
    /// round). Consumed and cleared by the first measure; `animate` selects a spring
    /// (`animateScrollToItem`).
    pub(crate) jump_request: crate::runtime::state::State<Option<(usize, f32, bool)>>,
    /// How far a fling may travel: written back during measure as `content height - viewport height`,
    /// or `f32::MAX` while that is still unknown. A REAL `0` means the content fits and the list cannot
    /// move — see `ScrollState::fling_limit`, which had the same "0 means unknown" conflation.
    ///
    /// Visible to `components::scrollbar`, which reads it as `content = limit + viewport`.
    pub(crate) fling_limit: crate::runtime::state::Backchannel<f32>,
    /// The viewport's main-axis size from the last measure: what a pager needs to turn a pixel offset
    /// into a page fraction. `0.0` until the first measure.
    ///
    /// A `State`, not a backchannel: the first measure is where a pager learns its page size, and the
    /// pager's own build does not run again just because the viewport appeared - a derived value that
    /// reads this has to be woken by the write.
    pub(crate) viewport_main: crate::runtime::state::State<f32>,
    /// The scroll-activity pulse (lights a boundary scrollbar; same semantics as
    /// `ScrollState::scroll_pulse`). A lazy list assembles its `ScrollState` during `build` from three
    /// clones (`offset`, `is_scrolling`, `fling_limit`), so the pulse has to live HERE to stay stable
    /// across frames; the assembled state clones it. Visible to `components::scrollbar`.
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
    /// Every key the data has held so far, so the next build can tell a key that is new to the DATA
    /// from one that is merely new to the composed window (scrolling composes the latter, and fading
    /// those in makes a scroll look like a strobe).
    pub(crate) known_keys: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    /// Keys that are new to the data and have not been placed yet; the policy takes one per place, so
    /// a fade starts exactly once even when a frame takes several compose passes.
    pub(crate) pending_new: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    /// Derived: the first visible item's index (updated after every `build`).
    pub first_visible_index: crate::runtime::state::State<usize>,
    /// Derived: the first visible item's offset (positive = how far it has scrolled out).
    pub first_visible_offset: crate::runtime::state::State<f32>,
    /// What the last measure pass saw — Compose's `LazyListState.layoutInfo`. A fresh `Arc` per
    /// measure, so a reader that tracks it is woken by every layout rather than by value equality.
    pub(crate) layout_info: crate::runtime::state::State<std::sync::Arc<super::LazyListLayoutInfo>>,
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
            known_keys: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            pending_new: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            is_scrolling: crate::runtime::state::State::new(false),
            first_visible_index: crate::runtime::state::State::new(0),
            first_visible_offset: crate::runtime::state::State::new(0.0),
            layout_info: crate::runtime::state::State::new(std::sync::Arc::new(
                super::LazyListLayoutInfo::default(),
            )),
        }
    }

    /// The first visible item's index.
    pub fn first_visible(&self) -> usize { self.first_visible_index.get() }

    /// The first visible item's offset (positive = scrolled out).
    pub fn offset(&self) -> f32 { self.offset.get() }

    /// How far the first visible item has been scrolled past, in pixels (`0.0` = its leading edge is
    /// at the viewport edge).
    pub fn first_visible_offset(&self) -> f32 { self.first_visible_offset.get() }

    /// The layout information of the last measure pass — Compose's `LazyListState.layoutInfo`: the
    /// visible items (index, key, main-axis offset and size), the viewport bounds and the list's
    /// parameters. Empty until the first measure.
    ///
    /// The read is TRACKED: a caller that derives from it is woken when the visible set changes.
    pub fn layout_info(&self) -> std::sync::Arc<super::LazyListLayoutInfo> {
        self.layout_info.get()
    }

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

    /// Jump to an index immediately (the item's top aligns with the viewport top, plus an offset).
    ///
    /// The signature matches Compose's `scrollToItem(index, scrollOffset = 0)`: no height cache and no
    /// spacing are needed here, because the measure pass consumes the request — it composes the window
    /// from the anchor first, then derives the pixel offset from the written-back heights (the same
    /// `prefix_height` the placement and the anchor resolution use, so a round trip is exact: no
    /// estimate-vs-measured drift of 48 against 47.5 moving the landing point). An out-of-range target is
    /// clamped during the measure; this method does not know the total.
    pub fn scroll_to_item(&self, index: usize, scroll_offset: f32) {
        self.jump_request.set(Some((index, scroll_offset, false)));
    }

    /// Animate a scroll to an index (Compose's `animateScrollToItem(index, scrollOffset)`).
    ///
    /// Same anchor authority as [`LazyListState::scroll_to_item`]: the measure pass consumes the request
    /// and derives the pixel target, and a spring (default damping 1.0, stiffness 200 — settles in about
    /// 300-400 ms) moves the offset there. The target is clamped into `[0, max_offset]`.
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

