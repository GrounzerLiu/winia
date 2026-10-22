use super::*;

/// Flight rect at scalar progress: size lerps linearly, the rect center
/// follows the motion path (linear or Compose `ArcSpline.Arc`
/// quarter-ellipse, arc-length uniform). Used for ARC flights and for the tests that build a visual by
/// hand; a straight flight paints its flown edges instead (`TransitionVisual::flown`). Single home for
/// paint, clip, hit and retarget continuity so an arc can never silently straighten on one consumer.
pub(crate) fn lerp_flight_rect(
    start: &SharedBounds,
    end: &SharedBounds,
    progress: f32,
    path: PathMotion,
) -> SharedBounds {
    let t = progress;
    let w = start.width + (end.width - start.width) * t;
    let h = start.height + (end.height - start.height) * t;
    let (sx, sy) = (start.x + start.width / 2.0, start.y + start.height / 2.0);
    let (ex, ey) = (end.x + end.width / 2.0, end.y + end.height / 2.0);
    let (cx, cy) = match path {
        PathMotion::Linear => (sx + (ex - sx) * t, sy + (ey - sy) * t),
        PathMotion::ArcBelow => arc_center(sx, sy, ex, ey, t, true),
        PathMotion::ArcAbove => arc_center(sx, sy, ex, ey, t, false),
    };
    SharedBounds::new(cx - w / 2.0, cy - h / 2.0, w, h)
}

pub(crate) fn rrect_vectors(pairs: [(f32, f32); 4]) -> [skia_safe::Vector; 4] {
    [
        skia_safe::Vector::new(pairs[0].0, pairs[0].1),
        skia_safe::Vector::new(pairs[1].0, pairs[1].1),
        skia_safe::Vector::new(pairs[2].0, pairs[2].1),
        skia_safe::Vector::new(pairs[3].0, pairs[3].1),
    ]
}

/// Corner radii quad [TL, TR, BR, BL] resolved from the nearest
/// Background/Border/Clip shape — same precedence as the render focus ring.
/// Pill/Circle resolve against the given (flight-start) size. No shape at
/// all → plain rect.
/// True when the marker's nearest shape derives its corners from the box
/// (`Pill`/`Circle`, and the percent end of a `PillToRounded` mid-press morph):
/// those must be evaluated on the painted box instead of being lerped between
/// the endpoint radii. `PillToRounded` was missing here, so a flight from a
/// mid-press button lerped a radius frozen at flight start while
/// `shared_shape_radii` resolved it against the box.
pub(crate) fn shared_shape_radius_is_auto(modifier: &Modifier) -> bool {
    modifier
        .elements()
        .iter()
        .rev()
        .find_map(|el| match el {
            ModifierElement::Background { shape, .. }
            | ModifierElement::Border { shape, .. }
            | ModifierElement::Clip { shape } => Some(shape),
            _ => None,
        })
        .map(|shape| shape.eval())
        .is_some_and(|s| matches!(s, Shape::Pill | Shape::Circle | Shape::PillToRounded { .. }))
}

