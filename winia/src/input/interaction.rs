//! InteractionSource / ComponentState — Compose foundation's `interaction`.
//!
//! Compose implements it as a `MutableSharedFlow<Interaction>` event stream plus extensions such
//! as `collectIsPressedAsState`, which maintain an "active interactions" list and expose boolean
//! state derived from it (upstream foundation 1.11.4: `InteractionSource.kt`,
//! `PressInteraction.kt`, and so on). winia has a single-pointer synchronous model, so four
//! `State<bool>` values express the same thing (pressed/focused/hovered/dragged):
//! - Reads (`is_*` / `state()`) go through `State::get()` → they register a composition dependency,
//!   so a state change recomposes the consumer;
//! - Emits (`emit_*`) go through `State::set()` → PartialEq dedup + notify + wake the event loop.
//!
//! The difference (a deliberate simplification, documented here): Compose's Press/Release/Cancel
//! carry an interaction instance identity so several pointers or interactions can coexist; winia
//! has one active pointer per window, where boolean flags suffice. Multi-pointer support waits for
//! the pointer pipeline upgrade.

use crate::runtime::state::State;
use parking_lot::Mutex;
use std::sync::Arc;

/// Ripple expansion duration — from the old ripple.rs (a 500ms Tween).
pub(crate) const RIPPLE_EXPAND_MS: u64 = 500;
/// Fade-out duration after release — from the old ripple.rs (a 300ms Tween).
pub(crate) const RIPPLE_FADE_MS: u64 = 300;
/// Ripple opacity — from the old ripple.rs (`ripple_opacity` 0.1).
pub(crate) const RIPPLE_OPACITY: f32 = 0.10;
/// State-layer opacity for hover/focus (from the old `background_opacity` 0.08).
pub(crate) const STATE_LAYER_HOVER: f32 = 0.08;
pub(crate) const STATE_LAYER_FOCUS: f32 = 0.12;
/// State-layer transition duration (the old hover animation, 500ms).
pub(crate) const STATE_LAYER_TRANSITION_MS: u64 = 500;
/// Focus-ring fade-in/fade-out duration (M3's focus indicator, about 150-200ms).
pub(crate) const FOCUS_INDICATOR_TRANSITION_MS: u64 = 180;

/// One ripple layer (following the layered design of the old `D:\winia ripple.rs`): each press
/// creates a layer centred on the press point (node-local coordinates, relative to the node's
/// top-left — Compose's `pressPosition`), with expansion progress 0→1. After release the layer is
/// marked `fading`, and once its opacity has faded to 0 the animation's `on_finish` removes it from
/// the list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RippleLayer {
    /// Layer id (incremented per press — reserved for multi-pointer; the model is single-pointer
    /// today).
    pub(crate) id: u64,
    /// The press point (node-local coordinates; drawing adds the layout origin to get canvas
    /// coordinates).
    pub(crate) center: (f32, f32),
    /// Expansion progress, 0..1 (driven by a 500ms Tween).
    pub(crate) progress: crate::runtime::state::State<f32>,
    /// Current opacity (0.10 on press; fades to 0 over 300ms after release).
    pub(crate) opacity: crate::runtime::state::State<f32>,
    /// Fading after release (guards against starting the fade twice).
    pub(crate) fading: bool,
}

impl RippleLayer {
    fn new(id: u64, center: (f32, f32)) -> Self {
        Self {
            id,
            center,
            progress: crate::runtime::state::State::new(0.0),
            opacity: crate::runtime::state::State::new(RIPPLE_OPACITY),
            fading: false,
        }
    }
}

/// A snapshot of a component's interaction state (material3's state combination:
/// enabled/pressed/hovered/focused/dragged). Read in one go from
/// `MutableInteractionSource::state(enabled)`, which registers the dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComponentState {
    pub enabled: bool,
    pub pressed: bool,
    pub hovered: bool,
    pub focused: bool,
    pub dragged: bool,
}

impl ComponentState {
    /// The disabled state (enabled=false — the component ignores interactions and is drawn
    /// de-emphasised).
    pub const fn disabled() -> Self {
        Self { enabled: false, pressed: false, hovered: false, focused: false, dragged: false }
    }

    /// Enabled with no interaction active.
    pub const fn idle() -> Self {
        Self { enabled: true, pressed: false, hovered: false, focused: false, dragged: false }
    }

    /// Whether the component is interactive (enabled and not visually disabled).
    pub fn is_interactive(&self) -> bool {
        self.enabled
    }

