//! Observable collections: `StateList<T>` and `StateMap<K, V>` — Compose's
//! `mutableStateListOf` / `mutableStateMapOf`.
//!
//! # What they buy
//!
//! A list held as `State<Vec<T>>` is mutated by rebuilding it:
//!
//! ```ignore
//! let mut next = items.get().clone();   // clone every element
//! next.push(row);
//! items.set(next);                      // compare every element before notifying
//! ```
//!
//! Two costs hide in that, and both grow with the collection: the clone, and `State::set`'s
//! `PartialEq` check, which walks the whole vector before it can decide the value changed. `Vec::eq`
//! short-circuits on the first difference, so a push to a 10 000-element list compares all 10 000
//! elements first (they are equal) and only then notices the length differs.
//!
//! `StateList` holds one shared snapshot and replaces it on mutation:
//!
//! ```ignore
//! rows.push(row);                       // one copy, made by the list itself
//! for row in rows.snapshot().iter() {}  // a cheap Arc clone, no element copies
//! ```
//!
//! The snapshot's `PartialEq` is **pointer identity**, so "did it change" is one pointer compare
//! instead of one per element, and a reader's `Arc` clone is free.
//!
//! # What they do NOT buy
//!
//! **No per-element invalidation.** A reader of the list is a dependent of the whole list, so any
//! mutation re-runs it — the same as with `State<Vec<T>>`. Compose is no different at this layer: a
//! `SnapshotStateList` notifies its readers for any structural change, and `LazyColumn`'s efficiency
//! comes from keyed item reuse rather than from the list telling it which index moved. What changes
//! here is the cost of *detecting* the change and of reading the value, not the number of readers
//! woken.
//!
//! # Equality differs from `State::set`, on purpose
//!
//! `State::set` skips notification when the new value `==` the old one. A list compares snapshots by
//! identity, so `list.push(x); list.pop();` notifies twice even though the content came back to where
//! it started — because two mutations happened and both are real. Replacing a snapshot with the same
//! one (`list.replace_with(list.snapshot())`) does nothing, which is the case identity is there for.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use crate::core::state::State;

// ═══════════════════════════════════════════════════════════
// Snapshots
// ═══════════════════════════════════════════════════════════

/// An immutable view of a `StateList`'s contents.
///
/// Cheap to clone (one `Arc`), and `Deref`s to a slice so `iter`, `len`, indexing and slices come
/// from the standard library. `arc()` hands out the `Arc<Vec<T>>` that [`crate::ui::LazyColumn`] takes.
pub struct ListSnapshot<T>(Arc<Vec<T>>);

impl<T> ListSnapshot<T> {
    /// The underlying `Arc` — what a `LazyColumn::items_from` wants, without copying elements.
    pub fn arc(&self) -> Arc<Vec<T>> {
        self.0.clone()
    }

    /// The elements as a slice.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<T> Clone for ListSnapshot<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> std::ops::Deref for ListSnapshot<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}

impl<T> From<ListSnapshot<T>> for Arc<Vec<T>> {
    /// What lets a `StateList`'s snapshot go straight into `LazyColumn::items_from`: the elements are
    /// shared, not copied.
    fn from(snapshot: ListSnapshot<T>) -> Self {
        snapshot.0
    }
}

impl<T> PartialEq for ListSnapshot<T> {
    /// Identity, not contents: a mutation always produces a new snapshot, so one pointer compare
    /// answers "did the list change" where a content comparison would walk every element. See the
    /// module docs for why this is the point of the type.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Eq for ListSnapshot<T> {}

impl<T: std::fmt::Debug> std::fmt::Debug for ListSnapshot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

/// An immutable view of a `StateMap`'s entries. Same identity-based equality as [`ListSnapshot`].
pub struct MapSnapshot<K, V>(Arc<HashMap<K, V>>);

impl<K, V> MapSnapshot<K, V> {
    /// The underlying `Arc`.
    pub fn arc(&self) -> Arc<HashMap<K, V>> {
        self.0.clone()
    }

    /// The entries as a map reference.
    pub fn as_map(&self) -> &HashMap<K, V> {
        &self.0
    }
}

impl<K, V> Clone for MapSnapshot<K, V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K, V> std::ops::Deref for MapSnapshot<K, V> {
    type Target = HashMap<K, V>;
    fn deref(&self) -> &HashMap<K, V> {
        &self.0
    }
}

impl<K, V> PartialEq for MapSnapshot<K, V> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<K, V> Eq for MapSnapshot<K, V> {}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for MapSnapshot<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.0.iter()).finish()
    }
}

