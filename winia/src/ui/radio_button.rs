//! RadioButton 组件 — 对标 material3 `RadioButton`（M3 1.4.0 / Compose androidx-main）
//!
//! M3 实现要点（对齐项）：
//! - 视觉 20×20（`RadioButtonTokens.IconSize`）：外圈 2dp 圆环描边 +
//!   选中时内实心圆点（半径 = DotSize/2 - 描边/2 = 5dp，`RadioButtonDotSize = 12dp`）；
//! - 触摸目标/状态层 40×40（`StateLayerSize`），波纹 unbounded（半径 20）；
//! - 选中动画：内点半径 0→5dp（Compose `animateDpAsState(FastSpatial)`）——
//!   本框架用 graphics_layer scale 0→1 近似（半径比例动画视觉等价）；
//!   颜色动画 `animateColorAsState(DefaultEffects)`——Tween 300ms；
//! - 颜色只分 enabled×selected（`RadioButtonColors` 四字段，无 hover/focus/press
//!   变体——交互反馈由 ripple/state layer 承担）；
//! - `onClick = null` → 不可交互（不挂 clickable），与 Compose 语义一致。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Color, GraphicsLayerParams, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 视觉图标尺寸（`RadioButtonTokens.IconSize = 20dp`）
pub const RADIO_BUTTON_SIZE: f32 = 20.0;
/// 状态层/触摸目标尺寸（`RadioButtonTokens.StateLayerSize = 40dp`）
pub const RADIO_TOUCH_TARGET: f32 = 40.0;
/// 圆环描边宽度（Compose `RadioStrokeWidth = 2dp`——对齐 M3 stroke 规格）
pub const RADIO_STROKE_WIDTH: f32 = 2.0;
/// 选中内点直径（Compose `RadioButtonDotSize = 12dp`；绘制半径 =
/// DotSize/2 - 描边/2 = 5dp——外圈内缘 8dp 与内点 5dp 间 3dp 间隙）
pub const RADIO_DOT_SIZE: f32 = 12.0;

/// 单选按钮颜色集（对标 material3 `RadioButtonColors`）——selected/unselected ×
/// enabled/disabled 四组色。M3 `RadioButtonColors` 无 hover/focus/press 变体。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadioButtonColors {
    pub selected_color: Color,
    pub unselected_color: Color,
    pub disabled_selected_color: Color,
    pub disabled_unselected_color: Color,
}

impl RadioButtonColors {
    pub fn new(
        selected_color: Color,
        unselected_color: Color,
        disabled_selected_color: Color,
        disabled_unselected_color: Color,
    ) -> Self {
        Self {
            selected_color,
            unselected_color,
            disabled_selected_color,
            disabled_unselected_color,
        }
    }

    /// 从主题推导默认色（对标 `RadioButtonDefaults.colors()`：
    /// `RadioButtonTokens` v0_117）
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        Self::new(
            theme.primary,
            theme.on_surface_variant,
            // Disabled：OnSurface @ 0.38（Disabled*IconOpacity = 0.38f）
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
        )
    }

    /// 按 enabled×selected 解析当前色（对标 M3 `radioColor(enabled, selected)`）
    pub fn radio_color(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            if selected {
                self.disabled_selected_color
            } else {
                self.disabled_unselected_color
            }
        } else if selected {
            self.selected_color
        } else {
            self.unselected_color
        }
    }
}

/// RadioButton 默认值（对标 material3 `RadioButtonDefaults`）
pub struct RadioButtonDefaults;

impl RadioButtonDefaults {
    pub fn radio_button_colors(theme: &ThemeColors) -> RadioButtonColors {
        RadioButtonColors::from_theme(theme)
    }

    /// 状态层/视觉形状（RadioButtonTokens 为 Circle——与 checkbox 的
    /// CheckboxDefaults::shape() 对应）
    pub fn shape() -> Shape {
        Shape::Circle
    }
}

