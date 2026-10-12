//! Card 组件 — 对齐 Compose material3 1.4.0 Card 一族
//!
//! M3 Card 家族三个变体（Card(filled) / ElevatedCard / OutlinedCard）用
//! `Card` builder 的构造变体表达（与 Button 的 filled/elevated/outlined 同模式）：
//! - `Card::new()` / `Card::filled()` —— 对标 `Card`（SurfaceContainerHighest 容器）
//! - `Card::elevated()` —— 对标 `ElevatedCard`（SurfaceContainerLow + 默认阴影）
//! - `Card::outlined()` —— 对标 `OutlinedCard`（Surface 容器 + 1dp outline 边框）
//!
//! 内容 = 顶部对齐 Column（对标 M3 Surface { Column(content) }）；内容色经
//! `WiniaTheme::with_content_color` 下传（Icon tint Auto 自动取卡片内容色）。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::interaction::{ComponentState, MutableInteractionSource};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════
// CardStyle / CardColors / CardElevation / CardBorder
// ═══════════════════════════════════════════════════════════

/// Card 风格变体（对标 M3 Card / ElevatedCard / OutlinedCard）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStyle {
    /// 实心卡片（对标 `Card`）——SurfaceContainerHighest 容器
    Filled,
    /// 悬浮卡片（对标 `ElevatedCard`）——SurfaceContainerLow + 默认阴影
    Elevated,
    /// 轮廓卡片（对标 `OutlinedCard`）——Surface 容器 + 1dp outline 边框
    Outlined,
}

impl Default for CardStyle {
    fn default() -> Self { Self::Filled }
}

/// 卡片颜色集（对标 material3 `CardColors`）——container/content 各含
/// enabled/disabled 变体；禁用：容器 M3 token（Filled SurfaceVariant@38% 叠底、
/// Elevated/Outlined 容器不变），内容色 @38%（对齐 M3 DisabledAlpha）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardColors {
    pub container: Color,
    pub content: Color,
    pub disabled_container: Color,
    pub disabled_content: Color,
}

impl CardColors {
    pub fn new(
        container: Color,
        content: Color,
        disabled_container: Color,
        disabled_content: Color,
    ) -> Self {
        Self { container, content, disabled_container, disabled_content }
    }

    /// 状态化容器色——仅区分 enabled/disabled（与 material3 CardColors 一致；
    /// hover/focus/press 视觉由 indication（ripple）状态层绘制，避免双重叠加）
    pub fn container_color_for(&self, state: &ComponentState) -> Color {
        if !state.enabled { self.disabled_container } else { self.container }
    }

    /// 状态化内容色（仅区分 enabled/disabled——与 material3 CardColors 一致）
    pub fn content_color_for(&self, state: &ComponentState) -> Color {
        if state.enabled { self.content } else { self.disabled_content }
    }

    /// 从主题按 style 生成默认色（对标 material3 CardDefaults.cardColors/
    /// elevatedCardColors/outlinedCardColors——M3 1.4.0 tokens）。
    ///
    /// 简化：M3 的 disabled 容器 = DisabledContainerColor @ DisabledContainerOpacity
    /// compositeOver(ContainerColor)；本仓库约定（同 checkbox/button）用 alpha
    /// 直乘近似——Filled 取 SurfaceVariant@38%，Elevated/Outlined 容器不变
    /// （M3：Elevated 为 Surface@38% 叠 Surface、Outlined 无 disabled 容器变化）。
    pub fn from_theme(theme: &crate::theme::ThemeColors, style: CardStyle) -> Self {
        use crate::graphics::Color;
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        match style {
            // FilledCardTokens: Container=SurfaceContainerHighest, Disabled=SurfaceVariant@38%
            CardStyle::Filled => Self::new(
                theme.surface_container_highest,
                theme.on_surface,
                alpha(theme.surface_variant, 0.38),
                alpha(theme.on_surface, 0.38),
            ),
            // ElevatedCardTokens: Container=SurfaceContainerLow, Disabled=Surface（不变）
            CardStyle::Elevated => Self::new(
                theme.surface_container_low,
                theme.on_surface,
                theme.surface_container_low,
                alpha(theme.on_surface, 0.38),
            ),
            // OutlinedCardTokens: Container=Surface, Disabled 容器不变
            CardStyle::Outlined => Self::new(
                theme.surface,
                theme.on_surface,
                theme.surface,
                alpha(theme.on_surface, 0.38),
            ),
        }
    }
}

