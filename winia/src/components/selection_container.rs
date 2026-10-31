//! `SelectionContainer` — Compose's text-selection container.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::runtime::composer::GroupStatus;



use crate::text::selection::{LOCAL_SELECTION_DISABLED, LOCAL_SELECTION_REGISTRAR, Selection, SelectionRegistrar};
// ═══════════════════════════════════════════════════════════
// SelectionContainer
// ═══════════════════════════════════════════════════════════

pub struct SelectionContainer {
    modifier: Modifier,
    on_change: Option<Box<dyn Fn(&Selection) + Send + Sync>>,
    state: Option<SelectionState>,
}

impl SelectionContainer {
    pub fn new() -> Self {
        SelectionContainer { modifier: Modifier::new(), on_change: None, state: None }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Install the selection-changed callback. It receives a [`Selection`] whose text is already
    /// stitched from the registered runs, so the caller keeps no parallel copy.
    ///
    /// It fires only when a NON-ZERO-WIDTH selection appears (a drag). A plain click is zero width
    /// and fires nothing, so clearing a selection has no notification path — a caller that wants to
    /// refresh on click has to do it around `content`.
    pub fn on_selection_change(mut self, f: impl Fn(&Selection) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Bind a [`SelectionState`], so the container's selection is observable and can be driven
    /// programmatically — Compose's `SelectionContainer(state = ...)`.
    pub fn state(mut self, state: SelectionState) -> Self {
        self.state = Some(state);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // One persistent registrar: a rebuild must not create a new one, because the segments have
        // to survive recomposition.
        let registrar = ctx.remember_at_key(key, || SelectionRegistrar::new()).get();
        if let Some(state) = &self.state {
            state.bind(&registrar);
        }
        // One callback, because the registrar has one slot: publish into the state, then hand the
        // selection to the caller's own closure.
        {
            let state = self.state.clone();
            let user = self.on_change;
            registrar.set_on_change(move |sel: &Selection| {
                if let Some(state) = &state {
                    state.set_selection(if sel.is_empty() { None } else { Some(sel.clone()) });
                }
                if let Some(cb) = &user {
                    cb(sel);
                }
            });
        }
        {
            let reg = registrar.clone();
            LOCAL_SELECTION_REGISTRAR.provides(reg, || {
                ctx.set_selection_registrar(registrar.clone());
                // The `content` closure becomes a composition scope of its own, like Column/Row/Stack.
                // No table reset on Enter: a text whose parameters did not change Skips, so it never
                // re-registers, and wiping here dropped it (measured before this fix: a State read in
                // the body took `total` from 10 to 0 and the selection from Some to None). Offsets are
                // rebuilt from the layout tree at the end of every layout pass instead —
                // `Composer::reindex_selection_registrars`, which keeps a skipped text's segment and
                // drops one whose node actually left the tree.
                // The selection lives here, so the COPY has to be handled here too: keys reach the
        // container whenever focus is inside it (it sits on the focus path), and no text node below
        // can answer for the whole container's selection.
        let copy_registrar = registrar.clone();
        let modifier = self.modifier.key_node(CopySelectionKey { registrar: copy_registrar });
        match ctx.start_restartable_group(key, modifier, BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        content(ctx);
                    }
                }
                ctx.end_restartable_group();
            });
            // Restore the composer's own registrar once `provides` exits. Without this, a Text built
            // after this container (say, the output that displays the selected text) would register
            // into it.
            ctx.clear_selection_registrar();
        }
    }
}

/// Ctrl+C (and Cmd+C, which arrives as `meta`) copies the container's selection.
///
/// A `KeyNode` rather than a `ModifierElement::KbEvent`, so no `TextContent`-style field list has to
/// grow and the node rides the existing key path (bubble, focus to root).
#[derive(Debug)]
struct CopySelectionKey {
    registrar: SelectionRegistrar,
}

impl crate::modifier::KeyNode for CopySelectionKey {
    fn on_event(&self, ev: &crate::input::KbEvent) -> bool {
        use crate::input::KbEventType;
        if ev.event_type != KbEventType::KeyDown { return false; }
        let is_c = matches!(&ev.key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("c"));
        if !is_c || !(ev.is_ctrl_pressed || ev.is_meta_pressed) { return false; }
        // No selection: leave the key to whatever else wanted it.
        let Some(selection) = self.registrar.build_selection() else { return false; };
        crate::clipboard::request_text(selection.text());
        true
    }

