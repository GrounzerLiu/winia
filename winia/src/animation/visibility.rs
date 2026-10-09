//! Enter/exit transition configuration — the spec half of `AnimatedVisibility` and of the
//! shared-transition endpoints.
//!
//! It is a motion spec, not a component: the Modifier chain carries it, `nav.rs` stores it per
//! transition and `shared_transition` pairs them. It lived in `components/animated_visibility.rs`,
//! which meant the toolkit named a component to describe how something fades.

use crate::animation::AnimationSpec;

/// Slide direction (`VisibilityTransition::slide_in/slide_out`).
/// Combined with [`SlideOffset`]: direction picks the axis+sign, offset picks the distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Slide distance (cf. Compose `slideInHorizontally(initialOffsetX: (fullWidth) -> Int)`).
///
/// Compose takes a lambda over the content size; the common cases are a fixed offset or a fraction
/// of it, which these two variants express without a closure. Both users of a slide share the type:
/// [`AnimatedVisibility`](crate::components::animated_visibility) and the navigation scene
/// transitions in [`crate::nav`], which had a second, near-identical enum of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlideOffset {
    /// Fixed logical pixels — the default 48, and the M3 shared-axis 30.
    Fixed(f32),
    /// Fraction of the content extent along the slide axis (1.0 = a full slide-in, the Compose
    /// `initialOffsetX = { fullWidth }`; -0.3 = the reverse parallax `{ -it / 3 }`).
    Fraction(f32),
}

impl SlideOffset {
    /// The distance to slide, given the container's extent **along the slide axis** — the caller
    /// knows whether that is the width or the height.
    pub fn resolve(&self, extent: f32) -> f32 {
        match self {
            SlideOffset::Fixed(px) => *px,
            SlideOffset::Fraction(f) => f * extent,
        }
    }
}

impl Default for SlideOffset {
    fn default() -> Self {
        Self::Fixed(48.0)
    }
}

/// Vertical expand anchor (cf. Compose `expandVertically(expandFrom: Alignment.Top)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandFrom {
    /// Content grows downward from the top (status quo).
    #[default]
    Top,
    /// Content grows upward from the bottom.
    Bottom,
}

/// Horizontal expand anchor (cf. Compose `expandHorizontally(expandFrom: Alignment.Start)`).
/// NOTE: Start is unconditionally the left edge and End unconditionally the right edge
/// (no RTL mirroring — Compose parity backlog; RTL callers pick the anchor explicitly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandFromH {
    /// Content grows rightward from the start (left) edge.
    #[default]
    Start,
    /// Content grows leftward from the end (right) edge.
    End,
}

/// Enter/exit transition config: fade/slide/expand/scale effects + animation spec.
/// Combine with `with_*` chaining (cf. Compose `fadeIn() + expandVertically()`).
#[derive(Debug, Clone)]
pub struct VisibilityTransition {
    /// Fade in/out (alpha 0<->1)
    pub fade: bool,
    /// Alpha the fade starts from, i.e. the alpha at `progress == 0` (cf. Compose
    /// `fadeIn(initialAlpha)` — the material3 date picker's year panel comes in from 0.6). Ignored
    /// when `fade` is off, which is also what makes it safe to leave at the default.
    pub alpha_from: f32,
    /// Slide in/out (direction + distance)
    pub slide: Option<(SlideDirection, SlideOffset)>,
    /// Vertical expand/shrink (container height 0<->full, layout layer, followers move along)
    pub expand: bool,
    /// Vertical expand anchor
    pub expand_from: ExpandFrom,
    /// Horizontal expand/shrink (container width 0<->full, layout layer)
    pub expand_h: bool,
    /// Horizontal expand anchor
    pub expand_from_h: ExpandFromH,
    /// Scale (scale_from<->1.0 around transform_origin)
    pub scale: bool,
    /// Scale start value (status quo 0.8 — cf. Compose `scaleIn(initialScale)`)
    pub scale_from: f32,
    /// Scale pivot, normalized (0.5, 0.5) = center (cf. Compose `transformOrigin`)
    pub transform_origin: (f32, f32),
    /// Animation spec
    pub spec: AnimationSpec,
    /// Per-channel specs. Compose's enter/exit transitions animate each channel with the spec its own
    /// builder carries (`fadeIn(tween(delayMillis = 100)) + expandIn(spring)`), so the channels can run
    /// out of step; `None` = use `spec`, which is what every builder sets as well, so a transition built
    /// the old way behaves exactly as before.
    pub fade_spec: Option<AnimationSpec>,
    /// The SIZE channel's spec — Compose animates one `IntSize` for both expand axes
    /// (`ChangeSize`), so vertical and horizontal expansion share it.
    pub size_spec: Option<AnimationSpec>,
    pub slide_spec: Option<AnimationSpec>,
    pub scale_spec: Option<AnimationSpec>,
}

