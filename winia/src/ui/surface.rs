//! Surface 组件 — 对齐 Compose material3 `Surface`（非交互重载）
//!
//! 对标 Compose `Surface` 的职责（源码注释原文）：
//! 1. **Clipping**——按 `shape` 裁剪子节点
//! 2. **Borders**——`shape` 有边框则绘制
//! 3. **Background**——按 `shape` 填充 `color`（`surface` 色叠加 tonal overlay；
//!    winia 简化：tonal 叠加由主题 surface 色直接决定，如需应用 `tonal_elevation`
//!    可在后续扩展）
//! 4. **Content color**——`content_color` 作为子内容（Text/Icon）默认色；未设时
//!    按主题匹配（`color == theme.surface` → `on_surface`，否则保持上层值）
//! 5. Blocking touch propagation behind the surface
//!
//! 与 Card 的区别：`Surface` 是**通用容器原语**（无 M3 Card 的状态化取色/
//! 水波纹），只负责「外观底板」——shadow → border → background → clip。
//! 面板（BottomSheet）用 `Surface` 承载 anchoredDraggable + nestedScroll（对齐
//! Compose：`Surface { .nestedScroll(...).anchoredDraggable(...) }`）。
//!
//! 仅实现纯外观重载（对标 Compose `Surface()`）。clickable/selectable/toggleable
//! 三个交互重载待后续需要时补充。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::modifier::{Color, Modifier, Shape};

/// 边框描边（对齐 Compose `BorderStroke(width, color)`——winia 用 (f32, Color) 表达；
/// shape 在绘制时传入）。为与 Compose Surface 参数名一致，此处用 `border: Option<Border>`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceBorder {
    pub width: f32,
    pub color: Color,
}

impl SurfaceBorder {
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

/// 通用外观底板（对标 Compose material3 `Surface` 非交互重载）。
pub struct Surface {
    shape: Shape,
    color: Option<Color>,
    content_color: Option<Color>,
    tonal_elevation: f32,
    shadow_elevation: f32,
    border: Option<SurfaceBorder>,
    modifier: Modifier,
}

impl Surface {
    /// 创建 Surface，默认 `shape=Rectangle`、`color=theme.surface`。
    pub fn new() -> Self {
        Self {
            shape: Shape::Rectangle,
            color: None,
            content_color: None,
            tonal_elevation: 0.0,
            shadow_elevation: 0.0,
            border: None,
            modifier: Modifier::new(),
        }
    }

    /// 表面形状（也是阴影/裁剪形状）。
    pub fn shape(mut self, shape: impl Into<Shape>) -> Self {
        self.shape = shape.into();
        self
    }

    /// 背景色。不传默认 `theme.surface`（Compose 默认 `MaterialTheme.colorScheme.surface`）。
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// 内容色（下传给 Text/Icon 作为默认色）。不传时按主题匹配：
    /// `color == theme.surface` → `on_surface`，否则保持上层值（Compose 语义）。
    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }

    /// 色调抬升（仅影响表面色叠加；winia 简化版先保留字段，后续可接 tonal 色）。
    pub fn tonal_elevation(mut self, elevation: f32) -> Self {
        self.tonal_elevation = elevation;
        self
    }

    /// 阴影高度（对齐 Compose `shadowElevation`——lift 到图形层阴影）。
    pub fn shadow_elevation(mut self, elevation: f32) -> Self {
        self.shadow_elevation = elevation;
        self
    }

    /// 描边（对齐 Compose `border: BorderStroke?`）。
    pub fn border(mut self, border: SurfaceBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 追加用户 modifier（外层，可覆盖默认样式）。
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 构建 Surface。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let theme = crate::ui::theme::WiniaTheme::colors();
        let shape = self.shape;
        // 默认背景 = theme.surface（Compose 默认 `colorScheme.surface`）
        let color = self.color.unwrap_or(theme.surface);
        // 内容色：显式传入优先；否则按主题匹配（color==surface→on_surface，否则保持）
        let content_color = self.content_color.unwrap_or_else(|| {
            if color == theme.surface { theme.on_surface } else { color }
        });
        let shadow_elevation = self.shadow_elevation;

        // Compose `Modifier.surface`（核心可复用链）：shadow → border → background → clip。
        // - shadow：仅 shadow_elevation > 0 时用 graphics_layer 阴影（对齐 Compose `graphicsLayer{shadowElevation, clip=false}`）
        // - border：`border(width, color, shape)`
        // - background：`background(color, shape)`
        // - clip：`clip(shape)`
        let mut modifier = Modifier::new();
        if shadow_elevation > 0.0 {
            modifier = modifier.graphics_layer(move || crate::modifier::GraphicsLayerParams {
                shadow_elevation,
                shadow_shape: Some(shape),
                ..Default::default()
            });
        }
        if let Some(b) = self.border {
            modifier = modifier.border(b.width, b.color, shape);
        }
        modifier = modifier.background(color, shape);
        modifier = modifier.clip(shape);

        // 追加用户 modifier（外层）
        modifier = modifier.then(self.modifier);

        // content 闭包为组合 scope；布局策略 = BoxLayout（对标 Compose Box）。
        // Box 无方向参数（层叠堆叠——子节点共享空间），Direction 走 modifier 层。
        match ctx.start_restartable_group(
            key,
            modifier,
            crate::layout::BoxLayout::new(),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 内容色下传（LocalContentColor 等价物——future Text/Icon 默认色）
                crate::ui::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    content(ctx);
                });
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for Surface {
    fn default() -> Self { Self::new() }
}
