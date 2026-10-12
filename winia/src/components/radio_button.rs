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

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, GraphicsLayerParams, Shape};
use crate::interaction::MutableInteractionSource;
use crate::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 视觉图标尺寸（`RadioButtonTokens.IconSize = 20dp`）
pub const RADIO_BUTTON_SIZE: f32 = 20.0;
/// The node a CLICKABLE radio occupies: `Modifier.minimumInteractiveComponentSize()` = 48 dp, with the 20 dp
/// icon centred inside (`material3/RadioButton.kt:100-115`; applied whenever `onClick != null`, disabled or not).
pub const RADIO_TOUCH_TARGET: f32 = 48.0;
/// `RadioButtonTokens.StateLayerSize = 40dp` — the DIAMETER of the hover/focus halo, which is what Compose hands
/// to `ripple` as its radius. It is smaller than the node, so it is named rather than derived from the node's
/// diagonal.
pub const RADIO_STATE_LAYER_SIZE: f32 = 40.0;
/// The node a radio WITHOUT a click handler occupies: the 20 dp icon plus Compose's 2 dp padding on each side
/// (`RadioButtonPadding`, `material3/RadioButton.kt:264`), and no minimum interactive size.
pub const RADIO_IDLE_SIZE: f32 = RADIO_BUTTON_SIZE + 4.0;
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

/// The radio's colour spec: M3 animates it with `animateColorAsState(target,
/// MotionSchemeKeyTokens.DefaultEffects.value())` (`material3/RadioButton.kt:237`) — a critically damped
/// spring (1.0 / 1600), not a 300 ms linear tween, which is what this component used while the engine
/// could not put a colour on a spring at all. Named so the token is assertable.
fn radio_colour_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::effects_default()
}

