//! Modifier 系统 — 不可变链式修饰符 + 自定义扩展
//!
//! 类似 Jetpack Compose 的 Modifier，用于解耦外观/行为/布局。
//! - 链式 API: `Modifier::new().size(100, 100).padding(10).background(Color::RED)`
//! - 左到右 = 外到内
//! - 分为三类: LayoutModifier / DrawModifier / PointerInputModifier

use std::sync::Arc;
use std::ops::Range;
use std::fmt::{self, Debug};
#[cfg(test)]
use crate::fling::SnapInfo;
use crate::graphics::{DEFAULT_AMBIENT_SHADOW_COLOR, DEFAULT_SPOT_SHADOW_COLOR};
use crate::input::{KbEvent, PointerEvent};
use crate::layout::{Dimension, IntrinsicSize, SizeValue};
use crate::text::RichSpanStyle;
use crate::input::interaction::MutableInteractionSource;
use crate::graphics::{
    BackgroundColor, Color, ColorFilter, FilterQuality, GraphicsLayerParams,
    GraphicsLayerSpec, ShadowParams, Shape, TransformOrigin,
};


// ── Dimension ──


// ── Shape ──


// ── Color (占位) ──


// ── Modifier ──

// ── KbEvent ──


// ── PointerEvent ──


// ── 图片绘制类型（ColorFilter / FilterQuality / BlendMode——对齐 Compose ui.graphics）──



// ── Modifier ──

/// 不可变的 Modifier 链。
///
/// 每次调用修饰方法返回一个新的 Modifier（内部 elements 是 clone-on-write 式的追加）。
/// Clone 是廉价操作。
#[derive(Debug, Clone)]
pub struct Modifier {
    elements: Vec<ModifierElement>,
    nodes: Vec<ModifierNode>,
}

