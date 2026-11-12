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

The example below is the real `Badge` (`winia/src/components/badge.rs`), trimmed to the shape this
section is about - it drops the size tokens, the alignment policy and the content closure's body.

```rust
pub struct Badge {
    container_color: Option<Color>,
    content_color: Option<Color>,
    content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
}

impl Badge {
    pub fn new() -> Self {
        Self {
            container_color: None,
            content_color: None,
            content: None,
            modifier: Modifier::new(),
        }
    }

    /// Every option is a method that returns `Self` - the builder stays linear.
    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // Resolve BEFORE declaring: the resolved set is itself a parameter (section 3.2).
        let container = self.container_color.unwrap_or(WiniaTheme::colors().error);
        let content_color = self
            .content_color
            .or_else(|| WiniaTheme::colors().content_color_for(container))
            .unwrap_or_else(WiniaTheme::content_color);
        ctx.changed(&container);
        ctx.changed(&content_color);
        // ... default_min_size, the restartable group, the content closure ...
        let m = Modifier::new().background(container, Shape::Rectangle);
        let m = self.modifier.then(m); // caller OUTERMOST here - see section 2.3
        let _ = (ctx, m, self.content);
    }
}
```

Note what the options hold: `Option<Color>`, not a resolved `Color`. The component cannot resolve at
construction time, because `WiniaTheme::colors()` is only valid inside a composition - so `new` records
"unset" and `build` derives.

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
- **MUST** mark any FUNCTION that composes several children of its own — a demo body, a page builder, a
  `fn card(ctx, ..)` helper that builds a Stack — even though it is not a component. Without the macro
  the function gets no scope, its statements get no ids, and every key it creates derives from the same
  chain base with only the per-base counter separating the siblings: a structure change then shifts that
  counter (`docs/key-system-design.md` section 1.2) and the materialiser panics with `[dup-key]`.
  Measured on the text demo: opening the settings sheet did it one run in three until the attribute was
  added. The demo bodies under `examples/` are checked by
  `components::conventions::every_demo_body_is_composable`.
- **Why:** a `State::get()` written at the top of the content closure belongs to whoever composed
  it. With a scope in between it registers on the container instead, the parent Skips, and the
  change never re-runs the content - that is how the checkbox demo's select-all stopped
  propagating. Every other component that composes a restartable group IS `#[composable]` (39 of
  the 45 component files that do, measured by grepping for a real attribute line, not a mention in a
  comment), because its content is composed by the component, not passed through. The six without
  the attribute are the four animation containers (`animated_visibility`, `animated_size`,
  `animated_content`, `crossfade`) plus `chip` and `split_button`, and all six are on that
  document's exclusion list. Chip and split_button earn their place there on this bullet's own
  ground: their `label` / `leading` / `trailing` closures come from the caller and are passed
  through to the theme scope unchanged, so a `State::get()` at the top of one of them has to
  register on the parent container to re-run. The `remember`/`animate_*` state they do keep is the
  component's own and is consumed by the component. `start_restartable_group` opens a key group,
  not a composition scope - it calls `set_current_scope(false)` - so adding the attribute would
  interpose exactly the scope this bullet forbids. The full rule and the list are in
  `docs/key-system-design.md` section 7.1-7.2.

The distinction is about the closure, not the shape: a container that passes the caller's closure
through unchanged must not insert a scope; a container that composes its children itself (and
therefore has to declare its own parameters) takes one.

### 1.3 Content closures

| Signature | Use when |
| --- | --- |
| `impl Fn(&mut ComposeCtx)` | content may be composed more than once (rows, items, repeated slots) |
| `impl FnOnce(&mut ComposeCtx)` | exactly once, and the closure consumes a builder |
| `impl FnOnce(&mut RowScope<'_, '_>)` | the container hands the content a RECEIVER — `Row`/`Column`/`Stack` do this, and the scope derefs to `ComposeCtx` so a `\|ctx\| …` closure still compiles |
| `impl Fn(&mut ComposeCtx, T)` | the container hands the content a separate scope VALUE, not a receiver — `BoxWithConstraints` (the measured constraints) and `AnimatedContent` (the target) |
| `impl Fn(&mut ComposeCtx) + Send + 'static` | the content crosses into a window's own composer |

**MUST** take the narrowest one that works: `FnOnce` documents that the content is not retained.

The receiver form and the separate-value form are NOT interchangeable, and the difference is what
the caller can name. A receiver (`RowScope`) is reached through the closure's own parameter, which is
why it can deref to `ComposeCtx` and leave every existing `|ctx| …` site unchanged; a separate value
(`T`) is a second argument the closure must destructure, so those call sites always name both. Reach
for the receiver when the scope only carries child-facing modifiers, and for the separate value when
it carries DATA the content reads (`BoxWithConstraintsScope`'s constraints, `AnimatedContent`'s
target).

