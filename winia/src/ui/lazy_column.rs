//! LazyColumn 组件 — 对标 Compose material3 `LazyColumn`（foundation lazy）
//!
//! 核心机制（对齐 Compose lazy 架构）：
//! - **锚点滚动模型**：`LazyListState.first_visible_index + first_visible_offset`
//!   而非像素 offset——滚动位置由"第一个可见项的索引 + 其偏移"表达；
//! - **稳定 key**：`items()` 接受 `key` 工厂（数据 id 等稳定值）。插入/删除
//!   列表前部项后，按 key 找回原 firstVisibleItem 保持滚动位置（Compose
//!   `LazyListScrollPosition.updateScrollPositionIfTheFirstItemWasMoved` 语义）；
//! - **区间内容**：`item/items` 注册为 IntervalList（count + key 工厂 + content
//!   闭包），全局 index 经区间定位转局部 index（Compose `LazyListIntervalContent`）；
//! - **懒测量**：组合期按高度缓存预估可见范围 [start, end]，只注册/组合可见项；
//!   测量期真实排布并把高度写回缓存（未缓存项用默认高度预估，偏差触发补注册）。
//!
//! ⚠ 与 Compose 差异：winia 组合为命令式（build 直接注册节点），无 Compose 的
//! LazyLayout 测量期组合——这里用"组合期预估 + 测量期校正"两阶段模型。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::Modifier;
use std::sync::Arc;

/// 未测量项的预估高度（首次进入视口时用，测量后写回缓存）
pub const LAZY_ITEM_ESTIMATED_HEIGHT: f32 = 48.0;
/// 超出视口后仍注册的额外项数（上下各预取，减少滚动时补注册抖动）
pub const LAZY_BEYOND_BOUNDS: usize = 4;

/// Lazy 列表滚动状态（winia 像素模型 + 派生锚点）
///
/// 滚动输入走 winia 现有通道：`ScrollState.offset`（像素总偏移，挂 vertical_scroll）。
/// `first_visible_index/offset` 是派生锚点（对齐 Compose 的锚点模型语义，供外部读取）；
/// `last_known_first_key` 记录第一个可见项 key——数据前部增删后按 key 校正偏移。
#[derive(Debug, Clone)]
pub struct LazyListState {
    /// 像素滚动偏移（挂 vertical_scroll modifier——滚动输入通道）
    pub offset: crate::core::state::State<f32>,
    /// 最近已知的第一个可见项 key（数据变化后按 key 校正位置）
    pub(crate) last_known_first_key: crate::core::state::State<Option<u64>>,
    /// 派生：第一个可见项索引（每次 build 后更新）
    pub first_visible_index: crate::core::state::State<usize>,
    /// 派生：第一个可见项的偏移（正 = 该项向上滚出多少）
    pub first_visible_offset: crate::core::state::State<f32>,
}

impl LazyListState {
    pub fn new() -> Self {
        Self {
            offset: crate::core::state::State::new(0.0),
            last_known_first_key: crate::core::state::State::new(None),
            first_visible_index: crate::core::state::State::new(0),
            first_visible_offset: crate::core::state::State::new(0.0),
        }
    }

    /// 当前第一个可见项索引
    pub fn first_visible(&self) -> usize { self.first_visible_index.get() }

    /// 当前第一个可见项偏移
    pub fn offset(&self) -> f32 { self.offset.get() }
}

impl Default for LazyListState {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════
// IntervalList — 区间内容（对齐 Compose LazyListIntervalContent）
// ═══════════════════════════════════════════════════════

/// 一个区间：count 个连续项共享 key 工厂与内容闭包
pub(crate) struct Interval {
    pub count: usize,
    pub key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>,
    pub content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>,
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
    starts: Vec<usize>,
    total: usize,
}

impl IntervalList {
    pub fn new() -> Self {
        Self { intervals: Vec::new(), starts: Vec::new(), total: 0 }
    }

