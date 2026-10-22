//! Semantics: what a node *is*, for a screen reader and for tests.
//!
//! Compose models this as a second tree ("the semantics tree") built from per-node semantics
//! properties, separate from both the composition and the layout tree. winia's version: a
//! [`crate::modifier::Modifier::semantics`] element declares role / name / state on a node, and
//! [`semantics_tree`] resolves those declarations into a tree at query time — no caching, no
//! invalidation, so a caller can ask once per frame (or once per platform request) and get the
//! current answer.
//!
//! Two rules do most of the work:
//!
//! - **A node that claims its subtree absorbs it.** A click target — or a node that asks for
//!   [`SemanticsConfig::merge_descendants`] explicitly — becomes ONE node whose name is the text it
//!   contains, so a button reads as "Submit" instead of as an unnamed button with a text child.
//!   Compose's `clickable` does the same inside foundation, which is why `Button { Text("Submit") }`
//!   is one node there too.
//! - **A node that declares nothing and claims nothing is transparent.** It does not appear at all;
//!   only its descendants do. A `Column` is therefore invisible to a screen reader, as it should be.
//!
//! Deliberate limits of this first slice (see `docs/semantics.md`): one tree rather than Compose's
//! (merged + unmerged) pair, a small property set — role, name, state, and the click action that is
//! already implied by the modifier chain — and no collections, custom actions or live regions.

use crate::layout::node::{scroll_offset_for_node, LayoutNode};
use crate::modifier::ModifierElement;
use crate::selection::ToggleableState;

// ═══════════════════════════════════════════════════════════
// Role
// ═══════════════════════════════════════════════════════════

/// What a control is — Compose's `Role`.
///
/// Only the roles with a consumer today are listed. A node with no role but a name is static text;
/// that is how `Text` ends up in the tree without naming a role of its own, matching Compose (where
/// `Text` sets the `Text` property and no role).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticsRole {
    Button,
    Checkbox,
    Switch,
    RadioButton,
    Tab,
    Image,
    ProgressBar,
    Dialog,
    /// Compose's `Role.DropdownList` — the node that a dropdown list (an exposed dropdown menu's anchor)
    /// hangs off, so a reader announces "dropdown list" rather than "button". material3 publishes it on a
    /// primary anchor (`ExposedDropdownMenu.kt:1472-1474`) and `Role.Button` on a secondary one (`:1469-1471`).
    DropdownList,
}

impl SemanticsRole {
    /// Stable lowercase name — the spelling used in the debug tree JSON and by tests.
    pub fn name(self) -> &'static str {
        match self {
            SemanticsRole::Button => "button",
            SemanticsRole::Checkbox => "checkbox",
            SemanticsRole::Switch => "switch",
            SemanticsRole::RadioButton => "radiobutton",
            SemanticsRole::Tab => "tab",
            SemanticsRole::Image => "image",
            SemanticsRole::ProgressBar => "progressbar",
            SemanticsRole::Dialog => "dialog",
            SemanticsRole::DropdownList => "dropdownlist",
        }
    }

    /// Parse a name produced by [`SemanticsRole::name`].
    pub fn from_name(name: &str) -> Option<Self> {
        [
            SemanticsRole::Button,
            SemanticsRole::Checkbox,
            SemanticsRole::Switch,
            SemanticsRole::RadioButton,
            SemanticsRole::Tab,
            SemanticsRole::Image,
            SemanticsRole::ProgressBar,
            SemanticsRole::Dialog,
            SemanticsRole::DropdownList,
        ]
        .into_iter()
        .find(|role| role.name() == name)
    }
}

// ═══════════════════════════════════════════════════════════
// State
// ═══════════════════════════════════════════════════════════

/// A node's accessibility state. Every field is optional: `None` means "nothing to say", which is
/// not the same as `Some(false)` — a node that never mentions selection must not be announced as
/// unselected (Compose's semantics keys behave the same way).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SemanticsState {
    selected: Option<bool>,
    checked: Option<ToggleableState>,
    expanded: Option<bool>,
    enabled: Option<bool>,
    /// What is wrong with the value, in the words the field itself uses — Compose's
    /// `SemanticsProperties.error`, which a text field sets to its supporting text when that text
    /// is an error. A reader announces it in place of the value rather than making the user find
    /// the message on the screen. `None` means the value is not in error, which is different from
    /// being in error with nothing to say.
    error: Option<String>,
    /// A progress value with its range — Compose's `ProgressBarRangeInfo`. `None` on an
    /// indeterminate indicator: there IS no value, which is different from a value of zero, and a
    /// screen reader should say "in progress" rather than "0 percent".
    progress: Option<(f32, f32, f32)>,
}

