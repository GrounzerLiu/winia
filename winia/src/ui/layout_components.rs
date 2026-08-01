//! Column / Row / Box 布局 composable
//!
//! 这些是用户面组件，内部使用 layout 模块的 MeasurePolicy

use crate::core::composer::ComposeCtx;
use crate::layout::{Arrangement, Alignment, ColumnLayout, RowLayout, BoxLayout};
use crate::modifier::Modifier;

// ── Column ──

pub struct Column {
    modifier: Modifier,
    modifier_fn: Option<Box<dyn Fn() -> Modifier + Send + Sync>>,
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
}

impl Column {
    pub fn new() -> Self {
        Column {
            modifier: Modifier::new(),
            modifier_fn: None,
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }

    /// 延迟求值的 modifier（研究用）：build 内 start 节点后求值，
    /// `State::get()` 注册依赖到本节点 → 动画值变化 → 本节点 Enter → 重算 modifier
    pub fn modifier_fn(mut self, f: impl Fn() -> Modifier + Send + Sync + 'static) -> Self {
        self.modifier_fn = Some(Box::new(f));
        self
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let dir = crate::ui::theme::WiniaTheme::direction();
        let policy = ColumnLayout::new()
            .arrangement(self.arrangement)
            .alignment(self.alignment)
            .spacing(self.spacing)
            .direction(dir);
        let modifier_fn = self.modifier_fn;
        // content 闭包自动成为组合 scope：content 内（组件外）的 State::get() 注册到本 content scope，
        // State 变化 → 本 content 整体重跑（子组件不 Skip，modifier/表达式重算）——无需显式 start_scope
        ctx.start_scope();
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                // modifier_fn：start 后求值（ACTIVE = 本节点）→ State::get() 注册依赖到本节点
                if let Some(f) = modifier_fn {
                    ctx.set_current_node_modifier(f());
                }
                content(ctx);
            }
        }
        ctx.end_restartable_group();
        ctx.end_scope();
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
        let key = ctx.next_key();
        let policy = BoxLayout::new().alignment(self.alignment);
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
