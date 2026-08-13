//! Column / Row / Box 布局 composable
//!
//! 这些是用户面组件，内部使用 layout 模块的 MeasurePolicy

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::{Arrangement, Alignment, ColumnLayout, RowLayout, BoxLayout, MeasurePolicy};
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
