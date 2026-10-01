//! Surface 组件 — 对齐 Compose material3 `Surface`
//!
//! 对标 Compose `Surface` 的职责（源码注释原文）：
//! 1. **Clipping**——按 `shape` 裁剪子节点
//! 2. **Borders**——`shape` 有边框则绘制
//! 3. **Background**——按 `shape` 填充 `color`（`surface` 色叠加 tonal overlay：
//!    仅当底色恰好等于 `theme.surface` 且 `tonal_elevation > 0`、且 tonal 开关打开时，
//!    才按 Compose `ColorScheme.applyTonalElevation` 把 `surface_tint` 叠上去）
//! 4. **Content color**——`content_color` 作为子内容（Text/Icon）默认色；未设时
//!    按主题匹配（`color == theme.surface` → `on_surface`，否则保持上层值）
//! 5. Blocking touch propagation behind the surface
//!
//! 与 Card 的区别：`Surface` 是**通用容器原语**（无 M3 Card 的状态化取色），
//! 只负责「外观底板」——shadow → border → background → clip，外加三个可选交互重载：
//! `on_click`（clickable）、`selectable(selected, on_click)`、`toggleable(checked, on_checked_change)`。
//! 面板（BottomSheet）用 `Surface` 承载 anchoredDraggable + nestedScroll（对齐
//! Compose：`Surface { .nestedScroll(...).anchoredDraggable(...) }`）。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::checkbox::ToggleableState;
use std::sync::Arc;

/// `ColorScheme.surfaceColorAtElevation`（Compose `ColorScheme.kt:1125-1129`）——按 elevation 把
/// `surfaceTint` 叠在 `surface` 上。公式与 Compose 逐字一致：`alpha = ((4.5·ln(elev+1)) + 2) / 100`，
/// 再 `surfaceTint(alpha).compositeOver(surface)`；winia 用 `Color::overlay` 做同一个 alpha 合成。
/// elevation 为 0 时原样返回 `surface`。
fn surface_color_at_elevation(theme: crate::ui::theme::ThemeColors, elevation: f32) -> Color {
    if elevation <= 0.0 {
        return theme.surface;
    }
    let alpha = ((4.5 * (elevation + 1.0).ln()) + 2.0) / 100.0;
    theme.surface.overlay(theme.surface_tint, alpha)
}

/// 边框描边（对齐 Compose `BorderStroke(width, color)`——winia 用 (f32, Color) 表达；
/// shape 在绘制时传入）。为与 Compose Surface 参数名一致，此处用 `border: Option<Border>`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceBorder {
    pub width: f32,
    pub color: Color,
}

impl SurfaceBorder {
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

/// 交互模式（对齐 Compose `Surface` 三个交互重载，但 winia 无 selectable/toggleable
/// modifier 原语——统一用 clickable_with_source + ripple 表达，语义差异走回调分发）。
#[derive(Clone)]
enum SurfaceInteraction {
    /// 纯展示（不可交互、无波纹）——对标 Compose `Surface()`
    None,
    /// 可点击——对标 Compose `Surface(onClick=...)`
    Click(Arc<dyn Fn() + Send + Sync>),
    /// 可选中（`selected` + onClick）——对标 Compose `Surface(selected, onClick=...)`
    Select { selected: bool, on_click: Arc<dyn Fn() + Send + Sync> },
    /// 可切换（`checked` + onCheckedChange）——对标 Compose `Surface(checked, onCheckedChange=...)`
    Toggle { checked: ToggleableState, on_checked_change: Arc<dyn Fn(ToggleableState) + Send + Sync> },
}

/// 通用外观底板（对标 Compose material3 `Surface`）。
pub struct Surface {
    shape: Shape,
    color: Option<Color>,
    content_color: Option<Color>,
    tonal_elevation: f32,
    shadow_elevation: f32,
    border: Option<SurfaceBorder>,
    modifier: Modifier,
    /// 交互模式（None = 纯展示；Click/Select/Toggle = 三个交互重载）
    interaction: SurfaceInteraction,
    /// 是否启用（对齐 Compose `enabled`——禁用时不响应交互；Surface 本身无
    /// disabled 容器色变体，视觉由调用方处理）
    enabled: bool,
    /// 交互源（None = build 时内部 remember——对标 Compose 可选注入）
    interaction_source: Option<MutableInteractionSource>,
}

impl Surface {
    /// 创建 Surface，默认 `shape=Rectangle`、`color=theme.surface`。
    pub fn new() -> Self {
        Self {
            shape: Shape::Rectangle,
            color: None,
            content_color: None,
            tonal_elevation: 0.0,
            shadow_elevation: 0.0,
            border: None,
            modifier: Modifier::new(),
            interaction: SurfaceInteraction::None,
            enabled: true,
            interaction_source: None,
        }
    }