/// 卡片阴影高度（对标 material3 `CardElevation`——六状态：default/pressed/
/// focused/hovered/dragged/disabled）。默认值对齐 M3 1.4.0 tokens：
/// - `card_elevation()`（Filled）：0/0/0/1/3/0
/// - `elevated_card_elevation()`（Elevated）：1/1/1/2/4/1
/// - `outlined_card_elevation()`（Outlined）：0/0/0/0/3/0
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub dragged: f32,
    pub disabled: f32,
}

impl CardElevation {
    pub fn new(default: f32, pressed: f32, focused: f32, hovered: f32, dragged: f32, disabled: f32) -> Self {
        Self { default, pressed, focused, hovered, dragged, disabled }
    }

    /// FilledCard 默认（对齐 M3 1.4.0 `FilledCardTokens`：
    /// rest 0 / pressed 0 / focused 0 / hovered 1 / dragged 3 / disabled 0——
    /// Filled 平时无阴影，hover/拖动时升高）
    pub fn default_elevation() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0, 3.0, 0.0)
    }

    /// ElevatedCard 默认（对齐 M3 1.4.0 `ElevatedCardTokens`：
    /// rest 1 / pressed 1 / focused 1 / hovered 2 / dragged 4 / disabled 1）
    pub fn elevated() -> Self {
        Self::new(1.0, 1.0, 1.0, 2.0, 4.0, 1.0)
    }

    /// OutlinedCard 默认（对齐 M3 1.4.0 `OutlinedCardTokens`：
    /// rest 0 / pressed 0 / focused 0 / hovered 0 / dragged 3 / disabled 0——
    /// Outlined 阴影靠边框表达，仅拖动时升高）
    pub fn outlined() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 3.0, 0.0)
    }

    /// 按状态取 elevation（优先级 disabled > pressed > dragged > hovered > focused >
    /// default——与 material3 的"最近交互优先"一致；各状态取独立配置值）。
    ///
    /// pressed 分支取 `max(pressed, hovered)`（与 Button 同一语义）：按下不低于
    /// hover 高度——触屏无 hover，按下升高才有阴影反馈（Elevated 默认 pressed=1
    /// 低于 hovered=2，按下若回落则触屏按下无反馈）。
    /// The elevation for a state snapshot.
    ///
    /// Compose resolves it from the LAST interaction the card received (`material3/Card.kt:705-719`:
    /// `val interaction = interactions.lastOrNull()` then a `when` over it) — not from a fixed priority, and
    /// not from a maximum. So a drag wins over the press that started it (which is why dragging shows the
    /// `dragged` elevation at all), and a press wins over the hover it followed, which is M3's deliberate
    /// "an M3 button and card FLATTEN when pressed": `PressedContainerElevation` is below
    /// `HoverContainerElevation` in every token set (Filled card: 0 against 1; Elevated card: 1 against 2).
    ///
    /// This used to rank `pressed` above `dragged` and to take `max(pressed, hovered)`, which made the
    /// `dragged` elevation unreachable with any real pointer (a drag always holds a press) and made a pressed
    /// card keep its hover height instead of flattening. The state order below is the same "latest wins"
    /// ordering Compose gets from its interaction list.
    pub fn for_state(&self, state: &ComponentState) -> f32 {
        if !state.enabled {
            self.disabled
        } else if state.dragged {
            // A drag is always the latest interaction: it began with a press and outlives it.
            self.dragged
        } else if state.pressed {
            self.pressed
        } else if state.hovered {
            self.hovered
        } else if state.focused {
            self.focused
        } else {
            self.default
        }
    }
}

impl Default for CardElevation {
    fn default() -> Self { Self::default_elevation() }
}