    fn node_key(&self) -> String { "SelectionContainer.copy".to_string() }
}

/// Disable selection for this subtree, like Compose's `DisableSelection`.
///
/// Text and RichText below it keep rendering normally but do not register with an enclosing
/// SelectionContainer. The local is scoped, so selectable siblings outside this wrapper are
/// unaffected.
pub struct DisableSelection {
    enabled: bool,
}

impl DisableSelection {
    /// Always disable selection, as Compose's `DisableSelection` does.
    pub fn new() -> Self { Self { enabled: true } }

    /// Conditionally disable selection while keeping this scope at a stable call site. When the
    /// value changes, the whole subtree is marked dirty so reused Text nodes re-read the local and
    /// clear/reapply their registrar instead of keeping a stale one across Skip.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let changed = ctx.changed(&self.enabled);
        if changed {
            ctx.mark_subtree_dirty();
        }
        LOCAL_SELECTION_DISABLED.provides(self.enabled, || content(ctx));
    }
}

impl Default for DisableSelection {
    fn default() -> Self { Self::new() }
}
/// The observable handle on a container's selection — Compose's `SelectionState`.
///
/// Bind it with `SelectionContainer::state(&state)`; the container then publishes every change into
/// it (a drag, a press that clears, and the programmatic actions below), so a composable that reads
/// [`Self::selection`] or [`Self::selected_text`] recomposes with the selection.
///
/// Compose keeps a list of `AnnotatedString`s (`selectedTexts`) because one selection can span
/// several text composables; winia's [`Selection`] already carries the stitched text as one string,
/// so [`Self::selected_text`] answers that and the per-run list is [`Self::selectable_texts`].
#[derive(Clone)]
pub struct SelectionState {
    inner: std::sync::Arc<SelectionStateInner>,
}

struct SelectionStateInner {
    /// The container this state is bound to, if any. Compose refuses a second, different one.
    registrar: parking_lot::Mutex<Option<SelectionRegistrar>>,
    /// The published selection. A `State` rather than a plain field, so readers recompose.
    selection: crate::runtime::state::State<Option<Selection>>,
}

impl SelectionState {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(SelectionStateInner {
                registrar: parking_lot::Mutex::new(None),
                selection: crate::runtime::state::State::new(None),
            }),
        }
    }

    /// The current selection, or `None` when there is none.
    pub fn selection(&self) -> Option<Selection> {
        self.inner.selection.get()
    }

    /// The selected text, already stitched across the runs the selection covers.
    pub fn selected_text(&self) -> Option<String> {
        self.inner.selection.get().map(|sel| sel.text().to_string())
    }

    /// Every selectable run in the container, in reading order — Compose's `getSelectableTexts`.
    ///
    /// A run excluded by [`DisableSelection`] is not in the container's table at all, so it is not
    /// in this list either.
    pub fn selectable_texts(&self) -> Vec<String> {
        self.bound().map(|reg| reg.selectable_texts()).unwrap_or_default()
    }

    /// Select every selectable run in the container — Compose's `selectAll`.
    pub fn select_all(&self) {
        let Some(reg) = self.bound() else { return };
        let total = reg.total_text_len();
        reg.set_selection(0, total);
        self.publish();
        reg.fire_on_change();
    }

    /// Select a range in the container's global offsets — Compose's `select(range)`.
    pub fn select(&self, start: usize, end: usize) {
        let Some(reg) = self.bound() else { return };
        reg.set_selection(start, end);
        self.publish();
        reg.fire_on_change();
    }

    /// Drop the selection — Compose's `clear`.
    pub fn clear(&self) {
        let Some(reg) = self.bound() else { return };
        reg.clear_selection();
        self.publish();
        reg.fire_on_change();
    }

    fn bound(&self) -> Option<SelectionRegistrar> {
        self.inner.registrar.lock().clone()
    }

    /// Bind to a container's registrar. A state belongs to ONE container; Compose raises on a
    /// second, and so does this (the alternative is two containers writing one selection).
    fn bind(&self, registrar: &SelectionRegistrar) {
        let mut bound = self.inner.registrar.lock();
        if let Some(existing) = bound.as_ref() {
            assert!(
                existing.is_same(registrar),
                "A SelectionState can only be bound to one SelectionContainer. Please use \
                 `remember_selection_state` to create a unique state for each container."
            );
        }
        *bound = Some(registrar.clone());
        drop(bound);
        self.publish();
    }

    /// Publish what the bound registrar currently holds.
    fn publish(&self) {
        let selection = self.bound().and_then(|reg| reg.build_selection());
        self.set_selection(selection);
    }

    fn set_selection(&self, selection: Option<Selection>) {
        // `State::set` notifies on a change, so an unchanged selection costs nothing.
        self.inner.selection.set(selection);
    }
}

