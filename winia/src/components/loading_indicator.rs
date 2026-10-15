//! Loading Indicator 组件 — 对标 M3 Expressive `LoadingIndicator` / `ContainedLoadingIndicator`
//!
//! M3 实现要点（对齐项）：
//! - **Two configurations**: Uncontained (`LoadingIndicator` — no container, the indicator is Primary)
//!   and Container (`.contained(true)` — a round container in PrimaryContainer with the indicator in
//!   OnPrimaryContainer);
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
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::effect::LaunchedEffect;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::theme::{ThemeColors, WiniaTheme};
use material_shapes::{MaterialShapes, Morph, MorphToPath, RoundedPolygon};
use std::sync::{Arc, LazyLock};
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

/// The two shapes a determinate indicator morphs between — `LoadingIndicatorDefaults
/// .DeterminateIndicatorPolygons`.
///
/// Upstream pre-rotates the circle by `360 / 20` degrees before normalising it ("Rotating the circle gets
/// us a smoother morphing to the soft-burst shapes"), which changes the vertex correspondence the morph
/// interpolates even though a circle looks identical either way. winia cannot reproduce that: the rotation
/// would have to go through `RoundedPolygon::transformed`, whose closure returns the crate's `Offset`, and
/// `material-shapes` keeps `mod offset` private (`lib.rs:17`). Unrotated, the morph still runs between the
/// same two shapes, just with a different vertex pairing.
static DETERMINATE_POLYGONS: LazyLock<[RoundedPolygon; 2]> = LazyLock::new(|| {
    [
        MaterialShapes::circle().normalized(),
        MaterialShapes::soft_burst().normalized(),
    ]
});

/// A determinate indicator runs ONE morph across its two shapes with no wrap-around: its progress travels
/// 0 → 1 once, where the indeterminate sequence loops through all seven shapes.
static DETERMINATE_MORPH: LazyLock<Morph<'static>> = LazyLock::new(|| {
    let polygons: &'static [RoundedPolygon] = &DETERMINATE_POLYGONS[..];
    Morph::new(&polygons[0], &polygons[1])
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
/// Call [`.contained()`](Self::contained) to switch to the **Container** configuration (M3's default):
/// - the round container is `PrimaryContainer`, the indicator `OnPrimaryContainer`
#[derive(Clone)]
pub struct LoadingIndicator {
    is_contained: bool,
    modifier: Modifier,
    indicator_color: Option<Color>,
    container_color: Option<Color>,
    container_shape: Shape,
    /// `Some(progress)` makes this a **determinate** indicator: it morphs between two shapes according to
    /// that 0..1 progress (values outside the range are coerced) and never spins. `None` is the ordinary
    /// indeterminate spinner.
    progress: Option<State<f32>>,
    /// A caller-supplied shape sequence in place of the built-in one — upstream's `polygons` parameter.
    /// `Arc` so the node can hold a copy without cloning the shapes every frame.
    shapes: Option<Arc<[RoundedPolygon]>>,
}

impl Default for LoadingIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadingIndicator {
    /// Create an uncontained **indeterminate** loading indicator (the spinner).
    pub fn new() -> Self {
        Self {
            is_contained: false,
            modifier: Modifier::new(),
            indicator_color: None,
            container_color: None,
            container_shape: LOADING_INDICATOR_CONTAINER_SHAPE,
            progress: None,
            shapes: None,
        }
    }

    /// Create a **determinate** loading indicator: it morphs between two shapes according to `progress`
    /// (0..1; values outside that range are coerced) instead of spinning.
    ///
    /// Upstream expresses this as an overload of `LoadingIndicator` whose first parameter is
    /// `progress: () -> Float`; winia keeps the builder and adds a constructor. Like upstream's determinate
    /// overload it has **no container** (`LoadingIndicatorImpl` is reached with `containerColor =
    /// Color.Unspecified`) and it does **not** rotate — the rotation in the default shape sequence is baked
    /// into the shapes, not animated.
    pub fn determinate(progress: State<f32>) -> Self {
        Self { progress: Some(progress), ..Self::new() }
    }

    /// Switch to the Container configuration (`ContainedLoadingIndicator`): a round container with the
    /// indicator inside it.
    ///
    /// The colours are the contained tokens — `PrimaryContainer` for the container and
    /// `OnPrimaryContainer` for the indicator (`LoadingIndicatorTokens.kt:26-27`). winia used to follow the
    /// M3 spec artwork instead (a Primary indicator over SecondaryContainer); use
    /// [`indicator_color`](Self::indicator_color) and [`container_color`](Self::container_color) to
    /// override either.
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

