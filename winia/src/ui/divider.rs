//! Divider 组件 — 对标 material3 `HorizontalDivider` / `VerticalDivider`
//!（Compose material3 Divider.kt + M3 token v0_117）
//!
//! M3 实现要点（对齐项）：
//! - **两种形态**：水平（fillMaxWidth × thickness）与垂直（thickness × fillMaxHeight）；
//!   旧 `Divider` 名在 Compose 已废弃改名 HorizontalDivider——winia 提供
//!   `Divider::horizontal()` / `Divider::vertical()` 双构造（无 deprecated 历史包袱）；
//! - **厚度**：默认 1dp（`DividerTokens.Thickness`）；`DIVIDER_HAIRLINE` 哨兵值
//!   渲染为 1 物理像素（任何 DPI 下都是单像素线，对标 Compose `Dp.Hairline`）；
//! - **颜色**：默认 OutlineVariant（`DividerTokens.Color = ColorSchemeKeyTokens.OutlineVariant`）；
//! - **绘制**：Canvas 中 drawLine 居中于厚度（start/end y = thickness/2——stroke 中心
//!   对齐容器中线，避免亚像素偏移）；
//! - **M3 尺寸变体**（full-width / inset 16dp / middle-inset 16dp）由用户 modifier
//!   padding 实现——组件本身只画满宽/满高线，Compose 同（无内置 inset 参数）。
//!
//! 架构：同 Slider/ProgressIndicator —— `Modifier::draw()` 自定义 Canvas 绘制，无交互。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier};
use crate::ui::theme::{ThemeColors, WiniaTheme};

/// 默认厚度（`DividerTokens.Thickness = 1dp`）
pub const DIVIDER_THICKNESS: f32 = 1.0;
/// 哨兵值：渲染为 1 物理像素（对标 Compose `Dp.Hairline`）——任何 DPI 下都是单像素线
pub const DIVIDER_HAIRLINE: f32 = f32::NAN;

/// 默认值（对标 Compose `DividerDefaults`）
pub struct DividerDefaults;

impl DividerDefaults {
    /// 默认厚度（`DividerTokens.Thickness = 1dp`）
    pub fn thickness() -> f32 {
        DIVIDER_THICKNESS
    }
    /// 默认颜色（`DividerTokens.Color = OutlineVariant`）
    pub fn color(theme: &ThemeColors) -> Color {
        theme.outline_variant
    }
}

/// 分隔线（对标 Compose `HorizontalDivider` / `VerticalDivider`）
///
/// `Divider::horizontal()`：fillMaxWidth × thickness；`Divider::vertical()`：thickness × fillMaxHeight。
/// 用户 modifier 可覆盖尺寸/加 padding 实现 M3 inset/middle-inset 变体。
#[derive(Clone)]
pub struct Divider {
    vertical: bool,
    thickness: f32,
    modifier: Modifier,
    color: Option<Color>,
}

impl Divider {
    /// 水平分隔线（对标 `HorizontalDivider`）
    pub fn horizontal() -> Self {
        Self { vertical: false, thickness: DIVIDER_THICKNESS, modifier: Modifier::new(), color: None }
    }

    /// 垂直分隔线（对标 `VerticalDivider`）
    pub fn vertical() -> Self {
        Self { vertical: true, thickness: DIVIDER_THICKNESS, modifier: Modifier::new(), color: None }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 线宽（dp；`DIVIDER_HAIRLINE` = 1 物理像素，对标 `Dp.Hairline`）
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    /// 颜色（默认 OutlineVariant）
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.vertical);
        ctx.changed(&self.thickness);
        ctx.changed(&self.color);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let color = self.color.unwrap_or_else(|| DividerDefaults::color(&theme));
        let vertical = self.vertical;
        let thickness = self.thickness;
        // 布局厚度：Hairline 时用 1.0 兜底（NaN 会崩布局；1dp@density1 ≈ 1 物理像素）
        let layout_thickness = if thickness.is_nan() { 1.0 } else { thickness };

        // Framework CustomDraw receives the NODE rect (padding not applied);
        // user modifier padding (M3 inset/middle-inset variants) shrinks the content
        // area. Read padding here and inset the draw rect, matching DrawIcon semantics.
        let pad_rect = self.modifier.clone();
        let m = if vertical {
            Modifier::new()
                .fill_max_height()
                .width(layout_thickness)
                .draw(move |canvas, rect| {
                    let (s, t, e, b) = pad_rect.get_padding_sides();
                    let inner = skia_safe::Rect::new(
                        rect.left + s, rect.top + t, rect.right - e, rect.bottom - b,
                    );
                    draw_divider_line(canvas, inner, true, thickness, color);
                })
        } else {
            Modifier::new()
                .fill_max_width()
                .height(layout_thickness)
                .draw(move |canvas, rect| {
                    let (s, t, e, b) = pad_rect.get_padding_sides();
                    let inner = skia_safe::Rect::new(
                        rect.left + s, rect.top + t, rect.right - e, rect.bottom - b,
                    );
                    draw_divider_line(canvas, inner, false, thickness, color);
                })
        };

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

/// 画分隔线：厚度为 1 物理像素（Hairline）或 dp 值；线居中于厚度（Compose 同）
pub(crate) fn draw_divider_line(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    vertical: bool,
    thickness: f32,
    color: Color,
) {
    // Hairline：1 物理像素（Compose Dp.Hairline 语义——不随 DPI 缩放）
    let px = if thickness.is_nan() { 1.0 } else { thickness };
    if px <= 0.0 { return; }
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(px);
    paint.set_color(skia_color(color));
    if vertical {
        // 垂直：x = 厚度/2（stroke 中心对齐中线），从顶到底
        let cx = rect.left + px / 2.0;
        canvas.draw_line(
            skia_safe::Point::new(cx, rect.top),
            skia_safe::Point::new(cx, rect.bottom),
            &paint,
        );
    } else {
        // 水平：y = 厚度/2
        let cy = rect.top + px / 2.0;
        canvas.draw_line(
            skia_safe::Point::new(rect.left, cy),
            skia_safe::Point::new(rect.right, cy),
            &paint,
        );
    }
}

fn skia_color(c: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::Constraints;

    // ── 默认值 ──
    #[test]
    fn defaults_match_compose() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        // Thickness = 1dp
        assert_eq!(DividerDefaults::thickness(), 1.0);
        // Color = OutlineVariant
        assert_eq!(DividerDefaults::color(&theme), theme.outline_variant);
    }