/// The radii a shape normalizes to, for a shared-element flight.
///
/// A flight CAPTURES these when it takes its endpoints (the detach and the target setup), so a
/// dynamic shape is read once at that moment and the flight then interpolates stable radii. The
/// alternative — re-evaluating per frame — would move the flight's target while the flight runs,
/// because the underlying shape keeps animating under its own clock.
pub(crate) fn shared_shape_radii(modifier: &Modifier, w: f32, h: f32) -> [f32; 4] {
    let shape = modifier.elements().iter().rev().find_map(|el| match el {
        ModifierElement::Background { shape, .. }
        | ModifierElement::Border { shape, .. }
        | ModifierElement::Clip { shape } => Some(shape),
        _ => None,
    })
    .map(|shape| shape.eval());
    match shape {
        None => [0.0; 4],
        Some(Shape::Rectangle) => [0.0; 4],
        Some(Shape::RoundedRect { corner_radius }) => [corner_radius; 4],
        Some(Shape::TopRoundedRect { radius }) => [radius, radius, 0.0, 0.0],
        // Corner order matches `RRect::new_rect_radii`: UL, UR, LR, LL.
        Some(Shape::RightRoundedRect { radius }) => [0.0, radius, radius, 0.0],
        Some(Shape::LeftRoundedRect { radius }) => [radius, 0.0, 0.0, radius],
        // A pill part-way to a fixed radius — a shared element flight carries the same shape the
        // renderer paints, and here the box IS known, so the percent end resolves exactly.
        Some(Shape::PillToRounded { corner_radius, t }) => {
            let percent = w.min(h) / 2.0;
            let r = percent + (corner_radius - percent) * t;
            [r; 4]
        }
        // Already per-corner: Compose's (top-start, top-end, bottom-end, bottom-start) in geometric
        // corners, which is the same order as `RRect::new_rect_radii`.
        Some(Shape::Corners { top_left, top_right, bottom_right, bottom_left }) => {
            [top_left, top_right, bottom_right, bottom_left]
        }
        Some(Shape::Pill) | Some(Shape::Circle) => {
            let m = w.min(h) / 2.0;
            [m; 4]
        }
    }
}

/// Cloned marker payload for coordinator use (built at most once per flight
/// end — the per-frame render path reads [`TransitionVisual`] instead).
#[derive(Debug, Clone)]
pub(crate) struct SharedMarker {
    pub scope_id: u64,
    pub key: String,
    pub kind: SharedKind,
    pub transform: BoundsTransform,
    pub path: PathMotion,
    pub z_index: f32,
    /// sharedBounds enter/exit (`None` on sharedElement markers).
    pub enter: Option<VisibilityTransition>,
    pub exit: Option<VisibilityTransition>,
    /// Compose `renderInOverlayDuringTransition` (default true).
    pub render_in_overlay: bool,
    /// Scene this end was composed inside, when a scene host published one (see
    /// [`with_nav_scene`]). Used to pair two LIVE ends of one key by "which scene is becoming
    /// visible" instead of by tree-walk order.
    pub scene: Option<u64>,
}

pub(crate) fn find_shared_marker(modifier: &Modifier) -> Option<SharedMarker> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::SharedTransition {
            scope_id, key, kind, transform, path, z_index, enter, exit, render_in_overlay, scene,
        } => Some(SharedMarker {
            scope_id: *scope_id,
            key: key.clone(),
            kind: kind.clone(),
            transform: transform.clone(),
            path: *path,
            z_index: *z_index,
            enter: enter.clone(),
            exit: exit.clone(),
            render_in_overlay: *render_in_overlay,
            scene: *scene,
        }),
        _ => None,
    })
}

/// Compose `Modifier.renderInSharedTransitionScopeOverlay` marker:
/// `(scope_id, z_index)` of a non-shared subtree that elevates while its scope
/// transitions.
pub(crate) fn find_scope_overlay_marker(modifier: &Modifier) -> Option<(u64, f32)> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::SharedScopeOverlay { scope_id, z_index } => Some((*scope_id, *z_index)),
        _ => None,
    })
}

/// `ResizeMode` declared by a marker. Compose resolves it per end; only the end
/// with live layout (the entering one) can honour it.
pub(crate) fn shared_resize_for_kind(kind: &SharedKind) -> ResizeMode {
    match kind {
        SharedKind::Bounds { resize, .. } => resize.clone(),
        // Compose's `sharedElement` has no resize parameter and always
        // re-measures: "During the bounds transform, sharedElement will
        // re-measure and relayout its child layout using fixed constraints
        // derived from its animated size, similar to RemeasureToBounds"
        // (SharedTransitionScope KDoc). Element content is identical on both
        // ends, so reflowing it is what keeps text and rows correct.
        SharedKind::Element { .. } => ResizeMode::RemeasureToBounds,
    }
}