impl VisibilityTransition {
    /// No-op transition (all channels off — slide/scale-only customs without
    /// fade, or sharedBounds endpoints that ride the flight opaquely).
    pub fn empty() -> Self {
        Self {
            fade: false,
            alpha_from: 0.0,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Tween(Default::default()),
            fade_spec: None,
            size_spec: None,
            slide_spec: None,
            scale_spec: None,
        }
    }

    fn base(spec: AnimationSpec) -> Self {
        Self {
            fade: false,
            alpha_from: 0.0,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec,
            fade_spec: None,
            size_spec: None,
            slide_spec: None,
            scale_spec: None,
        }
    }
    pub fn fade_in(spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        Self { fade: true, fade_spec: Some(spec.clone()), ..Self::base(spec) }
    }
    pub fn fade_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::fade_in(spec)
    }
    pub fn expand_in(spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        Self { expand: true, size_spec: Some(spec.clone()), ..Self::base(spec) }
    }
    pub fn shrink_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in(spec)
    }
    /// Horizontal expand (cf. Compose `expandHorizontally`).
    pub fn expand_in_h(spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        Self { expand_h: true, size_spec: Some(spec.clone()), ..Self::base(spec) }
    }
    /// Horizontal shrink (cf. Compose `shrinkHorizontally`).
    pub fn shrink_out_h(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in_h(spec)
    }
    pub fn slide_in(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        Self { slide: Some((dir, SlideOffset::default())), slide_spec: Some(spec.clone()), ..Self::base(spec) }
    }
    pub fn slide_out(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self::slide_in(dir, spec)
    }
    /// Slide with explicit distance (cf. Compose `initialOffsetX/Y` lambda).
    pub fn slide_in_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        let spec = spec.into();
        Self { slide: Some((dir, offset)), slide_spec: Some(spec.clone()), ..Self::base(spec) }
    }
    pub fn slide_out_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        Self::slide_in_offset(dir, offset, spec)
    }
    pub fn scale_in(spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        Self { scale: true, scale_spec: Some(spec.clone()), ..Self::base(spec) }
    }
    pub fn scale_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::scale_in(spec)
    }

    /// The spec the FADE channel runs on: its own if a builder gave it one, otherwise the shared one.
    pub fn fade_spec(&self) -> &AnimationSpec {
        self.fade_spec.as_ref().unwrap_or(&self.spec)
    }

    /// The spec the SIZE channel runs on (both expand axes — Compose animates one `IntSize`).
    pub fn size_spec(&self) -> &AnimationSpec {
        self.size_spec.as_ref().unwrap_or(&self.spec)
    }

    pub fn slide_spec(&self) -> &AnimationSpec {
        self.slide_spec.as_ref().unwrap_or(&self.spec)
    }

    pub fn scale_spec(&self) -> &AnimationSpec {
        self.scale_spec.as_ref().unwrap_or(&self.spec)
    }

    /// Combines two transitions the way Compose's `+` does: the channels are unioned, and each channel
    /// keeps the spec of the side that turned it on (this receiver first — `self` wins when both enable
    /// the same channel). The shared `spec` stays this side's.
    ///
    /// This is what makes per-channel timing expressible:
    /// `VisibilityTransition::fade_in(delayed_tween) + VisibilityTransition::expand_in(spring)` fades
    /// late while the size change starts at once.
    pub fn plus(self, other: VisibilityTransition) -> VisibilityTransition {
        VisibilityTransition {
            fade: self.fade || other.fade,
            alpha_from: if self.fade { self.alpha_from } else { other.alpha_from },
            slide: self.slide.or(other.slide),
            expand: self.expand || other.expand,
            expand_from: if self.expand { self.expand_from } else { other.expand_from },
            expand_h: self.expand_h || other.expand_h,
            expand_from_h: if self.expand_h { self.expand_from_h } else { other.expand_from_h },
            scale: self.scale || other.scale,
            scale_from: if self.scale { self.scale_from } else { other.scale_from },
            transform_origin: if self.scale { self.transform_origin } else { other.transform_origin },
            spec: self.spec.clone(),
            fade_spec: self.fade_spec.or(other.fade_spec),
            size_spec: self.size_spec.or(other.size_spec),
            slide_spec: self.slide_spec.or(other.slide_spec),
            scale_spec: self.scale_spec.or(other.scale_spec),
        }
    }
    /// Combine: overlay fade in/out
    pub fn with_fade(mut self) -> Self {
        self.fade = true;
        self
    }
    /// Combine: fade in from `initial_alpha` rather than transparent (cf. Compose
    /// `fadeIn(initialAlpha)` — the material3 date picker overlays its year panel from 0.6).
    pub fn with_alpha_from(mut self, initial_alpha: f32) -> Self {
        self.fade = true;
        self.alpha_from = initial_alpha;
        self
    }
    /// Combine: overlay vertical expand/shrink
    pub fn with_expand(mut self) -> Self {
        self.expand = true;
        self
    }
    /// Combine: overlay vertical expand/shrink with explicit anchor
    pub fn with_expand_from(mut self, from: ExpandFrom) -> Self {
        self.expand = true;
        self.expand_from = from;
        self
    }
    /// Combine: overlay horizontal expand/shrink
    pub fn with_expand_h(mut self) -> Self {
        self.expand_h = true;
        self
    }
    /// Combine: overlay horizontal expand/shrink with explicit anchor
    pub fn with_expand_h_from(mut self, from: ExpandFromH) -> Self {
        self.expand_h = true;
        self.expand_from_h = from;
        self
    }
    /// Combine: overlay slide (default 48px distance)
    pub fn with_slide(mut self, dir: SlideDirection) -> Self {
        self.slide = Some((dir, SlideOffset::default()));
        self
    }
    /// Combine: overlay slide with explicit distance
    pub fn with_slide_offset(mut self, dir: SlideDirection, offset: SlideOffset) -> Self {
        self.slide = Some((dir, offset));
        self
    }
    /// Combine: overlay scale (default 0.8 from center)
    pub fn with_scale(mut self) -> Self {
        self.scale = true;
        self
    }
    /// Combine: overlay scale with explicit start value and pivot
    /// (cf. Compose `scaleIn(initialScale, transformOrigin)`).
    pub fn with_scale_from(mut self, scale_from: f32, transform_origin: (f32, f32)) -> Self {
        self.scale = true;
        self.scale_from = scale_from;
        self.transform_origin = transform_origin;
        self
    }
    /// The draw-layer parameters this transition produces, for a content box of `content_extent`.
    ///
    /// `values` is how SETTLED each channel is: `1.0` is the resting state (whatever the transition's
    /// flags are, they contribute nothing), `0.0` is the far end — an enter that has not started, or an
    /// exit that has finished. An entering content therefore runs its progress 0 → 1 and a leaving one
    /// runs 1 → 0, both through this one function; that is what lets a crossfade's two halves be the
    /// same code with the two different transitions. Compose animates each channel on its own clock, so
    /// the channels are passed in separately; a single-clock caller passes `p` (or
    /// `ChannelValues::uniform(p)`) and every channel moves together.
    ///
    /// The content extent is only needed by a `SlideOffset::Fraction` slide, which resolves
    /// against it.
    pub fn layer_params(
        &self,
        values: impl Into<ChannelValues>,
        content_extent: (f32, f32),
    ) -> crate::graphics::GraphicsLayerParams {
        let values = values.into();
        let p = values.fade;
        let mut params = crate::graphics::GraphicsLayerParams::default();
        if self.fade {
            // `alpha_from` is the alpha at progress 0 (Compose's `fadeIn(initialAlpha)`); 0.0 is the
            // plain fade, so a transition that never sets it behaves exactly as before.
            params.alpha = self.alpha_from + (1.0 - self.alpha_from) * p;
        }
        if self.scale {
            let s = self.scale_from + (1.0 - self.scale_from) * values.scale;
            params.scale_x = s;
            params.scale_y = s;
            params.transform_origin =
                crate::graphics::TransformOrigin(self.transform_origin.0, self.transform_origin.1);
        }
        if let Some((dir, offset)) = self.slide {
            // The extent along the slide axis; `SlideOffset::resolve` does the rest.
            let (w, h) = content_extent;
            let extent = match dir {
                SlideDirection::Left | SlideDirection::Right => w,
                SlideDirection::Up | SlideDirection::Down => h,
            };
            let dist = offset.resolve(extent);
            let off = (1.0 - values.slide) * dist;
            match dir {
                SlideDirection::Left => params.translation_x = -off,
                SlideDirection::Right => params.translation_x = off,
                SlideDirection::Up => params.translation_y = -off,
                SlideDirection::Down => params.translation_y = off,
            }
        }
        params
    }
}

