# Component authoring

This is the contract for adding a component to `winia/src/components`. It is derived from the
components that already pass review, and cross-checked against the Compose sources in `reference/`
where the behaviour is inherited rather than invented.

Rules use MUST / SHOULD / MUST NOT. Each one carries the failure it prevents, because most of them are
invisible in a passing test.

Companion documents, read as needed:

| Document | What it settles |
| --- | --- |
| `docs/architecture.md` | the three phases and the runtime model |
| `docs/key-system-design.md` | keys, statements, and WHEN a component may take a scope |
| `docs/state-handles.md` | which `remember` shape to use for which kind of state |
| `docs/effects.md` | `LaunchedEffect` / `DisposableEffect` / `side_effect` / scope cancellation |
| `docs/semantics.md` | roles, claims, and what each control declares |
| `docs/<component>.md` | one file per component, written as you build it (section 12) |

## 1. The shape of a component

A component is a **builder struct** plus a `build` method. Construction is cheap and total; `build`
does the composing.

```rust
pub struct Badge {
    label: String,
    container: Color,
    modifier: Modifier,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            container: WiniaTheme::colors().error,
            modifier: Modifier::new(),
        }
    }

    /// Every option is a method that returns `Self` - the builder stays linear.
    pub fn container_color(mut self, color: Color) -> Self {
        self.container = color;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let modifier = Modifier::new()
            .background(self.container, Shape::Rectangle)
            .test_tag("badge")
            .then(self.modifier);
        // ... compose the content, see the sections below ...
        let _ = (ctx, modifier, self.label);
    }
}
```

**MUST** keep the fields private. Public fields freeze the representation and let callers mutate state
between `new` and `build`, which no component's invariants survive.

**MUST** name the default constructor `new` and give it the Compose default.

### 1.1 Variants are constructors, not types

Where Compose ships several composables that differ only in defaults, winia keeps ONE type and adds
associated constructors:

| Compose | winia |
| --- | --- |
| `Button` / `ElevatedButton` / `FilledTonalButton` / `OutlinedButton` | `Button::filled()` / `::elevated()` / `::filled_tonal()` / `::outlined()` (`winia/src/components/button.rs`) |
| `Card` / `ElevatedCard` / `OutlinedCard` | `Card::new()` / `::elevated()` / `::outlined()` |
| `TopAppBar` / `CenterAlignedTopAppBar` / `MediumTopAppBar` / `LargeTopAppBar` | `TopAppBar::new()` / `::center_aligned()` / `::medium()` / `::large()` |

**Why:** the variants share every slot, every measurement rule and every state contract. Separate
types would duplicate all of it and split the tests. The variant is a value the builder carries, not a
different machine.

### 1.2 `#[composable]` is a decision, not a default

The macro gives the component its own composition scope, which becomes the **dependency registration
target** for everything it reads.

- **MUST** mark a component `#[composable]` when it is self-contained: it remembers state and consumes
  that state itself (Button, Checkbox, TextField, Icon, the effect hooks).
- **MUST NOT** mark a LAYOUT PRIMITIVE that hands the caller's closure straight through: `Column`,
  `Row`, `Stack`, `Flow`, and the animation containers (`AnimatedVisibility`, `AnimatedContent`,
  `Crossfade`, `AnimatedSize`) stay scope-free.
- **Why:** a `State::get()` written at the top of the content closure belongs to whoever composed
  it. With a scope in between it registers on the container instead, the parent Skips, and the
  change never re-runs the content - that is how the checkbox demo's select-all stopped
  propagating. Every other component that composes a restartable group IS `#[composable]` (34 of
  the 39 that do), because its content is composed by the component, not passed through. The full
  rule and the current exclusion list are in `docs/key-system-design.md` section 7.1-7.2.

The distinction is about the closure, not the shape: a container that passes the caller's closure
through unchanged must not insert a scope; a container that composes its children itself (and
therefore has to declare its own parameters) takes one.

### 1.3 Content closures

| Signature | Use when |
| --- | --- |
| `impl Fn(&mut ComposeCtx)` | content may be composed more than once (rows, items, repeated slots) |
| `impl FnOnce(&mut ComposeCtx)` | exactly once, and the closure consumes a builder |
| `impl Fn(&mut ComposeCtx, T)` | the container hands the content a scope value (a `RowScope`-like context, the measured constraints) |
| `impl Fn(&mut ComposeCtx) + Send + 'static` | the content crosses into a window's own composer |