/// `PlaceHolderSize` declared by a marker (Compose `PlaceholderSize`).
pub(crate) fn shared_placeholder_for_kind(kind: &SharedKind) -> PlaceHolderSize {
    match kind {
        SharedKind::Bounds { placeholder, .. } => *placeholder,
        SharedKind::Element { placeholder } => *placeholder,
    }
}

/// Clip flag for a flight end. Always `false` now: the render clips EVERY
/// transitioning node to its morph-shaped lerped rect unconditionally, so a marker
/// cannot opt into (or out of) clipping — the `ResizeMode::ScaleToBounds` flag that
/// used to feed this measured as having no effect and was deleted.
pub(crate) fn shared_clip_for_kind(_kind: &SharedKind) -> bool {
    false
}

/// Compose `overlayClip` of a marker kind. `Element` ends have no clip parameter, so
/// they keep the morph-shaped default.
pub(crate) fn shared_overlay_clip_for_kind(kind: &SharedKind) -> OverlayClip {
    match kind {
        SharedKind::Bounds { overlay_clip, .. } => *overlay_clip,
        SharedKind::Element { .. } => OverlayClip::Bounds,
    }
}

/// The `overlayClip` a node's own marker declares (Compose reads it per end, and each
/// end renders with its own). Falls back to the default when the node carries none.
pub(crate) fn overlay_clip_of(modifier: &crate::modifier::Modifier) -> OverlayClip {
    find_shared_marker(modifier)
        .map(|m| shared_overlay_clip_for_kind(&m.kind))
        .unwrap_or_default()
}

/// Live flight: pure machine + engine handles. Non-terminal flights are polled
/// every frame post-layout; render reads plain f32 snapshots (never subscribes).
#[derive(Debug)]
pub(crate) struct ActiveFlight {
    pub flight: Flight,
    /// The crossfade clock: a scalar 0→1 driven by the flight's spec, which is what the role alphas
    /// and the corner-radius mix ride. It runs even when the two ends are the same rect, so a switch
    /// in place still fades its content the way Compose's enter/exit animations do.
    pub progress: State<f32>,
    /// The flight's four edges, driven by the same spec — Compose's shape (`Rect.VectorConverter`):
    /// one spring per edge, one pixel of threshold each. This is the geometry; `progress` is the fade.
    pub edges: State<FlightEdges>,
    pub spec: AnimationSpec,
    /// Detached retained source arena index (frozen content).
    pub source_idx: Option<usize>,
    /// Exact start bounds (WINDOW coords — canonical flight frame; writer
    /// composers subtract their `screen_origin` when emitting visuals).
    pub start: SharedBounds,
    /// Frozen ancestor scroll sums (canvas frame) for each end, captured
    /// when the ends resolve. Source ends are detached (rootless) so
    /// `start_scroll` is always `(0,0)`; `end_scroll` is the live target's
    /// ancestor sum at resolve time. Freezing (not per-frame refresh) keeps
    /// the visual rigid under mid-flight scroll — it shifts exactly like
    /// normal content instead of pinning to the window.
    pub start_scroll: (f32, f32),
    pub end_scroll: (f32, f32),
    /// Motion path, resolved from the target marker when the end resolves
    /// (same rule as the animation spec — one flight, one path).
    pub path: PathMotion,
    /// sharedBounds enter/exit pair, resolved at end-resolve time (`None` for Element flights,
    /// which always crossfade in winia). Enter comes from the target marker, exit from the source
    /// marker — each side declares its own. DEVIATION from Compose: winia draws BOTH ends of an
    /// Element flight and crossfades them, while Compose's `sharedElement` installs no enter/exit
    /// and (per its KDoc) renders only the copy that is becoming visible — see the open deviation
    /// entry in `docs/shared-element-gaps.md`.
    pub bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
    pub radius_from: [f32; 4],
    pub radius_to: [f32; 4],
    /// Corner KIND of each end (Compose percent vs fixed), frozen at resolve —
    /// see `TransitionVisual::radii`.
    pub radius_from_auto: bool,
    pub radius_to_auto: bool,
    pub clip: bool,
    /// Target-end `renderInOverlayDuringTransition`, frozen when the end
    /// resolves (from the target marker; `true` when the marker is gone).
    /// The source end is always detached, so it is always in the layer.
    pub target_in_overlay: bool,
    /// `ResizeMode` / `PlaceHolderSize` from the target marker, frozen when the
    /// end resolves. Compose resolves both per end; only the entering end has
    /// live layout, so it is the end these can act on.
    pub resize: ResizeMode,
    pub placeholder: PlaceHolderSize,
    /// The target's natural (resting) size in its own layout, captured the
    /// frame the end resolves — the `ContentSize` answer, and the size the
    /// parent keeps seeing while the flight runs.
    pub target_size: Option<crate::unit::Size>,
    /// Per-frame layout override handed to the target node.
    pub measure: State<FlightMeasureFrame>,
    /// Endpoint owner composers (Phase 4 Tier1). Equal ⟺ Tier0, driven by the
    /// owner poll; differing ⟺ Tier1, driven by cross-poll. The flight lives
    /// in the MAIN composer map (main outlives overlays).
    pub source_cid: u64,
    pub target_cid: u64,
}

