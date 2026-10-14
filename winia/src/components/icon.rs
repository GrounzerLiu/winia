//! Icon 组件与矢量图标基础设施。
//!
//! 与 Compose `Icon` 的差异（有意设计）：
//! - 默认不内置任何图标（可变字体图标需启用 `material-symbols-*` feature）；
//! - 支持四种来源：SVG path 字符串、完整 SVG 文档字符串、图片文件路径、
//!   可变字体符号（feature 启用时）；
//! - `auto_mirror` 是显式属性（默认关），开启后跟随布局方向（RTL）镜像；
//! - 可变字体轴 FILL / GRAD / opsz / wght 支持动画（渲染期求值，只重绘不重组）。
//!
//! 所有 SVG 渲染统一走 Skia 内置 `svg::Dom`（不自研路径解析）：
//! 裸 path 数据会被包成最小 `<svg viewBox="0 0 24 24">` 文档后交给 Dom。

use crate::graphics::{IconSource, IconSpec, SymbolAxes};
use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::modifier::{Modifier};
use crate::graphics::{Color};
use crate::theme::WiniaTheme;
use std::sync::Arc;

// Tint（三态：Auto 单色源染主题色 / 多色源不染；可显式覆盖）
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    /// Tints the single-colour sources — `SvgPath`, `Svg` and `Symbol` — with the current content colour;
    /// `File` keeps its own colours.
    ///
    /// The tint is a layer-wide `SrcIn` filter (`render.rs:558-563`), so an SVG *document* carrying several
    /// fills is flattened to one colour and nothing reports it. Compose behaves the same way and warns about
    /// it in `Icon`'s KDoc; use `Tint::None` when the artwork is meant to be multi-coloured.
    Auto,
    Color(Color),
    None,
}

impl From<Color> for Tint {
    fn from(c: Color) -> Self {
        Tint::Color(c)
    }
}

impl From<Option<Color>> for Tint {
    fn from(o: Option<Color>) -> Self {
        match o {
            Some(c) => Tint::Color(c),
            None => Tint::None,
        }
    }
}

