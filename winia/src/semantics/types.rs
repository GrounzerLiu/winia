use super::*;

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
    pub(crate) selected: Option<bool>,
    pub(crate) checked: Option<ToggleableState>,
    pub(crate) expanded: Option<bool>,
    pub(crate) enabled: Option<bool>,
    /// What is wrong with the value, in the words the field itself uses — Compose's
    /// `SemanticsProperties.error`, which a text field sets to its supporting text when that text
    /// is an error. A reader announces it in place of the value rather than making the user find
    /// the message on the screen. `None` means the value is not in error, which is different from
    /// being in error with nothing to say.
    pub(crate) error: Option<String>,
    /// A progress value with its range — Compose's `ProgressBarRangeInfo`. `None` on an
    /// indeterminate indicator: there IS no value, which is different from a value of zero, and a
    /// screen reader should say "in progress" rather than "0 percent".
    pub(crate) progress: Option<(f32, f32, f32)>,
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
    pub(crate) fn or(self, fallback: SemanticsState) -> SemanticsState {
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
    pub(crate) label: String,
    pub(crate) action: std::sync::Arc<dyn Fn() + Send + Sync>,
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
    pub(crate) role: Option<SemanticsRole>,
    pub(crate) content_description: Option<String>,
    pub(crate) state: SemanticsState,
    pub(crate) merge_descendants: bool,
    pub(crate) live_region: LiveRegionMode,
    /// Compose's `SemanticsProperties.SelectableGroup`: this node is a GROUP of selectable children (radio
    /// buttons, tabs), not a selectable thing itself. A reader announces the group as a collection and lets the
    /// user move inside it, which is why the flag has to survive on the tree rather than being folded away.
    pub(crate) selectable_group: bool,
    /// Named actions beyond click — Compose's `customActions`.
    pub(crate) custom_actions: Vec<SemanticsAction>,
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

