//! Navigation × shared elements: can a nav transition fly a marked hero?
//!
//! Two routes (`List` → `Detail`) each mark the SAME shared key on a hero card, and the whole
//! `NavDisplay` sits inside one `SharedTransitionLayout`. Compose's bridge for this is a
//! `NavEntryDecorator` plus the `AnimatedContentScope` that `NavDisplay` uses internally — see
//! `docs/nav-shared-transition.md` for the sources and for what winia has and lacks.
//!
//! Run with the debug feature to drive it:
//! `cargo run -p winia --example nav_shared_element_demo --features debug-server`
//! then `c <x> <y>` on "Open detail" (list) and "Back" (detail).

use letclone::clone;
use winia::animation::{SpringSpec, TweenSpec};
use winia::nav::{
    ListDetailStrategy, NavBackStack, NavDisplay, NavEntry, NavTransitionSpec, SceneStrategy,
};
use winia::prelude::*;

/// Type-safe routes (compares to Nav3's `NavKey` with `@Serializable`; winia needs no serialization).
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
enum Route {
    List,
    Detail,
}

/// The hero card: a container that draws nothing itself and fills with a coloured child, so a
/// lost child is visible as an empty hole (the shape that made the transparent-start bug
/// reproducible in a raster probe — see the shared-transition test module).
/// The hero's motion. Slower than the framework default on purpose — this demo exists to be WATCHED,
/// and at the default (`TweenSpec::default()` = a LINEAR 300 ms tween) a flight is over before the eye
/// catches which end is which. `slowness` is in the same spirit: 1 is the demo default, 2 is for
/// observing a single flight closely.
///
/// Critically damped (`damping_ratio = 1.0`) so it eases into place without overshooting. Measured with
/// `anim-trace`, the engine's own spring (it stops when displacement AND velocity are below
/// `threshold = 0.001`, which is NOT the 2 % rule `4.75 / sqrt(stiffness)` describes):
///   stiffness 25 (slowness 1): p=0.5 at 0.34 s, p=0.9 at 0.78 s, p=0.99 at 1.34 s
///   stiffness  8 (slowness 2): p=0.5 at 0.60 s, p=0.9 at 1.38 s, p=0.99 at 2.36 s
/// The two hero colours — different on purpose so a flight is easy to read: blue leaving the list,
/// orange arriving in the detail (mid-flight you see the two blend).
fn list_hero_color() -> Color {
    Color::from_argb(255, 70, 140, 235)
}

fn detail_hero_color() -> Color {
    Color::from_argb(255, 240, 150, 55)
}

fn hero_motion(slowness: u8) -> BoundsTransform {
    BoundsTransform::spring(SpringSpec {
        damping_ratio: 1.0,
        stiffness: if slowness >= 2 { 8.0 } else { 25.0 },
        mass: 1.0,
        threshold: 0.001,
    })
}

/// Each end paints a DIFFERENT colour on purpose (list blue, detail orange), so during a flight you
/// can see which end is being drawn — and, mid-flight, the crossfade between the two.
#[composable]
fn hero(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    w: f32,
    h: f32,
    shape: Shape,
    motion: BoundsTransform,
    color: Color,
) {
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .clip(shape)
                .shared_bounds_with_overlay_clip(
                    scope.shared_content_state("hero"),
                    VisibilityTransition::fade_in(TweenSpec::default()),
                    VisibilityTransition::fade_out(TweenSpec::default()),
                    motion,
                    // Re-lay-out at the lerped size every frame instead of scaling a stable snapshot
                    // into it: the hero's own content is re-measured for the animated box.
                    ResizeMode::RemeasureToBounds,
                    PlaceHolderSize::AnimatedSize,
                    // Bend the path below the straight line between the two rects, so the flight is
                    // visibly an arc rather than a slide.
                    PathMotion::ArcBelow,
                    0.0,
                    true,
                    OverlayClip::Bounds,
                ),
        )
        .build(ctx, |ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .fill_max_size()
                    .background(color, Shape::Rectangle),
            );
            ctx.end_node();
        });
}

