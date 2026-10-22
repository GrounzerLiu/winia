//! Spacer — a sized blank for explicit gaps.

use super::box_layout::BoxLayout;
use super::container::build_container;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

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
