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
}

impl VisibilityTransition {
    /// No-op transition (all channels off — slide/scale-only customs without
    /// fade, or sharedBounds endpoints that ride the flight opaquely).
    pub fn empty() -> Self {
        Self {
            fade: false,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Tween(Default::default()),
        }
    }

    fn base(spec: AnimationSpec) -> Self {
        Self {
            fade: false,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec,
        }
    }
    pub fn fade_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: true, ..Self::base(spec.into()) }
    }
    pub fn fade_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::fade_in(spec)
    }
    pub fn expand_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { expand: true, ..Self::base(spec.into()) }
    }
    pub fn shrink_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in(spec)
    }
    /// Horizontal expand (cf. Compose `expandHorizontally`).
    pub fn expand_in_h(spec: impl Into<AnimationSpec>) -> Self {
        Self { expand_h: true, ..Self::base(spec.into()) }
    }
    /// Horizontal shrink (cf. Compose `shrinkHorizontally`).
    pub fn shrink_out_h(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in_h(spec)
    }
    pub fn slide_in(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self { slide: Some((dir, SlideOffset::default())), ..Self::base(spec.into()) }
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
        Self { slide: Some((dir, offset)), ..Self::base(spec.into()) }
    }
    pub fn slide_out_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        Self::slide_in_offset(dir, offset, spec)
    }
    pub fn scale_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { scale: true, ..Self::base(spec.into()) }
    }
    pub fn scale_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::scale_in(spec)
    }
    /// Combine: overlay fade in/out
    pub fn with_fade(mut self) -> Self {
        self.fade = true;
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
    /// (cf. Compose `scaleIn(initialScale, transformOrigin)`)
    pub fn with_scale_from(mut self, scale_from: f32, transform_origin: (f32, f32)) -> Self {
        self.scale = true;
        self.scale_from = scale_from;
        self.transform_origin = transform_origin;
        self
    }
}

impl Default for VisibilityTransition {
    fn default() -> Self {
        Self {
            fade: true,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Tween(Default::default()),
        }
    }
}