    /// Which interaction the elevation should follow, in Compose's "latest wins" order, or `None` for the
    /// default (rest) state.
    ///
    /// Compose reads `interactions.lastOrNull()`; winia keeps four independent flags, so the order below is the
    /// latest-first approximation: a drag outlives the press that started it, a press outlives a hover, and a
    /// hover outlives a focus. A DISABLED component has no interaction at all — Compose returns the disabled
    /// elevation without animating (`material3/Button.kt:975-990`) — so a caller that sees `None` here while the
    /// state is disabled should snap rather than animate away from the previous interaction.
    pub fn elevation_interaction(&self) -> Option<crate::animation::ElevationInteraction> {
        use crate::animation::ElevationInteraction as I;
        if !self.enabled {
            None
        } else if self.dragged {
            Some(I::Drag)
        } else if self.pressed {
            Some(I::Press)
        } else if self.hovered {
            Some(I::Hover)
        } else if self.focused {
            Some(I::Focus)
        } else {
            None
        }
    }
}

/// A mutable interaction source — components (Button/TextField/clickable/focusable/hoverable)
/// emit interaction events and consumers read the derived state. Compose's
/// `MutableInteractionSource()`.
///
/// It must be created through `ctx.remember(|| MutableInteractionSource::new())` (like any other
/// State: it registers with the Composer queue when created, which is what lets `set()` push a
/// recomposition).
#[derive(Clone, Debug)]
pub struct MutableInteractionSource {
    pressed: State<bool>,
    hovered: State<bool>,
    focused: State<bool>,
    dragged: State<bool>,
    /// Ripple layers (one added per press; the Ripple renderer reads them).
    ripple_layers: Arc<Mutex<Vec<RippleLayer>>>,
    /// Layer id counter.
    next_layer_id: Arc<std::sync::atomic::AtomicU64>,
    /// State-layer opacity for hover/focus (animation-driven, so a state change does not jump).
    hover_opacity: crate::runtime::state::State<f32>,
    focus_opacity: crate::runtime::state::State<f32>,
    /// Focus-ring opacity (0→1 on focus, 1→0 on blur — independent of the state layer).
    focus_indicator_alpha: crate::runtime::state::State<f32>,
}

/// Identity comparison: the same interaction source instance (stable across clones) — used for
/// Modifier parameter equality (unlike a State's "same id or same value": two different sources
/// are not equal even when their flags match).
impl PartialEq for MutableInteractionSource {
    fn eq(&self, other: &Self) -> bool {
        // Identity goes through StateId (u64 — unique from the same source as the public u32 id,
        // unified by State Phase4).
        self.pressed.state_id() == other.pressed.state_id()
            && self.hovered.state_id() == other.hovered.state_id()
            && self.focused.state_id() == other.focused.state_id()
            && self.dragged.state_id() == other.dragged.state_id()
    }
}

impl Eq for MutableInteractionSource {}

impl Default for MutableInteractionSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MutableInteractionSource {
    pub fn new() -> Self {
        Self {
            pressed: State::new(false),
            hovered: State::new(false),
            focused: State::new(false),
            dragged: State::new(false),
            ripple_layers: Arc::new(Mutex::new(Vec::new())),
            next_layer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            hover_opacity: crate::runtime::state::State::new(0.0),
            focus_opacity: crate::runtime::state::State::new(0.0),
            focus_indicator_alpha: crate::runtime::state::State::new(0.0),
        }
    }

    // Emission (Compose's emit/tryEmit).

    /// Press (the equivalent of `PressInteraction.Press` — position (0, 0)).
    pub fn emit_press(&self) {
        self.emit_press_at((0.0, 0.0));
    }