/// 共享实现（对标 M3 `RadioButtonImpl`）。
/// #[composable]：内部圆环/内点叶子节点语句注入——多实例隔离
#[allow(clippy::too_many_arguments)]
#[composable]
fn radio_button_impl(
    ctx: &mut ComposeCtx,
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<RadioButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
) {
    ctx.changed(&selected);
    ctx.changed(&enabled);
    ctx.changed(&colors);
    let key = ctx.next_key();
    let theme = WiniaTheme::colors();
    let colors = colors.unwrap_or_else(|| RadioButtonDefaults::radio_button_colors(&theme));
    let interaction = interaction_source
        .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
    let color = colors.radio_color(enabled, selected);

    // 颜色动画：Tween 300ms Linear（对齐 checkbox；Compose
    // `animateColorAsState(MotionScheme.DefaultEffects)` = 300ms tween，
    // `push_animatable_color` 会把 Spring 降级为 `TweenSpec::default()`）。
    let color_spec =
        crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::default());
    let color_anim = ctx.animate_color_as_state(color, color_spec);

    // 内点出现/消失动画：scale 0↔1（Spring StiffnessMedium(400)/NoBouncy——对齐
    // checkbox 勾号规格；Compose `animateDpAsState(MotionScheme.FastSpatial)` 为
    // spring(600/30)，winia 无直接等价，用 M3 基准级近似——仅浮点动画生效）
    let dot_spec = crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
        stiffness: crate::animation::SpringSpec::STIFFNESS_MEDIUM,
        ..crate::animation::SpringSpec::default()
    });
    let dot_scale = ctx.animate_float_as_state(
        if selected { 1.0 } else { 0.0 },
        dot_spec,
    );

    let mut m = Modifier::new()
        .size(RADIO_TOUCH_TARGET, RADIO_TOUCH_TARGET)
        // 状态层形状 = Circle（焦点环跟随圆形；波纹 unbounded 不受 clip 影响）
        .clip(RadioButtonDefaults::shape());
    if enabled {
        if let Some(on_click) = on_click {
            m = m
                .clickable_with_source(&interaction, move || on_click())
                // M3：ripple(bounded = false, radius = StateLayerSize / 2)；
                // 本框架 ripple 无 radius 参数，半径由节点尺寸隐式决定
                .ripple(&interaction, theme.on_surface, false);
        }
    }
    m = m.then(modifier);

    match ctx.start_restartable_group(
        key,
        m,
        BoxLayout::new().alignment(crate::layout::Alignment::Center),
    ) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            // 视觉 20×20：圆环（border_dynamic 2dp Circle 描边——外圈中径
            // (20-2)/2 = 9dp，与 Compose drawCircle(radius = IconSize/2 -
            // strokeWidth/2, Stroke) 一致）+ 内点（background 实心圆）。
            let ring_color = color_anim.clone();
            let visual = Modifier::new()
                .size(RADIO_BUTTON_SIZE, RADIO_BUTTON_SIZE)
                .border_dynamic(
                    RADIO_STROKE_WIDTH,
                    move || ring_color.peek(),
                    Shape::Circle,
                );
            let vkey = ctx.next_key();
            match ctx.start_restartable_group(
                vkey,
                visual,
                BoxLayout::new().alignment(crate::layout::Alignment::Center),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 内点：10×10 实心圆（半径 5 = DotSize/2 - 描边/2），
                    // scale 0→1 动画（Compose 半径 0→5dp 动画的视觉等价——
                    // 同心圆从 0 放大到最终半径）。未选中静止态 scale=0 隐藏。
                    let k = ctx.next_key();
                    let scale = dot_scale.clone();
                    let c = color_anim.clone();
                    ctx.start_leaf(
                        k,
                        Modifier::new()
                            .size(RADIO_DOT_SIZE - RADIO_STROKE_WIDTH, RADIO_DOT_SIZE - RADIO_STROKE_WIDTH)
                            .background(move || c.peek(), Shape::Circle)
                            .graphics_layer(move || GraphicsLayerParams {
                                scale_x: scale.get(),
                                scale_y: scale.get(),
                                ..Default::default()
                            }),
                    );
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        }
    }
    ctx.set_current_node_focus_color(theme.primary);
    ctx.end_restartable_group();
}