impl Default for SelectionState {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Debug for SelectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectionState").field("selection", &self.selection()).finish()
    }
}

/// Remember a [`SelectionState`] for this composition position — Compose's `rememberSelectionState`.
#[composable]
pub fn remember_selection_state(ctx: &mut ComposeCtx) -> SelectionState {
    ctx.remember(SelectionState::new).get()
}
impl Default for SelectionContainer {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// The drag-selection core, shared by real PointerMoved handling and the debug probe, so there is
// one implementation and it is testable.
// ═══════════════════════════════════════════════════════════

/// The selection range after one pointer move.
///
/// `Some((registrar, global start, global end))`, or `None` when the selection does not move.
/// Two cases:
/// - Same container: `anchor_global .. current_global`, both in that container's offset space.
/// - Across containers (the pointer is over another container's text): snap to one end of the
///   ANCHOR's container. Switching to the other container's offset space would mix an anchor global
///   offset with a different container's origin and select the wrong text.
///   The end is picked from the DOMINANT axis of the drag: a Row of containers is dragged purely
///   horizontally, and a y-only rule then always answered "the end" even when the pointer moved
///   left.
/// - No anchor container (the press landed on something unselectable) -> `None`.
pub(crate) fn compute_selection(
    anchor_reg: Option<&SelectionRegistrar>,
    anchor_global: Option<usize>,
    cur_reg: &SelectionRegistrar,
    cur_global_off: Option<usize>,
    cur_index: usize,
    scene_x: f32,
    scene_y: f32,
    down_x: f32,
    down_y: f32,
) -> Option<(SelectionRegistrar, usize, usize)> {
    let same_reg = anchor_reg.map(|ar| ar.is_same(cur_reg)).unwrap_or(false);
    if same_reg {
        let off = cur_global_off?;
        let current_global = off + cur_index;
        let s = anchor_global.map(|a| a.min(current_global)).unwrap_or(current_global);
        let e = anchor_global.map(|a| a.max(current_global)).unwrap_or(current_global + 1);
        Some((cur_reg.clone(), s, e))
    } else if let (Some(anchor_reg), Some(a)) = (anchor_reg, anchor_global) {
        // Across containers: snap inside the anchor's container, clamping to its bounds.
        let total = anchor_reg.total_text_len();
        let (dx, dy) = (scene_x - down_x, scene_y - down_y);
        let towards_end = if dx.abs() >= dy.abs() { dx > 0.0 } else { dy > 0.0 };
        let edge = if towards_end { total } else { 0 };
        let s = a.min(edge);
        let e = a.max(edge);
        Some((anchor_reg.clone(), s, e))
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

/// End-to-end T-1 regression: a container that re-enters while its texts Skip keeps its
    /// segments, and the selection still resolves.
    ///
    /// Before the fix `reset_offsets()` ran on every `GroupStatus::Enter`, so the second frame had
    /// an empty table (`total=0`) and `build_selection()` returned None.
    #[test]
    fn a_container_reentering_with_skipped_texts_keeps_its_selection() {
        use crate::runtime::composer::Composer;
        use crate::runtime::state::State;

        let head = State::new("hello");
        let tick = State::new(0i32);
        let seen: std::sync::Arc<std::sync::Mutex<Option<SelectionRegistrar>>> = Default::default();
        let mut composer = Composer::new();
        let mut totals = Vec::new();
        for frame in 0..2 {
            let seen = seen.clone();
            let head_value = head.get();
            composer.compose(|ctx| {
                SelectionContainer::new().build(ctx, |ctx| {
                    *seen.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                    // Reading the state INSIDE the container body is what makes the container's own
                    // group re-enter next frame — and the two texts below keep their parameters, so
                    // they Skip instead of re-registering. This is the exact shape that lost the
                    // table before the fix.
                    let _ = tick.get();
                    crate::layout::components::Column::new().build(ctx, |ctx| {
                        crate::components::Text::new(head_value.clone()).build(ctx);
                        crate::components::Text::new("world").build(ctx);
                    });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 400.0));
            let reg = seen.lock().unwrap().clone().expect("registrar captured");
            totals.push(reg.total_text_len());
            if frame == 0 { tick.set(1); }
        }
        assert_eq!(totals, vec![10, 10], "the skipped texts must keep their segments");
        let reg = seen.lock().unwrap().clone().unwrap();
        reg.set_selection(0, reg.total_text_len());
        assert_eq!(
            reg.build_selection().map(|s| s.text().to_string()),
            Some("helloworld".to_string()),
            "the selection still resolves after the container re-entered"
        );
    }

    #[test]
    fn test_new_registrar_no_selection() {
        let reg = SelectionRegistrar::new();
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_local_selection_registrar_scopes_to_provides() {
        assert!(LOCAL_SELECTION_REGISTRAR.try_current().is_none());
        let reg = SelectionRegistrar::new();
        LOCAL_SELECTION_REGISTRAR.provides(reg.clone(), || {
            let current = LOCAL_SELECTION_REGISTRAR.try_current().expect("registrar should be visible inside provides");
            assert!(current.is_same(&reg));
        });
        assert!(LOCAL_SELECTION_REGISTRAR.try_current().is_none());
    }

    #[test]
    fn test_register_and_set_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "hello world");
        reg.set_selection(5, 15);
        // "hello world" is 11 bytes, so the end clamps to 11.
        assert_eq!(reg.selected_range(42), Some(5..11));
    }

    #[test]
    fn test_cross_text_merged() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");
        reg.register(2, "klmnopqrstuvwxy");
        reg.set_selection(5, 20);
        assert_eq!(reg.selected_range(1), Some(5..10));
        assert_eq!(reg.selected_range(2), Some(0..10));
        assert!(reg.selected_range(99).is_none());
    }

    #[test]
    fn test_partial_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghijklmnopqrst");
        reg.set_selection(15, 30); // extends beyond text
        assert_eq!(reg.selected_range(1), Some(15..20)); // clamped
    }

    #[test]
    fn test_selection_outside() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "abcdefghij");
        reg.set_selection(20, 30);
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clear() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "hello world");
        reg.set_selection(5, 15);
        reg.clear_selection();
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clone_shared() {
        let reg1 = SelectionRegistrar::new();
        let reg2 = reg1.clone();
        reg1.register(42, "abcdefghij");
        reg1.set_selection(2, 8);
        assert_eq!(reg2.selected_range(42), Some(2..8));
    }

    #[test]
    fn test_slot_dedup() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "hello");   // offset=0, next=5
        reg.register(1, "hello world");  // same slot, NOT new → offset stays 0, next=max(5,0+11)=11
        assert_eq!(reg.total_text_len(), 11);
        assert_eq!(reg.segment_info(1), Some((0, 11))); // preserves original offset, latest len
    }

    #[test]
    fn test_total_text_len_accumulation() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");  // offset=0, len=10, total=10
        reg.register(2, "klmnopqrstuvwxy");  // offset=10, len=15, total=25
        reg.register(3, "12345");   // offset=25, len=5, total=30
        assert_eq!(reg.total_text_len(), 30);
        assert_eq!(reg.segment_info(1), Some((0, 10)));
        assert_eq!(reg.segment_info(2), Some((10, 15)));
        assert_eq!(reg.segment_info(3), Some((25, 5)));
    }

    #[test]
    fn test_on_change_callback() {
        use std::sync::Mutex;
        let reg = SelectionRegistrar::new();
        let called = std::sync::Arc::new(Mutex::new(false));
        let c = called.clone();
        reg.set_on_change(move |_| { *c.lock().unwrap() = true; });
        reg.register(1, "abcdefghij");
        reg.set_selection(2, 5);
        reg.fire_on_change();
        assert!(*called.lock().unwrap());
    }

    #[test]

    /// Compose's DisableSelection applies to both Text and RichText, but not to selectable siblings
    /// outside the scope.
    #[test]
    fn disable_selection_excludes_text_and_rich_text_only_inside_its_scope() {
        use crate::runtime::composer::Composer;
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
        let mut composer = Composer::new();
        let seen_in = seen.clone();
        composer.compose(move |ctx| {
            let seen_body = seen_in.clone();
            SelectionContainer::new().build(ctx, move |ctx| {
                *seen_body.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                crate::layout::components::Column::new().build(ctx, |ctx| {
                    crate::components::Text::new("before").build(ctx);
                    DisableSelection::new().build(ctx, |ctx| {
                        crate::components::Text::new("disabled text").build(ctx);
                        crate::components::RichText::new().build(ctx, |rich| {
                            rich.text("disabled rich");
                        });
                    });
                    crate::components::Text::new("after").build(ctx);
                });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));

        let reg = seen.lock().unwrap().clone().expect("the outer container registered");
        assert_eq!(reg.total_text_len(), "beforeafter".len());
        reg.set_selection(0, reg.total_text_len());
        assert_eq!(reg.build_selection().map(|s| s.text().to_string()), Some("beforeafter".to_string()));
        let disabled_nodes: Vec<_> = composer.arena_nodes().iter().filter(|n| {
            n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::TextContent { content, .. } if content == "disabled text"))
                || n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::RichTextContent { content, .. } if content == "disabled rich"))
        }).collect();
        assert_eq!(disabled_nodes.len(), 2, "both disabled nodes still render");
        assert!(disabled_nodes.iter().all(|n| n.registrar.borrow().is_none()), "neither disabled node carries the outer registrar");
    }

    /// Toggling the same text position disabled -> enabled clears the reused node's stale registrar
    /// and then registers it again when selection is re-enabled.
    #[test]
    fn disable_selection_recomposition_clears_and_restores_the_registrar() {
        use crate::runtime::composer::Composer;
        use crate::runtime::state::State;
        use std::sync::{Arc, Mutex};

        let disabled = State::new(false);
        let seen: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
        let mut composer = Composer::new();
        let compose_frame = |composer: &mut Composer, disabled: &State<bool>, seen: &Arc<Mutex<Option<SelectionRegistrar>>>| {
            let disabled = disabled.clone();
            let seen = seen.clone();
            composer.compose(move |ctx| {
                let seen_body = seen.clone();
                SelectionContainer::new().build(ctx, move |ctx| {
                    *seen_body.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                    crate::layout::components::Column::new().build(ctx, move |ctx| {
                        let is_disabled = disabled.get();
                        DisableSelection::new().enabled(is_disabled).build(ctx, |ctx| {
                            crate::components::Text::new("same").build(ctx);
                        });
                    });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        compose_frame(&mut composer, &disabled, &seen);
        let reg = seen.lock().unwrap().clone().expect("registrar");
        assert_eq!(reg.total_text_len(), 4, "enabled text registers");

        disabled.set(true);
        compose_frame(&mut composer, &disabled, &seen);
        assert_eq!(reg.total_text_len(), 0, "disabled text is removed from the registrar");
        let disabled_node = composer.arena_nodes().iter().find(|n| {
            n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::TextContent { content, .. } if content == "same"))
        }).expect("the disabled text still renders");
        assert!(disabled_node.registrar.borrow().is_none(), "the reused text node's old registrar was explicitly cleared");

        disabled.set(false);
        compose_frame(&mut composer, &disabled, &seen);
        assert_eq!(reg.total_text_len(), 4, "re-enabled text registers again");
    }



    /// M-2: the state observes whatever the container's selection becomes — the same path a drag
    /// takes (`fire_on_change` is what the pointer handlers call on release).
    #[test]
    fn a_state_observes_the_containers_selection() {
        use crate::runtime::composer::Composer;
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
        let state = SelectionState::new();
        let state_in = state.clone();
        let mut composer = Composer::new();
        {
            let seen = seen.clone();
            composer.compose(move |ctx| {
                let seen_body = seen.clone();
                SelectionContainer::new()
                    .state(state_in.clone())
                    .build(ctx, move |ctx| {
                        *seen_body.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                        crate::layout::components::Column::new().build(ctx, |ctx| {
                            crate::components::Text::new("hello ").build(ctx);
                            crate::components::Text::new("world").build(ctx);
                        });
                    });
            });
        }
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let reg = seen.lock().unwrap().clone().expect("registrar");

        assert!(state.selection().is_none(), "nothing is selected to begin with");
        assert_eq!(state.selectable_texts(), vec!["hello ".to_string(), "world".to_string()]);

        reg.set_selection(6, 11);
        reg.fire_on_change();
        assert_eq!(state.selected_text(), Some("world".to_string()));
        assert_eq!(state.selection().map(|s| (s.start(), s.end())), Some((6, 11)));

        reg.clear_selection();
        reg.fire_on_change();
        assert!(state.selection().is_none(), "a clear is published as no selection");
    }

    /// The programmatic actions drive the container, and the state follows.
    #[test]
    fn select_all_select_and_clear_drive_the_container() {
        use crate::runtime::composer::Composer;
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
        let state = SelectionState::new();
        let state_in = state.clone();
        let mut composer = Composer::new();
        {
            let seen = seen.clone();
            composer.compose(move |ctx| {
                let seen_body = seen.clone();
                SelectionContainer::new()
                    .state(state_in.clone())
                    .build(ctx, move |ctx| {
                        *seen_body.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                        crate::layout::components::Column::new().build(ctx, |ctx| {
                            crate::components::Text::new("hello ").build(ctx);
                            crate::components::Text::new("world").build(ctx);
                        });
                    });
            });
        }
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let reg = seen.lock().unwrap().clone().expect("registrar");
        assert_eq!(reg.total_text_len(), 11, "two runs, five and six bytes");

        state.select_all();
        assert_eq!(state.selected_text(), Some("hello world".to_string()));

        state.select(0, 5);
        assert_eq!(state.selected_text(), Some("hello".to_string()));

        state.clear();
        assert!(state.selection().is_none());
        assert_eq!(reg.total_text_len(), 11, "the container still knows its text");
    }

    /// Compose refuses to bind one state to two containers; so does this one.
    #[test]
    #[should_panic(expected = "can only be bound to one SelectionContainer")]
    fn a_state_cannot_be_bound_to_two_containers() {
        use crate::runtime::composer::Composer;

        let state = SelectionState::new();
        let state_a = state.clone();
        let state_b = state.clone();
        let mut composer = Composer::new();
        composer.compose(move |ctx| {
            crate::layout::components::Column::new().build(ctx, move |ctx| {
                SelectionContainer::new().state(state_a.clone()).build(ctx, |ctx| {
                    crate::components::Text::new("one").build(ctx);
                });
                SelectionContainer::new().state(state_b.clone()).build(ctx, |ctx| {
                    crate::components::Text::new("two").build(ctx);
                });
            });
        });
    }

    /// S-7: when the LAST registered text leaves the tree, the container has to forget its segments.
    ///
    /// `reindex_selection_registrars` guards itself with "no node carries a registrar → return", and
    /// that guard is exactly what this case looks like: with the only text gone, no node reports a
    /// registrar, so the walk returned and the container kept answering `total_text_len() == 4`.
    /// Measured before the fix: `total` stayed 4 after the text was removed.
    #[test]
    fn a_container_whose_last_text_leaves_forgets_its_segments() {
        use crate::runtime::composer::Composer;
        use crate::runtime::state::State;
        use std::sync::{Arc, Mutex};

        let show = State::new(true);
        let seen: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer, show: &State<bool>, seen: &Arc<Mutex<Option<SelectionRegistrar>>>| {
            let show = show.clone();
            let seen = seen.clone();
            composer.compose(move |ctx| {
                let seen_body = seen.clone();
                SelectionContainer::new().build(ctx, move |ctx| {
                    *seen_body.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                    crate::layout::components::Column::new().build(ctx, move |ctx| {
                        // Read inside the body: a read outside never re-enters this group.
                        if show.get() {
                            crate::components::Text::new("gone").build(ctx);
                        }
                    });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        frame(&mut composer, &show, &seen);
        let reg = seen.lock().unwrap().clone().expect("registrar");
        assert_eq!(reg.total_text_len(), 4, "precondition: the text registered");

        show.set(false);
        frame(&mut composer, &show, &seen);
        assert_eq!(reg.total_text_len(), 0, "the table has to empty with the last text gone");
    }

    /// D-1: clearing a selection IS a state change, and the callback has to hear about it.
    ///
    /// Before this, `fire_on_change` returned early unless `build_selection` answered `Some`, so a
    /// consumer showing "N characters selected" never reset (measured: after `clear_selection()` the
    /// callback list still held only the previous selection).
    #[test]
    fn clearing_a_selection_notifies_with_an_empty_one() {
        use std::sync::Mutex;
        let reg = SelectionRegistrar::new();
        let seen = std::sync::Arc::new(Mutex::new(Vec::<(String, bool)>::new()));
        let sink = seen.clone();
        reg.set_on_change(move |sel| {
            sink.lock().unwrap().push((sel.text().to_string(), sel.is_empty()));
        });
        reg.register(1, "hello world");
        reg.set_selection(0, 5);
        reg.fire_on_change();
        reg.clear_selection();
        reg.fire_on_change();

        assert_eq!(
            *seen.lock().unwrap(),
            vec![("hello".to_string(), false), (String::new(), true)],
            "the clear is reported as an empty selection"
        );
    }

    /// With nothing ever selected the callback still learns there is nothing.
    #[test]
    fn on_change_reports_an_empty_selection_when_there_never_was_one() {
        use std::sync::Mutex;
        let reg = SelectionRegistrar::new();
        let empty = std::sync::Arc::new(Mutex::new(None::<bool>));
        let sink = empty.clone();
        reg.set_on_change(move |sel| { *sink.lock().unwrap() = Some(sel.is_empty()); });
        reg.fire_on_change();
        assert_eq!(*empty.lock().unwrap(), Some(true));
    }

    /// S-4: a collapsed caret inside a run is not a range. `build_selection` already said so;
    /// `selected_range` answered `Some(5..5)` for the same state.
    #[test]
    fn a_collapsed_selection_reports_no_range() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "hello world");
        reg.set_selection(5, 5);
        assert!(reg.selected_range(1).is_none(), "a caret is not a range");
        reg.set_selection(3, 8);
        assert_eq!(reg.selected_range(1), Some(3..8), "a real range still answers");
    }

    /// S-6: the order the container stitches in is the READING order, not the tree's.
    ///
    /// A `Row` of two `Column`s is laid out A1 A2 B1 B2 in the tree and A1 B1 A2 B2 on screen.
    /// Compose sorts its selectables by position for exactly this reason; without the sort a
    /// select-all copied "A1A2B1B2" (measured).
    #[test]
    fn a_two_column_layout_stitches_in_reading_order() {
        use crate::runtime::composer::Composer;
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
        let mut composer = Composer::new();
        {
            let seen = seen.clone();
            composer.compose(move |ctx| {
                SelectionContainer::new().build(ctx, move |ctx| {
                    *seen.lock().unwrap() = Some(LOCAL_SELECTION_REGISTRAR.current());
                    crate::layout::components::Row::new().build(ctx, |ctx| {
                        for (left, right) in [("A1", "A2"), ("B1", "B2")] {
                            crate::layout::components::Column::new().build(ctx, move |ctx| {
                                crate::components::Text::new(left)
                                    .modifier(Modifier::new().width(100.0))
                                    .build(ctx);
                                crate::components::Text::new(right)
                                    .modifier(Modifier::new().width(100.0))
                                    .build(ctx);
                            });
                        }
                    });
                });
            });
        }
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));

        let reg = seen.lock().unwrap().clone().expect("the container registered");
        assert_eq!(reg.total_text_len(), 8, "two columns of two two-byte labels");
        reg.set_selection(0, reg.total_text_len());
        assert_eq!(
            reg.build_selection().map(|s| s.text().to_string()),
            Some("A1B1A2B2".to_string()),
            "reading order, not the tree's A1A2B1B2"
        );
    }

    /// `reindex` keeps the segments the tree still reports, in the order the tree reports them.
    ///
    /// The old behaviour (a full clear on container re-entry) lost every text that skipped — see
    /// `docs/text-review.md` T-1.
    #[test]
    fn test_reindex_orders_by_tree_and_drops_dead_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");
        reg.register(2, "klmnopqrstuvwxy");
        assert_eq!(reg.total_text_len(), 25);
        // The tree reports them in the opposite order and no longer has slot 1.
        reg.reindex(&[2]);
        assert_eq!(reg.segment_info(2), Some((0, 15)), "the survivor moves to offset 0");
        assert_eq!(reg.segment_info(1), None, "a slot the tree dropped is gone");
        assert_eq!(reg.total_text_len(), 15, "no hole is left behind");
    }

    /// A text inserted between two existing ones lands in document order, not at the end.
    #[test]
    fn test_reindex_follows_document_order() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "AAA");
        reg.register(3, "CCC");
        assert_eq!(reg.segment_info(3), Some((3, 3)), "registered third, placed second");
        reg.register(2, "BBB");
        assert_eq!(reg.segment_info(2), Some((6, 3)), "registration order appends");
        // Layout says the document order is 1, 2, 3.
        reg.reindex(&[1, 2, 3]);
        assert_eq!(reg.segment_info(1), Some((0, 3)));
        assert_eq!(reg.segment_info(2), Some((3, 3)), "the inserted text sits in the middle");
        assert_eq!(reg.segment_info(3), Some((6, 3)));
        reg.set_selection(0, reg.total_text_len());
        assert_eq!(reg.build_selection().map(|s| s.text().to_string()), Some("AAABBBCCC".to_string()));
    }

    #[test]
    fn test_selection_across_three_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(10, &"x".repeat(100));  // offset=0
        reg.register(20, &"y".repeat(200));  // offset=100
        reg.register(30, &"z".repeat(50));   // offset=300
        // select spanning middle of 1st to middle of 3rd
        reg.set_selection(50, 320);
        assert_eq!(reg.selected_range(10), Some(50..100));   // local 50..100
        assert_eq!(reg.selected_range(20), Some(0..200));     // full second
        assert_eq!(reg.selected_range(30), Some(0..20));      // first 20 of third
    }

    #[test]
    fn test_reversed_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghijklmnopqrst");
        reg.set_selection(15, 5); // reversed: start > end
        assert_eq!(reg.selected_range(1), Some(5..15)); // normalized
    }

    #[test]
    fn test_selection_exactly_at_boundary() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");
        reg.register(2, "klmnopqrst"); // offset=10
        // selection at exact boundary
        reg.set_selection(10, 10); // zero-width
        assert!(reg.selected_range(1).is_none()); // local_end == 0
        assert!(reg.selected_range(2).is_none()); // local_start == 0, local_end == 0
    }

    #[test]
    fn test_selection_single_char_last_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");  // offset=0
        reg.register(2, "12345");   // offset=10
        reg.set_selection(13, 14);  // chars 13-14 in global = 3-4 in seg2
        assert_eq!(reg.selected_range(2), Some(3..4));
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_build_selection_single_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "Hello, world!");
        reg.set_selection(7, 12);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.start(), 7);
        assert_eq!(sel.end(), 12);
        assert_eq!(sel.text(), "world");
    }

    #[test]
    fn test_build_selection_cross_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "Hello! ");        // offset=0
        reg.register(2, "Drag me ");       // offset=7
        reg.register(3, "finish.");        // offset=15
        // Across three runs: the tail of the first, all of the second, the head of the third.
        reg.set_selection(4, 19);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.text(), "o! Drag me fini");
        // A reversed drag normalises its ends.
        reg.set_selection(19, 4);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.text(), "o! Drag me fini");
    }

    #[test]
    fn test_build_selection_emoji_boundary() {
        let reg = SelectionRegistrar::new();
        // The emoji 👋 (4 bytes) sits mid-run: an offset has to land on a character boundary for the
        // slice to succeed.
        reg.register(1, "Hi 👋 world");
        // Selecting "👋 wor" (bytes 3..11: H=0 i=1 sp=2, 👋=3..7, sp=7, w=8 o=9 r=10).
        reg.set_selection(3, 11);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.text(), "👋 wor");
        // A zero-width selection is `None`.
        reg.set_selection(5, 5);
        assert!(reg.build_selection().is_none());
    }

    #[test]
    fn test_build_selection_reversed() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdef");
        reg.set_selection(5, 2); // start > end
        let sel = reg.build_selection().unwrap();
        assert_eq!((sel.start(), sel.end()), (2, 5));
        assert_eq!(sel.text(), "cde");
    }
}

