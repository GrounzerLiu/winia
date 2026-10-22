#[derive(Clone)]
pub struct AnchorSlide(pub std::sync::Arc<dyn Fn() -> f32 + Send + Sync>);

impl AnchorSlide {
    /// Create from a progress source (0 = at the anchor, 1 = at the window corner).
    pub fn new(progress: std::sync::Arc<dyn Fn() -> f32 + Send + Sync>) -> Self {
        Self(progress)
    }

    /// Progress, clamped to the animatable range.
    pub fn progress(&self) -> f32 {
        (self.0)().clamp(0.0, 1.0)
    }
}

/// Interpolate from the anchor's corner to the window's corner for one axis — Compose's
/// `lerp(collapsedBounds.<axis>, 0, progress)`.
pub fn anchor_slide_lerp(anchor_axis: f32, progress: f32) -> f32 {
    anchor_axis * (1.0 - progress.clamp(0.0, 1.0))
}

/// The point an anchored overlay scales out of, as fractions of its own box — material3's
/// `calculateTransformOrigin(anchorBounds, menuBounds)` (`material3/Menu.kt`), which is what makes a
/// dropdown menu look like it grows out of the control that opened it instead of out of its centre.
///
/// M3's branches, verbatim: a menu entirely beside the anchor pins that axis to its near edge (`0` when
/// the menu starts past the anchor's end, `1` when it ends before the anchor starts); a menu that
/// overlaps the anchor on an axis uses the middle of the two boxes' intersection; a zero-sized menu
/// falls back to `0`.
pub fn overlay_transform_origin(
    anchor: (f32, f32, f32, f32),
    menu: (f32, f32, f32, f32),
) -> (f32, f32) {
    let (ax, ay, aw, ah) = anchor;
    let (mx, my, mw, mh) = menu;
    let pivot_x = if mx >= ax + aw {
        0.0
    } else if mx + mw <= ax {
        1.0
    } else if mw == 0.0 {
        0.0
    } else {
        let intersection_center = (ax.max(mx) + (ax + aw).min(mx + mw)) / 2.0;
        (intersection_center - mx) / mw
    };
    let pivot_y = if my >= ay + ah {
        0.0
    } else if my + mh <= ay {
        1.0
    } else if mh == 0.0 {
        0.0
    } else {
        let intersection_center = (ay.max(my) + (ay + ah).min(my + mh)) / 2.0;
        (intersection_center - my) / mh
    };
    (pivot_x, pivot_y)
}

/// The height an entering/closing overlay is clipped to, or `None` for "no clip".
///
/// Two render-time rules, both easy to get wrong:
/// - a slide (`dy != 0`) clips to the FULL height, because the translation is what moves the panel;
/// - a reveal clips to `height * reveal`, and once `reveal` reaches 1 there is NO clip at all.
///
/// "No clip" must stay distinguishable from "clip to zero height": the first is a settled overlay, the
/// second is an invisible one. Reporting the sentinel as `0` made a fully open docked dropdown read as
/// empty in the render trace (measured: `painted_h=0` while `layout_h=280`).
pub fn overlay_reveal_clip_height(height: f32, dy: f32, reveal: f32) -> Option<f32> {
    if dy != 0.0 {
        Some(height)
    } else if reveal < 1.0 {
        Some((height * reveal).max(0.0))
    } else {
        None
    }
}


/// Overlay **enter animation** spec (mirrors Compose content-layer
/// `AnimatedVisibility` `enter = scaleIn + fadeIn` — Compose Dialog itself has no
/// built-in animation; material2's fixed scale+fade lives in the content layer;
/// Winia pushes the animation down to the overlay container layer, driven
/// uniformly per frame without depending on content-layer animation components).
///
/// - `scale_from`: starting scale (default 0.8 — Compose `scaleIn(initialScale=0.8f)`)
/// - `fade`: whether to fade in (default true — Compose `fadeIn()`)
/// - `slide_from_y`: starting vertical offset as a multiple of overlay height;
///   negative = slide in from above — e.g. docked dropdown
///   `slideIn(initialOffset = { IntOffset(0, -it.height / 2) })`;
///   default 0.0 = no offset
/// - `reveal_top`: reveal from the top (clip height 0 → full — approximates the
///   fullscreen search bounds-morph expand; true shared-element morph needs
///   anchor geometry, out of scope for this mechanism; default false)
/// - `duration`: duration (default 200ms)
/// - `interpolator`: easing curve (default EaseOutCubic — Compose `easeOut`)
///
/// `None` (`OverlayDesc.enter_anim / exit_anim = None`) = no animation (instant
/// appear / disappear — default for menu-like popups).
#[derive(Clone)]
pub struct OverlayAnimSpec {
    pub(crate) scale_from: f32,
    pub(crate) fade: bool,
    pub(crate) slide_from_y: f32,
    pub(crate) reveal_top: bool,
    pub(crate) duration: std::time::Duration,
    pub(crate) delay: std::time::Duration,
    pub(crate) interpolator: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>,
    pub(crate) anchor_pivot: bool,
}

