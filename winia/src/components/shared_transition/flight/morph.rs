use super::*;

impl Composer {
    /// Active non-terminal flight for (scope, key), if any.
    pub(crate) fn flight_for_key(&self, scope_id: u64, key: &str) -> Option<FlightId> {
        self.shared_flights
            .iter()
            .find(|(_, a)| {
                !is_terminal(a.flight.phase) && a.flight.scope_id == scope_id && a.flight.key == key
            })
            .map(|(id, _)| *id)
    }

    /// Open a same-screen morph flight (no detach — the node stays live, so
    /// `source_idx` stays `None` and completion only clears visuals). The pure
    /// machine runs CounterpartAppeared + BoundsReady back-to-back with both
    /// slots identical; render picks [`TransitionRole::Morph`] from that.
    /// `radius_from_override`: seamless reopen continuity (else resolved from
    /// the modifier at the start size).
    pub(crate) fn begin_morph(
        &mut self,
        scope_id: u64,
        key: String,
        slot: u64,
        start: SharedBounds,
        end: SharedBounds,
        radius_from_override: Option<[f32; 4]>,
    ) {
        let root = match self.arena.root {
            Some(r) => r,
            None => return,
        };
        let tidx = match find_idx_by_slot(&self.arena.nodes, root, slot) {
            Some(i) => i,
            None => return,
        };
        let marker = find_shared_marker(&self.arena.nodes[tidx].modifier);
        let (spec, clip, path, bounds_fx) = match marker {
            Some(m) => {
                // Same node both ends — render ignores channels for Morph
                // role anyway; stored for uniformity.
                let fx = m
                    .enter
                    .clone()
                    .map(|enter| (enter, m.exit.clone().unwrap_or_default()));
                (m.transform.spec.clone(), shared_clip_for_kind(&m.kind), m.path, fx)
            }
            None => (BoundsTransform::default().spec, false, PathMotion::Linear, None),
        };
        let (tw, th, modifier) = {
            let n = &self.arena.nodes[tidx];
            (n.measured_size.width, n.measured_size.height, n.modifier.clone())
        };
        let radius_to = shared_shape_radii(&modifier, tw, th);
        let radius_from =
            radius_from_override.unwrap_or_else(|| shared_shape_radii(&modifier, start.width, start.height));
        // Same node, same frame for both ends — one frozen scroll sum
        // (BLOCKER #1); morphs stay rigid under mid-flight scroll.
        let scroll = {
            let id_to_idx: HashMap<u64, usize> = self
                .arena
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id, i))
                .collect();
            ancestor_scroll_sum(&self.arena.nodes, &id_to_idx, tidx)
        };
        // The morph's edges, driven by its spec (Compose's `Rect.VectorConverter` shape), plus the
        // scalar clock its content fade rides.
        let edges = State::new(crate::transition::FlightEdges::of(&start));
        push_animatable(
            edges.clone(),
            crate::transition::FlightEdges::of(&end),
            spec.clone(),
        );
        let progress = State::new(0.0f32);
        push_animatable(
            progress.clone(),
            1.0,
            crate::transition::BoundsTransform::clock_spec(&spec),
        );
        let id = self.next_flight_id;
        self.next_flight_id += 1;
        let mut flight = Flight::new(id, scope_id, key);
        let acts = flight.on_event(FlightEvent::CounterpartAppeared { source_slot: slot, target_slot: slot });
        debug_assert_eq!(
            acts,
            vec![FlightAction::CollectBounds { source_slot: slot, target_slot: slot }]
        );
        let acts = flight.on_event(FlightEvent::BoundsReady { start, end });
        debug_assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]));
        self.shared_flights.insert(
            id,
            ActiveFlight {
                flight,
                progress,
                edges,
                spec,
                source_idx: None,
                start,
                start_scroll: scroll,
                end_scroll: scroll,
                radius_from,
                radius_to,
                // A morph has ONE node, so both ends share its corner kind.
                radius_from_auto: shared_shape_radius_is_auto(&modifier),
                radius_to_auto: shared_shape_radius_is_auto(&modifier),
                clip,
                path,
                bounds_fx,
                target_in_overlay: true,
                resize: ResizeMode::scale_to_bounds(),
                placeholder: PlaceHolderSize::JumpCut,
                target_size: None,
                measure: State::new(FlightMeasureFrame::IDLE),
                source_cid: self.composer_id,
                target_cid: self.composer_id,
            },
        );
        self.write_flight_visuals(id);
    }

    /// Same-screen morph detection (post-layout, every frame): live marked
    /// slots whose SIZE changed beyond epsilon open (or seamlessly reopen) a
    /// morph flight. Baselines refresh every frame, so settled frames stay
    /// quiet; render-phase morphs never feed back into layout (no loops).
    pub(crate) fn poll_layout_morphs(&mut self, live: &HashMap<(u64, String), u64>) {
        let Some(root) = self.arena.root else {
            self.shared_last_bounds.clear();
            return;
        };
        enum Pending {
            Fresh { scope_id: u64, key: String, slot: u64, start: SharedBounds, end: SharedBounds },
            Reopen { id: FlightId, start: SharedBounds, radii: [f32; 4], end: SharedBounds },
        }
        let mut pending: Vec<Pending> = Vec::new();
        for (k, slot) in live {
            let Some(idx) = find_idx_by_slot(&self.arena.nodes, root, *slot) else {
                continue;
            };
            let nid = self.arena.nodes[idx].id;
            let (ax, ay) = crate::app::node_abs_position(&self.arena.nodes, root, nid);
            let (w, h) = {
                let n = &self.arena.nodes[idx];
                (n.measured_size.width, n.measured_size.height)
            };
            // Canonicalize to window coords (overlay-local + screen origin).
            let (ox, oy) = self.screen_origin;
            let cur = SharedBounds::new(ax + ox, ay + oy, w, h);
            // Baselines key on endpoint identity (scope, key), not the slot: a
            // conditional key-swap reusing one call-site slot must not inherit the
            // previous key's rect as its morph start.
            let bkey = (k.0, k.1.clone());
            // A flight OWNS this node's reported size right now (Compose
            // `PlaceHolderSize`), so a size delta here is the FLIGHT, not a layout
            // morph. Opening one would be wrong twice over: a Tier-1 flight lives in
            // the MAIN composer's map, so this composer's `flight_for_key` cannot see
            // it and would open a phantom T0 morph EVERY frame — and that morph's
            // idle frame (morphs never attach an override) deletes the flight's
            // override, which the cross-poll then rewrites, i.e. per-frame
            // drop/rewrite churn plus a phantom flight.
            //
            // The BASELINE must still track the override-driven size, though: the
            // baseline is the pre-flight rect, so freezing it here made the flight's
            // own layout change look like a fresh morph the moment the override was
            // dropped — the landed hero then replayed a whole animation. So this
            // skips the morph DECISION only; the insert below still runs.
            let overridden = self.arena.nodes[idx].flight_measure.is_some();
            if !overridden {
                if let Some(prev) = self.shared_last_bounds.get(&bkey) {
                    if size_delta(prev, &cur) > MORPH_EPS {
                    match self.flight_for_key(k.0, &k.1) {
                        Some(fid) => {
                            // Active flight: only morphs reopen (switch flights
                            // own their ends — documented v1).
                            let morph = self.shared_flights.get(&fid).is_some_and(|a| {
                                a.source_idx.is_none() && a.flight.source_slot == a.flight.target_slot
                            });
                            if morph {
                                let a = &self.shared_flights[&fid];
                                // Unclamped + path-aware like the render path:
                                // reopen continuity includes spring overshoot
                                // and any arc bend.
                                let p = a.progress_value();
                                let end_prev = a.flight.end.unwrap_or(a.start);
                                let s = lerp_flight_rect(&a.start, &end_prev, p, a.path);
                                let (rf, rt) = (a.radius_from, a.radius_to);
                                pending.push(Pending::Reopen {
                                    id: fid,
                                    start: s,
                                    radii: [0, 1, 2, 3].map(|i| rf[i] + (rt[i] - rf[i]) * p),
                                    end: cur,
                                });
                            }
                        }
                        None => {
                            // Compose ties a shared element's morph to a TRANSITION: the element
                            // animates when the transition animating it changes its bounds, not merely
                            // because a layout pass changed them. Without this gate a window resize
                            // opened a morph on every marked node whose rect changed (measured:
                            // `morph_open flight:entry:… fresh 420x524 -> 430x604` right after a
                            // resize), i.e. dragging the window animated the UI.
                            //
                            // The gate skips only the DECISION — the baseline insert below still runs, as
                            // the older `overridden` guard does. `continue`-ing here left the baseline
                            // stale, so an idle resize was replayed as a morph on the first transitioning
                            // frame instead: the same artefact, one navigation later.
                            if self.scope_is_transitioning(k.0) {
                                pending.push(Pending::Fresh {
                                    scope_id: k.0,
                                    key: k.1.clone(),
                                    slot: *slot,
                                    start: *prev,
                                    end: cur,
                                });
                            }
                        }
                    }
                }
                }
            }
            self.shared_last_bounds.insert(bkey, cur);
        }
        // Drop baselines for vanished endpoints (bounded memory).
        let live_keys: HashSet<(u64, String)> = live.keys().cloned().collect();
        self.shared_last_bounds.retain(|k, _| live_keys.contains(k));
        for p in pending {
            match p {
                Pending::Fresh { scope_id, key, slot, start, end } => {
                    // anim-trace: morph opens are otherwise invisible in a trace (no `start` event, only
                    // per-frame records), which made "what did the resize animate?" hard to answer.
                    crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
                        format!("flight:{key}"),
                        "morph_open",
                        format!(
                            "fresh {:.0}x{:.0} -> {:.0}x{:.0} at ({:.0},{:.0})",
                            start.width, start.height, end.width, end.height, end.x, end.y
                        ),
                    ));
                    self.begin_morph(scope_id, key, slot, start, end, None);
                }
                Pending::Reopen { id, start, radii, end } => {
                    let meta = self
                        .shared_flights
                        .get(&id)
                        .map(|a| (a.flight.scope_id, a.flight.key.clone(), a.flight.target_slot));
                    self.cancel_flight(id, "morph_reopen");
                    if let Some((scope_id, key, Some(slot))) = meta.map(|(s, k, t)| (s, k, t)) {
                        crate::debug::animation::record(crate::debug::animation::TraceRecord::event(
                            format!("flight:{key}"),
                            "morph_open",
                            format!(
                                "reopen {:.0}x{:.0} -> {:.0}x{:.0}",
                                start.width, start.height, end.width, end.height
                            ),
                        ));
                        self.begin_morph(scope_id, key, slot, start, end, Some(radii));
                    }
                }
            }
        }
    }
}

