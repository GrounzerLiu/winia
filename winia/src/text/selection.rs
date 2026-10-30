//! The selection model — what a `SelectionContainer` registers and what a text node reports into.
//!
//! It is held by `LayoutNode` (a text node keeps the registrar of the container above it) and read
//! by the renderer, so it cannot live in the component that provides it. `SelectionContainer` stays
//! where it is and provides the local declared here.

use crate::runtime::composition_local::CompositionLocal;
use parking_lot::Mutex;
use std::cell::RefCell;
use std::ops::Range;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

// ═══════════════════════════════════════════════════════════
// Supporting types.
// ═══════════════════════════════════════════════════════════

/// One registered run of text.
#[derive(Debug, Clone)]
pub(crate) struct RegisteredSegment {
    /// The slot that registered this segment. Kept because it is the record's identity — the
    /// registration site knows it and cannot be asked for it later; no reader needs it yet.
    #[allow(dead_code)]
    pub slot_key: u64,
    pub global_offset: usize,
    pub text_len: usize,
    /// The run's own text, captured at registration so the framework can stitch a selection
    /// together without the caller keeping a parallel copy.
    pub text: Arc<str>,
}

// ═══════════════════════════════════════════════════════════
// SelectionRegistrar
// ═══════════════════════════════════════════════════════════

type OnChangeFn = Arc<dyn Fn(&Selection) + Send + Sync>;

/// A selection — the callback's argument. Like Compose's `Selection`, it carries the selected text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    start: usize,
    end: usize,
    text: String,
}

impl Selection {
    /// The start offset across the whole container, over every registered run.
    pub fn start(&self) -> usize { self.start }
    /// The end offset across the whole container.
    pub fn end(&self) -> usize { self.end }
    /// The selected text, stitched from the registered runs (safe across run boundaries and
    /// emoji graphemes).
    pub fn text(&self) -> &str { &self.text }
    /// No selection at all: what the callback receives when the user clears one.
    ///
    /// It is a `Selection` rather than an `Option` so the callback signature stays one type; a
    /// consumer that shows "N characters selected" resets on `is_empty()`. Before this, clearing
    /// fired NOTHING (measured: after `clear_selection()` the callback list still held only the
    /// previous selection), so such a label stayed at the old count forever.
    pub fn is_empty(&self) -> bool { self.start == self.end && self.text.is_empty() }
    pub(crate) fn empty() -> Self { Selection { start: 0, end: 0, text: String::new() } }
}

/// One end of a selection: WHICH run, and where inside it.
///
/// A drag produces global offsets (they span every run in the container), but storing the selection
/// that way makes it follow the OFFSET rather than the characters: a run inserted or removed above
/// it moves the highlight onto different text. Measured before this: selecting "BBB" and inserting
/// "AAA" above left the range at `0..3`, which then read "AAA", and removing the earlier run
/// dropped the selection outright. Compose stores an anchor per end —
/// `AnchorInfo(selectableId, offset, direction)` — so this is the same idea: a slot key plus an
/// offset inside that run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Anchor {
    slot_key: u64,
    offset: usize,
}

#[derive(Clone)]
struct RegistrarInner {
    /// The two ends, in no particular order; `anchors()` puts them in document order.
    selection_start: Option<Anchor>,
    selection_end: Option<Anchor>,
    /// The runs in document order. `reindex` rewrites this from the layout tree every pass (tree
    /// order is not reading order — see `Composer::reindex_selection_registrars`); `register`
    /// appends a run it has not seen before.
    order: Vec<u64>,
    next_global_offset: usize,
    segments: HashMap<u64, RegisteredSegment>,
    on_change: Option<OnChangeFn>,
}

impl RegistrarInner {
    fn slot_index(&self, slot_key: u64) -> Option<usize> {
        self.order.iter().position(|&key| key == slot_key)
    }

    /// The two ends in document order, or `None` when either end's run has left the table (the
    /// selection then names nothing, and every query answers "nothing").
    fn anchors(&self) -> Option<(Anchor, Anchor)> {
        let (a, b) = (self.selection_start?, self.selection_end?);
        let (ia, ib) = (self.slot_index(a.slot_key)?, self.slot_index(b.slot_key)?);
        if (ia, a.offset) <= (ib, b.offset) { Some((a, b)) } else { Some((b, a)) }
    }

    /// Where an end sits in the CURRENT global space. The offsets move as runs come and go; the
    /// anchor does not.
    fn global_of(&self, anchor: Anchor) -> Option<usize> {
        let seg = self.segments.get(&anchor.slot_key)?;
        Some(seg.global_offset + anchor.offset.min(seg.text_len))
    }