impl ActiveFlight {
    /// The flight's progress: the crossfade clock (see [`Self::progress`]).
    pub(crate) fn progress_value(&self) -> f32 {
        self.progress.peek()
    }

    /// Place the flight at `t` along the start→end line — both the clock and the flown edges: how a
    /// test drives a flight without running the engine's animation.
    pub(crate) fn set_progress(&self, t: f32) {
        self.progress.set(t);
        let start = FlightEdges::of(&self.start);
        let end = FlightEdges::of(&self.flight.end.unwrap_or(self.start));
        self.edges.set(start.lerp(&end, t));
    }

    /// Whether every animation this flight owns has stopped: the edges (geometry) and the clock
    /// (content fade).
    pub(crate) fn is_done(&self) -> bool {
        !crate::animation::has_animation_for_state(self.edges.state_id())
            && !crate::animation::has_animation_for_state(self.progress.state_id())
    }
}

/// Detached source awaiting a cross-composer counterpart (Phase 4 Tier1).
/// Stashed by the owner retain hook, consumed or freed by cross-poll in the
/// SAME frame — never survives to the next frame.
#[derive(Debug, Clone)]
pub(crate) struct PendingSource {
    pub scope_id: u64,
    pub key: String,
    pub old_slot: u64,
    pub src_idx: usize,
    /// Start bounds + radii in WINDOW coords.
    pub start: SharedBounds,
    pub radius_from: [f32; 4],
    /// Corner kind of the leaving end (Compose percent vs fixed).
    pub radius_from_auto: bool,
}

/// Exact last-frame absolute rect via upward parent walk. Valid only at
/// compose-tail time (pre-layout): every cached position is still the old one.
/// `id_to_idx` must cover the whole arena (reused ancestors keep their objects).
/// Shared with hit-routing (node.rs) — keep the math identical to
/// `node_abs_position` (app.rs descent).
pub(crate) fn abs_rect_upward(
    nodes: &[LayoutNode],
    id_to_idx: &HashMap<u64, usize>,
    idx: usize,
) -> SharedBounds {
    let n = &nodes[idx];
    // Content box: a ghost's fraction map must land on the box the target
    // actually paints (its content), not on the placeholder size the target's
    // parent was told.
    let (w, h) = {
        let cb = n.content_box();
        (cb.width, cb.height)
    };
    let (mut ax, mut ay) = (n.position.x, n.position.y);
    let mut cur = idx;
    // Ascent mirrors the descent in `node_abs_position` (app.rs): each level
    // adds its position minus its own scroll offset. Keep the two in sync.
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        let p = &nodes[pidx];
        let (dx, dy) = scroll_offset_for_node(p);
        ax += p.position.x - dx;
        ay += p.position.y - dy;
        cur = pidx;
    }
    SharedBounds::new(ax, ay, w, h)
}

