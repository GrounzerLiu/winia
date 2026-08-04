//! SelectionContainer — 文本选中容器（对齐 Jetpack Compose）

use crate::core::composition_local::CompositionLocal;
use crate::core::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::core::composer::GroupStatus;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::Mutex;


// ═══════════════════════════════════════════════════════════
// 辅助类型
// ═══════════════════════════════════════════════════════════

/// 简单的 2D 矩形（对齐 Compose Rect）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
}

/// 注册的文本段信息
#[derive(Debug, Clone)]
pub(crate) struct RegisteredSegment {
    pub slot_key: u64,
    pub global_offset: usize,
    pub text_len: usize,
    pub bounds: Rect,
    /// 段实际文本（注册时保存——selected_text 拼接用，用户无需自维护平行字符串）
    pub text: Arc<str>,
}

// ═══════════════════════════════════════════════════════════
// SelectionRegistrar
// ═══════════════════════════════════════════════════════════

type OnChangeFn = Arc<dyn Fn(&Selection) + Send + Sync>;

/// 选择结果——回调参数（对标 Compose `Selection`：自带选中文本）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    start: usize,
    end: usize,
    text: String,
}

impl Selection {
    /// 容器全局起始偏移（含容器内所有已注册段）
    pub fn start(&self) -> usize { self.start }
    /// 容器全局结束偏移（含容器内所有已注册段）
    pub fn end(&self) -> usize { self.end }
    /// 选中的文本（框架按注册段自动拼接——含跨段/emoji 边界安全）
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

    pub fn register(&self, slot_key: u64, text: &str, bounds: Option<Rect>) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let text_len = text.len();
        let offset = if let Some(existing) = inner.segments.get(&slot_key) {
            existing.global_offset
        } else {
            let off = inner.next_global_offset;
            inner.next_global_offset += text_len;
            off
        };
        inner.segments.insert(slot_key, RegisteredSegment {
            slot_key, global_offset: offset, text_len, bounds: bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            text: text.into(),
        });
        offset
    }

    pub fn set_selection(&self, start: usize, end: usize) {
        let mut inner = self.inner.lock().unwrap();
        inner.selection_start = Some(start.min(end));
        inner.selection_end = Some(start.max(end));
    }

    pub fn clear_selection(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.selection_start = None;
        inner.selection_end = None;
    }

    /// 是否同一实例（Arc 身份——跨容器拖动的 anchor 归属判断：拖到别的
    /// SelectionContainer 的文本上时，用 anchor 容器做 edge snap，不切偏移空间）
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn segment_info(&self, slot_key: u64) -> Option<(usize, usize)> {
        let inner = self.inner.lock().unwrap();
        inner.segments.get(&slot_key).map(|s| (s.global_offset, s.text_len))
    }

    pub(crate) fn reset_offsets(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.next_global_offset = 0;
        inner.segments.clear();
    }

    pub fn total_text_len(&self) -> usize {
        self.inner.lock().unwrap().next_global_offset
    }

    pub fn selected_range(&self, slot_key: u64) -> Option<Range<usize>> {
        let inner = self.inner.lock().unwrap();
        let (global_start, global_end) = (inner.selection_start?, inner.selection_end?);
        let seg = inner.segments.get(&slot_key)?;
        let local_start = global_start.saturating_sub(seg.global_offset);
        let local_end = global_end.saturating_sub(seg.global_offset);
        if local_start >= seg.text_len || local_end == 0 { return None; }
        let len = seg.text_len;
        Some(local_start.min(len)..local_end.min(len))
    }

    /// 设置选区变化回调（参数为 Selection——含选中文本，用户无需自维护平行字符串）
    pub fn set_on_change(&self, f: impl Fn(&Selection) + Send + Sync + 'static) {
        self.inner.lock().unwrap().on_change = Some(Arc::new(f));
    }

    /// 构造当前选择的 Selection（按注册段自动拼接文本——跨段/emoji 边界安全）
    pub(crate) fn build_selection(&self) -> Option<Selection> {
        let inner = self.inner.lock().unwrap();
        let (s, e) = (inner.selection_start?, inner.selection_end?);
        let (s, e) = (s.min(e), s.max(e));
        // 零宽（点击未拖动）→ 无选择（与 selected_range 一致）
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
            // 防御：偏移理论上是字符边界（get_closest 保证），但跨段拼接时
            // 用 get 避免任何越界/非边界 panic（异常时跳过该段）
            if let Some(part) = seg.text.get(start..end) {
                text.push_str(part);
            }
        }
        Some(Selection { start: s, end: e, text })
    }

    pub(crate) fn fire_on_change(&self) {
        let cb = self.inner.lock().unwrap().on_change.clone();
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
// LOCAL_SELECTION_REGISTRAR + ACTIVE_REGISTRAR
// ═══════════════════════════════════════════════════════════

pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));