    /// Morph through a shape sequence of the caller's own instead of the built-in one — upstream's
    /// `polygons` parameter. The indeterminate indicator walks the sequence in order and wraps from the
    /// last shape back to the first; the determinate one takes its first two shapes as the ends of its
    /// single morph.
    ///
    /// Two shapes are the minimum, as upstream requires (it throws `IllegalArgumentException` for a shorter
    /// list). A shorter list is ignored here and the defaults stay in place — a panic at paint time would
    /// report the same mistake far worse.
    pub fn shapes(mut self, shapes: Vec<RoundedPolygon>) -> Self {
        if shapes.len() >= 2 {
            self.shapes = Some(Arc::from(shapes));
        }
        self
    }

    /// 当前是否 contained。
    pub fn is_contained(&self) -> bool {
        self.is_contained
    }

    /// Whether this is the determinate variant — see [`determinate`](Self::determinate).
    pub fn is_determinate(&self) -> bool {
        self.progress.is_some()
    }

    /// Default colour resolution, matching `LoadingIndicatorDefaults`.
    ///
    /// Uncontained takes `ActiveIndicatorColor` (Primary). Contained takes the contained pair:
    /// `ContainedActiveColor` / `ContainedContainerColor`, i.e. `OnPrimaryContainer` / `PrimaryContainer`
    /// (`LoadingIndicatorTokens.kt:24-27`). The M3 spec artwork draws a Primary indicator over a
    /// SecondaryContainer instead, and winia followed the artwork until now; from here it follows the
    /// tokens, so the values agree with what `LoadingIndicatorDefaults` actually resolves.
    fn resolve_colors(&self, theme: &ThemeColors) -> (Color, Color) {
        let default_indicator = if self.is_contained { theme.on_primary_container } else { theme.primary };
        let indicator_color = self.indicator_color.unwrap_or(default_indicator);
        let container_color = self.container_color.unwrap_or_else(|| {
            if self.is_contained { theme.primary_container } else { Color::TRANSPARENT }
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

        // Indeterminate only. The determinate variant is driven by its progress alone: it neither loops
        // through the shapes nor rotates, which is what upstream reaches `LoadingIndicatorImpl` with
        // (`containerColor = Color.Unspecified`, no rotation animation).
        let global_rotation = if self.progress.is_none() {
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
            Some(inf.animate_float(
                ctx,
                0.0,
                FULL_ROTATION,
                InfiniteRepeatableSpec::restart_tween(
                    Duration::from_millis(4666),
                    TweenSpec::new(Duration::from_millis(4666), interpolator::Linear::new()),
                ),
            ))
        } else {
            None
        };

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
                determinate: self.progress.clone(),
                shapes: self.shapes.clone(),
            });

        // The role is the same either way; the determinate variant reports no value, since winia's
        // semantics layer has no range to carry it.
        let m = m.semantics(crate::semantics::SemanticsConfig::new()
            .role(crate::semantics::SemanticsRole::ProgressBar));

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
pub(crate) struct LoadingIndicatorNode {
    pub(crate) is_contained: bool,
    pub(crate) container_shape: Shape,
    pub(crate) indicator_color: Color,
    pub(crate) container_color: Color,
    pub(crate) morph_progress: State<f32>,
    pub(crate) morph_index: State<usize>,
    pub(crate) morph_rotation_target: State<f32>,
    /// `None` for the determinate variant, which has no global spin to apply.
    pub(crate) global_rotation: Option<crate::runtime::state::Visual<f32>>,
    /// `Some(progress)` makes this the determinate variant: the morph `t` is that 0..1 value over the single
    /// determinate morph, and nothing rotates.
    pub(crate) determinate: Option<State<f32>>,
    /// The caller's shape sequence when they supplied one. Those polygons are not `'static`, so unlike the
    /// built-in sequences they cannot be baked into a cached `Morph` — the draw builds one per frame.
    pub(crate) shapes: Option<Arc<[RoundedPolygon]>>,
}

/// `RoundedPolygon` implements neither `Debug` nor `Clone` (material-shapes derives nothing on it), so a
/// derived impl would not compile once the node holds a caller's sequence. It is reported by length.
impl std::fmt::Debug for LoadingIndicatorNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadingIndicatorNode")
            .field("is_contained", &self.is_contained)
            .field("container_shape", &self.container_shape)
            .field("indicator_color", &self.indicator_color)
            .field("container_color", &self.container_color)
            .field("custom_shapes", &self.shapes.as_ref().map(|s| s.len()))
            .finish()
    }
}

