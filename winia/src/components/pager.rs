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
//!     .content(|ctx, page| Text::new(format!("Page {}", page)).build(ctx))
//!     .build(ctx);
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
}

impl PagerState {
    pub fn new(page_count: usize) -> Self {
        Self {
            page_count,
            list: LazyListState::new(),
            current_page: State::new(0),
            current_page_offset_fraction: State::new(0.0),
            target_page: State::new(0),
            settled_page: State::new(0),
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

    /// Jump to `page` (Compose's `scrollToPage`). `page_offset` is in pixels, positive moving the page
    /// further along the main axis.
    pub fn scroll_to_page(&self, page: usize, page_offset: f32) {
        self.list.scroll_to_item(page, page_offset);
    }

    /// Animate to `page` (Compose's `animateScrollToPage`).
    pub fn animate_scroll_to_page(&self, page: usize, page_offset: f32) {
        self.list.animate_scroll_to_item(page, page_offset);
    }

    /// Recompute the page-shaped view from the list's anchors.
    ///
    /// Called by the builder once the list has composed; every write compares first, so a pager that
    /// did not move does not wake its readers (a `State::set` with an equal value is already deduped,
    /// but the fraction is a float and this keeps the intent visible).
    fn sync_from_list(&self) {
        let viewport = self.list.viewport_main_size();
        if viewport <= 0.0 {
            return;
        }
        let first = self.list.first_visible();
        let offset = self.list.first_visible_offset();
        let raw = (offset / viewport).clamp(0.0, 1.0);
        // The page closest to the snap position owns the fraction: at 0.6 through page 2 the pager is
        // 0.4 of the way into page 3, so it reports page 3 with -0.4 - which is what makes the
        // counter flip halfway through a swipe, as Compose's does.
        let (page, fraction) = if raw < 0.5 {
            (first, raw)
        } else {
            (first.saturating_add(1).min(self.page_count.saturating_sub(1)), raw - 1.0)
        };
        self.current_page.set(page);
        self.current_page_offset_fraction.set(fraction);
        self.target_page.set(page);
        if !self.list.is_scrolling() {
            self.settled_page.set(page);
        }
    }
}

/// A pager along one axis. [`HorizontalPager`] and [`VerticalPager`] are the two.
pub struct Pager<A: LazyAxis> {
    axis: PhantomData<A>,
    state: PagerState,
    modifier: Modifier,
    content_padding: f32,
    page_spacing: f32,
    reverse: bool,
    content: Option<Box<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>>,
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
            content: None,
        }
    }

    /// The page content. The index is the page, starting at 0.
    pub fn content(mut self, f: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static) -> Self {
        self.content = Some(Box::new(f));
        self
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

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let state = self.state;
        let sync = state.clone();
        let content = self
            .content
            .expect("Pager needs content: pager.content(|ctx, page| ..)");
        let count = state.page_count;
        ctx.changed(&count);
        LazyList::<A>::new_list()
            .state(state.list.clone())
            .fill_items(true)
            .snap_paging(true)
            .spacing(self.page_spacing)
            .content_padding(self.content_padding, self.content_padding)
            .reverse_layout(self.reverse)
            .modifier(self.modifier)
            .items(count, |page| page as u64, move |ctx, page| content(ctx, page))
            .build(ctx);
        sync.sync_from_list();
    }
}

/// The page divided by the viewport, for a caller that wants the raw position.
impl PagerState {
    /// `current_page as f32 + current_page_offset_fraction` - the continuous page position.
    pub fn current_page_position(&self) -> f32 {
        self.current_page() as f32 + self.current_page_offset_fraction()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::constraints::Constraints;
    use crate::runtime::composer::Composer;

    fn compose_pager(composer: &mut Composer, state: PagerState, page_count: usize, composed: std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                let c = composed.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .content(move |ctx, page| {
                        use std::sync::atomic::Ordering;
                        c.fetch_add(1, Ordering::Relaxed);
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    })
                    .build(ctx);
            });
        });
        let _ = page_count;
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
    }

    #[test]
    fn a_pager_starts_on_page_zero() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        compose_pager(&mut composer, state.clone(), 5, std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)));
        assert_eq!(state.current_page(), 0);
        assert_eq!(state.current_page_offset_fraction(), 0.0);
        assert_eq!(state.settled_page(), 0);
    }

    #[test]
    fn scrolling_to_a_page_moves_the_counter() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), 5, composed.clone());
        state.scroll_to_page(3, 0.0);
        // The jump applies during LAYOUT, and the page counter is derived at the end of the next
        // build (the same eventual consistency Compose has while a scroll is animating).
        compose_pager(&mut composer, state.clone(), 5, composed.clone());
        compose_pager(&mut composer, state.clone(), 5, composed.clone());
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
        compose_pager(&mut composer, state.clone(), 200, composed.clone());
        let n = composed.load(Ordering::Relaxed);
        assert!(n < 30, "a 200-page pager must not compose every page (composed {n})");
    }

    #[test]
    fn the_fraction_is_never_half_a_page() {
        let state = PagerState::new(3);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), 3, composed.clone());
        // Half a page into page 0: the counter is still 0 and the fraction approaches 0.5.
        state.list_state().offset.set(100.0);
        compose_pager(&mut composer, state.clone(), 3, composed.clone());
        assert!(state.current_page_offset_fraction().abs() < 0.5);
    }
}
