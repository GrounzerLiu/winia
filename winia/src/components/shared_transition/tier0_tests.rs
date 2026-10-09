//! The shared-element tests, split by what they exercise.

use super::*;
use crate::animation::SpringSpec;
use crate::animation::visibility::SlideDirection;
use crate::animation::visibility::SlideOffset;
use crate::graphics::Shape;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::graphics::Color;
use crate::nav::{NavBackStack, NavDisplay, NavEntry};
use crate::layout::Column;
use crate::layout::Row;
use crate::layout::Stack;

/// Plain box leaf with a shared-element marker (no text — keeps the
/// mechanics test headless-simple; paint movement is asserted via raster).
/// `in_overlay` is Compose `renderInOverlayDuringTransition`.
fn hero_leaf(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
    in_overlay: bool,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(scope.shared_content_state("hero"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, in_overlay),
    );
    ctx.end_node();
}

/// Layout spacer (occupies space, paints nothing).
fn gap_leaf(ctx: &mut ComposeCtx, w: f32, h: f32) {
    let key = ctx.next_key();
    ctx.start_leaf(key, Modifier::new().size(w, h));
    ctx.end_node();
}

// NOTE: screens are #[composable] like production code. Plain closures
// would collapse in cfg(test) key fallback (path-hash only, no statement
// ids): the detail gap (Column-child-0) would collide with the list hero
// (same position) and reuse its node, aborting the flight. Macro'd
// screens carry distinct source hashes — mirroring production.
#[crate::composable]
fn list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, true);
}

#[crate::composable]
fn detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    gap_leaf(ctx, 400.0, 100.0);
    hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
}

/// One app-loop step: compose → layout → post-layout flight poll.
/// One app-loop step: compose → layout → post-layout flight poll.
fn frame(composer: &mut Composer, show: &State<bool>) {
    let s = show.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if s.get() {
                    list_screen(ctx, &scope);
                } else {
                    detail_screen(ctx, &scope);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn advance(composer: &mut Composer, show: &State<bool>) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    frame(composer, show);
}

/// Marked node indices in the CURRENT tree.
fn marked_indices(composer: &Composer) -> Vec<usize> {
    let mut out = Vec::new();
    if let Some(root) = composer.layout_root_idx() {
        let nodes = composer.arena_nodes();
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if find_shared_marker(&nodes[idx].modifier).is_some() {
                out.push(idx);
            }
            stack.extend(nodes[idx].children.iter().copied());
        }
    }
    out
}

/// Mark the scope of the composer's first marked node as having a running transition.
///
/// A morph only opens inside a transition now (Compose semantics: the element animates because the
/// transition animating it changed its bounds, not because a layout pass did). Tests that used a bare
/// layout change to start one declare the transition first; the coordinator's end-of-poll sync clears
/// the flag again once that poll has run.
fn mark_scope_active(composer: &Composer) {
    if let Some(idx) = marked_indices(composer).first() {
        if let Some(m) = find_shared_marker(&composer.arena_nodes()[*idx].modifier) {
            set_scope_transition_active_for_test(m.scope_id, true);
        }
    }
}

/// Mirror the app loop: main tree, then the transition layer
/// (detached sources + elevated endpoints, absolute coords).
fn render_heads(composer: &Composer) -> skia_safe::Surface {
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((400, 400)).expect("raster surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    let nodes = composer.arena_nodes();
    if let Some(root) = composer.layout_root_idx() {
        crate::render::render(nodes, root, surface.canvas());
        composer.render_layer(surface.canvas());
    }
    surface
}

fn pixel_rgb(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8) {
    let mut px = [0u8; 4];
    let info = skia_safe::ImageInfo::new(
        (1, 1),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    surface.read_pixels(&info, &mut px, 4, (x, y));
    (px[0], px[1], px[2])
}

fn close_enough(c: (u8, u8, u8), e: (u8, u8, u8), tol: u8) -> bool {
    c.0.abs_diff(e.0) <= tol && c.1.abs_diff(e.1) <= tol && c.2.abs_diff(e.2) <= tol
}

fn node_center(composer: &Composer, idx: usize) -> (i32, i32) {
    let n = &composer.arena_nodes()[idx];
    ((n.position.x + n.measured_size.width / 2.0) as i32, (n.position.y + n.measured_size.height / 2.0) as i32)
}

fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
    crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// A shared container that paints NOTHING itself, wrapping a leaf that fills it with green.
/// The raster harness can then only see the leaving end if the ghost still has its child —
/// `tier0_flight_completes_end_to_end` puts the colour on the container's own background, so a
/// vanished child is invisible to it and it cannot catch this defect.
fn painted_child_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, h: f32) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        Column::new()
            .modifier(
                Modifier::new()
                    .size(200.0, h)
                    .shared_bounds(
                        scope.shared_content_state("painted"),
                        VisibilityTransition::fade_in(TweenSpec::default()),
                        VisibilityTransition::fade_out(TweenSpec::default()),
                        BoundsTransform::default(),
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::AnimatedSize,
                        PathMotion::Linear,
                        0.0,
                        true,
                    ),
            )
            .build(ctx, |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(
                    key,
                    Modifier::new()
                        .fill_max_size()
                        .background(Color::GREEN, Shape::Rectangle),
                );
                ctx.end_node();
            });
    });
}

/// The two screens, as `#[composable]` wrappers calling the shared helper — the same shape as
/// `switch_frame_list` / `switch_frame_detail`, which is what gets the marker registered at a
/// stable call site (calling the helper directly leaves no endpoint to detect a switch with).
#[crate::composable]
fn painted_list(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    painted_child_screen(ctx, scope, 80.0);
}

#[crate::composable]
fn painted_detail(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    painted_child_screen(ctx, scope, 240.0);
}

/// The headless raster probe for the "the animation starts fully transparent" report: on the
/// switch frame the leaving end must still paint its CHILD. Built with a container that has no
/// background of its own, which is the part earlier attempts missed — a card that paints its
/// own background cannot show a lost child.
///
/// What it pins, measured (this test, and the review that supplied the scene):
/// - removing the descendant shelter in `detach_source` -> RED, the ghost's child becomes
///   `size=(0.0, 0.0) key=0x0` and the sample is the canvas colour `(255, 255, 255)`;
/// - calling `prune_stale_child_links` from `materialize()` again (before the retention) ->
///   RED, the ghost has `children=[]` and the same canvas colour.
///
/// NOT pinned, so do not claim it: with the prune DISABLED ENTIRELY this stays GREEN (measured
/// by the reviewer). The prune removes a stale listing; it is not what paints the ghost. This
/// test locks the shelter and the prune's PLACEMENT, not its existence.

/// The trace facility must record a headless flight: per frame, per end, with the geometry and the
/// opacity that a report can be built from. Gated on the feature — with `anim-trace` off there is
/// nothing to record by design, and the suite must still pass.
///
/// Teeth: remove the emission in `write_flight_visuals` and this fails on the very first
/// assertion, because nothing is captured at all.
/// Scrolled screens: the hero lives inside a vertically scrolled
/// container so both flight ends sit under an ancestor scroll offset.
#[crate::composable]
fn scrolled_list_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    scroll: &crate::modifier::ScrollState,
) {
    Column::new()
        .modifier(Modifier::new().height(200.0).vertical_scroll(scroll.clone()))
        .build(ctx, |ctx| {
            hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, true);
            gap_leaf(ctx, 400.0, 400.0);
        });
}

#[crate::composable]
fn scrolled_detail_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    scroll: &crate::modifier::ScrollState,
) {
    Column::new()
        .modifier(Modifier::new().height(200.0).vertical_scroll(scroll.clone()))
        .build(ctx, |ctx| {
            gap_leaf(ctx, 400.0, 140.0);
            hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
        });
}

/// One app-loop step for the scrolled screens.
fn scroll_frame(composer: &mut Composer, show: &State<bool>, scroll: &crate::modifier::ScrollState) {
    let s = show.clone();
    let sc = scroll.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if s.get() {
                    scrolled_list_screen(ctx, &scope, &sc);
                } else {
                    scrolled_detail_screen(ctx, &scope, &sc);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn scroll_advance(composer: &mut Composer, show: &State<bool>, scroll: &crate::modifier::ScrollState) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    scroll_frame(composer, show, scroll);
}

/// Pure position accumulation root → node (no scroll subtraction — the
/// frame render's `(x, y)` uses at the flight node).
fn pure_accumulation(composer: &Composer, idx: usize) -> (f32, f32) {
    let nodes = composer.arena_nodes();
    let id_to_idx: HashMap<u64, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let (mut ax, mut ay) = (nodes[idx].position.x, nodes[idx].position.y);
    let mut cur = idx;
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        ax += nodes[pidx].position.x;
        ay += nodes[pidx].position.y;
        cur = pidx;
    }
    (ax, ay)
}


// ── Overlay escape (Compose `renderInOverlayDuringTransition`) ──
//
// The entering hero lives inside a scroll viewport that sits LOW in the
// window while the leaving hero sits at the very top, so mid-flight the
// lerped rect is ABOVE that viewport — outside the target's ancestor clip.
// In-tree painting is cut there (the ancestor clip is on the canvas and a
// descendant can never un-set it); the transition layer paints it anyway.

#[crate::composable]
fn escape_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: bool) {
    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Column::new()
                .modifier(Modifier::new().size(60.0, 60.0))
                .build(ctx, |ctx| {
                    hero_leaf(ctx, 60.0, 60.0, Color::RED, scope, in_overlay);
                });
        });
}

#[crate::composable]
fn escape_detail_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    scroll: &crate::modifier::ScrollState,
    in_overlay: bool,
) {
    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            // Pushes the clipped viewport down to y ≈ 240.
            gap_leaf(ctx, 10.0, 240.0);
            Column::new()
                .modifier(Modifier::new().size(200.0, 160.0).vertical_scroll(scroll.clone()))
                .build(ctx, |ctx| {
                    gap_leaf(ctx, 10.0, 30.0);
                    hero_leaf(ctx, 140.0, 80.0, Color::BLUE, scope, in_overlay);
                });
        });
}

