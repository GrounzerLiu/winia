//! UI 值类型（对标 Jetpack Compose androidx.compose.ui.unit）
//!
//! 提供 Dp / Sp / Offset / Size 等类型，
//! 以及 Density 密度抽象用于 Dp/Sp ↔ px 转换。
//!
//! ## 设计
//! - `Dp` / `Sp` 是 f32 的强类型包装，避免混用 px/dp/sp
//! - `Offset` / `Size` 是 2D 向量类型，支持算术运算
//! - `Density` 提供 Dp/Sp ↔ px 转换（由窗口 scale_factor 提供）
//! - 均实现 `AnimatableValue`，可直接用于动画系统

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use std::sync::LazyLock;

// ═══════════════════════════════════════════════════════════
// Dp — 密度无关像素（1dp ≈ 1/160 inch）
// ═══════════════════════════════════════════════════════════

/// 密度无关像素
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Dp(pub f32);

impl Dp {
    pub fn value(&self) -> f32 { self.0 }

    /// 通过 Density 转换为物理像素
    pub fn to_px(&self, density: Density) -> f32 { self.0 * density.density }
}

// ── Dp 算术 ──

impl Add for Dp {
    type Output = Dp;
    fn add(self, rhs: Dp) -> Dp { Dp(self.0 + rhs.0) }
}
impl AddAssign for Dp {
    fn add_assign(&mut self, rhs: Dp) { self.0 += rhs.0; }
}
impl Sub for Dp {
    type Output = Dp;
    fn sub(self, rhs: Dp) -> Dp { Dp(self.0 - rhs.0) }
}
impl SubAssign for Dp {
    fn sub_assign(&mut self, rhs: Dp) { self.0 -= rhs.0; }
}
impl Mul<f32> for Dp {
    type Output = Dp;
    fn mul(self, rhs: f32) -> Dp { Dp(self.0 * rhs) }
}
impl Div<f32> for Dp {
    type Output = Dp;
    fn div(self, rhs: f32) -> Dp { Dp(self.0 / rhs) }
}
impl Neg for Dp {
    type Output = Dp;
    fn neg(self) -> Dp { Dp(-self.0) }
}
impl From<f32> for Dp {
    fn from(v: f32) -> Self { Dp(v) }
}
impl From<Dp> for f32 {
    fn from(d: Dp) -> Self { d.0 }
}

// ═══════════════════════════════════════════════════════════
// Sp — 缩放像素（字体大小，随用户字体缩放设置变化）
// ═══════════════════════════════════════════════════════════

/// 缩放像素（Scaling pixel，字体专用）
///
/// ## ⚠️ 当前实现说明
/// 系统字体缩放（无障碍大字体）的获取暂未接入——本项目目前不查询系统 font_scale，
/// `Density.font_scale` 恒为 1.0，因此 **Sp 当前行为与 Dp 完全一致**。
/// 未来接入系统设置后，`to_px` 将随用户字体缩放变化（dp 不变，sp 变大）。
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Sp(pub f32);

impl Sp {
    pub fn value(&self) -> f32 { self.0 }

    /// 通过 Density 转换为物理像素（density + font_scale）
    pub fn to_px(&self, density: Density) -> f32 {
        self.0 * density.density * density.font_scale
    }
}

// ── Sp 算术 ──

impl Add for Sp {
    type Output = Sp;
    fn add(self, rhs: Sp) -> Sp { Sp(self.0 + rhs.0) }
}
impl Sub for Sp {
    type Output = Sp;
    fn sub(self, rhs: Sp) -> Sp { Sp(self.0 - rhs.0) }
}
impl Mul<f32> for Sp {
    type Output = Sp;
    fn mul(self, rhs: f32) -> Sp { Sp(self.0 * rhs) }
}
impl Div<f32> for Sp {
    type Output = Sp;
    fn div(self, rhs: f32) -> Sp { Sp(self.0 / rhs) }
}
impl From<f32> for Sp {
    fn from(v: f32) -> Self { Sp(v) }
}
impl From<Sp> for f32 {
    fn from(s: Sp) -> Self { s.0 }
}

