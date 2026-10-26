//! Lazy item animation showcase: placement, fade-in and fade-out on a real `LazyColumn`.
//!
//! The Flow demo (`item_animation_demo`) shows the same contract on a plain column; this one is
//! about what a VIRTUALIZED list adds: the window scrolls, items leave and re-enter it, keys move
//! between slots, and a removed card still has to fade out after its layout node is gone.
//!
//! Three buttons drive it — insert at the head, insert in the middle, remove from the middle — plus
//! a shuffle (pure placement: same keys, new slots) and a spec cycler, so the difference between
//! Compose's default contract, a placement-only one and a slower custom one is visible side by side.
//! Tapping a card removes that item, which is the fade-out case.
//!
//! Run with `cargo run -p winia --example lazy_item_animation_demo`; the same UI is the
//! `lazy_item_animation` fixture scenario driven by `tests/ui_lazy_item_animation_test.rs`.

use std::sync::Arc;
use std::time::Duration;

use winia::animation::interpolator::EaseOutCubic;
use winia::animation::item::ItemAnimationSpec;
use winia::animation::{SpringSpec, TweenSpec};
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// How many items the list starts with — more than the viewport holds, so the window really scrolls.
const INITIAL: u64 = 12;

/// The three contracts the demo cycles through.
#[derive(Clone, Copy, PartialEq, Debug)]
enum SpecMode {
    /// Compose's `Modifier.animateItem()` defaults: placement + fade-in + fade-out, three springs at
    /// `StiffnessMediumLow` (400).
    Default,
    /// The same placement, no appearance animation (`without_fade_in().without_fade_out()`).
    PlacementOnly,
    /// A bouncier placement and a 450 ms ease for the fades, so the difference is obvious.
    Slow,
}

impl SpecMode {
    fn label(self) -> &'static str {
        match self {
            SpecMode::Default => "default",
            SpecMode::PlacementOnly => "placement",
            SpecMode::Slow => "slow",
        }
    }

    fn next(self) -> Self {
        match self {
            SpecMode::Default => SpecMode::PlacementOnly,
            SpecMode::PlacementOnly => SpecMode::Slow,
            SpecMode::Slow => SpecMode::Default,
        }
    }

    fn spec(self) -> ItemAnimationSpec {
        match self {
            SpecMode::Default => ItemAnimationSpec::new(),
            SpecMode::PlacementOnly => {
                ItemAnimationSpec::new().without_fade_in().without_fade_out()
            }
            SpecMode::Slow => ItemAnimationSpec::new()
                .placement(SpringSpec {
                    damping_ratio: 0.72,
                    stiffness: 180.0,
                    ..SpringSpec::default()
                })
                .fade_in(TweenSpec::new(Duration::from_millis(450), EaseOutCubic::new()))
                .fade_out(TweenSpec::new(Duration::from_millis(450), EaseOutCubic::new())),
        }
    }
}

fn button(
    ctx: &mut ComposeCtx,
    tag: &'static str,
    label: impl Into<String>,
    on_click: impl Fn() + Send + Sync + 'static,
) {
    Button::new()
        .modifier(Modifier::new().test_tag(tag))
        .on_click(on_click)
        .build(ctx, |ctx| Text::new(label.into()).font_size(12.0).build(ctx));
}

/// The two colours a card alternates between, so a reorder is easy to follow.
fn card_color(key: u64) -> Color {
    if key % 2 == 0 {
        Color::from_argb(255, 92, 126, 196)
    } else {
        Color::from_argb(255, 118, 168, 132)
    }
}

/// One item. The animation marker sits on the item's ROOT content — the wrapper the list sees, which
/// is what `wrapper_item_spec` looks for. A marker deeper in the subtree belongs to a nested
/// container and does not drive this item.
fn card(ctx: &mut ComposeCtx, keys: State<Vec<u64>>, spec: ItemAnimationSpec, key: u64, index: usize) {
    Stack::new()
        .modifier(
            Modifier::new()
                .animate_item_with(spec)
                .fill_max_width()
                .height(56.0)
                .test_tag(format!("lazy-anim-item-{key}"))
                .background(card_color(key), Shape::rounded(10.0))
                .clickable(move || {
                    let mut next = keys.get();
                    next.retain(|value| *value != key);
                    keys.set(next);
                }),
        )
        .content_alignment(ContentAlignment::CENTER)
        .build(ctx, |ctx| {
            Text::new(format!("#{key}  (slot {index})"))
                .font_size(15.0)
                .color(Color::WHITE)
                .build(ctx);
        });
}