fn escape_frame(
    composer: &mut Composer,
    show: &State<bool>,
    scroll: &crate::modifier::ScrollState,
    in_overlay: bool,
) {
    let s = show.clone();
    let sc = scroll.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if s.get() {
                    escape_list_screen(ctx, &scope, in_overlay);
                } else {
                    escape_detail_screen(ctx, &scope, &sc, in_overlay);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn escape_advance(
    composer: &mut Composer,
    show: &State<bool>,
    scroll: &crate::modifier::ScrollState,
    in_overlay: bool,
) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    escape_frame(composer, show, scroll, in_overlay);
}

/// (progress, lerped rect) of the single active flight.
fn flight_probe(composer: &Composer) -> Option<(f32, SharedBounds)> {
    composer.shared_flights.values().next().map(|a| {
        let p = a.progress.peek();
        let end = a.flight.end.unwrap_or(a.start);
        (p, a.start.lerp(&end, p))
    })
}

/// Absolute rect (canvas frame) of the nearest scrolled ancestor viewport.
fn nearest_viewport_abs(composer: &Composer, idx: usize) -> Option<(f32, f32, f32, f32)> {
    let nodes = composer.arena_nodes();
    let id_to_idx: HashMap<u64, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut cur = idx;
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { return None };
        let n = &nodes[pidx];
        if n.scroll_viewport_height > 0.0 {
            let b = abs_rect_upward(nodes, &id_to_idx, pidx);
            return Some((b.x, b.y, n.scroll_viewport_width, n.scroll_viewport_height));
        }
        cur = pidx;
    }
    None
}

/// Drive the escape scenario to mid-flight and return
/// (progress, lerped rect, target index, target ancestor viewport).
#[allow(clippy::type_complexity)]
fn escape_to_midflight(
    composer: &mut Composer,
    show: &State<bool>,
    scroll: &crate::modifier::ScrollState,
    in_overlay: bool,
) -> (f32, SharedBounds, usize, (f32, f32, f32, f32)) {
    escape_frame(composer, show, scroll, in_overlay);
    show.set(false);
    escape_frame(composer, show, scroll, in_overlay);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");
    for _ in 0..200 {
        if let Some((p, l)) = flight_probe(composer) {
            if (0.4..=0.6).contains(&p) {
                let tslot = composer
                    .shared_flights
                    .values()
                    .next()
                    .and_then(|a| a.flight.target_slot)
                    .expect("target slot");
                let nodes = composer.arena_nodes();
                let root = composer.layout_root_idx().expect("root");
                let tidx = find_idx_by_slot(nodes, root, tslot).expect("target node");
                let vp = nearest_viewport_abs(composer, tidx).expect("clipped ancestor");
                return (p, l, tidx, vp);
            }
        }
        if composer.shared_flights.is_empty() {
            break;
        }
        escape_advance(composer, show, scroll, in_overlay);
    }
    panic!("flight must pass through the mid-flight window");
}


/// Tier1 pair where the PEER (overlay) target sits inside a clipped
/// viewport low in the window — the same escape geometry, cross-composer.
fn escape_cross_frame(
    a: &mut Composer,
    b: &mut Composer,
    show_a: &State<bool>,
    show_b: &State<bool>,
    scope: &SharedTransitionScope,
    scroll: &crate::modifier::ScrollState,
) {
    let (sa, sb) = (show_a.clone(), show_b.clone());
    let (sca, scb) = (scope.clone(), scope.clone());
    let sc = scroll.clone();
    cross_frame(
        a,
        b,
        |ctx| shell(ctx, |ctx| {
            if sa.get() {
                hero_leaf(ctx, 60.0, 60.0, Color::RED, &sca, true);
            }
        }),
        |ctx| shell(ctx, |ctx| {
            if sb.get() {
                gap_leaf(ctx, 10.0, 240.0);
                Column::new()
                    .modifier(Modifier::new().size(200.0, 160.0).vertical_scroll(sc.clone()))
                    .build(ctx, |ctx| {
                        gap_leaf(ctx, 10.0, 30.0);
                        hero_leaf(ctx, 140.0, 80.0, Color::BLUE, &scb, true);
                    });
            }
        }),
    );
}

/// Tier1 hit routing: the peer's own hit test must reach the flying target
/// at its animated rect even where the peer's ancestors reject the point.
/// This is the case the cross-composer ghost cannot cover — its live target
/// lives in an arena the single-arena ghost search cannot see — so before
/// the overlay pass the tap was simply lost (`hit_overlay` probes exactly
/// this call for the overlay's local point).

/// Marker flags (and specs) are sampled when a flight resolves, so the
/// arena's marker must be fresh. Freshness follows *where* the flag is
/// read: a state read in the slot that builds the marker recomposes that
/// slot and rewrites the node's modifier, while a read in an OUTER slot
/// leaves this subtree skipped and keeps the cached modifier — the runtime
/// toggle then silently does nothing until something else rebuilds the
/// screen. Build markers with their flags read in their own slot.

/// Marker leaf whose flag is read in THIS composable's slot — the pattern
/// the demo's `hero_box` helper uses.
#[crate::composable]
fn flag_hero(
    ctx: &mut ComposeCtx,
    overlay: &State<bool>,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    color: Color,
) {
    let in_overlay = overlay.get();
    hero_leaf(ctx, w, h, color, scope, in_overlay);
}

/// One shared hero composable, called from BOTH screen arms — the shape the
/// demo uses after switching to a `hero_box` helper. A key collision across
/// the arms would silently degrade the screen switch into a same-screen
/// morph, so the demo's structure is pinned here.
#[crate::composable]
fn shared_hero_box(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    color: Color,
    in_overlay: bool,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(scope.shared_content_state("hero"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, in_overlay),
    );
    ctx.end_node();
}


/// Pixel delta between two frames of the same size (channel threshold 2 to
/// ignore antialiasing noise). Backing helper for the same-frame overlay
/// A/B checks, where only one flag changes between the two renders.
fn surface_diff(a: &mut skia_safe::Surface, b: &mut skia_safe::Surface) -> (usize, u8) {
    const W: usize = 400;
    const H: usize = 400;
    let info = skia_safe::ImageInfo::new(
        (W as i32, H as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let mut pa = vec![0u8; W * H * 4];
    let mut pb = vec![0u8; W * H * 4];
    a.read_pixels(&info, &mut pa, W * 4, (0, 0));
    b.read_pixels(&info, &mut pb, W * 4, (0, 0));
    let mut n = 0usize;
    let mut maxd = 0u8;
    for i in (0..pa.len()).step_by(4) {
        let d = pa[i]
            .abs_diff(pb[i])
            .max(pa[i + 1].abs_diff(pb[i + 1]))
            .max(pa[i + 2].abs_diff(pb[i + 2]));
        if d > 2 {
            n += 1;
        }
        maxd = maxd.max(d);
    }
    (n, maxd)
}

/// Same-frame A/B of the opt-out: render as-is, flip ONLY the target's
/// render pass, render again, report the delta. Everything else (progress,
/// rects, radii, scroll) is identical, so any difference is the pass.
fn opt_out_vs_elevated_delta(composer: &mut Composer) -> (usize, u8) {
    let mut before = render_heads(composer);
    let tslot = composer
        .shared_flights
        .values()
        .next()
        .and_then(|a| a.flight.target_slot);
    if let Some(slot) = tslot {
        let root = composer.layout_root_idx().expect("root");
        let tidx = find_idx_by_slot(composer.arena_nodes(), root, slot).expect("target");
        assert!(
            !composer.arena.nodes[tidx].transition.as_ref().unwrap().elevated,
            "A/B starts from the opt-out state"
        );
        composer.arena.nodes[tidx].transition.as_mut().unwrap().elevated = true;
        composer.elevated_roots.push(tidx);
        composer.rebuild_layer_order();
    }
    let mut after = render_heads(composer);
    surface_diff(&mut before, &mut after)
}

/// Drive to mid-flight in the given direction and A/B the render pass.
fn opt_out_ab_at_midflight(reverse: bool) -> (f32, usize, u8) {
    let mut composer = Composer::new();
    let show = State::new(!reverse); // forward starts on the list screen
    let scroll = crate::modifier::ScrollState::new();
    escape_frame(&mut composer, &show, &scroll, false);
    show.set(reverse);
    escape_frame(&mut composer, &show, &scroll, false);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");
    let mut p = 0.0;
    for _ in 0..200 {
        match flight_probe(&composer) {
            Some((prog, _)) if prog >= 0.5 => {
                p = prog;
                break;
            }
            None => break,
            _ => {}
        }
        escape_advance(&mut composer, &show, &scroll, false);
    }
    let (n, maxd) = opt_out_vs_elevated_delta(&mut composer);
    crate::animation::clear_all_animations();
    (p, n, maxd)
}

/// The opt-out artifact is DIRECTIONAL, and that is not obvious from a
/// running window (it cost a long debugging round): only the leg whose
/// DESTINATION clips shows the step, because only the entering end can be
/// clipped at all. Into the clipped screen → the two passes disagree;
/// flying back OUT of it → they are pixel-identical.

/// The two screens of the under-bar scenario. Macro'd composables on
/// purpose: the cfg(test) key fallback is path-hash based, so plain
/// branches let one screen's node take over the other's slot (the detail
/// hero ended up with the list gap's measurements) — the same trap the
/// list/detail screens document.
#[crate::composable]
fn under_bar_list(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: bool) {
    gap_leaf(ctx, 10.0, 300.0);
    hero_leaf(ctx, 150.0, 150.0, Color::RED, scope, in_overlay);
}

#[crate::composable]
fn under_bar_detail(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: bool) {
    // Lands at the top of the window, so its upper band sits under the bar.
    hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, in_overlay);
}

/// The correct use of `render_in_overlay = false` on the ENTERING side:
/// keep the flight in tree order so chrome painted later still covers it
/// ("content sliding under a pinned bar"). With the flag on (Compose
/// default) the endpoint becomes a layer root and paints OVER that chrome
/// instead — which is the artifact this flag exists to avoid when the
/// element belongs under a bar.
///
/// Tree: Stack { Column { screen }, bar } — the bar is the LAST child, so
/// it paints after the whole screen and tree order alone decides whether
/// the flying hero passes over it or under it.

/// Under-bar scenario with the bar opting into the scope overlay (Compose
/// `Modifier.renderInSharedTransitionScopeOverlay`).
fn chrome_frame(
    composer: &mut Composer,
    show: &State<bool>,
    chrome_z: f32,
    hero_in_overlay: bool,
) {
    let s = show.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("scope");
            Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        under_bar_list(ctx, &scope, hero_in_overlay);
                    } else {
                        under_bar_detail(ctx, &scope, hero_in_overlay);
                    }
                });
                Column::new()
                    .modifier(
                        Modifier::new()
                            .size(400.0, 60.0)
                            .background(Color::GREEN, Shape::Rectangle)
                            .render_in_shared_transition_scope_overlay(&scope, chrome_z),
                    )
                    .build(ctx, |_| {});
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn chrome_bar_index(composer: &Composer) -> usize {
    composer
        .arena_nodes()
        .iter()
        .position(|n| find_scope_overlay_marker(&n.modifier).is_some())
        .expect("chrome bar node")
}

