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
}

#[derive(Clone)]
struct RegistrarInner {
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    next_global_offset: usize,
    segments: HashMap<u64, RegisteredSegment>,
    on_change: Option<OnChangeFn>,
}

impl std::fmt::Debug for RegistrarInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrarInner").field("selection_start", &self.selection_start).field("selection_end", &self.selection_end).field("has_cb", &self.on_change.is_some()).finish()
    }
}

#[derive(Debug, Clone)]
pub struct SelectionRegistrar {
    inner: Arc<Mutex<RegistrarInner>>,
}

impl SelectionRegistrar {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistrarInner {
                selection_start: None,
                selection_end: None,
                next_global_offset: 0,
                segments: HashMap::new(),
                on_change: None,
            })),
        }
    }

    pub fn register(&self, slot_key: u64, text: &str) -> usize {
        let mut inner = self.inner.lock();
        let text_len = text.len();
        let offset = if let Some(existing) = inner.segments.get(&slot_key) {
            existing.global_offset
        } else {
            let off = inner.next_global_offset;
            inner.next_global_offset += text_len;
            off
        };
        // Keep the stitching invariant: the registered ranges must not overlap. When a re-register
        // covers longer text, the global offset advances; otherwise later runs would start inside
        // this one and `build_selection` would duplicate or drop characters.
        inner.next_global_offset = inner.next_global_offset.max(offset + text_len);
        inner.segments.insert(slot_key, RegisteredSegment {
            slot_key, global_offset: offset, text_len,
            text: text.into(),
        });
        offset
    }

    pub fn set_selection(&self, start: usize, end: usize) {
        let mut inner = self.inner.lock();
        inner.selection_start = Some(start.min(end));
        inner.selection_end = Some(start.max(end));
    }

    pub fn clear_selection(&self) {
        let mut inner = self.inner.lock();
        inner.selection_start = None;
        inner.selection_end = None;
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

    /// Recompute every segment's offset from the ORDER the layout tree reports, and drop the
    /// segments whose slot is no longer in the tree.
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
    /// Offsets are contiguous again afterwards, so a shrink does not leave a hole.
    pub(crate) fn reindex(&self, ordered_slots: &[u64]) {
        let mut inner = self.inner.lock();
        let mut next_offset = 0usize;
        let mut rebuilt: HashMap<u64, RegisteredSegment> = HashMap::with_capacity(ordered_slots.len());
        for &slot_key in ordered_slots {
            if let Some(mut seg) = inner.segments.remove(&slot_key) {
                seg.global_offset = next_offset;
                next_offset += seg.text_len;
                rebuilt.insert(slot_key, seg);
            }
        }
        // Anything left in `segments` belongs to a slot the tree did not report: it left.
        inner.segments = rebuilt;
        inner.next_global_offset = next_offset;
        // The selection space just changed under the stored range; clamp instead of leaving a range
        // that points past the end (`build_selection` would clamp anyway, `selected_range` would not).
        if let Some(s) = inner.selection_start { inner.selection_start = Some(s.min(next_offset)); }
        if let Some(e) = inner.selection_end { inner.selection_end = Some(e.min(next_offset)); }
    }

    pub fn total_text_len(&self) -> usize {
        self.inner.lock().next_global_offset
    }

    pub fn selected_range(&self, slot_key: u64) -> Option<Range<usize>> {
        let inner = self.inner.lock();
        let (global_start, global_end) = (inner.selection_start?, inner.selection_end?);
        let seg = inner.segments.get(&slot_key)?;
        let local_start = global_start.saturating_sub(seg.global_offset);
        let local_end = global_end.saturating_sub(seg.global_offset);
        if local_start >= seg.text_len || local_end == 0 { return None; }
        let len = seg.text_len;
        Some(local_start.min(len)..local_end.min(len))
    }

    /// Install the selection-changed callback. It receives a [`Selection`] with the text already
    /// stitched, so the caller keeps no parallel copy.
    pub fn set_on_change(&self, f: impl Fn(&Selection) + Send + Sync + 'static) {
        self.inner.lock().on_change = Some(Arc::new(f));
    }

    /// Build the current [`Selection`], stitching the registered runs (safe across run boundaries
    /// and emoji graphemes).
    pub(crate) fn build_selection(&self) -> Option<Selection> {
        let inner = self.inner.lock();
        let total = inner.next_global_offset;
        let (s, e) = (inner.selection_start?, inner.selection_end?);
        let (s, e) = (s.min(e), s.max(e));
        // Zero width (a click with no drag) is no selection, matching `selected_range`.
        if s == e { return None; }
        // Clamp to the container's total length: a bad offset must not surface as empty text with
        // an out-of-range value beside it.
        let (s, e) = (s.min(total), e.min(total));
        if s == e { return None; }
        let mut segs: Vec<&RegisteredSegment> = inner.segments.values().collect();
        segs.sort_by_key(|seg| seg.global_offset);
        let mut text = String::new();
        for seg in segs {
            let seg_s = seg.global_offset;
            let seg_e = seg_s + seg.text_len;
            if seg_e <= s || seg_s >= e { continue; }
            let start = s.max(seg_s) - seg_s;
            let end = e.min(seg_e) - seg_s;
            // The offsets are meant to be character boundaries (`get_closest_*` guarantees it), but
            // a cross-run stitch uses `get` so a bad one skips its run instead of panicking on an
            // out-of-range or non-boundary slice.
            if let Some(part) = seg.text.get(start..end) {
                text.push_str(part);
            }
        }
        // A rich-text inline element (image or placeholder) is a U+FFFC here. Copying one would give
        // the user a replacement character, so it is dropped from the selected text.
        if text.contains('\u{FFFC}') {
            text = text.replace('\u{FFFC}', "");
        }
        Some(Selection { start: s, end: e, text })
    }

    pub(crate) fn fire_on_change(&self) {
        let cb = self.inner.lock().on_change.clone();
        if let Some(cb) = cb {
            if let Some(sel) = self.build_selection() {
                cb(&sel);
            }
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