#[composable]
fn lazy_item_animation_demo(ctx: &mut ComposeCtx) {
    let keys = ctx.remember(|| (0u64..INITIAL).collect::<Vec<u64>>());
    let next_key = ctx.remember(|| INITIAL);
    let mode = ctx.remember(|| SpecMode::Default);
    let list_state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new("Lazy item animation").font_size(20.0).build(ctx);
            Text::new("insert, shuffle or tap a card to remove it")
                .font_size(12.0)
                .build(ctx);

            // A Flow, not a Row: five buttons do not fit one line at this width, and a button that
            // ends up outside the card cannot be clicked by coordinates (which is how the UI test drives
            // this page).
            FlowRow::new()
                .main_spacing(6.0)
                .cross_spacing(6.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                let (k, n) = (keys.clone(), next_key.clone());
                button(ctx, "lazy-anim-insert-head", "insert head", move || {
                    let mut next = k.get();
                    let id = n.get();
                    n.set(id + 1);
                    next.insert(0, id);
                    k.set(next);
                });
                let (k, n) = (keys.clone(), next_key.clone());
                button(ctx, "lazy-anim-insert-middle", "insert middle", move || {
                    let mut next = k.get();
                    let id = n.get();
                    n.set(id + 1);
                    let at = next.len() / 2;
                    next.insert(at, id);
                    k.set(next);
                });
                let k = keys.clone();
                button(ctx, "lazy-anim-remove-middle", "remove middle", move || {
                    let mut next = k.get();
                    if !next.is_empty() {
                        let at = next.len() / 2;
                        next.remove(at);
                    }
                    k.set(next);
                });
                let k = keys.clone();
                button(ctx, "lazy-anim-shuffle", "shuffle", move || {
                    // Same keys, different slots: pure placement, no fade at all.
                    let next = k.get();
                    let mid = next.len() / 2;
                    let (front, back) = next.split_at(mid);
                    let mut out = Vec::with_capacity(next.len());
                    for i in 0..front.len().max(back.len()) {
                        if let Some(value) = back.get(i) {
                            out.push(*value);
                        }
                        if let Some(value) = front.get(i) {
                            out.push(*value);
                        }
                    }
                    k.set(out);
                });
                // The label names the contract that is ACTIVE (tapping cycles to the next one), and it
                // is read INSIDE the button's own content closure: the dependency then registers on that
                // group, which re-runs on a change. A label computed out here and captured into the
                // closure never updates — the FlowRow around it skips, so the closure does not re-run
                // (the trap component-authoring 3.2 describes).
                let m = mode.clone();
                let m_label = mode.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lazy-anim-spec"))
                    .on_click(move || m.set(m.get().next()))
                    .build(ctx, |ctx| {
                        Text::new(format!("spec: {}", m_label.get().label()))
                            .font_size(12.0)
                            .build(ctx);
                    });
            });

            let spec = mode.get().spec();
            let keys_now = keys.get();
            Text::new(format!(
                "{} items | spec {} | fade {} | offset {:.0}",
                keys_now.len(),
                mode.get().label(),
                if mode.get() == SpecMode::PlacementOnly { "off" } else { "on" },
                list_state.offset(),
            ))
            .modifier(Modifier::new().test_tag("lazy-anim-info"))
            .font_size(12.0)
            .build(ctx);

            LazyColumn::new()
                .state(list_state.clone())
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(460.0)
                        .test_tag("lazy-anim-list"),
                )
                .spacing(8.0)
                .content_padding(4.0, 4.0)
                .beyond_bounds(2)
                .items_from(Arc::new(keys_now), |key| *key, move |ctx, index, key| {
                    card(ctx, keys.clone(), spec.clone(), *key, index);
                })
                .build(ctx);
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 780.0)
                .title("Lazy item animation")
                .build(ctx, |ctx| {
                    settings::shell("Lazy Item Animation", ctx, lazy_item_animation_demo);
                });
        });
    });
}