#[cfg(test)]
mod compute_selection_tests {
    use super::*;

    fn regs() -> (SelectionRegistrar, SelectionRegistrar) {
        (SelectionRegistrar::new(), SelectionRegistrar::new())
    }

    #[test]
    fn test_same_container_text_inner() {
        let (a, _) = regs();
        // Same container, over text: anchor 10 and current 20 give (10, 20).
        let r = compute_selection(Some(&a), Some(10), &a, Some(0), 20, 0.0, 100.0, 0.0, 50.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a));
        assert_eq!((s, e), (10, 20));
    }


    #[test]
    fn test_cross_container_uses_anchor_reg() {
        let (a, b) = regs();
        a.register(1, &"x".repeat(100)); // anchor 容器 total=100
        // Across containers: the anchor is in A while the pointer is over B, so the snap stays in
        // A's offset space.
        let r = compute_selection(Some(&a), Some(10), &b, Some(0), 5, 0.0, 200.0, 0.0, 50.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a), "应设 anchor 容器 a 的选择");
        assert_eq!((s, e), (10, 100)); // 向下拖出 a → clamp 到 a 末尾
    }

    /// A purely horizontal drag (a Row of containers): dy is 0, so the y-only rule used to answer
    /// "the end" for both directions. Left must reach the anchor's START.
    #[test]
    fn test_cross_container_leftward_drag_reaches_the_start() {
        let (a, b) = regs();
        a.register(1, &"x".repeat(100));
        let r = compute_selection(Some(&a), Some(40), &b, Some(0), 5, 100.0, 10.0, 208.0, 10.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a));
        assert_eq!((s, e), (0, 40), "dragging left selects back to the start, not to the end");
    }

    /// The same drag to the right reaches the END.
    #[test]
    fn test_cross_container_rightward_drag_reaches_the_end() {
        let (a, b) = regs();
        a.register(1, &"x".repeat(100));
        let r = compute_selection(Some(&a), Some(40), &b, Some(0), 5, 300.0, 10.0, 208.0, 10.0);
        let (_, s, e) = r.unwrap();
        assert_eq!((s, e), (40, 100));
    }

    #[test]
    fn test_no_anchor_container_returns_none() {
        let (_, b) = regs();
        // The press landed on something unselectable (`anchor_reg` is None), so a drag selects
        // nothing.
        let r = compute_selection(None, None, &b, Some(0), 5, 0.0, 200.0, 0.0, 50.0);
        assert!(r.is_none());
    }

    #[test]
    fn test_cross_container_no_anchor_global_none() {
        let (a, b) = regs();
        // Across containers with no `anchor_global` — a guard against a stale `anchor_reg`.
        let r = compute_selection(Some(&a), None, &b, Some(0), 5, 0.0, 200.0, 0.0, 50.0);
        assert!(r.is_none());
    }
}
