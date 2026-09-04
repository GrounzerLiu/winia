//! Loading Indicator 组件 — 对标 M3 Expressive `LoadingIndicator` / `ContainedLoadingIndicator`
//!
//! M3 实现要点（对齐项）：
//! - **两种配置**：Uncontained（`LoadingIndicator`——无容器，indicator = Primary）与
//!   Container（`.contained(true)`——圆形容器，container = SecondaryContainer，
//!   indicator = Primary）；
//! - **尺寸**：容器 48×48dp、active indicator 38dp（`md.comp.loading-indicator.container.width/
//!   height = 48dp`、`active-indicator.size = 38dp`），容器 shape = Full（圆）；
//! - **形状序列**：7 个 Material 3 shapes（SoftBurst → Cookie9Sided → Pentagon → Pill →
//!   Sunny → Cookie4Sided → Oval），`Morph` 相邻插值 + 首尾回环；
//! - **动画**：每个 morph 周期 Spring(bouncy 0.6/200) 0→1，周期约 650ms；
//!   每个周期额外旋转 90°（quarter rotation）；全局旋转 360° / 4666ms 线性无限循环；
//! - **绘制**：`Morph.to_path(progress)` → 缩放到 active indicator 尺寸 → 绕容器中心旋转。

use crate::animation::interpolator;
use crate::animation::{InfiniteRepeatableSpec, SpringSpec, TweenSpec};
use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::effect::LaunchedEffect;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::theme::{ThemeColors, WiniaTheme};
use material_shapes::{MaterialShapes, Morph, MorphToPath, RoundedPolygon};
use std::sync::LazyLock;
use std::time::Duration;

/// 容器总尺寸（`md.comp.loading-indicator.container.width/height = 48dp`）
pub const LOADING_INDICATOR_SIZE: f32 = 48.0;
/// Active indicator 尺寸（`md.comp.loading-indicator.active-indicator.size = 38dp`）
pub const LOADING_INDICATOR_ACTIVE_SIZE: f32 = 38.0;
/// 默认容器形状（M3 容器 shape 为 Full）
pub const LOADING_INDICATOR_CONTAINER_SHAPE: Shape = Shape::Circle;

/// 默认的 7 个 Material 3 shapes（对齐 `LoadingIndicatorDefaults.IndeterminateIndicatorPolygons`）
static POLYGONS: LazyLock<[RoundedPolygon; 7]> = LazyLock::new(|| {
    [
        MaterialShapes::soft_burst().normalized(),
        MaterialShapes::cookie_9_sided().normalized(),
        MaterialShapes::pentagon().normalized(),
        MaterialShapes::pill().normalized(),
        MaterialShapes::sunny().normalized(),
        MaterialShapes::cookie_4_sided().normalized(),
        MaterialShapes::oval().normalized(),
    ]
});

/// 预计算的相邻 shape Morph 序列（含首尾回环）
static MORPH_SEQUENCE: LazyLock<Vec<Morph<'static>>> = LazyLock::new(|| {
    let polygons: &'static [RoundedPolygon] = &POLYGONS[..];
    morph_sequence(polygons)
});

/// 把归一化 shape 缩放/平移到容器内 active indicator 区域的缩放因子
static SHAPE_SCALE_FACTOR: LazyLock<f32> = LazyLock::new(|| {
    calculate_scale_factor(&POLYGONS[..]) * (LOADING_INDICATOR_ACTIVE_SIZE / LOADING_INDICATOR_SIZE)
});

/// Loading Indicator 组件。
///
/// 默认是 **uncontained**（对标 Compose `LoadingIndicator`）：
/// - 无容器，indicator 色 = `Primary`
///
/// 调用 [`.contained()`](Self::contained) 切换为 **Container 模式**（M3 Default 配置）：
/// - 圆形容器 = `SecondaryContainer`，indicator 色 = `Primary`
#[derive(Clone)]
pub struct LoadingIndicator {
    is_contained: bool,
    modifier: Modifier,
    indicator_color: Option<Color>,
    container_color: Option<Color>,
    container_shape: Shape,
}

impl Default for LoadingIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadingIndicator {
    /// 创建 uncontained loading indicator。
    pub fn new() -> Self {
        Self {
            is_contained: false,
            modifier: Modifier::new(),
            indicator_color: None,
            container_color: None,
            container_shape: LOADING_INDICATOR_CONTAINER_SHAPE,
        }
    }

