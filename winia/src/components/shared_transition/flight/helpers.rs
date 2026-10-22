use super::*;

// ═══════════════════════════════════════════════════════════
// Phase 2: flight visuals (render-side data)
// ═══════════════════════════════════════════════════════════


/// Quarter-ellipse arc center (Compose `ArcSpline.Arc` math): `X = Cx +
/// A·sin θ`, `Y = Cy + B·cos θ`, θ swept 0→π/2 through an arc-length table
/// so travel is uniform along the curve. Orientation follows travel
/// direction + Above/Below exactly like AOSP (vertical time runs backward;
/// `Below→DownArc`, `Above→UpArc`). Either-dimension travel below epsilon
/// falls back to linear (Compose rule — axis-aligned flights stay straight).
/// Progress outside [0,1] (spring overshoot) pins the center at the nearer
/// endpoint — a deliberate deviation from AOSP extrapolation: Winia
/// overshoot rides scalar progress, and size/radii still overshoot visibly.


/// Opacity a SCENE owns for a flight end, if that end belongs to one (see
/// [`TransitionVisual::alpha`]): the visibility of the scene the end was composed in, read right
/// now. During a nav transition the scenes crossfade as layers, so the flight must not fade the same
/// element a second time — Compose splits it the same way (the scene transition owns opacity, the
/// shared-bounds animation owns the rect).
///
/// `leaving_end` matters because the two ends are painted from different places: a detached source
/// ghost is rendered from the transition layer, where the scene's own layer fade no longer applies,
/// so the flight carries that fade; a target that stays in tree is already faded by its scene's
/// layer, so the flight keeps it opaque unless the scene host elevates it into the layer too.
pub(crate) fn scene_alpha_for_end(nodes: &[LayoutNode], idx: usize, leaving_end: bool) -> Option<f32> {
    let marker = find_shared_marker(&nodes.get(idx)?.modifier)?;
    // Scene membership from the ANCESTOR chain, like the pairing path: the marker's own `scene` field is
    // captured when its modifier element is built and then reused across frames, so it freezes (measured:
    // one end reported None while the other reported the LEAVING scene). Reading the frozen field here
    // meant an in-tree end could double-fade (None while its scene also fades it) or never fade (a stale
    // Some(1.0) after its scene stopped fading it).
    scene_of_node(nodes, idx)?;
    if leaving_end || marker.render_in_overlay {
        // The flight paints this end from the transition layer, and the scene's own layer fade does
        // NOT apply there — so the flight has to fade it, and it must do so on the FLIGHT's clock so
        // that rect and opacity stay in step. Measured before this: the detached ghost faded on the
        // nav's 800 ms clock while its rect followed a 1.3 s spring, so the leaving end disappeared
        // before it had finished growing and the flight read as "only the entering end animates".
        None
    } else {
        // Painted in tree: its scene already fades it, so the flight keeps it opaque.
        Some(1.0)
    }
}


/// Per-frame layout instruction for a flight endpoint (Compose `ResizeMode` /
/// `PlaceHolderSize`), shared by the Tier 0 and Tier 1 writers.
pub(crate) fn placeholder_frame(
    remeasure: bool,
    animated: crate::unit::Size,
    placeholder: PlaceHolderSize,
    target_size: Option<crate::unit::Size>,
) -> FlightMeasureFrame {
    FlightMeasureFrame {
        // RemeasureToBounds: animated fixed constraints → the subtree reflows.
        content: remeasure.then_some(animated),
        // What the parent observes: AnimatedSize reflows the siblings,
        // ContentSize / JumpCut keep the target size (Compose's default).
        reported: match placeholder {
            PlaceHolderSize::AnimatedSize => Some(animated),
            PlaceHolderSize::ContentSize | PlaceHolderSize::JumpCut => {
                if remeasure {
                    target_size
                } else {
                    None
                }
            }
        },
    }
}

/// The lerped rect's size (the arc bends the centre only, never the size).
pub(crate) fn animated_size(
    start: &SharedBounds,
    end: &SharedBounds,
    p: f32,
    path: PathMotion,
) -> crate::unit::Size {
    let l = lerp_flight_rect(start, end, p, path);
    crate::unit::Size::new(l.width, l.height)
}


// ═══════════════════════════════════════════════════════════
// Phase 2: Tier-0 coordinator (compose/layout/app-loop hooks)
// ═══════════════════════════════════════════════════════════


