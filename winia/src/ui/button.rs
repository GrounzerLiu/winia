//! Button 组件 — 可点击按钮
//!
//! 用法:
//! ```ignore
//! Button::new()
//!     .on_click(|| println!("clicked"))
//!     .modifier(Modifier::new().size(200.0, 48.0).background(Color::BLUE))
//!     .build(ctx, |ctx| {
//!         Text::new("Click me").color(Color::WHITE).build(ctx);
//!     });
//! ```

use crate::core::composer::ComposeCtx;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier, Shape, SizeValue};
use crate::ui::interaction::{ComponentState, MutableInteractionSource};
use std::sync::Arc;
use std::fmt;

/// Button 变体风格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// 实心填充按钮（主要操作）
    Filled,
    /// 轮廓按钮（次要操作）
    Outlined,
    /// 纯文本按钮（最低强调）
    Text,
    /// 带浅色背景的填充按钮
    Tonal,
}

/// 按钮颜色集（对标 material3 `ButtonColors`）——container/content 各含
/// enabled/disabled 变体；`container_color(enabled)` / `content_color(enabled)`
/// 按状态取色（禁用：默认 50% alpha 近似 Compose 12%/38% 变体）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonColors {
    pub container: crate::modifier::Color,
    pub content: crate::modifier::Color,
    pub disabled_container: crate::modifier::Color,
    pub disabled_content: crate::modifier::Color,
}

impl ButtonColors {
    pub fn new(
        container: crate::modifier::Color,
        content: crate::modifier::Color,
        disabled_container: crate::modifier::Color,
        disabled_content: crate::modifier::Color,
    ) -> Self {
        Self { container, content, disabled_container, disabled_content }
    }

    /// 按启用状态取容器色
    pub fn container_color(&self, enabled: bool) -> crate::modifier::Color {
        self.container_color_for(&ComponentState { enabled, ..ComponentState::idle() })
    }

    /// 按启用状态取内容（文字）色
    pub fn content_color(&self, enabled: bool) -> crate::modifier::Color {
        self.content_color_for(&ComponentState { enabled, ..ComponentState::idle() })
    }

    /// 状态化容器色——仅区分 enabled/disabled（与 material3 ButtonColors 一致；
    /// hover/focus/press 视觉由 indication（ripple）状态层绘制，避免双重叠加）。
    pub fn container_color_for(&self, state: &ComponentState) -> crate::modifier::Color {
        if !state.enabled {
            self.disabled_container
        } else {
            self.container
        }
    }

    /// 状态化内容色（内容色仅区分 enabled/disabled——与 material3 ButtonColors 一致）
    pub fn content_color_for(&self, state: &ComponentState) -> crate::modifier::Color {
        if state.enabled { self.content } else { self.disabled_content }
    }

    /// 从主题按 style 生成默认色（Compose ButtonDefaults.buttonColors 对标）
    pub fn from_theme(theme: &crate::ui::theme::ThemeColors, style: ButtonStyle) -> Self {
        use crate::modifier::Color;
        let (container, content) = match style {
            ButtonStyle::Filled => (theme.primary, theme.on_primary),
            ButtonStyle::Tonal => (theme.secondary_container, theme.on_secondary_container),
            ButtonStyle::Outlined => (Color::from_argb(0, 0, 0, 0), theme.primary),
            ButtonStyle::Text => (Color::from_argb(0, 0, 0, 0), theme.primary),
        };
        // 禁用变体：容器 50% alpha、内容 50% alpha（近似 Compose 12%/38%）
        let disabled_container = Color::from_argb(
            (container.a as f32 * 0.5) as u8, container.r, container.g, container.b,
        );
        let disabled_content = Color::from_argb(
            (content.a as f32 * 0.5) as u8, content.r, content.g, content.b,
        );
        Self::new(container, content, disabled_container, disabled_content)
    }
}

/// 按钮阴影高度（对标 material3 `ButtonElevation`）——各交互状态取不同 elevation，
/// 由 `Button::elevation` 应用为 `Modifier.shadow`。默认 FilledButton 全 0
/// （M3 tokens）；`ButtonElevation::elevated()` 给出 ElevatedButton 近似值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub disabled: f32,
}

impl ButtonElevation {
    pub fn new(default: f32, pressed: f32, focused: f32, hovered: f32, disabled: f32) -> Self {
        Self { default, pressed, focused, hovered, disabled }
    }