/// 卡片边框（对标 material3 `BorderStroke`）——宽度 + 颜色；形状由
/// [`Card::shape`] 决定（画在容器形状边缘）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardBorder {
    pub width: f32,
    pub color: Color,
}

impl CardBorder {
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }

    /// OutlinedCard 默认边框（对标 material3：1dp + `OutlineVariant`；
    /// disabled 为 Outline @ DisabledOutlineOpacity(0.12) 叠 SurfaceContainerLow）
    fn outlined(theme: &crate::theme::ThemeColors, enabled: bool) -> Self {
        let color = if enabled {
            theme.outline_variant
        } else {
            // OutlinedCardTokens.DisabledOutlineColor = Outline @ 12% 叠容器
            let c = theme.outline;
            Color::from_argb((c.a as f32 * 0.12) as u8, c.r, c.g, c.b)
        };
        Self::new(1.0, color)
    }
}

/// Card 默认值（对标 material3 `CardDefaults`）
pub struct CardDefaults;

impl CardDefaults {
    /// 默认形状：8dp 圆角（对标 M3 `CardTokens.ContainerShape` = CornerMedium）
    pub fn shape() -> Shape {
        Shape::rounded(8.0)
    }

    /// 默认颜色（对标 material3 CardDefaults.cardColors——按 style 从主题推导）
    pub fn card_colors(theme: &crate::theme::ThemeColors, style: CardStyle) -> CardColors {
        CardColors::from_theme(theme, style)
    }

    /// FilledCard 默认阴影（M3 FilledCardTokens：0/0/0/1/3/0）
    pub fn card_elevation() -> CardElevation {
        CardElevation::default_elevation()
    }

    /// ElevatedCard 默认阴影（M3 ElevatedCardTokens：1/1/1/2/4/1）
    pub fn elevated_card_elevation() -> CardElevation {
        CardElevation::elevated()
    }

    /// OutlinedCard 默认阴影（M3 OutlinedCardTokens：0/0/0/0/3/0）
    pub fn outlined_card_elevation() -> CardElevation {
        CardElevation::outlined()
    }

    /// OutlinedCard 默认边框（M3 OutlinedCardTokens：1dp OutlineVariant）
    pub fn outlined_border(theme: &crate::theme::ThemeColors, enabled: bool) -> CardBorder {
        CardBorder::outlined(theme, enabled)
    }
}

// ═══════════════════════════════════════════════════════════
// Card 组件
// ═══════════════════════════════════════════════════════════

/// 卡片组件（对标 material3 Card 一族）。
///
/// 无 `on_click` 时为纯展示卡片（不可交互、无波纹）；设置 `on_click` 后
/// 可点击 + 水波纹（对标 M3 clickable Card）。
pub struct Card {
    /// 点击回调（None = 纯展示卡片）
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 是否启用
    enabled: bool,
    /// 卡片风格（Filled / Elevated / Outlined）
    style: CardStyle,
    /// 颜色集（None = 从主题按 style 默认）
    colors: Option<CardColors>,
    /// 交互源（None = build 时内部 remember——对标 Compose 可选注入）
    interaction_source: Option<MutableInteractionSource>,
    /// 阴影高度（None = 按 style 默认）
    elevation: Option<CardElevation>,
    /// 容器/边框/阴影形状（对标 material3 `Card(shape = ...)`——默认 8dp 圆角）
    shape: Shape,
    /// 边框（None = 按 style 默认——Outlined 有 1dp 主题色边框，其余无）
    border: Option<CardBorder>,
    /// 修饰符链
    modifier: Modifier,
}

impl Card {
    /// 创建新的 Card 组件（默认 Filled 样式）
    pub fn new() -> Self {
        Card {
            on_click: None,
            enabled: true,
            style: CardStyle::default(),
            colors: None,
            interaction_source: None,
            elevation: None,
            shape: CardDefaults::shape(),
            border: None,
            modifier: Modifier::new(),
        }
    }

    /// 实心卡片（对标 material3 `Card`）——默认 Filled 样式
    pub fn filled() -> Self {
        Self::new()
    }

