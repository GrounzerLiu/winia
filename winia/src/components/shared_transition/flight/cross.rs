use super::*;

impl Composer {
    /// Any non-terminal Tier1 flight in this map (z-order + idle gating).
    pub(crate) fn has_cross_flights(&self) -> bool {
        self.shared_flights.values().any(|a| {
            !is_terminal(a.flight.phase) && a.source_cid != a.target_cid
        })
    }

    /// Window-level Tier1 matcher + driver (app loop, after ALL composers laid
    /// out, before render). `all[0]` is the MAIN composer — Tier1 flights
    /// always live in its map (main outlives overlays). Pairs stashed sources
    /// with freshly-appeared counterparts in OTHER composers; frees unmatched
    /// stashes same-frame (invisible); drives active Tier1 (progress, both-end
    /// visuals, completion with cross-arena teardown).
    pub(crate) fn poll_cross_flights(all: &mut [&mut Composer]) {
        if all.is_empty() {
            return;
        }
        // Fast path: no stashes anywhere and no Tier1 in main map.
        let busy = all.iter().any(|c| !c.pending_cross.is_empty()) || all[0].has_cross_flights();
        if !busy {
            // Still publish the scope-activity union for the WHOLE frame. Syncing per composer inside
            // `poll_shared_flights` lets the last composer's poll overwrite the flags of every scope it
            // does not own (the app polls the main composer first, overlays after), so
            // `is_transition_active()` reported false during a Tier-0 transition whenever any overlay was
            // present — and the morph gate's fallback read the same stale value. This call is the last
            // poll of the frame, so its set is authoritative.
            let refs: Vec<&Composer> = all.iter().map(|c| &**c).collect();
            sync_scope_active_states(&refs);
            prune_scene_registry_window(&refs);
            return;
        }
        // 0. Drive active Tier1 (staleness-cancel → progress → complete).
        let tier1: Vec<FlightId> = all[0]
            .shared_flights
            .iter()
            .filter(|(_, a)| !is_terminal(a.flight.phase) && a.source_cid != a.target_cid)
            .map(|(id, _)| *id)
            .collect();
        for id in tier1 {
            Self::poll_one_cross_flight(&mut *all, id);
        }
        // 1. Match stashed sources in order (main first, then peers).
        for ci in 0..all.len() {
            let stashed: Vec<PendingSource> = std::mem::take(&mut all[ci].pending_cross);
            for p in stashed {
                Self::match_pending_source(&mut *all, ci, p);
            }
        }
        let refs: Vec<&Composer> = all.iter().map(|c| &**c).collect();
        sync_scope_active_states(&refs);
        // Chrome membership is a WINDOW-level decision: a peer composer cannot
        // see a Tier1 flight (it lives in the main map), so each composer's own
        // poll cannot decide for itself. Push the union to every participant —
        // otherwise a pinned bar inside the dialog would never elevate for the
        // hero flying into it. When nothing is cross-busy this function returns
        // early and the per-composer polls remain authoritative.
        let union: HashSet<u64> = all
            .iter()
            .flat_map(|c| c.shared_flights.values())
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| a.flight.scope_id)
            .collect();
        // The union of the FLIGHTS, not just their scopes: `refresh_paint_dispositions` decides placeholders
        // from the (scope, key) pairs, and a Tier1 flight lives only in the main composer's map, so a peer
        // would otherwise keep painting a second copy of a key that is flying.
        let flying_union: Vec<(u64, String)> = all
            .iter()
            .flat_map(|c| c.shared_flights.values())
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
            .collect();
        for c in all.iter_mut() {
            c.refresh_scope_overlay_roots(&union);
            c.rebuild_layer_order();
            // Recompute the dispositions, because both of the above just changed their inputs: a Tier1
            // flight that started or completed in this cross-poll would otherwise leave a duplicate copy
            // painted (or a placeholder hidden) for one frame, since each composer's own poll ran earlier.
            c.refresh_paint_dispositions_with(&flying_union);
        }
        let refs: Vec<&Composer> = all.iter().map(|c| &**c).collect();
        prune_scene_registry_window(&refs);
    }

    /// Drive one Tier1 flight: staleness-cancel, progress, both-end visuals
    /// (each in its writer's canvas frame), completion with cross-arena teardown.
    pub(crate) fn poll_one_cross_flight(all: &mut [&mut Composer], id: FlightId) {
        // Snapshot endpoints (main map).
        let (s_cid, t_cid, scope, key) = match all[0].shared_flights.get(&id) {
            Some(a) => (a.source_cid, a.target_cid, a.flight.scope_id, a.flight.key.clone()),
            None => return,
        };
        // Resolve composer indices (endpoint composer gone ⟹ cancel).
        let (Some(si), Some(ti)) = (
            all.iter().position(|c| c.composer_id == s_cid),
            all.iter().position(|c| c.composer_id == t_cid),
        ) else {
            Self::cancel_cross_flight(all, id);
            return;
        };
        // Staleness: target key missing-or-changed in its composer?
        let stale = {
            let t = &all[ti];
            match t.shared_live_map().get(&(scope, key.clone())) {
                Some(&slot) => {
                    Some(slot) != all[0].shared_flights.get(&id).and_then(|a| a.flight.target_slot)
                }
                None => true,
            }
        };
        // Superseded: a newer same-key Tier0 took over anywhere visible?
        // (Tier1 yields — the Tier0 owns the endpoints now.)
        let superseded = all.iter().any(|c| {
            c.shared_flights.iter().any(|(fid, a)| {
                *fid != id
                    && !is_terminal(a.flight.phase)
                    && a.flight.scope_id == scope
                    && a.flight.key == key
                    && a.source_cid == a.target_cid
            })
        });
        if stale || superseded {
            Self::cancel_cross_flight(all, id);
            return;
        }
        // Drive the edges + visuals.
        if let Some(a) = all[0].shared_flights.get_mut(&id) {
            a.flight.progress = a.progress_value();
        }
        Self::write_cross_visuals(&mut *all, si, ti, id);
        let done = all[0].shared_flights.get(&id).is_some_and(flight_progress_done);
        if done {
            let acts = all[0]
                .shared_flights
                .get_mut(&id)
                .map(|a| a.flight.on_event(FlightEvent::ProgressDone))
                .unwrap_or_default();
            debug_assert_eq!(acts.len(), 2);
            // Interpret [ReleaseRetained, FinishFlight] across composers.
            let (sslot, sidx, tslot) = all[0]
                .shared_flights
                .get(&id)
                .map(|a| (a.flight.source_slot, a.source_idx, a.flight.target_slot))
                .unwrap_or((None, None, None));
            if let (Some(slot), Some(idx)) = (sslot, sidx) {
                let owner = &mut all[si];
                if owner.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                    let mut visited = crate::layout::node::NodeMarks::default();
                    owner.arena.free_node_skip(idx, &crate::layout::node::NodeMarks::default(), &mut visited);
                }
                owner.transition_layer.retain(|&x| x != idx);
            }
            if let Some(slot) = tslot {
                // Route through the shared teardown so the peer's layout
                // override (Compose `ResizeMode`/`PlaceHolderSize`) is dropped
                // and its natural size restored — clearing only the visual
                // would freeze the peer at its last animated size forever.
                // The override carries the OWNER's identity (the composer whose
                // map holds the flight = main), not the peer's: that is what the
                // cross-writer stamps on the node.
                let owner_key = FlightKey { cid: all[0].composer_id, id };
                let peer = &mut all[ti];
                peer.clear_transition_for_slot(slot, owner_key);
            }
            all[0].shared_flights.remove(&id);
        }
    }

    /// Full Tier1 teardown across composers (stale/cancel paths). Owner arena
    /// may be gone (overlay closed with retained node) — then the arena died
    /// wholesale and there is nothing to free or leak.
    pub(crate) fn cancel_cross_flight(all: &mut [&mut Composer], id: FlightId) {
        let Some(a) = all[0].shared_flights.remove(&id) else {
            return;
        };
        if let (Some(slot), Some(idx)) = (a.flight.source_slot, a.source_idx) {
            if let Some(owner) = all.iter_mut().find(|c| c.composer_id == a.source_cid) {
                if owner.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                    let mut visited = crate::layout::node::NodeMarks::default();
                    owner.arena.free_node_skip(idx, &crate::layout::node::NodeMarks::default(), &mut visited);
                }
                owner.transition_layer.retain(|&x| x != idx);
            }
        }
        if let Some(slot) = a.flight.target_slot {
            // Same as the completion path: the peer's layout override must go
            // with the flight (see `clear_transition_for_slot`). Verified
            // load-bearing: reverting both Tier-1 clears makes
            // `stale_cancel_drops_the_surviving_peers_layout_override` fail with
            // the override still attached. Owner = the composer that held the
            // flight, i.e. the one whose map it was removed from — `all[0]`, the
            // same key the writer STAMPS. Using the source's cid here was wrong for a
            // PEER-SOURCED flight (overlay -> main, the "hero returns" direction): it
            // looked for a key nobody wrote, so the override was never cleared and the
            // element stayed frozen at the outgoing size forever (review round 3,
            // reproduced with a live override dump).
            let owner_key = FlightKey { cid: all[0].composer_id, id };
            if let Some(peer) = all.iter_mut().find(|c| c.composer_id == a.target_cid) {
                peer.clear_transition_for_slot(slot, owner_key);
            }
        }
    }

    /// Write both ends' visuals for a Tier1 flight, each in its writer's
    /// canvas frame (window coords minus writer origin).
    ///
    /// NOTE (Tier1 limitation): overlay enter/exit animations (scale/offset
    /// around `screen_pos`) are NOT folded into the visuals — while the panel
    /// animates (~200ms) the flight leads/lags the panel transform by that
    /// delta. Fixing it means plumbing the overlay progress into cross-poll.
    pub(crate) fn write_cross_visuals(all: &mut [&mut Composer], owner_idx: usize, peer_idx: usize, id: FlightId) {
        let snapshot = all[0].shared_flights.get(&id).map(|a| {
            (
                a.start,
                a.flight.end.unwrap_or(a.start),
                a.flight.progress,
                a.radius_from,
                a.radius_to,
                a.clip,
                a.flight.source_slot,
                a.flight.target_slot,
                a.source_idx,
                a.start_scroll,
                a.end_scroll,
                a.path,
                a.bounds_fx.clone(),
                a.target_in_overlay,
                a.resize.clone(),
                a.placeholder,
                a.target_size,
                a.measure.clone(),
                a.radius_from_auto,
                a.radius_to_auto,
            )
        });
        let (
            start,
            end,
            pr,
            rf,
            rt,
            clip,
            sslot,
            tslot,
            sidx,
            sscroll,
            escroll,
            path,
            fx,
            in_overlay,
            resize,
            placeholder,
            target_size,
            measure_state,
            radius_from_auto,
            radius_to_auto,
        ) = match snapshot {
            Some(v) => v,
            None => return,
        };
        let (so, to) = (all[owner_idx].screen_origin, all[peer_idx].screen_origin);
        // Same rule as the Tier-0 writer: a straight path paints the flown edges, an arc bends the
        // parameterised rect instead.
        let flown = all[0].shared_flights.get(&id).map(|a| a.edges.peek().bounds());
        let flown_source = if matches!(path, PathMotion::Linear) {
            flown.map(|r| off_origin(r, so))
        } else {
            None
        };
        let flown_target = if matches!(path, PathMotion::Linear) {
            flown.map(|r| off_origin(r, to))
        } else {
            None
        };
        if let (Some(slot), Some(idx)) = (sslot, sidx) {
            let owner = &mut all[owner_idx];
            if owner.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                owner.arena.nodes[idx].transition = Some(TransitionVisual {
                    start: off_origin(start, so),
                    end: off_origin(end, so),
                    progress: pr,
                    flown: flown_source,
                    // Tier 1 (cross-composer) ends carry no scene id yet: the scene host API is
                    // published per composer, so a flight spanning main and an overlay has no shared
                    // scene visibility to read here. Keep the flight's own crossfade.
                    scene_alpha: None,
                    role: TransitionRole::Source,
                    radius_from: rf,
                    radius_to: rt,
                    clip,
                    link_slot: tslot,
                    scroll: sscroll,
                    flight: id,
                    path,
                    bounds_fx: fx.clone(),
                    // Detached leaving end: the layer is its only home.
                    elevated: true,
                    remeasure: false,
                    scale_parts: None,
                    radius_from_auto,
                    radius_to_auto,
                });
            }
        }
        if let Some(slot) = tslot {
            // Ownership key BEFORE the mutable borrow: the flight lives in the
            // MAIN map, so its composer id is what namespaces the override.
            let owner_cid = all[0].composer_id;
            let peer = &mut all[peer_idx];
            if let Some(root) = peer.arena.root {
                if let Some(tidx) = find_idx_by_slot(&peer.arena.nodes, root, slot) {
                    // Layout contract for the peer end, identical to Tier 0.
                    let remeasure = matches!(resize, ResizeMode::RemeasureToBounds);
                    let frame = placeholder_frame(
                        remeasure,
                        animated_size(&start, &end, pr, path),
                        placeholder,
                        target_size,
                    );
                    measure_state.set(frame);
                    let key = peer.arena.nodes[tidx].slot_key;
                    let mine = FlightKey { cid: owner_cid, id };
                    // Same guard as the Tier-0 writer: a different flight's
                    // override on this node must survive (ids are per-composer).
                    let foreign = peer.arena.nodes[tidx]
                        .flight_measure
                        .as_ref()
                        .is_some_and(|f| f.owner != mine);
                    let had_override = !foreign && peer.arena.nodes[tidx].flight_measure.is_some();
                    if !foreign {
                        peer.arena.nodes[tidx].flight_measure = if frame.is_idle() {
                            None
                        } else {
                            Some(FlightMeasure { frame: measure_state, owner: mine })
                        };
                    }
                    if !frame.is_idle() || had_override {
                        peer.layout_dirty_keys.insert(key);
                    }
                    // Same first-attach signal as the Tier-0 writer, but on the PEER: the
                    // peer's own parent is the one that saw the natural size this frame, so
                    // the peer composer is the one that needs the extra pass.
                    if !frame.is_idle() && !had_override {
                        peer.layout_override_fresh = true;
                    }
                    peer.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start: off_origin(start, to),
                        end: off_origin(end, to),
                        progress: pr,
                        flown: flown_target,
                        // See the Tier 1 source comment above: no scene id across composers yet.
                        scene_alpha: None,
                        role: TransitionRole::Target,
                        radius_from: rf,
                        radius_to: rt,
                        clip,
                        link_slot: None,
                        // Rootless in the peer's layer — no ancestor translate.
                        scroll: if in_overlay { (0.0, 0.0) } else { escroll },
                        flight: id,
                        path,
                        bounds_fx: fx,
                        elevated: in_overlay,
                        remeasure,
                        scale_parts: resize.scale_to_bounds_parts(),
                        radius_from_auto,
                        radius_to_auto,
                    });
                    if in_overlay {
                        peer.elevated_roots.push(tidx);
                    }
                }
            }
        }
    }

    /// Pair one stashed source with a freshly-appeared counterpart in another
    /// composer, or free it same-frame when unmatched (invisible removal).
    /// Freshness (peer lacked the key last frame) plus Tier0-busy guards keep
    /// stable duplicates (same hero shown in two places) from ever pairing.
    pub(crate) fn match_pending_source(all: &mut [&mut Composer], owner_idx: usize, p: PendingSource) {
        let kk = (p.scope_id, p.key.clone());
        for peer_idx in 0..all.len() {
            if peer_idx == owner_idx {
                continue;
            }
            // --- reads (shared, sequential — never aliased) ---
            struct Hit {
                slot: u64,
                end: SharedBounds,
                end_scroll: (f32, f32),
                radius_to: [f32; 4],
                spec: AnimationSpec,
                clip: bool,
                path: PathMotion,
                bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
                peer_cid: u64,
                /// Peer target's `renderInOverlayDuringTransition`.
                target_in_overlay: bool,
                /// Peer target's layout contract (Compose `ResizeMode` /
                /// `PlaceHolderSize`) and its natural size.
                resize: ResizeMode,
                placeholder: PlaceHolderSize,
                target_size: crate::unit::Size,
                /// Peer target's corner kind (Compose percent vs fixed).
                radius_to_auto: bool,
            }
            let hit: Option<Hit> = (|| {
                let peer = &all[peer_idx];
                // Fresh appearance THIS frame only (stable duplicates never
                // pair — peer prev maps absorb appearances before cross runs).
                let slot = peer
                    .fresh_shared
                    .iter()
                    .find(|((s, k), _)| *s == kk.0 && *k == kk.1)
                    .map(|(_, sl)| *sl)?;
                // Tier0-active keys belong to Tier0 (both maps).
                if peer.shared_flights.values().any(|a| {
                    !is_terminal(a.flight.phase) && a.flight.scope_id == kk.0 && a.flight.key == kk.1
                }) {
                    return None;
                }
                if all[0].shared_flights.values().any(|a| {
                    !is_terminal(a.flight.phase) && a.flight.scope_id == kk.0 && a.flight.key == kk.1
                }) {
                    return None;
                }
                let root = peer.arena.root?;
                let tidx = find_idx_by_slot(&peer.arena.nodes, root, slot)?;
                let tid = peer.arena.nodes[tidx].id;
                let (lx, ly) = crate::app::node_abs_position(&peer.arena.nodes, root, tid);
                let (po_x, po_y) = peer.screen_origin;
                let (tw, th, marker) = {
                    let n = &peer.arena.nodes[tidx];
                    (
                        n.measured_size.width,
                        n.measured_size.height,
                        find_shared_marker(&n.modifier),
                    )
                };
                let (
                    spec,
                    clip,
                    path,
                    enter,
                    target_is_bounds,
                    target_in_overlay,
                    resize,
                    placeholder,
                ) = match marker {
                    Some(m) => (
                        m.transform.spec.clone(),
                        shared_clip_for_kind(&m.kind),
                        m.path,
                        m.enter,
                        matches!(m.kind, SharedKind::Bounds { .. }),
                        m.render_in_overlay,
                        shared_resize_for_kind(&m.kind),
                        shared_placeholder_for_kind(&m.kind),
                    ),
                    None => (
                        BoundsTransform::default().spec,
                        false,
                        PathMotion::Linear,
                        None,
                        false,
                        true,
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                    ),
                };
                let radius_to = {
                    let n = &peer.arena.nodes[tidx];
                    shared_shape_radii(&n.modifier, tw, th)
                };
                // Freeze the peer target's ancestor scroll sum (BLOCKER #1).
                let peer_id_to_idx: HashMap<u64, usize> = peer
                    .arena
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.id, i))
                    .collect();
                let end_scroll = ancestor_scroll_sum(&peer.arena.nodes, &peer_id_to_idx, tidx);
                // Exit from the owner-side retained source marker (fade-out
                // fallback when absent — same rule as Tier 0).
                let source_marker = all[owner_idx]
                    .arena
                    .nodes
                    .get(p.src_idx)
                    .and_then(|n| find_shared_marker(&n.modifier));
                if let Some(ref sm) = source_marker {
                    let source_is_bounds = matches!(sm.kind, SharedKind::Bounds { .. });
                    if source_is_bounds != target_is_bounds {
                        crate::debug_log!(
                            "[shared] mixed-kind pair scope={} key={} — flight follows the target side",
                            kk.0,
                            kk.1
                        );
                    }
                }
                let bounds_fx = match enter {
                    Some(enter) => {
                        let exit = source_marker
                            .and_then(|m| m.exit)
                            .unwrap_or_default();
                        Some((enter, exit))
                    }
                    None => None,
                };
                Some(Hit {
                    slot,
                    end: SharedBounds::new(lx + po_x, ly + po_y, tw, th),
                    end_scroll,
                    radius_to,
                    spec,
                    clip,
                    path,
                    bounds_fx,
                    peer_cid: peer.composer_id,
                    target_in_overlay,
                    resize: resize.clone(),
                    placeholder,
                    target_size: peer.arena.nodes[tidx].measured_size,
                    radius_to_auto: shared_shape_radius_is_auto(&peer.arena.nodes[tidx].modifier),
                })
            })();
            let Some(h) = hit else { continue };
            // --- writes (sequential &mut, never aliased) ---
            let owner_cid = all[owner_idx].composer_id;
            let id = all[0].next_flight_id;
            all[0].next_flight_id += 1;
            let mut flight = Flight::new(id, p.scope_id, p.key.clone());
            let acts = flight.on_event(FlightEvent::CounterpartAppeared {
                source_slot: p.old_slot,
                target_slot: h.slot,
            });
            debug_assert_eq!(
                acts,
                vec![FlightAction::CollectBounds { source_slot: p.old_slot, target_slot: h.slot }]
            );
            let acts = flight.on_event(FlightEvent::BoundsReady { start: p.start, end: h.end });
            debug_assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]));
            // The cross-composer flight animates its four edges, plus the scalar clock its content
            // crossfade rides.
            let edges = State::new(crate::transition::FlightEdges::of(&p.start));
            push_animatable(
                edges.clone(),
                crate::transition::FlightEdges::of(&h.end),
                h.spec.clone(),
            );
            let progress = State::new(0.0f32);
            push_animatable(
                progress.clone(),
                1.0,
                crate::transition::BoundsTransform::clock_spec(&h.spec),
            );
            all[0].shared_flights.insert(
                id,
                ActiveFlight {
                    flight,
                    progress,
                    edges,
                    spec: h.spec,
                    source_idx: Some(p.src_idx),
                    start: p.start,
                    // Detached sources render rootless — no ancestor scroll.
                    start_scroll: (0.0, 0.0),
                    end_scroll: h.end_scroll,
                    radius_from: p.radius_from,
                    radius_to: h.radius_to,
                    // Source kind was captured at stash time; the target's
                    // comes from the peer marker (see `Hit`).
                    radius_from_auto: p.radius_from_auto,
                    radius_to_auto: h.radius_to_auto,
                    clip: h.clip,
                    path: h.path,
                    bounds_fx: h.bounds_fx,
                    target_in_overlay: h.target_in_overlay,
                    resize: h.resize,
                    placeholder: h.placeholder,
                    // Resolved at match time: the peer node was just laid out.
                    target_size: Some(h.target_size),
                    measure: State::new(FlightMeasureFrame::IDLE),
                    source_cid: owner_cid,
                    target_cid: h.peer_cid,
                },
            );
            Self::write_cross_visuals(&mut *all, owner_idx, peer_idx, id);
            return;
        }
        // Unmatched: free retained same-frame (invisible plain removal).
        {
            let owner = &mut all[owner_idx];
            if owner.arena.nodes.get(p.src_idx).is_some_and(|n| n.slot_key == p.old_slot) {
                let mut visited = crate::layout::node::NodeMarks::default();
                owner.arena.free_node_skip(p.src_idx, &crate::layout::node::NodeMarks::default(), &mut visited);
            }
            owner.transition_layer.retain(|&x| x != p.src_idx);
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 3: same-screen size morph (animateBounds)
// ═══════════════════════════════════════════════════════════

/// Size-delta threshold for morph detection (layout pixels). Position-only
/// moves (scroll, sibling shifts) never trigger — scroll immunity by
/// construction, since scrolling never changes measured sizes.
pub(crate) const MORPH_EPS: f32 = 0.5;

pub(crate) fn size_delta(a: &SharedBounds, b: &SharedBounds) -> f32 {
    (a.width - b.width).abs().max((a.height - b.height).abs())
}