impl SemanticsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Selected — radio buttons, tabs, single-choice segmented buttons (Compose `Selected`).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Checked — checkboxes, switches, multi-choice segmented buttons (Compose `ToggleableState`).
    pub fn checked(mut self, checked: ToggleableState) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Convenience for the common two-state case.
    pub fn checked_bool(self, checked: bool) -> Self {
        self.checked(ToggleableState::from_bool(checked))
    }

    /// Expanded — disclosure rows and expanding panels (Compose `Expanded`).
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    /// Disabled. Compose only ever publishes `Disabled`, never "enabled"; winia keeps the boolean
    /// explicit so a caller can revert a disabled node to enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// What is wrong with this node's value, in the words the node itself uses — Compose's
    /// `SemanticsProperties.error`. A text field sets it to the message it draws under itself, so a
    /// reader announces the problem in place of the value instead of leaving the user to find the
    /// message on the screen.
    pub fn error(mut self, message: impl Into<String>) -> Self {
        self.error = Some(message.into());
        self
    }

    /// The error message, if this node is in error.
    pub fn error_value(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// A progress value and the range it sits in — what a screen reader announces as a percentage,
    /// and the reason a progress bar without it is just an unnamed shape. The range is explicit
    /// because a caller's units are their own (bytes, steps, seconds).
    pub fn progress(mut self, current: f32, min: f32, max: f32) -> Self {
        debug_assert!(max > min, "a progress range needs max > min, got {min}..{max}");
        debug_assert!(
            (min..=max).contains(&current),
            "the progress value {current} is outside {min}..{max}"
        );
        self.progress = Some((current, min, max));
        self
    }

    /// The progress value and range, if this node reports one.
    pub fn progress_value(&self) -> Option<(f32, f32, f32)> {
        self.progress
    }

    pub fn selected_value(&self) -> Option<bool> {
        self.selected
    }

    pub fn checked_value(&self) -> Option<ToggleableState> {
        self.checked
    }

    pub fn expanded_value(&self) -> Option<bool> {
        self.expanded
    }

    pub fn enabled_value(&self) -> Option<bool> {
        self.enabled
    }

    /// Whether this state says nothing at all.
    pub fn is_unspecified(&self) -> bool {
        self.selected.is_none()
            && self.checked.is_none()
            && self.expanded.is_none()
            && self.enabled.is_none()
            && self.error.is_none()
            && self.progress.is_none()
    }

    /// Fill in every field this state leaves open from `fallback` — the merge direction: a node's own
    /// statement wins, and only the gaps are answered by what it absorbed.
    fn or(self, fallback: SemanticsState) -> SemanticsState {
        SemanticsState {
            selected: self.selected.or(fallback.selected),
            checked: self.checked.or(fallback.checked),
            expanded: self.expanded.or(fallback.expanded),
            enabled: self.enabled.or(fallback.enabled),
            error: self.error.or(fallback.error),
            progress: self.progress.or(fallback.progress),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Declaration
// ═══════════════════════════════════════════════════════════

/// One named action beyond the primary click — Compose's `CustomAccessibilityAction`.
///
/// The closure is runtime state (rebuilt every composition), so equality compares the LABEL: two
/// declarations that offer the same action name are the same parameter, exactly like a click
/// callback.
#[derive(Clone)]
pub struct SemanticsAction {
    label: String,
    action: std::sync::Arc<dyn Fn() + Send + Sync>,
}

impl SemanticsAction {
    pub fn new(label: impl Into<String>, action: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            label: label.into(),
            action: std::sync::Arc::new(action),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Run the action — what a platform bridge calls when the user invokes this item.
    pub fn invoke(&self) {
        (self.action)();
    }
}

impl std::fmt::Debug for SemanticsAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticsAction").field("label", &self.label).finish_non_exhaustive()
    }
}

impl PartialEq for SemanticsAction {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
    }
}

/// What a node declares about itself, carried by `Modifier::semantics`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SemanticsConfig {
    role: Option<SemanticsRole>,
    content_description: Option<String>,
    state: SemanticsState,
    merge_descendants: bool,
    live_region: LiveRegionMode,
    /// Compose's `SemanticsProperties.SelectableGroup`: this node is a GROUP of selectable children (radio
    /// buttons, tabs), not a selectable thing itself. A reader announces the group as a collection and lets the
    /// user move inside it, which is why the flag has to survive on the tree rather than being folded away.
    selectable_group: bool,
    /// Named actions beyond click — Compose's `customActions`.
    custom_actions: Vec<SemanticsAction>,
}