    /// 悬浮卡片（对标 material3 `ElevatedCard`）——Elevated 样式 + 默认阴影
    pub fn elevated() -> Self {
        Self::new()
            .style(CardStyle::Elevated)
            .elevation(CardElevation::elevated())
    }

    /// 轮廓卡片（对标 material3 `OutlinedCard`）——Surface 容器 + 1dp outline 边框
    pub fn outlined() -> Self {
        Self::new().style(CardStyle::Outlined)
    }

    /// 设置点击回调（None = 纯展示；设置后卡片可点击 + 水波纹）
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// 设置启用状态
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设置卡片风格
    pub fn style(mut self, style: CardStyle) -> Self {
        self.style = style;
        self
    }

    /// 设置颜色集（覆盖主题默认——对标 material3 `Card(colors = ...)`）
    pub fn colors(mut self, colors: CardColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 注入交互源（hoist——Card 的 press/hover/focus 状态发射到此源；
    /// 不传则内部 remember 一个）
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// 设置阴影高度（各状态取值见 [`CardElevation::for_state`]）
    pub fn elevation(mut self, elevation: CardElevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    /// 设置容器/边框/阴影形状（对标 material3 `Card(shape = ...)`——默认 8dp 圆角）
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// 设置边框（覆盖 style 默认——Outlined 默认 1dp outline 色）
    pub fn border(mut self, border: CardBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 修饰符链（尺寸、内边距等——追加在外层）
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// 构建卡片。内容自动包在顶部对齐 Column 中（对标 M3 Surface { Column }）。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（参数相等跳过——style/enabled 未变 → 容器 Skip）
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        let key = ctx.next_key();
        let theme = crate::theme::WiniaTheme::colors();
        // 默认颜色统一从 CardDefaults 取（对标 material3 CardDefaults.cardColors）
        let colors = self.colors.unwrap_or_else(|| CardDefaults::card_colors(&theme, self.style));
        // 交互源：外部注入或内部 remember（对标 Compose Card 的 interactionSource 参数）
        let interaction = self.interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        // 状态化取色（读取注册依赖——press/hover/focus 变化自动重组）
        let state = interaction.state(self.enabled);
        let container = colors.container_color_for(&state);
        let content_color = colors.content_color_for(&state);
        // 阴影高度：状态变化（hover/focus/press 进入与离开）用动画 State
        // 平滑过渡——悬停/按下时阴影逐渐升高、移出后逐渐回落，不跳变。
        // The spec is Compose's — the same directional rule the Button uses
        // (`animation::elevation_spec_for`, from `material3/internal/Elevation.kt:101-118`); this used one
        // 180 ms `EaseOutCubic` for every direction. It needs the elevation being LEFT, which is the last
        // target this build pushed, kept in a backchannel so reading it registers no dependency.
        let last_elevation: crate::runtime::state::Backchannel<Option<f32>> =
            ctx.remember_backchannel(|| None);
        let elevation_anim = self.elevation.and_then(|e| {
            if e.default <= 0.0 && e.pressed <= 0.0 && e.focused <= 0.0
                && e.hovered <= 0.0 && e.dragged <= 0.0 && e.disabled <= 0.0
            {
                None // 全 0：无阴影，不加图层
            } else {
                let target = e.for_state(&state);
                let from = last_elevation.peek().unwrap_or(target);
                let spec =
                    crate::animation::elevation_spec_for(from, target, state.hovered && target < from);
                last_elevation.set(Some(target));
                Some(ctx.animate_float_as_state(target, spec))
            }
        });

        let shape = self.shape;
        // 容器：三种 style 都有不透明容器色（Filled=SurfaceContainerHighest、
        // Elevated=SurfaceContainerLow、Outlined=Surface——M3 Card 无透明底变体）
        let mut modifier = Modifier::new()
            .background(container, shape);
        // 边框：显式 border 优先，否则 Outlined 默认 1dp outline 色
        // （对标 material3 OutlinedCard = Card(border = BorderStroke(1.dp, OutlineVariant))）
        let border = self.border.or_else(|| {
            if self.style == CardStyle::Outlined {
                Some(CardBorder::outlined(&theme, self.enabled))
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape);
        }

        // 阴影渲染走 graphics_layer 动态闭包（shadow_elevation 每帧读取动画值——
        // 渲染期求值不触发重组；对标 Compose 层阴影语义；形状跟随 Card shape）
        if let Some(anim) = elevation_anim {
            modifier = modifier.graphics_layer(move || crate::graphics::GraphicsLayerParams {
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
                    // 显式传容器 shape——波纹按容器形状裁剪
                    .ripple_with_shape(&interaction, content_color, true, shape);
            }
        }

        // content 闭包自动成为组合 scope（与 Column 一致）；
        // 布局策略 = ColumnLayout（对标 M3：内容包在 Column 中，顶部对齐）
        //
        // A CLICKABLE card gets Compose's 48 dp touch target: its `onClick` overload goes through
        // `Surface(onClick = …)` (`material3/Card.kt:146-156`), whose Box applies
        // `Modifier.minimumInteractiveComponentSize()` (`material3/Surface.kt:219`). A card that takes no
        // clicks has no such minimum in Compose, and does not get one here either. The wrapper is a node of
        // its own for the reason the Button's is: winia decides a chain's size across the whole chain, so a
        // minimum inside the card's chain would fight its own background and padding. It also carries the
        // ring's shape and colour, since it is the node that takes focus.
        let dir = modifier.get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        let build_card = |ctx: &mut ComposeCtx| {
            match ctx.start_restartable_group(
                key,
                modifier,
                crate::layout::ColumnLayout::new().direction(dir),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // The content colour is passed down (the `LocalContentColor` equivalent) so an `Icon`
                    // with tint Auto picks up the card's own content colour: `onSurface` for all three
                    // styles.
                    crate::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                        content(ctx);
                    });
                }
            }
            // The focus ring's colour is the theme's `secondary` — M3's `CardTokens.FocusIndicatorColor`,
            // deliberately different from Button's `primary`. Captured at compose time into the node's desc,
            // because the theme's CompositionLocal is gone by render time.
            ctx.set_current_node_focus_color(theme.secondary);
            ctx.end_restartable_group();
        };
        if self.on_click.is_some() {
            crate::layout::components::build_container(
                ctx,
                Modifier::new()
                    .minimum_interactive_component_size()
                    .border(0.0, crate::graphics::Color::from_argb(0, 0, 0, 0), shape),
                crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center),
                |ctx| {
                    build_card(ctx);
                    // The wrapper is the focus node, so the ring's colour has to reach it too.
                    ctx.set_current_node_focus_color(theme.secondary);
                },
            );
        } else {
            build_card(ctx);
        }
    }