**MUST** take the narrowest one that works: `FnOnce` documents that the content is not retained.

## 2. Parameters

### 2.1 Order mirrors Compose

Compose's ordering is a reading convention the ecosystem already knows: the action, the modifier, the
behavioural flags, the appearance, the fine tuning, then the content. Match it (`Button.kt:139` in the
reference tree):

```text
on_click, modifier, enabled, shape, colors, elevation, border, content_padding, interaction_source, content
```

**MUST** keep `modifier` immediately after the action or the value, so a caller skimming the builder
finds it where Compose puts it.

### 2.2 Defaults live in a `XDefaults` type

Shape, colour and elevation defaults go in `ButtonDefaults` / `CardDefaults`-style types with
associated functions, not as `impl Default for X` and not inline in the builder. `ButtonDefaults::
filled_button_elevation()` is the Compose name and the Compose value; a component that inlines
`Shadow::new(0.0, 2.0)` is a token drift waiting to happen.

### 2.3 The caller's modifier goes LAST

```rust
let modifier = Modifier::new()
    .padding(padding)
    .background(container, shape)
    .then(self.modifier);   // the caller's last, so it can override
```

**MUST** append `self.modifier` at the end of the component's own modifier chain. **EXCEPT** semantics:
a role declared before `then(user_modifier)` survives a caller adding their own description, which is
exactly why `docs/semantics.md` states that ordering as a rule.

## 3. Composition identity: the part that breaks silently

### 3.1 Free keys, explicit keys

`#[composable]` injects a statement key into every statement that calls `ctx`, so a component normally
needs no key of its own. **MUST** use `ctx.key(id, |ctx| ...)` only when state has to survive a
branch that changes how many slots are consumed before it:

```rust
if expanded {
    let state = ctx.key("badge.state", |ctx| ctx.remember(|| BadgeState::new()));
}
```

### 3.2 The `ctx.changed` whitelist - read this twice

A container that compares its parameters to decide whether to Skip does **not** compare them
automatically. The comparison list is a **whitelist**: every parameter that can change what the
content composes has to be declared before the group starts.

```rust
#[composable]
pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    // Every parameter whose change must re-run `content`:
    ctx.changed(&self.variant);
    ctx.changed(&self.enabled);
    ctx.changed(&direction);

    let key = ctx.next_key();
    let policy = MyLayoutPolicy { ... };
    match ctx.start_restartable_group(key, modifier, policy) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => { /* compose the slots and the content here */ }
    }
    ctx.end_restartable_group();
}
```

**MUST** declare every parameter the content or the slots read. The modifier chain carries some of
them and the declaration carries the rest - know which channel you are using: `Modifier::param_eq`
(`modifier.rs`) compares sizes, padding, border, semantics, layout direction, z-index and `test_tag`,
and `TextField` relies on that for its visual parameters (`text_field.rs:909-916`); everything the
chain cannot express - closures, flag-shaped branch controls, values passed outside the modifier -
has to go through `ctx.changed`, or a Clean slot Skips past it.

**The failure it prevents:** an undeclared parameter does not cause a stale frame twice - it causes a
Clean slot to Skip past the change forever, and it looks exactly like a broken state notification. The
`ctx.changed` contract is stated in `winia/src/runtime/composer.rs` (`ComposeCtx::changed`) and
`docs/key-system-design.md`; a container with parameters and no `ctx.changed` calls is a bug unless its
content is driven by something other than parameter comparison (an animation tick, an inner
composition that reads the state itself). Those six files are listed with their reason in
`components::conventions` (`CONTENT_DRIVEN_WITHOUT_CHANGED`), and adding to that list is a deliberate
decision, not a workaround.

### 3.3 Which composition entry point

| Entry point | Use for |
| --- | --- |
| `ctx.start_leaf(key, modifier)` + `ctx.end_node()` | a leaf: one node, no children composed by us |
| `ctx.start_restartable_group(key, modifier, policy)` + `ctx.end_restartable_group()` | a container that composes children and can Skip |
| a custom `MeasurePolicy` | deciding how children are measured and placed |
| `subcompose(constraints, content)` (a free function in `layout/subcompose.rs`, callable from a `MeasurePolicy`) | content has to be composed with the constraints already known |

## 4. State

### 4.1 Internal state

`ctx.remember(|| value)` returns a `State<value>`. **MUST NOT** write
`ctx.remember(|| State::new(x))` - that nests a `State` inside a `State` for no reader. The four
shapes and when each applies are in `docs/state-handles.md` (section 7.4):