impl std::ops::Add for VisibilityTransition {
    type Output = VisibilityTransition;

    /// `fadeIn(spec) + expandIn(spec)` — Compose's combination operator.
    fn add(self, other: VisibilityTransition) -> VisibilityTransition {
        self.plus(other)
    }
}

/// The per-channel values a transition converts into layer parameters. Compose animates each channel
/// on its own clock (`EnterExitTransition.kt`'s `Fade`/`Slide`/`Scale`/`ChangeSize` each carry an
/// `animationSpec`), so they are passed separately here; a single-clock caller — `AnimatedContent`, or
/// an `AnimatedVisibility` whose channels share one spec — passes `ChannelValues::uniform(p)`, and a
/// `f32` converts to that automatically.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelValues {
    /// The fade channel (Compose's `Fade.alpha`).
    pub fade: f32,
    /// The slide channel (Compose's `Slide.slideOffset`).
    pub slide: f32,
    /// The scale channel (Compose's `Scale.scale`).
    pub scale: f32,
}

impl ChannelValues {
    /// One progress for every channel.
    pub fn uniform(progress: f32) -> Self {
        Self { fade: progress, slide: progress, scale: progress }
    }
}

impl From<f32> for ChannelValues {
    fn from(progress: f32) -> Self {
        Self::uniform(progress)
    }
}