impl SemanticsConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark this node as a group of selectable children — Compose's `Modifier.selectableGroup()`
    /// (`foundation/selection/SelectableGroup.kt:30`, the property at `ui/semantics/SemanticsProperties.kt:67`).
    pub fn selectable_group(mut self) -> Self {
        self.selectable_group = true;
        self
    }

    /// Whether this node declares itself a selectable group.
    pub fn is_selectable_group(&self) -> bool {
        self.selectable_group
    }

    pub fn role(mut self, role: SemanticsRole) -> Self {
        self.role = Some(role);
        self
    }

    /// The name to announce, when it is not the node's own text. Compose's `contentDescription`.
    pub fn content_description(mut self, description: impl Into<String>) -> Self {
        self.content_description = Some(description.into());
        self
    }

    /// Merge state field-wise, like the `SemanticsState` setters themselves, so a second call adds
    /// to the first rather than discarding it.
    pub fn state(mut self, state: SemanticsState) -> Self {
        self.state = state.or(self.state);
        self
    }

    /// Claim the subtree: this node becomes one accessibility element and its descendants are
    /// absorbed into it. Implied by a click target, so a button never needs to ask.
    pub fn merge_descendants(mut self, merge: bool) -> Self {
        self.merge_descendants = merge;
        self
    }

    pub fn role_value(&self) -> Option<SemanticsRole> {
        self.role
    }

    pub fn content_description_value(&self) -> Option<&str> {
        self.content_description.as_deref()
    }

    pub fn state_value(&self) -> SemanticsState {
        self.state.clone()
    }

    pub fn merges_descendants(&self) -> bool {
        self.merge_descendants
    }

    /// Mark this element as a live region — Compose's `liveRegion = LiveRegionMode.Polite`.
    ///
    /// A live region is content that changes WITHOUT the user moving focus to it, and that a screen
    /// reader should therefore announce when it does: a snackbar appearing, a status line updating, a
    /// validation message. This is the one case where a component expects to be spoken unprompted.
    ///
    /// [`LiveRegionMode::Polite`] waits for the reader to finish what it is saying; `Assertive`
    /// interrupts. A snackbar is polite — it is information, not an alarm.
    pub fn live_region(mut self, mode: LiveRegionMode) -> Self {
        self.live_region = mode;
        self
    }

    /// This element's live-region mode.
    pub fn live_region_mode(&self) -> LiveRegionMode {
        self.live_region
    }

    /// Add a named action beyond the primary click — Compose's `customActions` in the semantics
    /// block. A platform bridge lists the labels and calls [`SemanticsAction::invoke`].
    pub fn custom_action(
        mut self,
        label: impl Into<String>,
        action: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let label = label.into();
        if !self.custom_actions.iter().any(|a| a.label == label) {
            self.custom_actions.push(SemanticsAction::new(label, action));
        }
        self
    }

    pub fn custom_actions(&self) -> &[SemanticsAction] {
        &self.custom_actions
    }

    /// Fill every field this config leaves open from `fallback` — how a caller's declaration is
    /// folded into the one the component already made: what the caller sets wins, and what it does
    /// not mention keeps the component's value, so adding a content description does not wipe the
    /// role. Compose merges semantics properties per key the same way.
    pub fn or(self, fallback: SemanticsConfig) -> SemanticsConfig {
        SemanticsConfig {
            role: self.role.or(fallback.role),
            content_description: self.content_description.or(fallback.content_description),
            state: self.state.or(fallback.state),
            // A claim cannot be un-made: `merge_descendants(false)` is the default, so `true` from
            // either side stands.
            merge_descendants: self.merge_descendants || fallback.merge_descendants,
            // Same rule: `Off` is the default, so the loudest declaration wins.
            live_region: self.live_region.max(fallback.live_region),
            // And a group is a group: `false` is the default, so a `true` from either side stands.
            selectable_group: self.selectable_group || fallback.selectable_group,
            // Actions accumulate, deduped by label: the caller's declaration adds to the
            // component's, and declaring the same label again keeps the first closure.
            custom_actions: {
                let mut actions = self.custom_actions;
                for action in fallback.custom_actions {
                    if !actions.iter().any(|a| a.label == action.label) {
                        actions.push(action);
                    }
                }
                actions
            },
        }
    }
}

