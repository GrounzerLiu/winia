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

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = m; self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let policy = ColumnLayout::new()
            .arrangement(self.arrangement)
            .alignment(self.alignment)
            .spacing(self.spacing);
        ctx.start_container(key, self.modifier, policy);
        content(ctx);
        ctx.end_node();
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

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = m; self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let policy = RowLayout::new()
            .arrangement(self.arrangement)
            .alignment(self.alignment)
            .spacing(self.spacing);
        ctx.start_container(key, self.modifier, policy);
        content(ctx);
        ctx.end_node();
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

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = m; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let policy = BoxLayout::new().alignment(self.alignment);
        ctx.start_container(key, self.modifier, policy);
        content(ctx);
        ctx.end_node();
    }
}

impl Default for Stack {
    fn default() -> Self { Self::new() }
}