    /// 表面形状（也是阴影/裁剪形状）。
    pub fn shape(mut self, shape: impl Into<Shape>) -> Self {
        self.shape = shape.into();
        self
    }

    /// 背景色。不传默认 `theme.surface`（Compose 默认 `MaterialTheme.colorScheme.surface`）。
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// 内容色（下传给 Text/Icon 作为默认色）。不传时按主题匹配：
    /// `color == theme.surface` → `on_surface`，否则保持上层值（Compose 语义）。
    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }

    /// 色调抬升（仅影响表面色叠加；winia 简化版先保留字段，后续可接 tonal 色）。
    pub fn tonal_elevation(mut self, elevation: f32) -> Self {
        self.tonal_elevation = elevation;
        self
    }

    /// 阴影高度（对齐 Compose `shadowElevation`——lift 到图形层阴影）。
    pub fn shadow_elevation(mut self, elevation: f32) -> Self {
        self.shadow_elevation = elevation;
        self
    }

    /// 描边（对齐 Compose `border: BorderStroke?`）。
    pub fn border(mut self, border: SurfaceBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 追加用户 modifier（外层，可覆盖默认样式）。
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 是否启用（默认 true）。对齐 Compose `enabled`——禁用时不响应交互；
    /// Surface 本身无 disabled 容器色变体（视觉由调用方处理）。
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 可点击 Surface（对标 Compose `Surface(onClick=...)`——clickable 重载）。
    pub fn on_click(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.interaction = SurfaceInteraction::Click(Arc::new(cb));
        self
    }

    /// 可选中 Surface（对标 Compose `Surface(selected, onClick=...)`——selectable 重载）。
    pub fn selectable(mut self, selected: bool, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        self.interaction = SurfaceInteraction::Select { selected, on_click: Arc::new(on_click) };
        self
    }

    /// 可切换 Surface（对标 Compose `Surface(checked, onCheckedChange=...)`——toggleable 重载）。
    pub fn toggleable(mut self, checked: bool, on_checked_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        let state = if checked { ToggleableState::On } else { ToggleableState::Off };
        let cb = Arc::new(move |s: ToggleableState| {
            on_checked_change(s == ToggleableState::On);
        });
        self.interaction = SurfaceInteraction::Toggle { checked: state, on_checked_change: cb };
        self
    }

    /// 注入交互源（None = build 时内部 remember——对齐 Compose 可选注入）。
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// 构建 Surface。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // `enabled` only switches the interaction branch (no numeric modifier
        // change), so declare it — same closure-invisible class as
        // TextField::read_only (see text_field.rs build).
        ctx.changed(&self.enabled);
        let key = ctx.next_key();
        let theme = crate::ui::theme::WiniaTheme::colors();
        let shape = self.shape;
        // 默认背景 = theme.surface（Compose 默认 `colorScheme.surface`）
        let base_color = self.color.unwrap_or(theme.surface);
        // tonal overlay（对标 Compose `ColorScheme.applyTonalElevation` / `surfaceColorAtElevation`）：
        // **仅当底色恰好等于 theme.surface 且 tonal 开关打开时**才把 `surface_tint` 按 elevation 叠上去
        // （`ColorScheme.kt:1540-1543`, `:1125-1129`）。底色不是 surface（如对话框的
        // surfaceContainerHigh）则不上色——这正是 Compose 的行为，也是 AlertDialog 传了
        // `tonalElevation` 也是空转的原因。
        //
        // The elevation that decides the alpha is the ABSOLUTE one, not this surface's own: Compose
        // sums the ancestors first (`LocalAbsoluteTonalElevation.current + tonalElevation`, then
        // `provides` for the subtree — `Surface.kt:106,109` and the same three lines in the other
        // overloads at `:211`、`:317`、`:424`). The stated reason is `Surface.kt:146-150`: a Surface
        // must never look LESS raised than its ancestors. Tinting from the local value instead made
        // every nested surface in a stack read as flat as its parent.
        let absolute_elevation = crate::ui::theme::WiniaTheme::absolute_tonal_elevation()
            + self.tonal_elevation;
        // The gate is the COLOUR and the switch, never this surface's own elevation. Compose's
        // `applyTonalElevation` has no elevation term at all (`ColorScheme.kt:1540-1543`) — the only
        // zero-guard is inside `surfaceColorAtElevation` (`ColorScheme.kt:1126`, `if (elevation == 0.dp)
        // return surface`), and `Surface.kt:106` hands it the ABSOLUTE number. Testing the local value
        // here therefore left a `Surface(0)` nested in a `Surface(3)` flat while Compose tints it, which
        // is precisely what `Surface.kt:146-150` says the local exists to prevent.
        let color = if base_color == theme.surface
            && crate::ui::theme::WiniaTheme::tonal_elevation_enabled()
        {
            surface_color_at_elevation(theme, absolute_elevation)
        } else {
            base_color
        };
        // 内容色：显式传入优先；否则按主题匹配——color==theme.surface→on_surface，
        // 否则保持上层 content_color()（Compose 语义：非标准色时沿用父 Surface 内容色）。
        // NOTE：判据用 base_color（未经 tint 的原始底色），与 Compose 的
        // `applyTonalElevation` 一致——它比较的也是传入的 backgroundColor。
        let content_color = self.content_color.unwrap_or_else(|| {
            if base_color == theme.surface {
                theme.on_surface
            } else {
                crate::ui::theme::WiniaTheme::content_color()
            }
        });
        let shadow_elevation = self.shadow_elevation;
        // 交互源：外部注入或内部 remember（对齐 Compose Surface 可选注入——同 Card 惯例，
        // 纯展示 Surface 也 remember 一个空源，开销极小）。
        let interaction = self.interaction_source.unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());