/// A SECOND shared element, keyed "badge", so a scene exercises two flights at once. It is much
/// smaller than the hero and sits BESIDE it in the list but ABOVE it in the detail, so the surrounding
/// non-shared content shifts while the flights run.
#[composable]
fn badge(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, motion: BoundsTransform, label: &str) {
    Column::new()
        .modifier(
            Modifier::new()
                .clip(Shape::rounded(11.0))
                .shared_bounds_with_overlay_clip(
                    scope.shared_content_state("badge"),
                    VisibilityTransition::fade_in(TweenSpec::default()),
                    VisibilityTransition::fade_out(TweenSpec::default()),
                    motion,
                    ResizeMode::scale_to_bounds(),
                    PlaceHolderSize::AnimatedSize,
                    PathMotion::Linear,
                    0.0,
                    true,
                    OverlayClip::Bounds,
                ),
        )
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(12.0)
                .modifier(Modifier::new().padding(6.0))
                .color(Color::from_argb(255, 30, 30, 34))
                .build(ctx);
        });
}

/// A non-shared chip: it exists only on the list screen, so it must ride its scene, not fly.
#[composable]
fn chip(ctx: &mut ComposeCtx, label: &str) {
    Column::new()
        .modifier(
            Modifier::new()
                .clip(Shape::rounded(9.0))
                .background(Color::from_argb(255, 44, 48, 58), Shape::rounded(9.0))
                .padding(6.0),
        )
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(12.0)
                .color(Color::from_argb(255, 170, 180, 200))
                .build(ctx);
        });
}

#[composable]
fn list_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    back_stack: &NavBackStack<Route>,
    motion: BoundsTransform,
) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("List").font_size(22.0).build(ctx);
            // Hero and badge side by side here…
            Row::new().spacing(10.0).build(ctx, |ctx| {
                hero(ctx, scope, 96.0, 96.0, Shape::Circle, motion.clone(), list_hero_color());
                badge(ctx, scope, motion.clone(), "new");
            });
            // …with list-only, NON-shared chips below them.
            Row::new().spacing(8.0).build(ctx, |ctx| {
                chip(ctx, "Popular");
                chip(ctx, "Nearby");
                chip(ctx, "Saved");
            });
            let bs = back_stack.clone();
            Button::text()
                .on_click(move || bs.push(Route::Detail))
                .build(ctx, |ctx| Text::new("Open detail").build(ctx));
            Text::new("List-only footer: rows below the hero are pushed by its placeholder size.")
                .font_size(11.0)
                .color(Color::from_argb(255, 130, 140, 160))
                .build(ctx);
        });
}

#[composable]
fn detail_screen(
    ctx: &mut ComposeCtx,
    scope: &SharedTransitionScope,
    back_stack: &NavBackStack<Route>,
    motion: BoundsTransform,
) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            // Title and badge on one line, hero BELOW them — the opposite arrangement to the list, so
            // both the badge and everything around the hero change position between the two screens.
            Row::new().spacing(10.0).build(ctx, |ctx| {
                Text::new("Detail").font_size(22.0).build(ctx);
                badge(ctx, scope, motion.clone(), "detail");
            });
            // Push the hero DOWN so the flight has an obvious vertical move as well as a size change:
            // on the list it sits high beside the badge, here it starts ~90px lower.
            Column::new()
                .modifier(Modifier::new().padding_top(90.0))
                .build(ctx, |ctx| {
                    hero(ctx, scope, 320.0, 220.0, Shape::rounded(12.0), motion.clone(), detail_hero_color());
                });
            // Detail-only, NON-shared content: a description block and an action row. It belongs to the
            // scene, so it fades with the scene and never flies. It DOES move with the hero while the
            // hero's animated placeholder size changes (`PlaceHolderSize::AnimatedSize`): only the hero
            // is a shared element, everything below it is ordinary layout.
            Text::new(
                "Detail-only block: not a shared element, so it belongs to the scene. It must fade \
                 with the scene and never fly — but it sits below the hero, so it does move with the \
                 hero's animated placeholder while the flight runs.",
            )
            .font_size(12.0)
            .color(Color::from_argb(255, 160, 170, 190))
            .build(ctx);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Button::text().build(ctx, |ctx| Text::new("Share").font_size(12.0).build(ctx));
                Button::text().build(ctx, |ctx| Text::new("Save").font_size(12.0).build(ctx));
            });
            let bs = back_stack.clone();
            Button::text()
                .on_click(move || {
                    bs.pop();
                })
                .build(ctx, |ctx| Text::new("Back").build(ctx));
        });
}

