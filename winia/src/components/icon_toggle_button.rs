//! IconToggleButton 组件 — 对标 material3 `IconToggleButton` /
//! `FilledIconToggleButton` / `FilledTonalIconToggleButton` /
//! `OutlinedIconToggleButton`

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::components::button::ButtonBorder;
use crate::components::icon_button::{
    IconButtonDefaults, IconButtonShape, IconButtonSize, IconButtonStyle,
};
use crate::input::interaction::MutableInteractionSource;
use crate::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 图标切换按钮颜色集——enabled/disabled × checked/unchecked
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconToggleButtonColors {
    pub container: Color,
    pub content: Color,
    pub disabled_container: Color,
    pub disabled_content: Color,
    pub checked_container: Color,
    pub checked_content: Color,
}

impl IconToggleButtonColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        container: Color,
        content: Color,
        disabled_container: Color,
        disabled_content: Color,
        checked_container: Color,
        checked_content: Color,
    ) -> Self {
        Self {
            container,
            content,
            disabled_container,
            disabled_content,
            checked_container,
            checked_content,
        }
    }

    pub fn container_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            self.disabled_container
        } else if checked {
            self.checked_container
        } else {
            self.container
        }
    }

    pub fn content_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            self.disabled_content
        } else if checked {
            self.checked_content
        } else {
            self.content
        }
    }

    /// 从主题按变体生成默认色（对标 IconButtonDefaults.*IconToggleButtonColors；
    /// token 版本 14_1_0）
    pub fn from_theme(theme: &ThemeColors, style: IconButtonStyle) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        // DisabledContainerColor = OnSurface @ 0.10、DisabledColor = OnSurface @ 0.38
        let disabled_container = alpha(theme.on_surface, 0.10);
        let disabled_content = alpha(theme.on_surface, 0.38);
        match style {
            IconButtonStyle::Standard => Self::new(
                transparent,
                theme.on_surface,
                transparent,
                disabled_content,
                transparent,
                theme.primary,
            ),
            // UnselectedContainerColor = SurfaceContainer / UnselectedColor = OnSurfaceVariant
            IconButtonStyle::Filled => Self::new(
                theme.surface_container,
                theme.on_surface_variant,
                disabled_container,
                disabled_content,
                theme.primary,
                theme.on_primary,
            ),
            // Unselected = SecondaryContainer/OnSecondaryContainer；
            // Selected = Secondary/OnSecondary（14_1_0 token）
            IconButtonStyle::FilledTonal => Self::new(
                theme.secondary_container,
                theme.on_secondary_container,
                disabled_container,
                disabled_content,
                theme.secondary,
                theme.on_secondary,
            ),
            // Selected = InverseSurface/InverseOnSurface（14_1_0 token）
            IconButtonStyle::Outlined => Self::new(
                transparent,
                theme.on_surface_variant,
                transparent,
                disabled_content,
                theme.inverse_surface,
                theme.inverse_on_surface,
            ),
        }
    }
}

/// IconToggleButton 默认值（对标 material3 `IconButtonDefaults.*IconToggleButton*`）
pub struct IconToggleButtonDefaults;

impl IconToggleButtonDefaults {
    pub fn icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::Standard)
    }

    pub fn filled_icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::Filled)
    }

    pub fn filled_tonal_icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::FilledTonal)
    }

    pub fn outlined_icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::Outlined)
    }

    pub fn colors_for(theme: &ThemeColors, style: IconButtonStyle) -> IconToggleButtonColors {
        match style {
            IconButtonStyle::Standard => Self::icon_toggle_button_colors(theme),
            IconButtonStyle::Filled => Self::filled_icon_toggle_button_colors(theme),
            IconButtonStyle::FilledTonal => Self::filled_tonal_icon_toggle_button_colors(theme),
            IconButtonStyle::Outlined => Self::outlined_icon_toggle_button_colors(theme),
        }
    }

    /// Outlined 边框：仅未选中时绘制（对标
    /// `outlinedIconToggleButtonBorder`——checked 返回 null）；
    /// 1dp + OutlineVariant，disabled 38% alpha
    pub fn outlined_border(
        theme: &ThemeColors,
        enabled: bool,
        checked: bool,
        width: f32,
    ) -> Option<ButtonBorder> {
        if checked {
            return None;
        }
        // UnselectedOutlineColor = OutlineVariant（非 outline）
        let c = theme.outline_variant;
        let color = if enabled {
            c
        } else {
            Color::from_argb((c.a as f32 * 0.38) as u8, c.r, c.g, c.b)
        };
        Some(ButtonBorder::new(width, color))
    }
}

