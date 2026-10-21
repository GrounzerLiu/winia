//! Pull-to-refresh — material3's PullToRefreshBox, built on the nested-scroll pipeline.
//!
//! The box puts a NestedScrollConnection on itself. When the inner scrollable cannot consume a drag
//! (it is at its top) the leftover arrives in on_post_scroll, and the box turns it into a pull:
//! content translates down, a progress arc fills. Crossing the threshold starts the refresh: the
//! caller's callback runs, the content rests at the indicator offset, and the indicator switches to
//! the indeterminate spinner until the caller's is_refreshing turns false.
//!
//! The nested-scroll protocol has no release event, so the settle-back is armed with a short delay
//! and re-armed on every delta; letting go (no more deltas) returns the content to rest.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::animation::{AnimationSpec, TweenSpec};
use crate::composable;
use crate::components::loading_indicator::LoadingIndicator;
use crate::animation::interpolator::EaseOutCubic;
use crate::layout::components::{Column, Stack};
use crate::layout::ContentAlignment;
use crate::modifier::Modifier;
use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource, ScrollDelta};
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;

/// The pull distance (content offset, px) that starts a refresh.
pub const PULL_THRESHOLD: f32 = 64.0;
/// The farthest the content can be pulled.
pub const MAX_PULL: f32 = 96.0;
/// Where the content rests while the refresh runs.
pub const REFRESHING_OFFSET: f32 = 56.0;
/// The indicator's box (the LoadingIndicator's own size).
pub const INDICATOR_SIZE: f32 = 48.0;
/// Breathing room between the indicator and the content it sits above.
pub const INDICATOR_GAP: f32 = 8.0;
/// Drag distance to content offset — the pull is damped so it feels elastic.
const RESISTANCE: f32 = 0.5;

/// The remembered pull-to-refresh state; hoist it with ctx.remember only if a caller needs to inspect
/// the pull. PullToRefreshBox remembers its own when it is not passed one.
#[derive(Clone)]
pub struct PullToRefreshState {
    offset: State<f32>,
    /// 0..1, the determinate loading indicator's morph target while pulling.
    progress: State<f32>,
    triggered: Arc<AtomicBool>,
    refreshing: Arc<AtomicBool>,
}

impl PullToRefreshState {
    pub fn new() -> Self {
        Self {
            offset: State::new(0.0),
            progress: State::new(0.0),
            triggered: Arc::new(AtomicBool::new(false)),
            refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The current content offset (px) — exposed for tests and custom indicators.
    pub fn pull_offset(&self) -> f32 {
        self.offset.peek()
    }

    /// Whether this pull has crossed the threshold.
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::Relaxed)
    }

    fn begin_refresh(&self) {
        self.refreshing.store(true, Ordering::Relaxed);
        self.progress.set(1.0);
        if self.offset.peek() < REFRESHING_OFFSET {
            crate::animation::cancel_animation(&self.offset);
            crate::animation::push_animatable(
                self.offset.clone(),
                REFRESHING_OFFSET,
                AnimationSpec::Tween(
                    TweenSpec::new(Duration::from_millis(180), EaseOutCubic::new()),
                ),
            );
        }
    }

    fn finish_refresh(&self) {
        self.refreshing.store(false, Ordering::Relaxed);
        self.triggered.store(false, Ordering::Relaxed);
        self.progress.set(0.0);
        crate::animation::cancel_animation(&self.offset);
        crate::animation::push_animatable(
            self.offset.clone(),
            0.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(220), EaseOutCubic::new())),
        );
    }
}

impl Default for PullToRefreshState {
    fn default() -> Self {
        Self::new()
    }
}

struct PullConnection {
    state: PullToRefreshState,
    on_refresh: Arc<dyn Fn() + Send + Sync>,
}

impl NestedScrollConnection for PullConnection {
    fn on_post_scroll(
        &self,
        _child_consumed: ScrollDelta,
        available: ScrollDelta,
        source: NestedScrollSource,
    ) -> ScrollDelta {
        if source != NestedScrollSource::Drag || self.state.refreshing.load(Ordering::Relaxed) {
            return ScrollDelta::ZERO;
        }
        let dy = available.y;
        if dy == 0.0 {
            return ScrollDelta::ZERO;
        }
        let current = self.state.offset.peek();
        let next = (current + dy * RESISTANCE).clamp(0.0, MAX_PULL);
        let consumed = (next - current) / RESISTANCE;
        if consumed.abs() < 0.01 {
            return ScrollDelta::ZERO;
        }
        crate::animation::cancel_animation(&self.state.offset);
        self.state.offset.set(next);
        self.state
            .progress
            .set((next / PULL_THRESHOLD).clamp(0.0, 1.0));
        if next >= PULL_THRESHOLD && !self.state.triggered.swap(true, Ordering::Relaxed) {
            self.state.begin_refresh();
            (self.on_refresh)();
        } else if next < PULL_THRESHOLD {
            // Re-armed on every delta; a pause means the drag ended and the content settles back.
            crate::animation::push_animatable(
                self.state.offset.clone(),
                0.0,
                AnimationSpec::Tween(
                    TweenSpec::new(Duration::from_millis(200), EaseOutCubic::new())
                        .delay(Duration::from_millis(120)),
                ),
            );
        }
        ScrollDelta::new(0.0, consumed)
    }
}

/// material3's PullToRefreshBox: wrap a scrollable, report is_refreshing, run on_refresh.
pub struct PullToRefreshBox {
    is_refreshing: bool,
    on_refresh: Arc<dyn Fn() + Send + Sync>,
    modifier: Modifier,
}