    /// The anchor a global offset names. An offset past the last run's end anchors to that run's
    /// end, which is what the drag path produces when it snaps to a container edge.
    fn anchor_at(&self, global: usize) -> Option<Anchor> {
        let mut last = None;
        for &slot_key in &self.order {
            let Some(seg) = self.segments.get(&slot_key) else { continue };
            let end = seg.global_offset + seg.text_len;
            if global < end {
                return Some(Anchor { slot_key, offset: global.saturating_sub(seg.global_offset) });
            }
            last = Some(Anchor { slot_key, offset: seg.text_len });
        }
        last
    }

    /// Which part of `slot_key`'s run the selection covers, in that run's own offsets.
    fn local_range(&self, slot_key: u64) -> Option<Range<usize>> {
        let (a, b) = self.anchors()?;
        let seg = self.segments.get(&slot_key)?;
        let (ia, ib) = (self.slot_index(a.slot_key)?, self.slot_index(b.slot_key)?);
        let ik = self.slot_index(slot_key)?;
        let (start, end) = if ik == ia && ik == ib {
            (a.offset.min(b.offset), a.offset.max(b.offset))
        } else if ik == ia {
            (a.offset, seg.text_len)
        } else if ik == ib {
            (0, b.offset)
        } else if ik > ia && ik < ib {
            (0, seg.text_len)
        } else {
            return None;
        };
        let (start, end) = (start.min(seg.text_len), end.min(seg.text_len));
        // A collapsed caret is not a range. `build_selection` says so, and a consumer that branches
        // on `is_some()` (the uncached text-field fallback) used to paint a band for one.
        if start == end { None } else { Some(start..end) }
    }

    /// Whether the selection covers `slot_key` at all, edges included.
    fn selection_touches(&self, slot_key: u64) -> bool {
        let Some((a, b)) = self.anchors() else { return false };
        let (Some(ia), Some(ib), Some(ik)) = (
            self.slot_index(a.slot_key),
            self.slot_index(b.slot_key),
            self.slot_index(slot_key),
        ) else {
            return false;
        };
        ik >= ia && ik <= ib
    }