/// How urgently a live region's changes should be announced — Compose's `LiveRegionMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LiveRegionMode {
    /// Not a live region (the default): nobody announces this without focus.
    #[default]
    Off,
    /// Announced when the reader is free — information (a snackbar, a status line).
    Polite,
    /// Announced immediately, interrupting — an alert.
    Assertive,
}

// ═══════════════════════════════════════════════════════════
// Resolved tree
// ═══════════════════════════════════════════════════════════

/// One accessibility element: the resolved form of a node's declaration plus whatever it absorbed.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticsNode {
    /// The `LayoutNode` id this came from — what [`click_node_by_id`] needs to fire the action.
    pub node_id: u64,
    pub role: Option<SemanticsRole>,
    pub name: Option<String>,
    pub state: SemanticsState,
    /// Named actions beyond click — Compose's `customActions`; the primary click is `clickable`.
    pub actions: Vec<SemanticsAction>,
    /// Whether this element has a click action (a `clickable`/click node in its modifier chain).
    pub clickable: bool,
    pub focused: bool,
    /// Whether this element is announced when it changes without focus (see
    /// [`SemanticsConfig::live_region`]).
    pub live_region: LiveRegionMode,
    /// Whether this element is a group of selectable children (`Modifier::selectable_group`). A reader treats
    /// such a node as a collection whose items are its children.
    pub selectable_group: bool,
    /// Absolute bounds in logical pixels: `(x, y, width, height)`, scroll offsets applied — the same
    /// space `hit_test` works in, so a platform bridge can hand these to the OS and an action can be
    /// fired by id without hit-testing.
    pub bounds: (f32, f32, f32, f32),
    pub children: Vec<SemanticsNode>,
}
impl SemanticsNode {
    /// This element and every element below it, depth-first.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a SemanticsNode)) {
        f(self);
        for child in &self.children {
            child.walk(f);
        }
    }

    /// The first element named `name`, depth-first.
    pub fn find_by_name<'a>(&'a self, name: &str) -> Option<&'a SemanticsNode> {
        let mut found = None;
        self.walk(&mut |node| {
            if found.is_none() && node.name.as_deref() == Some(name) {
                found = Some(node);
            }
        });
        found
    }

    /// Every element with this role.
    pub fn find_by_role(&self, role: SemanticsRole) -> Vec<&SemanticsNode> {
        let mut out = Vec::new();
        self.walk(&mut |node| {
            if node.role == Some(role) {
                out.push(node);
            }
        });
        out
    }

    /// The name a consumer should announce: the element's own name, or the names of its children
    /// joined — what makes a label-only container still readable.
    pub fn effective_name(&self) -> Option<String> {
        if let Some(name) = self.name.as_deref() {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        let joined = self
            .children
            .iter()
            .filter_map(|c| c.effective_name())
            .collect::<Vec<_>>()
            .join(" ");
        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Published snapshot
// ═══════════════════════════════════════════════════════════

/// One overlay's elements, and the overlay's screen origin: its own tree is in the overlay's local
/// coordinates (it is rendered translated), so a consumer that wants screen coordinates adds it.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySemantics {
    pub id: u64,
    pub origin: (f32, f32),
    pub nodes: Vec<SemanticsNode>,
}

/// Everything one window declared in a frame: the main tree and every overlay above it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowSemantics {
    pub main: Vec<SemanticsNode>,
    pub overlays: Vec<OverlaySemantics>,
    /// What a platform bridge needs to turn logical bounds into physical ones: the window's client
    /// area in logical pixels and the scale it draws at. Where the window IS on screen is not here —
    /// that is the platform's to ask (the UIA bridge reads it off the window handle).
    pub window_size: (f32, f32),
    pub scale_factor: f32,
}

