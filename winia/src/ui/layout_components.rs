//! Column / Row / Box / Spacer 布局 composable
//!
//! 这些是用户面组件，内部使用 layout 模块的 MeasurePolicy

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::{Arrangement, Alignment, ColumnLayout, RowLayout, BoxLayout, FlowRowLayout, FlowColumnLayout, MeasurePolicy};
use crate::modifier::Modifier;

/// 容器 build 样板合并（P2-4）：Column/Row/Stack 共用——
/// start_restartable_group 的 Skip/Enter 分支 + end 配对收拢一处。
/// 参数暂存（changed）由各组件在自己 build 里做（参数集不同）。
fn build_container(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    policy: impl MeasurePolicy + 'static,
    content: impl FnOnce(&mut ComposeCtx),
) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, policy) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => { content(ctx); }
    }
    ctx.end_restartable_group();
}

// ── Column ──

pub struct Column {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
}

impl Column {
    pub fn new() -> Self {
        Column {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }

    /// ⚠ 不宏化：宏化引入内部 scope 会拦截 content 顶层 State.get() 的依赖注册
    /// （content 依赖注册到内部 scope → 父容器感知不到 → 内容不重跑 → 联动断）
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（阶段 5：参数相等跳过——下帧 is_skip 比较 slot.params：
        // spacing/arrangement/alignment 未变 → 容器 Skip（content 不重跑）；
        // 变化 → Enter（重跑——修复"参数变化仍 Skip 用旧值"的缺口）
        ctx.changed(&self.spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        // 方向：modifier 覆盖（Modifier::layout_direction）> CompositionLocal 默认
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::ui::theme::WiniaTheme::direction());
        build_container(
            ctx,
            self.modifier,
            ColumnLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .spacing(self.spacing)
                .direction(dir),
            content,
        );
    }
}

impl Default for Column {
    fn default() -> Self { Self::new() }
}

// ── Row ──

pub struct Row {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
}

impl Row {
    pub fn new() -> Self {
        Row {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（参数相等跳过——同 Column）
        ctx.changed(&self.spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        // 方向参与 changed（同 Column——方向切换必须 Enter 替换 policy）
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::ui::theme::WiniaTheme::direction());
        ctx.changed(&dir);
        build_container(
            ctx,
            self.modifier,
            RowLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .spacing(self.spacing)
                .direction(dir),
            content,
        );
    }
}

impl Default for Row {
    fn default() -> Self { Self::new() }
}

// ── Stack (层叠布局，类似 Compose Box) ──

pub struct Stack {
    modifier: Modifier,
    alignment: Alignment,
}

impl Stack {
    pub fn new() -> Self {
        Stack {
            modifier: Modifier::new(),
            alignment: Alignment::Start,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（参数相等跳过——同 Column）
        ctx.changed(&self.alignment);
        // content 闭包自动成为组合 scope（与 Column 一致）
        build_container(ctx, self.modifier, BoxLayout::new().alignment(self.alignment), content);
    }
}

impl Default for Stack {
    fn default() -> Self { Self::new() }
}

// ── FlowRow / FlowColumn（流式布局，对标 Compose foundation）──

/// 流式行：主轴填满即换行（chip 组/标签云/工具栏换行）。
///
/// 对标 `FlowRow(horizontalArrangement/verticalArrangement/maxItemsInEachRow)`：
/// `main_spacing` = 行内主轴间距，`cross_spacing` = 行间距，
/// `max_items_in_row` = 每行上限（默认不限）。
/// v1 无 weight/maxLines/overflow（见 `layout/flow.rs` 文档）。
pub struct FlowRow {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_row: usize,
}

impl FlowRow {
    pub fn new() -> Self {
        FlowRow {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_row: usize::MAX,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_row(mut self, m: usize) -> Self { self.max_items_in_row = m; self }

    /// ⚠ 不宏化（同 Row/Column：content 顶层 State.get() 需冒泡给父）。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.main_spacing);
        ctx.changed(&self.cross_spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        // max_items_in_row: usize 有 PartialEq，直接 changed
        ctx.changed(&self.max_items_in_row);
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::ui::theme::WiniaTheme::direction());
        ctx.changed(&dir);
        build_container(
            ctx,
            self.modifier,
            FlowRowLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .main_spacing(self.main_spacing)
                .cross_spacing(self.cross_spacing)
                .max_items_in_row(self.max_items_in_row)
                .direction(dir),
            content,
        );
    }
}

impl Default for FlowRow {
    fn default() -> Self { Self::new() }
}

/// 流式列：主轴（垂直）填满即换列。对标 `FlowColumn`，参数与 FlowRow 对称。
pub struct FlowColumn {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_column: usize,
}

impl FlowColumn {
    pub fn new() -> Self {
        FlowColumn {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_column: usize::MAX,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_column(mut self, m: usize) -> Self { self.max_items_in_column = m; self }

    /// ⚠ 不宏化（同 Row/Column）。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.main_spacing);
        ctx.changed(&self.cross_spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        ctx.changed(&self.max_items_in_column);
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::ui::theme::WiniaTheme::direction());
        ctx.changed(&dir);
        build_container(
            ctx,
            self.modifier,
            FlowColumnLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .main_spacing(self.main_spacing)
                .cross_spacing(self.cross_spacing)
                .max_items_in_column(self.max_items_in_column)
                .direction(dir),
            content,
        );
    }
}

impl Default for FlowColumn {
    fn default() -> Self { Self::new() }
}

// ── Spacer ──

/// 固定尺寸空白占位（对标 Compose `Spacer(modifier)`）。
///
/// 用于列/行间间距（`.spacing()` 之外的显式间隔）或布局占位。
/// 仅尺寸、无绘制、无子节点。
pub struct Spacer {
    modifier: Modifier,
}

impl Spacer {
    /// 垂直空白：固定高度 `h`、宽 0——用于 Column 内垂直间隔
    pub fn vertical(h: f32) -> Self {
        Spacer { modifier: Modifier::new().size(0.0, h) }
    }

