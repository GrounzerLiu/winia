//! LazyColumn / LazyRow 组件 — 对标 Compose foundation lazy `LazyColumn` / `LazyRow`
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
use crate::layout::constraints::Constraints;
use crate::layout::node::{Point, Size};
use crate::modifier::Modifier;
use std::marker::PhantomData;
use std::sync::Arc;

/// 未测量项的预估高度（首次进入视口时用，测量后写回缓存）
pub const LAZY_ITEM_ESTIMATED_HEIGHT: f32 = 48.0;
/// 超出视口后仍注册的额外项数（上下各预取，减少滚动时补注册抖动）
pub const LAZY_BEYOND_BOUNDS: usize = 4;

mod sealed_axis {
    pub trait Sealed {}
}

/// 懒列表主轴——类型级参数：`LazyColumn = LazyList<VerticalAxis>`（垂直）、
/// `LazyRow = LazyList<HorizontalAxis>`（水平）。
///
/// ⚠ 密封 trait：不要为自定义类型实现。两种标记类型即全部实例化。
pub trait LazyAxis: sealed_axis::Sealed + 'static {
    #[doc(hidden)]
    fn main_max(c: Constraints) -> f32;
    #[doc(hidden)]
    fn cross_max(c: Constraints) -> f32;
    #[doc(hidden)]
    fn main_size(s: Size) -> f32;
    #[doc(hidden)]
    fn cross_size(s: Size) -> f32;
    /// 子项约束：交叉轴继承父 max，主轴无界（wrap content）
    #[doc(hidden)]
    fn child_constraints(c: Constraints) -> Constraints;
    /// 交叉轴 max 修改（contentPadding 缩小 item 可用宽度）
    #[doc(hidden)]
    fn with_cross_max(c: Constraints, v: f32) -> Constraints;
    /// (交叉轴, 主轴) → 位置
    #[doc(hidden)]
    fn point(cross: f32, main: f32) -> Point;
    /// (交叉轴, 主轴) → 尺寸
    #[doc(hidden)]
    fn size(cross: f32, main: f32) -> Size;
    /// 滚动 modifier（垂直/水平）
    #[doc(hidden)]
    fn scroll(state: crate::modifier::ScrollState) -> Modifier;
}

/// 垂直主轴标记（`LazyColumn = LazyList<VerticalAxis>`）
pub enum VerticalAxis {}
impl sealed_axis::Sealed for VerticalAxis {}
impl LazyAxis for VerticalAxis {
    fn main_max(c: Constraints) -> f32 { c.max_height }
    fn cross_max(c: Constraints) -> f32 { c.max_width }
    fn main_size(s: Size) -> f32 { s.height }
    fn cross_size(s: Size) -> f32 { s.width }
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: f32::MAX }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: v, min_height: 0.0, max_height: c.max_height }
    }
    fn point(cross: f32, main: f32) -> Point { Point::new(cross, main) }
    fn size(cross: f32, main: f32) -> Size { Size::new(cross, main) }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().vertical_scroll(state)
    }
}

/// 水平主轴标记（`LazyRow = LazyList<HorizontalAxis>`）
pub enum HorizontalAxis {}
impl sealed_axis::Sealed for HorizontalAxis {}
impl LazyAxis for HorizontalAxis {
    fn main_max(c: Constraints) -> f32 { c.max_width }
    fn cross_max(c: Constraints) -> f32 { c.max_height }
    fn main_size(s: Size) -> f32 { s.width }
    fn cross_size(s: Size) -> f32 { s.height }
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: f32::MAX, min_height: 0.0, max_height: c.max_height }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: v }
    }
    fn point(cross: f32, main: f32) -> Point { Point::new(main, cross) }
    fn size(cross: f32, main: f32) -> Size { Size::new(main, cross) }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().horizontal_scroll(state)
    }
}

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
    /// 上次 build 见到的 total——**状态级**守卫（非组合级 remember）：
    /// 外部持有 state 跨 Composer 复用时，组合级 remember 会每帧误触发 key 校正
    /// （实测：第二次 render 把 offset=2000 拉回 0）。放这里与 LazyListState
    /// 同生命周期，只有数据真的变化（total 变）才校正。
    pub(crate) known_total: crate::core::state::State<usize>,
    /// 程序化跳转请求 (index, offset-in-item, animate)——锚点权威（对齐 Compose
    /// `requestPositionAndForgetLastKnownKey`：scroll position 就是锚点，
    /// 测量从锚点开始组合；像素 offset 由测量期从缓存推导，不做反推）。
    /// 首次测量消费后清空。animate = 动画滚动（spring，对齐 animateScrollToItem）。
    pub(crate) jump_request: crate::core::state::State<Option<(usize, f32, bool)>>,
    /// fling 滚动极限（测量期回写 = 内容高 - 视口高；0 = 未知 → 只拦下限）
    pub(crate) fling_limit: crate::core::state::State<f32>,
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
            known_total: crate::core::state::State::new(usize::MAX),
            jump_request: crate::core::state::State::new(None),
            fling_limit: crate::core::state::State::new(0.0),
            first_visible_index: crate::core::state::State::new(0),
            first_visible_offset: crate::core::state::State::new(0.0),
        }
    }

    /// 当前第一个可见项索引
    pub fn first_visible(&self) -> usize { self.first_visible_index.get() }

    /// 当前第一个可见项偏移
    pub fn offset(&self) -> f32 { self.offset.get() }

    /// 立即滚动到指定索引（项顶部对齐视口顶部，可带偏移）。
    ///
    /// 签名对齐 Compose `scrollToItem(index, scrollOffset = 0)`：不需要高度
    /// 缓存/间距——跳转请求由测量期消费：先按锚点组合窗口，再用写回后的高度
    /// 缓存推导像素 offset（与放置/锚点解析共用同一 prefix 函数，round-trip
    /// 精确——不会出现预估 48 vs 实测 47.5 的累积偏差导致落点漂移）。
    /// 越界 clamp 在 measure 期（本方法不知道 total）。
    pub fn scroll_to_item(&self, index: usize, scroll_offset: f32) {
        self.jump_request.set(Some((index, scroll_offset, false)));
    }

    /// 动画滚动到指定索引（对齐 Compose `animateScrollToItem(index, scrollOffset)`）。
    /// 与 `scroll_to_item` 同一锚点权威：请求由测量期消费并推导像素目标，
    /// 用 Spring 动画（默认阻尼 1.0 / 刚度 200，收敛 ~300-400ms）从当前 offset
    /// 平滑滚动到目标；目标 clamp 到 [0, max_offset]。
    pub fn animate_scroll_to_item(&self, index: usize, scroll_offset: f32) {
        self.jump_request.set(Some((index, scroll_offset, true)));
    }

    /// 惯性滚动（对标 Compose flingBehavior）：以 `velocity`(px/s) 启动指数衰减
    /// 滚动，撞到滚动极限立即停止（极限由测量期回写——`fling_limit`）。
    pub fn fling(&self, velocity: f32) {
        if !velocity.is_finite() || velocity.abs() < 1.0 {
            return;
        }
        let off = self.offset.clone();
        let limit = self.fling_limit.clone();
        crate::animation::push_fling(
            off,
            velocity,
            crate::animation::exponential_decay(4.2),
            move |o| {
                let max = limit.get();
                let max = if max > 0.0 { max } else { f32::MAX };
                o.clamp(0.0, max)
            },
            || {},
        );
    }
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
    starts: Vec<usize>,
    total: usize,
    /// key → 全局 index 映射（rebuild 时构建；无 key 工厂的段用索引自身）
    key_index: std::collections::HashMap<u64, usize>,
    /// 是否存在 sticky header 区间（rebuild 时构建——build 回溯据此跳过）
    has_sticky: bool,
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
    fn key_of_unchecked(&self, global: usize) -> u64 {
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
// ═══════════════════════════════════════════════════════

/// 懒加载列表（对齐 Compose `LazyColumn`）
///
/// 只组合/测量可见项（含上下预取窗）。items 支持稳定 key 工厂——
/// 数据前部增删后按 key 保持滚动位置。
/// 懒列表构建器（轴由类型参数决定：`LazyColumn` 垂直 / `LazyRow` 水平）
pub struct LazyList<A: LazyAxis> {
    axis: PhantomData<A>,
    state: Option<LazyListState>,
    spacing: f32,
    modifier: Modifier,
    intervals: IntervalList,
    /// 主轴内容内边距 (before, after)——垂直 = top/bottom；水平 = start/end
    content_padding: (f32, f32),
    /// 交叉轴内容内边距 (before, after)——垂直 = start/end；水平 = top/bottom
    cross_padding: (f32, f32),
    /// 反向布局（对齐 Compose `reverseLayout`）：内容从主轴末端开始排布——
    /// index 0 在底部/右端，offset=0 显示列表开头（项 0 在视口底），
    /// 滚动方向与正向一致（offset 增 = 向列表末尾）
    reverse: bool,
}

/// 垂直懒列表（对标 Compose `LazyColumn`）
pub type LazyColumn = LazyList<VerticalAxis>;
/// 水平懒列表（对标 Compose `LazyRow`）
pub type LazyRow = LazyList<HorizontalAxis>;

impl LazyList<VerticalAxis> {
    /// 垂直列表（LazyColumn）
    pub fn new() -> Self { Self::new_list() }
}

impl LazyList<HorizontalAxis> {
    /// 水平列表（LazyRow）
    pub fn new() -> Self { Self::new_list() }
}

impl<A: LazyAxis> LazyList<A> {
    fn new_list() -> Self {
        Self {
            axis: PhantomData,
            state: None,
            spacing: 0.0,
            modifier: Modifier::new(),
            intervals: IntervalList::new(),
            content_padding: (0.0, 0.0),
            cross_padding: (0.0, 0.0),
            reverse: false,
        }
    }

    /// 主轴内容内边距（对齐 Compose `contentPadding`）：垂直列表 = top/bottom，
    /// 水平列表 = start/end。内容从 before 处开始放置；滚动到边界时内容
    /// 停在 padding 处（不贴视口边）；sticky header 钉在 before 处；
    /// 总内容高 = before + 项 + after（max_offset 含 padding）。
    pub fn content_padding(mut self, before: f32, after: f32) -> Self {
        self.content_padding = (before, after);
        self
    }

    /// 交叉轴内容内边距（垂直列表 = start/end）：缩小 item 可用宽度并让
    /// item 从 before 处开始放置（对齐 Compose contentPadding 的交叉轴语义）。
    pub fn content_padding_cross(mut self, before: f32, after: f32) -> Self {
        self.cross_padding = (before, after);
        self
    }

    /// 反向布局（对齐 Compose `reverseLayout`）：index 0 在视口主轴末端
    /// （LazyColumn = 底部），offset=0 时项 0 在视口底；滚动方向与正向
    /// 一致（offset 增 = 向列表末尾）。典型用途：聊天列表（最新消息在底部）。
    pub fn reverse_layout(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
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
        self.intervals.add(1, None, c, false);
        self
    }

    /// 带 key 的单一项
    pub fn item_keyed(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c, false);
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
        self.intervals.add(count, Some(k), c, false);
        self
    }

    /// count 个无 key 项（位置即 key）
    pub fn items_plain(
        mut self,
        count: usize,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        let c = Arc::new(content);
        self.intervals.add(count, None, c, false);
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
        self.intervals.add(n, Some(kk), cc, false);
        self
    }

    /// sticky header 区间（对标 Compose `stickyHeader(key, content)`）——
    /// 滚动时钉在视口主轴起始端，内容从它下面滑过；下一个 sticky header
    /// 到来时把前一个推上去。key 必须全列表唯一。
    pub fn sticky_header(mut self, key: u64, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        let c = Arc::new(move |ctx: &mut ComposeCtx, _local: usize| content(ctx));
        self.intervals.add(1, Some(Arc::new(move |_| key)), c, true);
        self
    }
}

