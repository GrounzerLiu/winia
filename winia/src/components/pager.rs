//! Pager: one full-viewport page at a time, with snapping and a page counter (Compose's
//! `HorizontalPager` / `VerticalPager`).
//!
//! It is built on the lazy list rather than beside it: a page IS an item that fills the viewport
//! (`LazyList::fill_items`), and paging IS the lazy list's snap (`LazyList::snap_paging`). What this
//! module adds is the page-shaped view of that state - `current_page`, the offset fraction, and
//! page-granular scrolling - plus the two builders callers use.
//!
//! ```ignore
//! let state = ctx.remember(|| PagerState::new(5)).get();
//! HorizontalPager::new(state)
//!     .modifier(Modifier::new().fill_max_size())
//!     .build(ctx, |ctx, page| Text::new(format!("Page {}", page)).build(ctx));
//! ```

use std::marker::PhantomData;

use crate::composable;
use crate::layout::axis::{HorizontalAxis, VerticalAxis};
use crate::layout::lazy_column::{LazyAxis, LazyList, LazyListState};
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;

/// Which page the pager shows, in page units.
///
/// `current_page` is the page CLOSEST to the snap position (Compose's definition), and
/// `current_page_offset_fraction` is how far the list has moved past it, in `[-0.5, 0.5)` - so a list
/// sitting halfway between page 2 and page 3 reports page 2 with a fraction approaching 0.5, then
/// page 3 with a fraction approaching -0.5. Together they are the continuous position.
#[derive(Clone)]
pub struct PagerState {
    page_count: usize,
    list: LazyListState,
    current_page: State<usize>,
    current_page_offset_fraction: State<f32>,
    target_page: State<usize>,
    settled_page: State<usize>,
    /// A stable identity for this handle: the derive is keyed by it, so swapping the state a call site
    /// passes rebuilds the derive instead of keeping the first one forever.
    identity: u64,
    /// The page size along the main axis, as the builder resolved it (the viewport for `Fill`).
    page_size: State<f32>,
    /// The builder's page-size SPEC (`None` = Fill): the derive reads the spec, not a captured value,
    /// because `remember_derived_state` keeps the first closure it was given.
    page_size_spec: State<Option<f32>>,
    /// The spacing and the leading content padding the list was built with: the derive reads them as
    /// States so a builder change re-derives instead of freezing the first frame's values.
    spacing: State<f32>,
    pad_before: State<f32>,
}