// ═══════════════════════════════════════════════════════════
// Density — 密度抽象（Dp/Sp ↔ px 转换）
// ═══════════════════════════════════════════════════════════

/// 屏幕密度 + 字体缩放
/// ⚠️ `font_scale` 当前恒为 1.0（未接入系统字体缩放），
/// 因此 Sp 行为等同 Dp。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Density {
    /// 逻辑像素与物理像素比（1.0 = mdpi）
    pub density: f32,
    /// 用户字体缩放（无障碍大字体 >1.0）
    pub font_scale: f32,
}

impl Density {
    pub const fn from_density(density: f32) -> Self {
        Self { density, font_scale: 1.0 }
    }
    pub const fn standard() -> Self {
        Self { density: 1.0, font_scale: 1.0 }
    }

    pub fn to_px(&self, dp: Dp) -> f32 { dp.0 * self.density }
}

// ═══════════════════════════════════════════════════════════
// Offset — 2D 偏移（对标 Compose Offset）
// ═══════════════════════════════════════════════════════════

/// 2D 偏移（f32 精度）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub fn new(x: f32, y: f32) -> Self { Offset { x, y } }
}

impl Add for Offset {
    type Output = Offset;
    fn add(self, rhs: Offset) -> Offset { Offset::new(self.x + rhs.x, self.y + rhs.y) }
}
impl Sub for Offset {
    type Output = Offset;
    fn sub(self, rhs: Offset) -> Offset { Offset::new(self.x - rhs.x, self.y - rhs.y) }
}
impl Neg for Offset {
    type Output = Offset;
    fn neg(self) -> Offset { Offset::new(-self.x, -self.y) }
}

// ═══════════════════════════════════════════════════════════
// Size — 2D 尺寸（对标 Compose Size）
// ═══════════════════════════════════════════════════════════

/// 2D 尺寸
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };

    pub fn new(width: f32, height: f32) -> Self { Size { width, height } }

    /// 包含判断
    pub fn contains(&self, offset: Offset) -> bool {
        offset.x >= 0.0 && offset.x < self.width && offset.y >= 0.0 && offset.y < self.height
    }
}

impl Add for Size {
    type Output = Size;
    fn add(self, rhs: Size) -> Size { Size::new(self.width + rhs.width, self.height + rhs.height) }
}
impl Sub for Size {
    type Output = Size;
    fn sub(self, rhs: Size) -> Size { Size::new(self.width - rhs.width, self.height - rhs.height) }
}

// ═══════════════════════════════════════════════════════════
// AnimatableValue 实现（对接动画系统）
// ═══════════════════════════════════════════════════════════

impl crate::animation::AnimatableValue for Dp {
    fn lerp(&self, to: &Dp, t: f32) -> Dp { Dp(self.0 + (to.0 - self.0) * t) }
    fn to_f32(&self) -> f32 { self.0 }
    fn from_f32(v: f32) -> Dp { Dp(v) }
    fn supports_spring() -> bool { true }
}

impl crate::animation::AnimatableValue for i32 {
    fn lerp(&self, to: &i32, t: f32) -> i32 {
        let from = *self as f32;
        (from + (*to as f32 - from) * t).round() as i32
    }
    fn to_f32(&self) -> f32 { *self as f32 }
    fn from_f32(v: f32) -> i32 { v.round() as i32 }
    fn supports_spring() -> bool { true }
}

impl crate::animation::AnimatableValue for Sp {
    fn lerp(&self, to: &Sp, t: f32) -> Sp { Sp(self.0 + (to.0 - self.0) * t) }
    fn to_f32(&self) -> f32 { self.0 }
    fn from_f32(v: f32) -> Sp { Sp(v) }
    fn supports_spring() -> bool { true }
}

impl crate::animation::AnimatableValue for Offset {
    fn lerp(&self, to: &Offset, t: f32) -> Offset {
        Offset::new(self.x + (to.x - self.x) * t, self.y + (to.y - self.y) * t)
    }
    fn to_f32(&self) -> f32 { (self.x * self.x + self.y * self.y).sqrt() }
    fn from_f32(v: f32) -> Offset { Offset::new(v, v) }
}