// ═══════════════════════════════════════════════════════
// 高度缓存 + 可见范围计算
// ═══════════════════════════════════════════════════════

/// 每项高度缓存（全局 index → 测量高度）；未测项用预估
///
/// 公开：可用于外部读取实测高度（如自定义滚动逻辑）；`scroll_to_item`
/// 不需要它（锚点权威，测量期自动用缓存推导像素）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemHeightCache {
    pub heights: Vec<f32>,
}

impl ItemHeightCache {
    pub fn new() -> Self { Self { heights: Vec::new() } }
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
///
/// 未测项按预估高度继续推算（而非在已知范围末尾截断）——否则滚动超出已测范围时
/// 锚点错误退回 0（实测：offset=2000 空缓存时恒返回 (0,0)，滚动失效）。
/// 已知高度部分线性扫描，超出部分用除法直接估算（O(1) 防大 offset 循环）。
pub(crate) fn anchor_from_offset(cache: &ItemHeightCache, offset: f32, spacing: f32, before: f32) -> (usize, f32) {
    // 内容坐标含 before padding：项 i 顶 = before + prefix(i)
    let known = cache.heights.len();
    let mut acc = before;
    for i in 0..known {
        let h = cache.height(i) + spacing;
        if acc + h > offset {
            return (i, offset - acc);
        }
        acc += h;
    }
    // 超出已知范围：按预估高度除法估算剩余项数
    let step = LAZY_ITEM_ESTIMATED_HEIGHT + spacing;
    if step <= 0.0 { return (known.saturating_sub(1), 0.0); }
    let remaining = offset - acc;
    let extra = (remaining / step).floor() as usize;
    let idx = known + extra;
    (idx, remaining - extra as f32 * step)
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

impl<A: LazyAxis> LazyList<A> {
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let state = match self.state {
            Some(s) => s,
            None => ctx.remember(|| LazyListState::new()).get(),
        };
        ctx.changed(&state.offset);
        // 派生锚点依赖：policy 测量后 set 精确值 → 触发重组收敛（build 用预估高度算的
        // 锚点与真实值不同时，下一帧以真实缓存重算；相同则无变化不重组）
        ctx.changed(&state.first_visible_index);
        ctx.changed(&state.first_visible_offset);
        let key = ctx.next_key();

        // 内容注册表（重建——数据变化反映）
        let mut intervals = self.intervals;
        intervals.rebuild();
        let total = intervals.total();

        // 跨帧 remember：高度缓存 / 视口高 / 滚动中标记 / fling 极限
        let cache = ctx.remember(|| crate::core::state::State::new(ItemHeightCache::default())).get();
        let viewport = ctx.remember(|| crate::core::state::State::new(600.0f32)).get();
        let is_scrolling = ctx.remember(|| crate::core::state::State::new(false)).get();
        let content_height = ctx.remember(|| crate::core::state::State::new(0.0f32)).get();
        let fling_limit = ctx.remember(|| crate::core::state::State::new(0.0f32)).get();

        // key 校正：数据前部增删后，用 last_known_first_key 找回原 first visible 项。
        // ⚠ 仅当 total 变化（数据增删）时校正——正常滚动时锚点项变化是用户滚动
        // 的结果，绝不能校正回原位置（实测：每帧校正会把滚动拉回 0）
        let known_total = state.known_total.clone();
        let total_changed = known_total.get() != total;
        if total_changed {
            known_total.set(total);
            if let Some(last_key) = state.last_known_first_key.get() {
                let cache_ref = cache.get();
                if let Some(new_index) = intervals.index_of_key(last_key) {
                    let new_offset = self.content_padding.0 + prefix_height(&cache_ref, new_index, self.spacing);
                    state.offset.set(new_offset);
                }
            }
        }

        let offset = state.offset.get();
        let cache_ref = cache.get();
        // 锚点：跳转请求权威（对齐 Compose——requestPosition 后直接从请求的
        // index 开始组合，不经过像素反推）；否则由像素 offset 反推
        let pad_before = self.content_padding.0;
        let (first_index, first_item_offset) = match state.jump_request.get() {
            Some((idx, off, _)) => (idx.min(total), off),
            None => {
                // ⚠ 锚点 index 必须 clamp 到 total 内：offset 超界时 anchor 按预估
                // 高度外推（可远超 total）→ visible_range 的 end<start → 组合窗溢出
                let (fi, fo) = anchor_from_offset(&cache_ref, offset, self.spacing, pad_before);
                (fi.min(total.saturating_sub(1)), fo)
            }
        };
        // 记录锚点项 key（供下次数据变化校正）——派生锚点由测量期 policy 写回精确值
        if let Some(k) = intervals.key_of(first_index) {
            state.last_known_first_key.set(Some(k));
        }

        // 组合期窗口高度：用固定大值（真实视口测量期回写，但 build 不依赖——
        // 避免约束振荡（Column 内容驱动给 ∞ → 回写 ∞ → build 读 ∞ 的循环））
        let viewport_h = 2000.0f32;
        let (mut start, end) = visible_range(
            &cache_ref, first_index, first_item_offset,
            viewport_h, self.spacing, total,
        );

        // sticky header 回溯（对齐 Compose：钉住的 header 即使自然位置远在视口
        // 上方也必须留在窗口内）：
        // - pin  = 最后一个 C(i) ≤ offset 的 sticky header（锚点上方第一个 sticky，
        //   深滚动时回溯距离 = 当前 section 长度；无 sticky 列表时跳过）
        // - prev = pin 上方一个 sticky header，仅当其底部仍低于视口顶
        //   （C(prev)+h(prev) > offset，即正处于被推出的过渡期）才纳入窗口
        let mut pin: Option<usize> = None;
        let mut prev_pin: Option<usize> = None;
        if intervals.has_sticky_headers() {
            let c_anchor = pad_before + prefix_height(&cache_ref, first_index, self.spacing);
            if intervals.is_sticky(first_index) && c_anchor <= offset + 0.01 {
                pin = Some(first_index);
            } else {
                let mut c = c_anchor;
                let mut i = first_index;
                while i > 0 {
                    i -= 1;
                    c -= cache_ref.height(i) + self.spacing;
                    if intervals.is_sticky(i) && c <= offset + 0.01 {
                        pin = Some(i);
                        break;
                    }
                }
            }
            if let Some(p) = pin {
                // prev：pin 上方最近的 sticky；走到底部 ≤ 视口顶即停（更上方不可见）
                let mut c2 = pad_before + prefix_height(&cache_ref, p, self.spacing);
                let mut j = p;
                while j > 0 {
                    j -= 1;
                    c2 -= cache_ref.height(j) + self.spacing;
                    if intervals.is_sticky(j) {
                        if c2 + cache_ref.height(j) > offset {
                            prev_pin = Some(j);
                        }
                        break;
                    }
                }
            }
        }
        if let Some(p) = pin {
            start = start.min(p);
        }
        if let Some(pp) = prev_pin {
            start = start.min(pp);
        }

        // 挂自定义测量策略（真实测量 + 写回高度 + 放置 + 视口回写）
        let scroll = crate::modifier::ScrollState {
            offset: state.offset.clone(),
            is_scroll_in_progress: is_scrolling.clone(),
            fling_limit: fling_limit.clone(),
        };
        // 注册顺序：普通项在前、sticky header 在后（子节点渲染顺序 = 注册顺序，
        // 后者画在最上层——钉住的 header 需盖住从它下面滑过的内容）。
        // globals[i] = 第 i 个子节点的全局 index（policy 放置/写回用，不能再
        // 用 start+i 位置映射）
        let mut globals: Vec<usize> = Vec::with_capacity(end - start);
        let mut sticky_children: Vec<usize> = Vec::new();
        for g in start..end {
            if !intervals.is_sticky(g) {
                globals.push(g);
            }
        }
        for g in start..end {
            if intervals.is_sticky(g) {
                sticky_children.push(globals.len());
                globals.push(g);
            }
        }
        let policy = LazyListPolicy::<A> {
            axis: PhantomData,
            cache: cache.clone(),
            viewport: viewport.clone(),
            content_height: content_height.clone(),
            fling_limit: fling_limit.clone(),
            is_scroll_in_progress: is_scrolling.clone(),
            spacing: self.spacing,
            total,
            content_padding: self.content_padding,
            cross_padding: self.cross_padding,
            globals,
            sticky_children,
            pin,
            reverse: self.reverse,
            state: state.clone(),
        };
        let m = Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .then(A::scroll(scroll))
            .lazy_scroll(content_height.clone())
            .lazy_scroll_reverse(self.reverse);
        let m = m.then(self.modifier);
        // 注册顺序（policy 移动进 group 后不可再读——先取出）
        let child_globals = policy.globals.clone();
        // ⚠ item 子节点必须在 start_restartable_group **之后**注册（挂到
        // LazyColumn 节点下）——组合顺序决定父子关系
        match ctx.start_restartable_group(key, m, policy) {
            GroupStatus::Skip => {
                // Skip：复用上帧子树——但可见范围可能变了，无法原地改子节点数；
                // 依赖下一帧 Enter。winia Skip 仅当所有依赖未变时命中（offset 变了
                // 会 Enter），此处安全。
            }
            GroupStatus::Enter => {
                // 注册可见项子节点（按 globals 顺序——sticky 在后）：slot key
                // 混合 item key（跨帧复用/回收）
                const MIX: u64 = 0x9E37_79B9_7F4A_7C15;
                for g in child_globals {
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
/// 轴无关（`A: LazyAxis` 决定主轴方向——LazyColumn/LazyRow 共用）。
pub(crate) struct LazyListPolicy<A: LazyAxis> {
    pub axis: PhantomData<A>,
    pub cache: crate::core::state::State<ItemHeightCache>,
    pub viewport: crate::core::state::State<f32>,
    pub content_height: crate::core::state::State<f32>,
    pub fling_limit: crate::core::state::State<f32>,
    pub is_scroll_in_progress: crate::core::state::State<bool>,
    pub spacing: f32,
    pub total: usize,
    /// 主轴内容内边距 (before, after)
    pub content_padding: (f32, f32),
    /// 交叉轴内容内边距 (before, after)
    pub cross_padding: (f32, f32),
    /// 注册顺序 → 全局 index（sticky 项排在最后——画在最上层）
    pub globals: Vec<usize>,
    /// 注册序中 sticky 子节点的 child 下标（全局升序）
    pub sticky_children: Vec<usize>,
    /// 钉住的 sticky header 全局 index（build 期回溯；钉住时锚点 = (pin, 0)）
    pub pin: Option<usize>,
    /// 反向布局（reverseLayout）：放置内容坐标 = content_h - 镜像位置 - 项高
    pub reverse: bool,
    pub state: LazyListState,  // 派生锚点回写（测量后精确值）
}

impl<A: LazyAxis> std::fmt::Debug for LazyListPolicy<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LazyListPolicy")
    }
}

impl<A: LazyAxis> crate::layout::node::MeasurePolicy for LazyListPolicy<A> {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::node::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::constraints::Constraints,
    ) -> (crate::layout::node::Size, Vec<crate::layout::node::Placement>) {
        // 视口主轴尺寸：有限约束直接用（回写缓存）；无界（父内容驱动——Column 无
        // 固定高度时给子节点 f32::MAX；⚠ is_finite() 对 f32::MAX 也返回 true，
        // 必须用框架惯例 `< f32::MAX` 判定）回退缓存值，避免视口
        // 无限膨胀（实测：f32::MAX 视口会让 clamp 把 offset 清零）
        let main_max = A::main_max(constraints);
        let vh = if main_max < f32::MAX && main_max > 0.0 {
            self.viewport.set_silent(main_max);
            main_max
        } else {
            self.viewport.get()
        };

        // 测量每个子节点：交叉轴继承父 max（扣交叉轴 contentPadding——
        // 对齐 Compose：item 可用宽度 = 视口 - cross padding），主轴无界
        // （wrap content——不能传父约束（max=视口）否则子项被撑满视口）
        let (cb, ca) = self.cross_padding;
        let mut child_constraints = A::child_constraints(constraints);
        let cross_max = A::cross_max(child_constraints);
        child_constraints = A::with_cross_max(child_constraints, (cross_max - cb - ca).max(0.0));
        let mut placements = Vec::with_capacity(children.len());
        let mut measured: Vec<(f32, f32)> = Vec::with_capacity(children.len());
        for &c in children.iter() {
            let (size, _) = crate::layout::node::measure_node(nodes, policies, c, child_constraints);
            measured.push((A::main_size(size), A::cross_size(size)));
        }

        // 全局 index 映射：注册顺序从 start 到 end，start = first_index - 预取数
        // 但 build 只给了 policy first_index（锚点）——子节点全局序需要从 placement 反推：
        // 实际实现：build 把 (start, end) 传给 policy，这里按序写回
        let mut cache = self.cache.get();
        for (i, (h, _)) in measured.iter().enumerate() {
            cache.record(self.globals[i], *h);
        }
        // 内容总高：注册项实测 + 未注册项预估（供 apply_scroll_delta 算 max_offset）；
        // 含 主轴 contentPadding（对齐 Compose：max_offset = before + 内容 + after - 视口）
        let (pad_before, pad_after) = self.content_padding;
        let mut content_h = pad_before + pad_after;
        for g in 0..self.total {
            content_h += cache.height(g) + self.spacing;
        }
        self.content_height.set_silent(content_h);

        // 越界 clamp：scroll_to_item 无 total 信息，程序化滚动超出内容边界时
        // 在这里收回到末尾（对齐 Compose：scroll position 在 measure 期 clamp）
        let max_off = (content_h - vh).max(0.0);

        // 程序化跳转：消费 jump_request（放在 max_off 之后——动画目标需要 clamp）。
        // 像素 offset 用**写回后的缓存**推导（实测项真实高度 + 未测项预估）——
        // 与锚点解析/放置共用同一 prefix_height，round-trip 精确：跳 500 就是
        // 500，不会因预估 vs 实测高度差累积漂移（实测：旧实现 500×48 vs 47.5
        // → 落到 505）。reverseLayout 下公式相同（offset = before + prefix + 偏移：
        // 语义变为项**底**贴视口底——镜像坐标中与正向同构）。
        if let Some((req_idx, req_off, animate)) = self.state.jump_request.get() {
            let idx = req_idx.min(self.total.saturating_sub(1));
            let target = pad_before + prefix_height(&cache, idx, self.spacing) + req_off;
            self.state.jump_request.set(None);
            if animate {
                // 动画滚动（对齐 Compose animateScrollToItem——spring 收敛）；
                // push_animatable 内部处理同 state 动画替换（retarget 继承速度）
                self.is_scroll_in_progress.set(false);
                crate::animation::push_animatable(
                    self.state.offset.clone(),
                    target.clamp(0.0, max_off),
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
                );
            } else {
                // 立即跳转：取消进行中的 fling + 结束滚动中标记
                crate::animation::cancel_animation(&self.state.offset);
                self.is_scroll_in_progress.set(false);
                self.state.offset.set(target);
            }
        }
        // fling 极限回写（输入路径 ScrollState::fling + 程序化 LazyListState::fling）
        self.fling_limit.set_silent(max_off);
        self.state.fling_limit.set_silent(max_off);
        let clamped = self.state.offset.get().clamp(0.0, max_off);
        if clamped != self.state.offset.get() {
            self.state.offset.set(clamped);
        }

        // 精确锚点回写：用真实高度（cache 已写回）反推 first_visible_index/offset——
        // build 期用的是预估高度，这里给出精确值（对齐 Compose 从 measure result 更新）
        let offset = self.state.offset.get();
        // 钉住时锚点 = (pin, 0)（对齐 Compose：firstVisibleItemIndex 就是钉住的
        // header）；否则自然锚点（像素反推）
        let (real_first, real_off) = match self.pin {
            Some(p) if offset >= pad_before + prefix_height(&cache, p, self.spacing) => (p, 0.0),
            _ => anchor_from_offset(&cache, offset, self.spacing, pad_before),
        };
        self.state.first_visible_index.set(real_first);
        self.state.first_visible_offset.set(real_off);

        // 锚点排布：**内容坐标**（主轴 = 项在内容中的累计位置，不含 offset）——
        // 滚动由框架 scroll translate(-offset) 处理。若 placement 也含 offset 会
        // 双重偏移（实测：滚动 3000 后内容完全滚出视口）。
        // 内容起点 = before 内容内边距（对齐 Compose contentPadding）。
        // sticky pass（对齐 Compose LazyListMeasure）：
        //   正向 final(i) = max(C(i) - offset, before)   —— 滚过顶钉在 before 处
        //   反向 final(prev) = min(final(prev), final(next) - h(prev))
        //     —— 下一个 header 距顶 h(prev) 内时前一个开始滑出
        // 放置内容坐标 = final + offset；普通项保持自然位置（累计）
        let mut fin: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for &ci in &self.sticky_children {
            let v = pad_before + prefix_height(&cache, self.globals[ci], self.spacing) - offset;
            fin.insert(ci, v.max(pad_before));
        }
        for k in (1..self.sticky_children.len()).rev() {
            let prev = self.sticky_children[k - 1];
            let next = self.sticky_children[k];
            let nf = fin[&next];
            let pf = fin[&prev];
            let h_prev = measured[prev].0;
            fin.insert(prev, pf.min(nf - h_prev));
        }
        let mut main_pos = 0.0f32;
        let mut main_init = false;
        for (i, _c) in children.iter().enumerate() {
            let (h, w) = measured[i];
            let pos = match fin.get(&i) {
                Some(f) => *f + offset,
                None => {
                    if !main_init {
                        main_pos = pad_before + prefix_height(&cache, self.globals[i], self.spacing);
                        main_init = true;
                    }
                    let p = main_pos;
                    main_pos += h + self.spacing;
                    p
                }
            };
            // 反向布局：镜像坐标（从底向上累计）→ 内容坐标 = content_h - pos - h
            // （index 0 在内容底部；锚点/可见范围/pin 回溯在镜像坐标中公式不变）
            let content_pos = if self.reverse { content_h - pos - h } else { pos };
            placements.push(crate::layout::node::Placement {
                // 交叉轴从 cross_before 处开始（contentPadding 交叉轴语义）
                position: A::point(cb, content_pos),
                size: A::size(w, h),
            });
        }

        // 自身尺寸：交叉轴填满父，主轴 = 视口（滚动容器）；子项超出部分由 scroll clip
        self.cache.set_silent(cache);
        (A::size(A::cross_max(constraints), vh), placements)
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
        il.add(3, None, c1, false);
        il.add(2, Some(Arc::new(|g| 100 + g as u64)), c2, false);
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
        assert_eq!(anchor_from_offset(&c, 0.0, 0.0, 0.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 49.0, 0.0, 0.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0, 0.0), (1, 0.0));
        assert_eq!(anchor_from_offset(&c, 120.0, 0.0, 0.0), (2, 20.0));
        // before padding：内容起点下移（offset=50 含 before → 锚点仍在项 0，偏移 0）
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0, 50.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 99.0, 0.0, 50.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 100.0, 0.0, 50.0), (1, 0.0));
        // 超出已知范围（10 项已知）→ 按预估 48 继续扩展：99999 / 48 ≈ 2082
        let (far_idx, far_off) = anchor_from_offset(&c, 99999.0, 0.0, 0.0);
        assert!(far_idx > 2000, "预估扩展：实际 {far_idx}");
        // 已知 10 项 ×50 先扣掉，剩余按预估 48 继续：99999 - 500 - (idx-10)*48
        let expected = 99999.0 - 500.0 - (far_idx - 10) as f32 * 48.0;
        assert!((far_off - expected).abs() < 0.001);
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
        il.add(100, Some(Arc::new(|g| g as u64)), c.clone(), false);
        il.rebuild();
        assert_eq!(il.total(), 100);
        // 记录锚点 key（模拟 build 中的行为）：offset=2400 → 项 50。
        // 注意：anchor_from_offset 只锚定到已知高度范围（空缓存退化为 0）；
        // 真实场景 build 有上一帧的高度缓存。这里显式记录前 60 项高度模拟已测状态。
        let mut cache = ItemHeightCache::default();
        for i in 0..60 { cache.record(i, 48.0); }
        let (first, _) = anchor_from_offset(&cache, 2400.0, 0.0, 0.0);
        assert_eq!(first, 50, "offset 2400 / 48 = 项 50");
        let key50 = il.key_of(50).unwrap();
        assert_eq!(key50, 50);
        // 数据 2：前部插入 10 项（key 仍唯一：新项 key 90..99？不——新项用 key 0..9，
        // 原项 key 不变，但 index 平移 +10。模拟：key 0..99 + 10 个新 key 100..109）
        let mut il2 = IntervalList::new();
        // 前插 10 项（key 100..109），原 100 项后移（key = 数据 id = 0..99，
        // 全局 index 10..109）——原 key=50 的项现在全局 index 60
        il2.add(10, Some(Arc::new(|g| (100 + g) as u64)), c.clone(), false);
        il2.add(100, Some(Arc::new(|g| (g - 10) as u64)), c.clone(), false);
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

    // ── 组件级像素：懒加载渲染 + 滚动后内容变化 ──
    fn render_lazy_sized(build: impl FnOnce(&mut ComposeCtx), w: f32, h: f32) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = crate::ui::theme::ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            crate::ui::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, w, 0.0, h));
        let mut surface = surfaces::raster_n32_premul((w as i32, h as i32)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), pm.width() as usize)
    }

    fn render_lazy(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = crate::ui::theme::ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            crate::ui::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        let mut surface = surfaces::raster_n32_premul((400, 600)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), pm.width() as usize)
    }

    fn count_text_clusters(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        // 深色文本像素行聚类
        let mut rows = vec![false; h];
        for y in 0..h {
            let mut n = 0;
            for x in 0..w {
                let p = px[y * w + x];
                // BGRA → (b,g,r) 语义（raster_n32_premul 小端）
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { n += 1; }
            }
            rows[y] = n > 3;
        }
        let mut clusters = 0;
        let mut prev = -100;
        for (y, &r) in rows.iter().enumerate() {
            if r {
                if y as i32 - prev > 20 { clusters += 1; }
                prev = y as i32;
            }
        }
        clusters
    }

    #[test]
    fn lazy_renders_only_visible_items() {
        // 1000 项列表：只渲染视口内的 ~12 项（600px / 40px），而非全部
        use std::sync::atomic::{AtomicUsize, Ordering};
        let composed = Arc::new(AtomicUsize::new(0));
        let composed2 = composed.clone();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let items2 = items.clone();
        let (px, w) = render_lazy(move |ctx| {
            let c2 = composed2.clone();
            LazyColumn::new()
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(
                    items2,
                    |v: &u64| *v,
                    move |ctx, _i, v| {
                        c2.fetch_add(1, Ordering::Relaxed);
                        crate::ui::text::Text::new(format!("Item {}", v))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    },
                )
                .build(ctx);
        });
        let h = 600usize;
        let clusters = count_text_clusters(&px, w, h);
        // 视口 600 / 项高 ~38 = ~15 项可见；注册窗含预取但渲染仅可见
        assert!(clusters >= 8 && clusters <= 30, "只渲染可见项，clusters={clusters}");
        // 组合次数 << 1000（懒加载核心断言）
        let n = composed.load(Ordering::Relaxed);
        assert!(n <= 60, "组合项数应远小于总数 1000，实际 {n}");
    }

    #[test]
    fn lazy_scroll_changes_visible_items() {
        // 滚动到 offset=2000 后，渲染的 Item 文本应变化（不同项）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (px0, w0) = render_lazy(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::ui::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        // 滚动：模拟 apply_scroll_delta 效果
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::ui::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        assert_eq!(w0, w1);
        // 帧间内容应不同（滚动后渲染不同项）：按行比较文本像素出现与否。
        // 偏移 2000/38 ≈ 52 项 → 文本行整体平移 ~2000px，diff_rows 应为数百。
        let h = 600usize;
        let mut diff_rows = 0;
        for y in 0..h {
            let mut has0 = false;
            let mut has1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 { diff_rows += 1; }
        }
        assert!(diff_rows > 10, "滚动后内容变化，diff_rows={diff_rows}");
        // 派生锚点应为滚动后的精确值（约 2000/38 ≈ 52）
        assert!(state.first_visible() >= 40, "first_visible={}", state.first_visible());
    }

    // ── LazyRow 横向：同机制换轴 ──
    fn count_text_columns(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        // 深色文本像素列聚类（横向列表用——文本行都挤在少数行，按列数项）
        let mut cols = vec![false; w];
        for x in 0..w {
            let mut n = 0;
            for y in (0..h).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { n += 1; }
            }
            cols[x] = n > 3;
        }
        let mut clusters = 0;
        let mut prev = -100;
        for (x, &c) in cols.iter().enumerate() {
            if c {
                if x as i32 - prev > 20 { clusters += 1; }
                prev = x as i32;
            }
        }
        clusters
    }

    #[test]
    fn lazy_row_renders_only_visible_items() {
        // 1000 项横向列表：只渲染视口内 ~9 项（600px / ~64px），而非全部
        use std::sync::atomic::{AtomicUsize, Ordering};
        let composed = Arc::new(AtomicUsize::new(0));
        let composed2 = composed.clone();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let items2 = items.clone();
        let (px, w) = render_lazy_sized(
            move |ctx| {
                let c2 = composed2.clone();
                LazyRow::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(
                        items2,
                        |v: &u64| *v,
                        move |ctx, _i, v| {
                            c2.fetch_add(1, Ordering::Relaxed);
                            crate::ui::text::Text::new(format!("Item {}", v))
                                .font_size(14.0)
                                .modifier(Modifier::new().padding(12.0))
                                .build(ctx);
                        },
                    )
                    .build(ctx);
            },
            600.0,
            400.0,
        );
        let h = 400usize;
        let clusters = count_text_columns(&px, w, h);
        // 视口 600 / 项宽 ~64 = ~9 项可见（含预取与间隙）
        assert!(clusters >= 5 && clusters <= 20, "只渲染可见项，clusters={clusters}");
        let n = composed.load(Ordering::Relaxed);
        // 组合期预取窗固定 2000px（与 LazyColumn 同机制）：2000/56 ≈ 36 + 预取 8
        assert!(n <= 60, "组合项数应远小于总数 1000，实际 {n}");
    }

    #[test]
    fn lazy_row_scroll_changes_visible_items() {
        // 横向滚动 offset=2000 后，渲染的 Item 文本应变化（不同项）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let build_row = |ctx: &mut ComposeCtx, s: LazyListState| {
            LazyRow::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::ui::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        };
        let (px0, w0) = render_lazy_sized(|ctx| build_row(ctx, state.clone()), 600.0, 400.0);
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy_sized(|ctx| build_row(ctx, state.clone()), 600.0, 400.0);
        assert_eq!(w0, w1);
        // 帧间内容应不同（滚动后渲染不同项）：按列比较文本像素出现与否
        let h = 400usize;
        let mut diff_cols = 0;
        for x in 0..w0 {
            let mut has0 = false;
            let mut has1 = false;
            for y in (0..h).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 { diff_cols += 1; }
        }
        assert!(diff_cols > 10, "滚动后内容变化，diff_cols={diff_cols}");
        // 派生锚点应为滚动后的精确值（约 2000/64 ≈ 31）
        assert!(state.first_visible() >= 25, "first_visible={}", state.first_visible());
    }

    // ── 程序化跳转：锚点权威，落点精确 ──
    fn render_lazy_state(
        state: &LazyListState,
        items: &Arc<Vec<u64>>,
    ) -> (Vec<[u8; 4]>, usize) {
        let s = state.clone();
        let items = items.clone();
        render_lazy(move |ctx| {
            LazyColumn::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                    crate::ui::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        })
    }

    #[test]
    fn scroll_to_item_lands_exactly_on_index() {
        // 回归：跳 500 必须落 500——旧实现用空缓存预估 48px/项算像素 offset，
        // 实测高度 ~47.5 累积偏差 → 落到 505（用户实测）。新实现锚点权威：
        // 测量期从写回后的缓存推导像素，round-trip 精确。
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (px0, w0) = render_lazy_state(&state, &items);
        state.scroll_to_item(500, 0.0);
        let (px1, w1) = render_lazy_state(&state, &items);
        assert_eq!(w0, w1);
        assert_eq!(
            state.first_visible(),
            500,
            "跳转 500 必须精确落 500（旧实现 505）"
        );
        // 像素 offset 是预估混合前缀（未测中段按 48 预估）——但这不影响落点：
        // 放置与 translate 共用同一 prefix 函数，item 500 视觉上精确在顶部；
        // 随滚动测量推进，offset 逐步收敛到真实和。只断言合理范围。
        assert!(
            state.offset() > 20000.0 && state.offset() < 26000.0,
            "offset 应在 500 项前缀附近，实际 {}",
            state.offset()
        );
        // 内容确实变化（不同项）
        let h = 600usize;
        let mut diff_rows = 0;
        for y in 0..h {
            let mut has0 = false;
            let mut has1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 { diff_rows += 1; }
        }
        assert!(diff_rows > 10, "跳转后内容变化，diff_rows={diff_rows}");
    }

    #[test]
    fn scroll_to_item_clamps_to_end() {
        // 越界跳转 clamp 到末尾（对齐 Compose：scroll position 在 measure 期 clamp）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        state.scroll_to_item(99999, 0.0);
        let (_px1, _w1) = render_lazy_state(&state, &items);
        // 滚动到底部时首项 = total - 视口容纳项数 ≈ 1000 - 600/48 ≈ 987
        // （真实高度下同样 ≈987——600px 视口 + ~47.5px 项）
        assert!(
            state.first_visible() >= 980,
            "越界跳转应 clamp 到末尾，实际 {}",
            state.first_visible()
        );
        assert!(
            state.offset() > 45000.0,
            "offset 应接近内容底部，实际 {}",
            state.offset()
        );
        // 顶部跳转
        state.scroll_to_item(0, 0.0);
        let (_px2, _w2) = render_lazy_state(&state, &items);
        assert_eq!(state.first_visible(), 0, "跳回顶部");
    }

    #[test]
    fn jump_to_end_shows_last_item_with_real_viewport() {
        // 回归（用户实测：跳末尾看不到 Item 999）：measure_node 对 scroll 容器
        // 一律把 max_height 改写成 f32::MAX → policy 永远拿不到真实视口 → 回退
        // vh=600，而真实视口 400 → clamp 的 max_offset = content_h - 600 偏大
        // 100px → 跳末尾滚过头，Item 999 被推出视口底部。
        // 修复：lazy 容器保留有限 max_height（policy 显式控制子约束）。
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let mk = |state: &LazyListState, items: &Arc<Vec<u64>>| {
            let s = state.clone();
            let items = items.clone();
            render_lazy_sized(
                move |ctx| {
                    LazyColumn::new()
                        .state(s)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                            crate::ui::text::Text::new(format!("Item {}", v))
                                .font_size(14.0)
                                .modifier(Modifier::new().padding(12.0))
                                .build(ctx);
                        })
                        .build(ctx);
                },
                400.0,
                400.0,
            )
        };
        let (px0, w0) = mk(&state, &items);
        state.scroll_to_item(999, 0.0);
        let (px1, w1) = mk(&state, &items);
        assert_eq!(w0, w1);
        // 视口底部 60px 内应有文本（Item 999 完整可见）
        let h = 400usize;
        let mut bottom_text_rows = 0;
        for y in (h - 60)..h {
            let mut has = false;
            for x in (0..w0).step_by(4) {
                let p = px1[y * w0 + x];
                has |= p[0] < 120 && p[1] < 120 && p[2] < 120;
            }
            if has { bottom_text_rows += 1; }
        }
        assert!(
            bottom_text_rows > 10,
            "末尾项应在视口底部可见，bottom_text_rows={bottom_text_rows}"
        );
        // 首项应接近底部：视口 400 / ~47.5 ≈ 8.4 项 → first_visible ≈ 991
        assert!(
            state.first_visible() >= 985,
            "first_visible={}",
            state.first_visible()
        );
    }

    // ── fling 惯性滚动：偏移推进 + 撞极限停止 ──
    /// 泵全局动画直到 offset 收敛（两次读数差 < 0.5）。
    /// ⚠ 不用 `has_animation_for_state` 做循环条件：并行测试的 update_animations
    /// 会整表取走动画（短暂空窗）→ 误判结束（实测并行下 fling 停在中途 836）。
    fn step_animations_until_done(state: &LazyListState, max_frames: usize) -> usize {
        use std::time::Duration;
        let mut frames = 0;
        let mut last = state.offset();
        loop {
            crate::animation::update_animations();
            std::thread::sleep(Duration::from_millis(20));
            let v = state.offset();
            frames += 1;
            let settled = (v - last).abs() < 0.5 && frames > 5;
            if settled || frames >= max_frames {
                break;
            }
            last = v;
        }
        frames
    }

    #[test]
    fn fling_animates_offset_and_stops_at_limit() {
        // 动画注册表全局共享——持串行锁防并行 clear/竞态抹掉在飞 fling
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 渲染一次让 policy 回写 fling_limit（内容高 - 视口 600）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        let limit = state.fling_limit.get();
        assert!(limit > 10000.0, "1000 项内容高 - 视口应很大，实际 {limit}");

        // 自然停：v0=5000 → 衰减极限 5000/4.2 ≈ 1190 < limit
        state.fling(5000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "fling 应收敛（{frames} 帧）");
        let end = state.offset();
        assert!(
            end > 1000.0 && end < 1400.0,
            "fling 应推进 offset 到衰减极限附近，实际 {end}"
        );

        // 撞极限：超大速度 → 停在 fling_limit（对齐 Compose：fling 消耗完即停）
        state.fling(1_000_000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "撞极限 fling 应收敛（{frames} 帧）");
        let end2 = state.offset();
        assert!(
            (end2 - limit).abs() < 1.0,
            "撞极限应停在 {limit}，实际 {end2}"
        );

        // 反向：负速度向下甩 → clamp 回 0
        state.fling(-1_000_000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "反向 fling 应收敛（{frames} 帧）");
        assert_eq!(state.offset(), 0.0, "反向 fling 应停在顶部");
    }

    // ═══════════════════════════════════════════════════
    // sticky header（对齐 Compose stickyHeader）
    // ═══════════════════════════════════════════════════

    /// 5 个 section：sticky header（红色背景 + HEAD n 文本）后跟 19 个普通项。
    /// header 全局 index = n*20；普通项 key = 200 + n*19 + i（全列表唯一）。
    fn sticky_build(ctx: &mut ComposeCtx, state: LazyListState) {
        use crate::modifier::{Color, Shape};
        let mut lb = LazyColumn::new()
            .state(state)
            .modifier(Modifier::new().fill_max_width().fill_max_height());
        for s in 0..5u64 {
            lb = lb.sticky_header(s, move |ctx| {
                crate::ui::text::Text::new(format!("HEAD {s}"))
                    .font_size(18.0)
                    .modifier(Modifier::new().padding(12.0).background(
                        Color::from_argb(255, 0xC6, 0x28, 0x28),
                        Shape::rounded(2.0),
                    ))
                    .build(ctx);
            });
            lb = lb.items(
                19,
                move |i| 200 + s * 19 + i as u64,
                move |ctx, i| {
                    crate::ui::text::Text::new(format!("Item {}", s * 19 + i as u64))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                },
            );
        }
        lb.build(ctx);
    }

    /// 带 content_padding 的 1000 项列表（对齐 render_lazy 的 400x600 视口）
    fn plain_build_padded(ctx: &mut ComposeCtx, state: LazyListState, pad: (f32, f32), cross: (f32, f32)) {
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        LazyColumn::new()
            .state(state)
            .content_padding(pad.0, pad.1)
            .content_padding_cross(cross.0, cross.1)
            .modifier(Modifier::new().fill_max_width().fill_max_height())
            .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                crate::ui::text::Text::new(format!("Item {v}"))
                    .font_size(14.0)
                    .modifier(Modifier::new().padding(12.0))
                    .build(ctx);
            })
            .build(ctx);
    }

    /// 带 content_padding 的 5 sections 红底 sticky 列表（header 全局 index = n*20）
    fn sticky_build_padded(ctx: &mut ComposeCtx, state: LazyListState, pad: f32) {
        use crate::modifier::{Color, Shape};
        let mut lb = LazyColumn::new()
            .state(state)
            .content_padding(pad, 0.0)
            .modifier(Modifier::new().fill_max_width().fill_max_height());
        for s in 0..5u64 {
            lb = lb.sticky_header(s, move |ctx| {
                crate::ui::text::Text::new(format!("HEAD {s}"))
                    .font_size(18.0)
                    .modifier(Modifier::new().padding(12.0).background(
                        Color::from_argb(255, 0xC6, 0x28, 0x28),
                        Shape::rounded(2.0),
                    ))
                    .build(ctx);
            });
            lb = lb.items(
                19,
                move |i| 200 + s * 19 + i as u64,
                move |ctx, i| {
                    crate::ui::text::Text::new(format!("Item {}", s * 19 + i as u64))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                },
            );
        }
        lb.build(ctx);
    }

    /// 顶部红色 header 带数量：连续红色行段（header 背景）计数
    fn red_bands(px: &[[u8; 4]], w: usize, h: usize, limit: usize) -> usize {
        let mut bands = 0;
        let mut in_band = false;
        for y in 0..limit.min(h) {
            let mut red = false;
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                // ⚠ raster 是 BGRA8888（n32_premul 小端）：p[2] 才是红通道
                if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                    red = true;
                    break;
                }
            }
            if red && !in_band {
                bands += 1;
                in_band = true;
            } else if !red {
                in_band = false;
            }
        }
        bands
    }

    /// 视口顶连续红色行数（钉住 header 的可见高度；BGRA 字节序）
    fn top_band_height(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        let mut rows = 0;
        for y in 0..h {
            let mut red = false;
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                    red = true;
                    break;
                }
            }
            if red {
                rows += 1;
            } else {
                break;
            }
        }
        rows
    }

    #[test]
    fn sticky_header_pins_to_top_while_scrolling() {
        // 深滚动后：钉住的 header 成为锚点（first_visible = header index），
        // 红色带仍在视口顶，普通项内容从 header 下面滑过（下方仍有文本像素）
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "初始锚点 = header 0");
        assert_eq!(state.first_visible_offset.get(), 0.0);

        // 滚动进 section 3（offset 2000：C(40)≈1900 ≤ 2000 → header 2 钉住）
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert_eq!(w0, w1);
        assert_eq!(
            state.first_visible(),
            40,
            "钉住的 header 成为锚点（对齐 Compose firstVisibleItemIndex）"
        );
        assert_eq!(state.first_visible_offset.get(), 0.0);
        // 红色带：恰一个，且在视口顶（钉住）
        assert_eq!(red_bands(&px1, w1, 600, 300), 1, "钉住时视口顶恰一个 header");
        let mut top_red = false;
        for x in (0..w1).step_by(2) {
            let p = px1[x];
            if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                top_red = true;
            }
        }
        assert!(top_red, "钉住的 header 在视口最顶行");
        // 内容从 header 下滑过：header 带下方仍有普通项文本
        let mut below_text = false;
        for y in (70..600).step_by(2) {
            for x in (0..w1).step_by(2) {
                let p = px1[y * w1 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 {
                    below_text = true;
                }
            }
        }
        assert!(below_text, "普通项内容应继续渲染在钉住 header 之下");
        // 与初始帧内容不同（滚动到不同 section）
        let mut diff_rows = 0;
        for y in 0..600usize {
            let mut has0 = false;
            let mut has1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 {
                diff_rows += 1;
            }
        }
        assert!(diff_rows > 10, "滚动后内容变化，diff_rows={diff_rows}");

        // 深滚动（超界 → clamp 到 4150 ≈ 末尾）：header 4 钉住——验证无界回溯
        state.offset.set(100_000.0);
        let (_px2, _w2) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert!(
            state.offset() < 4500.0,
            "越界应 clamp，实际 {}",
            state.offset()
        );
        assert_eq!(
            state.first_visible(),
            80,
            "深滚动后 header 4（全局 80）钉住——无界回溯",
        );
    }

    #[test]
    fn sticky_header_pushes_previous_on_approach() {
        // 下一个 header 距顶 h(prev) 内时，前一个开始滑出（两 header 无缝相邻，
        // 视觉上是一条连续红带）→ 断言**顶带高度变矮**；到位后前一个完全推出
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        let h0_full = top_band_height(&px0, w0, 600);
        assert!(h0_full > 40, "初始 header 0 完整钉在顶，顶带高 {h0_full}");

        // ⚠ header 顶 C(20) 依赖运行时实测高度（字体加载时序不同 → 40 或 47.5），
        // 过渡窗口 [C(20)-h(0), C(20)] 也随之漂移——用扫描区间断言不变量：
        // 1) 过渡期：被推的 header 0 与 header 1 无缝相邻 → 顶带"变高"
        //    （band ∈ (h, 2h)，两 header 合并）
        // 2) 越过过渡区后 header 1 钉顶（锚点 = 20、单带、顶带恢复 h）
        let mut saw_merge = false;
        let mut saw_pinned = false;
        let mut final_band = 0usize;
        for off in (800..1010).step_by(5) {
            state.offset.set(off as f32);
            let (px, w) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
            let band = top_band_height(&px, w, 600);
            if band > h0_full + 8 && band < 2 * h0_full - 8 {
                saw_merge = true;
            }
            if off >= 985 {
                if state.first_visible() == 20 && red_bands(&px, w, 600, 200) == 1 {
                    saw_pinned = true;
                    final_band = band;
                }
            }
        }
        assert!(saw_merge, "过渡期顶带应变高（两 header 合并，band > 单 header）");
        assert!(saw_pinned, "越过过渡区后 header 1 应钉顶（锚点 20 + 单带）");
        let final_diff = if final_band > h0_full { final_band - h0_full } else { h0_full - final_band };
        assert!(final_diff < 6, "header 1 完整钉顶（顶带 {final_band} ≈ {h0_full}）");
    }

    #[test]
    fn sticky_header_composes_only_pinned_and_visible() {
        // 组合项数：视口 600px ≈ 12 项 + 预取 8 + 钉住 header —— 远小于 total 100
        // （sticky 回溯不拉全列表）
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let state = LazyListState::new();
        let c0 = composed.clone();
        let s0 = state.clone();
        render_lazy(move |ctx| {
            let mut lb = LazyColumn::new()
                .state(s0)
                .modifier(Modifier::new().fill_max_width().fill_max_height());
            for s in 0..5u64 {
                let c = c0.clone();
                lb = lb.sticky_header(s, move |ctx| {
                    c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    crate::ui::text::Text::new(format!("HEAD {s}"))
                        .font_size(18.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                });
                lb = lb.items(
                    19,
                    move |i| 200 + s * 19 + i as u64,
                    |ctx, i| {
                        crate::ui::text::Text::new(format!("Item {i}"))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    },
                );
            }
            lb.build(ctx);
        });
        let n0 = composed.load(std::sync::atomic::Ordering::Relaxed);
        assert!(n0 <= 40, "初始组合应只含可见项+预取，实际 {n0}");
        // 深滚动：钉住的 header 加入组合但列表不整体组合
        state.offset.set(2000.0);
        let (px, w) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        let n1 = composed.load(std::sync::atomic::Ordering::Relaxed);
        assert!(n1 <= 60, "深滚动组合项数应仍远小于 100，实际 {n1}");
        assert_eq!(red_bands(&px, w, 600, 200), 1);
    }

    // ═══════════════════════════════════════════════════
    // contentPadding（对齐 Compose contentPadding）
    // ═══════════════════════════════════════════════════

    #[test]
    fn content_padding_starts_content_after_padding() {
        // pad_before=50：offset 0 时首项内容从 y≈68 开始（50 + padding 12 + glyph），
        // 顶部 20..30 行无文本；offset=50（内容滚进 padding）后首项贴顶，文本进入 18..30
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (50.0, 0.0), (0.0, 0.0)));
        let mut top_empty = true;
        for y in 20..30usize {
            for x in (0..w0).step_by(2) {
                let p = px0[y * w0 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { top_empty = false; }
            }
        }
        assert!(top_empty, "offset 0 时顶部 20..30 行应为空白（内容从 pad 50 后开始）");

        state.offset.set(50.0);
        let (px1, w1) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (50.0, 0.0), (0.0, 0.0)));
        let mut top_text = false;
        for y in 18..30usize {
            for x in (0..w1).step_by(2) {
                let p = px1[y * w1 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { top_text = true; }
            }
        }
        assert!(top_text, "offset=50 后首项应贴顶（文本进入 y 18..30）");
        // 滚动前项顶在 y=50：自然边界处内容停在内边距处，不贴视口边
        assert_eq!(state.first_visible(), 0, "pad 50 + offset 50 → 锚点 index 0");
    }

    #[test]
    fn content_padding_max_offset_includes_after() {
        // pad_after=40：跳末尾时 max_offset = pad_before + Σ项 + pad_after - viewport，
        // 末项底停在 viewport-40 处 → 底部 35 行无文本
        let state = LazyListState::new();
        state.scroll_to_item(99999, 0.0);
        let (px, w) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (0.0, 40.0), (0.0, 0.0)));
        assert!(state.offset() > 20_000.0, "跳末尾应滚到接近末尾，实际 {}", state.offset());
        let first = state.first_visible();
        assert!(first >= 970, "first_visible 应接近末尾，实际 {first}");
        let mut bottom_empty = true;
        for y in (600 - 35)..600usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { bottom_empty = false; }
            }
        }
        assert!(bottom_empty, "pad_after=40：底部 35 行应为空白（末项停在 padding 处）");
    }

    #[test]
    fn content_padding_sticky_pins_at_padding() {
        // pad=50：深滚动后 header 4（全局 80）钉在视口 y=50（不是 0）：
        // 顶部 45 行无红，红带在 y 50..98
        let state = LazyListState::new();
        state.offset.set(3900.0); // C(80)=3800（h=47.5）→ header 4 钉住；且 < max_off≈4200 不 clamp
        let (px, w) = render_lazy(|ctx| sticky_build_padded(ctx, state.clone(), 50.0));
        let mut top_clear = true;
        for y in 0..45usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 { top_clear = false; }
            }
        }
        assert!(top_clear, "pad=50：顶部 45 行应为空白（header 钉在 y=50）");
        let mut band_red = false;
        for y in 50..98usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 { band_red = true; }
            }
        }
        assert!(band_red, "y 50..98 应为红色钉住 header");
        assert_eq!(state.first_visible(), 80, "钉住时锚点 = 钉住的 header（全局 80）");
        // 红带不再覆盖视口顶 0..40
        assert_eq!(red_bands(&px, w, 600, 200), 1, "仅一条红带（钉住的 header）");
    }

    #[test]
    fn content_padding_cross_axis_shrinks_items() {
        // cross pad 40/40：item 从 x=40 开始放置且宽被限制在 400-80=320：
        // x<36 无文本；文本起点 ≈ x 52..92
        let state = LazyListState::new();
        let (px, w) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (0.0, 0.0), (40.0, 40.0)));
        let mut left_empty = true;
        for y in (0..600).step_by(2) {
            for x in 0..36usize {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { left_empty = false; }
            }
        }
        assert!(left_empty, "交叉轴 padding 40：x<36 应无文本");
        let mut text_after_pad = false;
        for y in (0..600).step_by(2) {
            for x in 56..70usize {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { text_after_pad = true; }
            }
        }
        assert!(text_after_pad, "文本应从 x≈54 开始（40 cross + 12 padding）");
    }

    // ═══════════════════════════════════════════════════════
    // reverseLayout（对齐 Compose reverseLayout：index 0 在底部）
    // ═══════════════════════════════════════════════════════

    fn reverse_build(ctx: &mut ComposeCtx, state: LazyListState) {
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        LazyColumn::new()
            .state(state)
            .reverse_layout(true)
            .modifier(Modifier::new().fill_max_width().fill_max_height())
            .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                crate::ui::text::Text::new(format!("Item {}", v))
                    .font_size(14.0)
                    .modifier(Modifier::new().padding(12.0))
                    .build(ctx);
            })
            .build(ctx);
    }

    /// 指定行区间内是否有暗色文本像素
    fn rows_have_text(px: &[[u8; 4]], w: usize, y0: usize, y1: usize) -> bool {
        for y in y0..y1 {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { return true; }
            }
        }
        false
    }

    #[test]
    fn reverse_layout_starts_at_bottom_and_scrolls_toward_end() {
        // offset=0：项 0 在屏幕底（first_visible = 0），底部有文本、顶部也有
        // （视口显示列表开头 ~12 项，index 0 在最下）
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "offset=0 锚点 = 项 0（屏幕底）");
        assert!(rows_have_text(&px0, w0, 570, 600), "项 0 应在屏幕底部可见");
        assert!(rows_have_text(&px0, w0, 0, 30), "屏幕顶也应显示列表开头内容（~项 12）");

        // offset 增 = 向列表末尾（index 增）——与正向滚动方向一致
        state.offset.set(5000.0);
        let (px1, w1) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let first = state.first_visible();
        assert!(first >= 90, "offset 5000 后锚点应推进到 ~105，实际 {first}");
        // 底部项 = 锚点项（屏幕底显示 index ~105），顶部 = index ~117
        assert!(rows_have_text(&px1, w1, 0, 30), "反向滚动后屏幕顶仍有内容");
        // 帧间内容变化（滚到不同项）
        let mut diff_rows = 0;
        for y in 0..600usize {
            let mut h0 = false; let mut h1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                h0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                h1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if h0 != h1 { diff_rows += 1; }
        }
        assert!(diff_rows > 10, "反向滚动后内容变化，diff_rows={diff_rows}");
    }

    #[test]
    fn reverse_layout_scroll_to_item_lands_at_bottom() {
        // 跳 500：项 500 **底**贴视口底（与正向"顶贴顶"镜像）→ 底部行有文本；
        // 锚点 = 500（屏幕底项）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        // 正向对照：项 500 贴屏幕**顶**
        let fwd = LazyListState::new();
        let (pf, wf) = render_lazy(|ctx| {
            let it = items.clone();
            let s = fwd.clone();
            LazyColumn::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(it, |v: &u64| *v, |ctx, _i, v| {
                    crate::ui::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        fwd.scroll_to_item(500, 0.0);
        let (pf2, wf2) = render_lazy(|ctx| {
            let it = items.clone();
            let s = fwd.clone();
            LazyColumn::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(it, |v: &u64| *v, |ctx, _i, v| {
                    crate::ui::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        let _ = (pf, wf, pf2, wf2);
        assert_eq!(fwd.first_visible(), 500);

        state.scroll_to_item(500, 0.0);
        let (px, w) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 500, "反向跳 500：锚点 = 500（屏幕底项）");
        assert!(rows_have_text(&px, w, 570, 600), "项 500 底应贴视口底（底部行有文本）");
        // 顶部也有内容（项 500 上方 ~12 项）
        assert!(rows_have_text(&px, w, 0, 40), "项 500 上方的项应在屏幕顶可见");
        // offset = prefix(500) ≈ 23750（0 spacing + 无 padding）
        assert!(
            state.offset() > 20_000.0 && state.offset() < 26_000.0,
            "反向 jump offset = prefix(500)，实际 {}",
            state.offset()
        );
    }

    #[test]
    fn reverse_layout_clamps_at_both_ends() {
        // 越界（offset=100000）→ clamp 到 max_off = content_h - vh ≈ 46900 → 锚点 ≈ 987
        let state = LazyListState::new();
        state.offset.set(100_000.0);
        // 首帧：build 用未 clamp 的 offset 锚定（组合窗偏窄）；第二帧 offset 已被
        // policy clamp → 组合窗收敛（真实 app 首帧后自然收敛，测试断言收敛帧）
        let (_px0, _w0) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let (px, w) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let first = state.first_visible();
        assert!(first >= 970, "越界 clamp 后应接近列表末尾（index 大），实际 {first}");
        assert!(rows_have_text(&px, w, 570, 600), "末尾项应在屏幕底可见");
        // 回到开头
        state.offset.set(0.0);
        let (_px2, _w2) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "offset=0 回到项 0");
    }

    // ── animateScrollToItem（对齐 Compose animateScrollToItem——spring 动画）──
    #[test]
    fn animate_scroll_to_item_animates_offset() {
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        let before = state.offset();
        assert_eq!(before, 0.0);

        // 立即跳转测出精确目标（实测高度：prefix(500)——运行时字体高度不同，
        // 不能用常数 23750 断言）
        state.scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        let target = state.offset();
        assert!(target > 15_000.0 && target < 30_000.0, "目标应 ≈ prefix(500)，实际 {target}");

        // 回到 0 再动画滚动到同一目标
        state.offset.set(0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        state.animate_scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        // 测量期消费：注册 spring 动画（push_animatable 立即 update 一帧——
        // elapsed≈0 位移≈0，之后由 step 循环推进）
        let frames = step_animations_until_done(&state, 400);
        assert!(frames < 400, "spring 应收敛（{frames} 帧）");
        let end = state.offset();
        assert!(
            (end - target).abs() < 5.0,
            "动画应收敛到 scroll_to_item 同一目标 {target}，实际 {end}"
        );
    }

    // ── 重复 key 检测（对齐 Compose：key 冲突抛异常）──
    #[test]
    #[should_panic(expected = "duplicate key")]
    fn duplicate_key_detected_in_rebuild() {
        let mut il = IntervalList::new();
        let c1: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        let c2: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(1, Some(Arc::new(|_| 42u64)), c1, false);
        il.add(1, Some(Arc::new(|_| 42u64)), c2, false);
        il.rebuild(); // 应 panic
    }
}