impl WindowSemantics {
    /// The window's elements as JSON — the debug channel's `sem` answer.
    pub fn json(&self) -> String {
        let overlays: Vec<String> = self
            .overlays
            .iter()
            .map(|overlay| {
                format!(
                    "{{\"id\":{},\"screen\":[{:.0},{:.0}],\"tree\":{}}}",
                    overlay.id,
                    overlay.origin.0,
                    overlay.origin.1,
                    semantics_json(&overlay.nodes)
                )
            })
            .collect();
        format!("{{\"main\":{},\"overlays\":[{}]}}", semantics_json(&self.main), overlays.join(","))
    }

    /// The window's top-level elements: the main tree's, then each overlay's — with the origin each
    /// one's coordinates are relative to (an overlay's tree is in the overlay's own space).
    pub fn top_level(&self) -> Vec<(&SemanticsNode, (f32, f32))> {
        let mut out: Vec<(&SemanticsNode, (f32, f32))> =
            self.main.iter().map(|node| (node, (0.0, 0.0))).collect();
        for overlay in &self.overlays {
            out.extend(overlay.nodes.iter().map(|node| (node, overlay.origin)));
        }
        out
    }

    /// The element a path addresses, and the origin its coordinates are relative to.
    ///
    /// A path is indices into the tree: `[2]` is the third top-level element, `[2, 0]` its first
    /// child. This is the addressing a UIA provider navigates with, and it is the same shape for the
    /// main tree and for overlays, so nothing has to special-case a dialog.
    pub fn resolve(&self, path: &[usize]) -> Option<(&SemanticsNode, (f32, f32))> {
        let (&first, rest) = path.split_first()?;
        let (mut node, origin) = *self.top_level().get(first)?;
        for &index in rest {
            node = node.children.get(index)?;
        }
        Some((node, origin))
    }

    /// The children of a path, in tree order (the top-level elements for the empty path).
    pub fn children_at(&self, path: &[usize]) -> Vec<(&SemanticsNode, (f32, f32))> {
        match self.resolve(path) {
            Some((node, origin)) => node.children.iter().map(|child| (child, origin)).collect(),
            // The empty path is the window itself, whose children are the top-level elements.
            None if path.is_empty() => self.top_level(),
            None => Vec::new(),
        }
    }

    /// A fingerprint of the tree's SHAPE: which elements exist, in what order, with how many
    /// children each has. Names and states are deliberately not in it — a label changing is a
    /// property change, not a structural one, and a client that cares re-reads the property.
    ///
    /// A platform bridge compares this frame's fingerprint with the last one to decide whether to tell
    /// its clients the tree changed, so it has to be cheap (one pass, no allocation) and stable
    /// (the same tree always hashes the same).
    pub fn structure_fingerprint(&self) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        let mut mix = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x100000001b3);
        };
        // The overlay ids are part of the shape: a dialog appearing IS a structural change.
        for overlay in &self.overlays {
            mix(overlay.id);
        }
        for (node, _) in self.flatten() {
            mix(node.node_id);
            mix(node.children.len() as u64);
        }
        hash
    }

    /// The path of the element that currently has the keyboard focus, if any.
    ///
    /// This is what a focus-changed notification needs: the path addresses the element, and the bridge
    /// turns it into a provider. Depth-first, so the innermost focused element wins — an element whose
    /// child is also focused is not the focus.
    pub fn focused_path(&self) -> Option<Vec<usize>> {
        let mut found: Option<Vec<usize>> = None;
        let mut visit = |node: &SemanticsNode, _origin: (f32, f32), path: Vec<usize>| {
            if node.focused {
                found = Some(path);
            }
        };
        for (index, (node, origin)) in self.top_level().into_iter().enumerate() {
            walk_with_paths(node, vec![index], origin, &mut visit);
        }
        found
    }

    /// Every element of the window, parents before children, main tree first then each overlay — what
    /// a query that is not addressed by path needs ("the focused element", "the element under this
    /// point").
    pub fn flatten(&self) -> Vec<(&SemanticsNode, (f32, f32))> {
        let mut out = Vec::new();
        for (node, origin) in self.top_level() {
            node.walk(&mut |n| out.push((n, origin)));
        }
        out
    }
}

// ═══════════════════════════════════════════════════════════
// Actions requested from outside the UI thread
// ═══════════════════════════════════════════════════════════