    /// 水平空白：固定宽度 `w`、高 0——用于 Row 内水平间隔
    pub fn horizontal(w: f32) -> Self {
        Spacer { modifier: Modifier::new().size(w, 0.0) }
    }

    /// 自定义尺寸/修饰符（如 `.modifier(Modifier::size(10.0, 10.0))`）
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx) {
        // 空容器：BoxLayout 无子节点 → 仅占位尺寸
        build_container(ctx, self.modifier, BoxLayout::new(), |_| {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::Constraints;
    use crate::ui::theme::{ThemeColors, WiniaTheme};

    /// FlowRow 组合链路集成：content 闭包 4 个固定叶，容器宽 100 → 换行。
    #[test]
    fn flow_row_composes_and_wraps() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                FlowRow::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for _ in 0..4 {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, Modifier::new().size(30.0, 20.0));
                            ctx.end_node();
                        }
                    });
            });
        });
        composer.layout(Constraints::new(100.0, 100.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // 递归找 4 子节点（FlowRow 容器——WiniaTheme scope 无节点，不假设层级）
        fn find_4(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<usize> {
            if nodes[idx].children.len() == 4 { return Some(idx); }
            for &c in &nodes[idx].children {
                if let Some(f) = find_4(nodes, c) { return Some(f); }
            }
            None
        }
        let flow_idx = find_4(nodes, root).expect("应有 4 子 FlowRow 节点");
        let ys: Vec<f32> = nodes[flow_idx].children.iter()
            .map(|&c| nodes[c].position.y)
            .collect();
        assert_eq!(ys, vec![0.0, 0.0, 0.0, 20.0], "前 3 个首行，第 4 个换行，ys={ys:?}");
    }

    /// FlowColumn 组合链路集成：容器高 100 → 换列。
    #[test]
    fn flow_column_composes_and_wraps() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                FlowColumn::new()
                    .modifier(Modifier::new().fill_max_height())
                    .build(ctx, |ctx| {
                        for _ in 0..4 {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, Modifier::new().size(20.0, 30.0));
                            ctx.end_node();
                        }
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 600.0, 100.0, 100.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        fn find_4(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<usize> {
            if nodes[idx].children.len() == 4 { return Some(idx); }
            for &c in &nodes[idx].children {
                if let Some(f) = find_4(nodes, c) { return Some(f); }
            }
            None
        }
        let flow_idx = find_4(nodes, root).expect("应有 4 子 FlowColumn 节点");
        let xs: Vec<f32> = nodes[flow_idx].children.iter()
            .map(|&c| nodes[c].position.x)
            .collect();
        assert_eq!(xs, vec![0.0, 0.0, 0.0, 20.0], "前 3 个首列，第 4 个换列，xs={xs:?}");
    }
}