impl Default for VisibilityTransition {
    /// Compose's default enter/exit pair. Every `AnimatedVisibility` overload defaults to a FADE PLUS A
    /// SIZE CHANGE — `fadeIn() + expandIn()` at the top level (`AnimatedVisibility.kt:125-129`),
    /// `fadeIn() + expandVertically()` in a `ColumnScope` (`:272-276`),
    /// `fadeIn() + expandHorizontally()` in a `RowScope` (`:199-203`); the exits are the matching
    /// `fadeOut() + shrink*`. winia used to fade only, so a plain `AnimatedVisibility::new(v)` left its
    /// siblings in place where Compose moves them (measured: the container's height stayed at the full
    /// content height for the whole transition).
    ///
    /// The spec is Compose's channel default, `spring(stiffness = StiffnessMediumLow)` = 400
    /// (`EnterExitTransition.kt:304-306` for the fade, `:529-535` for the size change, which adds
    /// `visibilityThreshold = IntSize.VisibilityThreshold`).
    ///
    /// Two deliberate differences: winia expands BOTH axes by default (Compose picks the axis from the
    /// scope — a Row expands horizontally, a Column vertically — and winia has one default plus the
    /// explicit `expand_in_h`/`with_expand_h` builders), and the shared `spec` below doubles as the
    /// fallback for every channel. Per-channel specs ARE supported now — build the transition out of
    /// builders and combine them (`fade_in(spec) + expand_in(other)`), or set one channel's spec
    /// directly; a channel without its own spec uses this shared one, which is what a transition built
    /// the old way (`fade_in(tween).with_expand()`) relies on.
    fn default() -> Self {
        Self {
            fade: true,
            alpha_from: 0.0,
            slide: None,
            expand: true,
            expand_from: ExpandFrom::Top,
            expand_h: true,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Spring(crate::animation::SpringSpec {
                stiffness: crate::animation::SpringSpec::STIFFNESS_MEDIUM,
                threshold: 0.01,
                ..crate::animation::SpringSpec::default()
            }),
            fade_spec: None,
            size_spec: None,
            slide_spec: None,
            scale_spec: None,
        }
    }
}