/// A panicking scene must not leave its entry on the scene stack: markers composed afterwards would be
/// attributed to a scene that no longer exists, which is exactly the stale-attribution failure the
/// ancestry lookup exists to avoid. Teeth: remove the `PopOnDrop` guard in `with_nav_scene` and the
/// stack keeps the entry after the panic.

/// Chrome nested inside chrome keeps its OWN layer entry: the layer draws every marked root with
/// `layer_root = true`, so the inner bar keeps its `zIndexInOverlay` in the sort and escapes its
/// ancestor's clip. An earlier version suppressed the inner marker to avoid "painting nowhere" — the
/// premise was wrong (it painted exactly once, as its own entry) and the suppression silently made it an
/// ordinary child of the ancestor's draw.
///
/// Teeth: skip descending into a marked subtree (the suppressed behaviour) and the inner bar stops being
/// a layer root, which this asserts on.

/// Compose `Modifier.renderInSharedTransitionScopeOverlay`: chrome that
/// opts in keeps its spatial relationship while the flight runs, which is
/// the piece the shared-element flag cannot do — it covers BOTH ends,
/// including the detached leaving ghost.

/// The elevation lasts exactly as long as the scope is transitioning —
/// outside a flight the chrome is ordinary tree content again.

/// `z_index` orders the chrome against the flights (Compose
/// `zIndexInOverlay`): shared endpoints default to 0, so a bar on top
/// passes a larger value and one that wants to stay under passes a
/// smaller one.

/// Chrome in a PEER composer (the case a composer's own poll cannot decide:
/// a Tier1 flight lives in the MAIN map, so the dialog composer's
/// `shared_flights` is empty). The window-level cross-poll pushes the union.

/// Equal-z ties are decided by insertion order, and that order is part of
/// the contract: the entering target sits under its ghost (pre-overlay
/// compositing) and chrome sits under nothing (pinned chrome wins ties).

/// The `Morph` exemption: a same-screen resize marked `in_overlay = true`
/// stays in the tree — it is not a cross-composable flight, so escaping its
/// container would be a regression, not a feature.

/// The in-tree SKIP is what keeps a layer root from being painted twice.
/// An opaque bar hides a double draw, so this uses a translucent one: the
/// composite over the white background tells one draw from two.

/// Layout-side probe, no flight involved: a frame written on a node must
/// reach that node's reported size AND its parent's placement through the
/// layout invalidation channel alone — `layout()` with no `compose()` in
/// between (the zero-recomposition promise in miniature).

/// Counts real executions of the scenario composables: the flight must not
/// recompose anything, so this must not move while it flies.
static LAYOUT_SCENARIO_BUILDS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// A marked container with one live child (observable content width) …
#[crate::composable]
fn marked_block(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    color: Color,
    resize: ResizeMode,
    placeholder: PlaceHolderSize,
) {
    let _ = LAYOUT_SCENARIO_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_bounds(
                    scope.shared_content_state("hero"),
                    VisibilityTransition::fade_in(TweenSpec::default()),
                    VisibilityTransition::fade_out(TweenSpec::default()),
                    BoundsTransform::default(),
                    resize,
                    placeholder,
                    PathMotion::Linear,
                    0.0,
                    true,
                ),
        )
        .build(ctx, |ctx| {
            // Follows whatever width the container was measured at, and
            // carries a distinct colour so a raster test can tell SCALED
            // content (band shrinks with the flight) from CROPPED content
            // (band keeps its 60px height and runs out of the clip).
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .fill_max_width()
                    .height(60.0)
                    .background(Color::GREEN, Shape::Rectangle),
            );
            ctx.end_node();
        });
}

/// One sibling below the block: its y is the size the parent was told.
fn block_sibling(ctx: &mut ComposeCtx) {
    let key = ctx.next_key();
    ctx.start_leaf(key, Modifier::new().size(40.0, 10.0));
    ctx.end_node();
}

#[crate::composable]
fn list_layout_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    resize: ResizeMode,
    placeholder: PlaceHolderSize,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        marked_block(ctx, scope, 120.0, 60.0, Color::RED, resize, placeholder);
        block_sibling(ctx);
    });
}

#[crate::composable]
fn detail_layout_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    resize: ResizeMode,
    placeholder: PlaceHolderSize,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        marked_block(ctx, scope, 300.0, 200.0, Color::BLUE, resize, placeholder);
        block_sibling(ctx);
    });
}

/// Compose's layout contract for a flight, as one 4-way matrix:
///   - `RemeasureToBounds` re-lays-out the CONTENT at the animated size
///     (the inner child's width follows the flight) instead of scaling it;
///   - `PlaceHolderSize::AnimatedSize` makes the PARENT reflow (the sibling
///     rides the animated height); `ContentSize`/`JumpCut` keep the target
///     size, so the sibling never moves;
///   - nothing recomposes: the flight drives layout through the layout
///     invalidation channel (measure-time state read), not composition.

/// Teardown contract: dropping the override must also SEED one layout
/// invalidation. `layout()` clears `layout_dirty` tree-wide every pass, so
/// without the seed the fold would keep the last animated size forever —
/// which is exactly what a mid-flight cancel leaves behind (the final
/// frame is NOT the natural size, unlike a completed flight).

/// `LayoutNode.flight_measure` must not exist on the default contract
/// (`ScaleToBounds` + `JumpCut`): the writers used to seed the layout
/// invalidation unconditionally, which re-measured the endpoint and its
/// whole ancestor chain every frame for nothing.

/// `cargo run`-visible contract: `shared_element` re-measures like Compose's
/// (`ResizeMode` is not even a parameter there), and its placeholder still
/// decides what the parent observes.

/// `shared_element` variant of the marked block: Compose's Element marker
/// carries no resize parameter (it always re-measures), only a placeholder.
#[crate::composable]
fn marked_element_block(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    color: Color,
    placeholder: PlaceHolderSize,
) {
    let _ = LAYOUT_SCENARIO_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(
                    scope.shared_content_state("hero"),
                    BoundsTransform::default(),
                    placeholder,
                    PathMotion::Linear,
                    0.0,
                    true,
                ),
        )
        .build(ctx, |ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new().fill_max_width().height(20.0));
            ctx.end_node();
        });
}

#[crate::composable]
fn element_list_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    placeholder: PlaceHolderSize,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        marked_element_block(ctx, scope, 120.0, 60.0, Color::RED, placeholder);
        block_sibling(ctx);
    });
}

#[crate::composable]
fn element_detail_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    placeholder: PlaceHolderSize,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        marked_element_block(ctx, scope, 300.0, 200.0, Color::BLUE, placeholder);
        block_sibling(ctx);
    });
}

/// Pixel-level proof for the two `reported != content` combinations that
/// the layout-only assertions cannot see: the box the hero PAINTS is the
/// reel/content one, so nothing of it may appear outside the lerped rect.
/// (This is the blind spot that hid the scale/spill regression.)

/// Stacked marked block: ten 20px bands make the hit descent observable
/// (which band a tap lands on tells us the local mapping exactly).
#[crate::composable]
fn stacked_marked_block(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    resize: ResizeMode,
    placeholder: PlaceHolderSize,
) {
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .background(Color::RED, Shape::Rectangle)
                .shared_bounds(
                    scope.shared_content_state("hero"),
                    VisibilityTransition::fade_in(TweenSpec::default()),
                    VisibilityTransition::fade_out(TweenSpec::default()),
                    BoundsTransform::default(),
                    resize,
                    placeholder,
                    PathMotion::Linear,
                    0.0,
                    true, // render_in_overlay: hits go through the LAYER path
                ),
        )
        .build(ctx, |ctx| {
            for _ in 0..10 {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_width().height(20.0));
                ctx.end_node();
            }
        });
}

#[crate::composable]
fn stacked_list_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    resize: ResizeMode,
    placeholder: PlaceHolderSize,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        stacked_marked_block(ctx, scope, 120.0, 60.0, resize, placeholder);
    });
}

#[crate::composable]
fn stacked_detail_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    resize: ResizeMode,
    placeholder: PlaceHolderSize,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        stacked_marked_block(ctx, scope, 300.0, 200.0, resize, placeholder);
    });
}

/// The LAYER hit path must invert the same transform the paint applied.
/// With `RemeasureToBounds + ContentSize` the content box is the animated
/// size while the parent sees the target size; the elevated route used to
/// map through `measured_size`, scaling the local point by target/anim and
/// landing on the wrong child (the in-tree route was already correct).