#[composable]
fn demo(ctx: &mut ComposeCtx) {
    let back_stack = ctx.remember(|| NavBackStack::<Route>::with_initial(Route::List)).get();
    // A separate clone for the display itself: the entry closures below move their own clones.
    let display_stack = back_stack.clone();
    let depth = back_stack.len();
    // The entry-level bridge (winia's counterpart of Nav3's `sharedEntryInSceneNavEntryDecorator`):
    // with it ON each ENTRY's content is itself a shared element keyed by the entry.
    //
    // Measured: it is INERT in every flow this demo can produce — a plain List→Detail push holds two
    // DIFFERENT entries (so nothing pairs, and only the hero/badge markers fly), and switching
    // single-pane ↔ two-pane opens no entry flight either, because an entry keeps its composition
    // identity (`ctx.key(entry.content_key())`) while the scene arrangement changes, and a flight needs
    // a slot change. OFF is therefore the honest comparison here: it shows the markers flying with the
    // decorator simply not acting. Details and the case that WOULD make it act (a scene arrangement that
    // re-slots one entry) are in docs/nav-shared-transition.md §6.
    let entry_flight = ctx.remember(|| true);
    // Two-pane (ListDetail) mode: the LIST entry stays in its pane while the DETAIL entry is added,
    // so the same entry is present in two scenes at once — which is the case the entry-level
    // decorator exists for (a plain push holds two DIFFERENT entries, so nothing pairs and, measured,
    // no entry flight opens).
    let two_pane = ctx.remember(|| false);
    // Motion preset: a smooth spring by default, a slower one with the toggle.
    // 1 = the demo's own pace (p=0.99 at 1.34 s), 2 = slow enough to watch one flight closely (2.36 s).
    let slow_motion = ctx.remember(|| false);
    let motion = hero_motion(if slow_motion.get() { 2 } else { 1 });
    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new(format!("nav × shared elements — stack depth {depth}"))
                    .font_size(12.0)
                    .build(ctx);
                Button::text()
                    .on_click({
                        clone!(entry_flight);
                        move || entry_flight.update(|v| *v = !*v)
                    })
                    .build(ctx, |ctx| {
                        Text::new(if entry_flight.get() {
                            "entry flight: ON"
                        } else {
                            "entry flight: OFF"
                        })
                        .font_size(12.0)
                        .build(ctx)
                    });
                Button::text()
                    .on_click({
                        clone!(two_pane);
                        move || two_pane.update(|v| *v = !*v)
                    })
                    .build(ctx, |ctx| {
                        Text::new(if two_pane.get() { "two-pane: ON" } else { "two-pane: OFF" })
                            .font_size(12.0)
                            .build(ctx)
                    });
                // Motion preset toggle. It was dead state for a while: the button had been dropped while
                // the comments kept referring to it, which made `slowness 2` unreachable.
                Button::text()
                    .on_click({
                        clone!(slow_motion);
                        move || slow_motion.update(|v| *v = !*v)
                    })
                    .build(ctx, |ctx| {
                        Text::new(if slow_motion.get() {
                            "slow motion: ON"
                        } else {
                            "slow motion: OFF"
                        })
                        .font_size(12.0)
                        .build(ctx)
                    });
                let display = NavDisplay::new(&display_stack, {
                    clone!(scope);
                    let motion_for_list = motion.clone();
                    let motion_for_detail = motion.clone();
                    move |_ctx, key| match key {
                        Route::List => NavEntry::new(key.clone(), {
                            clone!(scope);
                            let m = std::sync::Arc::new(motion_for_list.clone());
                            let bs = back_stack.clone();
                            move |ctx, _| list_screen(ctx, &scope, &bs, (*m).clone())
                        }),
                        Route::Detail => NavEntry::new(key.clone(), {
                            clone!(scope);
                            let m = std::sync::Arc::new(motion_for_detail.clone());
                            let bs = back_stack.clone();
                            move |ctx, _| detail_screen(ctx, &scope, &bs, (*m).clone())
                        }),
                    }
                });
                let mut display = display;
                // Slow the SCENE transition to match the hero's glide: the nav's default is a 300 ms fade,
                // which reads as abrupt next to a flight that takes 1.34 s to reach p=0.99 (measured with
                // anim-trace at the default stiffness; 2.36 s with the slow-motion preset).
                display = display
                    .transition_spec(
                        NavTransitionSpec::fade().duration(std::time::Duration::from_millis(800)),
                    )
                    .pop_transition_spec(
                        NavTransitionSpec::fade().duration(std::time::Duration::from_millis(800)),
                    );
                if two_pane.get() {
                    display = display.scene_strategies(vec![
                        Box::new(ListDetailStrategy) as Box<dyn SceneStrategy<Route>>
                    ]);
                }
                if entry_flight.get() {
                    display = display.add_decorator(winia::nav::SharedEntryInSceneDecorator);
                }
                display.build(ctx);
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 620.0)
                .title("Nav × Shared Elements")
                .build(ctx, |ctx| {
                    demo(ctx);
                });
        });
    });
}
