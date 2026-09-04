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
        // area. Snapshot padding here into plain f32 (build 期求值——Divider 的
        // inset padding 为静态值；动态 padding（SizeValue::Dynamic）快照一次，
        // 后续变化不更新绘制。静态值变化经 param_eq 精确比 → Enter 重建重算。
        // 存 Modifier 进 node 涉 Debug/key，故解耦为四个 f32）。
        let (pad_s, pad_t, pad_e, pad_b) = self.modifier.get_padding_sides();
        let m = if vertical {
            Modifier::new()
                .fill_max_height()
                .width(layout_thickness)
                .draw_node(DividerNode {
                    vertical: true,
                    thickness,
                    color,
                    pad_s, pad_t, pad_e, pad_b,
                })
        } else {
            Modifier::new()
                .fill_max_width()
                .height(layout_thickness)
                .draw_node(DividerNode {
                    vertical: false,
                    thickness,
                    color,
                    pad_s, pad_t, pad_e, pad_b,
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

/// 分隔线绘制节点（exp/divider-node 迁移：原 `.draw` 匿名闭包 ×2）。
///
/// 照抄 `SliderTrackNode` 形状：具名 struct + `DrawNode`，`draw` 内复用
/// `draw_divider_line`。padding 内缩解耦为四个静态 f32（build 期
/// `get_padding_sides()` 快照——M3 inset 变体为静态值；动态 padding 快照一次，
/// 存 Modifier 进 node 涉 Debug/key，故不存）。
/// `thickness` 可为 `DIVIDER_HAIRLINE`（NaN）——`to_bits` 下 NaN==NaN 恒成立
/// （`to_bits` 按位比，NaN payload 相同即相等；`changed` 的 `PartialEq` 下
/// NaN != NaN 恒 dirty——两者语义不同，见注释）。
#[derive(Debug)]
pub(crate) struct DividerNode {
    pub(crate) vertical: bool,
    pub(crate) thickness: f32,
    pub(crate) color: Color,
    pub(crate) pad_s: f32,
    pub(crate) pad_t: f32,
    pub(crate) pad_e: f32,
    pub(crate) pad_b: f32,
}

impl crate::modifier::DrawNode for DividerNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        let inner = skia_safe::Rect::new(
            rect.left + self.pad_s,
            rect.top + self.pad_t,
            rect.right - self.pad_e,
            rect.bottom - self.pad_b,
        );
        draw_divider_line(canvas, inner, self.vertical, self.thickness, self.color);
    }
    fn node_key(&self) -> String {
        format!(
            "divider:{}:{}:{:?}:{}:{}:{}:{}",
            self.vertical,
            self.thickness.to_bits(),
            self.color,
            self.pad_s.to_bits(),
            self.pad_t.to_bits(),
            self.pad_e.to_bits(),
            self.pad_b.to_bits(),
        )
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

    // ── DividerNode 迁移（exp/divider-node）──
    #[test]
    fn divider_node_key_covers_all_static_params() {
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = DividerDefaults::color(&theme);
        let mk = |vertical: bool, thickness: f32, color: Color, pad_s: f32| {
            DividerNode {
                vertical, thickness, color,
                pad_s, pad_t: 0.0, pad_e: 0.0, pad_b: 0.0,
            }
        };
        let base = mk(false, 1.0, color, 0.0);
        assert_eq!(base.node_key(), mk(false, 1.0, color, 0.0).node_key());
        assert_ne!(base.node_key(), mk(true, 1.0, color, 0.0).node_key(), "vertical 应进 key");
        assert_ne!(base.node_key(), mk(false, 2.0, color, 0.0).node_key(), "thickness 应进 key");
        let other = Color::from_argb(255, 1, 2, 3);
        assert_ne!(base.node_key(), mk(false, 1.0, other, 0.0).node_key(), "color 应进 key");
        assert_ne!(base.node_key(), mk(false, 1.0, color, 20.0).node_key(), "pad_s 应进 key");
        // Hairline（NaN）：同 payload NaN → to_bits 相等 → key 相等。
        // 注：build 侧 changed 用 PartialEq（NaN != NaN 恒 dirty → 恒 Enter），
        // 与 node_key（to_bits 按位比）语义不同——Hairline 下迁移无 Skip 收益，
        // 但正确性无损（Enter 重跑结果正确）。
        let h1 = mk(false, DIVIDER_HAIRLINE, color, 0.0);
        let h2 = mk(false, DIVIDER_HAIRLINE, color, 0.0);
        assert_eq!(h1.node_key(), h2.node_key(), "同 Hairline key 应相等（to_bits 按位比）");
        assert_ne!(base.node_key(), h1.node_key(), "Hairline 与 1.0 key 应不等");
    }

    #[test]
    fn divider_node_renders_identical_to_enum_draw() {
        // 真双路对照：同参一路 draw_node(DividerNode)，一路旧 `.draw` 匿名闭包
        // （迁移前 build 侧体逐行复刻，含 padding 内缩），同 300×300 surface
        // 逐字节 assert_eq。
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = DividerDefaults::color(&theme);
        let render_with = |modifier: Modifier| {
            let mut composer = Composer::new();
            let scene = |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let key = ctx.next_key();
                    ctx.start_leaf(key, modifier.clone());
                    ctx.end_node();
                });
            };
            composer.compose(scene);
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        // 横线 + padding_start(20) 内缩场景
        let node_mod = Modifier::new().fill_max_width().height(1.0).draw_node(DividerNode {
            vertical: false, thickness: 1.0, color,
            pad_s: 20.0, pad_t: 0.0, pad_e: 0.0, pad_b: 0.0,
        });
        let enum_mod = Modifier::new().fill_max_width().height(1.0).draw(move |canvas, rect| {
            let inner = skia_safe::Rect::new(rect.left + 20.0, rect.top, rect.right, rect.bottom);
            draw_divider_line(canvas, inner, false, 1.0, color);
        });
        let px_node = render_with(node_mod);
        let px_enum = render_with(enum_mod);
        assert_eq!(px_node.len(), 300 * 300);
        assert_eq!(px_node, px_enum, "node 路与旧 draw 路必须逐字节一致（含 padding 内缩）");
        // 垂直线对照
        let node_v = Modifier::new().fill_max_height().width(2.0).draw_node(DividerNode {
            vertical: true, thickness: 2.0, color,
            pad_s: 0.0, pad_t: 0.0, pad_e: 0.0, pad_b: 0.0,
        });
        let enum_v = Modifier::new().fill_max_height().width(2.0).draw(move |canvas, rect| {
            draw_divider_line(canvas, rect, true, 2.0, color);
        });
        assert_eq!(render_with(node_v), render_with(enum_v), "垂直线双路必须一致");
    }
}