/// Placeholder-taking hero leaf (the Tier-1 layout contract needs a marker
/// that does NOT resolve to the default idle frame).
fn hero_leaf_ph(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
    placeholder: PlaceHolderSize,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(
                scope.shared_content_state("hero"),
                BoundsTransform::default(),
                placeholder,
                PathMotion::Linear,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

/// Bounds-marked hero leaf: the DEFAULT contract (ScaleToBounds + JumpCut), whose
/// frame is idle by construction — the Tier-1 equivalent of the Tier-0
/// "default costs no layout" test.
fn hero_leaf_bounds(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_bounds(
                scope.shared_content_state("hero"),
                VisibilityTransition::fade_in(TweenSpec::default()),
                VisibilityTransition::fade_out(TweenSpec::default()),
                BoundsTransform::default(),
                ResizeMode::scale_to_bounds(),
                PlaceHolderSize::JumpCut,
                PathMotion::Linear,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

/// The default contract must cost Tier 1 no layout either: its frame is IDLE, so
/// the peer attaches no override and nothing is re-seeded — while the seeding
/// mechanism demonstrably CAN move the counter (control), so a counter stuck at
/// zero cannot pass this.

/// Tier-1 layout contract, end to end: the peer composer's node receives the
/// per-frame override while the flight runs, and BOTH teardown paths give it
/// back. The completion path used to clear only the visual, which left the
/// peer permanently frozen at its last animated size (the layout fold then
/// never re-measured again).

/// Hero leaf whose shared marker can be dropped WITHOUT removing the node:
/// the modifier chain keeps its shape and the node keeps its slot, only the
/// live-map entry disappears — which is what makes the flight "stale".
fn hero_leaf_marked(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
    marked: bool,
) {
    let key = ctx.next_key();
    let mut m = Modifier::new()
        .size(w, h)
        .background(color, Shape::rounded(8.0));
    if marked {
        m = m.shared_element(
            scope.shared_content_state("hero"),
            BoundsTransform::default(),
            PlaceHolderSize::AnimatedSize,
            PathMotion::Linear,
            0.0,
            true,
        );
    }
    ctx.start_leaf(key, m);
    ctx.end_node();
}

/// The leak the review predicted, on the path that actually reproduces it:
/// when the peer's marker disappears mid-flight the node SURVIVES (same
/// slot, same modifier chain shape) and the flight is cancelled as stale —
/// so the teardown is the only thing that can drop the override. Clearing
/// just the visual leaves the node reporting its last animated size forever
/// (the layout fold then never re-measures it again).
#[test]
fn stale_cancel_drops_the_surviving_peers_layout_override() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let (mut a, mut b) = (Composer::new(), Composer::new());
    let scope = SharedTransitionScope::new(81);
    let show_a = State::new(true);
    let show_b = State::new(false);
    let peer_marked = State::new(true);
    let frame = |a: &mut Composer, b: &mut Composer| {
        let (sa, sb, sca, scb) = (show_a.clone(), show_b.clone(), scope.clone(), scope.clone());
        let pm = peer_marked.clone();
        cross_frame(
            a,
            b,
            |ctx| {
                shell(ctx, |ctx| {
                    if sa.get() {
                        hero_leaf_marked(ctx, 120.0, 80.0, Color::RED, &sca, true);
                    }
                })
            },
            |ctx| {
                shell(ctx, |ctx| {
                    if sb.get() {
                        hero_leaf_marked(ctx, 300.0, 160.0, Color::BLUE, &scb, pm.get());
                    }
                })
            },
        );
    };
    frame(&mut a, &mut b);
    show_a.set(false);
    show_b.set(true);
    frame(&mut a, &mut b);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 flight opens");
    let tidx = marked_in(&b)[0];
    // Let the override reach the peer's layout (the cross-poll runs after it).
    crate::animation::update_animations();
    frame(&mut a, &mut b);
    assert!(
        b.arena_nodes()[tidx].flight_measure.is_some(),
        "the peer carries the override while the flight runs"
    );

    // The marker disappears, the NODE does not (same slot, same chain).
    let node_before = b.arena_nodes()[tidx].id;
    peer_marked.set(false);
    crate::animation::update_animations();
    frame(&mut a, &mut b);
    assert!(a.shared_flights.is_empty(), "the stale flight is cancelled");
    assert_eq!(
        b.arena_nodes()[tidx].id, node_before,
        "the peer node survived the marker drop (materialize reuses it — it is not rebuilt)"
    );
    assert!(
        b.arena_nodes()[tidx].flight_measure.is_none(),
        "the cancelled flight takes its layout override with it"
    );
    // The override is dropped in the poll, i.e. after layout, so the node is
    // re-measured by the NEXT layout — the documented one-frame lag.
    frame(&mut a, &mut b);
    assert_eq!(
        b.arena_nodes()[tidx].measured_size,
        crate::unit::Size::new(300.0, 160.0),
        "…and the surviving node re-measures at its natural size"
    );
    crate::animation::clear_all_animations();
}

/// The GHOST route maps a tap from the leaving rect into the target's live
/// subtree (`link_slot`). That map must land on the target's CONTENT box:
/// with `RemeasureToBounds + ContentSize` the parent-visible size is the
/// target's, while the content box is the animated one — mapping through
/// the former scales the point and activates the wrong row.

/// Circle hero (percent corners) — the shape the demo uses.
#[crate::composable]
fn circle_hero_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    color: Color,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        Column::new()
            .modifier(
                Modifier::new()
                    .size(w, h)
                    .background(color, Shape::Circle)
                    .shared_bounds(
                        scope.shared_content_state("hero"),
                        VisibilityTransition::fade_in(TweenSpec::default()),
                        VisibilityTransition::fade_out(TweenSpec::default()),
                        BoundsTransform::default(),
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                        PathMotion::Linear,
                        0.0,
                        true,
                    ),
            )
            .build(ctx, |_| {});
    });
}

#[crate::composable]
fn circle_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    circle_hero_screen(ctx, scope, 150.0, 150.0, Color::RED);
}

#[crate::composable]
fn circle_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    circle_hero_screen(ctx, scope, 320.0, 170.0, Color::BLUE);
}

/// Percent corners (Circle/Pill) must track the box the flight PAINTS into.
/// They are evaluated on the lerped rect, so the leaving ghost — whose own
/// layout box never moves — would otherwise keep a frozen corner radius for
/// the whole flight (and any `ScaleToBounds` target with it).


/// Hero screen with an arbitrary shape (the demo flies a Circle in the list
/// to a Rectangle inside the detail container).
#[crate::composable]
fn shape_hero_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    color: Color,
    shape: Shape,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        Column::new()
            .modifier(
                Modifier::new()
                    .size(w, h)
                    .background(color, shape)
                    .shared_bounds(
                        scope.shared_content_state("hero"),
                        VisibilityTransition::fade_in(TweenSpec::default()),
                        VisibilityTransition::fade_out(TweenSpec::default()),
                        BoundsTransform::default(),
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                        PathMotion::Linear,
                        0.0,
                        true,
                    ),
            )
            .build(ctx, |_| {});
    });
}

#[crate::composable]
fn circle_to_rect_list(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    shape_hero_screen(ctx, scope, 150.0, 150.0, Color::RED, Shape::Circle);
}

#[crate::composable]
fn circle_to_rect_detail(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    shape_hero_screen(ctx, scope, 320.0, 170.0, Color::BLUE, Shape::Rectangle);
}

/// THE DEMO'S SHAPE PAIR: a percent corner (Circle) flying to a fixed one
/// (Rectangle). Both ends must paint the SAME radius at every progress — the
/// percent end is resolved against the lerped rect and then MIXED with the
/// other end by progress. Resolving it and returning it as-is keeps the
/// ghost on a big round corner while the target fades to a square, which is
/// what a user sees as "the corners no longer line up".

/// A RE-MEASURED end must not be scaled by the flight. The layout override is
/// written after layout, so `content_box()` always trails `lerped()` by one
/// poll; scaling by `l/box` therefore stretches the freshly re-flowed content
/// by the frame delta (measured 1.22-1.29x in the real app loop) instead of
/// leaving it re-laid-out — the exact opposite of what `RemeasureToBounds`
/// promises. The scale rule lives in `paint_scale`, which render calls, so
/// this pins the real path.

/// A writer must not clobber (or clear) an override that belongs to a
/// DIFFERENT flight. Flight ids are per-composer, so a peer flight with the
/// same number is normal, and R2's trace caught exactly that: a peer morph
/// with id 1 deleted the Tier-1 flight's override. The clear side is
/// namespaced by `FlightKey`; this pins the WRITE side.

/// Teardown must reach a node that LEFT the tree in the same frame: the tree
/// walk cannot see a ghost detached into the transition layer, so its override
/// used to survive its flight (R2-F4 — benign only because detached nodes are
/// never measured).
#[test]
fn teardown_reaches_a_slot_that_left_the_tree() {
    use crate::transition::{FlightMeasure, FlightMeasureFrame};
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
            Column::new()
                .modifier(Modifier::new().size(120.0, 60.0).background(Color::RED, Shape::rounded(8.0)))
                .build(ctx, |_| {});
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let root = composer.layout_root_idx().expect("root");
    let (parent, marked) = {
        let nodes = composer.arena_nodes();
        (root, nodes[root].children[0])
    };
    let key = composer.arena_nodes()[marked].slot_key;
    let owner = FlightKey { cid: composer.composer_id, id: 7 };
    composer.arena.nodes[marked].flight_measure = Some(FlightMeasure {
        frame: State::new(FlightMeasureFrame {
            content: Some(crate::unit::Size::new(200.0, 90.0)),
            reported: Some(crate::unit::Size::new(200.0, 90.0)),
        }),
        owner,
    });
    // Detach it from the tree the way a retarget does: the parent drops the
    // child and the node loses its parent link.
    composer.arena.nodes[parent].children.retain(|&c| c != marked);
    composer.arena.nodes[marked].parent_id = None;
    assert!(
        find_idx_by_slot(composer.arena_nodes(), root, key).is_none(),
        "the node is unreachable from the root now"
    );

    composer.clear_transition_for_slot(key, owner);
    assert!(
        composer.arena_nodes()[marked].flight_measure.is_none(),
        "the arena sweep drops the dead flight's override on a detached ghost"
    );
    assert!(
        composer.layout_dirty_keys.contains(&key),
        "…and seeds its slot so a returning node re-measures naturally"
    );
    crate::animation::clear_all_animations();
}

/// Compose's `CircleShape` IS `RoundedCornerShape(50)`, i.e. `Pill`: on a
/// NON-square box it paints the WHOLE box with percent-50 corners (a stadium).
/// winia drew a true circle — most of a wide box stayed unpainted, the clip
/// used an ellipse, and a Circle hero therefore popped circle→stadium the
/// moment a flight started.

/// Weighted-hero screens: the hero is the only flexible child, so its
/// allocation is what decides where the trailing sibling sits.
#[crate::composable]
fn weighted_hero_row(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    hero_h: f32,
    inner: f32,
    hero_weight: f32,
) {
    Row::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        let mut hero = Modifier::new()
            .height(hero_h)
            .background(Color::RED, Shape::rounded(8.0))
            .shared_bounds(
                scope.shared_content_state("hero"),
                VisibilityTransition::fade_in(TweenSpec::default()),
                VisibilityTransition::fade_out(TweenSpec::default()),
                BoundsTransform::default(),
                ResizeMode::RemeasureToBounds,
                PlaceHolderSize::AnimatedSize,
                PathMotion::Linear,
                0.0,
                true,
            );
        // FIXED width on one end, weighted on the other: that is what makes the
        // animated (reported) width differ from the weighted allocation.
        hero = if hero_weight > 0.0 {
            hero.layout_weight(hero_weight)
        } else {
            hero.width(120.0)
        };
        Column::new()
            .modifier(hero)
            .build(ctx, |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().size(inner, 24.0));
                ctx.end_node();
            });
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(40.0, 80.0));
        ctx.end_node();
    });
}