    /// 切换为 Container 模式（带 SecondaryContainer 圆形容器 + Primary indicator）。
    ///
    /// 配色按 M3 specs 配图（Primary + SecondaryContainer）；如需 Compose
    /// `ContainedLoadingIndicator` 的 PrimaryContainer + OnPrimaryContainer，
    /// 请用 [`indicator_color`](Self::indicator_color) 和
    /// [`container_color`](Self::container_color) 显式覆盖。
    pub fn contained(mut self, contained: bool) -> Self {
        self.is_contained = contained;
        self
    }

    /// 应用外部 Modifier。
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 覆盖 active indicator 颜色。
    pub fn indicator_color(mut self, color: Color) -> Self {
        self.indicator_color = Some(color);
        self
    }

    /// 覆盖容器颜色（仅在 contained 配置下可见）。
    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }

    /// 覆盖容器形状（默认圆形；仅在 contained 配置下可见）。
    pub fn container_shape(mut self, shape: Shape) -> Self {
        self.container_shape = shape;
        self
    }

    /// 当前是否 contained。
    pub fn is_contained(&self) -> bool {
        self.is_contained
    }

    /// 默认颜色解析（对标 Compose `LoadingIndicatorDefaults`）。
    fn resolve_colors(&self, theme: &ThemeColors) -> (Color, Color) {
        let indicator_color = self.indicator_color.unwrap_or(theme.primary);
        let container_color = self.container_color.unwrap_or_else(|| {
            if self.is_contained {
                theme.secondary_container
            } else {
                Color::TRANSPARENT
            }
        });
        (indicator_color, container_color)
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.is_contained);
        ctx.changed(&self.indicator_color);
        ctx.changed(&self.container_color);
        ctx.changed(&self.container_shape);

        let theme = WiniaTheme::colors();
        let (indicator_color, container_color) = self.resolve_colors(&theme);

        // 动画状态（remember 保证重组时保持同一份）
        let morph_progress: State<f32> = ctx.remember(|| 0.0);
        let morph_index: State<usize> = ctx.remember(|| 0);
        let morph_rotation_target: State<f32> = ctx.remember(|| QUARTER_ROTATION);

        // Morph 序列循环：Spring 0→1 → 切到下一 shape + 90° 步进 → 重置 progress
        LaunchedEffect::new(()).build(ctx, {
            let morph_progress = morph_progress.clone();
            let morph_index = morph_index.clone();
            let morph_rotation_target = morph_rotation_target.clone();
            move |_scope| async move {
                let sequence_len = MORPH_SEQUENCE.len();
                loop {
                    crate::animation::push_animatable(
                        morph_progress.clone(),
                        1.0,
                        crate::animation::AnimationSpec::Spring(SpringSpec::bouncy()),
                    );
                    tokio::time::sleep(Duration::from_millis(650)).await;
                    // 防止弹簧尚未完全收敛时被下一轮覆盖
                    crate::animation::cancel_animation(&morph_progress);
                    let current = morph_index.get();
                    let next = (current + 1) % sequence_len;
                    morph_index.set(next);
                    let angle = morph_rotation_target.get();
                    morph_rotation_target.set((angle + QUARTER_ROTATION) % FULL_ROTATION);
                    morph_progress.set(0.0);
                }
            }
        });

        // 全局旋转：360° / 4666ms 线性无限循环
        let mut inf = ctx.remember_infinite_transition();
        let global_rotation = inf.animate_float(
            ctx,
            0.0,
            FULL_ROTATION,
            InfiniteRepeatableSpec::restart_tween(
                Duration::from_millis(4666),
                TweenSpec::new(Duration::from_millis(4666), interpolator::Linear::new()),
            ),
        );

        let m = Modifier::new()
            .size(LOADING_INDICATOR_SIZE, LOADING_INDICATOR_SIZE)
            .draw_node(LoadingIndicatorNode {
                is_contained: self.is_contained,
                container_shape: self.container_shape,
                indicator_color,
                container_color,
                morph_progress: morph_progress.clone(),
                morph_index: morph_index.clone(),
                morph_rotation_target: morph_rotation_target.clone(),
                global_rotation: global_rotation.clone(),
            });

        let m = m.then(self.modifier);
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