| Shape | Notification | Use for |
| --- | --- | --- |
| `ctx.remember(|| v)` -> `State<T>` | recompose + wake | anything a component reads and renders |
| `ctx.remember_animating(|| v)` | recompose, no wake | animation ticks |
| `ctx.remember_visual(|| v)` | draw layer only | values consumed at draw time |
| `ctx.remember_backchannel(|| v)` | none | measurement write-back, per-frame sync |

A handle type that is none of these (`LazyListState`, `SheetState`) keeps
`ctx.remember(|| Handle::new()).get()` and is correct that way - see the same section.

### 4.2 Hoisted state

When the CALLER has to observe or drive the component, the component does not own the state: define an
`XState` value and hand it in.

```rust
pub struct SheetState { /* handles */ }
impl SheetState { pub fn new() -> Self; pub fn expand(&self); pub fn current_value(&self) -> SheetValue; }
pub fn remember_sheet_state(ctx: &mut ComposeCtx) -> SheetState;  // in the same module

BottomSheet::new().state(state.clone()).build(ctx, content);
```

**MUST** provide the `remember_x_state` counterpart so a caller who does not care about the handle does
not have to write `ctx.remember` themselves (Compose's `rememberSheetState`).

**SHOULD** hoist when Compose hoists, and not before: a colour, a size or a flag the caller only reads
from `XDefaults` stays a parameter.

### 4.3 A predicate over continuous state is a derive

When the component only needs "did this cross a threshold" but the source moves continuously (a
scroll offset, a fraction, a progress), **MUST** use `remember_derived_state` rather than reading the
source and recomposing per pixel:

```rust
let scrolled = remember_derived_state(ctx, move || source.offset() != 0.0);
```

Reading the raw state recomposes on every write; the derive publishes only the flips. The rule and
the cases where it is NOT worth it (a predicate already backed by a `State<bool>`) are in
`docs/flow-design.md` section 6.

### 4.4 Effects

- `LaunchedEffect::new(key).build(ctx, |scope| async move { ... })` for async work tied to the
  composition point. The body is cancelled when the point goes away and restarted when the key changes.
- `DisposableEffect::new(key).build(ctx, |key_value| move || { /* teardown */ })` when there is a
  teardown: the effect closure returns the cleanup, which runs when the point goes away or the key
  changes.
- `on_forget(ctx, closure)` for a resource whose lifetime IS the slot (a scope, a registration).
- `side_effect(ctx, closure)` to publish to something outside the composition at the end of the pass.

**MUST NOT** spawn a task with `rt.spawn` and keep the handle in a field: it outlives the slot. The
effect hooks exist so it cannot. `docs/effects.md` has the ordering and the cancellation contract.

## 5. Layout and measurement

### 5.1 Leaves

A leaf measures itself from its modifier chain (`start_leaf`), which is what
`Modifier::size`/`padding`/`minimum_size` already do. Reaching for a `MeasurePolicy` for a leaf means
the modifier chain was missing a node.

### 5.2 Containers

A container's `MeasurePolicy::measure` gets the children and the constraints and returns a size; the
positions it writes are **relative to the container**, and the place phase overwrites the measured
size. **MUST NOT** cache a measurement across frames: the constraints come in as an argument and the
runtime decides when to re-measure.

### 5.3 Touch targets and insets

**MUST** keep the M3 minimums: a 48dp touch slot for anything clickable, 4dp end insets where the
M3 spec has them, and the token values from the reference tree rather than a rounded number. The
`TopAppBar` slot helpers and `Button` are the worked examples.

### 5.4 Direction

**MUST** read `Modifier::get_layout_direction()` (or
`crate::layout::direction::current()`) and mirror `start`/`end` rather than assuming left/right.
Anything that computes a title position, a slot order or an inset in absolute pixels is RTL-broken.

## 6. Appearance

**MUST** resolve colours and text styles through `WiniaTheme::colors()` /
`WiniaTheme::typography()`, and prefix the call with the theme the component draws in. A literal
`Color::from_rgb(0.2, ...)` is a dark-mode bug.

Token constants are named after the Compose token class and carry the source in a comment:

```rust
/// `AppBarSmallTokens.ContainerHeight` (`material3/AppBarSmallTokens.kt`).
const TOP_APP_BAR_HEIGHT: f32 = 64.0;
```

**MUST** keep four-state colour sets (enabled / disabled / pressed / hovered) in the `XColors` type,
not as inline `if` chains, so a caller can restyle the component without copying it.

### 6.1 Restyling one component class, once

Role colours cover "this app is green"; a component styling covers "every button is green while the
palette stays blue". The second is optional per class and provided once for a subtree:

```rust
use winia::prelude::*;

with_component_styles(
    ComponentStyles::new().with(ButtonStyling::new(|scope| {
        scope.container(Color::from_argb(255, 0, 128, 0));
    })),
    || app(ctx),
);
```

**Where the pieces live.** `winia/src/component_styles.rs` holds the MECHANISM only: the generic
`Styling<S>` (a function over a scope, composable with `then`), the type-keyed `ComponentStyles`
registry, and the `CompositionLocal` plus its provider. A component owns its SCOPE and its styling
alias in its own file:

```rust
// winia/src/components/button.rs - the colours it sets are defined here, so the scope is here.
pub struct ButtonStylingScope {
    pub theme: ThemeColors,
    pub is_dark: bool,
    pub colors: ButtonColors,
    pub border: Option<Color>,
}
pub type ButtonStyling = Styling<ButtonStylingScope>;
pub fn button_visual_with_styles(theme: &ThemeColors, variant: ButtonStyle) -> ButtonVisual;
```

The framework itself, its contract for a component author, and the light/dark story are specified in
[component-styling.md](component-styling.md).

**MUST** keep it that way. A per-component `Option` field on the registry would make the mechanism
module depend on every component in the crate (the opposite of the direction every component reads),
and would grow one file by three types per component. The registry is keyed by `TypeId` of the
styling type instead, so adding a component touches that component's file and nothing else.

**MUST** resolve a component's default appearance through its own helper (`button_visual_with_styles`
for a button) rather than calling the theme derivation directly: the helper is the single place
"explicit parameter > styling > theme" is decided, and a component that bypasses it silently ignores
every styling. The current participants are Button and Checkbox.