**A container that changes its content signature to a receiver must update the sites that forward a
stored closure**, because a `fn(&mut ComposeCtx)` cannot coerce to `FnOnce(&mut RowScope)`. Those
sites wrap it (`|scope| content(scope)`); the row/column/stack change measured 19 of them (12 in
`winia/src`, 7 under `examples/`).

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
elevated_button_elevation()` (`components/button.rs`, `impl ButtonDefaults`) is the Compose name and the Compose value; a component that inlines
`Shadow::new(0.0, 2.0)` is a token drift waiting to happen.

### 2.3 The caller's modifier goes LAST

```rust
let modifier = Modifier::new()
    .padding(padding)
    .background(container, shape)
    .then(self.modifier);   // the caller's last, so it can override
```

**MUST** append `self.modifier` at the end of the component's APPEARANCE chain, so a caller's own
`background`/`border` paints over the component's (measured: caller-last puts the caller's colour on
top, caller-first puts the component's). Four documented exceptions — three about what the component
still has to add afterwards, one about an element whose meaning depends on the constraints it
receives:

- **semantics** - a role declared before `then(user_modifier)` survives a caller adding their own
  description, which is why `docs/semantics.md` states that ordering as a rule.
- **input nodes** - every clickable component appends `clickable`/ripple AFTER the caller's modifier
  (`Button::build`: `modifier.then(self.modifier)` precedes `.clickable_with_source`; `Card::build`
  and `ListItem::build` have the same shape), because the
  interaction node has to be added by the component, not by the caller.
- **the drawing itself** - a leaf whose payload IS its content appends that payload after the caller's
  chain (`components/text.rs` ends its chain with `text_content_full`, `rich_text.rs` likewise): the
  payload is a DRAW element, so a caller's `background` has to stay earlier in the chain and paint
  beneath the glyphs. Pinned by
  `render_snapshot.rs::a_caller_background_paints_beneath_the_text`.
- **`default_min_size` (and every `default*Size` element)** - it applies ONLY where the incoming
  minimum is still 0 (`layout/node/measure/engine.rs`, `SizeStep::DefaultMin`), so a caller's
  `Modifier::width(2.0)` placed after it is ignored and the component's floor wins. Compose chains
  `modifier.defaultMinSize(size).background(...)`, i.e. the caller's modifier OUTERMOST, which is
  what makes `Badge(modifier = Modifier.width(2.dp))` measure 2. Measured both ways: floor-then-caller
  gives 6, caller-then-floor gives 2. `Badge` is the component that takes this exception
  (`components/badge.rs`), and it is the reason its chain reads `self.modifier.then(m)` — the price
  is that a caller's `background` paints BENEATH the badge's rather than over it. A component that
  wants both properties cannot have them from one chain order: pick the one its callers need.

### 2.4 An animatable parameter: pick the carrier by the phase that reads it

"Make this animatable" is not one decision. The value has to be read in the phase that consumes it,
and that phase fixes the type:

| The parameter is | Take | Because |
| --- | --- | --- |
| a layout LENGTH - size, padding, offset, spacing, border width | `impl Into<Dimension>` | read during measure (a border's width during paint), so a literal, a `State<f32>`, an `Animating<f32>` or a closure all work, and the animation costs a re-measure (or a repaint) instead of a recomposition |
| a SIZE MODE - auto / intrinsic (a fill is `fill_max_*`, not a size value) | `impl Into<SizeValue>` | not a number: changing it is a composition decision, so it cannot be evaluated per frame |
| a PAINT scalar - alpha, scale, rotation, a variable-font axis | `impl Into<AxisValue>` (see `Icon::wght`) | read while painting; `Dimension` would drag it through layout and re-measure for a number layout never uses |
| a PAINT colour or shape | the closure form the modifier already carries (`background(closure)`, `border(width, \|\| colour, shape)`) | evaluated at draw time |
| a value that decides STRUCTURE | a plain parameter; the caller reads a `State` and recomposes | only a composition can change the tree |

Rules that follow:

- **Prefer one `Into` parameter to a `*_dynamic` overload.** `Icon::wght(impl Into<AxisValue>)` and
  `Modifier::border(width, color: impl Into<BorderColor>, shape)` each take a literal, a state and a
  closure through one signature. A pair of methods - a static builder plus a `*_dynamic` twin -
  doubles the surface, and only one of the pair tends to get maintained.
- **Read it in the consuming phase, and read it UNTRACKED there.** A `Dimension` is read during
  measure with `get()` - that read IS the layout dependency, and dropping it would freeze the value.
  A closure evaluated at paint time must use `peek()`: paint runs with the dependency tracker off
  (`DEP_MODE` is `None`), so `get()` registers nothing today. That is the difference between a rule
  and a coincidence - the day a paint pass runs inside a transaction (a snapshot, a preview, a
  subcompose during measure), `get()` would attach the dependency to whatever slot happens to be
  active and recompose for a frame that changed nothing. Every draw closure in this repo
  (`graphics_layer`, `background`, `border`) reads with `peek()` for that reason.
- **One carrier, many names.** `AxisValue`, `BackgroundColor`, `BorderColor`, `BrushSource` and
  `GraphicsLayerSpec` are aliases of `graphics::Dyn<T>` (`Dyn<f32>`, `Dyn<Color>`, `Dyn<Brush>`,
  `Dyn<GraphicsLayerParams>`). Modifiers whose value is consumed at paint take `impl Into<Dyn<...>>`
  directly: shapes on `background`/`border`/`clip`/`shadow`/`drop_shadow`/`background_brush`/
  `ripple_with_shape`; the icon tint (`Icon::tint`, `Tint::Color`); the ripple colour and state-layer
  size; image `alpha` and `color_filter`; `blur`/`backdrop_blur` radius; `drop_shadow` params; and the
  layer convenience methods (`alpha`, `rotate`, `scale`, `rotation_x`/`rotation_y`,
  `camera_distance`, `shadow_elevation`, the two shadow colours). `eval()` and `same_parameter()`
  therefore have exactly one implementation, and a static value is stored inline — a constant
  background or layer parameter allocates no closure. A paint-time value read inside one of those
  modifiers uses `peek()` (see the phase bullet above).
- **Teach `element_param_eq` the new parameter.** A dynamic value is a PARAMETER, not a value: it
  must compare equal to another dynamic one (`same_parameter` on `Dimension` / `SizeValue` / `AxisValue`), or the
  component re-enters on every frame and can never Skip.
- **Test the pair, not the number.** Setting the state and asserting the element resolves the new
  value catches "frozen at build time"; asserting that two dynamic values compare equal catches
  "Skip broken". `modifier.rs`'s border-width and padding tests are the two shapes.
- **A new parameter on an existing leaf payload goes in its OWN element, not a new field.**
  `ModifierElement::TextContent` is built by hand in 72 places across the crate and the suite, so a
  new field is a 72-site edit that also breaks every handwritten literal at once; the text work added
  `TextPaint` (shadow, feature settings, draw style, brush, geometric transform) and
  `TextLayoutCallback` as separate elements instead, and the places that construct `TextContent` did
  not move. Reach for a new element whenever the payload is a struct other code constructs directly.

- **Not every number should animate.** A policy value - a touch-target floor, a padding derived from
  the measured content - stays static; making it dynamic invents a state the component cannot
  sensibly consume.


### 2.5 Carrier API migration (v2)

Paint-time parameters became `Dyn<T>` carriers across the v2 work. Call sites that pass literals
keep compiling; code that names the old types needs the carrier form, and `Tint` is no longer
`Copy`.

| Before | After |
| --- | --- |
| `IconSpec { tint: Option<Color> }` | `IconSpec { tint: Option<Dyn<Color>> }` |
| `Tint::Color(Color)`, `Tint: Copy` | `Tint::Color(Dyn<Color>)`, `Tint: Clone` |
| `Image::get_color_filter() -> Option<&ColorFilter>` | `-> Option<ColorFilter>` (the evaluated value) |
| `Modifier::image_content(.., alpha: f32, color_filter: Option<ColorFilter>, ..)` | `alpha: impl Into<Dyn<f32>>`, `color_filter: Option<Dyn<ColorFilter>>` |
| `Modifier::ripple(.., color: Color, ..)`, `ripple_with_shape`, `ripple_with_state_layer_size(.., diameter: f32)` | `Into<Dyn<Color>>` / `Into<Dyn<f32>>` |
| `Modifier::blur(f32)`, `backdrop_blur(f32)` | `impl Into<Dyn<f32>>` |
| `Modifier::drop_shadow(shape, ShadowParams)` | `shape: Into<Dyn<Shape>>`, `params: Into<Dyn<ShadowParams>>` |
| `Modifier::alpha/rotate/rotation_x/rotation_y/camera_distance/shadow_elevation(f32)`, `scale(f32, f32)`, `ambient_shadow_color/spot_shadow_color(Color)` | `Into<Dyn<f32>>` / `Into<Dyn<Color>>` |
| `ModifierElement::BorderDynamic` | deleted; `Border.color: Dyn<Color>` |

## 3. Composition identity: the part that breaks silently

### 3.1 Free keys, explicit keys

`#[composable]` injects a statement key into every statement that calls `ctx`, so a component normally
needs no key of its own. **MUST** use `ctx.key(id, |ctx| ...)` only when state has to survive a
branch that changes how many slots are consumed before it. For generated children, prefer the
loop forms: `ctx.keyed_range(prefix, count, |ctx, index| ...)` for an index range and
`ctx.keyed_for_each(items, |item| key(item), |ctx, item| ...)` for an arbitrary iterator. Their
keys identify the item, not its current position, so a reorder preserves the child's slots.

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
and `TextField` relies on that for its visual parameters (its `.text_field_visual(..)` chain step, with
the rest declared through `ctx.changed` at the top of `TextField::build`); everything the chain cannot
express - closures, flag-shaped branch controls, values passed outside the modifier - has to go through
`ctx.changed`, or a Clean slot Skips past it.