impl OverlayAnimSpec {
    pub(crate) fn animation_spec(&self) -> crate::animation::AnimationSpec {
        let delay_ms = self.delay.as_millis() as u64;
        if delay_ms == 0 {
            return crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                self.duration,
                self.interpolator.clone(),
            ));
        }
        let duration_ms = self.duration.as_millis() as u64;
        let total = (duration_ms + delay_ms) as f32;
        let held = delay_ms as f32 / total;
        // Dense sampling with LINEAR segments: a curved segment interpolator replays the curve once per
        // segment (the sawtooth measured on the SearchBar expansion).
        let steps = 96usize;
        let mut frames: Vec<(f32, f32)> = Vec::with_capacity(steps + 2);
        frames.push((0.0, 0.0));
        frames.push((held, 0.0));
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            frames.push((held + (1.0 - held) * t, self.interpolator.interpolate(t)));
        }
        crate::animation::AnimationSpec::Keyframes(crate::animation::KeyframesSpec::new(
            std::time::Duration::from_millis(duration_ms + delay_ms),
            frames,
        ))
    }
    /// Default enter animation (scale 0.8 -> 1 + fade, 200ms EaseOutCubic — the
    /// classic material2 Dialog open effect).
    pub fn default_enter() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            reveal_top: false,
            duration: std::time::Duration::from_millis(200),
            delay: std::time::Duration::ZERO,
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
            anchor_pivot: false,
        }
    }

    /// Default exit animation (scale 1 -> 0.8 + fade out, 200ms EaseInCubic —
    /// reverse of enter; mirrors Compose `AnimatedVisibility(exit = scaleOut + fadeOut)`).
    pub fn default_exit() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            reveal_top: false,
            duration: std::time::Duration::from_millis(200),
            delay: std::time::Duration::ZERO,
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseInCubic::new()),
            anchor_pivot: false,
        }
    }

    /// Scale only (no fade).
    pub fn scale_only(from: f32, duration: std::time::Duration) -> Self {
        Self { scale_from: from, fade: false, slide_from_y: 0.0, reveal_top: false, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    /// Fade only.
    pub fn fade_only(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, reveal_top: false, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    /// Dropdown slide + fade (slide_in y=-height/2 with fade — mirrors docked
    /// dropdown `slideIn(-height/2) + fadeIn`).
    pub fn slide_down(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: -0.5, reveal_top: false, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    /// Hold the start value for `delay` before the animation runs (Compose's `delayMillis`). See the field
    /// doc: the overlay layer turns this into a hold-then-curve spec, so it must NOT be folded into
    /// `duration` by the caller.
    pub fn delay(mut self, d: std::time::Duration) -> Self {
        self.delay = d;
        self
    }

    /// Custom easing curve.
    pub fn with_interpolator(mut self, interp: impl crate::animation::interpolator::Interpolator + 'static) -> Self {
        self.interpolator = std::sync::Arc::new(interp);
        self
    }

    /// Starting scale (close to 0 = dialog grows from center; 1.0 = no scale).
    pub fn scale_from(mut self, v: f32) -> Self {
        self.scale_from = v;
        self
    }

    /// Starting vertical offset as a multiple of overlay height (-0.5 = slide
    /// in from half height above).
    pub fn slide_from_y(mut self, v: f32) -> Self {
        self.slide_from_y = v;
        self
    }

    /// Reveal from top (clip height 0 -> full).
    pub fn reveal_top(mut self, v: bool) -> Self {
        self.reveal_top = v;
        self
    }

    /// Fade toggle.
    pub fn fade(mut self, v: bool) -> Self {
        self.fade = v;
        self
    }

    /// Duration.
    pub fn duration(mut self, d: std::time::Duration) -> Self {
        self.duration = d;
        self
    }

    /// Expand reveal + fade (approximates the fullscreen search bounds-morph
    /// expand — true shared-element morph needs anchor geometry; 300-400ms
    /// with EaseOutCubic lands crisply).
    pub fn expand_fade(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, reveal_top: true, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    pub(crate) fn apply(&self, t: f32) -> (f32, f32, f32, f32) {
        let e = self.interpolator.interpolate(t.clamp(0.0, 1.0));
        let scale = self.scale_from + (1.0 - self.scale_from) * e;
        let alpha = if self.fade { e } else { 1.0 };
        let dy = self.slide_from_y * (1.0 - e);
        let reveal = if self.reveal_top { e } else { 1.0 };
        (scale, alpha, dy, reveal)
    }

    pub(crate) fn apply_exit(&self, t: f32) -> (f32, f32, f32, f32) {
        self.apply(t)
    }
}

impl Default for OverlayAnimSpec {
    fn default() -> Self { Self::default_enter() }
}