#[crate::composable]
fn weighted_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    weighted_hero_row(ctx, scope, 80.0, 100.0, 0.0);
}

#[crate::composable]
fn weighted_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    weighted_hero_row(ctx, scope, 160.0, 320.0, 1.0);
}

/// A `weight(1)` hero keeps its ALLOCATED slot while a flight reports a
/// placeholder size. The parent allocates the share; taking the placement from
/// the child's returned size let a weighted hero shrink to its natural width
/// mid-flight, so a trailing sibling slid over it and snapped back at the end.
#[test]
fn weighted_hero_keeps_its_allocation_while_flying() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                if s.get() {
                    weighted_list_screen(ctx, &scope);
                } else {
                    weighted_detail_screen(ctx, &scope);
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    let fid = *composer.shared_flights.keys().next().expect("flight id");
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .progress
        .set(0.5);
    frame(&mut composer);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

    let root = composer.layout_root_idx().expect("root");
    let (hero, trailing) = {
        let nodes = composer.arena_nodes();
        // The composable builds the Row at the top level, so the layout root
        // IS the row.
        let kids = nodes[root].children.clone();
        assert_eq!(kids.len(), 2, "the row keeps its two children");
        (kids[0], kids[1])
    };
    let nodes = composer.arena_nodes();
    assert!(
        (nodes[hero].measured_size.width - 360.0).abs() <= 1.0,
        "the weighted hero must keep its 360px allocation, got {}",
        nodes[hero].measured_size.width
    );
    assert!(
        (nodes[trailing].position.x - 360.0).abs() <= 1.0,
        "…so the trailing sibling stays at the row's edge, got {}",
        nodes[trailing].position.x
    );
    crate::animation::clear_all_animations();
}

/// A restored node must keep its CONTENT box. `place()` leaves
/// `measured_size` holding the placeholder size the parent was told, so a
/// rebuild that copies only that resurrects the placeholder as the node's own
/// box — the exact confusion the flight layout contract removed (review R3-F3).
#[test]
fn restored_node_keeps_its_content_box() {
    let mut node = crate::layout::node::LayoutNode::default();
    node.measured_size = crate::unit::Size::new(300.0, 200.0);
    node.flight_content_size = Some(crate::unit::Size::new(210.0, 130.0));
    assert_eq!(
        node.content_box(),
        crate::unit::Size::new(210.0, 130.0),
        "precondition: the content box wins over the placeholder size"
    );

    let mut restored = crate::layout::node::LayoutNode::default();
    restored.restore_layout(&node);
    assert_eq!(
        restored.content_box(),
        crate::unit::Size::new(210.0, 130.0),
        "the restore carries the content box"
    );
}

/// Endpoint exactness for the corner model across the shape pairs the review
/// named as untested: at p=0 the painted radius must equal the SOURCE's own
/// resolved radius, at p=1 the TARGET's, for percent->fixed, fixed->percent,
/// percent->percent and a per-corner shape (`TopRoundedRect`).


/// The Tier-0 writer must not hand a flight's visual (or its layout override) to
/// whatever node happens to sit at the frozen target slot. Slots are positional, so
/// a recomposition that shifts the target can leave an unrelated element there (R2
/// measured an unrelated 20x20 leaf laid out at the flight's reported size for one
/// pass). Here the slot's node loses its marker directly, which is the same
/// condition the writer must detect.

/// The morph detector must do BOTH things for a node whose size a flight owns:
/// skip the morph DECISION (otherwise a Tier-1 flight's own layout change opens a
/// phantom morph on the peer every frame) AND still keep the baseline current
/// (otherwise the landing compares the hero against where it took off from, opens
/// a fresh morph, and it flies the whole path again — the replay the reporter saw
/// in the demo).
///
/// This constructs the freeze condition directly instead of re-creating the real
/// flight timing (a flight that resolves in the frame it opens, with the flights
/// polled before the morph poll): an endpoint node with the override attached and
/// a changed layout. Both halves are discriminating — an early `continue` fails the
/// baseline half, dropping the guard fails the no-phantom-flight half.

/// A Tier-1 flight whose SOURCE is the peer (overlay -> main: the "hero returns"
/// direction) must take the surviving MAIN node's layout override with it when it is
/// cancelled. `write_cross_visuals` stamps the override with `all[0].composer_id`, so
/// the teardown has to clear with that same key: using `a.source_cid` looked for a key
/// nobody wrote, and the main node stayed frozen at the outgoing size forever — with
/// `flight_measure` attached, which also suppresses that node's morph detection for
/// good (review round 3, ported from the reviewer's reproducer, pre-fix verified).
#[test]
fn peer_sourced_tier1_cancel_drops_the_mains_override() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let (mut a, mut b) = (Composer::new(), Composer::new());
    let scope = SharedTransitionScope::new(82);
    let show_a = State::new(false); // main
    let show_b = State::new(true); // overlay: the SOURCE side
    let main_marked = State::new(true);
    let frame = |a: &mut Composer, b: &mut Composer| {
        let (sa, sb, sca, scb) = (show_a.clone(), show_b.clone(), scope.clone(), scope.clone());
        let mm = main_marked.clone();
        cross_frame(
            a,
            b,
            |ctx| {
                shell(ctx, |ctx| {
                    if sa.get() {
                        hero_leaf_marked(ctx, 300.0, 160.0, Color::BLUE, &sca, mm.get());
                    }
                })
            },
            |ctx| {
                shell(ctx, |ctx| {
                    if sb.get() {
                        hero_leaf_marked(ctx, 120.0, 80.0, Color::RED, &scb, true);
                    }
                })
            },
        );
    };
    frame(&mut a, &mut b);
    show_a.set(true);
    show_b.set(false);
    frame(&mut a, &mut b);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 flight opens");
    let fid = *a.shared_flights.keys().next().expect("flight");
    assert_eq!(
        a.shared_flights[&fid].source_cid, b.composer_id,
        "precondition: the source is the PEER composer"
    );
    assert_eq!(
        a.shared_flights[&fid].target_cid, a.composer_id,
        "precondition: the target is MAIN"
    );
    let tidx = marked_in(&a)[0];
    crate::animation::update_animations();
    frame(&mut a, &mut b);
    assert!(
        a.arena_nodes()[tidx].flight_measure.is_some(),
        "main carries the override while the flight runs"
    );

    // Cancel it: the main marker disappears, so the flight goes stale.
    let node_before = a.arena_nodes()[tidx].id;
    main_marked.set(false);
    crate::animation::update_animations();
    frame(&mut a, &mut b);
    assert!(a.shared_flights.is_empty(), "the stale flight is cancelled");
    assert_eq!(
        a.arena_nodes()[tidx].id, node_before,
        "the main node survived the marker drop (so it is the SAME node that leaked)"
    );
    for _ in 0..3 {
        crate::animation::update_animations();
        frame(&mut a, &mut b);
    }
    assert!(
        a.arena_nodes()[tidx].flight_measure.is_none(),
        "the cancelled peer-sourced flight must take its layout override with it"
    );
    crate::animation::clear_all_animations();
}

/// A bounds hero holding a child WIDER than its box, so the scaled content spills
/// past the lerped rect unless the marker's `overlayClip` clips it.
#[crate::composable]
fn spilling_hero_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    clip: OverlayClip,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        Column::new()
            .modifier(
                Modifier::new()
                    .size(w, h)
                    .background(Color::RED, Shape::rounded(8.0))
                    .shared_bounds_with_overlay_clip(
                        scope.shared_content_state("hero"),
                        VisibilityTransition::fade_in(TweenSpec::default()),
                        VisibilityTransition::fade_out(TweenSpec::default()),
                        BoundsTransform::default(),
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                        PathMotion::Linear,
                        0.0,
                        true,
                        clip,
                    ),
            )
            .build(ctx, |ctx| {
                // 500 wide inside a 120-wide hero: spills by construction. `required_size`, not
                // `size`: Compose coerces a plain `size()` into the incoming range
                // (`enforceIncoming = true`), so 500 would come out at the hero's 120 and there
                // would be nothing to spill; `requiredSize` is the deliberate-overflow escape.
                let key = ctx.next_key();
                ctx.start_leaf(
                    key,
                    Modifier::new()
                        .required_size(500.0, 30.0)
                        .background(Color::GREEN, Shape::Rectangle),
                );
                ctx.end_node();
            });
    });
}

#[crate::composable]
fn spilling_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, clip: OverlayClip) {
    spilling_hero_screen(ctx, scope, 120.0, 60.0, clip);
}

#[crate::composable]
fn spilling_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, clip: OverlayClip) {
    spilling_hero_screen(ctx, scope, 300.0, 200.0, clip);
}