impl crate::modifier::DrawNode for LoadingIndicatorNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        if self.is_contained && self.container_color.a != 0 {
            draw_container(canvas, rect, self.container_color, self.container_shape);
        }

        // Determinate: one morph across the two shapes, with `t` straight from the progress and nothing
        // rotating. Indeterminate: the seven-shape sequence at the loop's own progress, plus the 90°-per-cycle
        // step it accumulates and the global spin.
        let (path, total_rotation) = if let Some(progress) = &self.determinate {
            let t = progress.peek().clamp(0.0, 1.0);
            // Upstream also ROTATES the determinate indicator as far as the progress has come:
            // `val rotation = -progressValue * 180` with the comment "Rotate counterclockwise."
            // Skia's positive angle is already clockwise, so the sign carries over unchanged — the glyph
            // starts upright and ends upside down at full progress.
            let rotation = -t * 180.0;
            let path = match &self.shapes {
                // A caller's pair is not `'static`, so its single morph is built on the spot.
                Some(shapes) => Morph::new(&shapes[0], &shapes[1]).to_path(t, 0, None, None, None, None),
                None => DETERMINATE_MORPH.to_path(t, 0, None, None, None, None),
            };
            (path, rotation)
        } else {
            let current_index = self.morph_index.peek();
            let morph_progress = self.morph_progress.peek();
            let path = match &self.shapes {
                // Same reason as above: walk the caller's sequence in order, wrapping back to the first.
                Some(shapes) => {
                    let n = shapes.len();
                    let (a, b) = (&shapes[current_index % n], &shapes[(current_index + 1) % n]);
                    Morph::new(a, b).to_path(morph_progress, 0, None, None, None, None)
                }
                None => MORPH_SEQUENCE[current_index].to_path(morph_progress, 0, None, None, None, None),
            };
            let global_rotation = self.global_rotation.as_ref().map_or(0.0, |r| r.peek());
            let rotation = morph_progress * 90.0 + self.morph_rotation_target.peek() + global_rotation;
            (path, rotation)
        };
        let path = progress_path(path, (rect.width(), rect.height()), *SHAPE_SCALE_FACTOR);

        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(skia_color(self.indicator_color));
        paint.set_style(skia_safe::paint::Style::Fill);

        canvas.save();
        canvas.translate((rect.left, rect.top));
        let center = skia_safe::Point::new(rect.width() / 2.0, rect.height() / 2.0);
        canvas.rotate(total_rotation, Some(center));
        canvas.draw_path(&path, &paint);
        canvas.restore();
    }
    fn node_key(&self) -> String {
        // The static parameters, plus WHICH variant this is — indeterminate and determinate paint through
        // different paths. The animation values stay out, as before.
        format!(
            "loading:{:?}:{:?}:{:?}:{:?}:{}",
            self.is_contained,
            self.container_shape,
            self.indicator_color,
            self.container_color,
            self.determinate.is_some(),
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
            // Circle == Pill == Compose `RoundedCornerShape(50)`: percent corners against
            // the box being painted, so a non-square box is a stadium. A true circle left
            // the sides unpainted and disagreed with every other Circle site (review
            // round 3 found this as the last one).
            let r = rect.width().min(rect.height()) / 2.0;
            canvas.draw_round_rect(rect, r, r, &paint);
        }
        Shape::Pill => {
            let r = rect.width().min(rect.height()) / 2.0;
            canvas.draw_round_rect(rect, r, r, &paint);
        }
        Shape::PillToRounded { corner_radius, t } => {
            let percent = rect.width().min(rect.height()) / 2.0;
            let r = percent + (corner_radius - percent) * t;
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
        Shape::RightRoundedRect { radius } => {
            let rr = skia_safe::RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(radius, radius),
                skia_safe::Vector::new(radius, radius),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            // A contained indicator takes its container shape from the caller, so the per-corner
            // shape has to paint too — falling through to a rectangle would be a silent lie about
            // which shapes this component supports.
            let rr = skia_safe::RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(top_left, top_left),
                skia_safe::Vector::new(top_right, top_right),
                skia_safe::Vector::new(bottom_right, bottom_right),
                skia_safe::Vector::new(bottom_left, bottom_left),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        Shape::LeftRoundedRect { radius } => {
            let rr = skia_safe::RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(radius, radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(radius, radius),
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
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::graphics::Color;

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
        assert_eq!(ic, theme.on_primary_container, "contained indicator is ContainedActiveColor");
        assert_eq!(cc, theme.primary_container, "contained container is ContainedContainerColor");
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
    fn determinate_reports_its_variant_and_morphs_between_two_shapes() {
        let progress = State::new(0.0_f32);
        assert!(LoadingIndicator::determinate(progress.clone()).is_determinate());
        assert!(!LoadingIndicator::new().is_determinate(), "new() stays indeterminate");

        // The morph really answers to the progress: its two endpoints are the two different shapes.
        assert_eq!(DETERMINATE_POLYGONS.len(), 2);
        // `bounds()` borrows the path, so the paths have to outlive the comparison.
        let path_a = DETERMINATE_MORPH.to_path(0.0, 0, None, None, None, None);
        let path_b = DETERMINATE_MORPH.to_path(1.0, 0, None, None, None, None);
        let (a, b) = (path_a.bounds(), path_b.bounds());
        assert!(
            (a.width() - b.width()).abs() > 0.01 || (a.height() - b.height()).abs() > 0.01,
            "progress 0.0 and 1.0 must land on different shapes"
        );
    }

    /// The determinate variant drives its morph from the progress alone, so rendering it must leave the
    /// animation table EMPTY — the opposite of the indeterminate spinner, which registers a loop and a
    /// global rotation that never finish.
    #[test]
    fn a_determinate_indicator_does_not_spin() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let progress = State::new(0.4_f32);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(ThemeColors::default_light(), ctx, |ctx| {
                LoadingIndicator::determinate(progress.clone()).build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(
            !crate::animation::is_animating(),
            "a determinate indicator registers no animation of its own"
        );
        crate::animation::clear_all_animations();
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
        let indicator = theme.on_primary_container;
        let container = theme.primary_container;
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

    /// A caller-supplied sequence replaces the built-in one — upstream's `polygons` parameter.
    #[test]
    fn a_caller_supplied_shape_sequence_replaces_the_built_in_one() {
        // `RoundedPolygon` is not `Clone`, so this sequence is built once, compared with below, then moved
        // into the builder rather than cloned into it.
        let custom = vec![
            MaterialShapes::circle().normalized(),
            MaterialShapes::pill().normalized(),
        ];
        // It really drives the morph: the caller's pair does not end where the built-in one does.
        let built_in = DETERMINATE_MORPH.to_path(1.0, 0, None, None, None, None);
        let supplied = Morph::new(&custom[0], &custom[1]).to_path(1.0, 0, None, None, None, None);
        assert_ne!(
            built_in.bounds(),
            supplied.bounds(),
            "the caller's pair must not end where the built-in pair does"
        );

        let kept = LoadingIndicator::new().shapes(custom);
        assert_eq!(kept.shapes.as_ref().map(|s| s.len()), Some(2));

        // One shape is not a sequence. Upstream throws for that; here it is ignored and the defaults stay,
        // because a panic at paint time would report the same mistake far worse.
        let ignored = LoadingIndicator::new().shapes(vec![MaterialShapes::circle().normalized()]);
        assert!(ignored.shapes.is_none(), "a single shape is ignored");
    }

    // ── LoadingIndicatorNode 迁移（exp/wavy-node）──
    #[test]
    fn loading_node_key_covers_static_params() {
        use crate::modifier::DrawNode;
        let theme = ThemeColors::default_light();
        let mk = |contained: bool| LoadingIndicatorNode {
            is_contained: contained,
            container_shape: crate::graphics::Shape::Circle,
            indicator_color: theme.primary,
            container_color: theme.secondary_container,
            morph_progress: State::new(0.0),
            morph_index: State::new(0usize),
            morph_rotation_target: State::new(90.0),
            global_rotation: Some(crate::runtime::state::Visual::new(10.0)),
            determinate: None,
            shapes: None,
        };
        assert_eq!(mk(true).node_key(), mk(true).node_key());
        assert_ne!(mk(true).node_key(), mk(false).node_key(), "is_contained 应进 key");
        // 动画值变化 → 相等（不进 key）
        let moved = LoadingIndicatorNode {
            is_contained: true,
            container_shape: crate::graphics::Shape::Circle,
            indicator_color: theme.primary,
            container_color: theme.secondary_container,
            morph_progress: State::new(0.7),
            morph_index: State::new(2usize),
            morph_rotation_target: State::new(180.0),
            global_rotation: Some(crate::runtime::state::Visual::new(300.0)),
            determinate: None,
            shapes: None,
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
                container_shape: crate::graphics::Shape::Circle,
                indicator_color: theme.primary,
                container_color: Color::TRANSPARENT,
                morph_progress: State::new(0.0),
                morph_index: State::new(0usize),
                morph_rotation_target: State::new(90.0),
                global_rotation: Some(crate::runtime::state::Visual::new(0.0)),
                determinate: None,
                shapes: None,
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
