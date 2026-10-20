# Fling behavior

What happens between a release and the moment a scroll comes to rest. Compose calls this
`flingBehavior` (a `TargetedFlingBehavior`); winia's version is `winia::fling`.

## Why the shape is different

Compose's `TargetedFlingBehavior.performFling` is a SUSPENDING function that drives the scroll
itself: it calls `ScrollScope.scrollBy` every frame and returns the velocity it could not use, so
nested scroll has something to hand on (`TargetedFlingBehavior.kt:44-47`).

winia's animation engine is push-based: a motion is registered against a `State<f32>` and the frame
clock ticks it. A strategy therefore cannot run a motion; it can only DESCRIBE one. That is the trade
this module makes:

| | Compose |
| --- | --- |
| the strategy returns | `FlingPlan` — a target and a curve, or "decay", or "nothing" |
| the strategy drives | nothing: `ScrollState::fling_with_boundary` executes the plan |
| a strategy can | decide where a release lands and what it is launched with; fall back to momentum |
| a strategy cannot | invent a per-frame curve the engine does not have, or read the layout mid-flight |

For the two things a caller actually reaches for — "where does this fling land" and "what does it
feel like" — the plan is enough, and it keeps the strategy a pure function of the release.

## The vocabulary

```rust
pub trait FlingBehavior: Debug + Send + Sync {
    fn plan(&self, fling: FlingContext) -> FlingPlan;
}

pub struct FlingContext {
    pub offset: f32,      // where the container is now, scroll px
    pub velocity: f32,    // the release velocity, px/s, signed
    pub limit: f32,       // content - viewport, or f32::MAX while unmeasured
    pub snap: Option<SnapInfo>,  // how the container pages, if it does
    pub density: f32,     // for a strategy that thinks in dp
}

pub enum FlingPlan {
    Spring { target: f32, spec: SpringSpec },
    Decay { spec: DecaySpec },
    None,
}
```

`SnapInfo` is the paging configuration as a strategy sees it — step, anchor, `min_fling_velocity`,
`positional_threshold`, the settle spring and `max_pages_per_fling` — so a custom strategy can make
the same decisions the built-in one does without reaching into the container.

## The default

`SnapFlingBehavior` is the built-in and the default for every container:

- a container with NO snap spec keeps its momentum (`FlingPlan::Decay` at the tree's friction,
  `FLING_FRICTION = 4.2`);
- a PAGED container settles on a page boundary with ONE spring, launched with the gesture's
  velocity. A release under the fling floor settles on the nearer boundary unless it has crossed
  `positional_threshold`; above the floor it carries as many pages as the decay estimate suggests,
  capped by `max_pages_per_fling`.

That is the whole of `docs/pager.md`'s "one gesture moves at most one page" rule, and a custom
strategy is free to ignore it.

## Writing one

```rust
#[derive(Debug)]
struct AlwaysDecay;

impl FlingBehavior for AlwaysDecay {
    fn plan(&self, fling: FlingContext) -> FlingPlan {
        FlingPlan::decay(exponential_decay(4.2))
    }
}

HorizontalPager::new(state)
    .fling_behavior(AlwaysDecay)
    .build(ctx, |ctx, page| { /* ... */ });
```

To change one decision instead of all of them, delegate:

```rust
#[derive(Debug)]
struct NeverPastTheEnd;

impl FlingBehavior for NeverPastTheEnd {
    fn plan(&self, fling: FlingContext) -> FlingPlan {
        match SnapFlingBehavior.plan(fling.clone()) {
            FlingPlan::Spring { target, spec } => FlingPlan::spring(target.min(fling.limit), spec),
            other => other,
        }
    }
}
```

## Contracts

- **Asked once per fling**, from the gesture path, on the UI thread. It must be cheap and free of
  side effects: the same context may be produced again by a later release.
- **The plan is clamped to the container's limit.** A strategy may aim past the end on purpose (a
  "bounce past the edge" plan is clamped to the limit, which is what a scroll value is allowed to
  reach); a `Decay` is clamped per frame the same way.
- **`FlingPlan::None` starts no animation** and clears `is_scroll_in_progress`, so a container that
  declined does not sit there claiming to be scrolling.
- **Identity is the parameter.** A component that takes a behavior compares it by `Arc` identity, not
  by value, so swapping the object re-enters and passing the same one down does not.
- **The wheel is not a fling.** Page steps taken by a wheel notch go through
  `ScrollState::snap_page` and never consult a strategy.

## Where it is exposed

| API | What it sets |
| --- | --- |
| `Pager::fling_behavior(impl FlingBehavior)` | the strategy a pager's releases run through |
| `LazyList::fling_behavior(Arc<dyn FlingBehavior>)` | the same, for a list used without a pager |
| `ScrollState::fling_behavior` | the field both of the above assemble into; a caller holding a bare `ScrollState` can set it directly |

## Tests

`cargo test -p winia --lib fling::` - the built-in plans a decay without a snap spec, a spring on a
boundary with one, and clamps its own target.

`cargo test -p winia --lib a_custom_fling_behavior` / `a_strategy_that_declines` /
`a_strategy_can_choose_the_momentum` - the executor runs the plan it is given: a custom target, no
motion at all, or momentum with the strategy's own decay.

`cargo test -p winia --lib a_pager_hands_its_fling_behavior` - a pager hands its strategy to the
list's scroll state, and the release lands where the strategy says instead of on a page boundary.
