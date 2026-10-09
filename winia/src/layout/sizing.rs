//! The sizing vocabulary the layout and the modifier chain share: a static `Dimension`,
//! the dynamic `SizeValue` that evaluates per frame, and `IntrinsicSize`.

use std::sync::Arc;

/// 尺寸值，用于 Modifier 和 Layout
///
/// 支持多种单位：`Fixed(f32)`（逻辑像素）、`Dp`（密度无关，== 逻辑像素）、
/// `Px`（物理像素，需 Density 转换）、`Fill`、`Auto`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    /// 固定逻辑像素值
    Fixed(f32),
    /// 密度无关像素（本项目 1dp == 1 逻辑像素，无需转换）
    Dp(crate::unit::Dp),
    /// 物理像素（需 Density 转逻辑像素）
    Px(crate::unit::Px),
    /// 填满可用空间
    Fill,
    /// 自适应内容大小
    Auto,
}

/// Which of the content's intrinsic measurements a size modifier asks for — Compose's
/// `androidx.compose.foundation.layout.IntrinsicSize`.
///
/// An intrinsic measurement is what the content would be with NO incoming space to fill: `Min` is
/// the smallest it can be laid out at (for text, the longest unbreakable run), `Max` is the size it
/// takes with nothing wrapped. `Modifier::width(IntrinsicSize::Max)` therefore sizes a node to its
/// own content instead of to its parent, which is how Compose makes a menu exactly as wide as its
/// widest item (`material3/Menu.kt` uses `Column(width(IntrinsicSize.Max))`).
///
/// The incoming constraints still win afterwards: Compose documents the modifier as "the incoming
/// measurement constraints may override this value", and `requiredWidth/requiredHeight` are the
/// variant that ignores them (`enforceIncoming = false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSize {
    /// The smallest size the content can be laid out at (Compose `IntrinsicSize.Min`).
    Min,
    /// The size the content takes when nothing is wrapped or compressed (Compose `IntrinsicSize.Max`).
    Max,
}

/// 尺寸值：静态 `Dimension` 或动态求值（布局属性动画用）。
///
/// `size()` 统一入口——传 `f32`/`Dimension`（静态）或 `State<f32>`/闭包（动态）：
/// - `.size(50.0, 24.0)` 静态
/// - `.size(&scale, 24.0)` 动画（State 直接传，测量时 `get()` 注册依赖到本节点）
/// - `.size(|| scale.get() * 2.0, 24.0)` 复杂表达式（闭包）
pub enum SizeValue {
    Static(Dimension),
    Dynamic(Arc<dyn Fn() -> f32 + Send + Sync>),
    /// Size this axis to one of the content's own intrinsic measurements instead of to the
    /// incoming space — Compose's `Modifier.width/height(IntrinsicSize)`.
    Intrinsic(IntrinsicSize),
}

impl From<Dimension> for SizeValue {
    fn from(d: Dimension) -> Self { SizeValue::Static(d) }
}

impl From<f32> for SizeValue {
    fn from(v: f32) -> Self { SizeValue::Static(Dimension::Fixed(v)) }
}

impl From<crate::unit::Dp> for SizeValue {
    fn from(v: crate::unit::Dp) -> Self { SizeValue::Static(Dimension::Dp(v)) }
}

impl From<crate::unit::Px> for SizeValue {
    fn from(v: crate::unit::Px) -> Self { SizeValue::Static(Dimension::Px(v)) }
}