    // ── Getters（测试用）──
    pub fn get_enabled(&self) -> bool { self.enabled }
    pub fn get_style(&self) -> CardStyle { self.style }
    pub fn get_colors(&self) -> Option<CardColors> { self.colors }
    pub fn get_elevation(&self) -> Option<CardElevation> { self.elevation }
    pub fn get_shape(&self) -> Shape { self.shape }
    pub fn get_border(&self) -> Option<CardBorder> { self.border }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

impl Default for Card {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Compose's CLICKABLE card carries a 48 dp touch target, because the `onClick` overload is built on
    /// `Surface(onClick = …)` (`material3/Card.kt:146-156`) whose Box applies
    /// `Modifier.minimumInteractiveComponentSize()` (`material3/Surface.kt:219`, the node at
    /// `material3/InteractiveComponentSize.kt:98-140`): the box becomes `max(content, 48)` with the content
    /// centred. A card with no `on_click` has no such minimum in Compose — it is asserted here as the control.
    #[test]
    fn a_clickable_card_reserves_a_48_dp_touch_target() {
        let heights = |clickable: bool| -> Vec<f32> {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        let card = Card::filled();
                        let card = if clickable { card.on_click(|| {}) } else { card };
                        card.build(ctx, |ctx| {
                            crate::components::Text::new("Card")
                                .modifier(Modifier::new().padding(8.0))
                                .build(ctx);
                        });
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let nodes = composer.arena_nodes();
            nodes.iter().map(|n| n.measured_size.height).collect()
        };
        let clickable = heights(true);
        assert!(
            clickable.contains(&48.0),
            "the clickable card reserves the 48 dp touch target ({clickable:?})"
        );
        let plain = heights(false);
        assert!(
            !plain.contains(&48.0),
            "and a card that takes no clicks does not, as in Compose ({plain:?})"
        );
    }

