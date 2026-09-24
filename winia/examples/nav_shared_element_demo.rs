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
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;


/// Type-safe routes (compares to Nav3's `NavKey` with `@Serializable`; winia needs no serialization).
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
enum Route {
    List,
    Detail,
}

/// The hero card: a container that draws nothing itself and fills with a coloured child, so a
/// lost child is visible as an empty hole (the shape that made the transparent-start bug
/// reproducible in a raster probe — see the shared-transition test module).
/// The hero's motion: the FRAMEWORK DEFAULT unless the slow-motion toggle is on, so the demo shows the
/// normal timing by default and a watchable one on request.
///
/// `slowness 1` = no opinion at all (`BoundsTransform::default()`, i.e. `TweenSpec::default()`: a LINEAR
/// 300 ms tween — measured earlier at p=0.5 / 0.9 / 0.99 = 153 / 276 / 298 ms). `slowness 2` = a
/// critically damped spring that is slow enough to read which end is which while it flies; measured with
/// `anim-trace` on the engine's own spring (it stops when displacement AND velocity are below
/// `threshold = 0.001`, which is NOT the 2 % rule `4.75 / sqrt(stiffness)` describes):
///   stiffness 8 (slowness 2): p=0.5 at 0.60 s, p=0.9 at 1.38 s, p=0.99 at 2.36 s
/// The two hero colours — different on purpose so a flight is easy to read: blue leaving the list,
/// orange arriving in the detail (mid-flight you see the two blend).
fn list_hero_color() -> Color {
    Color::from_argb(255, 70, 140, 235)
}

fn detail_hero_color() -> Color {
    Color::from_argb(255, 240, 150, 55)
}

fn hero_motion(slowness: u8) -> BoundsTransform {
    if slowness < 2 {
        // The framework's own default: this is what an app gets with no opinion.
        return BoundsTransform::default();
    }
    BoundsTransform::spring(SpringSpec {
        damping_ratio: 1.0,
        stiffness: 8.0,
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
    // Measured: it opens no entry FLIGHT in any flow this demo can produce — a plain List→Detail push
    // holds two DIFFERENT entries (so nothing pairs, and only the hero/badge markers fly), and switching
    // single-pane ↔ two-pane opens no entry flight either, because an entry keeps its composition
    // identity (`ctx.key(entry.content_key())`) while the scene arrangement changes, and a flight needs a
    // slot change. It DOES take part in the transition-scoped size morph: flipping two-pane while a
    // transition runs opens one for the entry marker (`morph_open flight:entry:… fresh 252x484 -> 168x484`,
    // measured on a running demo), which is a size animation rather than a flight. Details and the case
    // that WOULD give it a flight (a scene arrangement that re-slots one entry) are in
    // docs/nav-shared-transition.md §6.
    let entry_flight = ctx.remember(|| true);
    // Two-pane (ListDetail) mode: the LIST entry stays in its pane while the DETAIL entry is added,
    // so the same entry is present in two scenes at once — which is the case the entry-level
    // decorator exists for (a plain push holds two DIFFERENT entries, so nothing pairs and, measured,
    // no entry flight opens).
    let two_pane = ctx.remember(|| false);
    // Motion preset: the framework default unless the toggle asks for a slower, watchable one.
    // 1 = no override at all (linear 300 ms tween, p=0.99 at ~0.30 s),
    // 2 = a slow spring (p=0.99 at 2.36 s).
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
                // Scene transition: the framework default (300 ms fade) unless slow motion is on, where an
                // 800 ms fade matches the slow hero's glide (2.36 s to p=0.99; the default 300 ms tween
                // would be over while a slow flight is still barely moving). With the toggle off the demo
                // overrides NOTHING, so what it shows is what an app gets.
                if slow_motion.get() {
                    display = display
                        .transition_spec(
                            NavTransitionSpec::fade().duration(std::time::Duration::from_millis(800)),
                        )
                        .pop_transition_spec(
                            NavTransitionSpec::fade().duration(std::time::Duration::from_millis(800)),
                        );
                }
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
                    settings::shell("Nav × Shared Elements", ctx, |ctx| {
                        demo(ctx);
                    });
                });
        });
    });
}