/// RadioButton 组件 Builder（对标 material3 `RadioButton(selected, onClick,
/// modifier, enabled, colors, interactionSource)`）
pub struct RadioButton {
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<RadioButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl RadioButton {
    pub fn new(selected: bool) -> Self {
        Self {
            selected,
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// 点击回调。None → 不可交互（不挂 clickable，对标 Compose onClick = null）。
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: RadioButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 注入交互源（hoist——press/hover/focus 状态发射到此源）
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        radio_button_impl(
            ctx,
            self.selected,
            self.on_click,
            self.enabled,
            self.colors,
            self.interaction_source,
            self.modifier,
        );
    }

    pub fn get_selected(&self) -> bool {
        self.selected
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<RadioButtonColors> {
        self.colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = RadioButtonDefaults::radio_button_colors(&theme);
        // Selected：Primary；Unselected：OnSurfaceVariant
        assert_eq!(colors.radio_color(true, true), theme.primary);
        assert_eq!(colors.radio_color(true, false), theme.on_surface_variant);
        // Disabled：OnSurface @ 38%（两态同值）
        assert_eq!(colors.radio_color(false, true), colors.disabled_selected_color);
        assert_eq!(colors.radio_color(false, false), colors.disabled_unselected_color);
        assert_eq!(colors.disabled_selected_color, colors.disabled_unselected_color);
        assert_eq!(colors.disabled_selected_color.a, (theme.on_surface.a as f32 * 0.38) as u8);
        // 自定义色覆盖
        let custom = RadioButtonColors::new(
            Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128),
        );
        assert_eq!(custom.radio_color(true, true), Color::RED);
        assert_eq!(custom.radio_color(true, false), Color::BLUE);
        assert_eq!(custom.radio_color(false, true), Color::from_argb(255, 128, 128, 128));
    }

    // ── 渲染级像素测试（对齐 checkbox 模式：raster surface + 像素断言）──
    // 约定：单层布局，node.position 即全局像素坐标；40×40 触摸目标内
    // 视觉层 20×20 居中（偏移 +10,+10），圆环描边中径半径 9（真圆环点
    // 距圆心 9：顶部 (20,11)/左侧 (11,20)），内点 10×10 居中（全局中心
    // (20,20)）。
    struct RenderScene {
        px: Vec<[u8; 4]>,
        w: usize,
        h: usize,
        origin: (f32, f32), // 40×40 触摸目标左上角
    }

    fn render_radio(selected: bool, enabled: bool, colors: RadioButtonColors) -> RenderScene {
        use skia_safe::{Color as SkColor, surfaces};
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            RadioButton::new(selected)
                .enabled(enabled)
                .colors(colors)
                .on_click(|| {})
                .build(ctx);
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // 找 40×40 触摸目标
        let mut origin = (0.0f32, 0.0f32);
        let mut found = false;
        for node in nodes {
            if node.measured_size.width == RADIO_TOUCH_TARGET
                && node.measured_size.height == RADIO_TOUCH_TARGET
            {
                origin = (node.position.x, node.position.y);
                found = true;
                break;
            }
        }
        assert!(found, "应有 40×40 触摸目标");
        RenderScene { px: px.to_vec(), w: 300, h: 300, origin }
    }

    impl RenderScene {
        /// 像素读取：raster_n32_premul 小端为 BGRA 布局——按 (R,G,B) 语义读
        fn at(&self, x: f32, y: f32) -> (i32, i32, i32) {
            let p = self.px[(y as usize) * self.w + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        }
        fn is_white(&self, x: f32, y: f32) -> bool {
            let p = self.at(x, y);
            p.0 > 245 && p.1 > 245 && p.2 > 245
        }
    }

    fn is_close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 6 && (a.1 - b.1).abs() <= 6 && (a.2 - b.2).abs() <= 6
    }

    #[test]
    fn selected_renders_ring_and_dot() {
        // 选中：圆环 + 内点都是 selected 色（红），圆环外角为白
        let scene = render_radio(
            true,
            true,
            RadioButtonColors::new(Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128)),
        );
        let (ox, oy) = scene.origin;
        // 中心 (20,20)：内点红
        assert!(
            is_close(scene.at(ox + 20.0, oy + 20.0), (255, 0, 0)),
            "选中态中心应为内点色（红）"
        );
        // 圆环顶部中径 (20,11)（距离圆心 9）：红
        assert!(
            is_close(scene.at(ox + 20.0, oy + 11.0), (255, 0, 0)),
            "选中态圆环顶部应为红（实际 {:?}）",
            scene.at(ox + 20.0, oy + 11.0)
        );
        // 圆环左侧中径 (11,20)：红
        assert!(
            is_close(scene.at(ox + 11.0, oy + 20.0), (255, 0, 0)),
            "选中态圆环左侧应为红"
        );
        // 视觉层外角 (11,11)：白（圆环外）
        assert!(scene.is_white(ox + 11.0, oy + 11.0), "圆环外角应为白");
    }

    #[test]
    fn unselected_renders_ring_only() {
        // 未选中：圆环 unselected 色（蓝），中心白（无内点）
        let scene = render_radio(
            false,
            true,
            RadioButtonColors::new(Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128)),
        );
        let (ox, oy) = scene.origin;
        // 圆环中径半径 9：顶部 (20,11) / 左侧 (11,20) 距离圆心 9——真圆环点
        assert!(
            is_close(scene.at(ox + 20.0, oy + 11.0), (0, 0, 255)),
            "未选中态圆环顶部应为 unselected 色（蓝）（实际 {:?}）",
            scene.at(ox + 20.0, oy + 11.0)
        );
        assert!(
            is_close(scene.at(ox + 11.0, oy + 20.0), (0, 0, 255)),
            "未选中态圆环左侧应为 unselected 色（蓝）"
        );
        assert!(scene.is_white(ox + 20.0, oy + 20.0), "未选中态中心应为白（无内点）");
    }