    /// The elevation animation is directional, as Compose's is (`material3/internal/Elevation.kt:101-118`) —
    /// the same rule the Button uses. One 180 ms `EaseOutCubic` for every direction was winia's.
    #[test]
    fn the_card_elevation_spec_follows_compose_direction_and_source() {
        let millis = |current: f32, target: f32, hovered: bool| {
            match crate::animation::elevation_spec_for(current, target, hovered) {
                crate::animation::AnimationSpec::Tween(t) => t.duration.as_millis(),
                other => panic!("an elevation animation is a tween, got {other:?}"),
            }
        };
        assert_eq!(millis(0.0, 1.0, false), 120, "growing is the incoming 120 ms");
        assert_eq!(millis(1.0, 0.0, false), 150, "falling is the outgoing 150 ms");
        assert_eq!(millis(1.0, 0.0, true), 120, "falling out of hover is shorter");
    }

    #[test]
    fn test_card_variant_constructors() {
        let filled = Card::new();
        assert_eq!(filled.get_style(), CardStyle::Filled);
        assert_eq!(filled.get_elevation(), None, "Filled 无默认阴影");

        let elevated = Card::elevated();
        assert_eq!(elevated.get_style(), CardStyle::Elevated);
        assert_eq!(elevated.get_elevation(), Some(CardElevation::elevated()));

        let outlined = Card::outlined();
        assert_eq!(outlined.get_style(), CardStyle::Outlined);
        assert_eq!(outlined.get_elevation(), None, "Outlined 默认阴影全 0");
    }

    #[test]
    fn test_card_default_shape_is_8dp_rounded() {
        assert_eq!(CardDefaults::shape(), Shape::rounded(8.0));
    }

    #[test]
    fn test_card_colors_match_m3_tokens() {
        let theme = crate::theme::ThemeColors::default_light();
        // Filled：SurfaceContainerHighest 容器 + OnSurface 内容
        let filled = CardDefaults::card_colors(&theme, CardStyle::Filled);
        assert_eq!(filled.container, theme.surface_container_highest);
        assert_eq!(filled.content, theme.on_surface);
        assert_eq!(
            filled.disabled_container.a,
            (theme.surface_variant.a as f32 * 0.38) as u8,
            "Filled 禁用容器 = SurfaceVariant @ 38%"
        );
        assert_eq!(filled.disabled_content.a, (theme.on_surface.a as f32 * 0.38) as u8);
        // Elevated：SurfaceContainerLow 容器，禁用容器不变
        let elevated = CardDefaults::card_colors(&theme, CardStyle::Elevated);
        assert_eq!(elevated.container, theme.surface_container_low);
        assert_eq!(elevated.disabled_container, elevated.container);
        // Outlined：Surface 容器，禁用容器不变
        let outlined = CardDefaults::card_colors(&theme, CardStyle::Outlined);
        assert_eq!(outlined.container, theme.surface);
        assert_eq!(outlined.disabled_container, outlined.container);
    }

    #[test]
    fn test_card_colors_state_resolution() {
        let colors = CardColors::new(
            Color::RED,
            Color::WHITE,
            Color::from_argb(100, 100, 100, 100),
            Color::from_argb(80, 200, 200, 200),
        );
        // disabled 优先
        assert_eq!(colors.container_color_for(&ComponentState::disabled()), colors.disabled_container);
        // 容器色仅区分 enabled/disabled（与 material3 CardColors 一致——
        // hover/focus/press 状态层由 ripple indication 绘制，不叠加在容器色上）
        assert_eq!(colors.container_color_for(&ComponentState::idle()), colors.container);
        assert_eq!(
            colors.container_color_for(&ComponentState { hovered: true, pressed: true, ..ComponentState::idle() }),
            colors.container,
        );
        // 内容色只区分 enabled/disabled
        assert_eq!(colors.content_color_for(&ComponentState::idle()), colors.content);
        assert_eq!(colors.content_color_for(&ComponentState::disabled()), colors.disabled_content);
    }