// ═══════════════════════════════════════════════════════════
// StateList
// ═══════════════════════════════════════════════════════════

/// A list that observes its own mutations — Compose's `mutableStateListOf`.
///
/// Clone the handle freely (it is one `Arc` plus a `State` clone): every clone mutates the same list,
/// which is what lets a callback own one while the composition reads another. Reads inside composition
/// register a dependency (`snapshot()`, `len()`, `get()`), writes notify.
pub struct StateList<T> {
    state: State<ListSnapshot<T>>,
}

impl<T> Clone for StateList<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<T: Clone + 'static> Default for StateList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + 'static> StateList<T> {
    /// An empty list.
    pub fn new() -> Self {
        Self {
            state: State::new(ListSnapshot(Arc::new(Vec::new()))),
        }
    }

    /// A list holding `values`.
    pub fn from_vec(values: Vec<T>) -> Self {
        Self {
            state: State::new(ListSnapshot(Arc::new(values))),
        }
    }

    // ── reads (tracked: the calling composition re-runs on any mutation) ──

    /// The current contents. Cheap: one `Arc` clone, no element copies.
    pub fn snapshot(&self) -> ListSnapshot<T> {
        self.state.get()
    }

    /// The current contents without registering a dependency — for a render-time or layout-time read
    /// that will be re-run by other means.
    pub fn peek(&self) -> ListSnapshot<T> {
        self.state.peek()
    }

    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshot().is_empty()
    }

    /// A clone of the element at `index`.
    pub fn get(&self, index: usize) -> Option<T> {
        self.snapshot().get(index).cloned()
    }

    /// Whether any element equals `value`.
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.snapshot().contains(value)
    }

    /// Every element, cloned — for a caller that needs an owned list. Prefer `snapshot()` when a
    /// reference or an `Arc` will do.
    pub fn to_vec(&self) -> Vec<T> {
        self.snapshot().as_slice().to_vec()
    }

    // ── writes (each one publishes a new snapshot and notifies readers) ──

    /// Append.
    pub fn push(&self, value: T) {
        self.mutate(|items| items.push(value));
    }

    /// Append every value.
    pub fn extend<I: IntoIterator<Item = T>>(&self, values: I) {
        self.mutate(|items| items.extend(values));
    }

    /// Insert at `index`, shifting later elements right. An index past the end appends (the list is
    /// not a fallible container; `get` and `set` are where bounds are reported).
    pub fn insert(&self, index: usize, value: T) {
        self.mutate(|items| {
            let at = index.min(items.len());
            items.insert(at, value);
        });
    }

    /// Remove and return the element at `index`.
    pub fn remove(&self, index: usize) -> Option<T> {
        let state = self.state.clone();
        let mut next = state.peek();
        let arc = Arc::make_mut(&mut next.0);
        if index >= arc.len() {
            return None;
        }
        let removed = arc.remove(index);
        state.set(next);
        Some(removed)
    }

    /// Remove and return the last element.
    pub fn pop(&self) -> Option<T> {
        let state = self.state.clone();
        let mut next = state.peek();
        let arc = Arc::make_mut(&mut next.0);
        let removed = arc.pop();
        if removed.is_some() {
            state.set(next);
        }
        removed
    }

    /// Replace the element at `index`, returning the previous one.
    pub fn set(&self, index: usize, value: T) -> Option<T> {
        let state = self.state.clone();
        let mut next = state.peek();
        let arc = Arc::make_mut(&mut next.0);
        let previous = std::mem::replace(arc.get_mut(index)?, value);
        state.set(next);
        Some(previous)
    }

    /// Remove everything.
    pub fn clear(&self) {
        self.mutate(|items| items.clear());
    }

    /// Keep only the elements `keep` accepts.
    pub fn retain(&self, keep: impl FnMut(&T) -> bool) {
        self.mutate(|items| items.retain(keep));
    }

    /// Sort in place.
    pub fn sort_by(&self, compare: impl FnMut(&T, &T) -> std::cmp::Ordering) {
        self.mutate(|items| items.sort_by(compare));
    }

    /// Exchange two elements. Out-of-range indices are ignored.
    pub fn swap(&self, a: usize, b: usize) {
        self.mutate(|items| {
            if a < items.len() && b < items.len() {
                items.swap(a, b);
            }
        });
    }

    /// Keep the first `len` elements.
    pub fn truncate(&self, len: usize) {
        self.mutate(|items| items.truncate(len));
    }

    /// In-place edit of one element (its clone is compared back: an unchanged element still publishes
    /// a new snapshot, because the caller asked for a mutation).
    pub fn update(&self, index: usize, edit: impl FnOnce(&mut T)) {
        self.mutate(|items| {
            if let Some(item) = items.get_mut(index) {
                edit(item);
            }
        });
    }

    /// Publish `snapshot` as the new contents. The one write that compares by identity: passing the
    /// current snapshot back is a no-op.
    pub fn replace(&self, snapshot: ListSnapshot<T>) {
        self.state.set(snapshot);
    }

    /// The `State` behind the list — for a caller that needs to pass the observable itself (an effect,
    /// an animation dependency) rather than the list API.
    pub fn state(&self) -> &State<ListSnapshot<T>> {
        &self.state
    }

    /// Copy-on-write: clone the current snapshot's contents into a unique one, edit it, publish it.
    fn mutate(&self, edit: impl FnOnce(&mut Vec<T>)) {
        let state = self.state.clone();
        let mut next = state.peek();
        edit(Arc::make_mut(&mut next.0));
        state.set(next);
    }
}

