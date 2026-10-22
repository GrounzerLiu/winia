use super::*;


// ═══════════════════════════════════════════════════════════
// Flight engine: pure state machine
// ═══════════════════════════════════════════════════════════


// ═══════════════════════════════════════════════════════════
// Modifier builders (frozen API)
// ═══════════════════════════════════════════════════════════

impl Modifier {
    /// Mark the shared element (same content on both ends — flies + crossfades).
    /// Compose's `sharedElement` has no resize parameter and always re-measures,
    /// so this marker resolves to `RemeasureToBounds`; `placeholder` selects
    /// what the parent observes while the flight runs (`PlaceHolderSize`).
    /// `path` selects the motion path (Compose `ArcMode` equivalent —
    /// Winia applies a flight-level quarter-ellipse port instead of
    /// Compose's per-keyframe `using ArcMode`).
    /// `z_index` orders flying pairs back-to-front in the transition layer
    /// (Compose `zIndexInOverlay`, default 0).
    /// `render_in_overlay` (Compose `renderInOverlayDuringTransition`,
    /// default `true`) paints this endpoint in the transition layer while it
    /// flies: it escapes ancestor clips and ancestor layer transforms (alpha,
    /// scale) and renders above non-shared content. `false` keeps in-tree
    /// painting, where ancestors still clip it. The leaving end is always
    /// detached, so it always renders in the layer (documented no-op there).
    pub fn shared_element(
        self,
        state: SharedContentState,
        transform: BoundsTransform,
        placeholder: PlaceHolderSize,
        path: PathMotion,
        z_index: f32,
        render_in_overlay: bool,
    ) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Element { placeholder },
            transform,
            path,
            z_index,
            enter: None,
            exit: None,
            render_in_overlay,
            scene: current_nav_scene().map(|s| s.id),
        })
    }

    /// Compose `Modifier.renderInSharedTransitionScopeOverlay(zIndexInOverlay)`:
    /// while this scope has a flight, render this **non-shared** subtree in the
    /// transition layer instead of the tree walk, so it keeps its spatial
    /// relationship with the flying shared elements (pinned app bars, FABs).
    /// Outside a flight the subtree is just ordinary tree content again — the
    /// elevation only lasts as long as the scope is transitioning.
    ///
    /// `z_index` is Compose's `zIndexInOverlay`: shared endpoints default to
    /// `0.0`, so a bar that must stay on top passes something larger. At equal
    /// z the chrome renders after the flights (i.e. above them).
    pub fn render_in_shared_transition_scope_overlay(
        self,
        scope: &SharedTransitionScope,
        z_index: f32,
    ) -> Self {
        self.push(ModifierElement::SharedScopeOverlay { scope_id: scope.scope_id, z_index })
    }

    /// Tag this subtree as the content composed for scene `id` (scene hosts call it on the wrapper
    /// they build per scene, e.g. one nav transition layer). Shared-element pairing reads it from the
    /// ANCESTOR chain at query time: which of two live ends of one key is leaving and which is
    /// entering is decided by their scenes' visibilities, and a marker cannot carry that itself (its
    /// modifier element is built once and reused, so a captured id freezes).
    pub fn scene_tag(self, id: u64) -> Self {
        self.push(ModifierElement::SceneTag { id })
    }

    /// Mark shared bounds (different content — container morphs + crossfades).
    /// `path` selects the motion path, same as in [`shared_element`](Self::shared_element).
    /// `z_index` orders flying pairs back-to-front, same as above.
    /// `render_in_overlay` is Compose `renderInOverlayDuringTransition`,
    /// same as above (default `true`).
    /// `enter` plays on the appearing end, `exit` on the disappearing end
    /// (Compose `enter`/`exit` — fade channels claimed by these replace the
    /// flight crossfade per-end; slide/scale/expand compose on top at flight
    /// progress; Morph role skips both).
    pub fn shared_bounds(
        self,
        state: SharedContentState,
        enter: VisibilityTransition,
        exit: VisibilityTransition,
        transform: BoundsTransform,
        resize: ResizeMode,
        placeholder: PlaceHolderSize,
        path: PathMotion,
        z_index: f32,
        render_in_overlay: bool,
    ) -> Self {
        // Compose's `overlayClip` defaults to the bounds' own resolved clip, which is this
        // crate's long-standing behaviour — so the 9-argument form keeps its signature and
        // the clip is opt-in through the sibling below.
        self.shared_bounds_with_overlay_clip(
            state,
            enter,
            exit,
            transform,
            resize,
            placeholder,
            path,
            z_index,
            render_in_overlay,
            OverlayClip::Bounds,
        )
    }

    /// [`shared_bounds`](Self::shared_bounds) with Compose's `overlayClip` slot.
    #[allow(clippy::too_many_arguments)]
    pub fn shared_bounds_with_overlay_clip(
        self,
        state: SharedContentState,
        enter: VisibilityTransition,
        exit: VisibilityTransition,
        transform: BoundsTransform,
        resize: ResizeMode,
        placeholder: PlaceHolderSize,
        path: PathMotion,
        z_index: f32,
        render_in_overlay: bool,
        overlay_clip: OverlayClip,
    ) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Bounds { resize, placeholder, overlay_clip },
            transform,
            path,
            z_index,
            enter: Some(enter),
            exit: Some(exit),
            render_in_overlay,
            scene: current_nav_scene().map(|s| s.id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::SpringSpec;

    #[test]
    pub(crate) fn bounds_lerp_endpoints_and_midpoint() {
        let a = SharedBounds::new(0.0, 10.0, 100.0, 50.0);
        let b = SharedBounds::new(200.0, 110.0, 300.0, 150.0);
        assert_eq!(a.lerp(&b, 0.0), a, "t=0 is the start rect");
        assert_eq!(a.lerp(&b, 1.0), b, "t=1 is the end rect");
        assert_eq!(
            a.lerp(&b, 0.5),
            SharedBounds::new(100.0, 60.0, 200.0, 100.0),
            "t=0.5 interpolates origin and size independently"
        );
    }

    #[test]
    pub(crate) fn bounds_lerp_overshoot_is_unclamped() {
        // Spring progress overshoots past 1.0 — the rect must follow (the
        // Compose spring look), not clamp.
        let a = SharedBounds::new(0.0, 0.0, 100.0, 100.0);
        let b = SharedBounds::new(100.0, 0.0, 100.0, 100.0);
        let past = a.lerp(&b, 1.25);
        assert_eq!(past.x, 125.0, "overshoot flies past the target");
    }

    #[test]
    pub(crate) fn bounds_animatable_trait_is_tween_exact() {
        let a = SharedBounds::new(0.0, 0.0, 10.0, 20.0);
        let b = SharedBounds::new(30.0, 40.0, 50.0, 60.0);
        // Trait entry clamps (engine contract); inherent `lerp` does not.
        assert_eq!(AnimatableValue::lerp(&a, &b, 0.5), SharedBounds::new(15.0, 20.0, 30.0, 40.0));
        assert!(!<SharedBounds as AnimatableValue>::supports_spring(), "the rect type is not the animated value — flights animate FlightEdges");
        assert!(<SharedBounds as AnimatableValue>::same_target(&a, &a));
    }

    #[test]
    pub(crate) fn transform_constructors_preserve_spec() {
        let t = BoundsTransform::spring(SpringSpec::default());
        assert!(matches!(t.spec, AnimationSpec::Spring(_)));
        let t = BoundsTransform::tween(TweenSpec::default());
        assert!(matches!(t.spec, AnimationSpec::Tween(_)));
        // The DEFAULT is Compose's spring, not a tween — see `BoundsTransform::default`.
        assert!(matches!(BoundsTransform::default().spec, AnimationSpec::Spring(_)));
    }

    /// The `ContentScale`/`ImageAlignment` math, pinned against Compose's documented
    /// semantics: `scaleToBounds()` defaults to `FillWidth` + `Center` (NOT `Image`'s
    /// `Fit`, and not `FillBounds` — which is what winia hard-coded before).
    #[test]
    pub(crate) fn content_scale_factors_match_compose() {
        let content = (150.0, 150.0);
        let bounds = (300.0, 150.0);
        assert_eq!(
            ContentScale::FillWidth.scale_factors(content, bounds),
            (2.0, 2.0),
            "FillWidth is uniform and matches the WIDTH (Compose's default)"
        );
        assert_eq!(
            ContentScale::FillHeight.scale_factors(content, bounds),
            (1.0, 1.0),
            "FillHeight is uniform and matches the HEIGHT"
        );
        assert_eq!(
            ContentScale::FillBounds.scale_factors(content, bounds),
            (2.0, 1.0),
            "FillBounds is the non-uniform stretch winia used to hard-code"
        );
        assert_eq!(ContentScale::Fit.scale_factors(content, bounds), (1.0, 1.0));
        assert_eq!(ContentScale::Crop.scale_factors(content, bounds), (2.0, 2.0));
        assert_eq!(
            ContentScale::Inside.scale_factors((500.0, 500.0), bounds),
            (0.3, 0.3),
            "Inside is Fit — both axes are limited by the tighter ratio"
        );
        assert_eq!(
            ContentScale::Inside.scale_factors((100.0, 100.0), bounds),
            (1.0, 1.0),
            "Inside never scales UP"
        );
        assert_eq!(
            ContentScale::None.scale_factors(content, bounds),
            (1.0, 1.0)
        );
        assert_eq!(
            ContentScale::FillWidth.scale_factors(content, (0.0, 150.0)),
            (1.0, 1.0),
            "degenerate bounds must not produce NaN/inf"
        );
        // A uniform FillWidth scale leaves the leftover axis to the alignment.
        let off = |a: ImageAlignment| ContentScale::align_offset(a, (300.0, 150.0), (300.0, 250.0), false);
        assert_eq!(off(ImageAlignment::Center), (0.0, 50.0));
        assert_eq!(off(ImageAlignment::BottomCenter), (0.0, 100.0));
        assert_eq!(off(ImageAlignment::TopStart), (0.0, 0.0));
    }

    #[test]
    pub(crate) fn shared_transition_defaults_match_hardcoded_behavior() {
        let t = SharedTransitionDefaults::bounds_transform();
        match &t.spec {
            AnimationSpec::Spring(spec) => {
                assert_eq!(spec.damping_ratio, SpringSpec::DAMPING_RATIO_NO_BOUNCY);
                assert_eq!(
                    spec.stiffness,
                    SpringSpec::STIFFNESS_MEDIUM,
                    "Compose's StiffnessMediumLow is 400, which winia spells STIFFNESS_MEDIUM"
                );
                assert_eq!(
                    spec.threshold,
                    crate::transition::BoundsTransform::VISIBILITY_THRESHOLD_PX,
                    "Compose's Rect.VisibilityThreshold is one PIXEL per edge, and the flight's edges \
                     are pixels — the spec runs on FlightEdges"
                );
                assert_eq!(spec.threshold, 1.0);
            }
            other => panic!(
                "the default bounds transform is Compose's spring, not a tween, got {other:?}"
            ),
        }
        assert_eq!(SharedTransitionDefaults::z_index_in_overlay(), 0.0);
        assert!(SharedTransitionDefaults::render_in_overlay());
    }

    #[test]
    pub(crate) fn shared_clip_and_layout_contract_resolution() {
        // Marker round-trip: shared_element carries the placeholder.
        let m = Modifier::new()
            .shared_element(
                SharedContentState { scope_id: 1, key: "k".to_string() },
                BoundsTransform::default(),
                PlaceHolderSize::AnimatedSize,
                PathMotion::Linear,
                0.0,
                true,
            );
        assert_eq!(
            find_shared_marker(&m).map(|x| x.kind),
            Some(SharedKind::Element { placeholder: PlaceHolderSize::AnimatedSize }),
            "element marker preserves the placeholder"
        );
        // Clip extraction: ALWAYS false now — the render clips every transitioning
        // node unconditionally, so no marker can opt in or out (the `clip` flag that
        // used to feed this measured as dead and was deleted).
        assert!(!shared_clip_for_kind(&SharedKind::Element {
            placeholder: PlaceHolderSize::JumpCut
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Element {
            placeholder: PlaceHolderSize::AnimatedSize
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Bounds {
            resize: ResizeMode::scale_to_bounds(),
            placeholder: PlaceHolderSize::JumpCut,
            overlay_clip: OverlayClip::Bounds,
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Bounds {
            resize: ResizeMode::RemeasureToBounds,
            placeholder: PlaceHolderSize::ContentSize,
            overlay_clip: OverlayClip::Bounds,
        }));
        // The marker's `overlayClip` reaches the render verbatim, and an Element end
        // (which has no such parameter) keeps the morph-shaped default.
        assert_eq!(
            shared_overlay_clip_for_kind(&SharedKind::Bounds {
                resize: ResizeMode::scale_to_bounds(),
                placeholder: PlaceHolderSize::JumpCut,
                overlay_clip: OverlayClip::None,
            }),
            OverlayClip::None
        );
        assert_eq!(
            shared_overlay_clip_for_kind(&SharedKind::Element {
                placeholder: PlaceHolderSize::JumpCut
            }),
            OverlayClip::Bounds
        );
        // Layout contract: the marker's resize/placeholder reach the flight
        // verbatim (no degradation is left in the pipeline).
        assert!(matches!(
            shared_resize_for_kind(&SharedKind::Bounds {
                resize: ResizeMode::RemeasureToBounds,
                placeholder: PlaceHolderSize::AnimatedSize,
                overlay_clip: OverlayClip::Bounds,
            }),
            ResizeMode::RemeasureToBounds
        ));
        assert_eq!(
            shared_placeholder_for_kind(&SharedKind::Bounds {
                resize: ResizeMode::RemeasureToBounds,
                placeholder: PlaceHolderSize::AnimatedSize,
                overlay_clip: OverlayClip::Bounds,
            }),
            PlaceHolderSize::AnimatedSize
        );
        // Compose's `sharedElement` always re-measures (its KDoc: "will
        // re-measure and relayout its child layout using fixed constraints
        // derived from its animated size") — Element markers must therefore
        // resolve to RemeasureToBounds, not to a scale.
        assert!(
            matches!(
                shared_resize_for_kind(&SharedKind::Element {
                    placeholder: PlaceHolderSize::JumpCut
                }),
                ResizeMode::RemeasureToBounds
            ),
            "sharedElement re-measures, like Compose"
        );
    }

    #[test]
    pub(crate) fn arc_lerped_endpoints_sides_and_degenerates() {
        // 2D travel: start (0,0,100,100) → end (200,100,100,100),
        // centers (50,50) → (250,150).
        let mk = |path, progress| TransitionVisual {
            start: SharedBounds::new(0.0, 0.0, 100.0, 100.0),
            end: SharedBounds::new(200.0, 100.0, 100.0, 100.0),
            progress,
            // Built by hand: this test exercises the arc form, which is what a `flown` rect replaces
            // when a flight runs on its four springs.
            flown: None,
            role: TransitionRole::Target,
            scene_alpha: None,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path,
            bounds_fx: None,
            elevated: false,
            remeasure: false,
            scale_parts: None,
            radius_from_auto: false,
            radius_to_auto: false,
        };
        // Linear midpoint is exactly the component-wise lerp.
        assert_eq!(
            mk(PathMotion::Linear, 0.5).lerped(),
            SharedBounds::new(100.0, 50.0, 100.0, 100.0)
        );
        // Arc midpoints bend off the straight center (150,100) to opposite
        // sides (quarter ellipse, arc-length parameterized — assert side +
        // magnitude, not LUT-exact digits).
        let dev_of = |path| {
            let l = mk(path, 0.5).lerped();
            (l.x + 50.0 - 150.0, l.y + 50.0 - 100.0)
        };
        let (bx, by) = dev_of(PathMotion::ArcBelow);
        let bmag = (bx * bx + by * by).sqrt();
        assert!(bmag > 20.0, "below-arc bends substantially, got ({bx}, {by})");
        let (ax, ay) = dev_of(PathMotion::ArcAbove);
        let amag = (ax * ax + ay * ay).sqrt();
        assert!(amag > 20.0, "above-arc bends substantially, got ({ax}, {ay})");
        assert!(
            bx * ax + by * ay < 0.0,
            "arcs bulge to opposite sides: below ({bx}, {by}) vs above ({ax}, {ay})"
        );
        // Arc endpoints are exact up to f32 trig at the table ends
        // (cos(π/2) ≈ -4e-8 — AOSP shares this; its tests use tolerance too).
        let close_bounds = |a: SharedBounds, b: SharedBounds| {
            (a.x - b.x).abs() < 1e-4
                && (a.y - b.y).abs() < 1e-4
                && (a.width - b.width).abs() < 1e-4
                && (a.height - b.height).abs() < 1e-4
        };
        assert!(
            close_bounds(
                mk(PathMotion::ArcBelow, 0.0).lerped(),
                SharedBounds::new(0.0, 0.0, 100.0, 100.0)
            ),
            "arc start endpoint"
        );
        assert!(
            close_bounds(
                mk(PathMotion::ArcBelow, 1.0).lerped(),
                SharedBounds::new(200.0, 100.0, 100.0, 100.0)
            ),
            "arc end endpoint"
        );
        assert!(
            close_bounds(
                mk(PathMotion::ArcAbove, 1.0).lerped(),
                SharedBounds::new(200.0, 100.0, 100.0, 100.0)
            ),
            "above-arc end endpoint"
        );
        // Compose degenerate rule: either-dimension travel falls back to
        // linear (axis-aligned flights stay straight — no sideways bulge).
        let flat = |sx: f32, sy: f32, ex: f32, ey: f32| {
            let v = TransitionVisual {
                start: SharedBounds::new(sx, sy, 100.0, 100.0),
                end: SharedBounds::new(ex, ey, 100.0, 100.0),
                ..mk(PathMotion::ArcBelow, 0.5)
            };
            v.lerped()
        };
        assert_eq!(flat(0.0, 0.0, 200.0, 0.0), SharedBounds::new(100.0, 0.0, 100.0, 100.0));
        assert_eq!(flat(0.0, 0.0, 0.0, 200.0), SharedBounds::new(0.0, 100.0, 100.0, 100.0));
        assert_eq!(flat(50.0, 50.0, 50.0, 50.0), SharedBounds::new(50.0, 50.0, 100.0, 100.0));
    }

    #[test]
    pub(crate) fn bounds_alpha_matrix_claims_fade_channels() {
        let mk = |role, fx: Option<(VisibilityTransition, VisibilityTransition)>| TransitionVisual {
            start: SharedBounds::new(0.0, 0.0, 100.0, 100.0),
            end: SharedBounds::new(200.0, 0.0, 100.0, 100.0),
            progress: 0.25,
            flown: None,
            role,
            scene_alpha: None,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path: PathMotion::Linear,
            bounds_fx: fx,
            elevated: false,
            remeasure: false,
            scale_parts: Some((ContentScale::FillBounds, ImageAlignment::TopStart)),
            radius_from_auto: false,
            radius_to_auto: false,
        };
        let fade_in = VisibilityTransition::fade_in(TweenSpec::default());
        let fade_out = VisibilityTransition::fade_out(TweenSpec::default());
        let empty = VisibilityTransition::empty();
        // Element: classic crossfade, always.
        assert_eq!(mk(TransitionRole::Target, None).alpha(), 0.25);
        assert_eq!(mk(TransitionRole::Source, None).alpha(), 0.75);
        // A SCENE owns opacity when the end carries one: alpha() is the scene's visibility, NOT the
        // flight's crossfade. Compose splits it the same way (the scene transition fades, the shared
        // bounds animation moves), and fading an element twice would dim it twice as fast. Checked
        // with a fade pair attached too, so the scene value wins over the channels.
        for role in [TransitionRole::Source, TransitionRole::Target, TransitionRole::Morph] {
            for (vis, want) in [(0.0, 0.0), (0.4, 0.4), (1.0, 1.0)] {
                let mut v = mk(role.clone(), Some((fade_in.clone(), fade_out.clone())));
                v.scene_alpha = Some(vis);
                assert_eq!(v.alpha(), want, "scene visibility must own opacity for {role:?}");
                let mut v = mk(role.clone(), None);
                v.scene_alpha = Some(vis);
                assert_eq!(v.alpha(), want, "…and with no enter/exit either");
            }
        }
        // Bounds + fade enter/exit: identical numbers (fade defaults claim
        // the crossfade they already produce).
        assert_eq!(
            mk(TransitionRole::Target, Some((fade_in.clone(), fade_out.clone()))).alpha(),
            0.25
        );
        assert_eq!(
            mk(TransitionRole::Source, Some((fade_in.clone(), fade_out.clone()))).alpha(),
            0.75
        );
        // Bounds + fade-less customs ride opaque (Compose rule).
        assert_eq!(
            mk(TransitionRole::Target, Some((empty.clone(), fade_out.clone()))).alpha(),
            1.0
        );
        assert_eq!(
            mk(TransitionRole::Source, Some((fade_in.clone(), empty.clone()))).alpha(),
            1.0
        );
        // Morphs never touch opacity, pair or not.
        assert_eq!(mk(TransitionRole::Morph, None).alpha(), 1.0);
        assert_eq!(
            mk(TransitionRole::Morph, Some((fade_in.clone(), fade_out.clone()))).alpha(),
            1.0
        );
    }

    #[test]
    pub(crate) fn detect_switch_finds_pair_under_different_slots() {
        let prev: HashMap<(u64, String), u64> =
            [((7, "hero".to_string()), 100), ((7, "static".to_string()), 101)].into_iter().collect();
        let live: HashMap<(u64, String), u64> =
            [((7, "hero".to_string()), 200), ((7, "static".to_string()), 101)].into_iter().collect();
        let found = detect_switch(&prev, &live);
        assert_eq!(found.len(), 1, "only the re-slotted key is a switch");
        assert_eq!(found[0].scope_id, 7);
        assert_eq!(found[0].key, "hero");
        assert_eq!((found[0].old_slot, found[0].new_slot), (100, 200));
    }

    #[test]
    pub(crate) fn detect_switch_ignores_unrelated_churn() {
        let prev: HashMap<(u64, String), u64> = [((1, "a".to_string()), 10)].into_iter().collect();
        // Brand-new key (no old endpoint) and vanished key (no new endpoint).
        let live: HashMap<(u64, String), u64> = [((1, "b".to_string()), 20)].into_iter().collect();
        assert!(detect_switch(&prev, &live).is_empty(), "appear-without-disappear is not a switch");
        assert!(detect_switch(&prev, &prev).is_empty(), "identical frames never switch");
    }

    #[test]
    pub(crate) fn flight_full_lifecycle() {
        let mut f = Flight::new(1, 7, "hero".to_string());
        let acts = f.on_event(FlightEvent::CounterpartAppeared { source_slot: 100, target_slot: 200 });
        assert_eq!(f.phase, FlightPhase::AwaitingBounds);
        assert_eq!(acts, vec![FlightAction::CollectBounds { source_slot: 100, target_slot: 200 }]);

        let (s, e) = (SharedBounds::new(0.0, 0.0, 50.0, 50.0), SharedBounds::new(200.0, 0.0, 150.0, 150.0));
        let acts = f.on_event(FlightEvent::BoundsReady { start: s, end: e });
        assert_eq!(f.phase, FlightPhase::Flying);
        assert_eq!(acts, vec![FlightAction::StartFlight { source_slot: 100, target_slot: 200, start: s, end: e }]);

        let acts = f.on_event(FlightEvent::ProgressDone);
        assert_eq!(f.phase, FlightPhase::Finishing);
        assert_eq!(
            acts,
            vec![
                FlightAction::ReleaseRetained { source_slot: 100 },
                FlightAction::FinishFlight,
            ],
            "source freed at alpha 0 — glitch-free removal"
        );

        let acts = f.on_event(FlightEvent::Retarget);
        assert_eq!((f.phase, acts), (FlightPhase::Idle, vec![FlightAction::Noop]));
    }

    #[test]
    pub(crate) fn flight_retarget_bumps_generation() {
        let mut f = Flight::new(2, 7, "hero".to_string());
        f.on_event(FlightEvent::CounterpartAppeared { source_slot: 1, target_slot: 2 });
        f.on_event(FlightEvent::BoundsReady {
            start: SharedBounds::new(0.0, 0.0, 10.0, 10.0),
            end: SharedBounds::new(50.0, 0.0, 10.0, 10.0),
        });
        let prev_gen = f.generation;
        let acts = f.on_event(FlightEvent::Retarget);
        assert_eq!(f.generation, prev_gen + 1, "actuators ignore stale generations");
        assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]), "restart from current visual");
        assert_eq!(f.phase, FlightPhase::Flying, "still flying after redirect");
    }

    #[test]
    pub(crate) fn flight_cancel_on_dispose_and_close() {
        for ev in [FlightEvent::ScopeDisposed, FlightEvent::WindowClosed] {
            let mut f = Flight::new(3, 7, "k".to_string());
            f.on_event(FlightEvent::CounterpartAppeared { source_slot: 1, target_slot: 2 });
            let acts = f.on_event(ev);
            assert_eq!(f.phase, FlightPhase::Cancelled);
            assert_eq!(acts, vec![FlightAction::CancelFlight]);
            // Terminal: further events are harmless.
            let acts = f.on_event(FlightEvent::ProgressDone);
            assert_eq!(acts, vec![FlightAction::Noop]);
        }
    }

    #[test]
    pub(crate) fn flight_stray_events_are_noop() {
        let mut f = Flight::new(4, 7, "k".to_string());
        assert_eq!(f.on_event(FlightEvent::ProgressDone), vec![FlightAction::Noop]);
        assert_eq!(f.on_event(FlightEvent::BoundsReady {
            start: SharedBounds::new(0.0, 0.0, 1.0, 1.0),
            end: SharedBounds::new(0.0, 0.0, 1.0, 1.0),
        }), vec![FlightAction::Noop]);
        assert_eq!(f.phase, FlightPhase::Idle);
    }

    #[test]
    pub(crate) fn scope_state_key_isolation() {
        let a = SharedTransitionScope::new(1);
        let b = SharedTransitionScope::new(2);
        assert_ne!(
            a.shared_content_state("hero"),
            b.shared_content_state("hero"),
            "same key in different scopes must never match"
        );
        assert_eq!(a.shared_content_state("hero"), a.shared_content_state("hero"));
    }
}

