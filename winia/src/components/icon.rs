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

use crate::graphics::{AxisValue, IconSource, IconSpec, PathFillType, SymbolAxes, decoded_icon};
use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier};
use crate::theme::{ThemeColors, WiniaTheme};
use skia_safe::svg;
use std::collections::HashMap;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use std::fmt;
use std::sync::{Arc, LazyLock};
use parking_lot::Mutex;

// Tint（三态：Auto 单色源染主题色 / 多色源不染；可显式覆盖）
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    /// 单色源（SvgPath/Svg/Symbol）染主题 on_surface；File 保留原色
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
    size: Option<f32>,
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

    /// 覆盖尺寸（默认按源固有尺寸，失败回退 24×24）
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// 无障碍描述（当前存储；semantics 树实现后接入）
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
        let tint = self.tint.resolve(&self.source, WiniaTheme::content_color());
        let (w, h) = self
            .size
            .map(|s| (s, s))
            .or_else(|| self.source.intrinsic_size())
            .unwrap_or((24.0, 24.0));
        let spec = IconSpec {
            source: self.source,
            tint,
            auto_mirror: self.auto_mirror,
            axes: self.axes,
        };
        // An icon that was given a description is an image with a name; one without is decorative
        // and must stay out of the semantics tree entirely (Compose: a null `contentDescription` is
        // "not announced", which is not the same as "announced as empty").
        let mut modifier = Modifier::new().size(w, h);
        if let Some(description) = self.content_description.as_deref() {
            modifier = modifier.semantics(
                crate::semantics::SemanticsConfig::new()
                    .role(crate::semantics::SemanticsRole::Image)
                    .content_description(description),
            );
        }
        let modifier = modifier.draw_icon(spec).then(self.modifier);
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn tint_auto_resolves_by_source() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let path_src = IconSource::svg_path("M0 0h1z");
        assert_eq!(Tint::Auto.resolve(&path_src, theme.on_surface), Some(theme.on_surface));
        let file_src = IconSource::file("logo.png");
        assert_eq!(Tint::Auto.resolve(&file_src, theme.on_surface), None, "File 默认保留原色");
        assert_eq!(Tint::None.resolve(&path_src, theme.on_surface), None);
        assert_eq!(Tint::Color(Color::RED).resolve(&file_src, theme.on_surface), Some(Color::RED));
    }

}