    #[test]
    fn disabled_uses_disabled_color() {
        // 禁用：圆环/内点都用 disabled 色（绿）
        let scene = render_radio(
            true,
            false,
            RadioButtonColors::new(Color::RED, Color::BLUE, Color::GREEN, Color::from_argb(255, 128, 128, 128)),
        );
        let (ox, oy) = scene.origin;
        let dc = scene.at(ox + 20.0, oy + 20.0);
        assert!(
            is_close(dc, (0, 255, 0)),
            "禁用选中态中心应为 disabled 色（绿）（实际 {dc:?}）"
        );
        assert!(
            is_close(scene.at(ox + 20.0, oy + 11.0), (0, 255, 0)),
            "禁用选中态圆环顶部应为 disabled 色（绿）（实际 {:?}）",
            scene.at(ox + 20.0, oy + 11.0)
        );
    }

    #[test]
    fn deselect_transition_keeps_selected_color() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 状态切换驱动：点击回调里翻转 selected
        let sel = crate::core::state::State::new(true);
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let s = sel.clone();
            RadioButton::new(sel.get())
                .colors(RadioButtonColors::new(Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128)))
                .on_click(move || s.update(|v| *v = !*v))
                .build(ctx);
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // 触发取消选中（状态立即切 false，但颜色动画尚未推进）
        sel.update(|v| *v = false);
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // 找触摸目标中心
        let mut center = (0.0f32, 0.0f32);
        for node in nodes {
            if node.measured_size.width == RADIO_TOUCH_TARGET {
                center = (node.position.x + 20.0, node.position.y + 20.0);
                break;
            }
        }
        let at = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
        };
        // 过渡帧：内点必须仍保留 selected 色（红）——颜色动画在状态切换后
        // 保持旧值直到动画推进（animate_color_as_state 从旧色过渡），防未来
        // 改动把颜色瞬切（吞掉退出动画）
        assert!(
            is_close(at(center.0, center.1), (255, 0, 0)),
            "取消选中过渡帧内点应保留 selected 色（实际 {:?}）——颜色瞬切吞掉退出动画",
            at(center.0, center.1)
        );
        // 动画推完 → 未选中静止态：中心白
        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at2 = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
        };
        let c = at2(center.0, center.1);
        assert!(
            c.0 > 245 && c.1 > 245 && c.2 > 245,
            "未选中静止态中心应为白（实际 {c:?}）"
        );
    }

}