static NEXT_PAGER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl PagerState {
    pub fn new(page_count: usize) -> Self {
        Self {
            page_count,
            identity: NEXT_PAGER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            list: LazyListState::new(),
            current_page: State::new(0),
            current_page_offset_fraction: State::new(0.0),
            target_page: State::new(0),
            settled_page: State::new(0),
            page_size: State::new(0.0),
            page_size_spec: State::new(None),
            spacing: State::new(0.0),
            pad_before: State::new(0.0),
        }
    }

    /// The usual spelling at a call site: a state remembered for this composition point.
    pub fn remember(ctx: &mut ComposeCtx, page_count: usize) -> Self {
        ctx.remember(|| PagerState::new(page_count)).get()
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// The lazy list behind the pager, for anything a caller wants from the list itself (offsets,
    /// `can_scroll_forward`, the fling limit).
    pub fn list_state(&self) -> &LazyListState {
        &self.list
    }

    /// The page closest to the snap position.
    pub fn current_page(&self) -> usize {
        self.current_page.get()
    }

    /// How far past `current_page` the list sits, in `[-0.5, 0.5)` of a page.
    pub fn current_page_offset_fraction(&self) -> f32 {
        self.current_page_offset_fraction.get()
    }

    /// The page a scroll is heading to (the current one while settled).
    pub fn target_page(&self) -> usize {
        self.target_page.get()
    }

    /// The page the pager came to rest on.
    pub fn settled_page(&self) -> usize {
        self.settled_page.get()
    }

    pub fn is_scroll_in_progress(&self) -> bool {
        self.list.is_scrolling()
    }

    /// Jump to `page` (Compose's `scrollToPage`).
    ///
    /// `page_offset` is a FRACTION of the page size, as in Compose (`PagerState.kt:679-680`), not
    /// pixels: `0.0` is the page's rest position, `1.0` is a whole page past it. It is converted with
    /// the page size the last frame resolved, so a caller that jumps on the first frame gets the
    /// pixel-ness of `PageSize::Fill`.
    pub fn scroll_to_page(&self, page: usize, page_offset: f32) {
        let step = self.page_size.peek();
        let pixels = if step > 0.0 {
            page_offset * step - self.pad_before.peek()
        } else {
            -self.pad_before.peek()
        };
        self.target_page.set(page);
        self.list.scroll_to_item(page, pixels);
    }

    /// Animate to `page` (Compose's `animateScrollToPage`); `page_offset` is a page fraction.
    pub fn animate_scroll_to_page(&self, page: usize, page_offset: f32) {
        let step = self.page_size.peek();
        let pixels = if step > 0.0 {
            page_offset * step - self.pad_before.peek()
        } else {
            -self.pad_before.peek()
        };
        self.target_page.set(page);
        self.list.animate_scroll_to_item(page, pixels);
    }

    /// Recompute the page-shaped view from the list's scroll offset.
    ///
    /// Called by the derived subscription on every scroll tick; the four writes go out in one
    /// `write_batch`, so a synchronous observer never sees the new page next to the old fraction.
    fn sync_from_list(&self, page_size: f32, spacing: f32, _pad_before: f32) {
        // Pages are placed after the list's leading padding, but the scroll positions themselves are
        // 0, step, 2*step... The page position therefore comes directly from the list offset - no
        // viewport division and no padding term.
        let step = page_size + spacing;
        if step <= 0.0 {
            return;
        }
        let position = (self.list.offset() / step).max(0.0);
        let last = self.page_count.saturating_sub(1) as f32;
        let rounded = position.round().clamp(0.0, last);
        let current = rounded as usize;
        // `round()` puts the rest position at an integer, so the fraction is in [-0.5, 0.5): half a
        // page into a swipe the counter flips, which is Compose's definition.
        let fraction = (position - rounded).clamp(-0.5, 0.5 - f32::EPSILON);
        // One batch: a synchronous observer (another derive, a `State::changes` collector) woken by the
        // first write would otherwise read the new page with the previous frame's fraction. Compose
        // publishes both in one measure result.
        let settled = !self.list.is_scrolling();
        crate::runtime::state::write_batch(|| {
            self.current_page.set(current);
            self.current_page_offset_fraction.set(fraction);
            // `target_page` is the page a programmatic scroll is HEADING to (Compose derives it from
            // the target until the scroll ends); resetting it on every tick erased what
            // `animate_scroll_to_page` had just asked for.
            if settled {
                self.target_page.set(current);
                self.settled_page.set(current);
            }
        });
    }
}

/// How a page is sized along the main axis (Compose's `PageSize`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// The page fills the viewport, which is what makes a page boundary a snap position.
    Fill,
    /// The page is this many pixels along the main axis, and the viewport shows more than one.
    Fixed(f32),
}

/// Where inside the viewport a page comes to rest (Compose's `SnapPosition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapPosition {
    Start,
    Center,
    End,
}

/// The pager's geometry, as of the last measure (Compose's `PagerLayoutInfo`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagerLayoutInfo {
    pub page_count: usize,
    /// The viewport size along the main axis.
    pub viewport_size: f32,
    /// The page size along the main axis (the viewport when `PageSize::Fill`).
    pub page_size: f32,
    pub current_page: usize,
    pub current_page_offset_fraction: f32,
}

/// A pager along one axis. [`HorizontalPager`] and [`VerticalPager`] are the two.
pub struct Pager<A: LazyAxis> {
    axis: PhantomData<A>,
    state: PagerState,
    modifier: Modifier,
    content_padding: f32,
    page_spacing: f32,
    reverse: bool,
    page_size: PageSize,
    snap_position: SnapPosition,
    beyond_viewport_page_count: usize,
    user_scroll_enabled: bool,
}