// ═══════════════════════════════════════════════════════════
// StateMap
// ═══════════════════════════════════════════════════════════

/// A map that observes its own mutations — Compose's `mutableStateMapOf`. Same contract as
/// [`StateList`]: identity-based change detection, cheap snapshots, no per-entry invalidation.
pub struct StateMap<K, V> {
    state: State<MapSnapshot<K, V>>,
}

impl<K, V> Clone for StateMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<K: Clone + Eq + Hash + 'static, V: Clone + 'static> Default for StateMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Clone + Eq + Hash + 'static, V: Clone + 'static> StateMap<K, V> {
    /// An empty map.
    pub fn new() -> Self {
        Self {
            state: State::new(MapSnapshot(Arc::new(HashMap::new()))),
        }
    }

    /// A map holding `entries`.
    pub fn from_pairs(entries: impl IntoIterator<Item = (K, V)>) -> Self {
        Self {
            state: State::new(MapSnapshot(Arc::new(entries.into_iter().collect()))),
        }
    }

    // ── reads ──

    /// The current entries. Cheap: one `Arc` clone.
    pub fn snapshot(&self) -> MapSnapshot<K, V> {
        self.state.get()
    }

    /// The entries without registering a dependency.
    pub fn peek(&self) -> MapSnapshot<K, V> {
        self.state.peek()
    }

    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshot().is_empty()
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.snapshot().get(key).cloned()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.snapshot().contains_key(key)
    }

    /// Every key, cloned.
    pub fn keys(&self) -> Vec<K> {
        self.snapshot().keys().cloned().collect()
    }

    /// Every value, cloned.
    pub fn values(&self) -> Vec<V> {
        self.snapshot().values().cloned().collect()
    }

    /// Every entry, cloned.
    pub fn to_vec(&self) -> Vec<(K, V)> {
        self.snapshot().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    // ── writes ──

    /// Insert or replace, returning the previous value for `key`.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let state = self.state.clone();
        let mut next = state.peek();
        let previous = Arc::make_mut(&mut next.0).insert(key, value);
        state.set(next);
        previous
    }

    /// Remove `key`, returning its value.
    pub fn remove(&self, key: &K) -> Option<V> {
        let state = self.state.clone();
        let mut next = state.peek();
        let removed = Arc::make_mut(&mut next.0).remove(key);
        if removed.is_some() {
            state.set(next);
        }
        removed
    }

    /// Remove everything.
    pub fn clear(&self) {
        let state = self.state.clone();
        let mut next = state.peek();
        if next.0.is_empty() {
            return;
        }
        Arc::make_mut(&mut next.0).clear();
        state.set(next);
    }

    /// Insert unless the key is present, returning whether it inserted.
    pub fn insert_if_absent(&self, key: K, value: V) -> bool {
        let state = self.state.clone();
        let mut next = state.peek();
        let map = Arc::make_mut(&mut next.0);
        if map.contains_key(&key) {
            false
        } else {
            map.insert(key, value);
            state.set(next);
            true
        }
    }

    /// In-place edit of one value. Does nothing when the key is absent.
    pub fn update(&self, key: &K, edit: impl FnOnce(&mut V)) {
        let state = self.state.clone();
        let mut next = state.peek();
        if let Some(value) = Arc::make_mut(&mut next.0).get_mut(key) {
            edit(value);
            state.set(next);
        }
    }

    /// The `State` behind the map.
    pub fn state(&self) -> &State<MapSnapshot<K, V>> {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::{ComposeCtx, Composer};
    use crate::layout::Constraints;

    /// Compose a scene, run the layout pass, and count how many times the reader ran — plus what it
    /// read on the last run.
    ///
    /// The layout pass matters: a group can only Skip when the previous frame left it a cached subtree
    /// to restore, so a test that composes twice without laying out never sees a Skip and cannot tell
    /// "the reader re-ran" from "the reader always re-runs".
    fn run(
        composer: &mut Composer,
        list: &StateList<i32>,
        runs: &std::rc::Rc<std::cell::Cell<usize>>,
        seen: &std::rc::Rc<std::cell::RefCell<Vec<i32>>>,
    ) {
        use crate::core::composer::GroupStatus;
        use crate::layout::BoxLayout;
        use crate::modifier::Modifier;

        let list = list.clone();
        let runs = runs.clone();
        let seen = seen.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    runs.set(runs.get() + 1);
                    let snapshot = list.snapshot();
                    seen.borrow_mut().push(snapshot.iter().copied().sum());
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
    }

    fn harness() -> (
        Composer,
        StateList<i32>,
        std::rc::Rc<std::cell::Cell<usize>>,
        std::rc::Rc<std::cell::RefCell<Vec<i32>>>,
    ) {
        let list = StateList::new();
        let mut composer = Composer::new();
        let runs = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        run(&mut composer, &list, &runs, &seen);
        (composer, list, runs, seen)
    }

    #[test]
    fn a_mutation_notifies_its_reader_and_an_idle_frame_skips() {
        let (mut composer, list, runs, seen) = harness();
        assert_eq!(
            (runs.get(), seen.borrow().as_slice()),
            (1, &[0][..]),
            "first frame reads an empty list"
        );

        // CONTROL: nothing changed, so the reader must Skip — otherwise "it re-ran" below means nothing.
        run(&mut composer, &list, &runs, &seen);
        assert_eq!(runs.get(), 1, "CONTROL: an unchanged frame must Skip the reader");

        list.push(5);
        run(&mut composer, &list, &runs, &seen);
        assert_eq!(runs.get(), 2, "a push must re-run the reader");
        assert_eq!(seen.borrow().last(), Some(&5), "and the reader must see the new element");
    }

    #[test]
    fn every_mutation_is_visible_to_the_next_read() {
        let list = StateList::from_vec(vec![1, 2, 3]);
        assert_eq!(list.len(), 3);

        list.push(4);
        list.insert(0, 0);
        assert_eq!(list.to_vec(), vec![0, 1, 2, 3, 4]);

        assert_eq!(list.remove(0), Some(0));
        assert_eq!(list.pop(), Some(4));
        assert_eq!(list.set(1, 99), Some(2));
        assert_eq!(list.to_vec(), vec![1, 99, 3]);

        list.swap(0, 2);
        assert_eq!(list.to_vec(), vec![3, 99, 1]);

        list.retain(|value| *value != 99);
        assert_eq!(list.to_vec(), vec![3, 1]);

        list.sort_by(|a, b| a.cmp(b));
        assert_eq!(list.to_vec(), vec![1, 3]);

        list.truncate(1);
        assert_eq!(list.to_vec(), vec![1]);

        list.extend([7, 8]);
        assert_eq!(list.to_vec(), vec![1, 7, 8]);

        list.update(0, |value| *value += 10);
        assert_eq!(list.get(0), Some(11));

        list.clear();
        assert!(list.is_empty());
    }

    #[test]
    fn out_of_range_writes_do_not_panic() {
        let list = StateList::from_vec(vec![1, 2]);
        assert_eq!(list.remove(9), None);
        assert_eq!(list.set(9, 5), None);
        list.swap(0, 9); // ignored
        assert_eq!(list.to_vec(), vec![1, 2], "nothing changed");
        // Inserting past the end appends rather than failing: `get`/`set` are where bounds are reported.
        list.insert(99, 3);
        assert_eq!(list.to_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn an_equal_content_mutation_still_notifies() {
        // The deliberate difference from `State::set`: two mutations happened, so the reader runs twice
        // even though the content came back to where it started.
        let (mut composer, list, runs, seen) = harness();
        list.push(1);
        run(&mut composer, &list, &runs, &seen);
        list.remove(0);
        run(&mut composer, &list, &runs, &seen);
        assert_eq!(runs.get(), 3, "both mutations notified");
        assert_eq!(seen.borrow().last(), Some(&0), "and the list is empty again");
    }

    #[test]
    fn replacing_with_the_current_snapshot_is_a_no_op() {
        // The one write that compares by identity — which is what makes the comparison O(1).
        let (mut composer, list, runs, seen) = harness();
        list.replace(list.snapshot());
        run(&mut composer, &list, &runs, &seen);
        assert_eq!(runs.get(), 1, "the same snapshot must not notify");
    }

    #[test]
    fn a_failed_removal_publishes_nothing() {
        let (mut composer, list, runs, seen) = harness();
        list.remove(0);
        list.pop();
        run(&mut composer, &list, &runs, &seen);
        assert_eq!(runs.get(), 1, "a removal that found nothing is not a change");
    }

    #[test]
    fn a_snapshot_is_shared_not_copied() {
        // The cost this type exists to remove: reading the list clones one `Arc`, not every element.
        let list = StateList::from_vec(vec![1, 2, 3]);
        let a = list.snapshot();
        let b = list.snapshot();
        assert!(Arc::ptr_eq(&a.arc(), &b.arc()), "two reads share the same allocation");
        assert_eq!(a.as_slice(), &[1, 2, 3]);

        list.push(4);
        let c = list.snapshot();
        assert!(!Arc::ptr_eq(&a.arc(), &c.arc()), "a mutation publishes a new snapshot");
        assert_eq!(a.as_slice(), &[1, 2, 3], "the old snapshot is untouched");
    }

    #[test]
    fn the_snapshot_feeds_a_lazy_list_without_copying() {
        // The integration this type was built for: `items_from` takes the snapshot's own allocation.
        let list = StateList::from_vec(vec![1, 2, 3]);
        let arc: Arc<Vec<i32>> = list.snapshot().into();
        assert!(Arc::ptr_eq(&arc, &list.snapshot().arc()));
    }

    #[test]
    fn a_map_mutation_reports_like_a_map() {
        let map: StateMap<String, i32> = StateMap::new();
        assert!(map.is_empty());

        assert_eq!(map.insert("a".to_string(), 1), None, "a fresh insert has no previous value");
        assert_eq!(map.insert("a".to_string(), 2), Some(1), "and a replace reports the old one");
        assert_eq!(map.get(&"a".to_string()), Some(2));

        // `insert_if_absent` reports whether IT inserted; a present key leaves the value alone.
        assert!(!map.insert_if_absent("a".to_string(), 9), "the key is present");
        assert_eq!(map.get(&"a".to_string()), Some(2), "and the value is untouched");
        assert!(map.insert_if_absent("b".to_string(), 3), "a new key inserts");

        map.update(&"a".to_string(), |value| *value += 1);
        assert_eq!(map.get(&"a".to_string()), Some(3));
        map.update(&"missing".to_string(), |_| unreachable!("an absent key is not edited"));

        assert_eq!(map.len(), 2);
        assert_eq!(map.remove(&"a".to_string()), Some(3));
        assert_eq!(map.remove(&"a".to_string()), None);
        assert!(!map.is_empty());
    }

    #[test]
    fn a_map_mutation_reaches_a_reader() {
        use crate::core::composer::GroupStatus;
        use crate::layout::BoxLayout;
        use crate::modifier::Modifier;

        let map: StateMap<i32, i32> = StateMap::new();
        let map_in = map.clone();
        let runs = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let runs_in = runs.clone();
        let mut composer = Composer::new();
        let scene = move |ctx: &mut ComposeCtx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    runs_in.set(runs_in.get() + 1);
                    let _ = map_in.snapshot().len();
                }
            }
            ctx.end_restartable_group();
        };
        // A closure is not Copy: the two frames below each need their own way in.
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| scene(ctx));
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        frame(&mut composer);
        assert_eq!(runs.get(), 1);

        frame(&mut composer);
        assert_eq!(runs.get(), 1, "CONTROL: an unchanged frame Skips");

        map.insert(1, 1);
        frame(&mut composer);
        assert_eq!(runs.get(), 2, "an insert re-runs the reader");
    }
}