        // Compose `Modifier.surface`（核心可复用链）：shadow → border → background → clip。
        // - shadow：仅 shadow_elevation > 0 时用 graphics_layer 阴影（对齐 Compose `graphicsLayer{shadowElevation, clip=false}`）
        // - border：`border(width, color, shape)`
        // - background：`background(color, shape)`
        // - clip：`clip(shape)`
        let mut modifier = Modifier::new();
        if shadow_elevation > 0.0 {
            modifier = modifier.graphics_layer(move || crate::modifier::GraphicsLayerParams {
                shadow_elevation,
                shadow_shape: Some(shape),
                ..Default::default()
            });
        }
        if let Some(b) = self.border {
            modifier = modifier.border(b.width, b.color, shape);
        }
        modifier = modifier.background(color, shape);
        modifier = modifier.clip(shape);

        // 交互重载：enabled 且非纯展示 → clickable_with_source + ripple（用容器 shape 裁剪）。
        // 回调分发：Click→on_click；Select→selected 翻转后 on_click；Toggle→checked 翻转后 on_checked_change。
        // 对齐 Compose：selectable/toggleable 在点击时切换 selected/checked 并回调。
        let may_interact = self.enabled && !matches!(self.interaction, SurfaceInteraction::None);
        if may_interact {
            match &self.interaction {
                SurfaceInteraction::Click(cb) => {
                    let cb = cb.clone();
                    modifier = modifier.clickable_with_source(&interaction, move || cb());
                }
                SurfaceInteraction::Select { selected, on_click } => {
                    let selected = *selected;
                    let on_click = on_click.clone();
                    modifier = modifier.clickable_with_source(&interaction, move || {
                        // Compose selectable：点击后状态由外部持有；此处仅回调 onClick
                        let _ = selected;
                        on_click();
                    });
                }
                SurfaceInteraction::Toggle { checked, on_checked_change } => {
                    let checked = *checked;
                    let cb = on_checked_change.clone();
                    modifier = modifier.clickable_with_source(&interaction, move || {
                        let next = if checked == ToggleableState::On { ToggleableState::Off } else { ToggleableState::On };
                        cb(next);
                    });
                }
                SurfaceInteraction::None => {}
            }
            modifier = modifier.ripple_with_shape(&interaction, content_color, true, shape);
        }

