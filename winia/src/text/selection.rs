//! The selection model — what a `SelectionContainer` registers and what a text node reports into.
//!
//! It is held by `LayoutNode` (a text node keeps the registrar of the container above it) and read
//! by the renderer, so it cannot live in the component that provides it. `SelectionContainer` stays
//! where it is and provides the local declared here.

use crate::runtime::composition_local::CompositionLocal;
use parking_lot::Mutex;
use std::ops::Range;
use std::collections::HashMap;
use std::sync::Arc;

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
    /// Where the container's context menu opens from (see `set_context_menu_requested`).
    context_menu: Option<Arc<dyn Fn(f32, f32) + Send + Sync>>,
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

/// How a selection snaps to boundaries — Compose's `SelectionAdjustment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAdjustment {
    /// The raw offsets, unchanged.
    None,
    /// The whole word under the offset (a double click).
    Word,
    /// The whole line under the offset (a triple click, Compose's paragraph).
    Paragraph,
}

/// Split `text` into the run of same-class characters around `at`.
///
/// The classes are alphanumeric/underscore, whitespace, and everything else, so `foo.bar` gives
/// `foo`, `.`, `bar` — the same shape ICU's word breaker produces for Latin text. Deliberately
/// simple: a script without spaces (CJK) is one long word here, where Compose's ICU segmenter picks
/// a smaller unit; that deviation is recorded in `docs/selection-review.md` (M-5).
pub(crate) fn word_range(text: &str, at: usize) -> Range<usize> {
    let at = at.min(text.len());
    // Snap to a char boundary so a mid-character offset still answers a word.
    let at = if text.is_char_boundary(at) { at } else {
        let mut i = at;
        while i > 0 && !text.is_char_boundary(i) { i -= 1; }
        i
    };
    if text.is_empty() { return 0..0; }
    fn class(c: char) -> u8 {
        if c.is_alphanumeric() || c == '_' { 0 }
        else if c.is_whitespace() { 1 }
        else { 2 }
    }
    // The character the offset points AT; at `text.len()` take the last one.
    let probe = if at < text.len() {
        text[at..].chars().next().expect("a char starts at a boundary")
    } else {
        text[..at].chars().next_back().unwrap_or(' ')
    };
    let want = class(probe);
    let mut start = at;
    while start > 0 {
        let prev = text[..start].chars().next_back().expect("start > 0 has a char");
        if class(prev) != want { break; }
        start -= prev.len_utf8();
    }
    let mut end = at;
    while end < text.len() {
        let next = text[end..].chars().next().expect("end < len has a char");
        if class(next) != want { break; }
        end += next.len_utf8();
    }
    start..end
}

/// The line around `at` (Compose's `getParagraphBoundary`): `\n`-delimited.
pub(crate) fn line_range(text: &str, at: usize) -> Range<usize> {
    let at = at.min(text.len());
    let start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[at..].find('\n').map(|i| at + i).unwrap_or(text.len());
    start..end
}
/// The end of the next WORD at or after `at`, skipping separators — the step
/// `extendSelectionByWord` takes.
///
/// `None` when nothing word-like remains in this run, which lets the caller continue into the next
/// run instead of stopping on a trailing space.
pub(crate) fn next_word_end(text: &str, at: usize) -> Option<usize> {
    let mut pos = at.min(text.len());
    while pos < text.len() {
        let range = word_range(text, pos);
        let starts_with_word = text[range.clone()].chars().next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        if starts_with_word && range.end > at { return Some(range.end); }
        pos = if range.end > pos {
            range.end
        } else {
            pos + text[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)
        };
    }
    None
}

impl RegistrarInner {
    fn next_slot_after(&self, slot_key: u64) -> Option<u64> {
        let i = self.slot_index(slot_key)?;
        self.order.get(i + 1).copied()
    }

    fn previous_slot_before(&self, slot_key: u64) -> Option<u64> {
        let i = self.slot_index(slot_key)?;
        i.checked_sub(1).and_then(|j| self.order.get(j).copied())
    }
}

