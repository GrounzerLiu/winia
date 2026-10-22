use super::*;

impl Composer {
    /// (scope, key) → slot for marked nodes in the CURRENT tree
    /// (post-materialize). First registration wins on duplicates (user error).
    pub(crate) fn shared_live_map(&self) -> HashMap<(u64, String), u64> {
        // Two LIVE ends of one key happen when a scene host keeps the outgoing and the incoming
        // scene composed at once (winia's nav). Which of them is "the" live end decides the whole
        // flight: the tree walk's order is not meaningful, and during a nav transition it alternates
        // between the two scenes every frame, so the flight system reads a fresh switch each frame
        // and starts/retargets flights instead of flying once (measured: 2 `begin_flight` calls per
        // navigate, 131-143 duplicate warnings).
        //
        // The winner is the end in the scene that is NOT leaving (`nav_scene_is_prev`), which is stable for
        // the whole transition. It is deliberately NOT "the more visible end": a leaving scene starts at
        // visibility 1 and the entering one at 0, so comparing visibilities picks the LEAVING end at the
        // switch frame (measured: the flight's target was the 96x96 list hero and it never grew). With no
        // scene information at all, the last one wins as before.
        let mut out: HashMap<(u64, String), (u64, Option<bool>)> = HashMap::new();
        if let Some(root) = self.arena.root {
            // Built ONCE for the whole walk: `scene_of_node` used to build this map per call, and this
            // walk calls it once per marked node (measured: 2.68 ms per call on a 12 000-node tree, so
            // 50 marked nodes cost ~134 ms per frame).
            let id_to_idx: HashMap<u64, usize> = self
                .arena
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id, i))
                .collect();
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                let node = &self.arena.nodes[idx];
                if let Some(m) = find_shared_marker(&node.modifier) {
                    // Scene membership comes from the ANCESTOR chain, not from the marker: a marker's
                    // modifier element is built once and reused, so an id stored there freezes its
                    // first value (measured: one end `scene=None`, the other the LEAVING scene, which
                    // made a nav flight pair the leaving hero with itself).
                    //
                    // The winner is the end in the scene that is NOT leaving — the surviving side. It is
                    // deliberately not "the more visible end": a leaving scene starts at visibility 1
                    // and the entering one at 0, so comparing visibilities picks the LEAVING end at the
                    // switch frame (measured: the flight's target was the 96x96 list hero, so it
                    // crossfaded without ever growing while a same-screen morph supplied the growth).
                    let scene = scene_of_node_with(&self.arena.nodes, &id_to_idx, idx);
                    let is_prev = scene.and_then(nav_scene_is_prev);
                    let key = (m.scope_id, m.key.clone());
                    match out.get(&key) {
                        Some(&(_, prev_is_prev)) => {
                            let replace = match (prev_is_prev, is_prev) {
                                // Prefer the surviving scene; if both are the same side (or a scene
                                // host is not involved at all), keep the last one, as before.
                                (Some(true), Some(false)) => true,
                                (Some(false), Some(true)) => false,
                                _ => true,
                            };
                            if replace {
                                out.insert(key, (node.slot_key, is_prev));
                            }
                        }
                        None => {
                            out.insert(key, (node.slot_key, is_prev));
                        }
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        out.into_iter().map(|(k, (slot, _))| (k, slot)).collect()
    }

    /// Transition-layer render order (detached sources + elevated in-tree
    /// endpoints, z-sorted back-to-front). Painted after the main tree, hit
    /// tested before it — one list, so paint order and hit order cannot drift.
    pub(crate) fn transition_roots(&self) -> &[usize] {
        &self.layer_order
    }

    /// Mark (or clear) the non-shared subtrees that must render above the
    /// flights while their scope is transitioning — Compose
    /// `Modifier.renderInSharedTransitionScopeOverlay`.
    ///
    /// `active` is the set of scopes with a non-terminal flight, supplied by
    /// the caller: a composer's own poll passes its own flight scopes (exact
    /// for the main composer, which owns Tier 1 too), and the window-level
    /// cross-poll passes the union across all composers, because a peer
    /// composer cannot see a Tier1 flight (it lives in the main map). Reading
    /// the app-facing `is_transition_active()` state instead would tie
    /// elevation to whether *somebody happened to subscribe*.
    ///
    /// Idle frames (empty `active`) return before the arena walk.
    pub(crate) fn refresh_scope_overlay_roots(&mut self, active: &HashSet<u64>) {
        for &idx in &self.scope_overlay_roots {
            if let Some(n) = self.arena.nodes.get_mut(idx) {
                n.paint = PaintDisposition::InTree;
            }
        }
        self.scope_overlay_roots.clear();
        if active.is_empty() {
            return; // idle: no arena walk
        }
        let Some(root) = self.arena.root else { return };
        let mut marked: Vec<usize> = Vec::new();
        {
            let nodes = &self.arena.nodes;
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                let node = &nodes[idx];
                if let Some((sid, _)) = find_scope_overlay_marker(&node.modifier) {
                    if active.contains(&sid) {
                        marked.push(idx);
                        // Keep descending: a chrome marker nested inside another one gets its OWN layer entry
                        // (`rebuild_layer_order` includes every entry of this list, and `render_layer` draws
                        // each one with `layer_root = true`), which is what keeps its own `zIndexInOverlay`
                        // in the sort and lets it escape its ancestor's clip. An earlier version skipped it
                        // on the theory that it would be skipped twice and paint nowhere; that was wrong (it
                        // was painted exactly once, as its own entry) and suppressing it silently downgraded
                        // the nested bar to an ordinary child of its ancestor's draw.
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        for &idx in &marked {
            self.arena.nodes[idx].paint = PaintDisposition::InLayer;
        }
        self.scope_overlay_roots = marked;
    }

    /// Rebuild [`Self::transition_roots`] from live state: every elevated
    /// in-tree endpoint written this frame ([`Self::elevated_roots`]), every
    /// detached source ([`Self::transition_layer`]), and the chrome that opted
    /// into the scope overlay ([`Self::scope_overlay_roots`]).
    /// Stable z-sort (`zIndexInOverlay`) — equal z keeps that insertion order,
    /// i.e. the entering target is painted first (under its own ghost, exactly
    /// like the in-tree + ghost compositing the overlay pass replaces) and
    /// chrome last (a bar that opts in expects to sit on top). Flights that
    /// never touch `z_index` therefore render as they did before the pass
    /// existed. When one node carries both markers the SHARED marker's z wins.
    pub(crate) fn rebuild_layer_order(&mut self) {
        let mut order: Vec<usize> = self.elevated_roots.clone();
        order.extend(self.transition_layer.iter().copied());
        // Chrome last at equal z: a bar that opts into the overlay expects to
        // sit on top of the flying pair unless it asks for less.
        order.extend(self.scope_overlay_roots.iter().copied());
        // Dedup (a detached source can be reached from both lists) while
        // preserving first-seen order, then z-sort back-to-front.
        let mut seen = HashSet::new();
        order.retain(|idx| seen.insert(*idx));
        let z_of = |nodes: &[LayoutNode], idx: usize| -> f32 {
            let Some(n) = nodes.get(idx) else { return 0.0 };
            if let Some(m) = find_shared_marker(&n.modifier) {
                m.z_index
            } else {
                find_scope_overlay_marker(&n.modifier).map(|(_, z)| z).unwrap_or(0.0)
            }
        };
        order.sort_by(|&a, &b| {
            z_of(&self.arena.nodes, a)
                .partial_cmp(&z_of(&self.arena.nodes, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.layer_order = order;
    }

    /// Draw the transition layer on `canvas` (the same canvas frame the main
    /// tree was drawn in). Each root is re-rendered rootless at its absolute
    /// position, so ancestor clips and ancestor layer transforms — which are
    /// still on the real canvas for the tree — do not apply: the Compose
    /// overlay elevation. Flight transforms themselves come from
    /// `render_pass1`'s `TransitionVisual` block, unchanged.
    pub(crate) fn render_layer(&self, canvas: &skia_safe::Canvas) {
        if self.layer_order.is_empty() {
            return;
        }
        let Some(root) = self.arena.root else { return };
        let nodes = &self.arena.nodes;
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        for &idx in &self.layer_order {
            let Some(n) = nodes.get(idx) else { continue };
            if n.transition.is_none() && n.paint != PaintDisposition::InLayer {
                continue;
            }
            let abs = abs_rect_upward(nodes, &id_to_idx, idx);
            crate::render::render_node_at(nodes, root, idx, canvas, abs.x, abs.y);
        }
    }

    /// Compose-tail hook (MUST run after materialize, before the prev drain):
    /// detect switches, cancel dead/retargeted flights, retain+detach sources.
    pub(crate) fn retain_shared_sources(&mut self) {
        let live = self.shared_live_map();
        // Fresh appearances this frame (cross-composer Tier1 matching reads
        // these; overwritten every frame — peer prev maps absorb appearances
        // before cross-poll runs, so this is the only freshness source).
        self.fresh_shared = live
            .iter()
            .filter(|(k, _)| !self.prev_shared_endpoints.contains_key(k))
            .map(|(k, s)| ((k.0, k.1.clone()), *s))
            .collect();
        if live.is_empty() && self.prev_shared_endpoints.is_empty() && self.shared_flights.is_empty() {
            return; // fast path: no shared content anywhere
        }
        // Cancel flights whose key left the live tree with no counterpart
        // (screen torn down around a flight). Tier1 (cross-composer, main map)
        // is cross-owned — per-composer sweeps must not touch it.
        let live_keys: HashSet<(u64, String)> = live.keys().cloned().collect();
        let dead: Vec<FlightId> = self
            .shared_flights
            .iter()
            .filter(|(_, a)| {
                !is_terminal(a.flight.phase)
                    && a.source_cid == self.composer_id
                    && a.target_cid == self.composer_id
                    && !live_keys.contains(&(a.flight.scope_id, a.flight.key.clone()))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            self.cancel_flight(id, "key_left_live_tree");
        }
        // Switches (fresh + retarget-lite).
        for c in detect_switch(&self.prev_shared_endpoints, &live) {
            if crate::debug::animation::enabled() {
                crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
                    format!("flight:{}", c.key),
                    "candidate",
                    format!("old={:#x} new={:#x}", c.old_slot, c.new_slot),
                ));
            }
            // Retarget-lite: same key already flying → cancel old (freeing its
            // retained node), restart from the current visual rect. Own-map
            // Tier0 only — Tier1 retargets resolve in cross-poll (which sees
            // both composers); touching them here would strand remote visuals.
            let mut start_override: Option<(SharedBounds, [f32; 4])> = None;
            if let Some(id) = self
                .shared_flights
                .iter()
                .find(|(_, a)| {
                    !is_terminal(a.flight.phase)
                        && a.source_cid == self.composer_id
                        && a.target_cid == self.composer_id
                        && a.flight.scope_id == c.scope_id
                        && a.flight.key == c.key
                })
                .map(|(id, _)| *id)
            {
                // Two shapes of candidate for an ALREADY-FLYING key, and they need opposite handling:
                //
                // * The end that moved is this flight's target: the element is the same one (same scope,
                //   same key) and it was merely re-slotted inside its scene while the flight runs — a
                //   scene host re-arranges layers mid-transition and winia slots are positional. Compose's
                //   identity here is the KEY (`rememberSharedContentState(key)`), not the position, and a
                //   position change updates the node in place. Retargeting this case cancelled a running
                //   crossfade and then bailed in `detach_source` (the old slot was already gone), so
                //   nothing flew and a same-screen morph — opacity-immune by design — took over.
                //   Rebind: keep the progress and the start rect, re-point the target slot, clear the
                //   stale node's visuals.
                // * The candidate's slots are this flight's own two ends REVERSED: that is a genuine
                //   navigation back (measured: clicking Back while the push flight still runs produces
                //   exactly `old=target new=source`), so it must go through retarget below.
                let target_moved = self
                    .shared_flights
                    .get(&id)
                    .is_some_and(|a| {
                        a.flight.target_slot == Some(c.old_slot)
                            && a.flight.source_slot != Some(c.new_slot)
                            && a.source_idx.is_some()
                    });
                if target_moved {
                    if let Some(a) = self.shared_flights.get_mut(&id) {
                        a.flight.target_slot = Some(c.new_slot);
                    }
                    let key = self
                        .shared_flights
                        .get(&id)
                        .map(|a| a.flight.key.clone())
                        .unwrap_or_default();
                    self.clear_transition_for_slot(c.old_slot, FlightKey { cid: self.composer_id, id });
                    if crate::debug::animation::enabled() {
                        crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
                            format!("flight:{key}"),
                            "rebind",
                            format!("target {:#x} -> {:#x}", c.old_slot, c.new_slot),
                        ));
                    }
                    continue;
                }
                if let Some(a) = self.shared_flights.get(&id) {
                    // Unclamped + path-aware: retarget continuity follows the
                    // true visual rect, including spring overshoot past the
                    // old end and any arc bend. NOTE: enter/exit channel
                    // offsets (slide/scale) restart from full — a mid-flight
                    // redirect of a slide-bounds flight may snap by the live
                    // offset ( folding scale pivots into a rect is
                    // ill-defined; Element retargets stay seamless).
                    let end = a.flight.end.unwrap_or(a.start);
                    // The visual rect the redirect starts from: the flown edges, arc bend included.
                    let p = a.progress_value();
                    let s = lerp_flight_rect(&a.start, &end, p, a.path);
                    let (rf, rt) = (a.radius_from, a.radius_to);
                    start_override = Some((s, [0, 1, 2, 3].map(|i| rf[i] + (rt[i] - rf[i]) * p)));
                }
                self.cancel_flight(id, "retarget_same_key");
            }
            self.begin_flight(c, start_override);
        }
        // Stash unmatched disappearances for cross-composer matching (Tier1).
        // Freed by cross-poll same frame when unmatched — invisible either way.
        // (Tier1-active keys are cross-owned; never re-stash them. Fresh Tier0
        // targets above are live, not disappeared.)
        let gone: Vec<((u64, String), u64)> = self
            .prev_shared_endpoints
            .iter()
            .filter(|(k, _)| !live.contains_key(k))
            .map(|(k, s)| ((k.0, k.1.clone()), *s))
            .filter(|(k, _)| {
                !self.shared_flights.values().any(|a| {
                    !is_terminal(a.flight.phase) && a.flight.scope_id == k.0 && a.flight.key == k.1
                })
            })
            .collect();
        for ((scope_id, key), old_slot) in gone {
            if let Some((src_idx, start, radius_from)) = self.detach_source(old_slot) {
                self.pending_cross.push(PendingSource {
                    scope_id,
                    key,
                    old_slot,
                    src_idx,
                    start,
                    radius_from,
                    radius_from_auto: self
                        .arena
                        .nodes
                        .get(src_idx)
                        .is_some_and(|n| shared_shape_radius_is_auto(&n.modifier)),
                });
            }
        }
        self.prev_shared_endpoints = live;
        // The listing prune used to be called here, which quietly tied an arena invariant to this
        // hook's fast path (`return` above when nothing is shared). It now runs in `compose()`
        // right after this call — same position in the frame, but for every composer.
    }

    /// Detach a vanished marked node (freeze): unlink from old parents, pin to
    /// absolute OWNER-canvas position, shelter from the drain. Returns arena
    /// index + exact start bounds (WINDOW coords) + start radii, or `None`
    /// when the node object was repurposed by the new tree (same-key reuse).
    pub(crate) fn detach_source(&mut self, old_slot: u64) -> Option<(usize, SharedBounds, [f32; 4])> {
        let src_idx = self.prev_node_by_key.remove(&old_slot)?;
        // Unlink from any still-referencing old parent (objects alive pre-drain;
        // reused ancestors already dropped the ref via children.clear()).
        for pidx in self.prev_node_by_key.values() {
            self.arena.nodes[*pidx].children.retain(|&x| x != src_idx);
        }
        // Belt-and-braces: the drain below must never reclaim it.
        self.reused_nodes.insert(src_idx);
        // A ghost must not own what the NEW tree already took over. `reused_nodes` at this point
        // holds exactly this frame's reuse decisions (materialize inserts them and the set is
        // cleared at the end of each compose), so a child listed there has been re-parented into
        // the frame's tree. Keeping it would make the ghost paint a LIVE node and then free it:
        // teardown releases a ghost's subtree with an empty skip set (probe from review:
        // `live_parent.children=[2] live_child.slot_key=0x0 pool=[2, 3]`). Compose never shares a
        // node between the outgoing and incoming content, so dropping it is also the aligned
        // behaviour; a true fix would freeze a copy of the subtree.
        {
            let taken: Vec<usize> = self.arena.nodes[src_idx]
                .children
                .iter()
                .copied()
                .filter(|c| self.reused_nodes.contains(*c))
                .collect();
            if !taken.is_empty() {
                crate::debug_log!(
                    "[detach] ghost {src_idx} drops {} child(ren) the new tree reused: {taken:?}",
                    taken.len()
                );
                self.arena.nodes[src_idx]
                    .children
                    .retain(|c| !taken.contains(c));
            }
        }
        // …and neither may it reclaim any DESCENDANT. Sheltering only the root let the pool hand
        // this subtree's indices to nodes of the new tree, so the frozen ghost listed a child
        // that had been recycled into a fresh, not-yet-measured node: measured, the ghost's only
        // child was `0x0` while the ghost's own rect, alpha and layer order were all correct — a
        // card that flies with nothing inside it, i.e. the reported "the animation starts fully
        // transparent". A frozen ghost owns its subtree for as long as it is in the layer.
        {
            let mut stack = self.arena.nodes[src_idx].children.clone();
            while let Some(c) = stack.pop() {
                if self.reused_nodes.insert(c) {
                    stack.extend(self.arena.nodes[c].children.iter().copied());
                }
            }
        }
        let id_to_idx: HashMap<u64, usize> =
            self.arena.nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut b = abs_rect_upward(&self.arena.nodes, &id_to_idx, src_idx);
        let (ox, oy) = self.screen_origin;
        b.x += ox;
        b.y += oy;
        let r = {
            let n = &self.arena.nodes[src_idx];
            shared_shape_radii(&n.modifier, n.measured_size.width, n.measured_size.height)
        };
        // Detach: absolute position IN THE OWNER'S CANVAS FRAME, rootless,
        // transition-layer owned.
        {
            let n = &mut self.arena.nodes[src_idx];
            n.position = crate::unit::Offset::new(b.x - ox, b.y - oy);
            n.parent_id = None;
        }
        self.transition_layer.push(src_idx);
        self.sort_transition_layer_by_z();
        Some((src_idx, b, r))
    }

    /// Stable z-order for retained ghosts (Compose `zIndexInOverlay`):
    /// transition roots render back-to-front, so ascending z paints
    /// higher-z pairs on top. Stable — equal z keeps detach (push) order,
    /// which is exactly today's behavior when nobody sets z. Read lazily
    /// off each retained node's own marker (no flight plumbing needed);
    /// in-tree targets keep tree order (documented Tier 0 limitation).
    pub(crate) fn sort_transition_layer_by_z(&mut self) {
        let z_of = |nodes: &[LayoutNode], idx: usize| -> f32 {
            nodes
                .get(idx)
                .and_then(|n| find_shared_marker(&n.modifier))
                .map(|m| m.z_index)
                .unwrap_or(0.0)
        };
        self.transition_layer.sort_by(|&a, &b| {
            z_of(&self.arena.nodes, a)
                .partial_cmp(&z_of(&self.arena.nodes, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Detach the source node (freeze) and open an AwaitingBounds flight.
    /// `start_override`: retarget visual continuity (WINDOW coords); otherwise
    /// the exact upward-walk bounds (no lookahead needed).
    pub(crate) fn begin_flight(&mut self, c: SwitchCandidate, start_override: Option<(SharedBounds, [f32; 4])>) {
        let (src_idx, walked, walked_r) = match self.detach_source(c.old_slot) {
            Some(v) => v,
            None => return,
        };
        let (start, radius_from) = start_override.unwrap_or((walked, walked_r));
        let id = self.next_flight_id;
        self.next_flight_id += 1;
        let mut flight = Flight::new(id, c.scope_id, c.key.clone());
        let acts = flight.on_event(FlightEvent::CounterpartAppeared {
            source_slot: c.old_slot,
            target_slot: c.new_slot,
        });
        debug_assert_eq!(
            acts,
            vec![FlightAction::CollectBounds { source_slot: c.old_slot, target_slot: c.new_slot }]
        );
        // anim-trace lifecycle event: which key opened a flight, and between which slots.
        let scene_of = |slot: u64| {
            self.arena
                .root
                .and_then(|r| find_idx_by_slot(&self.arena.nodes, r, slot))
                .map(|i| scene_of_node(&self.arena.nodes, i))
                .unwrap_or(None)
        };
        crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
            format!("flight:{}", c.key),
            "start",
            format!(
                "id={id} old={:#x}(scene={:?}) new={:#x}(scene={:?})",
                c.old_slot,
                scene_of(c.old_slot),
                c.new_slot,
                scene_of(c.new_slot)
            ),
        ));
        self.shared_flights.insert(
            id,
            ActiveFlight {
                flight,
                // Placeholders until the bounds resolve and both animations are pushed.
                progress: State::new(0.0f32),
                edges: State::new(crate::transition::FlightEdges::of(&start)),
                spec: BoundsTransform::default().spec,
                source_idx: Some(src_idx),
                start,
                // Detached sources render rootless — no ancestor scroll.
                start_scroll: (0.0, 0.0),
                // Filled when the end resolves (AwaitingBounds poll).
                end_scroll: (0.0, 0.0),
                // Ditto for the motion path (target marker wins).
                path: PathMotion::Linear,
                // Ditto for enter/exit (resolved below for Bounds flights).
                bounds_fx: None,
                radius_from,
                radius_to: radius_from,
                // Source kind is known now; the target's is filled at resolve
                // (same moment as clip/resize/placeholder).
                radius_from_auto: self
                    .arena
                    .nodes
                    .get(src_idx)
                    .is_some_and(|n| shared_shape_radius_is_auto(&n.modifier)),
                radius_to_auto: false,
                clip: false,
                // Filled when the end resolves (AwaitingBounds poll).
                target_in_overlay: true,
                // Layout contract: frozen at resolve from the target marker.
                resize: ResizeMode::scale_to_bounds(),
                placeholder: PlaceHolderSize::JumpCut,
                target_size: None,
                measure: State::new(FlightMeasureFrame::IDLE),
                source_cid: self.composer_id,
                target_cid: self.composer_id,
            },
        );
    }

    /// Free a retained source subtree + drop its slot (fires on_remove —
    /// removal semantic) + clear the target's visuals if still present.
    /// Is a transition running in this scope NOW?
    ///
    /// Checks this composer's own map first, so a flight opened EARLIER IN THIS SAME FRAME counts (the
    /// switch handling runs before the morph detection): the globally synced table is only updated at the
    /// end of a poll, so it would say "idle" on the very frame a transition starts. The global table is
    /// still consulted for Tier 1, whose flights live in the main composer's map.
    pub(crate) fn scope_is_transitioning(&self, scope_id: u64) -> bool {
        self.shared_flights
            .values()
            .any(|a| a.flight.scope_id == scope_id && !is_terminal(a.flight.phase))
            || scope_transition_active(scope_id)
    }

    pub(crate) fn cancel_flight(&mut self, id: FlightId, reason: &'static str) {
        let Some(a) = self.shared_flights.remove(&id) else {
            return;
        };
        // anim-trace: a cancelled flight is the usual reason a crossfade stops mid-way, and nothing
        // else in a trace says so — record the key, the progress it died at, and WHICH call site did
        // it (five of them can, and they mean different bugs). Gated: with the feature off `record` is a
        // no-op but its arguments would still be built.
        if crate::debug::animation::enabled() {
            let p = a.progress_value();
            crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
                format!("flight:{}", a.flight.key),
                "cancel",
                format!(
                    "reason={reason} id={id} has_source={} p={:.4} flight_slots=({:?},{:?})",
                    a.source_idx.is_some(),
                    p,
                    a.flight.source_slot,
                    a.flight.target_slot
                ),
            ));
        }
        if let (Some(slot), Some(idx)) = (a.flight.source_slot, a.source_idx) {
            self.free_retained_source(idx, slot);
        }
        if let Some(slot) = a.flight.target_slot {
            self.clear_transition_for_slot(slot, FlightKey { cid: self.composer_id, id });
        }
    }

    pub(crate) fn free_retained_source(&mut self, idx: usize, slot: u64) {
        // Index + slot double-guarded: arena indices recycle, so the index
        // alone proves nothing.
        // NOTE: no slot-tree surgery here (see `cancel_flight`): keys are
        // positional identities resurrected on navigate-back; key-based
        // removal murders the live successor. Stale slots self-clean via
        // truncate/Enter-prune.
        if self.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
            let mut visited = crate::layout::node::NodeMarks::default();
            self.arena.free_node_skip(idx, &crate::layout::node::NodeMarks::default(), &mut visited);
        }
        self.transition_layer.retain(|&x| x != idx);
    }

    /// Clear a slot's visual only when it still belongs to `id`. Slots are
    /// positional identities a successor flight can resurrect between this
    /// flight's last write and its teardown — unconditional clearing would
    /// flicker one frame off the new visual (self-heals next poll, but
    /// avoidable for one tag comparison).
    pub(crate) fn clear_transition_for_slot(&mut self, slot: u64, owner: FlightKey) {
        // `arena.root` is None on an empty composition, but the nodes (and the
        // key→index map) can outlive it, so a rootless tree must still be swept
        // — otherwise an orphan override rides a key-reused node.
        let found = match self.arena.root {
            Some(root) => find_idx_by_slot(&self.arena.nodes, root, slot),
            None => self.arena.nodes.iter().position(|n| n.slot_key == slot),
        };
        if let Some(idx) = found {
            let owned = self.arena.nodes[idx]
                .transition
                .as_ref()
                .is_some_and(|t| t.flight == owner.id);
            if owned {
                self.arena.nodes[idx].transition = None;
            }
            // The layout override lives exactly as long as the flight it belongs
            // to: drop it (only when it is still THIS flight's) and seed one last
            // invalidation so the natural size comes back next pass — otherwise
            // the fold keeps the last animated size forever.
            if self.arena.nodes[idx]
                .flight_measure
                .as_ref()
                .is_some_and(|f| f.owner == owner)
            {
                let key = self.arena.nodes[idx].slot_key;
                self.arena.nodes[idx].flight_measure = None;
                self.layout_dirty_keys.insert(key);
            }
        }
        // …AND an arena-wide sweep, because the tree walk above cannot reach a
        // node that was DETACHED into the transition layer in the same frame
        // (retarget/replace): the ghost would keep its dead flight's override.
        // That is harmless today only because detached nodes are never measured —
        // an accident, not an invariant.
        let mut orphan_keys: Vec<u64> = Vec::new();
        for n in self.arena.nodes.iter_mut() {
            if n.flight_measure.as_ref().is_some_and(|f| f.owner == owner) {
                n.flight_measure = None;
                orphan_keys.push(n.slot_key);
            }
        }
        for k in orphan_keys {
            self.layout_dirty_keys.insert(k);
        }
    }

    /// Post-layout hook (app loop, every frame): fill ends, start flights,
    /// rewrite per-frame visual snapshots, reap completed flights. Render
    /// reads plain f32 snapshots — never subscribes (zero-recomposition).
    /// Post-layout hook (app loop, every frame): fill ends, start flights,
    /// rewrite per-frame visual snapshots, reap completed flights, detect
    /// same-screen size morphs. Render reads plain f32 snapshots — never
    /// subscribes (zero-recomposition).
    pub(crate) fn poll_shared_flights(&mut self) {
        // Layer membership is rebuilt from this frame's writes (Tier0 writes
        // here, Tier1 writes land in the same frame's cross-poll below).
        self.elevated_roots.clear();
        // Chrome elevation (Compose renderInSharedTransitionScopeOverlay) is a
        // per-frame decision, so it is recomputed here as well — from THIS
        // composer's flights; the cross-poll re-runs it with the window union.
        let own: HashSet<u64> = self
            .shared_flights
            .values()
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| a.flight.scope_id)
            .collect();
        self.refresh_scope_overlay_roots(&own);
        // Live marker map doubles for flight polling and morph detection —
        // one arena walk per frame.
        let live = self.shared_live_map();
        if !self.shared_flights.is_empty() {
            // Snapshot ids: actuators mutate the map.
            let ids: Vec<FlightId> = self.shared_flights.keys().copied().collect();
            for id in ids {
                self.poll_one_flight(id);
            }
        }
        self.poll_layout_morphs(&live);
        // Layer membership is derived from the visuals this poll just wrote.
        self.rebuild_layer_order();
        // Paint dispositions follow from both of the above, so they are the last thing computed each
        // frame (including which marked copies are placeholders a flight already paints).
        self.refresh_paint_dispositions();
        self.trace_marked_nodes();
        sync_scope_active_states(&[self]);
    }

    /// Recompute [`crate::layout::node::PaintDisposition`] for every node, once per frame.
    ///
    /// One place decides where each node's pixels come from, instead of three independent booleans read
    /// at the render site:
    /// - an end the flight lifted into the transition layer → `InLayer`;
    /// - chrome that opted into the scope overlay this frame → `InLayer`;
    /// - a marked copy of a (scope, key) a running flight already owns, which is not itself an end →
    ///   `Placeholder` (the flight paints the animated rect; a second static copy must not appear);
    /// - everything else → `InTree`.
    ///
    /// Assigning unconditionally each frame is what keeps it transient: a node whose flight ended goes
    /// straight back to `InTree` without any per-flight cleanup.
    ///
    /// `paint_dirty` makes the idle case cheap: with no flights, no chrome and nothing left to clear, the
    /// whole-arena walk is skipped. It is set whenever the pass assigns anything other than `InTree`, so the
    /// pass always runs at least once more after the last non-`InTree` frame (that is what clears them).
    pub(crate) fn refresh_paint_dispositions(&mut self) {
        let flying: Vec<(u64, String)> = self
            .shared_flights
            .values()
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
            .collect();
        self.refresh_paint_dispositions_with(&flying);
    }

    /// The pass with a caller-provided flight set. The window-level caller passes the UNION of every
    /// composer's non-terminal flights, because a Tier1 flight lives only in the main composer's map: a peer
    /// that asked its own map would keep painting a second copy of a key that is flying.
    pub(crate) fn refresh_paint_dispositions_with(&mut self, flying: &[(u64, String)]) {
        if flying.is_empty() && self.scope_overlay_roots.is_empty() && !self.paint_dirty {
            // NOTE: the scene-registry prune deliberately does NOT happen here. The registry is shared by
            // every composer on the thread, so its live set has to be the union of their arenas; pruning from
            // one composer's walk let a peer WITHOUT scene tags delete the entries of the composer that has
            // them (a peer with its own flight runs this pass with an empty tag set), which degraded the
            // pairing to last-wins for the rest of a transition. The window-scoped prune is
            // `prune_scene_registry_window`, called from `Composer::poll_cross_flights` where every composer
            // is at hand.
            return;
        }
        let chrome: HashSet<usize> = self.scope_overlay_roots.iter().copied().collect();
        let mut dirty = false;
        for (idx, n) in self.arena.nodes.iter_mut().enumerate() {
            // Compared against a small Vec rather than a HashSet of owned keys: probing the set needs an
            // owned `(u64, String)` per marked node (an allocation per node per frame), and the flight list
            // is a handful of entries at most.
            let placeholder = !chrome.contains(&idx)
                && n.transition.is_none()
                && find_shared_marker(&n.modifier)
                    .is_some_and(|m| flying.iter().any(|(s, k)| *s == m.scope_id && k == &m.key));
            n.paint = if chrome.contains(&idx) {
                PaintDisposition::InLayer
            } else if placeholder {
                PaintDisposition::Placeholder
            } else {
                PaintDisposition::InTree
            };
            dirty |= n.paint != PaintDisposition::InTree;
        }
        self.paint_dirty = dirty;
    }


    /// anim-trace: record EVERY marked node this frame — not just the ends of a flight. "How many
    /// copies of one key are painted, and where" is otherwise only answerable by looking at pixels, and
    /// a duplicate (a scene host re-composing a copy of a key that a flight already owns) shows up here
    /// as two records with the same `key`, different rects, one of them with `role: null`.
    pub(crate) fn trace_marked_nodes(&self) {
        if !crate::debug::animation::enabled() {
            return;
        }
        // One id→index map for the whole pass (see `scene_of_node_with`).
        let id_to_idx: HashMap<u64, usize> = self
            .arena
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
        for (idx, n) in self.arena.nodes.iter().enumerate() {
            let Some(m) = find_shared_marker(&n.modifier) else {
                continue;
            };
            let role = n.transition.as_ref().map(|t| match t.role {
                TransitionRole::Source => "Source",
                TransitionRole::Target => "Target",
                TransitionRole::Morph => "Morph",
            });
            let mut rec = crate::debug::animation::TraceRecord::flight(format!(
                "mark:{}#{}",
                m.key,
                role.unwrap_or("plain")
            ));
            rec.kind = crate::debug::animation::TraceKind::Node;
            rec.scope = Some(m.scope_id);
            rec.key = Some(m.key.clone());
            rec.role = role;
            rec.phase = Some(match n.paint_disposition() {
                PaintDisposition::InLayer => "layer",
                PaintDisposition::Placeholder => "placeholder",
                PaintDisposition::InTree => "tree",
            });
            rec.scene = scene_of_node_with(&self.arena.nodes, &id_to_idx, idx);
            // Absolute (canvas) coords: `node.position` is parent-relative, so reporting it raw made a
            // copy look like it sat at (0,0) and could not be compared with a flight's lerped rect
            // (measured, then fixed here).
            let (ax, ay) = self
                .arena
                .root
                .map(|r| crate::app::node_abs_position(&self.arena.nodes, r, n.id))
                .unwrap_or((n.position.x, n.position.y));
            rec.layout = Some(crate::debug::animation::TraceRect::new(
                ax,
                ay,
                n.measured_size.width,
                n.measured_size.height,
            ));
            // What this node paints by itself: the flight's lerped rect while it is an end, otherwise
            // its own content box at its own layout origin.
            let (rect, alpha) = match n.transition.as_ref() {
                Some(t) => {
                    let l = t.lerped();
                    (
                        crate::debug::animation::TraceRect::new(l.x, l.y, l.width, l.height),
                        t.alpha(),
                    )
                }
                None => {
                    let cb = n.content_box();
                    (
                        crate::debug::animation::TraceRect::new(ax, ay, cb.width, cb.height),
                        1.0,
                    )
                }
            };
            rec.painted = Some(rect);
            rec.alpha = Some(alpha);
            rec.effective_alpha = Some(alpha);
            crate::debug::animation::record(rec);
        }
    }

    pub(crate) fn poll_one_flight(&mut self, id: FlightId) {
        // Tier1 flights live in the main map but span composers — driven by
        // cross-poll, never here.
        let mine = match self.shared_flights.get(&id) {
            Some(a) => a.source_cid == self.composer_id && a.target_cid == self.composer_id,
            None => return,
        };
        if !mine {
            return;
        }
        let phase = match self.shared_flights.get(&id) {
            Some(a) => a.flight.phase,
            None => return,
        };
        match phase {
            FlightPhase::AwaitingBounds => {
                // Resolve target in the CURRENT tree (layout rebuilt the map).
                let target_slot = self.shared_flights.get(&id).and_then(|a| a.flight.target_slot);
                let root = match self.arena.root {
                    Some(r) => r,
                    None => {
                        self.cancel_flight(id, "awaiting_no_root");
                        return;
                    }
                };
                let tidx = match target_slot.and_then(|s| find_idx_by_slot(&self.arena.nodes, root, s)) {
                    Some(i) => i,
                    None => {
                        self.cancel_flight(id, "awaiting_target_not_found");
                        return;
                    }
                };
                let tid = self.arena.nodes[tidx].id;
                let (tx, ty) = crate::app::node_abs_position(&self.arena.nodes, root, tid);
                let (tw, th) = {
                    let n = &self.arena.nodes[tidx];
                    // The target's own content box, NOT `measured_size`: while a flight reports a
                    // placeholder size the parent sees the animated size, so reading `measured_size`
                    // here can capture the flight's own current rect as the destination. Measured with
                    // anim-trace on the nav demo: with `measured_size` both ends reported a painted
                    // width of 96 for the whole flight, i.e. the crossfade faded without ever growing,
                    // and a separate same-screen morph (opaque by design) supplied the visible growth.
                    let cb = n.content_box();
                    (cb.width, cb.height)
                };
                // Canonicalize to window coords (overlay-local + screen origin).
                let (ox, oy) = self.screen_origin;
                // Gated: with the feature off `record` is a no-op but its arguments (including a
                // `scene_of_node` ancestry walk) would still be evaluated.
                if crate::debug::animation::enabled() {
                    crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
                        format!(
                            "flight:{}",
                            find_shared_marker(&self.arena.nodes[tidx].modifier)
                                .map(|m| m.key)
                                .unwrap_or_default()
                        ),
                        "resolve",
                        format!(
                            // `id=` lets a report pair this announcement with the resolved end's records
                            // (the event's subject carries only the key, not the role or the flight).
                            "id={id} tidx={tidx} own_size={:?} measured={:?} content_box={:?} scene={:?} end=({tw:.0},{th:.0})",
                            self.arena.nodes[tidx].modifier.elements().iter().find_map(|el| match el {
                                ModifierElement::Size { .. } => Some(format!("{el:?}")),
                                _ => None,
                            }),
                            self.arena.nodes[tidx].measured_size,
                            self.arena.nodes[tidx].content_box(),
                            scene_of_node(&self.arena.nodes, tidx),
                        ),
                    ));
                }
                let end = SharedBounds::new(tx + ox, ty + oy, tw, th);
                let marker = find_shared_marker(&self.arena.nodes[tidx].modifier);
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
                // Enter from the target marker, exit from the retained source
                // marker — each side declares its own. Only Bounds flights carry
                // the pair (an Element flight crossfades both ends in winia,
                // which is a recorded deviation from Compose's sharedElement);
                // a missing source exit falls back to fade-out (today's look).
                let source_marker = self
                    .shared_flights
                    .get(&id)
                    .and_then(|a| a.source_idx)
                    .and_then(|sidx| self.arena.nodes.get(sidx))
                    .and_then(|n| find_shared_marker(&n.modifier));
                // Mixed-kind pairing (Element one side, Bounds the other)
                // resolves silently toward the target side — log it.
                if let Some(ref sm) = source_marker {
                    let source_is_bounds = matches!(sm.kind, SharedKind::Bounds { .. });
                    if source_is_bounds != target_is_bounds {
                        // The pair exists for the log below, which folds away without the feature.
                        let (_scope, _key) = self
                            .shared_flights
                            .get(&id)
                            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
                            .unwrap_or((0, String::new()));
                        crate::debug_log!(
                            "[shared] mixed-kind pair scope={_scope} key={_key} — flight follows the target side"
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
                let radius_to = {
                    let n = &self.arena.nodes[tidx];
                    shared_shape_radii(&n.modifier, tw, th)
                };
                // Freeze the target's ancestor scroll sum (canvas frame) so
                // the flight transform can add it back at render (BLOCKER #1).
                let end_scroll = {
                    let id_to_idx: HashMap<u64, usize> = self
                        .arena
                        .nodes
                        .iter()
                        .enumerate()
                        .map(|(i, n)| (n.id, i))
                        .collect();
                    ancestor_scroll_sum(&self.arena.nodes, &id_to_idx, tidx)
                };
                let (start, acts) = {
                    let a = self.shared_flights.get_mut(&id).expect("polled flight exists");
                    a.spec = spec;
                    a.radius_to = radius_to;
                    a.end_scroll = end_scroll;
                    a.clip = clip;
                    a.path = path;
                    a.bounds_fx = bounds_fx;
                    a.target_in_overlay = target_in_overlay;
                    a.resize = resize.clone();
                    a.placeholder = placeholder;
                    // The target was measured naturally in this frame's layout
                    // (the flight had no bounds yet), so this IS its resting
                    // size — the baseline the placeholder policies compare to.
                    a.target_size = Some(self.arena.nodes[tidx].measured_size);
                    // Corner KIND of the target end (percent vs fixed): the two
                    // ends are resolved separately and then mixed by progress.
                    a.radius_to_auto =
                        shared_shape_radius_is_auto(&self.arena.nodes[tidx].modifier);
                    let start = a.start;
                    let acts = a.flight.on_event(FlightEvent::BoundsReady { start, end });
                    (start, acts)
                };
                debug_assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]));
                let _ = start;
                // Drive the flight: the four EDGES for the geometry (Compose's `Rect.VectorConverter`
                // shape — one spring per edge, one pixel of threshold each) and a scalar 0→1 clock for
                // the content crossfade, which must run even when the two rects are the same.
                {
                    let (spec, target) = self
                        .shared_flights
                        .get(&id)
                        .map(|a| (a.spec.clone(), a.flight.end.unwrap_or(a.start)))
                        .expect("the flight being started exists");
                    let a = self.shared_flights.get_mut(&id).expect("polled flight exists");
                    let edges = State::new(crate::transition::FlightEdges::of(&start));
                    a.edges = edges.clone();
                    push_animatable(
                        edges,
                        crate::transition::FlightEdges::of(&target),
                        spec.clone(),
                    );
                    let clock = State::new(0.0f32);
                    a.progress = clock.clone();
                    push_animatable(clock, 1.0, crate::transition::BoundsTransform::clock_spec(&spec));
                }
                self.write_flight_visuals(id);
            }
            FlightPhase::Flying => {
                if let Some(a) = self.shared_flights.get_mut(&id) {
                    // The mirror the pure state machine tracks, and what the trace reports.
                    a.flight.progress = a.progress_value();
                }
                self.write_flight_visuals(id);
                let done = self.shared_flights.get(&id).is_some_and(flight_progress_done);
                if done {
                    let acts = self
                        .shared_flights
                        .get_mut(&id)
                        .map(|a| a.flight.on_event(FlightEvent::ProgressDone))
                        .unwrap_or_default();
                    debug_assert_eq!(acts.len(), 2);
                    // Interpret [ReleaseRetained, FinishFlight].
                    let (src_slot, src_idx) = self
                        .shared_flights
                        .get(&id)
                        .map(|a| (a.flight.source_slot, a.source_idx))
                        .unwrap_or((None, None));
                    if let (Some(slot), Some(idx)) = (src_slot, src_idx) {
                        self.free_retained_source(idx, slot);
                    }
                    if let Some(slot) = self.shared_flights.get(&id).and_then(|a| a.flight.target_slot) {
                        self.clear_transition_for_slot(slot, FlightKey { cid: self.composer_id, id });
                    }
                    self.shared_flights.remove(&id);
                }
            }
            // Terminal-transient: the coordinator drops on dispatch; belt-and-braces.
            FlightPhase::Finishing | FlightPhase::Cancelled | FlightPhase::Idle => {
                self.shared_flights.remove(&id);
            }
        }
    }

    /// Rewrite both ends' render snapshots from current scalar progress.
    pub(crate) fn write_flight_visuals(&mut self, id: FlightId) {
        let snapshot = self.shared_flights.get(&id).map(|a| {
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
            mut start,
            mut end,
            p,
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
            target_in_overlay,
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
        // Identity of the endpoint this flight is chasing, for the target-slot
        // re-check below (slots are positional; a shift can hand the key away).
        let flight_key = self
            .shared_flights
            .get(&id)
            .map(|a| (a.flight.scope_id, a.flight.key.clone()));
        let Some(flight_key) = flight_key else { return };
        // Flight bounds are canonical window coords; visuals render in this
        // composer's canvas frame (main renders untranslated; overlays render
        // translated by screen_pos).
        let (ox, oy) = self.screen_origin;
        start.x -= ox;
        start.y -= oy;
        end.x -= ox;
        end.y -= oy;
        // The rect the flight's four springs produced, in this composer's canvas frame. A straight
        // path paints it as it is; an arc flight leaves it `None` and bends the parameterised rect
        // below (the arc is a winia addition Compose has no counterpart for).
        let flown = if matches!(path, PathMotion::Linear) {
            self.shared_flights.get(&id).map(|a| a.edges.peek().bounds()).map(|mut r| {
                r.x -= ox;
                r.y -= oy;
                r
            })
        } else {
            None
        };
        if let (Some(slot), Some(idx)) = (sslot, sidx) {
            let matched = self.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot);
            if matched {
                self.arena.nodes[idx].transition = Some(TransitionVisual {
                    start,
                    end,
                    progress: p,
                    flown,
                    scene_alpha: scene_alpha_for_end(&self.arena.nodes, idx, true),
                    role: TransitionRole::Source,
                    radius_from: rf,
                    radius_to: rt,
                    clip,
                    // Clicking the ghost routes into the live target (Phase 3).
                    link_slot: tslot,
                    scroll: sscroll,
                    flight: id,
                    path,
                    bounds_fx: fx.clone(),
                    // The leaving end is detached (no longer in any tree), so
                    // the layer is its only home — always elevated.
                    elevated: true,
                    // Frozen ghost: no live layout to re-measure.
                    remeasure: false,
                    scale_parts: None,
                    radius_from_auto,
                    radius_to_auto,
                });
            }
        }
        if let Some(slot) = tslot {
            if let Some(root) = self.arena.root {
                if let Some(tidx) = find_idx_by_slot(&self.arena.nodes, root, slot) {
                    // IDENTITY RE-CHECK (review 2, R2-F5): slots are positional, so a
                    // recomposition that shifts the target's position can hand this
                    // frozen key to an unrelated element — which would then take the
                    // flight's visual AND its layout override (measured: an unrelated
                    // 20x20 leaf laid out at the flight's reported size for one pass).
                    // Tier 1 has the equivalent guard (its staleness check); Tier 0
                    // re-checks the marker instead.
                    let still_ours = find_shared_marker(&self.arena.nodes[tidx].modifier)
                        .is_some_and(|m| {
                            m.scope_id == flight_key.0 && m.key == flight_key.1
                        });
                    if !still_ours {
                        return;
                    }
                    let role = if sslot == tslot {
                        // Same-screen size morph: opacity untouched.
                        TransitionRole::Morph
                    } else {
                        TransitionRole::Target
                    };
                    // Same-screen morphs are layout-driven resizes, not
                    // cross-composable flights — they stay in-tree like
                    // Compose's animateBounds (never escape their container).
                    let elevated = target_in_overlay && role == TransitionRole::Target;
                    // Layout contract (Compose `ResizeMode` / `PlaceHolderSize`).
                    // Only written when it overrides something, so the default
                    // path (ScaleToBounds + JumpCut/ContentSize) never touches
                    // the layout.
                    let remeasure = role == TransitionRole::Target
                        && matches!(resize, ResizeMode::RemeasureToBounds);
                    let frame = placeholder_frame(
                        remeasure,
                        animated_size(&start, &end, p, path),
                        placeholder,
                        target_size,
                    );
                    measure_state.set(frame);
                    let key = self.arena.nodes[tidx].slot_key;
                    let mine = FlightKey { cid: self.composer_id, id };
                    // Never clobber (or clear) a DIFFERENT flight's override:
                    // ids are per-composer, so another flight can legitimately
                    // own this node's override while this one is being torn down.
                    let foreign = self.arena.nodes[tidx]
                        .flight_measure
                        .as_ref()
                        .is_some_and(|f| f.owner != mine);
                    let had_override =
                        !foreign && self.arena.nodes[tidx].flight_measure.is_some();
                    if !foreign {
                        self.arena.nodes[tidx].flight_measure = if frame.is_idle() {
                            None
                        } else {
                            Some(FlightMeasure { frame: measure_state, owner: mine })
                        };
                    }
                    // `layout()` resets `layout_dirty` on the whole tree at the
                    // start of every pass, so an override has to be re-seeded
                    // EVERY frame — otherwise the folded parent never descends
                    // into it. The seed is skipped when nothing changed: the
                    // default contract (no override attached, none to drop)
                    // must not force a re-measure of the ancestor chain.
                    if !frame.is_idle() || had_override {
                        self.layout_dirty_keys.insert(key);
                    }
                    // FIRST attach (nothing was there before): the layout that just ran
                    // reported the target's NATURAL size to its parent, because the override
                    // can only be attached here — after layout — since resolving the flight
                    // needs the target's measured rect. The parent is therefore stale WITHIN
                    // this frame, which is what made the rows below a starting flight dip
                    // and snap back. The app loop consumes this flag and re-lays out once, so
                    // the frame renders coherent (see `take_layout_override_fresh`).
                    if !frame.is_idle() && !had_override {
                        self.layout_override_fresh = true;
                    }
                    self.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start,
                        end,
                        progress: p,
                        flown,
                        scene_alpha: scene_alpha_for_end(&self.arena.nodes, tidx, false),
                        role,
                        radius_from: rf,
                        radius_to: rt,
                        clip,
                        link_slot: None,
                        // Elevated targets render rootless in the layer, whose
                        // canvas carries no ancestor translate — the frozen
                        // ancestor sum must not be added back there.
                        scroll: if elevated { (0.0, 0.0) } else { escroll },
                        flight: id,
                        path,
                        bounds_fx: fx,
                        elevated,
                        remeasure,
                        scale_parts: resize.scale_to_bounds_parts(),
                        radius_from_auto,
                        radius_to_auto,
                    });
                    if elevated {
                        self.elevated_roots.push(tidx);
                    }
                }
            }
        }
        // anim-trace: one record per end per frame — `layout` is the node's measured rect, `painted`
        // is the flight's lerped rect (what the flight draws), and opacity is recorded in three parts
        // (`alpha` = the end's own, `effective_alpha` = what the draw composes, `scene_visibility` =
        // the scene host's value) so "who faded it" is answerable without guessing.
        if crate::debug::animation::enabled() {
            let (scope_id, key) = self
                .shared_flights
                .get(&id)
                .map(|a| (a.flight.scope_id, a.flight.key.clone()))
                .unwrap_or((0, String::new()));
            let svis = self
                .shared_flights
                .get(&id)
                .and_then(|a| a.source_idx)
                .and_then(|i| self.arena.nodes.get(i))
                .and_then(|n| find_shared_marker(&n.modifier))
                .and_then(|m| m.scene)
                .and_then(nav_scene_visibility);
            for (idx, role) in self
                .arena
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| {
                    let t = n.transition.as_ref()?;
                    if t.flight != id {
                        return None;
                    }
                    Some((i, t.role.clone()))
                })
                .collect::<Vec<_>>()
            {
                let n = &self.arena.nodes[idx];
                let t = match n.transition.as_ref() {
                    Some(t) => t,
                    None => continue,
                };
                let lerped = t.lerped();
                let mut rec = crate::debug::animation::TraceRecord::flight(match role {
                    TransitionRole::Source => format!("flight:{key}#Source"),
                    TransitionRole::Target => format!("flight:{key}#Target"),
                    TransitionRole::Morph => format!("flight:{key}#Morph"),
                });
                rec.scope = Some(scope_id);
                rec.key = Some(key.clone());
                rec.role = Some(match role {
                    TransitionRole::Source => "Source",
                    TransitionRole::Target => "Target",
                    TransitionRole::Morph => "Morph",
                });
                rec.flight = Some(id);
                rec.phase = Some(if t.progress >= 1.0 { "settled" } else { "flying" });
                rec.progress = Some(t.progress);
                rec.layout = Some(crate::debug::animation::TraceRect::new(
                    n.position.x,
                    n.position.y,
                    n.measured_size.width,
                    n.measured_size.height,
                ));
                rec.painted = Some(crate::debug::animation::TraceRect::new(
                    lerped.x, lerped.y, lerped.width, lerped.height,
                ));
                rec.alpha = Some(t.alpha());
                // A detached/elevated end is drawn from the transition layer, where the scene's own
                // layer fade does not apply; an in-tree end of a scene is multiplied by it.
                let scene_vis = if t.elevated { None } else { svis };
                rec.scene_visibility = scene_vis;
                rec.effective_alpha = Some(t.alpha() * scene_vis.unwrap_or(1.0));
                rec.radii = Some(t.radii());
                rec.clip = Some(t.clip);
                rec.composer = Some(self.composer_id);
                crate::debug::animation::record(rec);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 4: cross-composer Tier1 (main tree ↔ overlays)
// ═══════════════════════════════════════════════════════════

/// Window-canonical bounds into a composer canvas frame.
pub(crate) fn off_origin(b: SharedBounds, o: (f32, f32)) -> SharedBounds {
    SharedBounds::new(b.x - o.0, b.y - o.1, b.width, b.height)
}