**MUST NOT** let a styling replace the theme derivation wholesale: the scope starts from the theme's
answer, so a styling that sets one field leaves the rest exactly as the role colours left them.

**MUST** declare the RESOLVED appearance with `ctx.changed` before the restartable group starts
(`ctx.changed(&colors)`), the same whitelist rule as any other parameter: a class styling is global
rather than a field of the builder, so without this a Clean slot keeps the label style and the border
it built on an earlier pass while only live closures (background, ripple) update - the container
changes and the text inside it does not.

One caveat, the same one every `CompositionLocal` has: providing a NEW set does not invalidate
readers that were composed before it. A styling that changes at runtime (a user switching to a brand
palette) has to be driven by a `State` read inside the subtree, the way `WiniaTheme::auto` follows the
system theme - otherwise the components keep the colours they composed with.

## 7. Slots

Optional named content goes in `Option<Box<dyn FnOnce(&mut ComposeCtx) + Send>>` with a builder
method per slot (`navigation_icon`, `actions`, `subtitle`). **MUST** give a slot the M3 geometry it
has in Compose (touch slot, inset, alignment) instead of letting the content decide.

- Text slots **SHOULD** be wrapped in `ProvideTextStyle(...)` so the style is the component's,
  overridable by the caller, not baked into the content's `Text`.
- Slots that take a colour **SHOULD** be wrapped in `WiniaTheme::with_content_color(...)` so an `Icon`
  or `Text` inside inherits it.
- A slot that changes how much space another slot gets (a `subtitle` that changes the expanded height)
  is a **parameter**, not a hidden consequence: it has to be part of the `ctx.changed` list.

## 8. Interaction and focus

- A click target **MUST** use `Modifier::clickable` (which claims semantics automatically) and a
  `MutableInteractionSource` when pressed/hovered/focused appearance differs from the resting one.
- A disabled component **MUST NOT** silently drop its semantics: a disabled button has no `clickable`,
  so it has to claim its label explicitly or the accessibility tree loses its name
  (`docs/semantics.md`, rule 1).