    fn clear(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    fn has_selection(&self) -> bool {
        self.selection_start.is_some() || self.selection_end.is_some()
    }
}

impl std::fmt::Debug for RegistrarInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrarInner")
            .field("selection_start", &self.selection_start)
            .field("selection_end", &self.selection_end)
            .field("has_cb", &self.on_change.is_some())
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SelectionRegistrar {
    inner: Arc<Mutex<RegistrarInner>>,
}

thread_local! {
    /// Every registrar alive on this thread, so a press outside any of them can clear the one that
    /// owns the open selection. Weak: a container that leaves composition drops out on its own, and
    /// the list is pruned on every use (`clear_open_selections`).
    static LIVE_REGISTRARS: RefCell<Vec<Weak<Mutex<RegistrarInner>>>> = const { RefCell::new(Vec::new()) };
}

/// Clear the selection of every registrar that currently has one.
///
/// A press that is not continuing a selection ends it (Compose: `onClearSelectionRequested` / the tap
/// handler call `onRelease`). The press does not know which container owns the selection — it lands
/// on a node, and only registered text leaves carry a registrar — so this sweeps the live ones
/// instead of trusting a hit.
pub(crate) fn clear_open_selections() {
    LIVE_REGISTRARS.with(|live| {
        let mut live = live.borrow_mut();
        live.retain(|weak| {
            let Some(inner) = weak.upgrade() else { return false };
            let mut inner = inner.lock();
            if inner.has_selection() {
                inner.clear();
            }
            true
        });
    });
}

impl SelectionRegistrar {
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(RegistrarInner {
            selection_start: None,
            selection_end: None,
            order: Vec::new(),
            next_global_offset: 0,
            segments: HashMap::new(),
            on_change: None,
        }));
        LIVE_REGISTRARS.with(|live| {
            let mut live = live.borrow_mut();
            live.retain(|weak| weak.strong_count() > 0);
            live.push(Arc::downgrade(&inner));
        });
        Self { inner }
    }

    pub fn register(&self, slot_key: u64, text: &str) -> usize {
        let mut inner = self.inner.lock();
        let text_len = text.len();
        let (offset, stored, content_changed) = match inner.segments.get(&slot_key) {
            Some(existing) => {
                let changed = existing.text.as_ref() != text;
                // Reuse the string when it did not change: a re-register happens on every entry
                // into the run, and a fresh `Arc<str>` per frame is a per-frame allocation.
                let stored = if changed { Arc::from(text) } else { existing.text.clone() };
                (existing.global_offset, stored, changed)
            }
            None => {
                let off = inner.next_global_offset;
                inner.next_global_offset += text_len;
                inner.order.push(slot_key);
                (off, Arc::from(text), false)
            }
        };
        // Keep the stitching invariant: the registered ranges must not overlap. When a re-register
        // covers longer text, the global offset advances; otherwise later runs would start inside
        // this one.
        inner.next_global_offset = inner.next_global_offset.max(offset + text_len);
        inner.segments.insert(slot_key, RegisteredSegment {
            slot_key,
            global_offset: offset,
            text_len,
            text: stored,
        });
        // Compose ends the selection when the content of a selectable it covers changes
        // (`onSelectableChangeCallback` -> `onRelease(); selection = null`), because the offsets it
        // remembered no longer name the same characters. An anchor survives a sibling coming or
        // going, but not its own run being rewritten.
        if content_changed && inner.selection_touches(slot_key) {
            inner.clear();
        }
        offset
    }

    /// Select a range given in GLOBAL offsets, the way a drag reports it.
    pub fn set_selection(&self, start: usize, end: usize) {
        let mut inner = self.inner.lock();
        let (lo, hi) = (start.min(end), start.max(end));
        let (Some(a), Some(b)) = (inner.anchor_at(lo), inner.anchor_at(hi)) else {
            // No runs to anchor to: there is nothing to select.
            inner.clear();
            return;
        };
        inner.selection_start = Some(a);
        inner.selection_end = Some(b);
    }

    pub fn clear_selection(&self) {
        self.inner.lock().clear();
    }

    /// Whether this is the same instance (by `Arc` identity). A cross-container drag uses it to
    /// decide which container owns the anchor, so the edge snap stays in one offset space.
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn segment_info(&self, slot_key: u64) -> Option<(usize, usize)> {
        let inner = self.inner.lock();
        inner.segments.get(&slot_key).map(|s| (s.global_offset, s.text_len))
    }

    /// Recompute every run's offset from the ORDER the layout tree reports, and drop the runs whose
    /// slot is no longer in the tree.
    ///
    /// This is the one place offsets are authoritative. A container re-entering composition does NOT
    /// mean its texts re-run — a `Text` whose parameters did not change Skips, and a skipped text
    /// never calls [`Self::register`]. Wiping the table on re-entry therefore lost every text that
    /// skipped (measured: a State read in the container body took `total` from 10 to 0 and the
    /// selection from Some to None). Rebuilding from the tree instead keeps a skipped text's
    /// segment and still drops a text that actually left the tree, because the walk sees only live
    /// nodes.
    ///
    /// Order is document order — the order Compose's `SelectionRegistrarImpl` gets from layout —
    /// not registration order: a text inserted between two existing ones must land between them.
    /// The ORDER is what the selection is read through, which is why it is stored rather than
    /// derived: with anchors, a run appearing above the selection moves every offset below it while
    /// the selection itself stays on its own characters.
    pub(crate) fn reindex(&self, ordered_slots: &[u64]) {
        let mut inner = self.inner.lock();
        let mut next_offset = 0usize;
        let mut rebuilt: HashMap<u64, RegisteredSegment> = HashMap::with_capacity(ordered_slots.len());
        let mut order: Vec<u64> = Vec::with_capacity(ordered_slots.len());
        for &slot_key in ordered_slots {
            if let Some(mut seg) = inner.segments.remove(&slot_key) {
                seg.global_offset = next_offset;
                next_offset += seg.text_len;
                order.push(slot_key);
                rebuilt.insert(slot_key, seg);
            }
        }
        // Anything left in `segments` belongs to a slot the tree did not report: it left.
        inner.segments = rebuilt;
        inner.order = order;
        inner.next_global_offset = next_offset;
        // An anchor whose run left the table names nothing. End the selection rather than let it
        // slide onto whatever now occupies those offsets.
        let alive = match (inner.selection_start, inner.selection_end) {
            (Some(a), Some(b)) => {
                inner.segments.contains_key(&a.slot_key) && inner.segments.contains_key(&b.slot_key)
            }
            _ => true,
        };
        if !alive {
            inner.clear();
        }
    }

    pub fn total_text_len(&self) -> usize {
        self.inner.lock().next_global_offset
    }

    pub fn selected_range(&self, slot_key: u64) -> Option<Range<usize>> {
        self.inner.lock().local_range(slot_key)
    }

    /// Install the selection-changed callback. It receives a [`Selection`] with the text already
    /// stitched, so the caller keeps no parallel copy.
    pub fn set_on_change(&self, f: impl Fn(&Selection) + Send + Sync + 'static) {
        self.inner.lock().on_change = Some(Arc::new(f));
    }

    /// Build the current [`Selection`], stitching the runs the anchors cover (safe across run
    /// boundaries and emoji graphemes).
    ///
    /// `start`/`end` are the anchors' offsets in the CURRENT global space, so they follow a run
    /// that appears above the selection even though the anchor does not move.
    pub(crate) fn build_selection(&self) -> Option<Selection> {
        let inner = self.inner.lock();
        let (a, b) = inner.anchors()?;
        let (gs, ge) = (inner.global_of(a)?, inner.global_of(b)?);
        let (start, end) = (gs.min(ge), gs.max(ge));
        // Zero width (a click with no drag) is no selection, matching `selected_range`.
        if start == end { return None; }
        let mut text = String::new();
        for &slot_key in &inner.order {
            let Some(range) = inner.local_range(slot_key) else { continue };
            let Some(seg) = inner.segments.get(&slot_key) else { continue };
            // The offsets are meant to be character boundaries (`get_closest_*` guarantees it), but
            // a cross-run stitch uses `get` so a bad one skips its run instead of panicking on an
            // out-of-range or non-boundary slice.
            if let Some(part) = seg.text.get(range) {
                text.push_str(part);
            }
        }
        // A rich-text inline element (image or placeholder) is a U+FFFC here. Copying one would give
        // the user a replacement character, so it is dropped from the selected text.
        if text.contains('\u{FFFC}') {
            text = text.replace('\u{FFFC}', "");
        }
        Some(Selection { start, end, text })
    }

    pub(crate) fn fire_on_change(&self) {
        let cb = self.inner.lock().on_change.clone();
        let Some(cb) = cb else { return };
        match self.build_selection() {
            Some(sel) => cb(&sel),
            // Nothing selected is a state change too (`Selection::is_empty`).
            None => cb(&Selection::empty()),
        }
    }
}
impl Default for SelectionRegistrar {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// LOCAL_SELECTION_REGISTRAR
// ═══════════════════════════════════════════════════════════

pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));

