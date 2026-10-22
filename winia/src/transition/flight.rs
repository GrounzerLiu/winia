use super::*;

impl Flight {
    pub(crate) fn new(id: FlightId, scope_id: u64, key: String) -> Self {
        Self {
            id,
            scope_id,
            key,
            phase: FlightPhase::Idle,
            start: None,
            end: None,
            progress: 0.0,
            generation: 0,
            source_slot: None,
            target_slot: None,
        }
    }

    /// Pure state transition. Stray events are `Noop` (never panic — the
    /// coordinator may deliver late events from a previous generation).
    pub(crate) fn on_event(&mut self, ev: FlightEvent) -> Vec<FlightAction> {
        match (&self.phase, ev) {
            (FlightPhase::Idle, FlightEvent::CounterpartAppeared { source_slot, target_slot }) => {
                self.phase = FlightPhase::AwaitingBounds;
                self.source_slot = Some(source_slot);
                self.target_slot = Some(target_slot);
                vec![FlightAction::CollectBounds { source_slot, target_slot }]
            }
            (FlightPhase::AwaitingBounds, FlightEvent::BoundsReady { start, end }) => {
                self.phase = FlightPhase::Flying;
                self.start = Some(start);
                self.end = Some(end);
                self.progress = 0.0;
                vec![FlightAction::StartFlight {
                    source_slot: self.source_slot.unwrap_or(0),
                    target_slot: self.target_slot.unwrap_or(0),
                    start,
                    end,
                }]
            }
            // Retarget before launch: drop the pending pair; the coordinator
            // restarts matching from the current visual state.
            (FlightPhase::AwaitingBounds, FlightEvent::Retarget) => {
                self.phase = FlightPhase::Idle;
                self.source_slot = None;
                self.target_slot = None;
                vec![FlightAction::CancelFlight]
            }
            (FlightPhase::Flying, FlightEvent::ProgressDone) => {
                self.phase = FlightPhase::Finishing;
                self.progress = 1.0;
                vec![
                    FlightAction::ReleaseRetained { source_slot: self.source_slot.unwrap_or(0) },
                    FlightAction::FinishFlight,
                ]
            }
            // Mid-flight redirect: same slots, fresh generation, restart from
            // the current visual rect (actuator recomputes `start`).
            (FlightPhase::Flying, FlightEvent::Retarget) => {
                self.generation += 1;
                vec![FlightAction::StartFlight {
                    source_slot: self.source_slot.unwrap_or(0),
                    target_slot: self.target_slot.unwrap_or(0),
                    start: self.start.unwrap_or(SharedBounds::new(0.0, 0.0, 0.0, 0.0)),
                    end: self.end.unwrap_or(SharedBounds::new(0.0, 0.0, 0.0, 0.0)),
                }]
            }
            (phase, FlightEvent::ScopeDisposed | FlightEvent::WindowClosed)
                if matches!(phase, FlightPhase::AwaitingBounds | FlightPhase::Flying) =>
            {
                self.phase = FlightPhase::Cancelled;
                vec![FlightAction::CancelFlight]
            }
            (FlightPhase::Finishing, _) => {
                self.phase = FlightPhase::Idle;
                vec![FlightAction::Noop]
            }
            _ => vec![FlightAction::Noop],
        }
    }
}

/// Which end of the flight a node renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionRole {
    /// Leaving content: frozen detached node, fades 1→0 while morphing S→T.
    Source,
    /// Entering content: live in-tree node, fades 0→1 while morphing S→T.
    Target,
    /// Same-screen size morph (Phase 3 `animateBounds`): live in-tree node,
    /// opacity untouched, rect morphs old→new.
    Morph,
}