static ACTIVE_REGISTRAR: std::sync::LazyLock<Mutex<Option<SelectionRegistrar>>> = std::sync::LazyLock::new(|| Mutex::new(None));

pub(crate) fn active_registrar() -> SelectionRegistrar {
    ACTIVE_REGISTRAR.lock().unwrap().clone().unwrap_or_else(|| SelectionRegistrar::new())
}
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

    pub fn on_selection_change(mut self, f: impl Fn(&Selection) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // 持久化同一个 Registrar（重组时不新建，segments 跨重组保留）
        let registrar = ctx.remember_at_key(key, || SelectionRegistrar::new()).get();
        if let Some(cb) = self.on_change {
            registrar.set_on_change(cb);
        }
        {
            let reg = registrar.clone();
            *ACTIVE_REGISTRAR.lock().unwrap() = Some(reg.clone());
            LOCAL_SELECTION_REGISTRAR.provides(reg, || {
                ctx.set_selection_registrar(registrar.clone());
                // content 闭包自动成为组合 scope（与 Column/Row/Stack/Button 一致）
                match ctx.start_restartable_group(key, self.modifier, BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        registrar.reset_offsets();
                        content(ctx);
                    }
                }
                ctx.end_restartable_group();
            });
            // provides 退出后恢复 composer 的 selection_registrar（防残留——
            // build 之后的 Text 会误注册到本 SelectionContainer，如"显示选中文本"的输出 Text）
            ctx.clear_selection_registrar();
        }
    }
}