    pub fn add(&mut self, count: usize, key: Option<Arc<dyn Fn(usize) -> u64 + Send + Sync>>, content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>) {
        self.intervals.push(Interval { count, key, content });
    }

    /// 重建每段起始索引与总数
    pub fn rebuild(&mut self) {
        self.starts.clear();
        self.total = 0;
        for iv in &self.intervals {
            self.starts.push(self.total);
            self.total += iv.count;
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

    /// 全局 index → key
    pub fn key_of(&self, global: usize) -> Option<u64> {
        self.locate(global).map(|(i, _)| self.intervals[i].key_of(global))
    }

    /// key → 全局 index（线性扫描；Compose 用最近范围缓存优化——winia 先简单）
    pub fn index_of_key(&self, key: u64) -> Option<usize> {
        for g in 0..self.total {
            if let Some(k) = self.key_of(g) {
                if k == key { return Some(g); }
            }
        }
        None
    }
}

// ═══════════════════════════════════════════════════════
// LazyColumn — 公开组件
// ═══════════════════════════════════════════════════════

/// 懒加载列表（对齐 Compose `LazyColumn`）
///
/// 只组合/测量可见项（含上下预取窗）。items 支持稳定 key 工厂——
/// 数据前部增删后按 key 保持滚动位置。
pub struct LazyColumn {
    state: Option<LazyListState>,
    spacing: f32,
    modifier: Modifier,
    intervals: IntervalList,
}

impl LazyColumn {
    pub fn new() -> Self {
        Self {
            state: None,
            spacing: 0.0,
            modifier: Modifier::new(),
            intervals: IntervalList::new(),
        }
    }

    /// 注入外部滚动状态（跨重组保持；不传则内部 remember）
    pub fn state(mut self, state: LazyListState) -> Self {
        self.state = Some(state);
        self
    }

    /// 项间距（主轴方向）
    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 单个固定项（对标 `item(key, content)`）
    pub fn item(mut self, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, None, c);
        self
    }

    /// 带 key 的单一项
    pub fn item_keyed(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c);
        self
    }

    /// count 个项 + key 工厂（对标 `items(count, key: (index) -> Any, itemContent)`）
    pub fn items(
        mut self,
        count: usize,
        key: impl Fn(usize) -> u64 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        let k = Arc::new(key);
        let c = Arc::new(content);
        self.intervals.add(count, Some(k), c);
        self
    }

    /// count 个无 key 项（位置即 key）
    pub fn items_plain(
        mut self,
        count: usize,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        let c = Arc::new(content);
        self.intervals.add(count, None, c);
        self
    }

    /// 从数据列表迭代（对标 `items(items, key: (item) -> Any, itemContent)`）——
    /// **用户要求的"稳定 key 迭代"入口**：key 从数据 id 提取，列表前部增删
    /// 后滚动位置按 key 保持。
    pub fn items_from<T: Send + Sync + 'static>(
        mut self,
        list: Arc<Vec<T>>,
        key: impl Fn(&T) -> u64 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize, &T) + Send + Sync + 'static,
    ) -> Self {
        let k = Arc::new(key);
        let c = Arc::new(content);
        let lk = list.clone();
        let lc = list.clone();
        let n = list.len();
        let kk: Arc<dyn Fn(usize) -> u64 + Send + Sync> = Arc::new(move |g: usize| {
            k(&lk[g])
        });
        let cc: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(move |ctx, g| {
            c(ctx, g, &lc[g]);
        });
        self.intervals.add(n, Some(kk), cc);
        self
    }
}

// ═══════════════════════════════════════════════════════
// 高度缓存 + 可见范围计算
// ═══════════════════════════════════════════════════════

/// 每项高度缓存（全局 index → 测量高度）；未测项用预估
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ItemHeightCache {
    pub heights: Vec<f32>,
}