    /// M3 FilledButton 默认：全 0（阴影由用户显式配置）
    pub fn default_elevation() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0)
    }

    /// ElevatedButton 近似（rest 1 / pressed 8 / focused 3 / hovered 4 / disabled 0——
    /// hover 明显高于 rest，悬停即可见阴影升高；变化经动画平滑过渡）
    pub fn elevated() -> Self {
        Self::new(1.0, 8.0, 3.0, 4.0, 0.0)
    }

    /// 按状态取 elevation（优先级 disabled > pressed > dragged > hovered > focused > default——
    /// 与 material3 的"最近交互优先"一致）
    pub fn for_state(&self, state: &ComponentState) -> f32 {
        if !state.enabled {
            self.disabled
        } else if state.pressed {
            self.pressed
        } else if state.dragged {
            self.pressed.max(self.hovered)
        } else if state.hovered {
            self.hovered
        } else if state.focused {
            self.focused
        } else {
            self.default
        }
    }
}

impl Default for ButtonElevation {
    fn default() -> Self {
        Self::default_elevation()
    }
}

impl Default for ButtonStyle {
    fn default() -> Self {
        ButtonStyle::Filled
    }
}

/// 按钮边框（对标 material3 `BorderStroke`）——宽度 + 颜色；
/// 形状由 [`Button::shape`] 决定（画在容器形状边缘）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonBorder {
    pub width: f32,
    pub color: crate::modifier::Color,
}

impl ButtonBorder {
    pub fn new(width: f32, color: crate::modifier::Color) -> Self {
        Self { width, color }
    }
}

/// Button 默认值（对标 material3 `ButtonDefaults`——M3 small button tokens）
pub struct ButtonDefaults;

impl ButtonDefaults {
    /// 默认形状：胶囊（对标 `ButtonSmallTokens.ContainerShapeRound` = CornerFull）
    pub fn shape() -> Shape {
        Shape::pill()
    }

    /// 默认最小宽度（对标 `ButtonDefaults.MinWidth` 58.dp）
    pub fn min_width() -> f32 {
        58.0
    }

    /// 默认最小高度（对标 `ButtonDefaults.MinHeight` 40.dp——ContainerHeight）
    pub fn min_height() -> f32 {
        40.0
    }

    /// 默认内容 padding `(start, top, end, bottom)`——Button 系 24/8/24/8，
    /// Text 按钮 12/8/12/8（对标 `ButtonDefaults.ContentPadding` /
    /// `TextButtonContentPadding`）；`SizeValue`——支持动画
    pub fn content_padding(style: ButtonStyle) -> (SizeValue, SizeValue, SizeValue, SizeValue) {
        match style {
            ButtonStyle::Text => (12.0.into(), 8.0.into(), 12.0.into(), 8.0.into()),
            _ => (24.0.into(), 8.0.into(), 24.0.into(), 8.0.into()),
        }
    }
}

/// Button 组件 Builder
///
/// 声明式按钮组件。通过链式方法配置点击行为、样式、状态等。
/// 子内容通过 `build(ctx, |ctx| { ... })` 传入（通常是 Text + Icon）。
///
/// # 示例
/// ```ignore
/// Button::new()
///     .style(ButtonStyle::Filled)
///     .on_click(|| count.update(|v| *v += 1))
///     .enabled(true)
///     .modifier(Modifier::new()
///         .size(200.0, 48.0)
///         .background(Color::BLUE, Shape::rounded(8.0))
///     )
///     .build(ctx, |ctx| {
///         Text::new("Increment").color(Color::WHITE).build(ctx);
///     });
/// ```
#[derive(Clone)]
pub struct Button {
    /// 点击回调
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 是否启用
    enabled: bool,
    /// 按钮风格
    style: ButtonStyle,
    /// 颜色集（None = 从主题按 style 默认）
    colors: Option<ButtonColors>,
    /// 交互源（None = build 时内部 remember——对标 Compose 可选注入）
    interaction_source: Option<MutableInteractionSource>,
    /// 阴影高度（None = 默认全 0——对标 material3 ButtonElevation）
    elevation: Option<ButtonElevation>,
    /// 容器/边框/阴影形状（对标 material3 `Button(shape = ...)`——默认胶囊）
    shape: Shape,
    /// 内容 padding (start, top, end, bottom)——None = 按 style 默认；
    /// `SizeValue` 支持动态（动画 State/闭包——measure 期求值，只重测不重组）
    content_padding: Option<(SizeValue, SizeValue, SizeValue, SizeValue)>,
    /// 最小尺寸覆盖（None = ButtonDefaults MinWidth/MinHeight；支持动画）
    min_width: Option<SizeValue>,
    min_height: Option<SizeValue>,
    /// 边框（None = 按 style 默认——Outlined 有 1px 主题色边框，其余无）
    border: Option<ButtonBorder>,
    /// 修饰符链（尺寸、颜色、形状等）
    modifier: Modifier,
}