    /// Press and record the position (Compose's `PressInteraction.Press(pressPosition)`; `pos` is
    /// in **node-local coordinates**). A new ripple layer expands from the press point, its
    /// progress driven by the animation system (a 500ms Tween, as in the old ripple.rs) and stopping
    /// on its own when it completes.
    pub fn emit_press_at(&self, pos: (f32, f32)) {
        let id = self.next_layer_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.ripple_layers.lock().push(RippleLayer::new(id, pos));
        self.pressed.set(true);
        let progress = self.ripple_layers.lock().last().unwrap().progress.clone();
        crate::animation::push_animatable(
            progress,
            1.0,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(RIPPLE_EXPAND_MS),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
    }

    /// Release (the equivalent of `PressInteraction.Release`, including the cancel when a drag
    /// leaves the slop): the current layers start fading out (as in ripple.rs — opacity animates to
    /// 0 after release and `on_finish` cleans up).
    pub fn emit_release(&self) {
        let ids: Vec<u64> = {
            let mut layers = self.ripple_layers.lock();
            layers.iter_mut()
                .filter(|l| !l.fading)
                .map(|l| {
                    l.fading = true;
                    l.id
                })
                .collect()
        };
        for id in ids {
            let opacity = self.ripple_layers.lock().iter()
                .find(|l| l.id == id)
                .map(|l| l.opacity.clone());
            if let Some(opacity) = opacity {
                let src = self.clone();
                crate::animation::push_animatable_with_done(
                    opacity,
                    0.0,
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(RIPPLE_FADE_MS),
                        crate::animation::interpolator::EaseInOutCubic::new(),
                    )),
                    move || src.remove_ripple_layer(id),
                );
            }
        }
        self.pressed.set(false);
    }

    /// Fade-out completion callback: remove the layer from the list.
    fn remove_ripple_layer(&self, id: u64) {
        self.ripple_layers.lock().retain(|l| l.id != id);
    }

    /// Whether any ripple layer is still running (the event loop uses this to keep redrawing).
    #[allow(dead_code)] // the tests in this file call it
    pub(crate) fn has_active_ripples(&self) -> bool {
        !self.ripple_layers.lock().is_empty()
    }

    /// A snapshot of the ripple layers (read while rendering; centres in scene coordinates).
    pub(crate) fn ripple_layers(&self) -> Vec<RippleLayer> {
        self.ripple_layers.lock().clone()
    }

    /// Gain focus (the equivalent of `FocusInteraction.Focus`).
    pub fn emit_focus(&self) {
        self.focused.set(true);
        self.animate_state_layer(&self.focus_opacity, STATE_LAYER_FOCUS);
        self.animate_focus_indicator(1.0);
    }

    /// Lose focus (the equivalent of `FocusInteraction.Unfocus`).
    pub fn emit_unfocus(&self) {
        self.focused.set(false);
        self.animate_state_layer(&self.focus_opacity, 0.0);
        self.animate_focus_indicator(0.0);
    }

    /// Pointer hover enter (the equivalent of `HoverInteraction.Enter`) — the state layer fades in
    /// over 500ms.
    pub fn emit_hover_enter(&self) {
        self.hovered.set(true);
        self.animate_state_layer(&self.hover_opacity, STATE_LAYER_HOVER);
    }

    /// Pointer hover exit (the equivalent of `HoverInteraction.Exit`) — the state layer fades out
    /// over 500ms.
    pub fn emit_hover_exit(&self) {
        self.hovered.set(false);
        self.animate_state_layer(&self.hover_opacity, 0.0);
    }

    /// Animate the state layer's opacity (a smooth hover/focus transition — the old 500ms Tween).
    fn animate_state_layer(&self, state: &crate::runtime::state::State<f32>, target: f32) {
        crate::animation::push_animatable(
            state.clone(),
            target,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(STATE_LAYER_TRANSITION_MS),
                crate::animation::interpolator::EaseInOutCubic::new(),
            )),
        );
    }

    /// Animate the focus ring's opacity (the M3 focus indicator fading in/out).
    fn animate_focus_indicator(&self, target: f32) {
        crate::animation::push_animatable(
            self.focus_indicator_alpha.clone(),
            target,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(FOCUS_INDICATOR_TRANSITION_MS),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
    }

    /// The hover state layer's current opacity (read while rendering; an animated value).
    pub(crate) fn hover_opacity_value(&self) -> f32 {
        self.hover_opacity.peek()
    }

    /// The focus state layer's current opacity (read while rendering; an animated value).
    pub(crate) fn focus_opacity_value(&self) -> f32 {
        self.focus_opacity.peek()
    }

    /// The focus ring's current opacity (read while rendering; an animated value).
    pub(crate) fn focus_indicator_alpha_value(&self) -> f32 {
        self.focus_indicator_alpha.peek()
    }

    /// Source identity proxy (for node_key — the pressed State's id stands in for the identity).
    /// Stable across clones (State::Clone shares the Arc). Note that it is not equivalent to
    /// `PartialEq` (which compares all four State ids, while this takes `pressed` as the proxy —
    /// the fields are private, `new()` creates all four ids together and `clone()` shares all four
    /// Arcs, so in practice they agree; spelled out by P2-3). `State::id()` keeps a u32 for the
    /// transition (wrapping past 2^32 is unreachable in practice) and will move to the u64
    /// `state_id()` later.
    pub fn source_id(&self) -> u32 {
        self.pressed.id()
    }

    /// Drag start (the equivalent of `DragInteraction.Start`).
    pub fn emit_drag_start(&self) {
        self.dragged.set(true);
    }

    /// Drag end or cancel (the equivalent of `DragInteraction.Stop`/`Cancel`).
    pub fn emit_drag_end(&self) {
        self.dragged.set(false);
    }

    // Derived-state reads (they register composition dependencies, so a state change recomposes).

    pub fn is_pressed(&self) -> bool {
        self.pressed.get()
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered.get()
    }

    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }

    pub fn is_dragged(&self) -> bool {
        self.dragged.get()
    }

    // Render-time reads (`peek`, no dependency registered — for `DrawNode::draw` and render
    // closures, P1-4). Rendering does not happen inside a dependency frame, so `get` could not
    // register anyway; `peek` is the honest semantics and stops a future render-in-frame from
    // registering the wrong dependency. build/measure still use `get`, which must register.

    /// Render-time focused read (`peek`).
    pub(crate) fn is_focused_value(&self) -> bool {
        self.focused.peek()
    }


    /// Read all the state at once (matching material3's enabled/pressed/hovered/focused
    /// combination).
    pub fn state(&self, enabled: bool) -> ComponentState {
        ComponentState {
            enabled,
            pressed: self.pressed.get(),
            hovered: self.hovered.get(),
            focused: self.focused.get(),
            dragged: self.dragged.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;

    #[test]
    fn test_source_transitions() {
        let s = MutableInteractionSource::new();
        assert_eq!(s.state(true), ComponentState::idle());
        s.emit_press();
        assert!(s.is_pressed());
        assert!(!s.is_hovered());
        s.emit_hover_enter();
        assert!(s.is_hovered());
        s.emit_release();
        assert!(!s.is_pressed());
        assert!(s.is_hovered(), "release does not affect hover");
        s.emit_hover_exit();
        assert!(!s.is_hovered());
        s.emit_focus();
        assert!(s.is_focused());
        s.emit_drag_start();
        assert!(s.is_dragged());
        s.emit_drag_end();
        s.emit_unfocus();
        assert_eq!(s.state(true), ComponentState::idle());
        assert_eq!(s.state(false).enabled, false);
    }

    #[test]
    fn test_source_emit_triggers_recompose() {
        // A source created by `remember`: reads register dependencies and emits push into the
        // composer's pending set — verifying the "a state change recomposes the consumer" loop.
        let mut composer = Composer::new();
        let mut holder: Option<MutableInteractionSource> = None;
        let mut reads = 0i32;
        composer.compose(|ctx| {
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let _ = src.is_pressed(); // Registers the dependency (simulating a read in Button::build).
            holder = Some(src.clone());
            reads += 1;
        });
        let src = holder.expect("compose should run once");
        src.emit_press(); // Change → notify → pending.
        assert!(composer.has_pending_states(), "set should push into the composer pending queue");
        let before = reads;
        composer.recompose(|ctx| {
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let _ = src.is_pressed();
            reads += 1;
        });
        assert!(reads > before, "the slot that read is_pressed should be marked dirty and re-run");
        assert!(src.is_pressed());
    }

    #[test]
    fn test_ripple_layers_press_release() {
        let s = MutableInteractionSource::new();
        // Not pressed: no layers.
        assert!(!s.has_active_ripples());
        // Press: one layer is created, centred on the press point.
        s.emit_press_at((12.0, 34.0));
        assert!(s.is_pressed());
        let layers = s.ripple_layers();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].center, (12.0, 34.0));
        assert_eq!(layers[0].opacity.get(), RIPPLE_OPACITY, "ripple starts at opacity 0.10");
        assert!(!layers[0].fading);
        assert!(s.has_active_ripples());
        // Release: fading starts (the progress/opacity animations are driven by the animation
        // system and covered by the animation tests).
        s.emit_release();
        assert!(!s.is_pressed());
        assert!(s.ripple_layers()[0].fading);
    }

    #[test]
    fn test_ripple_removed_after_fade() {
        // The cleanup path of the fade-out animation's `on_finish`: remove_ripple_layer removes by
        // id.
        let s = MutableInteractionSource::new();
        s.emit_press_at((1.0, 2.0));
        let id = s.ripple_layers()[0].id;
        s.remove_ripple_layer(id);
        assert!(!s.has_active_ripples(), "the layer should be cleaned up once the fade completes");
    }

    #[test]
    fn test_focus_indicator_alpha_api() {
        let s = MutableInteractionSource::new();
        assert_eq!(s.focus_indicator_alpha_value(), 0.0, "no focus ring initially");
        s.emit_focus();   // Starts the fade-in animation (the global animation system advances it).
        s.emit_unfocus(); // Starts the fade-out animation.
    }
}