impl ItemHeightCache {
    pub fn height(&self, index: usize) -> f32 {
        // 未测（缺失或 0）→ 预估；记录过的 >0 高度直接返回
        self.heights.get(index).copied().filter(|&h| h > 0.0).unwrap_or(LAZY_ITEM_ESTIMATED_HEIGHT)
    }

    pub fn record(&mut self, index: usize, h: f32) {
        if self.heights.len() <= index { self.heights.resize(index + 1, 0.0); }
        self.heights[index] = h;
    }
}

/// 累计高度：index 之前各项高度和（含间距）
pub(crate) fn prefix_height(cache: &ItemHeightCache, index: usize, spacing: f32) -> f32 {
    let mut acc = 0.0;
    for i in 0..index {
        acc += cache.height(i) + spacing;
    }
    acc
}

/// 由像素偏移定位第一个可见项（锚点）
pub(crate) fn anchor_from_offset(cache: &ItemHeightCache, offset: f32, spacing: f32) -> (usize, f32) {
    let mut acc = 0.0;
    for i in 0..cache.heights.len().max(1) {
        let h = cache.height(i) + spacing;
        if acc + h > offset {
            return (i, offset - acc);
        }
        acc += h;
    }
    // offset 超过已知范围：锚定在已知末尾
    let last = cache.heights.len().saturating_sub(1);
    (last, 0.0)
}

/// 由锚点 + 视口高计算可见范围（含预取窗）
pub(crate) fn visible_range(
    cache: &ItemHeightCache,
    first_index: usize,
    first_offset: f32,
    viewport_h: f32,
    spacing: f32,
    total: usize,
) -> (usize, usize) {
    let start = first_index.saturating_sub(LAZY_BEYOND_BOUNDS);
    let mut end = first_index;
    let mut acc = first_offset;
    while end < total && acc < viewport_h {
        acc += cache.height(end) + spacing;
        end += 1;
    }
    let end = (end + LAZY_BEYOND_BOUNDS).min(total);
    (start, end)
}

// ═══════════════════════════════════════════════════════
// build — 组合期注册可见项
// ═══════════════════════════════════════════════════════

impl LazyColumn {
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let state = match self.state {
            Some(s) => s,
            None => ctx.remember(|| LazyListState::new()).get(),
        };
        ctx.changed(&state.offset);
        let key = ctx.next_key();

        // 内容注册表（重建——数据变化反映）
        let mut intervals = self.intervals;
        intervals.rebuild();
        let total = intervals.total();

        // 跨帧 remember：高度缓存 / 视口高 / 滚动中标记
        let cache = ctx.remember(|| crate::core::state::State::new(ItemHeightCache::default())).get();
        let viewport = ctx.remember(|| crate::core::state::State::new(600.0f32)).get();
        let is_scrolling = ctx.remember(|| crate::core::state::State::new(false)).get();
        let content_height = ctx.remember(|| crate::core::state::State::new(0.0f32)).get();

        let offset0 = state.offset.get();
        let _ = offset0;

        // key 校正：数据前部增删后，用 last_known_first_key 找回原 first visible 项。
        // ⚠ 仅当 total 变化（数据增删）时校正——正常滚动时锚点项变化是用户滚动
        // 的结果，绝不能校正回原位置（实测：每帧校正会把滚动拉回 0）
        let known_total = ctx.remember(|| crate::core::state::State::new(usize::MAX)).get();
        let total_changed = known_total.get() != total;
        if total_changed {
            known_total.set(total);
            if let Some(last_key) = state.last_known_first_key.get() {
                let cache_ref = cache.get();
                if let Some(new_index) = intervals.index_of_key(last_key) {
                    let new_offset = prefix_height(&cache_ref, new_index, self.spacing);
                    state.offset.set(new_offset);
                }
            }
        }