/// Loading 绘制节点（exp/wavy-node 迁移：原 `.draw` 匿名闭包）。
/// 4 个动画值（morph_progress/index/rotation_target/global_rotation）peek，
/// 不进 key；静态参数全进。
#[derive(Debug)]
pub(crate) struct LoadingIndicatorNode {
    pub(crate) is_contained: bool,
    pub(crate) container_shape: Shape,
    pub(crate) indicator_color: Color,
    pub(crate) container_color: Color,
    pub(crate) morph_progress: State<f32>,
    pub(crate) morph_index: State<usize>,
    pub(crate) morph_rotation_target: State<f32>,
    pub(crate) global_rotation: State<f32>,
}

impl crate::modifier::DrawNode for LoadingIndicatorNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        if self.is_contained && self.container_color.a != 0 {
            draw_container(canvas, rect, self.container_color, self.container_shape);
        }

        let current_index = self.morph_index.peek();
        let progress = self.morph_progress.peek();
        let morph = &MORPH_SEQUENCE[current_index];
        let path = morph.to_path(progress, 0, None, None, None, None);
        let path =
            progress_path(path, (rect.width(), rect.height()), *SHAPE_SCALE_FACTOR);

        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(skia_color(self.indicator_color));
        paint.set_style(skia_safe::paint::Style::Fill);

        let morph_rotation_target = self.morph_rotation_target.peek();
        let global_rotation = self.global_rotation.peek();
        let total_rotation = progress * 90.0 + morph_rotation_target + global_rotation;

        canvas.save();
        canvas.translate((rect.left, rect.top));
        let center = skia_safe::Point::new(rect.width() / 2.0, rect.height() / 2.0);
        canvas.rotate(total_rotation, Some(center));
        canvas.draw_path(&path, &paint);
        canvas.restore();
    }
    fn node_key(&self) -> String {
        format!(
            "loading:{:?}:{:?}:{:?}:{:?}",
            self.is_contained,
            self.container_shape,
            self.indicator_color,
            self.container_color,
        )
    }
}