/// Per-frame render instruction, rewritten every frame by the coordinator
/// (Phase 2) from the flight's flown edges (or start/end + progress for an arc).
/// All fields are plain data — render only `peek`s, never subscribes
/// (zero-recomposition rule).
#[derive(Debug, Clone)]
pub(crate) struct TransitionVisual {
    pub start: SharedBounds,
    pub end: SharedBounds,
    /// Scalar progress snapshot (0→1, possibly overshooting under spring) — derived from the animated
    /// edges, and what the role crossfade and the corner-radius mix ride.
    pub progress: f32,
    /// The rect the flight's four springs produced, when they are driving the visual directly (a
    /// `PathMotion::Linear` flight). `None` for an arc flight, which post-processes the flown rect, and
    /// for the tests that build a visual by hand from start/end/progress.
    pub flown: Option<SharedBounds>,
    pub role: TransitionRole,
    /// Opacity owned by a SCENE instead of by the flight (see [`Self::alpha`]): the visibility of the
    /// scene this end belongs to, for flights whose ends carry a scene id. `None` = the flight's own
    /// crossfade applies, which is right when no scene host is involved.
    pub scene_alpha: Option<f32>,
    /// Normalized corner radii [TL, TR, BR, BL] at both ends (see
    /// [`shared_shape_radii`]). Every `Shape` variant normalizes to this
    /// quad, so cross-kind morphs (pill→rect) stay continuous.
    pub radius_from: [f32; 4],
    pub radius_to: [f32; 4],
    /// Clip to the lerped bounds during flight (Bounds+clip only; Element
    /// content scales exactly into bounds — clipping would cut shadows).
    pub clip: bool,
    /// Hit-routing link (Phase 3): source visuals point at the target slot so
    /// clicks on the flying ghost descend into the live target subtree.
    /// Targets and morphs carry `None`.
    pub link_slot: Option<u64>,
    /// Ancestor scroll sum in this composer's canvas frame (frozen when the
    /// flight's ends resolve). Render's canvas carries `translate(-S)` from
    /// scrolled ancestors while `start`/`end` are scroll-corrected window
    /// coords — the flight transform adds S back so in-tree endpoints paint
    /// exactly on the lerped rect (hit test and clip already live in that
    /// frame). Detached sources render rootless, so theirs is always (0,0).
    pub scroll: (f32, f32),
    /// Owning flight id. Teardown clears a slot's visual only when the tag
    /// matches — slots are positional identities a successor flight can
    /// resurrect, and unconditional clearing would flicker one frame off the
    /// new flight's freshly written visual.
    pub flight: FlightId,
    /// Motion path (copied from the flight at write time).
    pub path: PathMotion,
    /// sharedBounds enter/exit pair (`None` for Element flights).
    pub bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
    /// Paint in the transition layer instead of in-tree (Compose
    /// `renderInOverlayDuringTransition`): `render_pass1` skips this subtree
    /// in the main walk and the coordinator re-renders it rootless from
    /// [`Composer::render_layer`], escaping ancestor clips and ancestor layer
    /// transforms. Detached sources are always elevated; targets follow their
    /// own marker. Elevated visuals freeze `scroll` at `(0,0)` — the layer
    /// canvas carries no ancestor translate, so the bounds are already the
    /// absolute ones.
    pub elevated: bool,
    /// Each END's corner kind (Compose percent vs fixed), captured at resolve
    /// from that end's own marker. Percent corners are resolved against the
    /// lerped rect before the two ends are mixed, so a `Circle` -> `Rectangle`
    /// flight still fades its corners out while staying aligned end to end.
    pub radius_from_auto: bool,
    pub radius_to_auto: bool,
    /// This end is being re-measured at the animated size (Compose
    /// `RemeasureToBounds`): the content already has the animated size, so
    /// render must NOT scale it again and hit testing maps 1:1.
    pub remeasure: bool,
    /// How a scaling end fits its STABLE content into the lerped rect and where the
    /// leftover axis sits (Compose `ContentScale` + `Alignment`). `None` for
    /// `RemeasureToBounds`, which never scales.
    pub scale_parts: Option<(ContentScale, ImageAlignment)>,
}

impl TransitionVisual {
    pub(crate) fn lerped(&self) -> SharedBounds {
        // The flown edges when the flight animates them directly (every `PathMotion::Linear` flight,
        // which is the default): the rect the four springs produce, overshoot included. An arc flight
        // is post-processed instead, so it keeps the parameterised form below.
        if let Some(flown) = self.flown {
            return flown;
        }
        lerp_flight_rect(&self.start, &self.end, self.progress, self.path)
    }

