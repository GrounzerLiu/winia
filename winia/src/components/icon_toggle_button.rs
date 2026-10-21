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
use crate::interaction::MutableInteractionSource;
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
        let container = ctx.animate_color_as_state(container, colour_spec.clone()).get();
        // The RIPPLE keeps the animated content colour: it is painted, read per frame by `draw_ripple`, so
        // it can follow the fade. The GLYPH cannot — its tint is captured at build time, which is the note
        // below.
        let ripple_color = ctx.animate_color_as_state(content_color, colour_spec).get();

        // The CONTENT colour is deliberately NOT animated, and this is the one thing to keep in mind here.
        // `Icon`'s tint is resolved at build time, and a `Crossfade` generation only rebuilds its content
        // when its own slot parameters change (`crossfade.rs`, the `ctx.changed(&value)` in the generation
        // loop) — so a glyph built on the first frame of a colour animation captures the colour it is
        // LEAVING and keeps it for good. Measured: a Filled button switched to checked went on painting its
        // glyph in `on_surface_variant` (68, 71, 78) instead of `on_primary`.
        //
        // Handing each generation its own resting colour is also what gives the ICON its transition: the
        // outgoing glyph fades out in the old colour while the incoming one arrives in the new one. The
        // container still animates, so the surface moves smoothly underneath either way.

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
        let shape = match self.shape {
            Some(explicit) => explicit,
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
                Shape::rounded(radius.get())
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
            .clip(shape)
            .background(container, shape);
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
            modifier = modifier.border(b.width, b.color, shape);
        }
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_checked_change) = &self.on_checked_change {
                let cb = on_checked_change.clone();
                let checked = self.checked;
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb(!checked))
                    .ripple_with_shape(&interaction, ripple_color, true, shape);
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
mod tests {
    use super::*;

