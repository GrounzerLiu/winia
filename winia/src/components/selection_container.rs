//! SelectionContainer — 文本选中容器（对齐 Jetpack Compose）

use crate::runtime::composition_local::CompositionLocal;
use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::runtime::composer::GroupStatus;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::Mutex;



use crate::text::selection::{LOCAL_SELECTION_REGISTRAR, Selection, SelectionRegistrar};
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

    /// 注册选区变化回调。参数为 `Selection`——含选中文本（框架按注册段自动
    /// 拼接，用户无需自维护平行字符串）。
    ///
    /// 语义：仅在实际产生**非零宽**选择（拖动）时触发——单击（零宽）不触发，
    /// 因此"清空选择"没有通知路径（如需在单击后刷新 UI，可在 content 外层
    /// 自行处理）。
    pub fn on_selection_change(mut self, f: impl Fn(&Selection) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // 持久化同一个 Registrar（重组时不新建，segments 跨重组保留）
        let registrar = ctx.remember_at_key(key, || SelectionRegistrar::new()).get();
        if let Some(cb) = self.on_change {
            registrar.set_on_change(cb);
        }
        {
            let reg = registrar.clone();
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

    #[test]
    fn test_new_registrar_no_selection() {
        let reg = SelectionRegistrar::new();
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_local_selection_registrar_scopes_to_provides() {
        assert!(LOCAL_SELECTION_REGISTRAR.try_current().is_none());
        let reg = SelectionRegistrar::new();
        LOCAL_SELECTION_REGISTRAR.provides(reg.clone(), || {
            let current = LOCAL_SELECTION_REGISTRAR.try_current().expect("registrar should be visible inside provides");
            assert!(current.is_same(&reg));
        });
        assert!(LOCAL_SELECTION_REGISTRAR.try_current().is_none());
    }

    #[test]
    fn test_register_and_set_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "hello world");
        reg.set_selection(5, 15);
        // "hello world" 仅 11 字节——clamp 到 11
        assert_eq!(reg.selected_range(42), Some(5..11));
    }

    #[test]
    fn test_cross_text_merged() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");
        reg.register(2, "klmnopqrstuvwxy");
        reg.set_selection(5, 20);
        assert_eq!(reg.selected_range(1), Some(5..10));
        assert_eq!(reg.selected_range(2), Some(0..10));
        assert!(reg.selected_range(99).is_none());
    }

    #[test]
    fn test_partial_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghijklmnopqrst");
        reg.set_selection(15, 30); // extends beyond text
        assert_eq!(reg.selected_range(1), Some(15..20)); // clamped
    }

    #[test]
    fn test_selection_outside() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "abcdefghij");
        reg.set_selection(20, 30);
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clear() {
        let reg = SelectionRegistrar::new();
        reg.register(42, "hello world");
        reg.set_selection(5, 15);
        reg.clear_selection();
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clone_shared() {
        let reg1 = SelectionRegistrar::new();
        let reg2 = reg1.clone();
        reg1.register(42, "abcdefghij");
        reg1.set_selection(2, 8);
        assert_eq!(reg2.selected_range(42), Some(2..8));
    }

    #[test]
    fn test_slot_dedup() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "hello");   // offset=0, next=5
        reg.register(1, "hello world");  // same slot, NOT new → offset stays 0, next=max(5,0+11)=11
        assert_eq!(reg.total_text_len(), 11);
        assert_eq!(reg.segment_info(1), Some((0, 11))); // preserves original offset, latest len
    }

    #[test]
    fn test_total_text_len_accumulation() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");  // offset=0, len=10, total=10
        reg.register(2, "klmnopqrstuvwxy");  // offset=10, len=15, total=25
        reg.register(3, "12345");   // offset=25, len=5, total=30
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
        reg.register(1, "abcdefghij");
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
        reg.register(1, "abcdefghij");
        reg.register(2, "klmnopqrstuvwxy");
        assert_eq!(reg.total_text_len(), 25);
        reg.reset_offsets();
        assert_eq!(reg.total_text_len(), 0);
        // re-register starts from offset 0
        reg.register(3, "12345");
        assert_eq!(reg.segment_info(3), Some((0, 5)));
    }

    #[test]
    fn test_selection_across_three_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(10, &"x".repeat(100));  // offset=0
        reg.register(20, &"y".repeat(200));  // offset=100
        reg.register(30, &"z".repeat(50));   // offset=300
        // select spanning middle of 1st to middle of 3rd
        reg.set_selection(50, 320);
        assert_eq!(reg.selected_range(10), Some(50..100));   // local 50..100
        assert_eq!(reg.selected_range(20), Some(0..200));     // full second
        assert_eq!(reg.selected_range(30), Some(0..20));      // first 20 of third
    }

    #[test]
    fn test_reversed_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghijklmnopqrst");
        reg.set_selection(15, 5); // reversed: start > end
        assert_eq!(reg.selected_range(1), Some(5..15)); // normalized
    }

    #[test]
    fn test_selection_exactly_at_boundary() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");
        reg.register(2, "klmnopqrst"); // offset=10
        // selection at exact boundary
        reg.set_selection(10, 10); // zero-width
        assert!(reg.selected_range(1).is_none()); // local_end == 0
        assert!(reg.selected_range(2).is_none()); // local_start == 0, local_end == 0
    }

    #[test]
    fn test_selection_single_char_last_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "abcdefghij");  // offset=0
        reg.register(2, "12345");   // offset=10
        reg.set_selection(13, 14);  // chars 13-14 in global = 3-4 in seg2
        assert_eq!(reg.selected_range(2), Some(3..4));
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_build_selection_single_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "Hello, world!");
        reg.set_selection(7, 12);
        let sel = reg.build_selection().unwrap();
        assert_eq!(sel.start(), 7);
        assert_eq!(sel.end(), 12);
        assert_eq!(sel.text(), "world");
    }

    #[test]
    fn test_build_selection_cross_segments() {
        let reg = SelectionRegistrar::new();
        reg.register(1, "Hello! ");        // offset=0
        reg.register(2, "Drag me ");       // offset=7
        reg.register(3, "finish.");        // offset=15
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
        reg.register(1, "Hi 👋 world");
        // 选择 "👋 wor"（字节 3..11：H=0 i=1 sp=2 👋=3..7 sp=7 w=8 o=9 r=10 l=11）
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
        reg.register(1, "abcdef");
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
        a.register(1, &"x".repeat(100)); // total=100
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
        a.register(1, &"x".repeat(100)); // anchor 容器 total=100
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