/// Something another thread (a platform bridge) asked the UI to do on a node.
///
/// UIA calls a provider at a moment the application does not control, so the provider only queues
/// this; the frame loop drains it and runs the same code a real interaction would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    /// Fire the node's click action.
    Invoke(u64),
    /// Move the keyboard focus to the node.
    Focus(u64),
}

static ACTIONS: std::sync::Mutex<Option<Vec<(u64, UiAction)>>> = std::sync::Mutex::new(None);

/// Queue an action for a window's next frame.
pub fn request_action(window_id: u64, action: UiAction) {
    let mut store = ACTIONS.lock().unwrap();
    store.get_or_insert_with(Default::default).push((window_id, action));
}

/// Whether anything is queued — the event loop checks this before walking its windows.
pub fn has_actions() -> bool {
    ACTIONS
        .lock()
        .map(|store| store.as_ref().is_some_and(|store| !store.is_empty()))
        .unwrap_or(false)
}

/// Take a window's queued actions.
pub fn take_actions(window_id: u64) -> Vec<UiAction> {
    let Ok(mut store) = ACTIONS.lock() else {
        return Vec::new();
    };
    let Some(store) = store.as_mut() else {
        return Vec::new();
    };
    let mut taken = Vec::new();
    store.retain(|(id, action)| {
        if *id == window_id {
            taken.push(*action);
            false
        } else {
            true
        }
    });
    taken
}

static PUBLISHED: std::sync::Mutex<Option<std::collections::HashMap<u64, std::sync::Arc<WindowSemantics>>>> =
    std::sync::Mutex::new(None);

/// Whether the frame loop should build snapshots at all. Both consumers are optional, and the walk is
/// pure overhead without one — so it compiles away entirely in a plain build.
pub const fn publishing_enabled() -> bool {
    cfg!(feature = "debug-server") || cfg!(feature = "accessibility")
}

/// Replace one window's snapshot — called once per rendered frame, so a query answers with what the
/// frame that just drew declared rather than what a later frame will.
pub fn publish(window_id: u64, snapshot: WindowSemantics) {
    let mut store = PUBLISHED.lock().unwrap();
    store
        .get_or_insert_with(Default::default)
        .insert(window_id, std::sync::Arc::new(snapshot));
}

/// The last published snapshot of one window.
pub fn published(window_id: u64) -> Option<std::sync::Arc<WindowSemantics>> {
    PUBLISHED.lock().ok()?.as_ref()?.get(&window_id).cloned()
}

