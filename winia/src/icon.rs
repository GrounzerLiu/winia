//! Material Symbols 可变字体图标（仅在启用对应 cargo feature 时编译）。
//!
//! 默认不启用任何特性——Icon 默认不内置任何图标。启用后：
//! - 内嵌对应主题的 Material Symbols 可变字体（Outlined / Rounded / Sharp）；
//! - 提供 4200+ 码点常量（如 `Outlined::ADD`、`Rounded::ARROW_BACK`）；
//! - 支持可变轴 FILL / GRAD / opsz / wght（范围见 `docs/icon.md`）。
//!
//! 字体与码点表来源：<https://github.com/google/material-design-icons>
//! （Apache License 2.0，见本目录 NOTICE）。

use skia_safe::font_arguments::variation_position::Coordinate;
use skia_safe::font_arguments::VariationPosition;
use skia_safe::{Font, FontArguments, FontMgr, FourByteTag, TextBlob, Typeface};
use std::sync::OnceLock;

#[cfg(feature = "material-symbols-outlined")]
mod outlined;
#[cfg(feature = "material-symbols-outlined")]
pub use outlined::*;

#[cfg(feature = "material-symbols-rounded")]
mod rounded;
#[cfg(feature = "material-symbols-rounded")]
pub use rounded::*;

#[cfg(feature = "material-symbols-sharp")]
mod sharp;
#[cfg(feature = "material-symbols-sharp")]
pub use sharp::*;

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialSymbol {
    #[cfg(feature = "material-symbols-outlined")]
    Outlined(&'static str),
    #[cfg(feature = "material-symbols-rounded")]
    Rounded(&'static str),
    #[cfg(feature = "material-symbols-sharp")]
    Sharp(&'static str),
}

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
impl MaterialSymbol {
    pub(crate) fn codepoint(self) -> &'static str {
        match self {
            #[cfg(feature = "material-symbols-outlined")]
            MaterialSymbol::Outlined(cp) => cp,
            #[cfg(feature = "material-symbols-rounded")]
            MaterialSymbol::Rounded(cp) => cp,
            #[cfg(feature = "material-symbols-sharp")]
            MaterialSymbol::Sharp(cp) => cp,
        }
    }
}

#[cfg(feature = "material-symbols-outlined")]
static FONT_OUTLINED: &[u8] = include_bytes!("icon/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf");
#[cfg(feature = "material-symbols-rounded")]
static FONT_ROUNDED: &[u8] = include_bytes!("icon/MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf");
#[cfg(feature = "material-symbols-sharp")]
static FONT_SHARP: &[u8] = include_bytes!("icon/MaterialSymbolsSharp[FILL,GRAD,opsz,wght].ttf");

#[cfg(feature = "material-symbols-outlined")]
fn typeface_outlined() -> &'static Typeface {
    static T: OnceLock<Typeface> = OnceLock::new();
    T.get_or_init(|| {
        FontMgr::default()
            .new_from_data(FONT_OUTLINED, None)
            .expect("Material Symbols Outlined 字体加载失败")
    })
}

#[cfg(feature = "material-symbols-rounded")]
fn typeface_rounded() -> &'static Typeface {
    static T: OnceLock<Typeface> = OnceLock::new();
    T.get_or_init(|| {
        FontMgr::default()
            .new_from_data(FONT_ROUNDED, None)
            .expect("Material Symbols Rounded 字体加载失败")
    })
}

#[cfg(feature = "material-symbols-sharp")]
fn typeface_sharp() -> &'static Typeface {
    static T: OnceLock<Typeface> = OnceLock::new();
    T.get_or_init(|| {
        FontMgr::default()
            .new_from_data(FONT_SHARP, None)
            .expect("Material Symbols Sharp 字体加载失败")
    })
}

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
fn typeface_for(symbol: MaterialSymbol) -> &'static Typeface {
    match symbol {
        #[cfg(feature = "material-symbols-outlined")]
        MaterialSymbol::Outlined(_) => typeface_outlined(),
        #[cfg(feature = "material-symbols-rounded")]
        MaterialSymbol::Rounded(_) => typeface_rounded(),
        #[cfg(feature = "material-symbols-sharp")]
        MaterialSymbol::Sharp(_) => typeface_sharp(),
    }
}

/// 按可变轴参数生成字形 TextBlob。
///
/// 轴范围（官方）：FILL 0..=1、GRAD -50..=200、opsz 20..=48、wght 100..=700。
/// 调用方负责先钳制；这里只透传。
#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
pub(crate) fn symbol_text_blob(
    symbol: MaterialSymbol,
    size: f32,
    fill: f32,
    grad: f32,
    opsz: f32,
    wght: f32,
) -> Option<TextBlob> {
    // 绘制路径不做 panic：轴参数/缺字形失败时返回 None，渲染层跳过绘制
    let typeface = typeface_for(symbol).clone_with_arguments(
        &FontArguments::new().set_variation_design_position(VariationPosition {
            coordinates: &[
                Coordinate { axis: FourByteTag::from_chars('F', 'I', 'L', 'L'), value: fill },
                Coordinate { axis: FourByteTag::from_chars('G', 'R', 'A', 'D'), value: grad },
                Coordinate { axis: FourByteTag::from_chars('o', 'p', 's', 'z'), value: opsz },
                Coordinate { axis: FourByteTag::from_chars('w', 'g', 'h', 't'), value: wght },
            ],
        }),
    )?;
    let font = Font::new(&typeface, size);
    TextBlob::new(symbol.codepoint(), &font)
}