/// 绘制 contained 容器（默认圆形 Full）。
fn draw_container(canvas: &skia_safe::Canvas, rect: skia_safe::Rect, color: Color, shape: Shape) {
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(skia_color(color));
    paint.set_style(skia_safe::paint::Style::Fill);
    match shape {
        Shape::Circle => {
            let radius = rect.width().min(rect.height()) / 2.0;
            canvas.draw_circle((rect.center_x(), rect.center_y()), radius, &paint);
        }
        Shape::Pill => {
            let r = rect.height() / 2.0;
            canvas.draw_round_rect(rect, r, r, &paint);
        }
        Shape::RoundedRect { corner_radius } => {
            canvas.draw_round_rect(rect, corner_radius, corner_radius, &paint);
        }
        Shape::TopRoundedRect { radius } => {
            let rr = skia_safe::RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(radius, radius),
                skia_safe::Vector::new(radius, radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        Shape::Rectangle => {
            canvas.draw_rect(rect, &paint);
        }
    }
}

/// 把 Morph 生成的 Path 缩放到 active indicator 尺寸并居中。
fn progress_path(path: skia_safe::Path, size: (f32, f32), scale_factor: f32) -> skia_safe::Path {
    let scale_x = size.0 * scale_factor;
    let scale_y = size.1 * scale_factor;

    let size_center_x = size.0 / 2.0;
    let size_center_y = size.1 / 2.0;

    let translate_x = size_center_x - scale_x / 2.0;
    let translate_y = size_center_y - scale_y / 2.0;
    path.make_scale((scale_x, scale_y))
        .make_offset((translate_x, translate_y))
}

const FULL_ROTATION: f32 = 360.0;
const QUARTER_ROTATION: f32 = FULL_ROTATION / 4.0;

/// 计算一组归一化 polygon 相对其最大旋转包围盒的缩放因子。
fn calculate_scale_factor(polygons: &[RoundedPolygon]) -> f32 {
    let mut scale_factor = 1.0_f32;
    for polygon in polygons {
        let bounds = polygon.calculate_bounds(None);
        let max_bounds = polygon.calculate_max_bounds();
        let scale_x = bounds.width() / max_bounds.width();
        let scale_y = bounds.height() / max_bounds.height();
        scale_factor = scale_factor.min(scale_x.max(scale_y));
    }
    scale_factor
}

/// 构造相邻 shape 的 Morph 序列（含首尾回环）。
fn morph_sequence(polygons: &[RoundedPolygon]) -> Vec<Morph<'_>> {
    let mut morphs = Vec::new();
    for i in 0..polygons.len() {
        if i + 1 < polygons.len() {
            morphs.push(Morph::new(&polygons[i], &polygons[i + 1]));
        } else {
            morphs.push(Morph::new(&polygons[i], &polygons[0]));
        }
    }
    morphs
}

trait BoundsSizeExt {
    fn width(&self) -> f32;
    fn height(&self) -> f32;
}

impl BoundsSizeExt for [f32; 4] {
    fn width(&self) -> f32 {
        self[2] - self[0]
    }
    fn height(&self) -> f32 {
        self[3] - self[1]
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
    use crate::modifier::Color;

    #[test]
    fn default_is_uncontained() {
        let li = LoadingIndicator::new();
        assert!(!li.is_contained());
        assert_eq!(li.indicator_color, None);
        assert_eq!(li.container_color, None);
    }

    #[test]
    fn contained_switches_colors() {
        let theme = ThemeColors::default_light();
        let uncontained = LoadingIndicator::new();
        let (ic, cc) = uncontained.resolve_colors(&theme);
        assert_eq!(ic, theme.primary);
        assert_eq!(cc, Color::TRANSPARENT);

        let contained = LoadingIndicator::new().contained(true);
        assert!(contained.is_contained());
        let (ic, cc) = contained.resolve_colors(&theme);
        assert_eq!(ic, theme.primary);
        assert_eq!(cc, theme.secondary_container);
    }

    #[test]
    fn morph_sequence_has_seven_morphs() {
        assert_eq!(POLYGONS.len(), 7);
        assert_eq!(MORPH_SEQUENCE.len(), 7);
        // 每个 morph 都能生成非空 path
        let morph = &MORPH_SEQUENCE[0];
        let path = morph.to_path(0.5, 0, None, None, None, None);
        assert!(!path.is_empty());
    }

    #[test]
    fn scale_factor_fits_all_shapes() {
        let factor = *SHAPE_SCALE_FACTOR;
        assert!(factor > 0.0 && factor <= 1.0);
        // 对每个 shape：缩放后 bounds 应落在容器内
        for polygon in POLYGONS.iter() {
            let bounds = polygon.calculate_bounds(None);
            let max_bounds = polygon.calculate_max_bounds();
            let scale_x = bounds.width() / max_bounds.width();
            let scale_y = bounds.height() / max_bounds.height();
            let min_scale = scale_x.max(scale_y);
            assert!(factor <= min_scale + 1e-4, "factor {factor} > {min_scale}");
        }
    }

    #[test]
    fn progress_path_centers_and_scales() {
        let polygon = &POLYGONS[0];
        let path = Morph::new(polygon, polygon).to_path(0.0, 0, None, None, None, None);
        let scaled = progress_path(path, (48.0, 48.0), *SHAPE_SCALE_FACTOR);
        let b = scaled.bounds();
        // 缩放后应居中：中心 ≈ (24, 24)
        assert!(
            (b.center_x() - 24.0).abs() < 1.0,
            "center_x {}",
            b.center_x()
        );
        assert!(
            (b.center_y() - 24.0).abs() < 1.0,
            "center_y {}",
            b.center_y()
        );
        // 不应超出 48 容器
        assert!(b.width() <= 48.0 + 1.0);
        assert!(b.height() <= 48.0 + 1.0);
    }

    fn render_loading(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        // indeterminate 会注册无限动画（永不完成）——持串行锁并前后清理，
        // 防止残留污染其他测试的全局注册表断言/排空循环
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        // LaunchedEffect 需要 tokio 运行时上下文；渲染测试不依赖任务推进，仅需 Handle 存在
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::default_light();
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
        crate::animation::clear_all_animations();
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
    fn uncontained_renders_indicator_without_container() {
        let theme = ThemeColors::default_light();
        let (buf, w) = render_loading(|ctx| {
            LoadingIndicator::new().build(ctx);
        });
        let primary = theme.primary;
        let container = theme.primary_container;
        let mut found_primary = false;
        let mut found_container = false;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) {
                    found_primary = true;
                }
                if color_eq(container, p, 4) {
                    found_container = true;
                }
            }
        }
        assert!(found_primary, "uncontained 应绘制 primary indicator");
        assert!(!found_container, "uncontained 不应绘制 container 色");
    }

    #[test]
    fn contained_renders_container_and_indicator() {
        let theme = ThemeColors::default_light();
        let (buf, w) = render_loading(|ctx| {
            LoadingIndicator::new().contained(true).build(ctx);
        });
        let indicator = theme.primary;
        let container = theme.secondary_container;
        let mut found_indicator = false;
        let mut found_container = false;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(indicator, p, 4) {
                    found_indicator = true;
                }
                if color_eq(container, p, 4) {
                    found_container = true;
                }
            }
        }
        assert!(found_indicator, "contained 应绘制 indicator");
        assert!(found_container, "contained 应绘制 container");
    }

    // ── LoadingIndicatorNode 迁移（exp/wavy-node）──
    #[test]
    fn loading_node_key_covers_static_params() {
        use crate::modifier::DrawNode;
        let theme = ThemeColors::default_light();
        let mk = |contained: bool| LoadingIndicatorNode {
            is_contained: contained,
            container_shape: crate::modifier::Shape::Circle,
            indicator_color: theme.primary,
            container_color: theme.secondary_container,
            morph_progress: State::new(0.0),
            morph_index: State::new(0usize),
            morph_rotation_target: State::new(90.0),
            global_rotation: State::new(10.0),
        };
        assert_eq!(mk(true).node_key(), mk(true).node_key());
        assert_ne!(mk(true).node_key(), mk(false).node_key(), "is_contained 应进 key");
        // 动画值变化 → 相等（不进 key）
        let moved = LoadingIndicatorNode {
            is_contained: true,
            container_shape: crate::modifier::Shape::Circle,
            indicator_color: theme.primary,
            container_color: theme.secondary_container,
            morph_progress: State::new(0.7),
            morph_index: State::new(2usize),
            morph_rotation_target: State::new(180.0),
            global_rotation: State::new(300.0),
        };
        assert_eq!(mk(true).node_key(), moved.node_key(), "morph/旋转动画值不应进 key");
    }

    #[test]
    fn loading_node_renders_identical_to_enum_draw() {
        // 双路对照（首帧确定值：progress=0/index=0/rot=90/global=0）：node 路 vs
        // 旧闭包复刻，裸 leaf 同尺寸 surface 逐字节。
        let theme = ThemeColors::default_light();
        let render_leaf = |modifier: crate::modifier::Modifier| {
            let mut composer = Composer::new();
            let scene = |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let key = ctx.next_key();
                    ctx.start_leaf(key, modifier.clone());
                    ctx.end_node();
                });
            };
            composer.compose(scene);
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
            let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let node_mod = crate::modifier::Modifier::new()
            .size(40.0, 40.0)
            .draw_node(LoadingIndicatorNode {
                is_contained: false,
                container_shape: crate::modifier::Shape::Circle,
                indicator_color: theme.primary,
                container_color: Color::TRANSPARENT,
                morph_progress: State::new(0.0),
                morph_index: State::new(0usize),
                morph_rotation_target: State::new(90.0),
                global_rotation: State::new(0.0),
            });
        let enum_mod = crate::modifier::Modifier::new()
            .size(40.0, 40.0)
            .draw(move |canvas, rect| {
                let morph = &MORPH_SEQUENCE[0];
                let path = morph.to_path(0.0, 0, None, None, None, None);
                let path = progress_path(path, (rect.width(), rect.height()), *SHAPE_SCALE_FACTOR);
                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(skia_color(theme.primary));
                paint.set_style(skia_safe::paint::Style::Fill);
                canvas.save();
                canvas.translate((rect.left, rect.top));
                let center = skia_safe::Point::new(rect.width() / 2.0, rect.height() / 2.0);
                canvas.rotate(0.0 * 90.0 + 90.0 + 0.0, Some(center));
                canvas.draw_path(&path, &paint);
                canvas.restore();
            });
        let px_node = render_leaf(node_mod);
        let px_enum = render_leaf(enum_mod);
        assert_eq!(px_node.len(), 100 * 100);
        assert_eq!(px_node, px_enum, "loading node 路与旧 draw 路必须逐字节一致");
    }
}
