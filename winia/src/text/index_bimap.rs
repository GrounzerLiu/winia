//! 双向索引映射 — 实现 UTF-8 字节 ↔ UTF-16 索引的双向查找
//!
//! Skia Paragraph 内部使用 UTF-16 索引，而 Rust String 是 UTF-8 编码。
//! 对于中文、Emoji 等多字节字符，两种索引体系的值不同，
//! 此映射表负责在两者之间进行转换。

/// 双向索引映射表
///
/// 内部维护两个有序的 `Vec<usize>`，分别存储 left→right 和 right→left 的映射。
/// 通过二分查找实现 O(log n) 的查询效率。
#[derive(Debug, Clone)]
pub struct IndexBiMap {
    left: Vec<usize>,
    right: Vec<usize>,
}

impl IndexBiMap {
    pub fn new() -> Self {
        IndexBiMap {
            left: Vec::new(),
            right: Vec::new(),
        }
    }

    /// Insert one pair. Both keys must be strictly increasing.
    ///
    /// The lookups are binary searches, so an out-of-order or duplicated pair silently corrupts
    /// every later lookup instead of failing. The check is a `debug_assert!`: the builder inserts
    /// once per character on the measure path, so a release-mode scan would be a real cost for a
    /// bug that can only come from the builder itself.
    pub fn insert(&mut self, left: usize, right: usize) {
        debug_assert!(
            self.left.last().is_none_or(|last| *last < left),
            "IndexBiMap left keys must strictly increase: {left} after {:?}",
            self.left.last()
        );
        debug_assert!(
            self.right.last().is_none_or(|last| *last < right),
            "IndexBiMap right keys must strictly increase: {right} after {:?}",
            self.right.last()
        );
        self.left.push(left);
        self.right.push(right);
    }

    /// 通过 left 查找对应的 right
    pub fn get_by_left(&self, left: &usize) -> Option<&usize> {
        if let Ok(idx) = self.left.binary_search(left) {
            self.right.get(idx)
        } else {
            None
        }
    }

    /// 通过 right 查找对应的 left
    pub fn get_by_right(&self, right: &usize) -> Option<&usize> {
        if let Ok(idx) = self.right.binary_search(right) {
            self.left.get(idx)
        } else {
            None
        }
    }

    pub fn contains_left(&self, left: &usize) -> bool {
        self.left.binary_search(left).is_ok()
    }

    pub fn contains_right(&self, right: &usize) -> bool {
        self.right.binary_search(right).is_ok()
    }

    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
    }

    /// 获取 left 方向所有键的引用
    pub fn left_keys(&self) -> &[usize] {
        &self.left
    }

    /// 获取 right 方向所有键的引用
    pub fn right_keys(&self) -> &[usize] {
        &self.right
    }
}

impl Default for IndexBiMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::IndexBiMap;

    #[test]
    fn an_increasing_insert_round_trips() {
        let mut map = IndexBiMap::new();
        map.insert(0, 0);
        map.insert(1, 1);
        map.insert(3, 2);
        assert_eq!(map.get_by_left(&3), Some(&2));
        assert_eq!(map.get_by_right(&2), Some(&3));
        assert!(map.get_by_left(&2).is_none());
    }

    /// A repeated key breaks the binary search, so the builder must not be able to insert one
    /// silently. `debug_assert!` makes the bug loud in tests and free in release.
    #[test]
    #[should_panic(expected = "must strictly increase")]
    fn a_repeated_left_key_is_rejected_in_debug_builds() {
        let mut map = IndexBiMap::new();
        map.insert(4, 0);
        map.insert(4, 1);
    }

    #[test]
    #[should_panic(expected = "must strictly increase")]
    fn a_backwards_right_key_is_rejected_in_debug_builds() {
        let mut map = IndexBiMap::new();
        map.insert(0, 5);
        map.insert(1, 4);
    }
}