/// The checked/unchecked colour fade. `IconButton` takes its colours from `enabled` alone and animates
/// none of them, and `IconToggleButton` inherited that shape — which left this the only toggle in the
/// framework that snapped on a state change, while chip, checkbox and radio all fade. So this is a
/// winia addition rather than a ported behaviour, recorded here the way `navigation_drawer` records its
/// own fade. `effects_default` is M3's `DefaultEffects`: the token the framework already uses for a
/// state arriving (`top_app_bar_container_spec`, `radio_colour_spec`).
fn icon_toggle_button_colour_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::effects_default()
}

/// IconToggleButton 组件 Builder
pub struct IconToggleButton {
    style: IconButtonStyle,
    checked: bool,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    colors: Option<IconToggleButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    /// `None` means "use the size's Round container and morph it on press", which is what
    /// `IconButton`'s `shape: Option<Shape>` does; `Some` pins an explicit shape and skips the morph.
    shape: Option<Shape>,
    border: Option<ButtonBorder>,
    size_variant: IconButtonSize,
    modifier: Modifier,
}

impl IconToggleButton {
    pub fn new(checked: bool) -> Self {
        Self {
            style: IconButtonStyle::Standard,
            checked,
            on_checked_change: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            shape: None,
            border: None,
            size_variant: IconButtonSize::Small,
            modifier: Modifier::new(),
        }
    }

    pub fn filled(checked: bool) -> Self {
        Self::new(checked).style(IconButtonStyle::Filled)
    }

    pub fn filled_tonal(checked: bool) -> Self {
        Self::new(checked).style(IconButtonStyle::FilledTonal)
    }

    pub fn outlined(checked: bool) -> Self {
        Self::new(checked).style(IconButtonStyle::Outlined)
    }