- A focusable component **MUST** obtain a `FocusRequester` and attach it with
  `Modifier::focus_requester(..)` (the requester's own `request_focus()` is what moves focus);
  `TextField` is the model to copy (`text_field.rs:1568`). Focus has to survive recomposition: the runtime
  restores it by slot key, so a component that changes its slot layout on a branch can lose it
  (that is what `ctx.key` is for).

## 9. Semantics

Every interactive component declares a role and the state a screen reader needs. The current table is
in `docs/semantics.md` ("What the components declare"); a new control has to add a row to it.

Two rules cover most cases:

1. A node that claims its subtree absorbs it. A single-control component calls
   `merge_descendants(true)` even when disabled.
2. A node that declares and claims nothing is transparent - a layout container must stay invisible in
   the tree.

**MUST** declare the role and state BEFORE appending the caller's modifier, so a caller's own
`semantics(...)` can override while keeping the role when it declares nothing.

## 10. Testing

| Level | Where | What it must cover |
| --- | --- | --- |
| Unit | `#[cfg(test)] mod tests` in the component file | builder defaults, measured size and positions, colour selection, the `ctx.changed` behaviour |
| Fixture | `winia/tests/ui_fixtures/fixture_all.rs` (one bin, scenario by `argv[1]`) | a scenario the visual/behavioural suite can drive |
| UI | `winia/tests/ui_test.rs` (`--features debug-server`) | clicks, key events, semantics tree, screenshots |

**MUST** write the unit test against the arena, not against a rendering:

```rust
let mut composer = Composer::new();
composer.compose(|ctx| TopAppBar::large(|ctx| ...).build(ctx));
composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
let root = composer.layout_root_idx().unwrap();
assert_eq!(composer.arena_nodes()[root].measured_size.height, 152.0);
```

Test names are sentences in snake_case, phrased as the property, not the method:
`the_flexible_variants_take_their_height_from_the_subtitle`, not `test_height`.

**MUST** hold `crate::animation::tests::TEST_SERIAL` and clear the registry in any test that pushes an
animation (`push_animatable`, `update_animations`, `clear_all_animations`); the suite relies on
serialisation that only works if every animation-driving test takes it (see
`docs/developer-guide.md` section 8.4).

## 11. Registering

1. `winia/src/components/<snake_case>.rs`
2. `pub mod <snake_case>;` in `winia/src/components.rs`, next to the components it belongs with
3. the public types in the prelude's component block in `winia/src/lib.rs`
4. `docs/<kebab-case>.md` (section 12) and, if it is a control, a row in `docs/semantics.md`

## 12. Documentation

Every component ships `docs/<component>.md` with these sections, in this order:

1. **Sources** - the Compose files it mirrors (paths in `reference/`), so the next reader can diff
   behaviour instead of guessing.
2. **Variants** - one table: constructor, height, when to use it.
3. **Layout** - how it measures and places, in words, including the RTL behaviour.
4. **Behaviour** - state, interaction, what changes and when. Name the tokens.
5. **Differences from Compose** - deliberate ones, with the reason. Silence here is a claim of parity.
6. **Tests** - the commands to run it, and the fixture scenario name.

## 13. Checklist

Copy this into the pull request and tick it there:

```text
[ ] builder struct, private fields, new() + one method per option returning Self
[ ] #[composable] decided by section 1.2, not by habit
[ ] every parameter that can change the content is in a ctx.changed call BEFORE the group starts
     (enforced: components::conventions::every_container_declares_its_parameters)
[ ] ctx.key only where a branch can change the slot layout
[ ] ctx.remember without a nested State; the right handle shape for the kind of state
     (enforced: components::conventions::no_component_nests_a_state_in_remember)
[ ] no rt.spawn in a component - effect hooks own spawning
     (enforced: components::conventions::no_component_spawns_its_own_task)
[ ] hoisted state + remember_x_state when the caller must drive it
[ ] remember_derived_state for a threshold over a continuous source
[ ] self.modifier appended last, semantics declared before it
[ ] colours and text styles from WiniaTheme; token constants named after the Compose token
[ ] M3 geometry: 48dp touch targets, insets, RTL mirroring
[ ] role + state declared, merge_descendants for a single-control component
[ ] disabled state keeps the semantics
[ ] unit tests over the arena, animation tests take TEST_SERIAL
[ ] docs/<component>.md with the six sections, and a semantics.md row if it is a control
[ ] registered in components.rs and the prelude
```

## 14. The three failures this document exists to prevent

1. **A parameter that never reaches the content.** Missing `ctx.changed` -> the container Skips past
   the change forever, and it looks like a broken State chain (section 3.2).
2. **A scope in the wrong place.** `#[composable]` on a container -> the parent Skips and the content
   stops reacting to state it reads (section 1.2).
3. **A component that is not in the world it draws in.** Hard-coded colours, absolute positions and
   left/right assumptions pass every test that does not switch theme or direction (sections 5.4, 6).