impl crate::animation::AnimatableValue for Size {
    fn lerp(&self, to: &Size, t: f32) -> Size {
        Size::new(self.width + (to.width - self.width) * t, self.height + (to.height - self.height) * t)
    }
    fn to_f32(&self) -> f32 { (self.width * self.width + self.height * self.height).sqrt() }
    fn from_f32(v: f32) -> Size { Size::new(v, v) }
}

// ═══════════════════════════════════════════════════════════
// Dp/Sp 扩展 trait（对标 Compose：100.dp / 14.sp）
// ═══════════════════════════════════════════════════════════

/// 数字 → Dp 扩展（`100.dp()`，对标 Compose 的 `100.dp`）
pub trait DpExt {
    fn dp(self) -> Dp;
}

impl DpExt for f32 {
    fn dp(self) -> Dp { Dp(self) }
}
impl DpExt for i32 {
    fn dp(self) -> Dp { Dp(self as f32) }
}
impl DpExt for u32 {
    fn dp(self) -> Dp { Dp(self as f32) }
}

/// 数字 → Sp 扩展（`14.sp()`，对标 Compose 的 `14.sp`）
pub trait SpExt {
    fn sp(self) -> Sp;
}

impl SpExt for f32 {
    fn sp(self) -> Sp { Sp(self) }
}
impl SpExt for i32 {
    fn sp(self) -> Sp { Sp(self as f32) }
}
impl SpExt for u32 {
    fn sp(self) -> Sp { Sp(self as f32) }
}

// ═══════════════════════════════════════════════════════════
// Px — 物理像素
// ═══════════════════════════════════════════════════════════

/// 物理像素（需 Density 转换为逻辑像素用于布局）
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Px(pub f32);

impl Px {
    /// 通过 Density 转换为逻辑像素
    pub fn to_logical(&self, density: Density) -> f32 {
        self.0 / density.density.max(f32::EPSILON)
    }
}

/// 数字 → Px 扩展（`20.px()`）
pub trait PxExt {
    fn px(self) -> Px;
}

impl PxExt for f32 {
    fn px(self) -> Px { Px(self) }
}
impl PxExt for i32 {
    fn px(self) -> Px { Px(self as f32) }
}
impl PxExt for u32 {
    fn px(self) -> Px { Px(self as f32) }
}

// ═══════════════════════════════════════════════════════════
// TextUnit — 文本尺寸单位（Sp/Px 统一，对标 Compose TextUnit）
// ═══════════════════════════════════════════════════════════

/// 文本尺寸单位：Sp（字体缩放像素）或 Px（物理像素）
///
/// `f32` 默认按 Sp 处理（font_scale=1 时 Sp == 逻辑像素，向后兼容旧 API）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextUnit {
    /// 缩放像素（当前 font_scale=1，行为等同逻辑像素）
    Sp(Sp),
    /// 物理像素（需 Density 转逻辑像素）
    Px(Px),
}

impl TextUnit {
    /// 解析为逻辑像素（Sp 直接取值；Px 需 Density 转换）
    pub fn to_logical_px(&self) -> f32 {
        match self {
            TextUnit::Sp(s) => s.value(),
            TextUnit::Px(p) => p.to_logical(current_density()),
        }
    }
}

impl From<f32> for TextUnit {
    fn from(v: f32) -> Self { TextUnit::Sp(Sp(v)) }
}
impl From<Sp> for TextUnit {
    fn from(s: Sp) -> Self { TextUnit::Sp(s) }
}
impl From<Px> for TextUnit {
    fn from(p: Px) -> Self { TextUnit::Px(p) }
}

// ═══════════════════════════════════════════════════════════
// LOCAL_DENSITY — CompositionLocal（对标 Compose LocalDensity）
// ═══════════════════════════════════════════════════════════

use crate::core::composition_local::CompositionLocal;

