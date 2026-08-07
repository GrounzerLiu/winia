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
use crate::modifier::{Modifier, Shape};
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

    /// ElevatedButton 近似（M3 tokens：rest 1 / pressed 8 / focused 2 / hovered 2 / disabled 0）
    pub fn elevated() -> Self {
        Self::new(1.0, 8.0, 2.0, 2.0, 0.0)
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
        let elevation = self.elevation.map(|e| e.for_state(&state)).unwrap_or(0.0);

        // 根据 style 在最内层插入主题默认背景/边框
        // 默认 wrap content（不撑满父容器），用户可用 .size()/.fill_max_size() 覆盖
        let mut modifier = match self.style {
            ButtonStyle::Filled => {
                Modifier::new().padding_horizontal(12.0).padding_vertical(8.0).background(container, Shape::rounded(20.0))
            }
            ButtonStyle::Tonal => {
                Modifier::new().padding_horizontal(12.0).padding_vertical(8.0).background(container, Shape::rounded(20.0))
            }
            ButtonStyle::Outlined => {
                Modifier::new().padding_horizontal(12.0).padding_vertical(8.0).border(1.0, colors.content_color(self.enabled), Shape::rounded(20.0))
            }
            ButtonStyle::Text => {
                Modifier::new().padding_horizontal(12.0).padding_vertical(8.0)
            }
        };

        // 阴影（elevation > 0 才应用——Modifier.shadow 本身也按 elevation>0 短路）
        if elevation > 0.0 {
            modifier = modifier.shadow(
                elevation,
                Shape::rounded(20.0),
                true,
                crate::modifier::Color::BLACK,
            );
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
                    .ripple(&interaction, text_color, true);
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
        assert_eq!(btn.get_modifier().elements().len(), 0);
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