    pub(crate) fn alpha(&self) -> f32 {
        // A SCENE owns opacity when the end belongs to one. During a nav transition the scenes
        // crossfade as layers, so the flight must not fade the same end a second time: its alpha is
        // the visibility of the scene the end came from (Compose's split — the scene transition owns
        // opacity, the shared bounds animation owns the rect). Only set for flights whose ends carry
        // a scene id; everything else keeps the crossfade below.
        if let Some(a) = self.scene_alpha {
            return a.clamp(0.0, 1.0);
        }
        match (&self.role, &self.bounds_fx) {
            // sharedBounds: fade channels claimed by enter/exit — fade
            // defaults reproduce the crossfade exactly; fade-less customs
            // ride opaque (Compose rule). Morphs never disappear.
            (TransitionRole::Target, Some((enter, _))) => {
                if enter.fade {
                    self.progress
                } else {
                    1.0
                }
            }
            (TransitionRole::Source, Some((_, exit))) => {
                if exit.fade {
                    1.0 - self.progress
                } else {
                    1.0
                }
            }
            _ => match self.role {
                TransitionRole::Source => 1.0 - self.progress,
                TransitionRole::Target => self.progress,
                // Same-screen morph never touches opacity.
                TransitionRole::Morph => 1.0,
            },
        }
        .clamp(0.0, 1.0)
    }

    pub(crate) fn radii(&self) -> [f32; 4] {
        // Unclamped like lerped(): spring overshoot carries corner radii past
        // the end value for one consistent flight-t (alpha stays clamped).
        // Floored at zero: shrinking radii would extrapolate negative under
        // overshoot, which Skia RRects reject.
        //
        // Compose interpolates RESOLVED corner sizes: a percent corner
        // (`Circle`/`Pill`) is resolved against the box being painted — the
        // lerped rect — before the two ends are mixed. Resolving it against the
        // endpoint's own (frozen) box would freeze it; keeping the resolved
        // percent as the end's value instead of mixing it would also diverge
        // from the other end (a `Circle` -> `Rectangle` flight must still fade
        // its corners out).
        let t = self.progress;
        let l = self.lerped();
        let percent = [l.width.min(l.height) / 2.0; 4];
        let from = if self.radius_from_auto { percent } else { self.radius_from };
        let to = if self.radius_to_auto { percent } else { self.radius_to };
        [0, 1, 2, 3].map(|i| (from[i] + (to[i] - from[i]) * t).max(0.0))
    }

    /// Clip rect at an explicit canvas offset — call BEFORE the flight
    /// canvas transform (clip is captured in the pre-transform space, like
    /// the GL clip rule).
    fn clip_rrect_at(&self, ox: f32, oy: f32) -> skia_safe::RRect {
        let l = self.lerped();
        let r = self.radii();
        skia_safe::RRect::new_rect_radii(
            skia_safe::Rect::new(l.x + ox, l.y + oy, l.x + ox + l.width, l.y + oy + l.height),
            &rrect_vectors([(r[0], r[0]), (r[1], r[1]), (r[2], r[2]), (r[3], r[3])]),
        )
    }

    /// Device-frame clip rect (zero offset — the frame hit test uses).
    #[allow(dead_code)]
    pub(crate) fn screen_rrect(&self) -> skia_safe::RRect {
        self.clip_rrect_at(0.0, 0.0)
    }

    /// Canvas-frame clip rect: `screen_rrect` translated by the frozen
    /// ancestor scroll sum. The clip call site sits INSIDE the ancestors'
    /// `translate(-S)`, so canvas coords must add S back to land on the
    /// lerped rect (same add-back as the flight translate below).
    pub(crate) fn canvas_rrect(&self) -> skia_safe::RRect {
        self.clip_rrect_at(self.scroll.0, self.scroll.1)
    }

    /// Hit-test remap (Phase 3): test the lerped visual rect; on hit, return
    /// the point remapped into the node's layout space for child descent
    /// (children keep layout positions — the flight transform is inverted
    /// here). `None` = visual miss → pass through (v1 skip behavior).
    /// `(nx, ny)` is the node's layout-space origin, `(w, h)` its layout size.
    pub(crate) fn remap_hit(
        &self,
        x: f32,
        y: f32,
        nx: f32,
        ny: f32,
        w: f32,
        h: f32,
    ) -> Option<(f32, f32)> {
        let l = self.lerped();
        if x < l.x || x > l.x + l.width || y < l.y || y > l.y + l.height {
            return None;
        }
        let (mut sx, mut sy) = self.paint_scale(w, h);
        if sx.abs() <= f32::EPSILON {
            sx = 1e-6;
        }
        if sy.abs() <= f32::EPSILON {
            sy = 1e-6;
        }
        // Mirrors the render transform exactly: scaled into the lerped rect from the
        // node origin, then placed by `paint_offset`.
        let (off_x, off_y) = self.paint_offset(w, h);
        // Clamp into layout bounds (float-safe: a visual hit must route
        // somewhere, never vanish at the edge).
        let lx = (nx + (x - l.x) / sx - off_x).clamp(nx.min(nx + w), nx.max(nx + w));
        let ly = (ny + (y - l.y) / sy - off_y).clamp(ny.min(ny + h), ny.max(ny + h));
        Some((lx, ly))
    }