    // ── 像素测试：水平/垂直渲染 ──
    fn render_divider(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), pm.width() as usize)
    }

    fn px_at(buf: &[[u8; 4]], w: usize, x: usize, y: usize) -> [u8; 4] {
        let p = buf[y * w + x];
        [p[2], p[1], p[0], p[3]]
    }

    fn color_eq(c: Color, px: [u8; 4], tol: u8) -> bool {
        (c.r as i16 - px[0] as i16).abs() <= tol as i16
            && (c.g as i16 - px[1] as i16).abs() <= tol as i16
            && (c.b as i16 - px[2] as i16).abs() <= tol as i16
            && (c.a as i16 - px[3] as i16).abs() <= tol as i16
    }

    #[test]
    fn horizontal_divider_pixels() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = DividerDefaults::color(&theme);
        let (buf, w) = render_divider(|ctx| {
            Divider::horizontal().build(ctx);
        });
        // fillMaxWidth → 线横跨整个宽度；高度 1dp → 在 y≈0（BoxLayout 默认对齐）
        // 扫第一行（y=0）应整行都是 divider 色
        let mut found = 0;
        for x in 0..w {
            if color_eq(color, px_at(&buf, w, x, 0), 6) { found += 1; }
        }
        assert!(found > w * 9 / 10, "水平分隔线应横跨 90%+ 宽度，found={found}/{w}");
    }
    #[test]
    fn vertical_divider_pixels() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = DividerDefaults::color(&theme);
        let (buf, w) = render_divider(|ctx| {
            Divider::vertical().build(ctx);
        });
        // fillMaxHeight → 线纵贯整个高度；宽度 1dp → x≈0
        let mut found = 0;
        for y in 0..300 {
            if color_eq(color, px_at(&buf, w, 0, y), 6) { found += 1; }
        }
        assert!(found > 270, "垂直分隔线应纵贯 90%+ 高度，found={found}");
    }

    #[test]
    fn custom_color_and_thickness() {
        // 自定义颜色 + 3dp 厚度：线应出现在 y=0..3 行
        let custom = Color::from_argb(255, 255, 0, 0);
        let (buf, w) = render_divider(|ctx| {
            Divider::horizontal().thickness(3.0).color(custom).build(ctx);
        });
        let mut rows = 0;
        for y in 0..3 {
            if color_eq(custom, px_at(&buf, w, 10, y), 6) { rows += 1; }
        }
        assert!(rows >= 2, "3dp 厚度线应占 2-3 行，rows={rows}");
        // 下方应无颜色
        assert!(!color_eq(custom, px_at(&buf, w, 10, 10), 6), "3dp 线不应延伸到 y=10");
    }

    #[test]
    fn hairline_renders_single_pixel() {
        // DIVIDER_HAIRLINE：仅 1 行（1 物理像素）
        let custom = Color::from_argb(255, 0, 0, 255);
        let (buf, w) = render_divider(|ctx| {
            Divider::horizontal().thickness(DIVIDER_HAIRLINE).color(custom).build(ctx);
        });
        let mut rows = 0;
        for y in 0..5 {
            if color_eq(custom, px_at(&buf, w, 10, y), 6) { rows += 1; }
        }
        assert_eq!(rows, 1, "Hairline 应只占 1 行，rows={rows}");
    }

    #[test]
    fn padding_insets_divider_line() {
        // M3 inset 变体：modifier padding 应内缩绘制（CustomDraw 用节点 rect 的 workaround）
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = DividerDefaults::color(&theme);
        let (buf, w) = render_divider(|ctx| {
            Divider::horizontal()
                .modifier(Modifier::new().padding_start(20.0))
                .build(ctx);
        });
        // 线应从 x=20 开始（不在 x<20 出现）
        let mut early = 0;
        for x in 0..15 {
            if color_eq(color, px_at(&buf, w, x, 0), 6) { early += 1; }
        }
        assert_eq!(early, 0, "padding_start(20) 后 x<15 不应有线，early={early}");
        // 线应在 x>=20 出现
        let mut later = 0;
        for x in 22..40 {
            if color_eq(color, px_at(&buf, w, x, 0), 6) { later += 1; }
        }
        assert!(later > 0, "padding_start(20) 后线应从 x=20 开始");
    }
}