/// The dot's appearance spec: M3 animates the dot radius with the `FastSpatial` token
/// (`material3/RadioButton.kt:86`: `animateDpAsState(targetValue = … RadioButtonDotSize / 2 else 0.dp,
/// animationSpec = MotionSchemeKeyTokens.FastSpatial.value())`) — damping 0.9, stiffness 1400
/// (`tokens/StandardMotionTokens.kt:24-25`). winia used a no-bouncy spring at stiffness 400 and a
/// comment quoting "spring(600/30)", neither of which is this token.
fn radio_dot_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::spatial_fast()
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

    // The radio's colour, per M3 — see `radio_colour_spec`.
    let color_spec = crate::animation::AnimationSpec::Spring(radio_colour_spec());
    let color_anim = ctx.animate_color_as_state(color, color_spec);

    // The dot's appearance, which M3 animates with the `FastSpatial` token — see `radio_dot_spec`.
    let dot_scale = ctx.animate_float_as_state(
        if selected { 1.0 } else { 0.0 },
        crate::animation::AnimationSpec::Spring(radio_dot_spec()),
    );

    let mut m = Modifier::new()
        // Size: Compose applies `Modifier.minimumInteractiveComponentSize()` in front of the selectable
        // modifier ONLY when the radio takes clicks (`material3/RadioButton.kt:100-115`), so an interactive
        // radio is a 48 dp node with its 20 dp icon centred, and one without a click handler is just the icon
        // plus Compose's 2 dp padding. winia used `StateLayerSize` (40 dp) for both — the same 8 dp-short
        // target the checkbox had.
        .size(
            if on_click.is_some() { RADIO_TOUCH_TARGET } else { RADIO_IDLE_SIZE },
            if on_click.is_some() { RADIO_TOUCH_TARGET } else { RADIO_IDLE_SIZE },
        )
        // The ring's SHAPE, carried without clipping: the renderer infers it from the nearest
        // Background/Border/Clip and strokes the ring OUTSIDE the node's box, so a `clip` here cut the ring away
        // (the reported "the focus ring is gone") — the same trap the Button's Text style hit. A zero-width
        // transparent border states the shape and clips nothing.
        .border(
            0.0,
            crate::graphics::Color::from_argb(0, 0, 0, 0),
            RadioButtonDefaults::shape(),
        );
    if enabled {
        if let Some(on_click) = on_click {
            m = m
                .clickable_with_source(&interaction, move || on_click())
                // `RadioButton.kt:97`: `ripple(bounded = false, radius = RadioButtonTokens.StateLayerSize / 2)`
                // — the halo is `StateLayerSize` (40 dp), SMALLER than the 48 dp node, so it has to be named
                // rather than derived from the node. The spreading layer still grows to the node's diagonal.
                .ripple_with_state_layer_size(
                    &interaction,
                    theme.on_surface,
                    false,
                    RADIO_STATE_LAYER_SIZE,
                );
        }
    }
    // Accessibility: a radio is SELECTED, not checked (Compose's `selectable` → `Selected`).
    m = m.semantics(
        crate::semantics::SemanticsConfig::new()
            .role(crate::semantics::SemanticsRole::RadioButton)
            .merge_descendants(true)
            .state(crate::semantics::SemanticsState::new().selected(selected).enabled(enabled)),
    );
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

    /// M3's `FastSpatial` spring (0.9/1400) for the dot, pinned — a no-bouncy 400 approximation fails.
    #[test]
    fn the_dot_spec_is_m3s_fast_spatial_spring() {
        let spec = radio_dot_spec();
        assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
        assert_eq!(spec.damping_ratio, 0.9);
        assert_eq!(spec.stiffness, 1400.0);
    }

    /// M3's `DefaultEffects` (spring 1.0/1600) and nothing else: a silent return to a tween fails here.
    #[test]
    fn the_colour_spec_is_m3s_default_effects_spring() {
        let spec = radio_colour_spec();
        assert_eq!(spec, crate::animation::SpringSpec::effects_default());
        assert_eq!(spec.damping_ratio, 1.0);
        assert_eq!(spec.stiffness, 1600.0);
    }

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
    // `origin` is the touch target's OWN top-left corner: its 20 dp visual is centred inside, so the samples
    // below (the visual's centre at +20,+20, the ring's mid-stroke at radius 9 from that centre) hold for any
    // node size — 40 dp before, 48 dp now.
    struct RenderScene {
        px: Vec<[u8; 4]>,
        w: usize,
        origin: (f32, f32), // the touch target's top-left corner
    }

    fn render_radio(selected: bool, enabled: bool, colors: RadioButtonColors) -> RenderScene {
        use skia_safe::{Color as SkColor, surfaces};
        let mut composer = crate::runtime::composer::Composer::new();
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
        let origin = radio_frame_origin(&composer).expect("the 48x48 touch target is in the tree");
        RenderScene { px: px.to_vec(), w: 300, origin }
    }

    /// The frame every sample offset in these tests is written against: the node's CENTRE mapped onto the old
    /// 40 dp frame, so `+20,+20` is the visual's centre and `±9` its ring's mid-stroke, whatever the node size
    /// is now. Positions in the arena are parent-relative, so they are accumulated from the root — the tests
    /// that used to add a bare `+20` sampled the wrong pixel once the node became 48 dp.
    fn radio_frame_origin(composer: &crate::runtime::composer::Composer) -> Option<(f32, f32)> {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            ox: f32,
            oy: f32,
        ) -> Option<(f32, f32)> {
            let node = &nodes[idx];
            let x = ox + node.position.x;
            let y = oy + node.position.y;
            if node.measured_size.width == RADIO_TOUCH_TARGET {
                // The sample offsets below are written against the OLD 40 dp frame (its centre at +20,+20, the
                // ring's mid-stroke 9 dp from it), so rebase the node's centre onto that frame.
                let half = 20.0;
                return Some((
                    x + node.measured_size.width / 2.0 - half,
                    y + node.measured_size.height / 2.0 - half,
                ));
            }
            for &child in &node.children {
                if let Some(hit) = walk(nodes, child, x, y) {
                    return Some(hit);
                }
            }
            None
        }
        let root = composer.layout_root_idx()?;
        walk(composer.arena_nodes(), root, 0.0, 0.0)
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

    /// The node an interactive radio occupies, and the state layer it names.
    ///
    /// `material3/RadioButton.kt:100-115` applies `Modifier.minimumInteractiveComponentSize()` (48 dp) whenever
    /// the radio takes clicks, with its 20 dp icon centred; a radio with no click handler is 24 dp (no minimum).
    /// The halo is `StateLayerSize` (40 dp, `:97`) — smaller than the node, so it travels as a NAMED size while
    /// the spreading layer still reaches the node's diagonal.
    #[test]
    fn an_interactive_radio_reserves_48_dp_and_names_a_40_dp_state_layer() {
        let geometry = |interactive: bool| -> (f32, bool, usize, Option<f32>) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        let rb = RadioButton::new(false);
                        let rb = if interactive { rb.on_click(|| {}) } else { rb };
                        rb.build(ctx);
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let nodes = composer.arena_nodes();
            let clickables = nodes
                .iter()
                .filter(|n| {
                    n.modifier
                        .elements()
                        .iter()
                        .any(|el| matches!(el, crate::modifier::ModifierElement::Clickable { .. }))
                })
                .count();
            let node = nodes
                .iter()
                .find(|n| {
                    n.measured_size.width == RADIO_TOUCH_TARGET
                        || n.measured_size.width == RADIO_IDLE_SIZE
                })
                .expect("the radio node");
            let has_ripple = node
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, crate::modifier::ModifierElement::Ripple { .. }));
            let state_layer = node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::Ripple { state_layer_diameter, .. } => {
                    *state_layer_diameter
                }
                _ => None,
            });
            (node.measured_size.width, has_ripple, clickables, state_layer)
        };

        let (size, has_ripple, clickables, state_layer) = geometry(true);
        assert_eq!(size, 48.0, "an interactive radio reserves 48 dp");
        assert_eq!(clickables, 1, "and carries one clickable node");
        assert!(has_ripple, "with an unbounded ripple");
        assert_eq!(
            state_layer,
            Some(RADIO_STATE_LAYER_SIZE),
            "and names a 40 dp state layer, smaller than the node"
        );

        let (size, has_ripple, clickables, _) = geometry(false);
        assert_eq!(size, RADIO_IDLE_SIZE, "with no click handler the minimum is not applied");
        assert_eq!(clickables, 0, "and nothing is clickable");
        assert!(!has_ripple, "and there is no ripple");
    }

    /// A focused radio paints its focus ring OUTSIDE its box, and an unfocused one does not.
    ///
    /// The ring is drawn 3 dp wide with its centre line `gap (2) + 1.5` outside the node's rect, and the node
    /// used to carry `clip(Circle)` — which the renderer sets up BEFORE the ring, so it cut the ring away. That
    /// is the "the focus ring is gone" report: the shape now travels as a zero-width transparent border, which
    /// states the shape without clipping (measured here at the ring's mid-stroke radius).
    #[test]
    fn a_focused_radio_paints_its_ring_outside_the_node() {
        use skia_safe::{Color as SkColor, surfaces};
        let pixels = |focused: bool| -> Vec<[u8; 4]> {
            let mut composer = crate::runtime::composer::Composer::new();
            let source = MutableInteractionSource::new();
            let src = source.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                RadioButton::new(false)
                    .on_click(|| {})
                    .interaction_source(src.clone())
                    .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            if focused {
                source.emit_focus();
            }
            composer.compose(move |ctx: &mut ComposeCtx| {
                RadioButton::new(false)
                    .on_click(|| {})
                    .interaction_source(source.clone())
                    .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            crate::render::render(composer.arena_nodes(), root, canvas);
            // Real frames so the ring's fade-in is complete.
            for _ in 0..40 {
                if !crate::animation::update_animations() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let mut surface2 = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas2 = surface2.canvas();
            canvas2.clear(SkColor::WHITE);
            crate::render::render(composer.arena_nodes(), root, canvas2);
            let pm = surface2.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };

        // The ring's centre line: half the node plus gap + half the 3 dp stroke.
        let ring_radius = RADIO_TOUCH_TARGET / 2.0 + 2.0 + 1.5;
        let centre = RADIO_TOUCH_TARGET / 2.0;
        let ring_pixel = |px: &[[u8; 4]], angle_deg: f32| -> (i32, i32, i32) {
            let a = angle_deg.to_radians();
            let x = (centre + ring_radius * a.cos()) as usize;
            let y = (centre + ring_radius * a.sin()) as usize;
            let p = px[y * 300 + x];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        let is_white = |p: (i32, i32, i32)| p.0 > 245 && p.1 > 245 && p.2 > 245;

        let unfocused = pixels(false);
        assert!(
            is_white(ring_pixel(&unfocused, 0.0)) && is_white(ring_pixel(&unfocused, 180.0)),
            "no ring while the radio is not focused"
        );

        let focused = pixels(true);
        let on_ring = [ring_pixel(&focused, 0.0), ring_pixel(&focused, 180.0), ring_pixel(&focused, 90.0)];
        assert!(
            on_ring.iter().any(|p| !is_white(*p)),
            "the focused radio paints its ring at radius {ring_radius} (sampled {on_ring:?})"
        );
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
        let sel = crate::runtime::state::State::new(true);
        let mut composer = crate::runtime::composer::Composer::new();
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
        // The touch target centre, in the same 40 dp reference frame (`radio_frame_origin`).
        let frame = radio_frame_origin(&composer).expect("the touch target is in the tree");
        let center = (frame.0 + 20.0, frame.1 + 20.0);
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