impl Default for SelectionContainer {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// 拖动选择核心（真实 PointerMoved 与 debug 模拟共用——消除平行代码 + 可测）
// ═══════════════════════════════════════════════════════════

/// 计算一次 PointerMove 后的选择范围。
///
/// 返回 `Some((要设选的 registrar, 全局 start, 全局 end))`；`None` = 不更新选择。
/// 三种情况：
/// - 同容器 + 当前文本内：`anchor_global ↔ current_global`（当前文本的全局偏移）
/// - 同容器 + 容器空白：edge snap 到当前容器边界（上拖 0 / 下拖 total）
/// - 跨容器（当前在别的 SelectionContainer 文本上）：用 anchor 容器 edge snap——
///   绝不切到当前容器的偏移空间（否则 anchor 全局偏移与当前容器 global_off 混合 → 错选）
/// - 无 anchor 容器（Down 在不可选节点上，如 SelectionContainer 外的输出 Text）→ None
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
            // 同容器超出：edge snap 到容器边界
            let edge = if scene_y < node_abs_y { 0 } else { cur_reg.total_text_len() };
            let s = a.min(edge);
            let e = a.max(edge);
            Some((cur_reg.clone(), s, e))
        } else {
            None
        }
    } else if let (Some(anchor_reg), Some(a)) = (anchor_reg, anchor_global) {
        // 跨容器：用 anchor 容器做 edge snap（拖出 anchor 容器 → clamp 到其边界）
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
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: f32, t: f32, r: f32, b: f32) -> Option<Rect> {
        Some(Rect::new(l, t, r, b))
    }

    #[test]
    fn test_new_registrar_no_selection() {
        let reg = SelectionRegistrar::new();
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_register_and_set_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "hello world", rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(5, 15);
        // "hello world" 仅 11 字节——clamp 到 11
        assert_eq!(reg.selected_range(42), Some(5..11));
    }

    #[test]
    fn test_cross_text_merged() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij", rect(0.0, 0.0, 100.0, 20.0));
        reg.register(2, "klmnopqrstuvwxy", rect(0.0, 0.0, 150.0, 20.0));
        reg.set_selection(5, 20);
        assert_eq!(reg.selected_range(1), Some(5..10));
        assert_eq!(reg.selected_range(2), Some(0..10));
        assert!(reg.selected_range(99).is_none());
    }

    #[test]
    fn test_partial_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghijklmnopqrst", None);
        reg.set_selection(15, 30); // extends beyond text
        assert_eq!(reg.selected_range(1), Some(15..20)); // clamped
    }

    #[test]
    fn test_selection_outside() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "abcdefghij", rect(0.0, 0.0, 100.0, 20.0));
        reg.set_selection(20, 30);
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clear() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "hello world", rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(5, 15);
        reg.clear_selection();
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clone_shared() {
        let reg1 = SelectionRegistrar::new();
        let reg2 = reg1.clone();
        reg1.register(42, "abcdefghij", rect(0.0, 0.0, 100.0, 20.0));
        reg1.set_selection(2, 8);
        assert_eq!(reg2.selected_range(42), Some(2..8));
    }

    #[test]
    fn test_slot_dedup() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "hello", None);   // offset=0, next=5
        reg.register(1, "hello world", None);  // same slot, NOT new → offset stays 0, next stays 5
        assert_eq!(reg.total_text_len(), 5);
        assert_eq!(reg.segment_info(1), Some((0, 11))); // preserves original offset, latest len
    }

    #[test]
    fn test_total_text_len_accumulation() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij", None);  // offset=0, len=10, total=10
        reg.register(2, "klmnopqrstuvwxy", None);  // offset=10, len=15, total=25
        reg.register(3, "12345", None);   // offset=25, len=5, total=30
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
        reg.register(1, "abcdefghij", None);
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

    #[test]
    fn test_reset_offsets_clears_and_restarts() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij", None);
        reg.register(2, "klmnopqrstuvwxy", None);
        assert_eq!(reg.total_text_len(), 25);
        reg.reset_offsets();
        assert_eq!(reg.total_text_len(), 0);
        // re-register starts from offset 0
        reg.register(3, "12345", None);
        assert_eq!(reg.segment_info(3), Some((0, 5)));
    }

    #[test]
    fn test_selection_across_three_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(10, &"x".repeat(100), None);  // offset=0
        reg.register(20, &"y".repeat(200), None);  // offset=100
        reg.register(30, &"z".repeat(50), None);   // offset=300
        // select spanning middle of 1st to middle of 3rd
        reg.set_selection(50, 320);
        assert_eq!(reg.selected_range(10), Some(50..100));   // local 50..100
        assert_eq!(reg.selected_range(20), Some(0..200));     // full second
        assert_eq!(reg.selected_range(30), Some(0..20));      // first 20 of third
    }

    #[test]
    fn test_reversed_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghijklmnopqrst", None);
        reg.set_selection(15, 5); // reversed: start > end
        assert_eq!(reg.selected_range(1), Some(5..15)); // normalized
    }

    #[test]
    fn test_selection_exactly_at_boundary() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij", None);
        reg.register(2, "klmnopqrst", None); // offset=10
        // selection at exact boundary
        reg.set_selection(10, 10); // zero-width
        assert!(reg.selected_range(1).is_none()); // local_end == 0
        assert!(reg.selected_range(2).is_none()); // local_start == 0, local_end == 0
    }

    #[test]
    fn test_selection_single_char_last_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij", None);  // offset=0
        reg.register(2, "12345", None);   // offset=10
        reg.set_selection(13, 14);  // chars 13-14 in global = 3-4 in seg2
        assert_eq!(reg.selected_range(2), Some(3..4));
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_build_selection_single_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "Hello, world!", None);
        reg.set_selection(7, 12);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.start(), 7);
        assert_eq!(sel.end(), 12);
        assert_eq!(sel.text(), "world");
    }

    #[test]
    fn test_build_selection_cross_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "Hello! ", None);        // offset=0
        reg.register(2, "Drag me ", None);       // offset=7
        reg.register(3, "finish.", None);        // offset=15
        // 跨三段：第 1 段尾部 + 第 2 段全部 + 第 3 段头部
        reg.set_selection(4, 19);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.text(), "o! Drag me fini");
        // 反向选择归一化
        reg.set_selection(19, 4);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.text(), "o! Drag me fini");
    }

    #[test]
    fn test_build_selection_emoji_boundary() {
        let reg = SelectionRegistrar::new();
        // emoji 👋（4 字节）在段中间——偏移必须落在字符边界才切片成功
        reg.register(1, "Hi 👋 world", None);
        // 选择 "👋 wo"（字节 3..11：H=0 i=1 sp=2 👋=3..7 sp=7 w=8 o=9 r=10 l=11）
        reg.set_selection(3, 11);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.text(), "👋 wor");
        // 空选择（零宽）→ None
        reg.set_selection(5, 5);
        assert!(reg.build_selection().is_none());
    }

    #[test]
    fn test_build_selection_reversed() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdef", None);
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
        // 同容器文本内：anchor 10 → current 20 → (10, 20)
        let r = compute_selection(Some(&a), Some(10), &a, Some(0), 20, 100.0, 50.0, 50.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a));
        assert_eq!((s, e), (10, 20));
    }

    #[test]
    fn test_same_container_edge_snap_down() {
        let (a, _) = regs();
        a.register(1, &"x".repeat(100), None); // total=100
        // 同容器超出（向下拖出节点）：edge = total = 100
        let r = compute_selection(Some(&a), Some(5), &a, None, 0, 300.0, 50.0, 100.0);
        let (_, s, e) = r.unwrap();
        assert_eq!((s, e), (5, 100));
    }

    #[test]
    fn test_same_container_edge_snap_up() {
        let (a, _) = regs();
        // 同容器超出（向上拖出节点）：edge = 0
        let r = compute_selection(Some(&a), Some(5), &a, None, 0, 10.0, 50.0, 100.0);
        let (_, s, e) = r.unwrap();
        assert_eq!((s, e), (0, 5));
    }

    #[test]
    fn test_cross_container_uses_anchor_reg() {
        let (a, b) = regs();
        a.register(1, &"x".repeat(100), None); // anchor 容器 total=100
        // 跨容器：anchor 在 a，当前在 b 的文本上 → 用 a 做 edge snap（不切 b 的偏移空间）
        let r = compute_selection(Some(&a), Some(10), &b, Some(0), 5, 200.0, 50.0, 50.0);
        let (reg, s, e) = r.unwrap();
        assert!(reg.is_same(&a), "应设 anchor 容器 a 的选择");
        assert_eq!((s, e), (10, 100)); // 向下拖出 a → clamp 到 a 末尾
    }

    #[test]
    fn test_no_anchor_container_returns_none() {
        let (_, b) = regs();
        // Down 在不可选节点（anchor_reg None）→ 拖动不选
        let r = compute_selection(None, None, &b, Some(0), 5, 200.0, 50.0, 50.0);
        assert!(r.is_none());
    }

    #[test]
    fn test_cross_container_no_anchor_global_none() {
        let (a, b) = regs();
        // 跨容器但无 anchor_global（不可选节点按下但 anchor_reg 残留？——守卫）→ None
        let r = compute_selection(Some(&a), None, &b, Some(0), 5, 200.0, 50.0, 50.0);
        assert!(r.is_none());
    }
}