impl Modifier {
    /// 创建空的 Modifier 链
    pub fn new() -> Self {
        Modifier {
            elements: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// 内部方法：追加一个元素并返回新 Modifier
    pub(crate) fn push(mut self, element: ModifierElement) -> Self {
        self.elements.push(element);
        self
    }

    /// Attach the container's item-animation host so the renderer can draw its exit pictures
    /// inside the container's own scroll clip and translate.
    pub(crate) fn with_item_animation_host(
        self,
        host: crate::layout::item_animation::ItemAnimationHost,
    ) -> Self {
        self.push(ModifierElement::ItemAnimationHost(host))
    }

    /// 合并另一个 Modifier 链的所有元素（追加到末尾）。
    ///
    /// 等价于 Compose 的 `Modifier.then(other)`，用于叠加两个独立的 Modifier 链。
    ///
    /// # 示例
    /// ```ignore
    /// let base = Modifier::new().size(100, 50).background(RED);
    /// let extra = Modifier::new().padding(8).clickable(|| {});
    /// let combined = base.then(extra);
    /// // combined = size(100,50) → background(RED) → padding(8) → clickable
    /// ```
    pub fn then(mut self, other: Modifier) -> Self {
        self.elements.extend(other.elements);
        self.nodes.extend(other.nodes);
        self
    }

    /// 返回所有元素的只读引用
    pub(crate) fn elements(&self) -> &[ModifierElement] {
        &self.elements
    }

    // ── Node 轨道（实验性开放扩展点） ──

    /// Append a draw node (renders at the background layer, after the enum chain).
    pub fn draw_node(self, node: impl DrawNode + 'static) -> Self {
        self.push_node(ModifierNode::Draw(std::sync::Arc::new(node)))
    }

    /// Compose's `Modifier.drawWithCache`: `build` runs once per (size, density) and `draw` receives
    /// the cached value on every paint. What the cache is for: a Skia shader, a paragraph, a
    /// geometry path — anything whose construction should not repeat 60 times a second.
    pub fn draw_with_cache<C: Send + Sync + 'static>(
        self,
        build: impl Fn(DrawCacheScope) -> C + Send + Sync + 'static,
        draw: impl Fn(&C, &skia_safe::Canvas, skia_safe::Rect) + Send + Sync + 'static,
    ) -> Self {
        self.draw_node(DrawCacheNode {
            build: std::sync::Arc::new(build),
            draw: std::sync::Arc::new(draw),
            cache: std::sync::Mutex::new(None),
        })
    }

    /// Append a wrapping draw node (before at the background layer, after above
    /// content — after children and ripple, inside the scroll translate).
    pub fn draw_wrap_node(self, node: impl DrawWrapNode + 'static) -> Self {
        self.push_node(ModifierNode::DrawWrap(std::sync::Arc::new(node)))
    }

    /// 追加一个点击节点（与 Clickable 枚举同优先级参与点击分发）。
    pub fn click_node(self, node: impl ClickNode + 'static) -> Self {
        self.push_node(ModifierNode::Click(std::sync::Arc::new(node)))
    }

    /// 追加一个指针节点（与 PointerEvent 枚举同语义参与隧道/冒泡分发）。
    pub fn pointer_node(self, node: impl PointerNode + 'static) -> Self {
        self.push_node(ModifierNode::Pointer(std::sync::Arc::new(node)))
    }

    /// 追加一个键盘节点（与 KbEvent 枚举同语义参与隧道/冒泡分发）。
    pub fn key_node(self, node: impl KeyNode + 'static) -> Self {
        self.push_node(ModifierNode::Key(std::sync::Arc::new(node)))
    }

    /// 追加一个布局节点 A 型（约束变换——resolved_size 之后、padding 之前串行）。
    pub fn layout_node(self, node: impl LayoutModifierNode + 'static) -> Self {
        self.push_node(ModifierNode::Layout(std::sync::Arc::new(node)))
    }

    pub(crate) fn push_node(mut self, node: ModifierNode) -> Self {
        self.nodes.push(node);
        self
    }

    /// 返回所有开放节点的只读引用
    #[allow(dead_code)] // the tests in this file call it
    pub(crate) fn modifier_nodes(&self) -> &[ModifierNode] {
        &self.nodes
    }

    /// Open draw-node iteration (render pipeline — background layer, after the enum chain).
    pub(crate) fn draw_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn DrawNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Draw(d) => Some(d),
            _ => None,
        })
    }

    /// Open wrapping-draw-node iteration (render pipeline — before at the background
    /// layer, after above content).
    pub(crate) fn draw_wrap_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn DrawWrapNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::DrawWrap(w) => Some(w),
            _ => None,
        })
    }

    /// 开放指针节点迭代（输入管线用——与枚举 PointerEvent 同序交织分发）。
    pub(crate) fn pointer_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn PointerNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Pointer(p) => Some(p),
            _ => None,
        })
    }

    /// 开放键盘节点迭代（输入管线用——与枚举 KbEvent 同序交织分发）。
    pub(crate) fn key_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn KeyNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Key(k) => Some(k),
            _ => None,
        })
    }

    /// 开放布局节点迭代（测量管线用——resolved_size 之后串行变换约束）。
    pub(crate) fn layout_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn LayoutModifierNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Layout(l) => Some(l),
            _ => None,
        })
    }

    /// 是否含布局节点（measure 早退用——P2-2）。
    pub(crate) fn has_layout_nodes(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n, ModifierNode::Layout(_)))
    }

    // ── 试点节点（exp/modifier-node）：Background/Clickable 的 node 等价物 ──
    // 用法示例见本文件末尾测试 `node_track_*`。第三方自定义行为照此形状实现
    // DrawNode/ClickNode trait 即可，无需改核心枚举与管线 match。
}

impl Default for Modifier {
    fn default() -> Self {
        Self::new()
    }
}


// ── 查询方法: 布局参数提取 ──


// ── ScrollState ──


/// TextField 支持文本绘制参数（画在容器底部外侧 4dp）
#[derive(Clone, Debug)]
pub struct SupportingVisual {
    pub content: String,
    pub font_size: f32,
    pub font_weight: crate::text::FontWeight,
    pub font_style: crate::text::FontSlant,
    pub letter_spacing: f32,
    pub line_height: Option<f32>,
    pub color: Color,
}

impl SupportingVisual {
    pub fn height(&self) -> f32 {
        4.0 + self.line_height.unwrap_or(self.font_size * 1.4)
    }
}


/// 滚动状态，对齐 Compose ScrollState

#[cfg(test)]
pub(crate) use crate::scroll::state::snap_target;
pub use crate::scroll::state::ScrollState;

pub use crate::focus::FocusRequester;
pub(crate) use crate::focus::{focus_window, take_focus_requests, FocusWindowGuard};

// ── 测试 ──


#[cfg(test)]
mod tests;

// ── RichSpanStyle ──



mod debug;
mod param_eq;
mod query;

// `debug-server` builds the tree JSON outside a test build, and the tree includes each open
// node's key (`debug::server::describe_modifier`).
#[cfg(any(test, feature = "debug-server"))]
pub(crate) use param_eq::node_key_of;

#[cfg(test)]
mod param_eq_tests;

mod element;

use element::DrawCacheNode;
pub use element::*;
pub(crate) use element::{ModifierElement, SizeStep};

mod builders;

/// The text payload's extra parameters — see `builders::draw::TextExtras`.
pub(crate) use builders::draw::TextExtras;