    /// Layout-space radii pairs for bg/border/clip-element overrides (drawn
    /// The scale the render applies to this end's content box. `RemeasureToBounds`
    /// must NOT scale: the content was already laid out at the animated size, and that
    /// box trails `lerped()` by one poll (writers run after layout), so scaling by
    /// `l/box` stretches the freshly re-flowed content by the frame delta instead of
    /// leaving it re-laid-out.
    pub(crate) fn paint_scale(&self, box_w: f32, box_h: f32) -> (f32, f32) {
        if self.remeasure {
            return (1.0, 1.0);
        }
        let l = self.lerped();
        match self.scale_parts {
            Some((content_scale, _)) => {
                content_scale.scale_factors((box_w, box_h), (l.width, l.height))
            }
            // TEST-ONLY fallback: only the hand-built `TransitionVisual`s in the test
            // module leave this unset, and they want the pre-`ScaleToBounds` geometry,
            // i.e. `FillBounds`. Expressed through the same rule as everything else so
            // there is exactly one place that turns a mode into scales.
            None => ContentScale::FillBounds.scale_factors((box_w, box_h), (l.width, l.height)),
        }
    }

    /// NODE-space offset the render adds so the scaled content is placed by Compose
    /// `Alignment` (the render applies it inside `canvas.scale`, so it is the
    /// device-space leftover divided by the scale). Zero for `RemeasureToBounds`, for
    /// a mode that fills both axes, and for the historical top-left default.
    pub(crate) fn paint_offset(&self, box_w: f32, box_h: f32) -> (f32, f32) {
        if self.remeasure {
            return (0.0, 0.0);
        }
        let Some((_, alignment)) = self.scale_parts else {
            return (0.0, 0.0);
        };
        let (sx, sy) = self.paint_scale(box_w, box_h);
        let l = self.lerped();
        let (ox, oy) = ContentScale::align_offset(
            alignment,
            (box_w * sx, box_h * sy),
            (l.width, l.height),
            // The flight path does not carry a layout direction, so Start/End are
            // interpreted LTR here (Compose mirrors them with the environment).
            false,
        );
        (
            if sx.abs() > f32::EPSILON { ox / sx } else { 0.0 },
            if sy.abs() > f32::EPSILON { oy / sy } else { 0.0 },
        )
    }

    /// UNDER the flight transform — screen radii pre-divided by the PAINT scales).
    pub(crate) fn radii_pairs(&self, node_w: f32, node_h: f32) -> [(f32, f32); 4] {
        let l = self.lerped();
        // MUST be `paint_scale`, not the plain axis ratios: the render draws these
        // inside `canvas.scale(paint_scale)`, and under `ContentScale::FillWidth` the
        // two axes scale by the SAME factor while `l.h/node_h` is a different number.
        // Dividing by the axis ratios there painted an elliptical corner (measured
        // 8x24 device px for an intended 8x8 round corner on a 100x100 -> 300x100
        // flight, and an 8.7% vertical stretch in the live width-preserving case).
        let (mut sx, mut sy) = self.paint_scale(node_w, node_h);
        if sx.abs() < 1e-6 {
            sx = 1e-6;
        }
        if sy.abs() < 1e-6 {
            sy = 1e-6;
        }
        // Percent corners (Pill/Circle) are resolved inside `radii()` against
        // the lerped rect — each end first, then mixed by progress — so a plain
        // `radii()` is already correct here.
        // A DEVICE radius must not exceed half the smaller side of the rect it is
        // painted into, which a spring overshoot can drive past (a percent end
        // resolved against the animated box, or a fixed end extrapolating). Compose
        // scales such corners down proportionally in `createOutline`; Skia instead
        // silently clamps at construction, so without this the corner would jump
        // rather than follow the overshoot.
        let cap = l.width.min(l.height) / 2.0;
        let r = self.radii().map(|v| v.min(cap));
        [
            (r[0] / sx, r[0] / sy),
            (r[1] / sx, r[1] / sy),
            (r[2] / sx, r[2] / sy),
            (r[3] / sx, r[3] / sy),
        ]
    }
}