// ═══════════════════════════════════════════════════════════
// TextSelectionColors
// ═══════════════════════════════════════════════════════════

/// The colours a selection is drawn in — Compose's `TextSelectionColors`.
///
/// `handle` is carried for API parity and is unused until the container has drag handles (M-3 in
/// `docs/selection-review.md`); only `background` reaches the renderer today.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSelectionColors {
    pub handle: crate::graphics::Color,
    pub background: crate::graphics::Color,
}

impl TextSelectionColors {
    pub fn new(handle: crate::graphics::Color, background: crate::graphics::Color) -> Self {
        Self { handle, background }
    }
}

/// Compose's `DefaultTextSelectionColors`: the Compose-logo blue, and the same blue at 40% for the
/// highlight (`DefaultTextSelectionColors.android.kt`, `Color(0xFF4286F4).copy(alpha = 0.4f)`).
impl Default for TextSelectionColors {
    fn default() -> Self {
        let base = crate::graphics::Color::from_argb(255, 0x42, 0x86, 0xF4);
        Self { handle: base, background: crate::graphics::Color::from_argb(102, 0x42, 0x86, 0xF4) }
    }
}

/// The selection colours for a subtree — Compose's `LocalTextSelectionColors`.
pub static LOCAL_TEXT_SELECTION_COLORS: std::sync::LazyLock<CompositionLocal<TextSelectionColors>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(TextSelectionColors::default));

#[cfg(test)]
mod text_selection_colors_tests {
    use super::*;

    /// Compose's `DefaultTextSelectionColors` is `Color(0xFF4286F4)` and the same colour at 40%
    /// alpha, so a caller that ports Compose code sees the same highlight.
    #[test]
    fn the_defaults_are_composes() {
        let colors = TextSelectionColors::default();
        assert_eq!((colors.handle.r, colors.handle.g, colors.handle.b, colors.handle.a), (0x42, 0x86, 0xF4, 255));
        assert_eq!((colors.background.r, colors.background.g, colors.background.b), (0x42, 0x86, 0xF4));
        assert_eq!(colors.background.a, 102, "0.4 * 255");
    }
}