        // 追加用户 modifier（外层）
        modifier = modifier.then(self.modifier);

        // content 闭包为组合 scope；布局策略 = BoxLayout（对标 Compose Box）。
        // Box 无方向参数（层叠堆叠——子节点共享空间），Direction 走 modifier 层。
        match ctx.start_restartable_group(
            key,
            modifier,
            crate::layout::BoxLayout::new(),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 内容色下传（LocalContentColor 等价物——future Text/Icon 默认色）
                crate::ui::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    // And the absolute elevation down with it, so a nested Surface tints from the sum
                    // rather than from its own number alone (`Surface.kt:109`).
                    crate::ui::theme::WiniaTheme::with_absolute_tonal_elevation(
                        absolute_elevation,
                        ctx,
                        |ctx| {
                            content(ctx);
                        },
                    );
                });
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for Surface {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::Constraints;
    use crate::ui::theme::{ThemeColors, WiniaTheme};

    /// Compose a surface under `theme` (and optionally with the tonal toggle) and return the colour its
    /// background modifier actually resolves to — the painted value, read through the same
    /// `Background { color_fn }` the renderer evaluates (`render.rs:436`).
    fn painted_background(
        theme: ThemeColors,
        tonal_enabled: bool,
        build: impl FnOnce(&mut ComposeCtx) -> Surface,
    ) -> Color {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme, ctx, |ctx| {
                WiniaTheme::with_tonal_elevation_enabled(tonal_enabled, ctx, |ctx| {
                    build(ctx).build(ctx, |_| {});
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        composer
            .arena_nodes()
            .iter()
            .find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { color_fn, .. } => Some(color_fn()),
                    _ => None,
                })
            })
            .expect("a surface paints a background")
    }

    /// Compose's published alphas, from `alpha = ((4.5·ln(elev+1)) + 2) / 100` over the
    /// `ElevationTokens` levels (`ElevationTokens.kt:24-29`, Level0 at `:24` through Level5 at `:29`).
    fn compose_alpha(elevation: f32) -> f32 {
        ((4.5 * (elevation + 1.0).ln()) + 2.0) / 100.0
    }

    /// Every background the tree paints, outermost first — the same read as [`painted_background`], for
    /// the cases where there is more than one surface and they are not interchangeable.
    fn painted_backgrounds(
        theme: ThemeColors,
        tonal_enabled: bool,
        build: impl FnOnce(&mut ComposeCtx),
    ) -> Vec<Color> {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme, ctx, |ctx| {
                WiniaTheme::with_tonal_elevation_enabled(tonal_enabled, ctx, |ctx| build(ctx));
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        composer
            .arena_nodes()
            .iter()
            .filter_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { color_fn, .. } => Some(color_fn()),
                    _ => None,
                })
            })
            .collect()
    }

    /// A nested Surface tints from the SUM of its ancestors' elevations, not from its own number alone
    /// (`Surface.kt:106,109`; the reason is at `:146-150` — a Surface must never look less raised than
    /// its ancestors). Reading the local value made a 3dp-inside-3dp surface paint the same 8.24% as
    /// its parent, which is to say it looked perfectly flat.
    #[test]
    fn a_nested_surface_tints_from_its_ancestors_elevation_too() {
        let theme = ThemeColors::default_light();
        let backgrounds = painted_backgrounds(theme.clone(), true, |ctx| {
            Surface::new().tonal_elevation(3.0).build(ctx, |ctx| {
                Surface::new().tonal_elevation(3.0).build(ctx, |_| {});
            });
        });
        assert_eq!(backgrounds.len(), 2, "both surfaces paint, got {backgrounds:?}");
        assert_eq!(
            backgrounds[0],
            theme.surface.overlay(theme.surface_tint, compose_alpha(3.0)),
            "the outer surface is unchanged — nothing above it"
        );
        assert_eq!(
            backgrounds[1],
            theme.surface.overlay(theme.surface_tint, compose_alpha(6.0)),
            "the inner surface adds its 3dp to the outer's, so it reads 6dp and not 3dp"
        );
        assert_ne!(
            backgrounds[1], backgrounds[0],
            "a nested surface that paints exactly its parent's tint is the bug"
        );

        // Three deep, to show it is a sum rather than a two-level special case.
        let deep = painted_backgrounds(theme.clone(), true, |ctx| {
            Surface::new().tonal_elevation(1.0).build(ctx, |ctx| {
                Surface::new().tonal_elevation(1.0).build(ctx, |ctx| {
                    Surface::new().tonal_elevation(1.0).build(ctx, |_| {});
                });
            });
        });
        assert_eq!(deep.len(), 3, "all three paint, got {deep:?}");
        for (depth, painted) in deep.iter().enumerate() {
            let elevation = depth as f32 + 1.0;
            assert_eq!(
                *painted,
                theme.surface.overlay(theme.surface_tint, compose_alpha(elevation)),
                "surface {depth} should read {elevation}dp"
            );
        }

        // And a surface that adds NOTHING of its own still inherits what is above it. This is the case
        // the gate used to get wrong: it tested this surface's own elevation, so a `tonal_elevation(0)`
        // child of a `tonal_elevation(3)` parent painted flat while Compose tints it — the one shape
        // where "local" and "absolute" disagree.
        let zero_child = painted_backgrounds(theme.clone(), true, |ctx| {
            Surface::new().tonal_elevation(3.0).build(ctx, |ctx| {
                Surface::new().tonal_elevation(0.0).build(ctx, |_| {});
            });
        });
        assert_eq!(zero_child.len(), 2, "both paint, got {zero_child:?}");
        assert_eq!(
            zero_child[1], zero_child[0],
            "a surface that declares no elevation of its own still reads its parent's 3dp"
        );
        assert_eq!(
            zero_child[1],
            theme.surface.overlay(theme.surface_tint, compose_alpha(3.0)),
            "and that is the 3dp tint, not a flat surface"
        );

        // The converse still holds: with nothing above it, an absolute elevation of zero is a no-op,
        // which is what `ColorScheme.kt:1126` does the deciding for.
        let zero_root = painted_backgrounds(theme.clone(), true, |ctx| {
            Surface::new().tonal_elevation(0.0).build(ctx, |_| {});
        });
        assert_eq!(zero_root, vec![theme.surface], "a lone 0dp surface paints plain surface");
    }

    #[test]
    fn a_surface_at_a_tonal_elevation_is_tinted_toward_the_surface_tint() {
        // Level2 = 3dp (`ElevationTokens.kt:26`), which is the menu's own shadow elevation.
        let theme = ThemeColors::default_light();
        let level = 3.0f32;
        let painted = painted_background(theme.clone(), true, |_| {
            Surface::new().tonal_elevation(level)
        });
        assert_eq!(
            painted,
            theme.surface.overlay(theme.surface_tint, compose_alpha(level)),
            "Level2 tints surface by the formula's alpha"
        );
        // And the formula is the documented one, not just self-consistent: 3dp lands at 8.24%.
        assert!(
            (compose_alpha(level) - 0.0824).abs() < 0.0005,
            "the Level2 alpha is ~8.24%, got {}",
            compose_alpha(level)
        );
        assert_ne!(painted, theme.surface, "a tinted surface differs from plain surface");
    }

    #[test]
    fn tonal_elevation_does_nothing_when_the_color_is_not_surface() {
        // Compose's `applyTonalElevation` gate (`ColorScheme.kt:1542`): only a background that IS
        // `surface` is tinted. This is why an AlertDialog — whose container is
        // `surfaceContainerHigh` — gains nothing from a `tonalElevation`.
        let theme = ThemeColors::default_light();
        for color in [
            theme.surface_container_high,
            theme.surface_container_highest,
            theme.primary,
        ] {
            let painted = painted_background(theme.clone(), true, |_| {
                Surface::new().color(color).tonal_elevation(3.0)
            });
            assert_eq!(
                painted, color,
                "a non-surface background is left alone (Compose does the same)"
            );
        }
    }

    #[test]
    fn zero_elevation_and_the_tonal_switch_both_leave_the_color_alone() {
        let theme = ThemeColors::default_light();

        let plain = painted_background(theme.clone(), true, |_| Surface::new());
        assert_eq!(plain, theme.surface, "no elevation leaves surface as it is");

        let zero = painted_background(theme.clone(), true, |_| {
            Surface::new().tonal_elevation(0.0)
        });
        assert_eq!(zero, theme.surface, "an explicit zero is the same as none");

        let off = painted_background(theme.clone(), false, |_| {
            Surface::new().tonal_elevation(3.0)
        });
        assert_eq!(
            off, theme.surface,
            "LocalTonalElevationEnabled = false suppresses the tint for the subtree"
        );
    }

    #[test]
    fn an_explicit_content_colour_still_follows_the_untinted_base_colour() {
        // The content-colour rule compares the BASE colour (`color == surface`), not the tinted one, so
        // raising the elevation does not silently change the content colour a surface hands down.
        let theme = ThemeColors::default_light();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Surface::new()
                    .tonal_elevation(3.0)
                    .build(ctx, |ctx| {
                        assert_eq!(
                            WiniaTheme::content_color(),
                            theme.on_surface,
                            "a tinted surface still provides on_surface"
                        );
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    }

    #[test]
    fn the_tint_reaches_the_rendered_pixels() {
        // The modifier assertions above read what the surface ASKED for. This one reads what it PAINTS:
        // a 100x100 tinted surface rendered at its centre, against the same surface flat, so the two
        // differ by exactly the tint's alpha ramp and not by anything the layout did.
        use skia_safe::{Color as SkColor, surfaces};

        fn centre_pixel(theme: ThemeColors, tonal_enabled: bool, elevation: f32) -> (u8, u8, u8) {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme, ctx, |ctx| {
                    WiniaTheme::with_tonal_elevation_enabled(tonal_enabled, ctx, |ctx| {
                        Surface::new()
                            .modifier(Modifier::new().size(100.0, 100.0))
                            .tonal_elevation(elevation)
                            .build(ctx, |_| {});
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((400, 300)).expect("surface");
            let canvas = surface.canvas();
            canvas.clear(SkColor::TRANSPARENT);
            let root = composer.layout_root_idx().expect("root");
            crate::render::render(composer.arena_nodes(), root, canvas);
            let pixmap = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pixmap.pixels::<[u8; 4]>().expect("pixels");
            let p = px[50 * 400 + 50];
            // `raster_n32_premul` is BGRA in memory — read it back as RGB (the convention the other
            // ui pixel tests use, e.g. `badge.rs:423`).
            (p[2], p[1], p[0])
        }

        let theme = ThemeColors::default_light();
        let flat = centre_pixel(theme.clone(), true, 0.0);
        let tinted = centre_pixel(theme.clone(), true, 3.0);
        let suppressed = centre_pixel(theme.clone(), false, 3.0);

        assert_eq!(flat, (theme.surface.r, theme.surface.g, theme.surface.b));
        assert_ne!(tinted, flat, "the elevation tints the painted pixels");
        assert_eq!(
            suppressed, flat,
            "and the tonal switch puts them back to flat surface"
        );
        // The ramp: each channel moves from surface toward surface_tint by the formula's alpha. Compared
        // with a small tolerance because this reads the RASTERISER's output — Skia composites in
        // premultiplied space and rounds there, while `Color::overlay` is the exact float lerp the
        // modifier carries (asserted exactly in the test above). One unit of drift is expected.
        let alpha = compose_alpha(3.0);
        for (flat_c, tint_c, tinted_c, name) in [
            (theme.surface.r, theme.surface_tint.r, tinted.0, "r"),
            (theme.surface.g, theme.surface_tint.g, tinted.1, "g"),
            (theme.surface.b, theme.surface_tint.b, tinted.2, "b"),
        ] {
            let expected =
                (flat_c as f32 * (1.0 - alpha) + tint_c as f32 * alpha).round() as i32;
            assert!(
                (tinted_c as i32 - expected).abs() <= 1,
                "channel {name} ramps from surface toward surface_tint at alpha {alpha}: \
                 expected ~{expected}, painted {}",
                tinted_c
            );
        }
    }
}