/// A pager whose pages scroll horizontally (Compose's `HorizontalPager`).
pub type HorizontalPager = Pager<HorizontalAxis>;
/// A pager whose pages scroll vertically (Compose's `VerticalPager`).
pub type VerticalPager = Pager<VerticalAxis>;

impl Pager<HorizontalAxis> {
    pub fn new(state: PagerState) -> Self {
        Self::with_state(state)
    }
}

impl Pager<VerticalAxis> {
    pub fn new(state: PagerState) -> Self {
        Self::with_state(state)
    }
}

impl<A: LazyAxis> Pager<A> {
    fn with_state(state: PagerState) -> Self {
        Self {
            axis: PhantomData,
            state,
            modifier: Modifier::new(),
            content_padding: 0.0,
            page_spacing: 0.0,
            reverse: false,
            page_size: PageSize::Fill,
            snap_position: SnapPosition::Start,
            beyond_viewport_page_count: 4,
            user_scroll_enabled: true,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Padding before the first page and after the last, along the main axis (`contentPadding`).
    pub fn content_padding(mut self, padding: f32) -> Self {
        self.content_padding = padding;
        self
    }

    /// Space between pages (`pageSpacing`).
    pub fn page_spacing(mut self, spacing: f32) -> Self {
        self.page_spacing = spacing;
        self
    }

    /// Index 0 at the far end of the main axis (`reverseLayout`).
    pub fn reverse_layout(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    /// How a page is sized (`PageSize`), `PageSize::Fill` by default.
    pub fn page_size(mut self, size: PageSize) -> Self {
        self.page_size = size;
        self
    }

    /// Where a page rests inside the viewport (`SnapPosition`), `Start` by default. Only visible when
    /// the pages are smaller than the viewport (`PageSize::Fixed`).
    pub fn snap_position(mut self, position: SnapPosition) -> Self {
        self.snap_position = position;
        self
    }

    /// How many pages beyond the viewport compose (`beyondViewportPageCount`).
    pub fn beyond_viewport_page_count(mut self, pages: usize) -> Self {
        self.beyond_viewport_page_count = pages;
        self
    }

    /// Whether the pager reacts to wheel and drag input (Compose's `userScrollEnabled`).
    pub fn user_scroll_enabled(mut self, enabled: bool) -> Self {
        self.user_scroll_enabled = enabled;
        self
    }

    /// Compose the pager. `content` receives the page index, starting at 0 - the `PagerScope` shape
    /// (`Fn(&mut ComposeCtx, T)` in docs/component-authoring.md 1.3), since a pager composes its
    /// content once.
    #[composable]
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) {
        let state = self.state;
        // The page count reaches the list as its item count, which the lazy list declares; declaring it
        // here would land on this scope while the parameters are compared on the list's own group.
        let count = state.page_count;
        // The list reports the full viewport; Pager pages use the usable space between the two
        // main-axis content paddings, just like Compose's `mainAxisAvailableSize`.
        let viewport = state.list.viewport_main_size();
        let page_size_spec = match self.page_size {
            PageSize::Fill => None,
            PageSize::Fixed(size) => Some(size),
        };
        if state.page_size_spec.peek() != page_size_spec {
            state.page_size_spec.set(page_size_spec);
        }
        // Compose subtracts the content padding from the page size (`PagerMeasurePolicy.kt:122-147`):
        // a Fill page is `viewport - before - after`, or the page is placed at `before` and clipped by
        // the padding it was supposed to sit inside.
        let page_size = match page_size_spec {
            Some(size) => size,
            None => (viewport - self.content_padding - self.content_padding).max(0.0),
        };
        let page_spacing = self.page_spacing;
        let content_padding = self.content_padding;
        if state.page_size.peek() != page_size {
            state.page_size.set(page_size);
        }
        if state.spacing.peek() != self.page_spacing {
            state.spacing.set(self.page_spacing);
        }
        if state.pad_before.peek() != self.content_padding {
            state.pad_before.set(self.content_padding);
        }
        // SnapPosition is measured inside the usable layout space, not the outer padding. The list
        // places the first page after `pad_before` while the scroll position for page 0 remains 0.
        let available = (viewport - self.content_padding - self.content_padding).max(0.0);
        let snap_offset = match self.snap_position {
            SnapPosition::Start => 0.0,
            SnapPosition::Center => ((available - page_size) / 2.0).max(0.0),
            SnapPosition::End => (available - page_size).max(0.0),
        };
        let mut list = LazyList::<A>::new_list()
            .state(state.list.clone())
            .snap_paging(true)
            .snap_offset(snap_offset)
            .beyond_bounds(self.beyond_viewport_page_count)
            .user_scroll(self.user_scroll_enabled)
            .spacing(self.page_spacing)
            .content_padding(self.content_padding, self.content_padding)
            .reverse_layout(self.reverse)
            .modifier(self.modifier);
        list = match self.page_size {
            PageSize::Fill => list.fill_items(true),
            PageSize::Fixed(size) => list.item_main_size(size),
        };
        list.items(count, |page| page as u64, move |ctx, page| content(ctx, page))
            .build(ctx);
        // The counter has to follow the list WHILE it scrolls, and this call site does not re-run for
        // that (the inner list group re-enters, not the pager's build) - which is why a build-time
        // derive left the dots on the old page during a wheel or a slow drag. `remember_derived_state`
        // subscribes to the offset directly, so the two States move on every scroll frame; Compose's
        // `currentPageOffsetFraction` is continuous for the same reason.
        let mirror = state.clone();
        let list = state.list.clone();
        // `remember_derived_state` keeps the FIRST closure it was given, so a caller that swaps the
        // `PagerState` it passes at one call site would keep deriving from the old one. Keying the scope
        // by the state's identity rebuilds the derive when the state changes.
        let state_id = state.identity;
        let _position = ctx.key(state_id, |ctx| crate::flow::remember_derived_state(ctx, move || {
            // Everything is read through `get()`: the derive is remembered ONCE, so a captured builder
            // value would freeze the first frame's answer - and the first frame's viewport is 0, which
            // is exactly why the counter never moved (step 0 -> early return, forever).
            let pad_before = mirror.pad_before.get();
            let page_size = match mirror.page_size_spec.get() {
                Some(size) => size,
                // A Fill page is `viewport - before - after` - the size `build` measures and snaps by.
                // Dividing by the raw viewport here reported the page one late and scaled the fraction
                // by page/viewport on any padded pager (the padded test only ever sat on page 0).
                None => (list.viewport_main_size() - 2.0 * pad_before).max(0.0),
            };
            let spacing = mirror.spacing.get();
            mirror.sync_from_list(page_size, spacing, pad_before);
            (
                mirror.current_page.peek(),
                mirror.current_page_offset_fraction.peek(),
            )
        }));
    }
}

/// The page divided by the viewport, for a caller that wants the raw position.
impl PagerState {
    /// `current_page as f32 + current_page_offset_fraction` - the continuous page position.
    pub fn current_page_position(&self) -> f32 {
        self.current_page() as f32 + self.current_page_offset_fraction()
    }

    /// The pager's geometry as of the last measure (`PagerLayoutInfo`).
    pub fn layout_info(&self) -> PagerLayoutInfo {
        let viewport_size = self.list.viewport_main_size();
        let page_size = self.page_size.peek();
        PagerLayoutInfo {
            page_count: self.page_count,
            viewport_size,
            page_size,
            current_page: self.current_page(),
            current_page_offset_fraction: self.current_page_offset_fraction(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::constraints::Constraints;
    use crate::runtime::composer::Composer;

    fn compose_pager(
        composer: &mut Composer,
        state: PagerState,
        composed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                let c = composed.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, move |ctx, page| {
                        use std::sync::atomic::Ordering;
                        c.fetch_add(1, Ordering::Relaxed);
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
    }

    #[test]
    fn a_pager_starts_on_page_zero() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        compose_pager(&mut composer, state.clone(), std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)));
        assert_eq!(state.current_page(), 0);
        assert_eq!(state.current_page_offset_fraction(), 0.0);
        assert_eq!(state.settled_page(), 0);
    }

    #[test]
    fn scrolling_to_a_page_moves_the_counter() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        state.scroll_to_page(3, 0.0);
        // The jump applies during LAYOUT, and the page counter is derived at the end of the next
        // build (the same eventual consistency Compose has while a scroll is animating).
        compose_pager(&mut composer, state.clone(), composed.clone());
        compose_pager(&mut composer, state.clone(), composed.clone());
        assert_eq!(state.list_state().first_visible(), 3, "the jump landed on page 3");
        assert_eq!(state.current_page(), 3, "scrollToPage(3) makes page 3 current");
        assert!(state.current_page_position() >= 2.99, "position={}", state.current_page_position());
    }

    #[test]
    fn only_a_window_of_pages_composes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let state = PagerState::new(200);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        let n = composed.load(Ordering::Relaxed);
        assert!(n < 30, "a 200-page pager must not compose every page (composed {n})");
    }

    #[test]
    fn the_fraction_is_never_half_a_page() {
        let state = PagerState::new(3);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        // Half a page into page 0: the counter is still 0 and the fraction approaches 0.5.
        state.list_state().offset.set(100.0);
        compose_pager(&mut composer, state.clone(), composed.clone());
        assert!(state.current_page_offset_fraction().abs() < 0.5);
    }

    #[test]
    fn a_fixed_page_is_the_size_it_was_given() {
        let state = PagerState::new(9);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .page_size(PageSize::Fixed(120.0))
                    .snap_position(SnapPosition::Center)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .page_size(PageSize::Fixed(120.0))
                    .snap_position(SnapPosition::Center)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        let info = state.layout_info();
        assert_eq!(info.page_size, 120.0, "a fixed page keeps its size");
        assert_eq!(info.viewport_size, 300.0);
        assert_eq!(info.page_count, 9);
        // The page counter divides by the STEP (page + spacing), not by the viewport.
        assert_eq!(state.current_page(), 0, "page 0 at rest");
        assert!(state.current_page_offset_fraction().abs() < 1e-3, "fraction 0 at rest, got {}", state.current_page_offset_fraction());
    }

    #[test]
    fn a_restricted_window_composes_fewer_pages() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mut counts = Vec::new();
        for beyond in [0usize, 4] {
            let state = PagerState::new(200);
            let mut composer = Composer::new();
            let theme = crate::theme::ThemeColors::default_light();
            let composed = std::sync::Arc::new(AtomicUsize::new(0));
            let c = composed.clone();
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let s = state.clone();
                    HorizontalPager::new(s)
                        .beyond_viewport_page_count(beyond)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, move |ctx, page| {
                            c.fetch_add(1, Ordering::Relaxed);
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
            counts.push(composed.load(Ordering::Relaxed));
        }
        assert!(
            counts[0] < counts[1],
            "beyondViewportPageCount=0 must compose fewer pages than 4 (got {:?})",
            counts
        );
    }

    #[test]
    fn beyond_zero_keeps_paging_snap_configured() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                HorizontalPager::new(state.clone())
                    .beyond_viewport_page_count(0)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, _page| {
                        crate::components::text::Text::new("page").build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        assert!(state.list_state().snap.peek().is_some(), "a two-page pager must snap even with beyond=0");
    }

    #[test]
    fn a_centered_fixed_pager_can_reach_its_last_page() {
        let state = PagerState::new(4);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state.clone())
                        .page_size(PageSize::Fixed(120.0))
                        .snap_position(SnapPosition::Center)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        };
        build(&mut composer);
        state.scroll_to_page(3, 0.0);
        build(&mut composer);
        build(&mut composer);
        assert_eq!(state.current_page(), 3, "the last page remains reachable with Center snap");
        assert!(state.current_page_offset_fraction().abs() < 1e-3);
    }

    #[test]
    fn user_scroll_disabled_drops_the_scroll_channel() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .user_scroll_enabled(false)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        // The lazy list's own bookkeeping element stays (the measure pass needs the content height);
        // what must disappear is the wheel/drag channel.
        let has_wheel_channel = composer.arena_nodes().iter().any(|node| {
            node.modifier
                .elements()
                .iter()
                .any(|el| matches!(el, crate::modifier::ModifierElement::HorizontalScroll { .. }))
        });
        assert!(!has_wheel_channel, "userScrollEnabled=false must not attach the horizontal scroll channel");
    }

    /// The snap positions stay on page boundaries even when the page is offset inside the viewport: the
    /// `SnapPosition` shift is applied to the page's PLACEMENT, not to the scroll target (Compose
    /// shifts the page too). A centred fixed page therefore rests at `k * step`, not at `k * step +
    /// offset`, which is what this pins.
    #[test]
    fn a_centred_fixed_page_rests_on_a_page_boundary() {
        let state = PagerState::new(9);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let mut build = |composer: &mut Composer, state: PagerState| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state)
                        .page_size(PageSize::Fixed(120.0))
                        .snap_position(SnapPosition::Center)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        };
        build(&mut composer, state.clone());
        state.scroll_to_page(1, 0.0);
        build(&mut composer, state.clone());
        build(&mut composer, state.clone());
        assert!(
            (state.list_state().offset() - 120.0).abs() < 0.5,
            "page 1 rests at one page boundary (120), not at boundary + snap offset: got {}",
            state.list_state().offset()
        );
        assert_eq!(state.current_page(), 1);
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "and the fraction is 0 at rest, got {}",
            state.current_page_offset_fraction()
        );
    }

    /// The user's bug: while the list scrolls by something OTHER than this call site re-running, the
    /// counter has to follow. Writing the offset directly is exactly that shape - no compose happens -
    /// so this pins the derive being a subscription and not a build-time computation.
    #[test]
    fn the_counter_follows_a_scroll_that_never_recomposes_the_pager() {
        let state = PagerState::new(9);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        // A second pass: the page size is resolved from the viewport the FIRST measure wrote.
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        assert_eq!(state.current_page(), 0);

        // Two whole pages of scroll, written like the wheel does it: no compose call in between.
        let step = state.layout_info().page_size;
        assert!(step > 0.0, "the first measure resolved the page size");
        state.list_state().offset.set(step * 2.0);

        assert_eq!(
            state.current_page(),
            2,
            "the counter must follow the offset without this command re-composing"
        );
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "and land on the boundary exactly, got {}",
            state.current_page_offset_fraction()
        );
    }

    /// A padded pager starts AT its first snap position: the fraction is 0 on the first frame, not
    /// `-pad/step`, and the page is not clipped by the padding it sits inside.
    #[test]
    fn a_padded_pager_starts_on_its_first_snap_position() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer, state: PagerState| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state)
                        .content_padding(16.0)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        build(&mut composer, state.clone());
        build(&mut composer, state.clone());
        assert_eq!(state.current_page(), 0);
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "a padded pager rests on fraction 0, got {}",
            state.current_page_offset_fraction()
        );
        let info = state.layout_info();
        assert!(
            (info.page_size - 268.0).abs() < 0.5,
            "a Fill page is viewport - before - after (300 - 16 - 16), got {}",
            info.page_size
        );
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let first_page = nodes[root].children.iter().copied().find(|&child| {
            (nodes[child].measured_size.width - info.page_size).abs() < 0.5
        }).expect("the first page uses the resolved Fill size");
        assert!(
            (nodes[first_page].measured_size.width - 268.0).abs() < 0.5,
            "the measured page must honor content padding, got {}",
            nodes[first_page].measured_size.width
        );

        // Page 1 at rest: the counter divides by the PADDED page size (268), not by the viewport (300).
        // The derive used to fall back to the raw viewport, which only ever showed past page 0.
        state.scroll_to_page(1, 0.0);
        build(&mut composer, state.clone());
        build(&mut composer, state.clone());
        assert_eq!(state.current_page(), 1, "page 1 is the current page");
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "and it rests on fraction 0, got {}",
            state.current_page_offset_fraction()
        );
    }

    /// A different `PagerState` at the same call site must be followed: the derive is keyed by the
    /// state's identity, not remembered across the swap.
    #[test]
    fn swapping_the_state_at_one_call_site_follows_the_new_state() {
        let first = PagerState::new(5);
        let second = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer, state: PagerState| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        build(&mut composer, first.clone());
        build(&mut composer, first.clone());
        // Swap the state at the same call site and scroll the NEW one.
        build(&mut composer, second.clone());
        build(&mut composer, second.clone());
        let step = second.layout_info().page_size;
        assert!(step > 0.0, "the new state resolved its page size");
        second.list_state().offset.set(step * 2.0);
        assert_eq!(
            second.current_page(),
            2,
            "the derive moved to the new state"
        );
    }

    /// The `ScrollState` the last frame assembled for the pager's list - the handle a wheel notch
    /// steps (it is what carries the `snap` spec and the page goal).
    fn pager_scroll_state(composer: &Composer) -> crate::modifier::ScrollState {
        composer
            .arena_nodes()
            .iter()
            .find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::HorizontalScroll { state, .. } => {
                        Some(state.clone())
                    }
                    _ => None,
                })
            })
            .expect("the pager attaches a horizontal scroll channel")
    }

    /// The wheel's page goal lives on the LIST, not on the handle a frame assembles.
    ///
    /// A scroll tick rebuilds the lazy list, and `build` assembles a new `ScrollState` each time. With
    /// the goal owned by that handle, the fresh one saw an empty goal and computed the next page from
    /// the LIVE offset - which, mid-spring, is inside the page the first notch is already heading to,
    /// so the second notch re-targeted the same page and the pager stopped responding. The goal is a
    /// field of `LazyListState` now and every assembled handle clones it.
    #[test]
    fn a_page_step_survives_the_lists_rebuild() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state.clone())
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        // The page size resolves from the viewport the PREVIOUS measure wrote, so the first frame
        // only records the viewport and the second resolves the Fill page.
        build(&mut composer);
        build(&mut composer);
        let page = state.layout_info().page_size;
        assert!(page > 0.0, "the first measure resolved the page size");

        let first = pager_scroll_state(&composer);
        assert!(
            std::sync::Arc::ptr_eq(&first.step_goal, &state.list_state().step_goal),
            "the frame's handle shares the list's page goal - that sharing is what outlives the frame"
        );
        assert!(first.snap_page(true, false), "page 0 -> 1 starts a step");
        assert_eq!(
            *state.list_state().step_goal.lock().unwrap_or_else(|e| e.into_inner()),
            Some(page),
            "the step's goal is recorded on the list, not on the frame's handle"
        );

        // A scroll tick moves the offset, and the next frame rebuilds the list - the real path a
        // second wheel notch takes while the first spring is still moving.
        for _ in 0..4 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        build(&mut composer);
        let second = pager_scroll_state(&composer);
        assert!(second.snap_page(true, false), "page 1 -> 2 starts the next step");
        assert_eq!(
            *state.list_state().step_goal.lock().unwrap_or_else(|e| e.into_inner()),
            Some(page * 2.0),
            "the notch after the rebuild steps from the goal the first one left, not from the live offset"
        );
        crate::animation::clear_all_animations();
    }

    /// `current_page_offset_fraction` is documented as `[-0.5, 0.5)`, a HALF-OPEN interval. Past the
    /// last reachable page the counter clamps to the last page, so an unclamped fraction would report
    /// exactly `0.5` - and `current_page_position()` half a page further than the pager can be.
    #[test]
    fn the_fraction_stays_below_half_a_page_past_the_end() {
        let state = PagerState::new(3);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        // A second frame: the page size is resolved from the viewport the first measure wrote.
        compose_pager(&mut composer, state.clone(), composed.clone());
        let step = state.layout_info().page_size;
        assert!(step > 0.0, "the first measure resolved the page size");

        // Written like a wheel or an overshooting fling leaves it, with no measure to clamp it back.
        state.list_state().offset.set(step * 5.0);
        assert_eq!(state.current_page(), 2, "the counter clamps to the last page");
        assert!(
            state.current_page_offset_fraction() < 0.5,
            "the fraction must stay inside [-0.5, 0.5), got {}",
            state.current_page_offset_fraction()
        );
    }
}