static LOCAL_DENSITY: LazyLock<CompositionLocal<Density>> = LazyLock::new(|| {
    CompositionLocal::new(|| Density::standard())
});

/// 读取当前子树 Density（默认 standard=1.0）
pub fn current_density() -> Density {
    LOCAL_DENSITY.current()
}

/// 在子树中提供 Density
pub fn with_density<R>(density: Density, content: impl FnOnce() -> R) -> R {
    LOCAL_DENSITY.provides(density, content)
}

// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_px_conversion() {
        let d = Density::from_density(2.0);
        let dp = Dp(10.0);
        assert_eq!(dp.to_px(d), 20.0);
    }

    #[test]
    fn sp_px_conversion_with_font_scale() {
        let d = Density::from_density(2.0);
        let sp = Sp(12.0);
        assert_eq!(sp.to_px(d), 24.0);
    }

    #[test]
    fn dp_arithmetic() {
        let a = Dp(5.0);
        let b = Dp(3.0);
        assert_eq!(a + b, Dp(8.0));
        assert_eq!(a - b, Dp(2.0));
        assert_eq!(a * 2.0, Dp(10.0));
        assert_eq!(a / 2.0, Dp(2.5));
        assert_eq!(-a, Dp(-5.0));
    }

    #[test]
    fn offset_size_ops() {
        let o = Offset::new(1.0, 2.0);
        let o2 = Offset::new(3.0, 4.0);
        assert_eq!(o + o2, Offset::new(4.0, 6.0));
        assert_eq!(o2 - o, Offset::new(2.0, 2.0));

        let s = Size::new(100.0, 50.0);
        assert!(s.contains(Offset::new(50.0, 25.0)));
        assert!(!s.contains(Offset::new(101.0, 25.0)));
    }

    #[test]
    fn animatable_value_impls() {
        use crate::animation::AnimatableValue;
        // Dp 动画
        let a = Dp(0.0);
        let b = Dp(100.0);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid, Dp(50.0));
        // Offset 动画
        let o = Offset::new(0.0, 0.0).lerp(&Offset::new(10.0, 20.0), 0.5);
        assert_eq!(o, Offset::new(5.0, 10.0));
        // Size 动画
        let s = Size::new(0.0, 0.0).lerp(&Size::new(8.0, 6.0), 0.25);
        assert_eq!(s, Size::new(2.0, 1.5));
    }

    #[test]
    fn dp_sp_extensions() {
        // 对标 Compose 100.dp / 14.sp
        let w: Dp = 100.dp();
        assert_eq!(w, Dp(100.0));
        let f: Dp = 2.5f32.dp();
        assert_eq!(f, Dp(2.5));
        let u: Dp = 30u32.dp();
        assert_eq!(u, Dp(30.0));

        let s: Sp = 14.sp();
        assert_eq!(s, Sp(14.0));
        let sf: Sp = 1.5f32.sp();
        assert_eq!(sf, Sp(1.5));
    }

    #[test]
    fn px_extension_and_dimension_conversion() {
        // 20.px() → Px
        let px: Px = 20.px();
        assert_eq!(px, Px(20.0));
        let pxf: Px = 3.5f32.px();
        assert_eq!(pxf, Px(3.5));

        // Dp/Px → Dimension（供 .size() 消费）
        use crate::modifier::Dimension;
        let d: Dimension = 10.dp().into();
        assert_eq!(d, Dimension::Dp(Dp(10.0)));
        let p: Dimension = 20.px().into();
        assert_eq!(p, Dimension::Px(Px(20.0)));

        // Dp → 逻辑像素 = dp 值；Px → 逻辑像素 = px/density
        with_density(Density::from_density(2.0), || {
            assert_eq!(Dimension::Dp(Dp(10.0)).to_logical_px(), 10.0);
            assert_eq!(Dimension::Px(Px(20.0)).to_logical_px(), 10.0); // 20px / 2.0 = 10 逻辑
            assert_eq!(Dimension::Fixed(8.0).to_logical_px(), 8.0);
        });
    }
}