/// Compose `overlayClip`: the default (`Bounds`) clips the overlay-rendered pair to
/// the flight's own corner quad, `None` is the escape hatch that lets content
/// overflow. Both directions are asserted, because the probe is only meaningful if
/// the spill really is visible without the clip — the same control that showed the
/// deleted `clip` flag was dead (the render clipped unconditionally then, so the
/// spill could never be seen at all).
#[test]
fn overlay_clip_none_lets_content_overflow_the_lerped_rect() {
    let _g = lock_serial();
    let sample = |clip: OverlayClip| {
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let frame = |composer: &mut Composer| {
            let s = show.clone();
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    if s.get() {
                        spilling_list_screen(ctx, &scope, clip);
                    } else {
                        spilling_detail_screen(ctx, &scope, clip);
                    }
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        frame(&mut composer);
        show.set(false);
        frame(&mut composer);
        let fid = *composer.shared_flights.keys().next().expect("flight id");
        composer
            .shared_flights
            .get_mut(&fid)
            .expect("flight")
            .progress
            .set(0.5);
        frame(&mut composer);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let mut surface = render_heads(&composer);
        // The lerped rect is 210x130; the 500-wide band is scaled by 210/300 = 0.7, so
        // it reaches x=350 at the TOP of the hero — well outside both the rect and the
        // hero's own background.
        let out = pixel_rgb(&mut surface, 260, 10);
        crate::animation::clear_all_animations();
        out
    };
    let unclipped = sample(OverlayClip::None);
    let clipped = sample(OverlayClip::Bounds);
    assert!(
        !close_enough(unclipped, (255, 255, 255), 12),
        "with `overlayClip: None` the spill must be visible at x=260 (got {unclipped:?})"
    );
    assert!(
        close_enough(clipped, (255, 255, 255), 12),
        "the default `overlayClip` must clip the pair to the lerped rect (got {clipped:?})"
    );
}

/// The assumption `radii_pairs`' cap rests on, measured instead of assumed (risk 18's
/// open item): Skia SILENTLY clamps a rounded-rect radius to half the shorter side, so
/// capping at `min(lerped)/2` matches the geometry of the rect the corner is drawn into
/// rather than over-shrinking it — which is what the review question was.

/// Screens for the "parent must see the animated size on the SWITCH frame" test: a
/// marked hero with a probe leaf BELOW it, whose y position is the observable — the
/// parent's idea of the hero's height is whatever the leaf is placed after.
#[crate::composable]
fn switch_frame_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    h: f32,
) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        Column::new()
            .modifier(
                Modifier::new()
                    .size(200.0, h)
                    .background(Color::RED, Shape::Rectangle)
                    .shared_bounds(
                        scope.shared_content_state("hero"),
                        VisibilityTransition::fade_in(TweenSpec::default()),
                        VisibilityTransition::fade_out(TweenSpec::default()),
                        BoundsTransform::default(),
                        ResizeMode::scale_to_bounds(),
                        // The follow-the-flight policy: the parent is told the ANIMATED
                        // size, which is what makes the stale-layout frame visible.
                        PlaceHolderSize::AnimatedSize,
                        PathMotion::Linear,
                        0.0,
                        true,
                    ),
            )
            .build(ctx, |_| {});
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(40.0, 20.0));
        ctx.end_node();
    });
}

#[crate::composable]
fn switch_frame_list(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    switch_frame_screen(ctx, scope, 80.0);
}

#[crate::composable]
fn switch_frame_detail(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    switch_frame_screen(ctx, scope, 240.0);
}

/// On the frame a flight FIRST attaches its layout override, the entering end's parent
/// must already see the ANIMATED size — not the target's natural one. The override can
/// only be attached by the post-layout poll (resolving needs the target's measured rect),
/// so without the app loop's extra pass the parent below the hero is placed for the
/// 240px detail hero for exactly one frame and then snaps up to the 80px source size:
/// the dip-then-snap reported from `shared_transition_image_demo`.
///
/// The observable is the probe leaf's y: 80 (animated, source) vs 240 (natural, target).

/// The path `collect_node_keys` exists for: a SECOND compose inside one frame, across a
/// branch flip. Its precondition is a node listed TWICE under a parent reachable from the
/// root — the walk then visits that node twice and panics with `[dup-key]` (both printed
/// indices equal, the tell), and if the second listing is the one that survives, the node is
/// never measured (0x0, drawing nothing — the "back is the image disappearing" report).
///
/// The precondition is CONSTRUCTED here, the way `teardown_reaches_a_slot_that_left_the_tree`
/// constructs its detached node: the demo produces it through materialize's multi-path tree
/// build.
///
/// HONESTY, and this matters more than the test: it does NOT fail with the prune disabled
/// (measured). The reason is now understood too — the compose that follows rebuilds the
/// parent's `children`, so an injected duplicate is cleared before `collect_node_keys` runs.
/// Six attempts at a headless reproducer all came back green, so this is an invariant guard —
/// "the hero ends up listed exactly once and measured" — and the fix rests on the debug-server
/// measurement (3 round trips, `[dup-key]` 90 -> 0, hero back at (16,57) 96x96).
#[test]
fn a_duplicate_child_listing_is_pruned_before_the_key_walk() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer, twice: bool| {
        let s = show.clone();
        let mut once = |composer: &mut Composer| {
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    if s.get() {
                        switch_frame_list(ctx, &scope);
                    } else {
                        switch_frame_detail(ctx, &scope);
                    }
                });
            });
        };
        once(composer);
        if twice {
            once(composer);
        }
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer, false);
    show.set(false);
    // Let the switch frame run, so the flight is live and the hero has a `transition`.
    frame(&mut composer, false);

    // Construct the precondition: a reachable parent lists the hero twice. The hero is the
    // flight's endpoint, which is the robust way to name it here — its `measured_size` is the
    // ANIMATED size while it flies, so matching a literal width would miss it.
    let hero = composer
        .arena_nodes()
        .iter()
        .position(|n| {
            n.transition
                .as_ref()
                .is_some_and(|t| matches!(t.role, TransitionRole::Target))
        })
        .expect("the flying hero node (the entering end)");
    // The parent is the node that LISTS the hero, not the one `parent_id` names: measured,
    // the flight endpoint's `parent_id` can point at an arena node that no longer exists.
    let parent = composer
        .arena_nodes()
        .iter()
        .position(|n| n.children.contains(&hero))
        .expect("the node that lists the hero");
    composer.arena.nodes[parent].children.push(hero);

    // Second compose in the same frame: this is where `collect_node_keys` runs.
    frame(&mut composer, true);

    // Re-locate the hero and its parent by ID: the frame may have replaced nodes, so the
    // indices captured above are only good for building the precondition.
    let hero_now = composer.arena_nodes().iter().position(|n| {
        n.transition
            .as_ref()
            .is_some_and(|t| matches!(t.role, TransitionRole::Target))
            && n.measured_size.width > 1.0
    });
    let hero_now = hero_now.expect("the hero is still present and measured");
    let pid = composer.arena_nodes()[hero_now]
        .parent_id
        .expect("the hero still has a parent");
    let listings = composer
        .arena_nodes()
        .iter()
        .map(|n| n.children.iter().filter(|&&c| c == hero_now).count())
        .sum::<usize>();
    assert_eq!(
        listings, 1,
        "the hero must be listed exactly once in the arena (parent id {pid})"
    );
    crate::animation::clear_all_animations();
}

/// HONESTY: this does NOT fail with the pruning reverted (measured) — five attempts at a
/// headless reproducer for that defect all came back green, so it is an end-state invariant
/// guard, not the regression test for it. The defect needs a stale listing from the
/// multi-path tree build, which the demo reproduces and a hand-written test has not; the
/// fix rests on the debug-server measurement (3 round trips, `[dup-key]` 90 -> 0).

/// Bouncy hero leaf (spring overshoot must render past the end rect).
fn spring_hero_leaf(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
    path: PathMotion,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(
                scope.shared_content_state("hero"),
                BoundsTransform::spring(SpringSpec::bouncy()),
                PlaceHolderSize::JumpCut,
                path,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

#[crate::composable]
fn spring_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, path: PathMotion) {
    spring_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, path);
}

#[crate::composable]
fn spring_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, path: PathMotion) {
    gap_leaf(ctx, 400.0, 100.0);
    spring_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, path);
}

/// App-loop step for the bouncy screens.
fn spring_frame(composer: &mut Composer, show: &State<bool>, path: PathMotion) {
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if show.get() {
                    spring_list_screen(ctx, &scope, path);
                } else {
                    spring_detail_screen(ctx, &scope, path);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn spring_advance(composer: &mut Composer, show: &State<bool>, path: PathMotion) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    spring_frame(composer, show, path);
}


/// Small arc hero (40px): the bend (≈43px) exceeds the half-size, so
/// edge probes discriminate arc paint from straight paint.
fn arc_hero_leaf(ctx: &mut ComposeCtx, w: f32, h: f32, color: Color, scope: &SharedTransitionScope) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(
                scope.shared_content_state("hero"),
                BoundsTransform::spring(SpringSpec::bouncy()),
                PlaceHolderSize::JumpCut,
                PathMotion::ArcBelow,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

#[crate::composable]
fn arc_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    Column::new().build(ctx, |ctx| {
        gap_leaf(ctx, 400.0, 40.0);
        Row::new().build(ctx, |ctx| {
            arc_hero_leaf(ctx, 40.0, 40.0, Color::RED, scope);
            gap_leaf(ctx, 260.0, 40.0);
        });
    });
}

#[crate::composable]
fn arc_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    Column::new().build(ctx, |ctx| {
        gap_leaf(ctx, 400.0, 140.0);
        Row::new().build(ctx, |ctx| {
            gap_leaf(ctx, 260.0, 40.0);
            arc_hero_leaf(ctx, 40.0, 40.0, Color::BLUE, scope);
        });
    });
}

fn arc_frame(composer: &mut Composer, show: &State<bool>) {
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if show.get() {
                    arc_list_screen(ctx, &scope);
                } else {
                    arc_detail_screen(ctx, &scope);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn arc_advance(composer: &mut Composer, show: &State<bool>) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    arc_frame(composer, show);
}


/// The scene registry must not grow forever: entries whose scene is no longer tagged in the tree are
/// dropped, and a live one is kept. Without this, every scene a display has ever visited kept an entry
/// (and its `Arc` visibility closure) for the life of the process — `clear_nav_scenes` is test-only.

/// The idle early-out must still clear a stale disposition: `paint_dirty` is what guarantees one more
/// pass after the last non-`InTree` frame, and that pass is what returns the node to `InTree`.
///
/// Teeth: make the early-out ignore `paint_dirty` and the stale placeholder survives the poll.
#[test]
fn the_idle_early_out_still_clears_a_stale_disposition() {
    let _g = lock_serial();
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |_| {});
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    let idx = composer.layout_root_idx().expect("root");

    // Simulate the frame right after a flight ended: the node is still marked, nothing is flying.
    composer.arena.nodes[idx].paint = PaintDisposition::Placeholder;
    composer.paint_dirty = true;
    composer.poll_shared_flights();
    assert_eq!(
        composer.arena.nodes[idx].paint,
        PaintDisposition::InTree,
        "the pass must run once more to clear the stale disposition"
    );
    assert!(
        !composer.paint_dirty,
        "…and then nothing is left to clear, so the next idle frame may early-out"
    );

    // A second idle frame now takes the early-out and leaves it alone.
    composer.poll_shared_flights();
    assert_eq!(composer.arena.nodes[idx].paint, PaintDisposition::InTree);
    crate::animation::clear_all_animations();
}