pub(crate) fn find_idx_by_slot(nodes: &[LayoutNode], root: usize, slot: u64) -> Option<usize> {
    let mut stack = vec![root];
    while let Some(idx) = stack.pop() {
        if nodes[idx].slot_key == slot {
            return Some(idx);
        }
        stack.extend(nodes[idx].children.iter().copied());
    }
    None
}

pub(crate) fn arc_center(sx: f32, sy: f32, ex: f32, ey: f32, t: f32, below: bool) -> (f32, f32) {
    const EPS: f32 = 0.001;
    const LUT: usize = 101;
    const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
    let (dx, dy) = (ex - sx, ey - sy);
    if dx.abs() < EPS || dy.abs() < EPS {
        return (sx + dx * t, sy + dy * t);
    }
    let is_vertical = if below { dy > 0.0 } else { dy < 0.0 };
    let vertical = if is_vertical { -1.0 } else { 1.0 };
    let (a, b) = (dx / vertical, dy / -vertical);
    let (cx, cy) = (
        if is_vertical { ex } else { sx },
        if is_vertical { sy } else { ey },
    );
    // Arc-length table over θ ∈ [0, π/2] (stack, no alloc).
    let mut lut = [0.0f32; LUT];
    let (mut qx, mut qy) = (cx, cy + b);
    for i in 1..LUT {
        let th = HALF_PI * i as f32 / (LUT - 1) as f32;
        let (rx, ry) = (cx + a * th.sin(), cy + b * th.cos());
        lut[i] = lut[i - 1] + (rx - qx).hypot(ry - qy);
        qx = rx;
        qy = ry;
    }
    let total = lut[LUT - 1];
    let percent = if is_vertical { 1.0 - t } else { t };
    let target = percent.clamp(0.0, 1.0) * total;
    // Invert: angle fraction whose cumulative length brackets the target.
    let k = lut.partition_point(|&l| l < target);
    let frac = if k == 0 {
        0.0
    } else if k >= LUT {
        1.0
    } else {
        let (l0, l1) = (lut[k - 1], lut[k]);
        let f = if l1 > l0 { (target - l0) / (l1 - l0) } else { 0.0 };
        ((k - 1) as f32 + f) / (LUT - 1) as f32
    };
    let ang = HALF_PI * frac;
    (cx + a * ang.sin(), cy + b * ang.cos())
}

/// Per-frame layout override for a shared-element flight (Compose `ResizeMode`
/// / `PlaceHolderSize`). The coordinator rewrites the frame every frame and
/// re-seeds the node's slot key into the layout invalidation set, so the layout
/// actually descends into it (a folded parent never would); reading the frame
/// during measure registers a LAYOUT dependency as well.
#[derive(Clone)]
pub(crate) struct FlightMeasure {
    pub frame: crate::runtime::state::State<FlightMeasureFrame>,
    /// Identity of the flight that owns this override. Flight ids are
    /// COMPOSER-local (every composer's counter starts at 1) while a Tier-1
    /// override is written onto a PEER's node, so the composer id is part of the
    /// key — otherwise an unrelated peer flight with the same number could clear
    /// it. Teardown AND writes must match it: slot keys are positional identities
    /// a SUCCESSOR flight can resurrect, so acting blindly would destroy the
    /// newer flight's override.
    pub owner: crate::transition::FlightKey,
}

/// One frame of that override; `None` on a field means "no override there".
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct FlightMeasureFrame {
    /// Tight constraints the child content is measured at — Compose
    /// `RemeasureToBounds`: the subtree re-lays-out at the animated size
    /// instead of being scaled into it.
    pub content: Option<Size>,
    /// Size reported to the PARENT — Compose `PlaceHolderSize::AnimatedSize`
    /// reports the animated size so siblings reflow; `ContentSize` keeps the
    /// target size so the surrounding layout holds still.
    pub reported: Option<Size>,
}