/// The start of the word BEFORE `at` (the mirror of [`next_word_end`]).
pub(crate) fn previous_word_start(text: &str, at: usize) -> Option<usize> {
    let mut pos = at.min(text.len());
    while pos > 0 {
        let range = word_range(text, pos - 1);
        let starts_with_word = text[range.clone()].chars().next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        if starts_with_word && range.start < at { return Some(range.start); }
        pos = if range.start < pos { range.start } else { pos - text[..pos].chars().next_back().map(|c| c.len_utf8()).unwrap_or(1) };
    }
    None
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
            context_menu: None,
        }));
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

    /// Select around `anchor_global` with a boundary adjustment (a double click asks for a word, a
    /// triple click for the line). Returns true when there is something to select.
    pub fn set_selection_by_adjustment(&self, anchor_global: usize, adjustment: SelectionAdjustment) -> bool {
        let mut inner = self.inner.lock();
        // The run the anchor falls in, in reading order; a past-the-end offset belongs to the last.
        let mut target: Option<(u64, usize)> = None;
        for &slot_key in &inner.order {
            let Some(seg) = inner.segments.get(&slot_key) else { continue };
            let end = seg.global_offset + seg.text_len;
            if anchor_global < end {
                target = Some((slot_key, anchor_global.saturating_sub(seg.global_offset)));
                break;
            }
            target = Some((slot_key, seg.text_len));
        }
        let Some((slot_key, local)) = target else { return false };
        let Some(seg) = inner.segments.get(&slot_key) else { return false };
        let (start, end) = match adjustment {
            SelectionAdjustment::None => (local, local),
            SelectionAdjustment::Word => {
                let range = word_range(&seg.text, local);
                (range.start, range.end)
            }
            SelectionAdjustment::Paragraph => {
                let range = line_range(&seg.text, local);
                (range.start, range.end)
            }
        };
        if start == end { return false; }
        let base = seg.global_offset;
        let (a, b) = (Anchor { slot_key, offset: start }, Anchor { slot_key, offset: end });
        inner.selection_start = Some(a);
        inner.selection_end = Some(b);
        let _ = base;
        true
    }

    /// Whether this is the same instance (by `Arc` identity). A cross-container drag uses it to
    /// decide which container owns the anchor, so the edge snap stays in one offset space.
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Every registered run's text, in reading order — Compose's `getSelectableTexts`.
    ///
    /// The order is the one `reindex` established, so it is what a select-all would produce.
    pub fn selectable_texts(&self) -> Vec<String> {
        let inner = self.inner.lock();
        inner.order.iter()
            .filter_map(|slot_key| inner.segments.get(slot_key))
            .map(|seg| seg.text.to_string())
            .collect()
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

    /// Extend the selection's end by one word — Compose's `extendSelectionByWord`.
    ///
    /// The walk crosses runs: a word that ends exactly where its run does continues into the next
    /// run, which is what Compose does (its `SelectionLayout` is container-wide).
    pub fn extend_selection_by_word(&self) -> bool {
        let mut inner = self.inner.lock();
        let Some((start, end)) = inner.anchors() else { return false };
        let Some(last_index) = inner.slot_index(end.slot_key) else { return false };
        // Find the first run at or after the end anchor that has a word to reach.
        let mut carried_whole = true;
        let mut new_end = end;
        for &slot_key in inner.order.iter().skip(last_index) {
            let Some(seg) = inner.segments.get(&slot_key) else { continue };
            let from = if slot_key == end.slot_key { end.offset } else { 0 };
            if let Some(target) = next_word_end(&seg.text, from) {
                new_end = Anchor { slot_key, offset: target };
                carried_whole = false;
                break;
            }
            // Nothing word-like left in this run: the next one starts from its beginning.
            if slot_key != end.slot_key { carried_whole = false; }
        }
        if carried_whole { return false; }
        if new_end.slot_key == end.slot_key && new_end.offset <= end.offset { return false; }
        inner.selection_start = Some(start);
        inner.selection_end = Some(new_end);
        true
    }

    /// Select everything in the container — Ctrl+A.
    pub fn select_all(&self) -> bool {
        let total = {
            let inner = self.inner.lock();
            if inner.segments.is_empty() { return false; }
            inner.next_global_offset
        };
        if total == 0 { return false; }
        self.set_selection(0, total);
        true
    }

    /// Extend the selection's ACTIVE end (the stored `selection_end`) by one character.
    ///
    /// Crossing a run boundary continues in the next/previous run, so a container made of several
    /// texts selects through it like one document (Compose's `SelectionLayout` is container-wide).
    pub fn extend_selection_by_character(&self, forward: bool) -> bool {
        self.move_end(|inner, anchor| {
            let seg = inner.segments.get(&anchor.slot_key)?;
            if forward {
                if anchor.offset < seg.text_len {
                    let step = seg.text[anchor.offset..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    return Some(Anchor { slot_key: anchor.slot_key, offset: anchor.offset + step });
                }
                let next = inner.next_slot_after(anchor.slot_key)?;
                Some(Anchor { slot_key: next, offset: 0 })
            } else {
                if anchor.offset > 0 {
                    let step = seg.text[..anchor.offset].chars().next_back().map(|c| c.len_utf8()).unwrap_or(1);
                    return Some(Anchor { slot_key: anchor.slot_key, offset: anchor.offset - step });
                }
                let prev = inner.previous_slot_before(anchor.slot_key)?;
                let prev_len = inner.segments.get(&prev)?.text_len;
                Some(Anchor { slot_key: prev, offset: prev_len })
            }
        })
    }

    /// Extend the active end by one word (Ctrl+Shift+arrow).
    pub fn extend_selection_by_word_step(&self, forward: bool) -> bool {
        self.move_end(|inner, anchor| {
            let seg = inner.segments.get(&anchor.slot_key)?;
            if forward {
                if let Some(target) = next_word_end(&seg.text, anchor.offset) {
                    return Some(Anchor { slot_key: anchor.slot_key, offset: target });
                }
                let next = inner.next_slot_after(anchor.slot_key)?;
                let next_len = inner.segments.get(&next)?.text_len;
                let target = next_word_end(&inner.segments.get(&next)?.text, 0).unwrap_or(next_len);
                Some(Anchor { slot_key: next, offset: target })
            } else {
                if let Some(target) = previous_word_start(&seg.text, anchor.offset) {
                    return Some(Anchor { slot_key: anchor.slot_key, offset: target });
                }
                let prev = inner.previous_slot_before(anchor.slot_key)?;
                let prev_len = inner.segments.get(&prev)?.text_len;
                let target = previous_word_start(&inner.segments.get(&prev)?.text, prev_len).unwrap_or(0);
                Some(Anchor { slot_key: prev, offset: target })
            }
        })
    }

    /// Extend the active end to the start or the end of its line (Shift+Home / Shift+End).
    pub fn extend_selection_to_line_edge(&self, to_end: bool) -> bool {
        self.move_end(|inner, anchor| {
            let seg = inner.segments.get(&anchor.slot_key)?;
            let range = line_range(&seg.text, anchor.offset);
            let target = if to_end { range.end } else { range.start };
            (target != anchor.offset).then_some(Anchor { slot_key: anchor.slot_key, offset: target })
        })
    }

    /// Extend the active end one line up or down (Shift+Up / Shift+Down).
    ///
    /// The column is kept as a BYTE offset inside the line, which is what the runs know: a real
    /// column would need the paragraph's x positions, which the registrar does not hold.
    pub fn extend_selection_by_line(&self, down: bool) -> bool {
        self.move_end(|inner, anchor| {
            let seg = inner.segments.get(&anchor.slot_key)?;
            let text = &seg.text;
            let here = line_range(text, anchor.offset);
            let column = anchor.offset - here.start;
            let target = if down {
                if here.end >= text.len() { return None; }
                let next = line_range(text, here.end + 1);
                next.start + column.min(next.end - next.start)
            } else {
                if here.start == 0 { return None; }
                let prev = line_range(text, here.start - 1);
                prev.start + column.min(prev.end - prev.start)
            };
            Some(Anchor { slot_key: anchor.slot_key, offset: target })
        })
    }

    /// Move the stored end anchor with `step`, then rewrite the selection.
    fn move_end(&self, step: impl FnOnce(&RegistrarInner, Anchor) -> Option<Anchor>) -> bool {
        let mut inner = self.inner.lock();
        let Some((_, end)) = inner.anchors() else { return false };
        let Some(target) = step(&inner, end) else { return false };
        if target.slot_key == end.slot_key && target.offset == end.offset { return false; }
        inner.selection_end = Some(target);
        true
    }

    /// The two ends of the selection as `(slot_key, local offset)`, in document order.
    ///
    /// A handle needs to know WHICH run and offset it sits at; the stitched `Selection` only
    /// carries global bytes.
    pub fn selection_ends(&self) -> Option<((u64, usize), (u64, usize))> {
        let inner = self.inner.lock();
        let (a, b) = inner.anchors()?;
        let order = |anchor: Anchor| inner.slot_index(anchor.slot_key).unwrap_or(usize::MAX);
        if (order(a), a.offset) <= (order(b), b.offset) {
            Some(((a.slot_key, a.offset), (b.slot_key, b.offset)))
        } else {
            Some(((b.slot_key, b.offset), (a.slot_key, a.offset)))
        }
    }

    /// The global offset of one end, for seeding a handle drag from the OTHER end.
    pub fn end_global(&self, slot_key: u64, offset: usize) -> Option<usize> {
        self.inner.lock().global_of(Anchor { slot_key, offset })
    }

    /// Install the container's context-menu opener: the pointer path knows WHERE the press was, the
    /// container owns the menu.
    pub fn set_context_menu_requested(&self, f: impl Fn(f32, f32) + Send + Sync + 'static) {
        self.inner.lock().context_menu = Some(Arc::new(f));
    }

    /// Ask the container to open its menu at a scene position. False when it has none.
    pub fn request_context_menu(&self, x: f32, y: f32) -> bool {
        let cb = self.inner.lock().context_menu.clone();
        let Some(cb) = cb else { return false };
        cb(x, y);
        true
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

/// Whether selectable children are disabled in this subtree (Compose's `DisableSelection`).
pub static LOCAL_SELECTION_DISABLED: std::sync::LazyLock<CompositionLocal<bool>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| false));

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
#[cfg(test)]
mod boundary_tests {
    use super::*;

    fn word(text: &str, at: usize) -> &str {
        let r = word_range(text, at);
        &text[r]
    }

    #[test]
    fn a_word_stops_at_punctuation_and_spaces() {
        assert_eq!(word("hello world", 7), "world");
        assert_eq!(word("hello world", 0), "hello");
        assert_eq!(word("foo.bar", 1), "foo");
        assert_eq!(word("foo.bar", 3), ".");
        assert_eq!(word("foo.bar", 5), "bar");
        assert_eq!(word("two  spaces", 4), "  ", "a separator run is its own word");
    }

    #[test]
    fn a_word_handles_multibyte_text_without_panicking() {
        // Two CJK characters (three bytes each) are one alphanumeric run.
        let text = "\u{4f60}\u{597d} ab";
        assert_eq!(word(text, 1), "\u{4f60}\u{597d}");
        assert_eq!(word(text, 3), "\u{4f60}\u{597d}");
        assert_eq!(word(text, 8), "ab");
        // A mid-character offset snaps to the character start instead of slicing inside it.
        assert_eq!(word(text, 1), "\u{4f60}\u{597d}");
    }

    #[test]
    fn a_line_stops_at_newlines() {
        let text = "first\nsecond line\nthird";
        let r = line_range(text, 8);
        assert_eq!(&text[r], "second line");
        assert_eq!(&text[line_range(text, 0)], "first");
        assert_eq!(&text[line_range(text, text.len())], "third");
    }

    #[test]
    fn next_word_end_skips_separators() {
        let text = "hello world";
        assert_eq!(next_word_end(text, 0), Some(5), "from inside the first word: its end");
        assert_eq!(next_word_end(text, 5), Some(11), "from the space: the next word's end");
        assert_eq!(next_word_end(text, 11), None, "at the end: nothing left in this run");
    }
}
#[cfg(test)]
mod keyboard_selection_tests {
    use super::*;

    /// Two runs, as a container of two texts registers them.
    fn two_runs() -> SelectionRegistrar {
        let reg = SelectionRegistrar::new();
        reg.register(1, "hello ");
        reg.register(2, "world");
        reg.reindex(&[1, 2]);
        reg
    }

    fn one_run(text: &str) -> SelectionRegistrar {
        let reg = SelectionRegistrar::new();
        reg.register(1, text);
        reg.reindex(&[1]);
        reg
    }

    fn range(reg: &SelectionRegistrar) -> Option<(usize, usize)> {
        reg.build_selection().map(|s| (s.start(), s.end()))
    }

    #[test]
    fn a_character_step_crosses_into_the_next_run() {
        let reg = two_runs();
        reg.set_selection(0, 6);
        assert_eq!(range(&reg), Some((0, 6)));
        assert!(reg.extend_selection_by_character(true), "the end steps into the second run");
        assert_eq!(range(&reg), Some((0, 7)));
        assert!(reg.extend_selection_by_character(false));
        assert_eq!(range(&reg), Some((0, 6)));
    }

    #[test]
    fn a_character_step_backwards_shrinks_by_one() {
        let reg = one_run("abcd");
        reg.set_selection(1, 4);
        assert!(reg.extend_selection_by_character(false));
        assert_eq!(range(&reg), Some((1, 3)));
    }

    #[test]
    fn a_word_step_reaches_the_word_boundaries() {
        let reg = one_run("hello world");
        reg.set_selection(0, 0);
        assert!(reg.extend_selection_by_word_step(true));
        assert_eq!(range(&reg), Some((0, 5)), "the first word");
        assert!(reg.extend_selection_by_word_step(true));
        assert_eq!(range(&reg), Some((0, 11)), "then the second");
        assert!(reg.extend_selection_by_word_step(false));
        assert_eq!(range(&reg), Some((0, 6)), "and back to the start of the second");
    }

    #[test]
    fn a_line_edge_step_uses_the_newline_boundaries() {
        let reg = one_run("aa\nbbbb");
        reg.set_selection(0, 0);
        assert!(reg.extend_selection_to_line_edge(true));
        assert_eq!(range(&reg), Some((0, 2)), "to the end of the first line");
        // A caret at 6 sits inside the second line ("aa\nbbbb": line one is 0..2, line two starts
        // at 3), so Shift+Home selects from the line start to the caret.
        reg.set_selection(6, 6);
        assert!(reg.extend_selection_to_line_edge(false));
        assert_eq!(range(&reg), Some((3, 6)), "home goes to the start of the caret's line");
    }

    #[test]
    fn a_line_step_keeps_the_column() {
        let reg = one_run("aa\nbbbb");
        reg.set_selection(1, 1);
        assert!(reg.extend_selection_by_line(true));
        assert_eq!(range(&reg), Some((1, 4)), "down one line, same column");
        // Up from (line two, column 1) lands on (line one, column 1) — the stored END anchor moves
        // onto the start anchor, which is a collapsed selection and therefore reported as none.
        reg.set_selection(1, 4);
        assert!(reg.extend_selection_by_line(false));
        assert_eq!(range(&reg), None, "the end lands on the start: a collapsed selection");
    }

    #[test]
    fn select_all_covers_every_run() {
        let reg = two_runs();
        assert!(reg.select_all());
        assert_eq!(range(&reg), Some((0, 11)));
        assert_eq!(reg.build_selection().map(|s| s.text().to_string()), Some("hello world".to_string()));
        let empty = SelectionRegistrar::new();
        assert!(!empty.select_all(), "nothing to select");
    }

    #[test]
    fn the_previous_word_start_mirrors_next_word_end() {
        assert_eq!(previous_word_start("hello world", 11), Some(6));
        // At the start of "world" the previous word is "hello", so its start is what a
        // Ctrl+Shift+Left has to land on.
        assert_eq!(previous_word_start("hello world", 6), Some(0));
        assert_eq!(previous_word_start("hello world", 0), None);
    }
}