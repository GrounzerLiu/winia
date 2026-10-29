//! `SelectionContainer` — Compose's text-selection container.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::runtime::composer::GroupStatus;



use crate::text::selection::{LOCAL_SELECTION_REGISTRAR, Selection, SelectionRegistrar};
// ═══════════════════════════════════════════════════════════
// SelectionContainer
// ═══════════════════════════════════════════════════════════

pub struct SelectionContainer {
    modifier: Modifier,
    on_change: Option<Box<dyn Fn(&Selection) + Send + Sync>>,
}

impl SelectionContainer {
    pub fn new() -> Self {
        SelectionContainer { modifier: Modifier::new(), on_change: None }
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

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // One persistent registrar: a rebuild must not create a new one, because the segments have
        // to survive recomposition.
        let registrar = ctx.remember_at_key(key, || SelectionRegistrar::new()).get();
        if let Some(cb) = self.on_change {
            registrar.set_on_change(cb);
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
/// Four cases:
/// - Same container, pointer over text: `anchor_global .. current_global` in that container's
/// - Same container, pointer in the empty area: snap to the container edge (0 above, total below).
/// - Across containers (the pointer is over another container's text): snap within the ANCHOR's
///   container. Switching to the other container's offset space would mix an anchor global offset
///   with a different container's origin and select the wrong text.
/// - No anchor container (the press landed on something unselectable) -> `None`.
pub(crate) fn compute_selection(
    anchor_reg: Option<&SelectionRegistrar>,
    anchor_global: Option<usize>,
    cur_reg: &SelectionRegistrar,
    cur_global_off: Option<usize>,
    cur_index: usize,
    scene_y: f32,
    down_y: f32,
    node_abs_y: f32,
) -> Option<(SelectionRegistrar, usize, usize)> {
    let same_reg = anchor_reg.map(|ar| ar.is_same(cur_reg)).unwrap_or(false);
    if same_reg {
        if let Some(off) = cur_global_off {
            let current_global = off + cur_index;
            let s = anchor_global.map(|a| a.min(current_global)).unwrap_or(current_global);
            let e = anchor_global.map(|a| a.max(current_global)).unwrap_or(current_global + 1);
            Some((cur_reg.clone(), s, e))
        } else if let Some(a) = anchor_global {
            // Same container, past its end: snap to the edge.
            let edge = if scene_y < node_abs_y { 0 } else { cur_reg.total_text_len() };
            let s = a.min(edge);
            let e = a.max(edge);
            Some((cur_reg.clone(), s, e))
        } else {
            None
        }
    } else if let (Some(anchor_reg), Some(a)) = (anchor_reg, anchor_global) {
        // Across containers: snap inside the anchor's container, clamping to its bounds.
        let total = anchor_reg.total_text_len();
        let edge = if scene_y < down_y { 0 } else { total };
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
    fn test_on_change_not_called_when_no_selection() {
        use std::sync::Mutex;
        let reg = SelectionRegistrar::new();
        let called = std::sync::Arc::new(Mutex::new(false));
        let c = called.clone();
        reg.set_on_change(move |_| { *c.lock().unwrap() = true; });
        reg.fire_on_change(); // no selection set → should not fire
        assert!(!*called.lock().unwrap());
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
        let r = compute_selection(Some(&a), Some(10), &a, Some(0), 20, 100.0, 50.0, 50.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a));
        assert_eq!((s, e), (10, 20));
    }

    #[test]
    fn test_same_container_edge_snap_down() {
        let (a, _) = regs();
        a.register(1, &"x".repeat(100)); // total=100
        // Same container, dragged below the node: the edge is the total, 100.
        let r = compute_selection(Some(&a), Some(5), &a, None, 0, 300.0, 50.0, 100.0);
        let (_, s, e) = r.unwrap();
        assert_eq!((s, e), (5, 100));
    }

    #[test]
    fn test_same_container_edge_snap_up() {
        let (a, _) = regs();
        // Same container, dragged above the node: the edge is 0.
        let r = compute_selection(Some(&a), Some(5), &a, None, 0, 10.0, 50.0, 100.0);
        let (_, s, e) = r.unwrap();
        assert_eq!((s, e), (0, 5));
    }

    #[test]
    fn test_cross_container_uses_anchor_reg() {
        let (a, b) = regs();
        a.register(1, &"x".repeat(100)); // anchor 容器 total=100
        // Across containers: the anchor is in A while the pointer is over B, so the snap stays in
        // A's offset space.
        let r = compute_selection(Some(&a), Some(10), &b, Some(0), 5, 200.0, 50.0, 50.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a), "应设 anchor 容器 a 的选择");
        assert_eq!((s, e), (10, 100)); // 向下拖出 a → clamp 到 a 末尾
    }

    #[test]
    fn test_no_anchor_container_returns_none() {
        let (_, b) = regs();
        // The press landed on something unselectable (`anchor_reg` is None), so a drag selects
        // nothing.
        let r = compute_selection(None, None, &b, Some(0), 5, 200.0, 50.0, 50.0);
        assert!(r.is_none());
    }

    #[test]
    fn test_cross_container_no_anchor_global_none() {
        let (a, b) = regs();
        // Across containers with no `anchor_global` — a guard against a stale `anchor_reg`.
        let r = compute_selection(Some(&a), None, &b, Some(0), 5, 200.0, 50.0, 50.0);
        assert!(r.is_none());
    }
}