impl Button {
    /// 创建新的 Button 组件
    pub fn new() -> Self {
        Button {
            on_click: None,
            enabled: true,
            style: ButtonStyle::default(),
            colors: None,
            interaction_source: None,
            elevation: None,
            shape: ButtonDefaults::shape(),
            content_padding: None,
            min_width: None,
            min_height: None,
            border: None,
            modifier: Modifier::new(),
        }
    }

    /// 设置点击回调
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// 设置启用状态
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设置按钮风格
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// 设置颜色集（覆盖主题默认——对标 material3 `Button(colors = ...)`）
    pub fn colors(mut self, colors: ButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 注入交互源（hoist——Button 的 press/hover/focus 状态发射到此源；
    /// 不传则内部 remember 一个）
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// 设置阴影高度（各状态取值见 [`ButtonElevation::for_state`]）
    pub fn elevation(mut self, elevation: ButtonElevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    /// 设置容器/边框/阴影形状（对标 material3 `Button(shape = ...)`——默认胶囊）
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// 设置内容 padding (start, top, end, bottom)——不设置则按 style 取
    /// [`ButtonDefaults::content_padding`]；每边支持动态 `SizeValue`
    /// （`&State<f32>` / `State<f32>` / 闭包——动画可作用于内边距）
    pub fn content_padding<A: Into<SizeValue>, B: Into<SizeValue>, C: Into<SizeValue>, D: Into<SizeValue>>(
        mut self,
        padding: (A, B, C, D),
    ) -> Self {
        self.content_padding = Some((
            padding.0.into(),
            padding.1.into(),
            padding.2.into(),
            padding.3.into(),
        ));
        self
    }

    /// 覆盖默认最小宽度（对标 material3 内部 `defaultMinSize(MinWidth, MinHeight)`——
    /// 默认 58/40，用户可经此或 `Modifier::min_width` 覆盖；支持动态 `SizeValue`）
    pub fn min_size<A: Into<SizeValue>, B: Into<SizeValue>>(
        mut self,
        min_width: A,
        min_height: B,
    ) -> Self {
        self.min_width = Some(min_width.into());
        self.min_height = Some(min_height.into());
        self
    }

    /// 设置边框（对标 material3 `Button(border = BorderStroke(...))`）——
    /// 显式传入后覆盖 style 默认（Outlined 的 1px 主题色边框），形状跟随 shape
    pub fn border(mut self, border: ButtonBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 设置修饰符链（追加到已有 modifier）
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 注册到组合树并执行子内容。
    /// 根据 style 自动从 WiniaTheme 读取默认颜色（用户 modifier 可覆盖）。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（参数相等跳过——style/enabled 未变 → 容器 Skip）
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        let key = ctx.next_key();
        let theme = crate::ui::theme::WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| ButtonColors::from_theme(&theme, self.style));
        // 交互源：外部注入或内部 remember（对标 Compose Button 的 interactionSource 参数）
        let interaction = self.interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        // 状态化取色（读取注册依赖——press/hover/focus 变化自动重组）
        let state = interaction.state(self.enabled);
        let container = colors.container_color_for(&state);
        let text_color = colors.content_color_for(&state);
        // 阴影高度：状态变化（hover/focus/press 进入与离开）用动画 State
        // 平滑过渡——悬停时阴影逐渐升高、移出后逐渐回落，不跳变。
        let elevation_anim = self.elevation.and_then(|e| {
            if e.default <= 0.0 && e.pressed <= 0.0 && e.focused <= 0.0
                && e.hovered <= 0.0 && e.disabled <= 0.0
            {
                None // 全 0：无阴影，不加图层
            } else {
                Some(ctx.animate_float_as_state(
                    e.for_state(&state),
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(180),
                        crate::animation::interpolator::EaseOutCubic::new(),
                    )),
                ))
            }
        });

        // 对标 material3 Button：内部 Row = defaultMinSize(58,40) + contentPadding，
        // 容器/边框/阴影统一使用 shape 参数（默认胶囊 CornerFull）。
        let shape = self.shape;
        let (pad_s, pad_t, pad_e, pad_b) = self
            .content_padding
            .unwrap_or_else(|| ButtonDefaults::content_padding(self.style));
        let min_w = self
            .min_width
            .clone()
            .unwrap_or_else(|| ButtonDefaults::min_width().into());
        let min_h = self
            .min_height
            .clone()
            .unwrap_or_else(|| ButtonDefaults::min_height().into());