    /// The node is grown to the 48 dp touch target while the container paints at the token size. Without
    /// `minimum_interactive_component_size` a Small IconToggleButton is 40 dp AND has no touch target at all
    /// — which is what happened when `IconButtonSize::container_size()` went 48 -> 40 and nothing here
    /// compensated. Deleting that line again now fails this test.
    #[test]
    fn the_node_keeps_the_48_dp_touch_target() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::new(false)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                            .build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        // Both axes: the touch target is a 48 square, and folding only the width would miss a height left at
        // the token container's 40.
        let (widest, tallest) =
            composer
                .arena_nodes()
                .iter()
                .fold((0.0f32, 0.0f32), |(w, h), node| {
                    (
                        w.max(node.measured_size.width),
                        h.max(node.measured_size.height),
                    )
                });
        assert_eq!(
            (widest, tallest),
            (48.0, 48.0),
            "the node is the 48 dp touch target, with the token container painted inside it"
        );
    }

    /// The parity half of `IconButton`'s `.clip(shape)`: without it a toggle button's ripple and children
    /// spill past the container's corners.
    #[test]
    fn the_container_is_clipped_to_its_shape() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::new(false)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                            .build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let clipped = composer.arena_nodes().iter().any(|node| {
            node.modifier
                .elements()
                .iter()
                .any(|el| matches!(el, crate::modifier::ModifierElement::Clip { .. }))
        });
        assert!(
            clipped,
            "the container carries a clip, so its children cannot spill past the corners"
        );
    }

    /// The same press morph `IconButton` has. Round is CornerFull over the token container, so at rest the
    /// radius is half the container and a press interpolates down to that size's PressedContainerShape —
    /// deleting the morph leaves the radius at 20 and fails this test.
    #[test]
    fn a_press_morphs_the_container_shape_over_time() {
        // `update_animations` below drives the process-wide table, which `clear_all_animations` (200+ call
        // sites) empties for everyone — so this holds the same lock the animation tests take.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(false)
                        .on_checked_change(|_| {})
                        .interaction_source(src.clone())
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let radius = |composer: &crate::runtime::composer::Composer| -> f32 {
            let painted = composer.arena_nodes().iter().find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { shape, .. } => Some(*shape),
                    _ => None,
                })
            });
            match painted {
                Some(Shape::RoundedRect { corner_radius }) => corner_radius,
                other => panic!("the container should be a rounded rect, got {other:?}"),
            }
        };

        build(&mut composer);
        // Round over the token container: the paint is inset back to 40 dp inside the 48 dp touch target.
        assert_eq!(radius(&composer), 20.0, "at rest the container is Round");

        source.emit_press();
        build(&mut composer);
        // The morph must be REGISTERED, and the shape must not have switched yet. `assert_ne!` on its own
        // would also pass if no animation had started at all: the resting radius is 20, not 8.
        assert!(
            crate::animation::is_animating(),
            "pressing registers the shape morph"
        );
        assert_ne!(
            radius(&composer),
            8.0,
            "a press starts a morph instead of switching the shape"
        );

        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            (radius(&composer) - 8.0).abs() < 0.5,
            "and it arrives at Small's PressedContainerShape, got {}",
            radius(&composer)
        );
    }

    /// Switching `checked` fades the container rather than snapping. `Filled` is the style this test has
    /// to use: its two container colours actually differ (`surfaceContainer` -> `primary`), while the
    /// Standard one is transparent in both states. Deleting the `animate_color_as_state` call leaves the
    /// colour at the checked value on the first frame and fails the `assert_ne!` below.
    #[test]
    fn a_checked_switch_fades_the_container_over_time() {
        // `update_animations` drives the process-wide table, so this takes the same lock the other
        // animation tests hold.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let on = checked.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::filled(on)
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let container = |composer: &crate::runtime::composer::Composer| -> Color {
            composer
                .arena_nodes()
                .iter()
                .find_map(|node| {
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { color, .. } => {
                            Some(color.eval())
                        }
                        _ => None,
                    })
                })
                .expect("the container paints a background")
        };
        // A spring settles within its threshold rather than exactly on the target, so compare per channel.
        let near = |got: Color, want: Color| {
            (got.r as i32 - want.r as i32).abs() <= 2
                && (got.g as i32 - want.g as i32).abs() <= 2
                && (got.b as i32 - want.b as i32).abs() <= 2
        };

        build(&mut composer);
        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        let unchecked = colors.container_color(true, false);
        let target = colors.container_color(true, true);
        assert_ne!(
            unchecked, target,
            "this test needs a style whose two container colours differ"
        );
        assert_eq!(
            container(&composer),
            unchecked,
            "at rest the container is the unchecked colour"
        );

        checked.update(|c| *c = true);
        build(&mut composer);
        // The fade must be REGISTERED and must not have landed yet: `assert_ne!` alone would also pass if
        // no animation had started and the paint were simply stale.
        assert!(
            crate::animation::is_animating(),
            "switching checked registers the colour fade"
        );
        assert!(
            !near(container(&composer), target),
            "the colour does not jump to the checked value, got {:?}",
            container(&composer)
        );

        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            near(container(&composer), target),
            "and the fade arrives at the checked colour, got {:?}",
            container(&composer)
        );
    }

    /// The documented way to fade the ICON as well as the colour: `content` is a `FnOnce`, so the caller
    /// wraps the glyph in `Crossfade` (see `docs/icon.md`, and the four buttons in `icon_demo`). What this
    /// pins is that the two compose: `IconToggleButton` opens a restartable group, and the crossfade has to
    /// stay functional inside it — the outgoing generation must remain on screen next to the incoming one.
    #[test]
    fn the_icon_cross_fades_with_the_checked_state() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(checked.get())
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::Crossfade::new(checked.clone())
                                .build(ctx, |ctx, on| {
                                    let path = if on { "M0 0h1v1z" } else { "M0 0h2v2z" };
                                    crate::components::icon::Icon::svg_path(path).build(ctx);
                                });
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        // Each generation draws its own icon, so counting them counts the generations on screen.
        let icons = |composer: &crate::runtime::composer::Composer| -> usize {
            composer
                .arena_nodes()
                .iter()
                .filter(|node| {
                    node.modifier
                        .elements()
                        .iter()
                        .any(|el| matches!(el, crate::modifier::ModifierElement::DrawIcon { .. }))
                })
                .count()
        };

        build(&mut composer);
        assert_eq!(icons(&composer), 1, "one generation while nothing is changing");

        checked.update(|c| *c = true);
        build(&mut composer);
        assert_eq!(
            icons(&composer),
            2,
            "the outgoing icon stays on screen while the incoming one fades in"
        );
    }

    /// The colour must survive the cross-fade wrapper: the content colour this button provides has to reach
    /// a glyph built INSIDE a `Crossfade` generation, or every icon in the fade paints in the wrong colour.
    #[test]
    fn the_icon_keeps_its_content_colour_inside_the_crossfade() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(true);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::filled(checked.get())
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::Crossfade::new(checked.clone()).build(ctx, |ctx, on| {
                            let path = if on { "M0 0h1v1z" } else { "M0 0h2v2z" };
                            crate::components::icon::Icon::svg_path(path).build(ctx);
                        });
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut tints = Vec::new();
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    tints.push(spec.tint);
                }
            }
        }
        assert!(
            !tints.is_empty(),
            "the icon should be built inside the crossfade"
        );
        for tint in &tints {
            assert_eq!(
                *tint,
                Some(theme.on_primary),
                "every generation's icon takes the checked content colour"
            );
        }
    }

    /// Switching `checked` must move the glyph to the checked colour. This is the case the "starts checked"
    /// test above cannot see: the content colour is ANIMATED, and a `Crossfade` generation only rebuilds its
    /// content when its own slot parameters change — so a generation built on the first frame of that
    /// animation can capture the colour it is leaving and keep it forever.
    #[test]
    fn the_glyph_follows_the_content_colour_after_a_checked_switch() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::filled(checked.get())
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::Crossfade::new(checked.clone()).build(
                                ctx,
                                |ctx, on| {
                                    let path = if on { "M0 0h1v1z" } else { "M0 0h2v2z" };
                                    crate::components::icon::Icon::svg_path(path).build(ctx);
                                },
                            );
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let tints = |composer: &crate::runtime::composer::Composer| -> Vec<Option<Color>> {
            composer
                .arena_nodes()
                .iter()
                .flat_map(|node| {
                    node.modifier.elements().iter().filter_map(|el| match el {
                        crate::modifier::ModifierElement::DrawIcon { spec } => Some(spec.tint),
                        _ => None,
                    })
                })
                .collect()
        };

        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        let unchecked = colors.content_color(true, false);
        let target = colors.content_color(true, true);

        build(&mut composer);
        assert_eq!(
            tints(&composer),
            vec![Some(unchecked)],
            "an unchecked button paints the unchecked content colour"
        );

        checked.update(|c| *c = true);
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert_eq!(
            tints(&composer),
            vec![Some(target)],
            "once the switch settles only the checked generation is left, in the checked colour"
        );
    }

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(colors.container_color(true, false), colors.container);
        assert_eq!(colors.content_color(true, false), colors.content);
        assert_eq!(colors.container_color(true, true), theme.primary, "checked 容器 = primary");
        assert_eq!(colors.content_color(true, true), theme.on_primary);
        // disabled 优先于 checked
        assert_eq!(colors.container_color(false, true), colors.disabled_container);
        assert_eq!(colors.content_color(false, true), colors.disabled_content);
        // 标准版 checked 内容 = primary（M3 SelectedColor）
        let s = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Standard);
        assert_eq!(s.content_color(true, true), theme.primary);
        // disabled 色值本身：OnSurface 10%/38%（防与 IconButton 漂移）
        assert_eq!(s.disabled_content.a, (theme.on_surface.a as f32 * 0.38) as u8);
        let f2 = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(f2.disabled_container.a, (theme.on_surface.a as f32 * 0.10) as u8);
        // Filled 未选 = surfaceContainer/onSurfaceVariant
        let f = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(f.container, theme.surface_container);
        assert_eq!(f.content, theme.on_surface_variant);
        // Tonal 选中 = secondary/onSecondary（14_1_0 token）
        let t = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::FilledTonal);
        assert_eq!(t.checked_container, theme.secondary);
        assert_eq!(t.checked_content, theme.on_secondary);
        // Outlined 选中 = inverseSurface/inverseOnSurface
        let o = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Outlined);
        assert_eq!(o.checked_container, theme.inverse_surface);
        assert_eq!(o.checked_content, theme.inverse_on_surface);
    }

    #[test]
    fn outlined_border_hidden_when_checked() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        assert!(IconToggleButtonDefaults::outlined_border(&theme, true, false, 1.0).is_some());
        assert_eq!(
            IconToggleButtonDefaults::outlined_border(&theme, true, false, 1.0).unwrap().color,
            theme.outline_variant
        );
        assert!(IconToggleButtonDefaults::outlined_border(&theme, true, true, 1.0).is_none(), "checked 无边框");
        let disabled = IconToggleButtonDefaults::outlined_border(&theme, false, false, 1.0).unwrap();
        assert_eq!(disabled.color.a, (theme.outline_variant.a as f32 * 0.38) as u8);
        assert_eq!(
            IconToggleButtonDefaults::outlined_border(&theme, true, false, IconButtonSize::Large.outline_width())
                .unwrap()
                .width,
            2.0
        );
    }

    #[test]
    fn constructors() {
        assert!(IconToggleButton::new(true).get_checked());
        assert!(!IconToggleButton::new(false).get_checked());
        assert_eq!(IconToggleButton::filled(true).get_style(), IconButtonStyle::Filled);
        assert_eq!(IconToggleButton::filled_tonal(true).get_style(), IconButtonStyle::FilledTonal);
        assert_eq!(IconToggleButton::outlined(true).get_style(), IconButtonStyle::Outlined);
        assert_eq!(IconToggleButton::new(true).checked(false).get_checked(), false);
        assert_eq!(IconToggleButton::new(true).get_size(), IconButtonSize::Small);
        assert_eq!(
            IconToggleButton::new(true).size(IconButtonSize::XLarge).get_size(),
            IconButtonSize::XLarge
        );
        // 尺寸表与 IconButton 共用同一 token 表
        assert_eq!(IconToggleButton::new(true).size(IconButtonSize::XSmall).get_size().container_size(), 32.0);
        assert_eq!(IconToggleButton::new(true).size(IconButtonSize::Large).get_size().icon_size(), 32.0);
    }

    #[test]
    fn checked_content_color_propagates_to_icon() {
        // 集成：Filled checked 内 Icon（tint Auto）解析为 on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::filled(true)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut tint = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    tint = spec.tint;
                }
            }
        }
        assert_eq!(tint, Some(theme.on_primary), "checked 内容色下传 Icon tint");
    }

    #[test]
    fn disabled_toggle_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            IconToggleButton::new(false)
                .enabled(false)
                .on_checked_change(|_| {})
                .build(ctx, |ctx| {
                    crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Ripple { .. })
                        && !matches!(el, ModifierElement::Focusable { .. }),
                    "禁用 Toggle 不应有波纹/可聚焦元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn outlined_checked_has_no_border_element() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            IconToggleButton::outlined(true)
                .on_checked_change(|_| {})
                .build(ctx, |ctx| {
                    crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Border { .. }),
                    "Outlined checked 节点不应有 Border 元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn toggle_callback_receives_inverted_checked() {
        use crate::modifier::ModifierElement;
        use std::sync::atomic::{AtomicBool, Ordering};
        // 未选中 → 点击回调应收到 true（取反）
        for (checked, expect) in [(false, true), (true, false)] {
            let received = Arc::new(AtomicBool::new(false));
            let cb = received.clone();
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                IconToggleButton::new(checked)
                    .on_checked_change(move |v| cb.store(v, Ordering::Relaxed))
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
            let mut clicked = false;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    if let ModifierElement::Clickable { on_click, .. } = el {
                        on_click();
                        clicked = true;
                    }
                }
            }
            assert!(clicked, "存在 Clickable");
            assert_eq!(received.load(Ordering::Relaxed), expect, "回调收到 !checked");
        }
    }

    /// A CHECKED toggle is not the resting circle. Upstream's default toggle shapes are
    /// (`IconButtonDefaults.kt:1046-1055`)
    ///
    ///     shape        = SmallIconButtonTokens.ContainerShapeRound         (CornerFull)
    ///     pressedShape = SmallIconButtonTokens.PressedContainerShape       (CornerSmall)
    ///     checkedShape = SmallIconButtonTokens.SelectedContainerShapeRound (CornerMedium)
    ///
    /// and `shapeByInteraction` picks PRESSED > CHECKED > resting (`IconButton.kt:1622-1638`). The
    /// `SelectedContainerShape*` tokens went unused here, so a checked toggle kept its resting
    /// circle; deleting the checked branch fails this test.
    #[test]
    fn a_checked_toggle_morphs_to_the_selected_container_shape() {
        // `update_animations` drives the process-wide table, so this holds the same lock the other
        // animation tests take.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let on = checked.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(on)
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let radius = |composer: &crate::runtime::composer::Composer| -> f32 {
            let painted = composer.arena_nodes().iter().find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { shape, .. } => Some(*shape),
                    _ => None,
                })
            });
            match painted {
                Some(Shape::RoundedRect { corner_radius }) => corner_radius,
                other => panic!("the container should be a rounded rect, got {other:?}"),
            }
        };

        build(&mut composer);
        assert_eq!(
            radius(&composer),
            20.0,
            "unchecked, Small's container is CornerFull over its 40 dp box"
        );

        checked.set(true);
        build(&mut composer);
        // The morph must be REGISTERED, and the corners must not have switched yet — the same shape
        // the press test above takes, and the reason its `build` sits before this loop: with
        // nothing animating, `update_animations` returns false on the first call and the loop would
        // exit before the build that starts the morph ever ran.
        assert!(
            crate::animation::is_animating(),
            "switching checked registers the shape morph"
        );
        assert_ne!(
            radius(&composer),
            12.0,
            "the checked shape arrives over time instead of switching"
        );
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            (radius(&composer) - 12.0).abs() < 0.5,
            "checked, it arrives at Small's SelectedContainerShapeRound (CornerMedium = 12), got {}",
            radius(&composer)
        );
    }
}
