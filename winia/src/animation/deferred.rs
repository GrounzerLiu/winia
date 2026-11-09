//! Compose's `DeferredTargetAnimation` (`animation-core/DeferredTargetAnimation.kt`).
//!
//! An animation value that is only created when the first target arrives. A component that cannot
//! know its initial value at construction (an anchored draggable before it has measured, a sheet
//! whose content decides the first stop) holds one of these in a field and calls
//! [`DeferredTargetAnimation::update_target`]; the first call creates the value AT the target
//! without moving anything, every later call starts or retargets the motion.
//!
//! Compose's version takes a `TwoWayConverter` because `Animatable` is generic over the vector;
//! winia's [`AnimatableValue`] already owns its components, so the type parameter is the value only.

use super::*;

/// A lazily-created animation value (Compose's `DeferredTargetAnimation`).
pub struct DeferredTargetAnimation<T: AnimatableValue + Send + Sync + 'static> {
    state: Option<State<T>>,
    target: Option<T>,
}

impl<T: AnimatableValue + Send + Sync + 'static> Default for DeferredTargetAnimation<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AnimatableValue + Send + Sync + 'static> DeferredTargetAnimation<T> {
    pub fn new() -> Self {
        Self {
            state: None,
            target: None,
        }
    }

    /// Compose's `updateTarget`: returns the current value of the animation, or `None` on the very
    /// first call - the value is created AT the target, so nothing animates and there is no
    /// previous value to report. A call with the same target is a no-op (it does not restart the
    /// motion).
    pub fn update_target(&mut self, target: T, spec: impl Into<AnimationSpec>) -> Option<T> {
        match &self.state {
            None => {
                self.state = Some(State::new(target.clone()));
                self.target = Some(target);
                None
            }
            Some(state) => {
                if self.target.as_ref() != Some(&target) {
                    self.target = Some(target.clone());
                    push_animatable(state.clone(), target, spec.into());
                }
                Some(state.peek())
            }
        }
    }

    /// The current value, `None` before the first [`Self::update_target`].
    pub fn value(&self) -> Option<T> {
        self.state.as_ref().map(|s| s.peek())
    }

    /// Whether a motion is currently running for this value.
    pub fn is_running(&self) -> bool {
        self.state
            .as_ref()
            .map(|s| is_animating_state(s.state_id()))
            .unwrap_or(false)
    }

    /// Compose's `reset`: forget the value, so the next target creates it again.
    pub fn reset(&mut self) {
        if let Some(state) = &self.state {
            clear_animations_for_states(&[state.state_id()]);
        }
        self.state = None;
        self.target = None;
    }
}