impl Tint {
    /// Auto：单色源染当前内容色（默认主题 on_surface），File 保留原色
    fn resolve(self, source: &IconSource, default_content: Color) -> Option<Color> {
        match self {
            Tint::Auto => source.default_tinted().then_some(default_content),
            Tint::Color(c) => Some(c),
            Tint::None => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
/// Icon 组件——多来源、tint、autoMirror、可变轴（feature 启用时）。
///
/// ```ignore
/// Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
///     .tint(theme.primary)
///     .build(ctx);
/// ```
pub struct Icon {
    source: IconSource,
    tint: Tint,
    auto_mirror: bool,
    size: Option<crate::layout::SizeValue>,
    content_description: Option<String>,
    axes: SymbolAxes,
    modifier: Modifier,
}

impl Icon {
    pub fn new(source: impl Into<IconSource>) -> Self {
        Self {
            source: source.into(),
            tint: Tint::Auto,
            auto_mirror: false,
            size: None,
            content_description: None,
            axes: SymbolAxes::default(),
            modifier: Modifier::new(),
        }
    }

    pub fn svg_path(data: impl Into<Arc<str>>) -> Self {
        Self::new(IconSource::svg_path(data))
    }

    pub fn svg(data: impl Into<Arc<str>>) -> Self {
        Self::new(IconSource::svg(data))
    }

    pub fn file(path: impl Into<Arc<str>>) -> Self {
        Self::new(IconSource::file(path))
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn symbol(symbol: crate::icon::MaterialSymbol) -> Self {
        Self::new(IconSource::symbol(symbol))
    }

    pub fn tint(mut self, tint: impl Into<Tint>) -> Self {
        self.tint = tint.into();
        self
    }

    /// 开启后图标跟随布局方向（RTL 时水平镜像）。默认 false。
    pub fn auto_mirror(mut self, enabled: bool) -> Self {
        self.auto_mirror = enabled;
        self
    }

    /// Override the size. Takes whatever `Modifier::size` takes — `f32`, `Dimension`, or a `SizeValue`
    /// including a `Dynamic` closure — so an icon can follow a size animated at LAYOUT time without
    /// recomposing. Unset, the source's own measurement is used (24×24 when it has none).
    pub fn size(mut self, size: impl Into<crate::layout::SizeValue>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// The node's accessible name. `build` publishes it as an `Image` semantics node, so a described icon
    /// is announced by that name; an icon WITHOUT one publishes nothing at all, which is Compose's
    /// decorative case (`material3/Icon.kt:146-154`) — not the same as being announced as empty.
    /// (This doc used to say the description was merely stored until the semantics tree existed.)
    pub fn content_description(mut self, desc: impl Into<String>) -> Self {
        self.content_description = Some(desc.into());
        self
    }

    pub fn content_description_value(&self) -> Option<&str> {
        self.content_description.as_deref()
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn fill(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.fill = v.into();
        self
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn grad(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.grad = v.into();
        self
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn opsz(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.opsz = v.into();
        self
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn wght(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.wght = v.into();
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.source);
        ctx.changed(&self.tint);
        ctx.changed(&self.auto_mirror);
        ctx.changed(&self.axes);
        // A `Symbol` is a monochrome glyph with no colour of its own, so `Tint::None` would paint it BLACK and
        // make it vanish on a dark surface. That one source gets the content colour; the sources that DO carry
        // colours — an SVG document, an image file — keep `Tint::None`'s literal meaning of "leave me alone",
        // which is the case that option exists for.
        let content_color = WiniaTheme::content_color();
        let tint = match self.tint {
            Tint::None if self.source.carries_no_colour() => Some(content_color),
            other => other.resolve(&self.source, content_color),
        };
        // An explicit size applies to both axes; otherwise the source's own measurement wins, which for an
        // SVG document need not be square.
        let (width, height) = match self.size.clone() {
            Some(size) => (size.clone(), size),
            None => {
                let (w, h) = self.source.intrinsic_size().unwrap_or((24.0, 24.0));
                (
                    crate::layout::SizeValue::from(w),
                    crate::layout::SizeValue::from(h),
                )
            }
        };
        let spec = IconSpec {
            source: self.source,
            tint,
            auto_mirror: self.auto_mirror,
            axes: self.axes,
        };
        // An icon that was given a description is an image with a name; one without is decorative
        // and must stay out of the semantics tree entirely (Compose: a null `contentDescription` is
        // "not announced", which is not the same as "announced as empty").
        // The caller's modifier goes FIRST so the component's own semantics land after it — the order
        // `image.rs` uses, and the one that matters when a caller supplies their own `content_description`:
        // the component's name wins either way, instead of depending on which component you reached for.
        let mut modifier = self.modifier.then(Modifier::new().size(width, height));
        if let Some(description) = self.content_description.as_deref() {
            modifier = modifier.semantics(
                crate::semantics::SemanticsConfig::new()
                    .role(crate::semantics::SemanticsRole::Image)
                    .content_description(description),
            );
        }
        let modifier = modifier.draw_icon(spec);
        let key = ctx.next_key();
        // `start_leaf` / `end_node`, as `image.rs` does. A restartable group around an EMPTY body only adds
        // group bookkeeping — the body has already run — and reads like a body that was skipped. The
        // alignment is dead config too: a childless node is centred by `fit_rect`, not by its layout.
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeColors;



    #[test]
    fn tint_auto_resolves_by_source() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let path_src = IconSource::svg_path("M0 0h1z");
        assert_eq!(Tint::Auto.resolve(&path_src, theme.on_surface), Some(theme.on_surface));
        let file_src = IconSource::file("logo.png");
        assert_eq!(Tint::Auto.resolve(&file_src, theme.on_surface), None, "a file keeps its own colours");
        assert_eq!(Tint::None.resolve(&path_src, theme.on_surface), None);
        assert_eq!(Tint::Color(Color::RED).resolve(&file_src, theme.on_surface), Some(Color::RED));
    }

    /// Two frames, the pattern `icon_button.rs` uses for the same class of bug: a leaf that Skips still has
    /// to receive the CURRENT frame's modifier, and a changed parameter has to re-enter. Worth pinning here
    /// because `Icon` used to wrap an already-executed body in a restartable group.
    #[test]
    fn a_skipping_frame_still_paints_and_a_changed_tint_re_enters() {
        use crate::runtime::composer::Composer;
        use crate::runtime::state::State;

        let theme = ThemeColors::light_from_seed(0x6750A4);
        let tint = State::new(Color::RED);
        let mut composer = Composer::new();
        let mut compose = |composer: &mut Composer| {
            let chosen = tint.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    Icon::svg_path("M0 0h1v1z").tint(chosen).build(ctx);
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        let painted_tint = |composer: &Composer| -> Option<Color> {
            composer.arena_nodes().iter().find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::DrawIcon { spec } => spec.tint,
                    _ => None,
                })
            })
        };

        compose(&mut composer);
        assert_eq!(painted_tint(&composer), Some(Color::RED), "the first frame paints the tint");

        // Unchanged parameters: this frame Skips, and the modifier must still be applied.
        compose(&mut composer);
        assert_eq!(painted_tint(&composer), Some(Color::RED), "a skipping frame still paints");

        tint.update(|c| *c = Color::BLUE);
        compose(&mut composer);
        assert_eq!(
            painted_tint(&composer),
            Some(Color::BLUE),
            "a changed tint re-enters the leaf"
        );
    }
}