**A container whose CONTENT changes without any parameter changing** declares a SIGNATURE of that
content instead of a parameter: a lazy list compares the sequence of item KEYS (`docs/lazy-column.md`
section 2.2), because the item closures are registered inside the body and cannot be compared. The
signature is a normal `ctx.changed` parameter — what makes it special is that it is derived from the
data, not carried by the builder, so a caller cannot pass it wrong.

**The caller has the same obligation**, and it is easier to get wrong there: a value read in the
parent and captured into a container's content closure is invisible to that container's Skip
decision. The parent re-runs (a sibling caption updates) while the captured content stays behind -
measured on a manual page indicator: `let current = state.current_page(); Row::new().build(ctx, |ctx|
... current ...)` kept highlighting dot 1 while the caption next to it said page 3. Read the state
INSIDE the closure that has to re-run, or give the container a parameter to compare.

The same trap in a different shape: a `State::get()` inside a container whose body is a restartable
group makes THAT group a dependent, which is usually what you want - but only if the read happens
there rather than in the caller.

The framework's invalidation is deliberately one level deep (`composer.rs`, and
`docs/key-system-design.md` section 2): a write marks the slot that read it plus its direct children
when the reader is a scope. So a state read two groups above the consumer reaches the intermediate
group but not the consumer's own - the consumer has to read it itself, or the intermediate has to
`mark_subtree_dirty`. And `ctx.changed` is only available for a value the container compares
(`PartialEq + Clone`), which a content closure is not: a container with no comparable parameter
cannot take the value at all, so reading it inside the consuming group is the only local fix.

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
pub fn remember_date_picker_state(ctx: &mut ComposeCtx) -> DatePickerState; // the real pattern

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