/// The registry is SHARED by every composer on the thread, while an arena belongs to one of them, so
/// the prune must use the union of their live scene tags: a peer with no scene host must not delete the
/// entry of the composer that has one. Pruning from a single composer's own arena (the first version)
/// did exactly that, and the pairing then fell back to last-wins for the rest of the transition — the
/// "paired the leaving end with itself / never grew" failure this branch exists to fix.
///
/// Teeth: give the peer its own flight (so its per-composer pass really runs) and prune from that
/// composer's arena — the live scene disappears and this fails.

/// The per-frame scope-activity flags are a UNION over the frame's composers, not whoever polled last.
///
/// Teeth: without the union sync on the cross poll's idle path this fails — the app polls the main
/// composer first and overlays after, and each per-composer sync published only its own set, so the
/// peer's poll cleared the main scope's flag and `is_transition_active()` (and the morph gate's
/// fallback) read "idle" during a Tier-0 transition whenever any overlay existed.

/// A `Placeholder` — a marked copy of a key a running flight already owns, which is NOT painted —
/// must not be hit either. Otherwise the invisible duplicate that the demo measured (a second 96x96
/// hero at its own rect) keeps receiving taps and firing its handlers.
///
/// This pins the hit walk's RULE, using the same transient state the coordinator writes each frame.
/// Producing a placeholder through a scene host is the nav flow (two composed scenes), which the demo
/// covers end to end; composing two same-key copies here does not reproduce it (measured: they collapse
/// into one node).
///
/// Teeth: remove the `PaintDisposition::Placeholder` early return in `hit_test_recursive` and the tap
/// below reaches the placeholder again.


// ── Phase 3 tests ──

use crate::layout::node::{hit_test, hit_test_with_flights};

/// Marked box with layout-driven width (registers a LAYOUT dep only —
/// size changes remeasure without recomposition, mirroring the app loop).
fn morph_leaf(ctx: &mut ComposeCtx, w: &State<f32>, scope: &SharedTransitionScope) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, 80.0)
            .background(Color::GREEN, Shape::Rectangle)
            .shared_element(scope.shared_content_state("morph"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
    );
    ctx.end_node();
}

#[crate::composable]
fn morph_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
    morph_leaf(ctx, w, scope);
}

fn morph_compose(composer: &mut Composer, w: &State<f32>) {
    let ww = w.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                morph_screen(ctx, &scope, &ww);
            });
        });
    });
}

/// Layout-only advance (no compose — mirrors the app loop when no compose
/// deps fire; proves morphs need zero recomposition).
fn advance_layout_only(composer: &mut Composer) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}


/// Marked container with a plain child (container morphs must keep
/// their subtree hittable mid-flight — MAJOR #3).
#[crate::composable]
fn morph_parent_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, 80.0)
                .background(Color::GREEN, Shape::Rectangle)
                .shared_element(scope.shared_content_state("box"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
        )
        .build(ctx, |ctx| {
            gap_leaf(ctx, 50.0, 50.0);
        });
}

fn morph_parent_compose(composer: &mut Composer, w: &State<f32>) {
    let ww = w.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                morph_parent_screen(ctx, &scope, &ww);
            });
        });
    });
}


/// Keyed hero leaf (morph baselines key on (scope, key) — MINOR #5).
fn keyed_hero_leaf(
    ctx: &mut ComposeCtx,
    key: &str,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
) {
    let slot = ctx.next_key();
    ctx.start_leaf(
        slot,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(scope.shared_content_state(key), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
    );
    ctx.end_node();
}

/// Data-driven key at ONE statement (same slot, different marker key —
/// morph baselines must key on (scope, key), MINOR #5).
#[crate::composable]
fn keyed_item_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, use_a: &State<bool>) {
    let (k, w, h, c) = if use_a.get() {
        ("ka", 120.0, 80.0, Color::RED)
    } else {
        ("kb", 300.0, 160.0, Color::BLUE)
    };
    keyed_hero_leaf(ctx, k, w, h, c, scope);
}

fn keyed_frame(composer: &mut Composer, show: &State<bool>) {
    let s = show.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                keyed_item_screen(ctx, &scope, &s);
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}


/// z-ordered hero leaf (retained ghosts sort back-to-front by this).
fn z_hero_leaf(
    ctx: &mut ComposeCtx,
    key: &str,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
    z: f32,
) {
    let slot = ctx.next_key();
    ctx.start_leaf(
        slot,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(
                scope.shared_content_state(key),
                BoundsTransform::default(),
                PlaceHolderSize::JumpCut,
                PathMotion::Linear,
                z,
                true,
            ),
    );
    ctx.end_node();
}

#[crate::composable]
fn z_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    // k1 carries the higher z but composes FIRST — detach order must
    // not decide paint order.
    z_hero_leaf(ctx, "k1", 120.0, 80.0, Color::RED, scope, 1.0);
    z_hero_leaf(ctx, "k2", 120.0, 80.0, Color::GREEN, scope, 0.0);
}

#[crate::composable]
fn z_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    z_hero_leaf(ctx, "k1", 300.0, 160.0, Color::BLUE, scope, 1.0);
    z_hero_leaf(ctx, "k2", 300.0, 160.0, Color::BLUE, scope, 0.0);
}

fn z_frame(composer: &mut Composer, show: &State<bool>) {
    let s = show.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if s.get() {
                    z_list_screen(ctx, &scope);
                } else {
                    z_detail_screen(ctx, &scope);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

#[test]
fn retained_ghosts_sort_back_to_front_by_z() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    z_frame(&mut composer, &show);
    z_frame(&mut composer, &show);
    show.set(false);
    z_frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 2, "one flight per key");
    assert_eq!(composer.transition_layer.len(), 2, "both sources retained");

    // Key per retained slot, from the flights (detach order is a
    // HashMap iteration — assert order, not identity sequence).
    let key_of: HashMap<u64, String> = composer
        .shared_flights
        .values()
        .filter_map(|a| a.flight.source_slot.map(|s| (s, a.flight.key.clone())))
        .collect();
    let ordered: Vec<&str> = composer.transition_layer
        .iter()
        .map(|&idx| {
            key_of
                .get(&composer.arena_nodes()[idx].slot_key)
                .map(|s| s.as_str())
                .unwrap_or("?")
        })
        .collect();
    assert_eq!(ordered, vec!["k2", "k1"], "ascending z paints k1 on top, got {ordered:?}");

    // The sort reads z lazily off the retained markers.
    for &idx in &composer.transition_layer {
        let z = find_shared_marker(&composer.arena_nodes()[idx].modifier)
            .expect("retained marker")
            .z_index;
        let key = &key_of[&composer.arena_nodes()[idx].slot_key];
        assert_eq!(z, if key == "k1" { 1.0 } else { 0.0 }, "z rides the retained node");
    }
    crate::animation::clear_all_animations();
}

/// sharedBounds hero (tween flight; enter slides from above, exit
/// slides downward — both faded, Compose container-transform shape).
/// Slide distance 160px: at p≈0.5 the ends sit ±80px off the lerped
/// center (onscreen, non-overlapping — decisive probes).
fn bounds_hero_leaf(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_bounds(
                scope.shared_content_state("hero"),
                VisibilityTransition::slide_in_offset(
                    SlideDirection::Up,
                    SlideOffset::Fixed(160.0),
                    TweenSpec::default(),
                )
                .with_fade(),
                VisibilityTransition::slide_out_offset(
                    SlideDirection::Down,
                    SlideOffset::Fixed(160.0),
                    TweenSpec::default(),
                )
                .with_fade(),
                BoundsTransform::default(),
                ResizeMode::scale_to_bounds(),
                PlaceHolderSize::JumpCut,
                PathMotion::Linear,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

#[crate::composable]
fn bounds_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    bounds_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
}

#[crate::composable]
fn bounds_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    gap_leaf(ctx, 400.0, 100.0);
    bounds_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
}

fn bounds_frame(composer: &mut Composer, show: &State<bool>) {
    let s = show.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if s.get() {
                    bounds_list_screen(ctx, &scope);
                } else {
                    bounds_detail_screen(ctx, &scope);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn bounds_advance(composer: &mut Composer, show: &State<bool>) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    bounds_frame(composer, show);
}


/// sharedBounds hero with expand enter/exit (wipe-in from the top
/// edge, wipe-out toward it — both faded).
fn expand_hero_leaf(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_bounds(
                scope.shared_content_state("hero"),
                VisibilityTransition::expand_in(TweenSpec::default()).with_fade(),
                VisibilityTransition::shrink_out(TweenSpec::default()).with_fade(),
                BoundsTransform::default(),
                ResizeMode::scale_to_bounds(),
                PlaceHolderSize::JumpCut,
                PathMotion::Linear,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

#[crate::composable]
fn expand_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    expand_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
}

#[crate::composable]
fn expand_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    gap_leaf(ctx, 400.0, 100.0);
    expand_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
}

fn expand_frame(composer: &mut Composer, show: &State<bool>) {
    let s = show.clone();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if s.get() {
                    expand_list_screen(ctx, &scope);
                } else {
                    expand_detail_screen(ctx, &scope);
                }
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn expand_advance(composer: &mut Composer, show: &State<bool>) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    expand_frame(composer, show);
}


/// Morph box persistent across a screen switch + disappearing/appearing
/// hero pair: a morph flight and a switch flight coexist, then both
/// complete cleanly.
#[crate::composable]
fn mixed_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
    morph_leaf(ctx, w, scope);
    hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, true);
}

#[crate::composable]
fn mixed_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
    morph_leaf(ctx, w, scope);
    gap_leaf(ctx, 400.0, 100.0);
    hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
}


// ── Phase 4 Tier1 tests (two-composer harness: A = main, B = overlay) ──

/// Column shell with caller content (keeps slot paths aligned frames).
fn shell(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, content);
}

/// Two-composer app-loop mirror (same canvas): compose + layout + polls,
/// then cross-poll. Contents are plain closures with no same-position
/// swaps within one composer (test-fallback keys stay sound).
fn cross_frame(
    a: &mut Composer,
    b: &mut Composer,
    ca: impl FnOnce(&mut ComposeCtx),
    cb: impl FnOnce(&mut ComposeCtx),
) {
    a.compose(ca);
    b.compose(cb);
    a.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    a.poll_shared_flights();
    b.poll_shared_flights();
    let mut all: Vec<&mut Composer> = vec![a, b];
    Composer::poll_cross_flights(&mut all);
}