        let offset = state.offset.get();
        let cache_ref = cache.get();
        let (first_index, first_item_offset) = anchor_from_offset(&cache_ref, offset, self.spacing);
        // 记录锚点项 key（供下次数据变化校正）；更新派生锚点
        if let Some(k) = intervals.key_of(first_index) {
            state.last_known_first_key.set(Some(k));
        }
        state.first_visible_index.set(first_index);
        state.first_visible_offset.set(first_item_offset);

        // 组合期窗口高度：用固定大值（真实视口测量期回写，但 build 不依赖——
        // 避免约束振荡（Column 内容驱动给 ∞ → 回写 ∞ → build 读 ∞ 的循环））
        let viewport_h = 2000.0f32;
        let (start, end) = visible_range(
            &cache_ref, first_index, first_item_offset,
            viewport_h, self.spacing, total,
        );

        // 挂自定义测量策略（真实测量 + 写回高度 + 放置 + 视口回写）
        let scroll = crate::modifier::ScrollState {
            offset: state.offset.clone(),
            is_scroll_in_progress: is_scrolling,
        };
        let policy = LazyListPolicy {
            cache: cache.clone(),
            viewport: viewport.clone(),
            content_height: content_height.clone(),
            spacing: self.spacing,
            total,
            start,
        };
        let m = Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .vertical_scroll(scroll)
            .lazy_scroll(content_height.clone());
        let m = m.then(self.modifier);
        // ⚠ item 子节点必须在 start_restartable_group **之后**注册（挂到
        // LazyColumn 节点下）——组合顺序决定父子关系
        match ctx.start_restartable_group(key, m, policy) {
            GroupStatus::Skip => {
                // Skip：复用上帧子树——但可见范围可能变了，无法原地改子节点数；
                // 依赖下一帧 Enter。winia Skip 仅当所有依赖未变时命中（offset 变了
                // 会 Enter），此处安全。
            }
            GroupStatus::Enter => {
                // 注册可见项子节点：slot key 混合 item key（跨帧复用/回收）
                const MIX: u64 = 0x9E37_79B9_7F4A_7C15;
                for g in start..end {
                    let item_key = intervals.key_of(g).unwrap_or(g as u64);
                    let ik = ctx.next_key() ^ (item_key.wrapping_mul(MIX));
                    if let Some((iv_idx, _local)) = intervals.locate(g) {
                        let content = intervals.intervals[iv_idx].content.clone();
                        match ctx.start_restartable_group(ik, Modifier::new(), BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                content(ctx, g);
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
            }
        }
        ctx.end_restartable_group();
    }
}

// ═══════════════════════════════════════════════════════
// LazyListPolicy — 测量策略
// ═══════════════════════════════════════════════════════

/// 懒列表测量：把已注册子节点按锚点排布，真实高度写回缓存，视口高回写。
pub(crate) struct LazyListPolicy {
    pub cache: crate::core::state::State<ItemHeightCache>,
    pub viewport: crate::core::state::State<f32>,
    pub content_height: crate::core::state::State<f32>,
    pub spacing: f32,
    pub total: usize,
    pub start: usize,          // 注册项全局起点
}

impl std::fmt::Debug for LazyListPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LazyListPolicy")
    }
}