    pub fn style(mut self, style: IconButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_checked_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_checked_change = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: IconToggleButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    pub fn border(mut self, border: ButtonBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 尺寸变体（XSmall/Small/Medium/Large/XLarge，默认 Small）
    pub fn size(mut self, size: IconButtonSize) -> Self {
        self.size_variant = size;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        ctx.changed(&self.checked);
        ctx.changed(&self.colors);
        ctx.changed(&self.shape);
        // The size picks the container, the glyph AND the checked/pressed corner tokens, and
        // without this the whitelist skipped it: `IconToggleButton::size()` on an already
        // composed slot re-executed nothing. (`IconButton` declares its own size the same way.)
        ctx.changed(&self.size_variant);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| IconToggleButtonDefaults::colors_for(&theme, self.style));
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let container = colors.container_color(self.enabled, self.checked);
        let content_color = colors.content_color(self.enabled, self.checked);
        let size = self.size_variant.container_size();

        // The CONTAINER colour fade — `icon_toggle_button_colour_spec` records why this is a winia addition.
        // Enabled/disabled does NOT fade: Compose's rule is that there should be no animation between those
        // two states (`material3/Checkbox.kt:634-643`), which `checkbox.rs` already honours, so that branch
        // snaps.
        let colour_spec = if self.enabled {
            crate::animation::AnimationSpec::Spring(icon_toggle_button_colour_spec())
        } else {
            crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default())
        };
        // The container and the ripple colour are PAINT-time values: the states are handed to the
        // background and to the ripple, whose drawers peek them per frame, so the fade costs no
        // recomposition. (`Icon`'s tint can also take a state now — see `Tint::Color` — when a caller
        // wants the glyph to follow a colour it owns.)
        let container = ctx.animate_color_as_state(container, colour_spec.clone());
        let ripple_color = ctx.animate_color_as_state(content_color, colour_spec);

        // The CONTENT colour this button PROVIDES is still a resting colour per generation: a
        // `Crossfade` generation only rebuilds its content when its own slot parameters change
        // (`crossfade.rs`, the `ctx.changed(&value)` in the generation loop), and the content is the
        // caller's closure, so the button cannot hand it a live colour. What the caller gets is the
        // starting generation's colour and the arriving one's, and the crossfade is what makes the
        // colour change read as a transition. Measured before this comment was written: a Filled button
        // switched to checked kept painting its glyph in `on_surface_variant` (68, 71, 78) instead of
        // `on_primary` when the content colour was not per generation.

        // The THREE shapes a toggle morphs between, in upstream's precedence: PRESSED beats CHECKED
        // beats resting (`shapeByInteraction`, `material3/IconButton.kt:1622-1638`). Upstream's
        // default toggle is (`IconButtonDefaults.kt:1046-1055`):
        //
        //     shape        = SmallIconButtonTokens.ContainerShapeRound         (CornerFull)
        //     pressedShape = SmallIconButtonTokens.PressedContainerShape       (CornerSmall)
        //     checkedShape = SmallIconButtonTokens.SelectedContainerShapeRound (CornerMedium)
        //
        // so it rests as a circle, flattens while pressed, and becomes this size's rounded rectangle
        // while CHECKED. winia had only the first two — the `SelectedContainerShape*` tokens went
        // unused and a checked toggle kept its resting circle.
        //
        // `Shape::Circle` carries no radius to interpolate, hence the arithmetic. An explicit shape is
        // pinned and skips the morph entirely, which is upstream's `isStatic` early-out.
        // The shape morph is a paint-time value too: the radius state is peeked inside the carrier, so
        // the corner interpolates without recomposing the button on every frame of the spring.
        let shape: crate::graphics::Dyn<Shape> = match self.shape {
            Some(explicit) => crate::graphics::Dyn::Static(explicit),
            None => {
                let round = size / 2.0;
                let target = if interaction.state(self.enabled).pressed {
                    self.size_variant.pressed_container_radius()
                } else if self.checked {
                    self.size_variant
                        .selected_container_radius(IconButtonShape::Round, round)
                } else {
                    round
                };
                let radius =
                    ctx.animate_float_as_state(target, crate::components::icon_button::shape_morph_spec());
                crate::graphics::Dyn::Dynamic(Arc::new(move || Shape::rounded(radius.peek())))
            }
        };

        // The node is grown to the 48 dp touch target and the paint is inset back to the container by the
        // offset that publishes — without this an IconToggleButton lost both the target and 8 dp of its own
        // box when the Small container went 48 -> 40. `.clip(shape)` sits before the background, as in
        // compose (`IconButton.kt:180`), and the renderer clips at `paint_rect` — the token container, not
        // the touch target.
        let mut modifier = Modifier::new()
            .minimum_interactive_component_size()
            .size(size, size)
            .clip(shape.clone())
            .background(&container, shape.clone());
        let border = self.border.or_else(|| {
            if self.style == IconButtonStyle::Outlined {
                IconToggleButtonDefaults::outlined_border(
                    &theme,
                    self.enabled,
                    self.checked,
                    self.size_variant.outline_width(),
                )
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape.clone());
        }
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_checked_change) = &self.on_checked_change {
                let cb = on_checked_change.clone();
                let checked = self.checked;
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb(!checked))
                    .ripple_with_shape(&interaction, &ripple_color, true, shape);
            }
        }

        match ctx.start_restartable_group(
            key,
            modifier,
            BoxLayout::new().alignment(crate::layout::Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 内容色下传（含 checked 态内容色）——Icon tint Auto 自动跟随
                WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    crate::components::text::ProvideTextStyle(
                        crate::text::TextStyle::new().color(content_color),
                        ctx,
                        content,
                    );
                });
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }

    pub fn get_checked(&self) -> bool {
        self.checked
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_style(&self) -> IconButtonStyle {
        self.style
    }
    pub fn get_colors(&self) -> Option<IconToggleButtonColors> {
        self.colors
    }
    pub fn get_shape(&self) -> Shape {
        // The resting shape is the size's Round container, so a caller who never pinned one still gets
        // what the button paints while it is not pressed.
        self.shape.unwrap_or_else(IconButtonDefaults::shape)
    }
    pub fn get_border(&self) -> Option<ButtonBorder> {
        self.border
    }
    pub fn get_size(&self) -> IconButtonSize {
        self.size_variant
    }
}


#[cfg(test)]
mod tests;