/// Drop a window's snapshot (its window closed — a stale tree would answer for a window that is gone).
pub fn forget(window_id: u64) {
    if let Ok(mut store) = PUBLISHED.lock() {
        if let Some(store) = store.as_mut() {
            store.remove(&window_id);
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Building the tree
// ═══════════════════════════════════════════════════════════

/// Resolve the semantics of one composer's arena into elements, top level first.
///
/// `root` is the arena index of the layout root, as `Composer::layout_root_idx` returns it.
pub fn semantics_tree(nodes: &[LayoutNode], root: usize) -> Vec<SemanticsNode> {
    let mut out = Vec::new();
    collect(nodes, root, 0.0, 0.0, &mut out);
    out
}

fn collect(nodes: &[LayoutNode], idx: usize, parent_x: f32, parent_y: f32, out: &mut Vec<SemanticsNode>) {
    let node = &nodes[idx];
    let config = node.modifier.semantics_config();
    let (x, y) = (parent_x + node.position.x, parent_y + node.position.y);
    let bounds = (x, y, node.measured_size.width, node.measured_size.height);

    let clickable = node.modifier.on_click().is_some() || node.modifier.node_click().is_some();

    // `clearAndSetSemantics`: the block REPLACES this node's declaration and cuts the whole subtree
    // out of the tree. A block that declares nothing leaves the node out too (Compose's
    // "invisible to the user"), which is why the name comes only from the block — never from the
    // text the clear was meant to suppress.
    if let Some(clear) = node.modifier.clear_and_set_semantics_config() {
        let declared = clear.role_value().is_some()
            || clear.content_description_value().is_some()
            || !clear.state_value().is_unspecified()
            || clear.merges_descendants()
            || clear.is_selectable_group()
            || clear.live_region_mode() != LiveRegionMode::Off
            || !clear.custom_actions().is_empty();
        if declared {
            out.push(SemanticsNode {
                node_id: node.id,
                role: clear.role_value(),
                name: clear.content_description_value().map(str::to_string),
                state: clear.state_value(),
                actions: clear.custom_actions().to_vec(),
                clickable,
                focused: node.focused,
                live_region: clear.live_region_mode(),
                selectable_group: clear.is_selectable_group(),
                bounds,
                children: Vec::new(),
            });
        }
        return;
    }

    let declared_name = config
        .content_description_value()
        .map(str::to_string)
        .or_else(|| own_name(node));

    // Where this node's children start: below any scroll offset, the same way the focus walk and hit
    // testing do it, so bounds stay in one coordinate space.
    let (scroll_x, scroll_y) = scroll_offset_for_node(node);
    let (child_x, child_y) = (x - scroll_x, y - scroll_y);
    let children_of = node.children.clone();

    if clickable || config.merges_descendants() {
        // Claimed subtree: fold every descendant into this one element and drop them from the tree.
        let mut absorbed = Vec::new();
        for child in children_of {
            collect(nodes, child, child_x, child_y, &mut absorbed);
        }
        // Every absorbed descendant's state is folded in, not just the first one that says
        // something. Taking only the first loses whatever the rest declared: a dialog holding a
        // toggle (`enabled`) and an entry field in error would report the toggle's state and drop
        // the error, and the reader would never hear what is wrong with the value.
        let absorbed_state = absorbed
            .iter()
            .map(|child| child.state.clone())
            .reduce(|merged, next| next.or(merged))
            .unwrap_or_default();
        let absorbed_role = absorbed.iter().find_map(|child| child.role);
        // Actions accumulate across the claimed subtree, deduped by label (same rule as `or`).
        let mut actions = config.custom_actions().to_vec();
        for child in &absorbed {
            for action in &child.actions {
                if !actions.iter().any(|a| a.label() == action.label()) {
                    actions.push(action.clone());
                }
            }
        }
        out.push(SemanticsNode {
            node_id: node.id,
            role: config.role_value().or(absorbed_role),
            name: declared_name.or_else(|| joined_name(&absorbed)),
            state: config.state_value().or(absorbed_state),
            actions,
            clickable: clickable || absorbed.iter().any(|child| child.clickable),
            focused: node.focused,
            // An absorbed live region stays live: a snackbar wrapping its message is one element, and
            // the mode came from the wrapper.
            live_region: config
                .live_region_mode()
                .max(absorbed.iter().map(|child| child.live_region).max().unwrap_or_default()),
            selectable_group: config.is_selectable_group(),
            bounds,
            children: Vec::new(),
        });
        return;
    }

    let mut children = Vec::new();
    for child in children_of {
        collect(nodes, child, child_x, child_y, &mut children);
    }

    let role = config.role_value();
    if role.is_some()
        || declared_name.is_some()
        || !config.state_value().is_unspecified()
        // A group flag is a declaration too: without this the group node would be transparent and the tree
        // would lose the one thing that says its children are a collection.
        || config.is_selectable_group()
    {
        // A declared node keeps its place and its children (nothing was claimed).
        out.push(SemanticsNode {
            node_id: node.id,
            role,
            name: declared_name,
            state: config.state_value(),
            actions: config.custom_actions().to_vec(),
            clickable,
            focused: node.focused,
            live_region: config.live_region_mode(),
            selectable_group: config.is_selectable_group(),
            bounds,
            children,
        });
    } else {
        // Declares nothing: transparent. Its descendants stand in its place, so an ordinary `Column`
        // never appears between a panel and its buttons.
        out.extend(children);
    }
}

/// This node's own name source, without looking at descendants: its text, or an image/icon
/// description.
fn own_name(node: &LayoutNode) -> Option<String> {
    for element in node.modifier.elements() {
        match element {
            ModifierElement::TextContent { content, .. } => return Some(content.clone()),
            ModifierElement::RichTextContent { content, .. } => return Some(content.clone()),
            ModifierElement::Semantics(config) => {
                if let Some(description) = config.content_description_value() {
                    return Some(description.to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Depth-first walk handing each element its index path and the origin its coordinates are relative
/// to. `crate::accessibility` has its own copy for the provider tree; this one is here because
/// [`WindowSemantics::focused_path`] needs it on every platform.
fn walk_with_paths(
    node: &SemanticsNode,
    path: Vec<usize>,
    origin: (f32, f32),
    f: &mut impl FnMut(&SemanticsNode, (f32, f32), Vec<usize>),
) {
    f(node, origin, path.clone());
    for (index, child) in node.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(index);
        walk_with_paths(child, child_path, origin, f);
    }
}

/// The names an absorbed subtree contributes, in tree order.
fn joined_name(absorbed: &[SemanticsNode]) -> Option<String> {
    let names: Vec<String> = absorbed.iter().filter_map(|node| node.effective_name()).collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(" "))
    }
}

/// Fire the click action of the element with this `LayoutNode` id — what a platform bridge's Invoke
/// pattern needs, and the reason [`SemanticsNode::node_id`] is part of the tree.
///
/// Returns whether a callback ran; an element without an action (a label) reports `false`.
pub fn click_node_by_id(nodes: &[LayoutNode], node_id: u64) -> bool {
    let Some(node) = nodes.iter().find(|node| node.id == node_id) else {
        return false;
    };
    if let Some(callback) = node.modifier.on_click() {
        callback();
        return true;
    }
    if let Some(node_callback) = node.modifier.node_click() {
        node_callback.on_click();
        return true;
    }
    false
}

// ═══════════════════════════════════════════════════════════
// Debug JSON
// ═══════════════════════════════════════════════════════════

/// The tree as JSON — what the debug server's `sem` command returns and UI tests assert on.
///
/// Hand-written rather than serde: `serde_json` is a dev-dependency, and this is a handful of fields.
pub fn semantics_json(tree: &[SemanticsNode]) -> String {
    let mut out = String::from("[");
    for (i, node) in tree.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        node_json(node, &mut out);
    }
    out.push(']');
    out
}

fn node_json(node: &SemanticsNode, out: &mut String) {
    let (x, y, w, h) = node.bounds;
    out.push_str(&format!(
        r#"{{"id":{},"role":{},"name":{},"state":"#,
        node.node_id,
        match node.role {
            Some(role) => format!("\"{}\"", role.name()),
            None => "null".to_string(),
        },
        match node.name.as_deref() {
            Some(name) => json_string(name),
            None => "null".to_string(),
        },
    ));
    // State: only the fields that say something, so a consumer can tell "unchecked" from "silent".
    let mut state = Vec::new();
    if let Some(selected) = node.state.selected_value() {
        state.push(format!("\"selected\":{selected}"));
    }
    if let Some(checked) = node.state.checked_value() {
        let name = match checked {
            ToggleableState::Off => "off",
            ToggleableState::On => "on",
            ToggleableState::Indeterminate => "indeterminate",
        };
        state.push(format!("\"checked\":\"{name}\""));
    }
    if let Some(expanded) = node.state.expanded_value() {
        state.push(format!("\"expanded\":{expanded}"));
    }
    if let Some(enabled) = node.state.enabled_value() {
        state.push(format!("\"enabled\":{enabled}"));
    }
    if let Some((current, min, max)) = node.state.progress_value() {
        state.push(format!("\"progress\":{{\"value\":{current},\"min\":{min},\"max\":{max}}}"));
    }
    if let Some(message) = node.state.error_value() {
        // `json_string` already quotes and escapes; quoting it again would make the whole
        // snapshot unparseable, which reads from the debug channel as no answer at all.
        state.push(format!("\"error\":{}", json_string(message)));
    }
    let actions = node
        .actions
        .iter()
        .map(|action| json_string(action.label()))
        .collect::<Vec<_>>()
        .join(",");
    out.push_str(&format!(
        "{{{}}},\"actions\":[{actions}],\"clickable\":{},\"focused\":{},\"liveRegion\":{},\"selectableGroup\":{},\"bounds\":[{x:.0},{y:.0},{w:.0},{h:.0}],\"children\":[",
        state.join(","),
        node.clickable,
        node.focused,
        match node.live_region {
            LiveRegionMode::Off => "null",
            LiveRegionMode::Polite => "\"polite\"",
            LiveRegionMode::Assertive => "\"assertive\"",
        },
        node.selectable_group,
    ));
    for (i, child) in node.children.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        node_json(child, out);
    }
    out.push_str("]}");
}

fn json_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\u{8}', "\\b")
        .replace('\u{c}', "\\f");
    format!("\"{escaped}\"")
}


#[cfg(test)]
mod tests;