/// Canvas scroll translation applied by a node's STRICT ancestors.
///
/// Mirrors the render scroll block (`render_pass1`): each scroll container
/// translates its children by `-offset` (`reverseLayout` mirrored). The
/// node's own offset is excluded (it only shifts descendants). Rootless
/// (detached retained sources) sum to `(0,0)`.
///
/// NOTE: this deliberately follows the render convention, not
/// `scroll_offset_for_node` (which leaves vertical-reverse unmirrored): the
/// sum is added back onto the render canvas, so it must equal what the
/// canvas carries. The pre-existing vertical-reverse hit/render divergence
/// is out of scope.
pub(crate) fn ancestor_scroll_sum(
    nodes: &[LayoutNode],
    id_to_idx: &HashMap<u64, usize>,
    idx: usize,
) -> (f32, f32) {
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut cur = idx;
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        let p = &nodes[pidx];
        for el in p.modifier.elements() {
            match el {
                ModifierElement::VerticalScroll { state } => {
                    let mut off = state.offset.get();
                    if p.scroll_reverse {
                        off = (p.scroll_content_height - p.scroll_viewport_height - off).max(0.0);
                    }
                    // Accumulate: scroll containers nest additively on the
                    // canvas (each ancestor translates its children).
                    sy += off;
                }
                ModifierElement::HorizontalScroll { state, .. } => {
                    let mut off = state.offset.get();
                    if p.scroll_reverse {
                        off = (p.scroll_content_width - p.scroll_viewport_width - off).max(0.0);
                    }
                    sx += off;
                }
                _ => {}
            }
        }
        cur = pidx;
    }
    (sx, sy)
}


/// Test-only: mark a scope as transitioning (or idle) without a real flight.
///
/// A morph now only opens while its scope has a running transition (Compose ties the two together), so
/// tests that used to start one from a bare layout change mark the scope active first. The coordinator's
/// end-of-poll full sync clears it again, which is fine: the morph opens during that same poll and
/// continues through the ungated reopen path.
#[cfg(test)]
pub(crate) fn set_scope_transition_active_for_test(scope_id: u64, active: bool) {
    SCOPE_ACTIVE
        .lock()
        .unwrap()
        .entry(scope_id)
        .or_insert_with(|| State::new(false))
        .set(active);
}

/// Is a transition running in this scope right now — i.e. does any non-terminal flight belong to it?
///
/// The same question [`SharedTransitionScope::is_transition_active`] answers for composition, asked by
/// [`SharedTransitionScope`]-less code (the coordinator's morph detection) from the synced global table,
/// so both tiers agree. Scopes nobody has read yet count as idle.
pub(crate) fn scope_transition_active(scope_id: u64) -> bool {
    SCOPE_ACTIVE
        .lock()
        .unwrap()
        .get(&scope_id)
        .map(|s| s.get())
        .unwrap_or(false)
}

pub(crate) fn is_terminal(phase: FlightPhase) -> bool {
    matches!(phase, FlightPhase::Finishing | FlightPhase::Cancelled)
}

/// Sync per-scope activity flags from flight maps (both tiers — Tier 1
/// flights live in the main map under the same `scope_id`). Called at the
/// end of every coordinator poll so the flag tracks opens and teardowns
/// within one frame. Only scopes somebody has read (map entries) are
/// touched — flights never create entries by themselves.
/// NOTE: deliberately a full sync (not true-only) per composer — headless
/// Tier 0 polls must clear flags alone, and the transient cross-poll flap
/// (overlay poll clears, union poll re-sets) is invisible: no render runs
/// between polls, and steady-state `set` dedups without notify.
pub(crate) fn sync_scope_active_states(all: &[&Composer]) {
    let mut active: HashSet<u64> = HashSet::new();
    for c in all {
        active.extend(
            c.shared_flights
                .values()
                .filter(|a| !is_terminal(a.flight.phase))
                .map(|a| a.flight.scope_id),
        );
    }
    let map = SCOPE_ACTIVE.lock().unwrap();
    for (id, state) in map.iter() {
        state.set(active.contains(id));
    }
}

/// Flight completion gate (MAJOR #2): while the animation engine still owns
/// the progress state the flight stays alive — even past `p >= 0.999`, whose
/// first passage precedes the overshoot peak under bouncy spring specs
/// (snapping there would swallow the overshoot). Once the engine releases
/// the state it has settled exactly at 1.0, so the threshold decides; the
/// same threshold covers manually-driven progress with no engine entry
/// (headless tests).
///
/// A flight is over when its edge animation has nothing left running — the engine registers a spring
/// until every lane is at rest (each edge within its own visibility threshold) and a tween until it
/// lands, which is what Compose's `animateTo` reports as `Finished`.
///
/// NOTE: flight specs must be finite (Tween/Spring/Keyframes settle and release). An infinite
/// Repeatable spec would hold the flight open forever.
pub(crate) fn flight_progress_done(a: &ActiveFlight) -> bool {
    a.is_done()
}

