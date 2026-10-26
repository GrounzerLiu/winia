use super::*;

// ═══════════════════════════════════════════════════════
// IntervalList — interval content (Compose's `LazyListIntervalContent`)
// ═══════════════════════════════════════════════════════

/// One interval: `count` consecutive items sharing a key factory and a content closure.
pub(crate) struct Interval {
    pub count: usize,
    pub key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>,
    pub content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>,
    /// A sticky-header interval (Compose's `stickyHeader` — pinned to the viewport top while scrolling).
    pub is_sticky_header: bool,
}

impl Interval {
    /// The key for a global index inside this interval (the index itself when there is no factory —
    /// the position IS the key).
    pub fn key_of(&self, global_index: usize) -> u64 {
        match &self.key {
            Some(f) => f(global_index),
            None => global_index as u64,
        }
    }
}

/// The interval list: a global index resolves to `(interval, local_index)`.
pub(crate) struct IntervalList {
    pub intervals: Vec<Interval>,
    /// The global index each interval starts at (a cache built lazily).
    pub(crate) starts: Vec<usize>,
    pub(crate) total: usize,
    /// key → global index map, built during `rebuild` (a section without a key factory uses its
    /// index as the key).
    pub(crate) key_index: std::sync::Arc<std::collections::HashMap<u64, usize>>,
    /// Whether any sticky-header interval exists (built during `rebuild`; `build` skips the
    /// walk-back when there is none).
    pub(crate) has_sticky: bool,
}

impl IntervalList {
    /// The empty list: no intervals, no keys, no sticky headers.
    pub fn new() -> Self {
        Self {
            intervals: Vec::new(), starts: Vec::new(), total: 0,
            key_index: std::sync::Arc::new(std::collections::HashMap::new()),
            has_sticky: false,
        }
    }

    /// Append an interval — `count` items, an optional key factory, the content closure, and whether it
    /// is a sticky header. Call [`IntervalList::rebuild`] before the list is measured.
    pub fn add(&mut self, count: usize, key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>, content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>, sticky: bool) {
        self.intervals.push(Interval { count, key, content, is_sticky_header: sticky });
    }

    /// Whether a global index is a sticky header (an interval flag — O(intervals)).
    pub fn is_sticky(&self, global: usize) -> bool {
        self.locate(global)
            .map(|(iv, _)| self.intervals[iv].is_sticky_header)
            .unwrap_or(false)
    }

    /// Whether the list has any sticky header (cached by `rebuild`).
    pub fn has_sticky_headers(&self) -> bool { self.has_sticky }

    /// Rebuild the per-interval start indices and the total, and build the key → index map.
    pub fn rebuild(&mut self) {
        self.starts.clear();
        self.total = 0;
        self.has_sticky = false;
        for iv in &self.intervals {
            self.starts.push(self.total);
            self.total = self.total.saturating_add(iv.count);
            self.has_sticky |= iv.is_sticky_header;
        }
        // The key map. Keys must be unique across the whole list (Compose throws on a duplicate: a
        // conflict corrupts `LazyLayout`'s saved state, which is why the API requires uniqueness).
        // A fresh map behind one Arc: the IndexForKey action holds the previous frame's map until
        // this one replaces it, and nobody pays for a copy.
        let mut key_index = std::collections::HashMap::with_capacity(self.total);
        for g in 0..self.total {
            let k = self.key_of_unchecked(g);
            if let Some(prev) = key_index.insert(k, g) {
                panic!(
                    "LazyColumn/LazyRow: duplicate key {k} — global index {g} collides with {prev}.                      Keys must be unique across the whole list (Compose throws IllegalStateException)"
                );
            }
        }
        self.key_index = std::sync::Arc::new(key_index);
    }

    pub fn total(&self) -> usize { self.total }

    /// Global index → `(interval index, local index inside it)`.
    pub fn locate(&self, global: usize) -> Option<(usize, usize)> {
        if global >= self.total { return None; }
        // Linear search: there are usually few intervals (a binary search would do).
        for i in 0..self.intervals.len() {
            let start = self.starts[i];
            let count = self.intervals[i].count;
            if global >= start && global < start + count {
                return Some((i, global - start));
            }
        }
        None
    }

    /// Global index → key (a section without a factory uses the index itself). An out-of-range index
    /// answers with itself — callers have to check `total` first.
    pub(crate) fn key_of_unchecked(&self, global: usize) -> u64 {
        if global >= self.total { return global as u64; }
        for i in 0..self.intervals.len() {
            let start = self.starts[i];
            let count = self.intervals[i].count;
            if global >= start && global < start + count {
                return self.intervals[i].key_of(global);
            }
        }
        global as u64
    }

    /// Global index → key.
    pub fn key_of(&self, global: usize) -> Option<u64> {
        if global >= self.total { return None; }
        Some(self.key_of_unchecked(global))
    }

    /// Key → global index (the HashMap built by `rebuild` — O(1), which is what Compose's
    /// `NearestRangeKeyIndexMap` is for: no linear scan on a large data set).
    /// The key -> index map itself, cheap to clone: what the IndexForKey action captures.
    pub(crate) fn key_index_arc(&self) -> std::sync::Arc<std::collections::HashMap<u64, usize>> {
        self.key_index.clone()
    }

    pub fn index_of_key(&self, key: u64) -> Option<usize> {
        self.key_index.get(&key).copied()
    }
}

// ═══════════════════════════════════════════════════════
// LazyColumn — the public component
