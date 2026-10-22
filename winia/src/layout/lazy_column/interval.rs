use super::*;

// ═══════════════════════════════════════════════════════
// IntervalList — 区间内容（对齐 Compose LazyListIntervalContent）
// ═══════════════════════════════════════════════════════

/// 一个区间：count 个连续项共享 key 工厂与内容闭包
pub(crate) struct Interval {
    pub count: usize,
    pub key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>,
    pub content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>,
    /// sticky header 区间（对标 Compose `stickyHeader`——滚动时钉在视口顶）
    pub is_sticky_header: bool,
}

impl Interval {
    /// 计算区间内全局 index 对应的 key（无工厂时用索引自身——位置即 key）
    pub fn key_of(&self, global_index: usize) -> u64 {
        match &self.key {
            Some(f) => f(global_index),
            None => global_index as u64,
        }
    }
}

/// 区间列表：全局 index 定位到 (interval, local_index)
pub(crate) struct IntervalList {
    pub intervals: Vec<Interval>,
    /// 每段起始全局 index（惰性构建缓存）
    pub(crate) starts: Vec<usize>,
    pub(crate) total: usize,
    /// key → 全局 index 映射（rebuild 时构建；无 key 工厂的段用索引自身）
    pub(crate) key_index: std::collections::HashMap<u64, usize>,
    /// 是否存在 sticky header 区间（rebuild 时构建——build 回溯据此跳过）
    pub(crate) has_sticky: bool,
}

impl IntervalList {
    pub fn new() -> Self {
        Self {
            intervals: Vec::new(), starts: Vec::new(), total: 0,
            key_index: std::collections::HashMap::new(),
            has_sticky: false,
        }
    }

    pub fn add(&mut self, count: usize, key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>, content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>, sticky: bool) {
        self.intervals.push(Interval { count, key, content, is_sticky_header: sticky });
    }

    /// 全局 index 是否为 sticky header（区间标志——O(区间数)）
    pub fn is_sticky(&self, global: usize) -> bool {
        self.locate(global)
            .map(|(iv, _)| self.intervals[iv].is_sticky_header)
            .unwrap_or(false)
    }

    /// 列表是否含 sticky header（rebuild 缓存）
    pub fn has_sticky_headers(&self) -> bool { self.has_sticky }

    /// 重建每段起始索引与总数，并构建 key → index 映射
    pub fn rebuild(&mut self) {
        self.starts.clear();
        self.total = 0;
        self.has_sticky = false;
        for iv in &self.intervals {
            self.starts.push(self.total);
            self.total += iv.count;
            self.has_sticky |= iv.is_sticky_header;
        }
        // key 映射（key 必须全列表唯一——对齐 Compose：重复 key 抛异常，
        // LazyLayout 中 key 冲突会导致状态错乱，官方约束 key 唯一）
        self.key_index.clear();
        for g in 0..self.total {
            let k = self.key_of_unchecked(g);
            if let Some(prev) = self.key_index.insert(k, g) {
                panic!(
                    "LazyColumn/LazyRow: duplicate key {k} — global index {g} collides with {prev}.                      Keys must be unique across the whole list (Compose throws IllegalStateException)"
                );
            }
        }
    }

    pub fn total(&self) -> usize { self.total }

    /// 全局 index → (interval 索引, 段内局部 index)
    pub fn locate(&self, global: usize) -> Option<(usize, usize)> {
        if global >= self.total { return None; }
        // 线性查找（区间数通常少；可改二分）
        for i in 0..self.intervals.len() {
            let start = self.starts[i];
            let count = self.intervals[i].count;
            if global >= start && global < start + count {
                return Some((i, global - start));
            }
        }
        None
    }

    /// 全局 index → key（无工厂段 = 索引自身；越界返回自身——调用方需先查 total）
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

    /// 全局 index → key
    pub fn key_of(&self, global: usize) -> Option<u64> {
        if global >= self.total { return None; }
        Some(self.key_of_unchecked(global))
    }

    /// key → 全局 index（rebuild 构建的 HashMap 缓存——O(1)，对齐 Compose
    /// NearestRangeKeyIndexMap 的目的；大数据量下无线性扫描）
    pub fn index_of_key(&self, key: u64) -> Option<usize> {
        self.key_index.get(&key).copied()
    }
}

// ═══════════════════════════════════════════════════════
// LazyColumn — 公开组件