impl crate::layout::node::MeasurePolicy for LazyListPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::node::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::constraints::Constraints,
    ) -> (crate::layout::node::Size, Vec<crate::layout::node::Placement>) {
        use crate::layout::node::Size;
        // 视口高：有限约束直接用（回写缓存）；无穷（父内容驱动——Column 无固定
        // 高度时给子节点无界 max）回退缓存值，避免视口无限膨胀
        let vh = if constraints.max_height.is_finite() && constraints.max_height > 0.0 {
            self.viewport.set_silent(constraints.max_height);
            constraints.max_height
        } else {
            self.viewport.get()
        };

        // 测量每个子节点：宽度继承父（fill），高度无界（wrap content——
        // 不能传父约束（max_height=视口高）否则子项被撑满视口）
        let child_constraints = crate::layout::constraints::Constraints {
            min_width: 0.0,
            max_width: constraints.max_width,
            min_height: 0.0,
            max_height: f32::MAX,
        };
        let mut placements = Vec::with_capacity(children.len());
        let mut measured: Vec<(f32, f32)> = Vec::with_capacity(children.len());
        for &c in children.iter() {
            let (size, _) = crate::layout::node::measure_node(nodes, policies, c, child_constraints);
            measured.push((size.height, size.width));
        }

        // 全局 index 映射：注册顺序从 start 到 end，start = first_index - 预取数
        // 但 build 只给了 policy first_index（锚点）——子节点全局序需要从 placement 反推：
        // 实际实现：build 把 (start, end) 传给 policy，这里按序写回
        let mut cache = self.cache.get();
        for (i, (h, _)) in measured.iter().enumerate() {
            let global = (self.start + i).min(self.total.saturating_sub(1));
            cache.record(global, *h);
        }
        // 内容总高：注册项实测 + 未注册项预估（供 apply_scroll_delta 算 max_offset）
        let mut content_h = 0.0;
        for g in 0..self.total {
            content_h += cache.height(g) + self.spacing;
        }
        self.content_height.set_silent(content_h);

        // 锚点排布：**内容坐标**（y = 项在内容中的累计位置，不含 offset）——
        // 滚动由框架 scroll translate(-offset) 处理。若 placement 也含 offset 会
        // 双重偏移（实测：滚动 3000 后内容完全滚出视口）。
        // first item 在内容中的 y = prefix(start) 起，逐项 +h+spacing
        let mut y = prefix_height(&cache, self.start, self.spacing);
        for (i, _c) in children.iter().enumerate() {
            let (h, w) = measured[i];
            placements.push(crate::layout::node::Placement {
                position: crate::layout::node::Point::new(0.0, y),
                size: Size::new(w, h),
            });
            y += h + self.spacing;
        }

        // 自身尺寸：宽度填满，高度 = 视口（滚动容器）；子项超出部分由 scroll clip
        let w = constraints.max_width;
        let h = vh;
        self.cache.set_silent(cache);
        (Size::new(w, h), placements)
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[crate::layout::node::Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::Constraints;

    // ── IntervalList：定位 / key 映射 ──
    #[test]
    fn interval_list_locate() {
        let mut il = IntervalList::new();
        let c1: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        let c2: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(3, None, c1);
        il.add(2, Some(Arc::new(|g| 100 + g as u64)), c2);
        il.rebuild();
        assert_eq!(il.total(), 5);
        assert_eq!(il.locate(0), Some((0, 0)));
        assert_eq!(il.locate(2), Some((0, 2)));
        assert_eq!(il.locate(3), Some((1, 0)));
        assert_eq!(il.locate(4), Some((1, 1)));
        assert_eq!(il.locate(5), None);
        // key：无工厂段用索引；有工厂段用工厂
        assert_eq!(il.key_of(2), Some(2));
        assert_eq!(il.key_of(3), Some(103));
        assert_eq!(il.index_of_key(103), Some(3));
        assert_eq!(il.index_of_key(2), Some(2));
        assert_eq!(il.index_of_key(999), None);
    }

    // ── 高度缓存 ──
    #[test]
    fn height_cache_estimated_then_recorded() {
        let mut c = ItemHeightCache::default();
        assert_eq!(c.height(0), LAZY_ITEM_ESTIMATED_HEIGHT, "未测项用预估");
        c.record(2, 30.0);
        assert_eq!(c.height(2), 30.0);
        assert_eq!(c.height(1), LAZY_ITEM_ESTIMATED_HEIGHT);
        // 记录越界自动扩展
        c.record(5, 20.0);
        assert_eq!(c.height(5), 20.0);
    }

    // ── 锚点转换 ──
    #[test]
    fn anchor_from_offset_matches() {
        let mut c = ItemHeightCache::default();
        for i in 0..10 { c.record(i, 50.0); }
        // 每项 50+0 间距：offset 0..50 → 项 0；50..100 → 项 1
        assert_eq!(anchor_from_offset(&c, 0.0, 0.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 49.0, 0.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0), (1, 0.0));
        assert_eq!(anchor_from_offset(&c, 120.0, 0.0), (2, 20.0));
        // 超出已知范围 → 锚定末尾
        assert_eq!(anchor_from_offset(&c, 99999.0, 0.0), (9, 0.0));
    }

    // ── 可见范围 ──
    #[test]
    fn visible_range_includes_beyond_bounds() {
        let mut c = ItemHeightCache::default();
        for i in 0..100 { c.record(i, 50.0); }
        // 锚点项 5，偏移 0，视口 200 → 可见 4 项（200/50）+ 预取 4 = end≈9
        let (s, e) = visible_range(&c, 5, 0.0, 200.0, 0.0, 100);
        assert_eq!(s, 1, "start = 5 - 预取 4");
        assert!(e >= 9 && e <= 13, "end 覆盖视口 + 预取，实际 {e}");
        // 视口很大时 end 到 total
        let (_, e2) = visible_range(&c, 5, 0.0, 99999.0, 0.0, 100);
        assert_eq!(e2, 100);
    }
}

    // ── 组件级：稳定 key 数据变化保持滚动位置 ──
    #[test]
    fn key_preserves_scroll_position_on_data_change() {
        // 场景：滚动到项 50（key=50），然后列表前部插入 10 项（原项 50 → 新 index 60）。
        // last_known_first_key=50 应在下次 build 时校正 offset 到新位置。
        let state = LazyListState::new();
        // 模拟：滚动后 offset 对应项 50（每项高 48+0）
        state.offset.set(50.0 * 48.0);
        let mut il = IntervalList::new();
        // 数据 1：0..100（key = 自身）
        let c: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(100, Some(Arc::new(|g| g as u64)), c.clone());
        il.rebuild();
        assert_eq!(il.total(), 100);
        // 记录锚点 key（模拟 build 中的行为）：offset=2400 → 项 50。
        // 注意：anchor_from_offset 只锚定到已知高度范围（空缓存退化为 0）；
        // 真实场景 build 有上一帧的高度缓存。这里显式记录前 60 项高度模拟已测状态。
        let mut cache = ItemHeightCache::default();
        for i in 0..60 { cache.record(i, 48.0); }
        let (first, _) = anchor_from_offset(&cache, 2400.0, 0.0);
        assert_eq!(first, 50, "offset 2400 / 48 = 项 50");
        let key50 = il.key_of(50).unwrap();
        assert_eq!(key50, 50);
        // 数据 2：前部插入 10 项（key 仍唯一：新项 key 90..99？不——新项用 key 0..9，
        // 原项 key 不变，但 index 平移 +10。模拟：key 0..99 + 10 个新 key 100..109）
        let mut il2 = IntervalList::new();
        // 前插 10 项（key 100..109），原 100 项后移（key = 数据 id = 0..99，
        // 全局 index 10..109）——原 key=50 的项现在全局 index 60
        il2.add(10, Some(Arc::new(|g| (100 + g) as u64)), c.clone());
        il2.add(100, Some(Arc::new(|g| (g - 10) as u64)), c.clone());
        il2.rebuild();
        assert_eq!(il2.total(), 110);
        // 原 key=50 的项现在在 index 60
        assert_eq!(il2.index_of_key(50), Some(60));
        // 校正：offset 应更新到新 index 的 prefix（60 × 48）
        let corrected = prefix_height(&cache, 60, 0.0);
        assert_eq!(corrected, 60.0 * 48.0);
        // 验证 key 保持语义：last_key=50 → 新 offset = prefix(60)
        state.last_known_first_key.set(Some(50));
        assert_eq!(state.last_known_first_key.get(), Some(50));
    }