        let mut modifier = Modifier::new()
            .min_width(min_w)
            .min_height(min_h)
            .padding_sides(pad_s, pad_t, pad_e, pad_b);
        // 背景按 style（Filled/Tonal 有容器色，Outlined/Text 无）
        modifier = match self.style {
            ButtonStyle::Filled | ButtonStyle::Tonal => modifier.background(container, shape),
            ButtonStyle::Outlined | ButtonStyle::Text => modifier,
        };
        // 边框：显式 border 优先，否则 Outlined 默认 1px 主题内容色
        // （对标 material3 OutlinedButton = Button(border = BorderStroke(1.dp, outline))）
        let border = self.border.or_else(|| {
            if self.style == ButtonStyle::Outlined {
                Some(ButtonBorder::new(1.0, colors.content_color(self.enabled)))
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape);
        }

        // 阴影渲染走 graphics_layer 动态闭包（shadow_elevation 每帧读取动画值——
        // 渲染期求值不触发重组；对标 Compose 层阴影语义；形状跟随 Button shape）
        if let Some(anim) = elevation_anim {
            modifier = modifier.graphics_layer(move || crate::modifier::GraphicsLayerParams {
                shadow_elevation: anim.get(),
                shadow_shape: Some(shape),
                ..Default::default()
            });
        }

        // 追加用户 modifier（在外层，可覆盖默认样式）
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_click) = &self.on_click {
                let cb = on_click.clone();
                // clickable（press/focus/hover 交互）+ 水波纹（对标 Compose
                // clickable 默认 indication=ripple；颜色用内容色）
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb())
                    // 显式传容器 shape——Outlined/Text 无 Background 元素，
                    // 若让波纹自行推断会回退成矩形裁剪（超出胶囊范围）
                    .ripple_with_shape(&interaction, text_color, true, shape);
            }
        }

        // content 闭包自动成为组合 scope（与 Column 一致）
        match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                crate::ui::text::ProvideTextStyle(
                    crate::ui::text::TextStyle::new().color(text_color),
                    ctx, content,
                );
            }
        }
        ctx.end_restartable_group();
    }

    // ── Getters（测试用）──
    pub fn get_enabled(&self) -> bool { self.enabled }
    pub fn get_style(&self) -> ButtonStyle { self.style }
    pub fn get_colors(&self) -> Option<ButtonColors> { self.colors }
    pub fn get_shape(&self) -> Shape { self.shape }
    pub fn get_content_padding(&self) -> Option<&(SizeValue, SizeValue, SizeValue, SizeValue)> {
        self.content_padding.as_ref()
    }
    pub fn get_min_size(&self) -> (Option<SizeValue>, Option<SizeValue>) {
        (self.min_width.clone(), self.min_height.clone())
    }
    pub fn get_border(&self) -> Option<ButtonBorder> { self.border }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Button")
            .field("enabled", &self.enabled)
            .field("style", &self.style)
            .field("modifier", &self.modifier)
            .field("on_click", &self.on_click.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;

    #[test]
    fn test_button_defaults() {
        let btn = Button::new();
        assert!(btn.get_enabled());
        assert_eq!(btn.get_style(), ButtonStyle::Filled);
        assert_eq!(btn.get_shape(), Shape::pill(), "默认形状对齐 CornerFull 胶囊");
        assert!(btn.get_content_padding().is_none(), "content_padding 默认按 style 取");
        assert!(btn.get_min_size().0.is_none() && btn.get_min_size().1.is_none(),
            "min 尺寸默认取 ButtonDefaults");
        assert_eq!(btn.get_border(), None, "border 默认按 style（Outlined 才有）");
        assert_eq!(btn.get_modifier().elements().len(), 0);
    }

    #[test]
    fn test_button_defaults_values() {
        assert_eq!(ButtonDefaults::shape(), Shape::pill());
        assert_eq!(ButtonDefaults::min_width(), 58.0);
        assert_eq!(ButtonDefaults::min_height(), 40.0);
        let p = ButtonDefaults::content_padding(ButtonStyle::Filled);
        assert!(matches!(p.0, SizeValue::Static(crate::modifier::Dimension::Fixed(24.0))));
        assert!(matches!(p.1, SizeValue::Static(crate::modifier::Dimension::Fixed(8.0))));
        let p = ButtonDefaults::content_padding(ButtonStyle::Text);
        assert!(matches!(p.0, SizeValue::Static(crate::modifier::Dimension::Fixed(12.0))));
    }

    #[test]
    fn test_button_border() {
        let b = ButtonBorder::new(2.0, crate::modifier::Color::RED);
        assert_eq!(b.width, 2.0);
        let btn = Button::new().border(b);
        assert_eq!(btn.get_border(), Some(b));
        assert_eq!(btn.get_border().unwrap().color, crate::modifier::Color::RED);
    }

    #[test]
    fn test_button_dynamic_padding_and_min_size() {
        use crate::core::state::State;
        let pad = State::new(10.0);
        let btn = Button::new()
            .content_padding((&pad, 4.0, &pad, 4.0))
            .min_size(&pad, 40.0);
        let (p, (mw, mh)) = (btn.get_content_padding().unwrap(), btn.get_min_size());
        assert!(matches!(&p.0, SizeValue::Dynamic(_)), "State 引用 → 动态（动画可驱动）");
        assert!(matches!(&p.1, SizeValue::Static(_)));
        assert!(matches!(&mw.unwrap(), SizeValue::Dynamic(_)));
        assert!(matches!(&mh.unwrap(), SizeValue::Static(_)));
    }

    #[test]
    fn test_button_builder() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();

        let btn = Button::new()
            .on_click(move || clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst))
            .enabled(false)
            .style(ButtonStyle::Outlined)
            .modifier(Modifier::new().size(200.0, 48.0));

        assert!(!btn.get_enabled());
        assert_eq!(btn.get_style(), ButtonStyle::Outlined);
        assert_eq!(btn.get_modifier().elements().len(), 1);

        // 由于 on_click 被移动到 Button 中，测试完成后 drop
    }

    #[test]
    fn test_button_composition() {
        let mut composer = Composer::new();

        composer.compose(|ctx| {
            let _btn = Button::new()
                .modifier(Modifier::new().size(100.0, 40.0));

            // 模拟 build 过程：Button 是容器
            let key = ctx.next_key();
            ctx.start_container(key, Modifier::new().size(100.0, 40.0), BoxLayout::new());
            // 子内容（模拟 Text）
            let text_key = ctx.next_key();
            ctx.start_leaf(text_key, Modifier::new());
            ctx.end_node();
            ctx.end_node();
        });

        // 组合树应该正确构建
        assert!(composer.layout_root().is_some());
    }

    #[test]
    fn test_button_colors_state_resolution() {
        use crate::modifier::Color;
        let colors = ButtonColors::new(
            Color::RED,
            Color::WHITE,
            Color::from_argb(100, 100, 100, 100),
            Color::from_argb(80, 200, 200, 200),
        );
        // disabled 优先
        assert_eq!(colors.container_color_for(&ComponentState::disabled()), colors.disabled_container);
        // 容器色仅区分 enabled/disabled（与 material3 ButtonColors 一致——
        // hover/focus/press 状态层由 ripple indication 绘制，不叠加在容器色上）
        assert_eq!(colors.container_color_for(&ComponentState::idle()), colors.container);
        assert_eq!(
            colors.container_color_for(&ComponentState { hovered: true, pressed: true, ..ComponentState::idle() }),
            colors.container,
        );
        // 内容色只区分 enabled/disabled（与 material3 ButtonColors 一致）
        assert_eq!(colors.content_color_for(&ComponentState::idle()), colors.content);
        assert_eq!(colors.content_color_for(&ComponentState::disabled()), colors.disabled_content);
    }

    #[test]
    fn test_button_elevation_priority() {
        let e = ButtonElevation::new(1.0, 8.0, 2.0, 2.0, 0.0);
        assert_eq!(e.for_state(&ComponentState::disabled()), 0.0);
        assert_eq!(e.for_state(&ComponentState::idle()), 1.0);
        assert_eq!(e.for_state(&ComponentState { pressed: true, ..ComponentState::idle() }), 8.0);
        assert_eq!(e.for_state(&ComponentState { hovered: true, ..ComponentState::idle() }), 2.0);
        assert_eq!(e.for_state(&ComponentState { focused: true, ..ComponentState::idle() }), 2.0);
        // 优先级：disabled > pressed > dragged > hovered > focused > default
        assert_eq!(
            e.for_state(&ComponentState { pressed: true, hovered: true, ..ComponentState::idle() }),
            8.0
        );
        assert_eq!(ButtonElevation::default_elevation().for_state(&ComponentState::idle()), 0.0);
    }
}
