//! Column / Row / Box 布局 composable
//!
//! 这些是用户面组件，内部使用 layout 模块的 MeasurePolicy

use crate::core::composer::ComposeCtx;
use crate::layout::{Arrangement, Alignment, ColumnLayout, RowLayout, BoxLayout};
use crate::modifier::Modifier;

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

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（阶段 5：参数相等跳过——下帧 is_skip 比较 slot.params：
        // spacing/arrangement/alignment 未变 → 容器 Skip（content 不重跑）；
        // 变化 → Enter（重跑——修复"参数变化仍 Skip 用旧值"的缺口）
        ctx.changed(&self.spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        let key = ctx.next_key();
        let dir = crate::ui::theme::WiniaTheme::direction();
        let policy = ColumnLayout::new()
            .arrangement(self.arrangement)
            .alignment(self.alignment)
            .spacing(self.spacing)
            .direction(dir);
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                content(ctx);
            }
        }
        ctx.end_restartable_group();
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
        let key = ctx.next_key();
        let dir = crate::ui::theme::WiniaTheme::direction();
        let policy = RowLayout::new()
            .arrangement(self.arrangement)
            .alignment(self.alignment)
            .spacing(self.spacing)
            .direction(dir);
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                content(ctx);
            }
        }
        ctx.end_restartable_group();
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
        let key = ctx.next_key();
        let policy = BoxLayout::new().alignment(self.alignment);
        // content 闭包自动成为组合 scope（与 Column 一致）
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                content(ctx);
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for Stack {
    fn default() -> Self { Self::new() }
}
