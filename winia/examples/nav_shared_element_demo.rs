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
/// The hero's motion: a critically damped spring (`damping_ratio = 1.0` — it eases into place without
/// overshooting) at a low stiffness, so a flight reads as a smooth glide over roughly half a second.
///
/// The framework default is `TweenSpec::default()` = a LINEAR 300 ms tween, which is why a flight
/// started out looking mechanical and abrupt; `slow` lowers the stiffness further for a longer, softer
/// settle so the two can be compared side by side.
fn hero_motion(slow: bool) -> BoundsTransform {
    BoundsTransform::spring(SpringSpec {
        damping_ratio: 1.0,
        stiffness: if slow { 55.0 } else { 120.0 },
        mass: 1.0,
        threshold: 0.001,
    })
}

#[composable]
fn hero(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: f32, h: f32, shape: Shape, motion: BoundsTransform) {
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
                    ResizeMode::scale_to_bounds(),
                    PlaceHolderSize::AnimatedSize,
                    PathMotion::Linear,
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
                    .background(Color::from_argb(255, 90, 150, 220), Shape::Rectangle),
            );
            ctx.end_node();
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
            hero(ctx, scope, 96.0, 96.0, Shape::Circle, motion);
            let bs = back_stack.clone();
            Button::text()
                .on_click(move || bs.push(Route::Detail))
                .build(ctx, |ctx| Text::new("Open detail").build(ctx));
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
            Text::new("Detail").font_size(22.0).build(ctx);
            hero(ctx, scope, 320.0, 220.0, Shape::rounded(12.0), motion);
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
    // with it ON the ENTRY ITSELF is a shared element keyed by the entry, so a nav transition can fly
    // a whole screen; OFF compares against the plain nav transition (only the hero marker flies).
    let entry_flight = ctx.remember(|| true);
    // Two-pane (ListDetail) mode: the LIST entry stays in its pane while the DETAIL entry is added,
    // so the same entry is present in two scenes at once — which is the case the entry-level
    // decorator exists for (a plain push holds two DIFFERENT entries, so nothing pairs and, measured,
    // no entry flight opens).
    let two_pane = ctx.remember(|| false);
    // Motion preset: a smooth spring by default, a slower one with the toggle.
    let slow_motion = ctx.remember(|| false);
    let motion = hero_motion(slow_motion.get());
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
                let mut display = NavDisplay::new(&display_stack, {
                    clone!(scope);
                    let motion_for_list = motion.clone();
                    let motion_for_detail = motion.clone();
                    move |ctx, key| match key {
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
                // Slow the SCENE transition to match the hero's glide: the nav's default is 300 ms,
                // which reads as abrupt next to a ~600 ms flight.
                display = display
                    .transition_spec(
                        NavTransitionSpec::fade().duration(std::time::Duration::from_millis(450)),
                    )
                    .pop_transition_spec(
                        NavTransitionSpec::fade().duration(std::time::Duration::from_millis(450)),
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