/// List/detail pair across composers sharing one scope handle (production:
/// the overlay inherits the scope via the CompositionLocal snapshot).
fn xframe(
    a: &mut Composer,
    b: &mut Composer,
    show_a: &State<bool>,
    show_b: &State<bool>,
    scope: &SharedTransitionScope,
) {
    let (sa, sb) = (show_a.clone(), show_b.clone());
    let (sca, scb) = (scope.clone(), scope.clone());
    cross_frame(
        a,
        b,
        |ctx| shell(ctx, |ctx| {
            if sa.get() {
                hero_leaf(ctx, 120.0, 80.0, Color::RED, &sca, true);
            }
        }),
        |ctx| shell(ctx, |ctx| {
            if sb.get() {
                hero_leaf(ctx, 300.0, 160.0, Color::BLUE, &scb, true);
            }
        }),
    );
}

fn xadvance(
    a: &mut Composer,
    b: &mut Composer,
    show_a: &State<bool>,
    show_b: &State<bool>,
    scope: &SharedTransitionScope,
) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    xframe(a, b, show_a, show_b, scope);
}

/// Render both arenas onto one surface (mirrors app: main pass, then the
/// overlay pass translated by its origin).
fn render_cross(a: &Composer, b: &Composer, b_origin: (f32, f32)) -> skia_safe::Surface {
    let mut surface = skia_safe::surfaces::raster_n32_premul((400, 400)).expect("raster surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    let an = a.arena_nodes();
    if let Some(root) = a.layout_root_idx() {
        crate::render::render(an, root, surface.canvas());
        a.render_layer(surface.canvas());
    }
    let bn = b.arena_nodes();
    if let Some(root) = b.layout_root_idx() {
        surface.canvas().save();
        surface.canvas().translate((b_origin.0, b_origin.1));
        crate::render::render(bn, root, surface.canvas());
        b.render_layer(surface.canvas());
        surface.canvas().restore();
    }
    surface
}

fn marked_in(composer: &Composer) -> Vec<usize> {
    let mut out = Vec::new();
    if let Some(root) = composer.layout_root_idx() {
        let nodes = composer.arena_nodes();
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if find_shared_marker(&nodes[idx].modifier).is_some() {
                out.push(idx);
            }
            stack.extend(nodes[idx].children.iter().copied());
        }
    }
    out
}


/// Keyed hero leaf (multi-pair flights).
fn hero_keyed_leaf(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
    key: &str,
) {
    let k = ctx.next_key();
    ctx.start_leaf(
        k,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::Rectangle)
            .shared_element(scope.shared_content_state(key), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
    );
    ctx.end_node();
}


#[crate::composable]
fn b_plain(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
}

#[crate::composable]
fn b_shifted(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    gap_leaf(ctx, 400.0, 100.0);
    hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
}


// ── Nav scene host (scaffolding for the scene-host test below) ─────────────

/// Routes for the nav scene-host test.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
enum NavRoute {
    List,
    Detail,
}

/// The hero both nav scenes mark with the SAME key (see `hero_leaf`).
fn nav_hero_leaf(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    scope: &SharedTransitionScope,
) {
    let key = ctx.next_key();
    ctx.start_leaf(
        key,
        Modifier::new()
            .size(w, h)
            .background(color, Shape::rounded(8.0))
            .shared_element(
                scope.shared_content_state("hero"),
                BoundsTransform::default(),
                PlaceHolderSize::JumpCut,
                PathMotion::Linear,
                0.0,
                true,
            ),
    );
    ctx.end_node();
}

/// List: the hero is the only leaf, at the canvas origin (0,0)-(120,80).
#[crate::composable]
fn nav_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            nav_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
        });
}

/// Detail: the hero is pushed DOWN (240px of nothing above it) and is bigger, so a flight between
/// the two ends is a long diagonal move that leaves the list rect early.
#[crate::composable]
fn nav_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            gap_leaf(ctx, 400.0, 240.0);
            nav_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
        });
}

/// One app-loop step with REAL scene hosts: one `SharedTransitionLayout` (the scope) wrapping a
/// `Stack` that holds the `NavDisplay` under test and a PEER `NavDisplay` (see the test doc).
fn nav_frame(
    composer: &mut Composer,
    main: &NavBackStack<NavRoute>,
    peer: &NavBackStack<NavRoute>,
) {
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("inside SharedTransitionLayout");
            Stack::new()
                .modifier(Modifier::new().fill_max_size())
                .build(ctx, |ctx| {
                    // The scene host under test — the demo's shape: while a nav transition runs it
                    // composes the outgoing AND the incoming scene at once.
                    NavDisplay::new(main, move |_ctx, key: &NavRoute| {
                        let scope = scope.clone();
                        match key {
                            NavRoute::List => NavEntry::new(key.clone(), move |ctx, _| {
                                nav_list_screen(ctx, &scope)
                            }),
                            NavRoute::Detail => NavEntry::new(key.clone(), move |ctx, _| {
                                nav_detail_screen(ctx, &scope)
                            }),
                        }
                    })
                    .build(ctx);
                    // A second host on the same screen showing the SAME route, so its scene key
                    // equals the display's LEAVING scene key while it plays the other role. It shares
                    // nothing (it marks no key and paints nothing), so nothing but the scene ids can
                    // observe it — which is exactly the point (see the test doc). Composed AFTER the
                    // display above on purpose: its publication must be the later one.
                    NavDisplay::new(peer, |_ctx, key: &NavRoute| {
                        NavEntry::new(key.clone(), |_ctx, _| {})
                    })
                    .build(ctx);
                });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
}

fn nav_advance(
    composer: &mut Composer,
    main: &NavBackStack<NavRoute>,
    peer: &NavBackStack<NavRoute>,
) {
    crate::animation::update_animations();
    std::thread::sleep(std::time::Duration::from_millis(16));
    nav_frame(composer, main, peer);
}

/// Arena indices of the marked nodes playing `role` in a flight.
fn nav_ends(composer: &Composer, role: TransitionRole) -> Vec<usize> {
    composer
        .arena_nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| n.transition.as_ref().is_some_and(|t| t.role == role))
        .map(|(idx, _)| idx)
        .collect()
}

/// Arena indices of every node whose paint disposition is `Placeholder` — the copies a running
/// flight owns but is not an end of, which must not paint.
fn nav_placeholders(composer: &Composer) -> Vec<usize> {
    composer
        .arena_nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| n.paint_disposition() == PaintDisposition::Placeholder)
        .map(|(idx, _)| idx)
        .collect()
}

/// `(scene id, is the scene the LEAVING one)` for a node, read from the `SceneTag` in its ancestry.
fn nav_scene_of(composer: &Composer, idx: usize) -> (u64, Option<bool>) {
    let id = scene_of_node(composer.arena_nodes(), idx)
        .expect("a marked node inside a scene host carries a SceneTag");
    (id, nav_scene_is_prev(id))
}

/// The pairing rule of `shared_live_map`, end to end through a real scene host.
///
/// A faithful scene host composes BOTH scenes at once during a transition — the outgoing and the
/// incoming one — which is what `examples/nav_shared_element_demo.rs` does and what no other test
/// in this module did: every other test switches one screen inside one composer, so a key has
/// exactly one live end, and neither the pairing rule (which of the TWO live ends the flight
/// takes) nor [`PaintDisposition::Placeholder`] (the copy the flight does NOT own) is exercised.
///
/// Scaffolding: `SharedTransitionLayout` wrapping a `NavDisplay` over a `NavBackStack` — the
/// demo's shape — with a list and a detail route that both mark a small leaf with the SAME key,
/// driven frame by frame (`compose` + `layout` + `poll_shared_flights`), then a List → Detail push.
///
/// A SECOND `NavDisplay` rides along on the same screen publishing the SAME route, and it is not
/// decoration: a published scene id must name the LAYER, not the scene. Both hosts' List layers
/// carry the same scene key, so an id derived from the scene key alone collides, the later
/// publisher (the peer) wins, and the leaving end below is then classified with the peer's role —
/// `is_prev == false` instead of `true`. That is the only way this test can reach the collision:
/// within ONE host the two layers always carry different scene keys, because a scene-key change is
/// what starts the transition in the first place.
///
/// What this covers:
/// (a) exactly ONE flight for the key (two live ends, one flight), starting at the leaving hero's
///     rect and landing on the ENTERING hero — the surviving end, not the more visible one (the
///     leaving scene starts at visibility 1, the entering one at 0);
/// (b) exactly one marked node is a `Placeholder`: the leaving scene's re-composed copy of the
///     flying key, which is neither the flight's source nor its target;
/// (c) that placeholder paints nothing — its own rect keeps the cleared background in a raster
///     probe, while the flight's lerped rect does paint (so the probe cannot pass trivially);
/// (d) the two ends publish DIFFERENT scene tags, each keeps its role, and both ids are stable
///     across two consecutive frames.
///
/// Teeth (each verified by temporarily breaking the production code, then restoring it):
/// - prefer the LEAVING end in `shared_live_map`: the flight lands on the 120x80 leaving copy
///   instead of the detail hero and the placeholder becomes the ENTERING scene's copy (the
///   "paired the leaving end with itself" failure this branch exists to fix);
/// - `let placeholder = false;` in `refresh_paint_dispositions`: no placeholder exists at all, and
///   the raster probe reads the leaving copy's faded red (measured (255,163,163)) where the rect
///   must stay background;
/// - derive the scene id from the scene key alone (`hash(scene_key)`): the peer host's
///   publication overwrites the leaving layer's registry entry, the leaving end reports
///   `is_prev == false`, and the pairing breaks exactly as in the first item.
///
/// What it does NOT cover (see `docs/nav-shared-transition.md`): the pop direction, the two-pane
/// `ListDetailStrategy`, and the `SharedEntryInSceneDecorator`. The flight's own geometry is only
/// asserted at its two ends and via the raster probe, not frame by frame against wall-clock time.

mod chrome;
mod escape;
mod measure;
mod opt_out;
mod paint;
mod scroll;
mod flight;
mod hit;
mod scenes;
mod shape;
mod tier1;