impl From<crate::runtime::state::State<f32>> for SizeValue {
    fn from(s: crate::runtime::state::State<f32>) -> Self {
        SizeValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<crate::runtime::state::Animating<f32>> for SizeValue {
    fn from(s: crate::runtime::state::Animating<f32>) -> Self {
        SizeValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<crate::runtime::state::DerivedValue<f32>> for SizeValue {
    fn from(d: crate::runtime::state::DerivedValue<f32>) -> Self {
        SizeValue::Dynamic(Arc::new(move || d.get()))
    }
}

impl Clone for SizeValue {
    fn clone(&self) -> Self {
        match self {
            SizeValue::Static(d) => SizeValue::Static(*d),
            SizeValue::Dynamic(f) => SizeValue::Dynamic(f.clone()),
            SizeValue::Intrinsic(s) => SizeValue::Intrinsic(*s),
        }
    }
}

impl std::fmt::Debug for SizeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SizeValue::Static(d) => write!(f, "{:?}", d),
            SizeValue::Dynamic(_) => write!(f, "<dynamic>"),
            SizeValue::Intrinsic(s) => write!(f, "intrinsic({:?})", s),
        }
    }
}

impl From<IntrinsicSize> for SizeValue {
    fn from(s: IntrinsicSize) -> Self { SizeValue::Intrinsic(s) }
}

impl Dimension {
    pub fn is_fixed(&self) -> bool {
        matches!(self, Dimension::Fixed(_) | Dimension::Dp(_) | Dimension::Px(_))
    }

    pub fn is_fill(&self) -> bool {
        matches!(self, Dimension::Fill)
    }

    /// 解析为逻辑像素（Px 需要 Density，Dp/Fixed 直接是逻辑像素）
    pub fn to_logical_px(&self) -> f32 {
        match self {
            Dimension::Fixed(v) => *v,
            Dimension::Dp(d) => d.value(),
            Dimension::Px(p) => p.to_logical(crate::runtime::density::current_density()),
            Dimension::Fill | Dimension::Auto => 0.0,
        }
    }
}

impl From<f32> for Dimension {
    fn from(v: f32) -> Self {
        Dimension::Fixed(v)
    }
}

impl From<crate::unit::Dp> for Dimension {
    fn from(d: crate::unit::Dp) -> Self {
        Dimension::Dp(d)
    }
}

impl From<crate::unit::Px> for Dimension {
    fn from(p: crate::unit::Px) -> Self {
        Dimension::Px(p)
    }
}

impl From<&crate::runtime::state::State<f32>> for SizeValue {
    fn from(s: &crate::runtime::state::State<f32>) -> Self {
        let s = s.clone();
        SizeValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<&crate::runtime::state::Animating<f32>> for SizeValue {
    fn from(s: &crate::runtime::state::Animating<f32>) -> Self {
        let s = s.clone();
        SizeValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<&crate::runtime::state::DerivedValue<f32>> for SizeValue {
    fn from(d: &crate::runtime::state::DerivedValue<f32>) -> Self {
        let d = d.clone();
        SizeValue::Dynamic(Arc::new(move || d.get()))
    }
}

impl From<&crate::runtime::state::State<crate::unit::Dp>> for SizeValue {
    fn from(s: &crate::runtime::state::State<crate::unit::Dp>) -> Self {
        let s = s.clone();
        SizeValue::Dynamic(Arc::new(move || s.get().value()))
    }
}

impl From<&crate::runtime::state::Animating<crate::unit::Dp>> for SizeValue {
    fn from(s: &crate::runtime::state::Animating<crate::unit::Dp>) -> Self {
        let s = s.clone();
        SizeValue::Dynamic(Arc::new(move || s.get().value()))
    }
}

impl<F: Fn() -> f32 + Send + Sync + 'static> From<F> for SizeValue {
    fn from(f: F) -> Self { SizeValue::Dynamic(Arc::new(f)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::density::with_density;

    /// Dp → 逻辑像素 = dp 值；Px → 逻辑像素 = px/density. The `Px` arm reads the ambient density,
    /// so this is the module that supplies it — the assertion used to sit in `unit.rs`, which must
    /// not name the runtime layer for it.
    #[test]
    fn dimension_to_logical_px_resolves_against_the_ambient_density() {
        with_density(crate::unit::Density::from_density(2.0), || {
            assert_eq!(Dimension::Dp(crate::unit::Dp(10.0)).to_logical_px(), 10.0);
            assert_eq!(Dimension::Px(crate::unit::Px(20.0)).to_logical_px(), 10.0); // 20px / 2.0
            assert_eq!(Dimension::Fixed(8.0).to_logical_px(), 8.0);
        });
    }
}