impl PullToRefreshBox {
    pub fn new(is_refreshing: bool, on_refresh: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            is_refreshing,
            on_refresh: Arc::new(on_refresh),
            modifier: Modifier::new(),
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx) + Send + 'static,
    ) {
        // Declare the parameter that decides which indicator renders: without it the box's group
        // Skips when the caller flips is_refreshing, and the determinate spinner never gives way to
        // the rotating one.
        ctx.changed(&self.is_refreshing);
        let state = ctx.remember(|| PullToRefreshState::new()).get();
        let was_refreshing = state.refreshing.load(Ordering::Relaxed);
        if self.is_refreshing && !was_refreshing {
            state.begin_refresh();
        } else if !self.is_refreshing && was_refreshing {
            state.finish_refresh();
        }

        let connection = PullConnection {
            state: state.clone(),
            on_refresh: self.on_refresh.clone(),
        };
        let content_offset = state.offset.clone();
        let indicator_offset = state.offset.clone();
        let refreshing = self.is_refreshing;

        Stack::new()
            .content_alignment(ContentAlignment::TOP_CENTER)
            .modifier(
                self.modifier
                    .then(Modifier::new().nested_scroll(connection)),
            )
            .build(ctx, move |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_size().graphics_layer(move || {
                        crate::graphics::GraphicsLayerParams {
                            translation_y: content_offset.peek(),
                            ..Default::default()
                        }
                    }))
                    .build(ctx, content);

                if refreshing {
                    // The LoadingIndicator's indeterminate form, exactly as examples/
                    // loading_indicator_demo.rs uses it: the contained configuration runs the
                    // seven-shape morph loop plus the global 360-degree rotation (its own infinite
                    // transition), so the refresh reads as "still working".
                    LoadingIndicator::new()
                        .contained(true)
                        .modifier(
                            Modifier::new()
                                .size(INDICATOR_SIZE, INDICATOR_SIZE)
                                .graphics_layer(move || {
                                    let pull = indicator_offset.peek();
                                    crate::graphics::GraphicsLayerParams {
                                        translation_y: (pull - INDICATOR_SIZE - INDICATOR_GAP)
                                            .max(0.0),
                                        alpha: (pull / INDICATOR_SIZE).clamp(0.0, 1.0),
                                        ..Default::default()
                                    }
                                }),
                        )
                        .build(ctx);
                } else {
                    LoadingIndicator::determinate(state.progress.clone())
                        .modifier(
                            Modifier::new()
                                .size(INDICATOR_SIZE, INDICATOR_SIZE)
                                .graphics_layer(move || {
                                    let pull = indicator_offset.peek();
                                    crate::graphics::GraphicsLayerParams {
                                        translation_y: (pull - INDICATOR_SIZE - INDICATOR_GAP)
                                            .max(0.0),
                                        alpha: (pull / INDICATOR_SIZE).clamp(0.0, 1.0),
                                        ..Default::default()
                                    }
                                }),
                        )
                        .build(ctx);
                }
            });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::nested_scroll::{NestedScrollSource, ScrollDelta};
    use std::sync::atomic::AtomicUsize;

    fn connection() -> (PullConnection, PullToRefreshState, Arc<AtomicUsize>) {
        let state = PullToRefreshState::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let connection = PullConnection {
            state: state.clone(),
            on_refresh: Arc::new(move || {
                h.fetch_add(1, Ordering::Relaxed);
            }),
        };
        (connection, state, hits)
    }

    #[test]
    fn a_drag_past_the_threshold_starts_the_refresh() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let (connection, state, hits) = connection();
        let consumed = connection.on_post_scroll(
            ScrollDelta::ZERO,
            ScrollDelta::new(0.0, 200.0),
            NestedScrollSource::Drag,
        );
        assert_eq!(consumed.y, 192.0, "the pull consumes only the damped part it can take");
        assert_eq!(state.pull_offset(), MAX_PULL, "the pull clamps at MAX_PULL");
        assert!(state.is_triggered());
        assert_eq!(hits.load(Ordering::Relaxed), 1);

        // While refreshing, further drags are left to nobody: the pull is held.
        let held = connection.on_post_scroll(
            ScrollDelta::ZERO,
            ScrollDelta::new(0.0, 40.0),
            NestedScrollSource::Drag,
        );
        assert_eq!(held, ScrollDelta::ZERO);
        assert_eq!(hits.load(Ordering::Relaxed), 1, "on_refresh runs once");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn a_short_pull_does_not_refresh_and_retracts() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let (connection, state, hits) = connection();
        let consumed = connection.on_post_scroll(
            ScrollDelta::ZERO,
            ScrollDelta::new(0.0, 40.0),
            NestedScrollSource::Drag,
        );
        assert_eq!(consumed.y, 40.0);
        assert_eq!(state.pull_offset(), 20.0, "the pull is damped by RESISTANCE");
        assert!(!state.is_triggered());
        assert_eq!(hits.load(Ordering::Relaxed), 0);

        let retract = connection.on_post_scroll(
            ScrollDelta::ZERO,
            ScrollDelta::new(0.0, -80.0),
            NestedScrollSource::Drag,
        );
        assert_eq!(retract.y, -40.0, "only what the pull still has is consumed");
        assert_eq!(state.pull_offset(), 0.0);
        crate::animation::clear_all_animations();
    }

    #[test]
    fn a_wheel_is_not_a_pull() {
        let (connection, state, _) = connection();
        let consumed = connection.on_post_scroll(
            ScrollDelta::ZERO,
            ScrollDelta::new(0.0, 100.0),
            NestedScrollSource::Wheel,
        );
        assert_eq!(consumed, ScrollDelta::ZERO);
        assert_eq!(state.pull_offset(), 0.0);
    }
}