    #[test]
    fn test_card_elevation_defaults() {
        // M3 1.4.0 tokens
        assert_eq!(CardElevation::default_elevation(), CardElevation::new(0.0, 0.0, 0.0, 1.0, 3.0, 0.0));
        assert_eq!(CardElevation::elevated(), CardElevation::new(1.0, 1.0, 1.0, 2.0, 4.0, 1.0));
        assert_eq!(CardElevation::outlined(), CardElevation::new(0.0, 0.0, 0.0, 0.0, 3.0, 0.0));
        assert_eq!(CardDefaults::card_elevation(), CardElevation::default_elevation());
        assert_eq!(CardDefaults::elevated_card_elevation(), CardElevation::elevated());
        assert_eq!(CardDefaults::outlined_card_elevation(), CardElevation::outlined());
    }

    #[test]
    fn test_card_elevation_priority() {
        let e = CardElevation::new(1.0, 8.0, 2.0, 2.0, 5.0, 0.0);
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
        // Compose takes the elevation of the LATEST interaction (`Card.kt:705-719`), so a drag outranks the
        // press that started it …
        assert_eq!(
            e.for_state(&ComponentState { dragged: true, ..ComponentState::idle() }),
            5.0
        );
        assert_eq!(
            e.for_state(&ComponentState { dragged: true, pressed: true, ..ComponentState::idle() }),
            5.0,
            "a drag is always later than its own press, and it is what the `dragged` token is for"
        );
        // … and a press outranks the hover it followed, because M3 flattens on press (`PressedContainerElevation`
        // sits below `HoverContainerElevation` in every token set).
        let low_press = CardElevation::new(1.0, 1.0, 1.0, 3.0, 5.0, 0.0);
        assert_eq!(
            low_press.for_state(&ComponentState { pressed: true, hovered: true, ..ComponentState::idle() }),
            1.0,
            "pressed, not the hover it followed — this used to take max(pressed, hovered)"
        );
        // The real Elevated tokens (1/1/1/2/4/1): a press flattens to 1 where the hover above it was 2 — the
        // M3 press feedback — and a drag carries the card up to 4.
        assert_eq!(
            CardElevation::elevated().for_state(&ComponentState { pressed: true, ..ComponentState::idle() }),
            1.0,
            "pressed flattens below hover, as the tokens say"
        );
        assert_eq!(
            CardElevation::elevated()
                .for_state(&ComponentState { pressed: true, hovered: true, ..ComponentState::idle() }),
            1.0,
            "and hovering before the press does not keep it up"
        );
        assert_eq!(
            CardElevation::elevated().for_state(&ComponentState { dragged: true, ..ComponentState::idle() }),
            4.0
        );
    }

    #[test]
    fn test_outlined_border_uses_theme_outline_variant() {
        let theme = crate::theme::ThemeColors::default_light();
        let enabled = CardBorder::outlined(&theme, true);
        assert_eq!(enabled.width, 1.0);
        assert_eq!(enabled.color, theme.outline_variant);
        let disabled = CardBorder::outlined(&theme, false);
        assert_eq!(
            disabled.color.a,
            (theme.outline.a as f32 * 0.12) as u8,
            "禁用边框 = Outline @ 12%"
        );
        // Outlined 变体默认带边框，Filled/Elevated 不带
        assert_eq!(Card::outlined().get_border(), None, "边框在 build 时按 style 解析");
    }

    #[test]
    fn test_card_builder() {
        let card = Card::elevated()
            .on_click(|| {})
            .modifier(Modifier::new().padding(8.0));
        assert!(card.get_enabled());
        assert_eq!(card.get_style(), CardStyle::Elevated);
        assert_eq!(card.get_shape(), Shape::rounded(8.0));
        assert!(card.get_modifier().elements().len() >= 1);
    }
}