**Where the pieces live.** `winia/src/components/component_styles.rs` holds the MECHANISM only: the generic
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
system theme - otherwise the components keep the colours they composed with. Section 13.4 is the
general rule, with the two steps such a provider has to take.

## 7. Slots

Optional named content goes in `Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>` (the shape the
real slots use, e.g. `TextField`'s `placeholder` field) with a builder
method per slot (`navigation_icon`, `actions`, `subtitle`). **MUST** give a slot the M3 geometry it
has in Compose (touch slot, inset, alignment) instead of letting the content decide.

- Text slots **SHOULD** be wrapped in `ProvideTextStyle(...)` so the style is the component's,
  overridable by the caller, not baked into the content's `Text`.
- Slots that take a colour **SHOULD** be wrapped in `WiniaTheme::with_content_color(...)` so an `Icon`
  or `Text` inside inherits it.
- A slot that changes how much space another slot gets (a `subtitle` that changes the expanded height)
  is a **parameter**, not a hidden consequence: it has to be part of the `ctx.changed` list.
- A slot that needs any OTHER ambient value (density, direction, a motion scheme, a custom local)
  **SHOULD** get it from a provider too. Section 13 is the catalogue, the merge vs replace rule, and
  the one way a provider silently stops working.

## 8. Interaction and focus

- A click target **MUST** use `Modifier::clickable` (which claims semantics automatically) and a
  `MutableInteractionSource` when pressed/hovered/focused appearance differs from the resting one.
- A disabled component **MUST NOT** silently drop its semantics: a disabled button has no `clickable`,
  so it has to claim its label explicitly or the accessibility tree loses its name
  (`docs/semantics.md`, rule 1).
- A focusable component **MUST** obtain a `FocusRequester` and attach it with
  `Modifier::focus_requester(..)` (the requester's own `request_focus()` is what moves focus);
  `TextField` is the model to copy (its `.focus_requester(&fr)` chain step). Focus has to survive recomposition: the runtime
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

| Level | Where | What it covers |
| --- | --- | --- |
| Unit | `#[cfg(test)] mod tests` in the component file | builder defaults, measured size and positions, colour selection, the `ctx.changed` behaviour |
| Fixture | `winia/tests/ui_fixtures/fixture_all.rs` (one bin, scenario by `argv[1]`) | a demo compiled in through `#[path]`, so the UI test drives the same tree a human runs |
| UI | `winia/tests/ui_test.rs` (`--features debug-server`) | clicks by test tag, the debug tree's tags/texts/sizes, the animation registry, a layout-direction override |

The UI harness can do more than the suite currently asks of it: `tests/ui/support.rs` also exposes
`key`/`key_until`, `semantics`/`semantics_entries`, `pixel`/`pixels_at_logical`, `drag`/`fling_inside`,
`scroll` and `resize`. That is CAPABILITY, not coverage - `ui_test.rs` is four demo-behaviour tests
(flow features and reflow, item animation, flex layout) and exercises none of the key, semantics or
pixel readers today. Treat those as available tools for a new test, not as existing proof.

**MUST** lay a demo's control row out as a `FlowRow`, not a `Row`: the UI test clicks a button by its
reported coordinates, and a fixed row pushes the tail of a long button run outside the card, where a
click reaches nothing. Measured on `lazy_item_animation_demo`: five buttons in a 520 dp window put the
fifth at x = 497 with a `Row` (off the card) and wrap it to a second line with a `FlowRow`.

**MUST** write the unit test against the arena, not against a rendering (a split module keeps
the same rule in its `<module>/tests.rs`, as `layout/flow/tests.rs` does):

```rust
let mut composer = Composer::new();
composer.compose(|ctx| TopAppBar::large(|ctx| ...).build(ctx));
composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
let root = composer.layout_root_idx().unwrap();
assert_eq!(composer.arena_nodes()[root].measured_size.height, 152.0);
```

**SHOULD** name a test for the property it pins, so a `... FAILED` line explains itself: a failing
`a_named_size_takes_its_own_padding_and_the_default_takes_the_baseline` says what broke, while
`test_button_sizes` says only which file to open. Phrase it in snake_case, as a statement about
behaviour rather than about the thing being tested.

The testable question is **does the name state the BEHAVIOUR (what must be true) or only the SUBJECT
(what it touches)** — not whether it has a prefix. A `test_` prefix carries no meaning either way:
`test_outlined_border_uses_theme_outline` names its property and is a perfectly good name, and
`button_size_dimensions` has no prefix and tells a reader nothing. Renaming a test purely to drop the
prefix is churn, and the earlier version of this rule asked for exactly that; it was wrong. Rename a
test when its NAME cannot explain a failure, whatever it starts with.

(The "cargo prints `test <name>` already" argument for banning the prefix does not survive contact
with the suite either: inside `src/`, cargo prints the module path between the two —
`test components::button::tests::test_button_defaults` — so the doubling only exists for the
integration tests under `winia/tests/`, and there it is one redundant word, not a cost.)

**MUST** hold `crate::animation::tests::TEST_SERIAL` and clear the registry in any test that pushes an
animation (`push_animatable`, `update_animations`, `clear_all_animations`); the suite relies on
serialisation that only works if every animation-driving test takes it (see
`docs/developer-guide.md` section 8.4).

## 11. Registering

1. `winia/src/components/<snake_case>.rs`
2. `pub mod <snake_case>;` in `winia/src/components.rs`, next to the components it belongs with
3. the public types in the prelude's component block in `winia/src/lib.rs`
4. `docs/<kebab-case>.md` (section 12) and, if it is a control, a row in `docs/semantics.md`

A LAYOUT-LEVEL container (`Row`, `Column`, `Stack`, `Flow*`, the lazy lists) lives in
`winia/src/layout/` instead: the module is declared in `winia/src/layout.rs` and its types are
re-exported from there and from the prelude. Steps 2-3 are the same obligation, a different file.

## 12. Documentation

Every NEW component ships `docs/<component>.md` with these sections, in this order (existing docs are
converted when they are edited for another reason - 4 of the 110 docs have all six, measured
2026-11-10: `docs/alert-dialog.md`, `docs/pager.md`, `docs/scrollbar.md` and `docs/text.md`).
Layout primitives (`Column`, `Row`, `Stack`, `FlowRow`, `FlowColumn`) are not components in this
sense, and neither are the container families built on them (the lazy lists, `docs/lazy-column.md`):
their docs use the shorter Sources / API and semantics / Differences / Tests shape, of which
`docs/flow-layout.md` is the model.

1. **Sources** - the Compose files it mirrors (paths in `reference/`), so the next reader can diff
   behaviour instead of guessing.
2. **Variants** - one table: constructor, height, when to use it.
3. **Layout** - how it measures and places, in words, including the RTL behaviour.
4. **Behaviour** - state, interaction, what changes and when. Name the tokens.
5. **Differences from Compose** - deliberate ones, with the reason. Silence here is a claim of parity.
6. **Tests** - the commands to run it, and the fixture scenario name.

## 13. Providers: `ProvideTextStyle`, `CompositionLocal`, and the rest

A component that composes CALLER content usually owes that content some context: a text style, a
content colour, a density, a direction. Pushing it down is what a provider is for, and "set it on my
own leaf instead" is the mistake this section exists to prevent — the caller's content is composed
inside the component, so the component already knows what its labels should look like and the
caller does not.

**MUST** provide context rather than restating it per leaf. The failure is silent and only shows up
in the combination: `ButtonGroupItem::clickable` set material3's four label settings on its own
`Text`, and a hand-built item set only the two its author remembered. They drifted, and pressing a
neighbour cut "Week" to "We" in one row while the identical label beside it stayed whole
(`docs/button-group.md`).

Two things follow, and the rest of this section is those two:

1. **Use an existing provider when one fits** — §13.1 is the catalogue, and `ProvideTextStyle` is
   the one a component reaches for most.
2. **Write a `CompositionLocal` when none does** — §13.3. A component MAY own a local of its own
   (a chip's density, a list's row height, a card's tonal step); what it MUST NOT do is thread the
   value through every leaf's parameter list instead, because the leaf is often the CALLER's.

### 13.1 The providers that already exist

Reach for one of these before writing anything new:

| Provider | Provides | Notes |
| --- | --- | --- |
| `ProvideContentColorTextStyle(color, style, ctx, \|ctx\| …)` | `LocalContentColor` + `LocalTextStyle` | **prefer this** for a label slot: colour and style travel together (§13.2) |
| `ProvideTextStyle(style, ctx, \|ctx\| …)` | the subtree's `TextStyle` | **MERGED**, not replaced (§13.2) |
| `WiniaTheme::with_content_color(color, ctx, \|ctx\| …)` | `LocalContentColor` | what an `Icon` / `Text` inherits |
| `WiniaTheme::with_theme*` (four overloads) | colours + typography + direction | see the warning in §13.4 |
| `WiniaTheme::with_typography(type, ctx, \|ctx\| …)` | the type scale only | leaves colours and direction alone |
| `WiniaTheme::with_motion_scheme(scheme, ctx, \|ctx\| …)` | the springs | how an app switches every animation at once |
| `WiniaTheme::with_tonal_elevation_enabled(bool, ctx, \|ctx\| …)` | `Surface`'s tonal overlay | |
| `ctx.provide(&[A.provided(x), B.provided(y)], \|ctx\| …)` | several locals at ONE site | Compose's `CompositionLocalProvider(A provides x, B provides y)`; one call = one slot position |
| `with_component_styles_when_changed(styles, ctx, \|ctx\| …)` | the per-class styling registry | `_arc` / plain variants in §13.5 |
| `with_density(density, \|…\|)` | `Density` | no `ctx` — a plain closure |
| `layout::direction::provides(direction, \|…\|)` | `LayoutDirection` | no `ctx` either; `direction::provided` is its tracked form |

**SHOULD** wrap a text slot in `ProvideContentColorTextStyle(...)` when it carries both a colour and a
style, and in `ProvideTextStyle(...)` when it only carries a style — so the styling is the
COMPONENT's and the caller can still override it, rather than baking the settings into the content's
own `Text`. **SHOULD** wrap a colour-only slot in `WiniaTheme::with_content_color(...)` for the same
reason.

**MUST** prefer one `ctx.provide` call over nesting two single-value providers in one statement:
nesting spends two slot positions on what is one decision, and the two then have to agree about which
ran first.

### 13.2 `ProvideTextStyle` merges; most providers replace

`ProvideTextStyle` is Compose's `ProvideTextStyle`, which **merges field by field** with what it
inherits:

```rust
let merged = merge_text_styles(&LOCAL_TEXT_STYLE.current(), &style);
LOCAL_TEXT_STYLE.provide(merged, ctx, |ctx| content(ctx));
```

Every field resolves `override_.field.or(base.field)`, so a nested provider that names only
`letter_spacing` keeps the outer size, weight, alignment and so on. That is what makes the layering
work: a component provides the label RULES, and `Button` inside it provides the theme's
`label_large` (size, line height, spacing, weight, colour) without erasing them. A `Text` then
resolves `self.field.or(style.field)` (`winia/src/components/text.rs`), so an explicit
`.max_lines(2)` on one label still beats the provided value.

Every OTHER `CompositionLocal` provider replaces wholesale — `provides(value, …)` pushes a value and
`current()` returns the innermost one. There is no field-level merge unless the value's own type
does it: `TextStyle` does (§13.2 above) and `ComponentStyles` does it per COMPONENT CLASS
(`merge` puts `other`'s entry on top and keeps every other class), while a bare `Color` cannot.

### 13.3 Writing a new one

```rust
use std::sync::LazyLock;
use crate::runtime::composition_local::CompositionLocal;

// 1. ONE static per local. `CompositionLocal::new` allocates a unique id, so calling it twice
//    makes two DIFFERENT locals that never see each other's values.
pub(crate) static LOCAL_FOO: LazyLock<CompositionLocal<Foo>> =
    LazyLock::new(|| CompositionLocal::new(Foo::default));

// 2. A provider function, named after Compose's, taking the content closure LAST.
pub fn with_foo(foo: Foo, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    LOCAL_FOO.provides(foo, || content(ctx));
}

// 3. A reader. `current()` falls back to the default; `try_current()` returns Option and does NOT
//    call the default — use it when "nobody provided" must be distinguishable from "provided the
//    default" (Text's selection registrar does exactly that).
pub fn current_foo() -> Foo { LOCAL_FOO.current() }
```

**MUST** hold the local in a `LazyLock` (or a `static`). A `CompositionLocal` created per call gets
a fresh id every time, so the provider and the reader end up on different locals and the value never
arrives — with no compile error and no panic.

**MUST** give the provider the `ctx`-then-content shape when the content is composed in this
composition (`impl FnOnce(&mut ComposeCtx)`), and the `ctx`-less shape (`impl FnOnce() -> R`) for a
pure value provider such as `with_density`. Do not mix the two in one family.

### 13.4 `provide` invalidates; `provides` does not — pick by whether the value moves

Two verbs, one meaning each:

| Form | Signature | Invalidates its readers? |
| --- | --- | --- |
| `LOCAL.provide(value, ctx, \|ctx\| …)` | takes `ctx` | **yes** — compares against the last frame's value and notifies the scopes that read it |
| `ctx.provide(&[A.provided(x), B.provided(y)], \|ctx\| …)` | takes `ctx` | **yes**, per entry, under each local's own comparison |
| `LOCAL.provides(value, \|…\|)` | ctx-less | **no** — decides only what the NEXT reader sees |
| `LOCAL.provided(value)` | — | nothing at all: it BUILDS an entry for `ctx.provide` |

**MUST** use `provide` for anything that can change at runtime. The ctx-less `provides` is for the
handful of values installed once or wrapping a whole frame (§13.4.2).

#### 13.4.1 How the invalidation works

A provided entry carries a `StateSignal`, and reading the local subscribes to it —
`CompositionLocal::current()` hands the signal to `register_dependency`, the same call `State::get`
makes. So the scopes that re-run are exactly the ones that READ the value. Nothing else has to
happen: no component writes invalidation code, and no subtree is dirtied wholesale because "we do not
know who read it".

Two consequences a component author has to know:

1. **A reader must actually READ.** A group that never calls `current()` has nothing to wake. This is
   not a burden — reading is what the component was doing anyway — but it does mean a test that
   provides a value and never reads it proves nothing about invalidation.
2. **The move lands one pass later than you might expect.** `Composer::compose` drains the pending
   notification queue at its START, before any content runs; a `notify` raised while content is
   running can therefore only be acted on by the NEXT pass. The frame path loops for this reason
   (`PerWindow::recompose_until_stable`), so a frame never PRESENTS the intermediate state. In a unit
   test that means: after changing a provided value, converge (`Composer::recompose_to_stable` under
   `cfg(test)`, or your own loop over `recompose` and `has_pending_states`) rather than calling
   `recompose` once and asserting.

The old advice — compare by hand, then `ctx.mark_subtree_dirty()` — is gone. `provide_resolved`, the
`Button`/`Badge` label colour, and `DisableSelection` each carried a hand-rolled copy of it; each is
now one `provide` call.

#### 13.4.2 The four providers that keep the ctx-less `provides`

- `runtime::density::with_density` wraps a whole FRAME, not a composition: `per_window/core.rs`
  opens it around `recompose_until_stable()` + `layout()` + `sw.draw(...)` in ONE closure. It cannot
  take `&mut ComposeCtx` — the caller holds the composer across that span, so threading the ctx in is
  a borrow conflict, not a style choice. And composition invalidation would be the wrong lever even
  if it fit: the readers that matter for `density` run in MEASURE and DRAW
  (`modifier/element.rs`'s `DrawNode::draw`, `render/pass.rs`'s clip slack), which no composition
  invalidation reaches. Tracking it would promise something it cannot deliver.
- `layout::direction::provides` is the ctx-less form for a test that supplies a direction around a
  bare `Composer::compose`, and for a caller that wants to wrap a frame. Where a direction can
  CHANGE, use the tracked entry instead: `WiniaTheme::provide_resolved` puts
  `layout::direction::provided(direction)` into its `ctx.provide` call for exactly that reason. Until
  recently direction rode inside a hand-compared tuple in that function; leaving it on the ctx-less
  form would have silently dropped direction-change invalidation.
- `saveable::with_saveable_registry` provides a per-window `Arc` installed once and never rebuilt;
  `component_styles::with_component_styles` has no production call site at all (a different, non-`Arc`
  form for callers that never change the set — its changing twin is
  `with_component_styles_when_changed`). A signal over an unchanging value is pure per-frame cost.

The rule that survives all of it: **if the value changes at runtime AND its readers compose, use
`provide`. If either half is false, the tracking is either impossible or useless.**

**One more trap, specific to `WiniaTheme::with_theme`:** it forwards to
`with_theme_typography_and_direction(colors, Typography::default(), LayoutDirection::Ltr, …)`, so
nesting it inside a themed subtree RESETS the typography and the direction to the defaults. Wrap a
component that needs its own palette in `with_theme_and_direction` (passing the direction through),
or change the colours some other way.

### 13.5 A value with no usable `PartialEq`

The comparison lives on the LOCAL, not at the call site — `CompositionLocal::new_by(default, same)`
takes it once, and every `provide` of that local uses it. Two framework examples:

- `LOCAL_OVERSCROLL_FACTORY` compares by `Arc::ptr_eq`, because `Arc<dyn Fn>` has no `PartialEq` that
  means anything;
- `LOCAL_STYLES` does the same, so `with_component_styles_when_changed` compares by IDENTITY: the
  caller must rebuild the set only when it really changes. A set reconstructed every frame is a
  different set every frame, and will re-compose its readers every frame.

**SHOULD** reach for `new_by` rather than wrapping a plain local in a hand-written comparison. A
`same` that answers `false` too eagerly re-runs readers every frame; one that answers `true` too
readily leaves them on a stale value with no way to tell. Compare by the identity that actually
distinguishes two states.

**Do not confuse this with `new_static`.** `new_static` is Compose's `staticCompositionLocalOf`: the
value still moves (a reader that composes later for its own reasons must see the CURRENT one), but no
reader is ever woken. It is for a handle installed once and never rebuilt, not an optimisation for a
value that changes.

`CompositionLocal::provide` also keeps its own memory per SITE (a per-site key over the composition
position), so two providers in one statement do not overwrite each other's remembered value — which
is what lets `ProvideTextStyle` nest inside another `ProvideTextStyle` and each keep its own. That key
is load-bearing: pinning it to a constant makes two providers in one group share one entry, read each
other's value as "changed", and re-dirty on every pass.

## 14. Checklist

Copy this into the pull request and tick it there:

```text
[ ] builder struct, private fields, new() + one method per option returning Self
[ ] #[composable] decided by section 1.2, not by habit
[ ] every parameter that can change the content is in a ctx.changed call BEFORE the group starts
     (partly enforced: components::conventions::every_container_declares_its_parameters checks that a
     file composing a restartable group declares at least one parameter - per FILE, so a second group
     or a wrapper container in the same file is not covered)
[ ] ctx.key only where a branch can change the slot layout
[ ] ctx.remember without a nested State; the right handle shape for the kind of state
     (enforced: components::conventions::no_component_nests_a_state_in_remember)
[ ] a parameter that should animate takes the carrier its consuming phase reads - Dimension for a
     length, AxisValue / a closure for a paint value, NEVER Dimension for a draw-only number - and a
     dynamic value compares equal in element_param_eq
[ ] no rt.spawn in a component - effect hooks own spawning
     (enforced: components::conventions::no_component_spawns_its_own_task)
[ ] hoisted state + remember_x_state when the caller must drive it
[ ] remember_derived_state for a threshold over a continuous source
[ ] coloured / styled slots get their context from a PROVIDER (ProvideContentColorTextStyle,
     ProvideTextStyle, with_content_color, a CompositionLocal) rather than from each leaf's own
     parameters (section 13)
[ ] a provider whose value can change at runtime uses CompositionLocal::provide (or one
     ctx.provide call for several locals) - NOT the ctx-less provides, which invalidates nobody;
     a value without a usable PartialEq gets CompositionLocal::new_by instead of a hand-written
     comparison (sections 13.4, 13.5)
[ ] self.modifier appended last, semantics declared before it
[ ] colours and text styles from WiniaTheme; token constants named after the Compose token
[ ] M3 geometry: 48dp touch targets, insets, RTL mirroring
[ ] role + state declared, merge_descendants for a single-control component
[ ] disabled state keeps the semantics
[ ] unit tests over the arena, animation tests take TEST_SERIAL
[ ] docs/<component>.md with the six sections, and a semantics.md row if it is a control
[ ] registered in components.rs and the prelude
```

## 15. The three failures this document exists to prevent

1. **A parameter that never reaches the content.** Missing `ctx.changed` -> the container Skips past
   the change forever, and it looks like a broken State chain (section 3.2).
2. **A scope in the wrong place.** `#[composable]` on a container -> the parent Skips and the content
   stops reacting to state it reads (section 1.2).
3. **A component that is not in the world it draws in.** Hard-coded colours, absolute positions and
   left/right assumptions pass every test that does not switch theme or direction (sections 5.4, 6).
