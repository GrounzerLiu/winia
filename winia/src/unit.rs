//! UI 值类型（对标 Jetpack Compose androidx.compose.ui.unit）
//!
//! 提供 Dp / Sp / Offset / Size / IntOffset / IntSize 等类型，
//! 以及 Density 密度抽象用于 Dp/Sp ↔ px 转换。
//!
//! ## 设计
//! - `Dp` / `Sp` 是 f32 的强类型包装，避免混用 px/dp/sp
//! - `Offset` / `Size` 是 2D 向量类型，支持算术运算
//! - `Density` 提供 Dp/Sp ↔ px 转换（由窗口 scale_factor 提供）
//! - 均实现 `AnimatableValue`，可直接用于动画系统

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

// ═══════════════════════════════════════════════════════════
// Dp — 密度无关像素（1dp ≈ 1/160 inch）
// ═══════════════════════════════════════════════════════════

/// 密度无关像素
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Dp(pub f32);

impl Dp {
    pub const ZERO: Dp = Dp(0.0);
    pub const UNSPECIFIED: Dp = Dp(f32::NAN);

    pub fn new(value: f32) -> Self { Dp(value) }
    pub fn value(&self) -> f32 { self.0 }

    /// 是否未指定
    pub fn is_specified(&self) -> bool { !self.0.is_nan() }

    /// 通过 Density 转换为物理像素
    pub fn to_px(&self, density: Density) -> f32 { self.0 * density.density }

    /// 通过 Density 从物理像素构造
    pub fn from_px(px: f32, density: Density) -> Self {
        Dp(px / density.density.max(f32::EPSILON))
    }

    /// 取整到最近的整数 dp
    pub fn round(self) -> Dp { Dp(self.0.round()) }
    pub fn floor(self) -> Dp { Dp(self.0.floor()) }
    pub fn ceil(self) -> Dp { Dp(self.0.ceil()) }
    pub fn abs(self) -> Dp { Dp(self.0.abs()) }
    pub fn min(self, other: Dp) -> Dp { Dp(self.0.min(other.0)) }
    pub fn max(self, other: Dp) -> Dp { Dp(self.0.max(other.0)) }
    pub fn coerce_in(self, min: Dp, max: Dp) -> Dp { Dp(self.0.clamp(min.0, max.0)) }
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
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Sp(pub f32);

impl Sp {
    pub const ZERO: Sp = Sp(0.0);

    pub fn new(value: f32) -> Self { Sp(value) }
    pub fn value(&self) -> f32 { self.0 }

    /// 通过 Density 转换为物理像素（density + font_scale）
    pub fn to_px(&self, density: Density) -> f32 {
        self.0 * density.density * density.font_scale
    }

    /// 通过 Density 从物理像素构造
    pub fn from_px(px: f32, density: Density) -> Self {
        Sp(px / (density.density * density.font_scale).max(f32::EPSILON))
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Density {
    /// 逻辑像素与物理像素比（1.0 = mdpi）
    pub density: f32,
    /// 用户字体缩放（无障碍大字体 >1.0）
    pub font_scale: f32,
}

impl Density {
    pub const fn new(density: f32, font_scale: f32) -> Self {
        Self { density, font_scale }
    }
    pub const fn from_density(density: f32) -> Self {
        Self { density, font_scale: 1.0 }
    }
    pub const fn standard() -> Self {
        Self { density: 1.0, font_scale: 1.0 }
    }

    pub fn to_px(&self, dp: Dp) -> f32 { dp.0 * self.density }
    pub fn to_dp(&self, px: f32) -> Dp { Dp(px / self.density.max(f32::EPSILON)) }
    pub fn to_sp_px(&self, sp: Sp) -> f32 { sp.0 * self.density * self.font_scale }
    pub fn to_sp(&self, px: f32) -> Sp {
        Sp(px / (self.density * self.font_scale).max(f32::EPSILON))
    }
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
    pub const ZERO: Offset = Offset { x: 0.0, y: 0.0 };
    pub const UNSPECIFIED: Offset = Offset { x: f32::NAN, y: f32::NAN };

    pub fn new(x: f32, y: f32) -> Self { Offset { x, y } }
    pub fn is_specified(&self) -> bool { !self.x.is_nan() && !self.y.is_nan() }

    /// 与另一个 Offset 相加（无重载，语义化命名）
    pub fn plus(&self, other: Offset) -> Offset { Offset::new(self.x + other.x, self.y + other.y) }
    pub fn minus(&self, other: Offset) -> Offset { Offset::new(self.x - other.x, self.y - other.y) }
    pub fn times(&self, scale: f32) -> Offset { Offset::new(self.x * scale, self.y * scale) }

    pub fn distance(&self, other: Offset) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
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
    pub const UNSPECIFIED: Size = Size { width: f32::NAN, height: f32::NAN };

    pub fn new(width: f32, height: f32) -> Self { Size { width, height } }
    pub fn is_specified(&self) -> bool { !self.width.is_nan() && !self.height.is_nan() }

    pub fn width(&self) -> f32 { self.width }
    pub fn height(&self) -> f32 { self.height }
    pub fn area(&self) -> f32 { self.width * self.height }
    pub fn min_dimension(&self) -> f32 { self.width.min(self.height) }
    pub fn max_dimension(&self) -> f32 { self.width.max(self.height) }

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
// IntOffset / IntSize — 整数版本（像素级，滚动偏移等）
// ═══════════════════════════════════════════════════════════

/// 整数 2D 偏移
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct IntOffset {
    pub x: i32,
    pub y: i32,
}

impl IntOffset {
    pub const ZERO: IntOffset = IntOffset { x: 0, y: 0 };

    pub fn new(x: i32, y: i32) -> Self { IntOffset { x, y } }
    pub fn to_offset(&self) -> Offset { Offset::new(self.x as f32, self.y as f32) }
}

/// 整数 2D 尺寸
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct IntSize {
    pub width: i32,
    pub height: i32,
}

impl IntSize {
    pub const ZERO: IntSize = IntSize { width: 0, height: 0 };

    pub fn new(width: i32, height: i32) -> Self { IntSize { width, height } }
    pub fn to_size(&self) -> Size { Size::new(self.width as f32, self.height as f32) }
}

// ═══════════════════════════════════════════════════════════
// AnimatableValue 实现（对接动画系统）
// ═══════════════════════════════════════════════════════════

impl crate::animation::AnimatableValue for Dp {
    fn lerp(&self, to: &Dp, t: f32) -> Dp { Dp(self.0 + (to.0 - self.0) * t) }
    fn to_f32(&self) -> f32 { self.0 }
    fn from_f32(v: f32) -> Dp { Dp(v) }
}

impl crate::animation::AnimatableValue for Sp {
    fn lerp(&self, to: &Sp, t: f32) -> Sp { Sp(self.0 + (to.0 - self.0) * t) }
    fn to_f32(&self) -> f32 { self.0 }
    fn from_f32(v: f32) -> Sp { Sp(v) }
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
        assert_eq!(Dp::from_px(20.0, d), Dp(10.0));
    }

    #[test]
    fn sp_px_conversion_with_font_scale() {
        let d = Density::new(2.0, 1.5);
        let sp = Sp(12.0);
        assert_eq!(sp.to_px(d), 36.0);
        assert_eq!(Sp::from_px(36.0, d), Sp(12.0));
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
        assert_eq!(o.distance(o2), 2.0f32.sqrt() * 2.0);

        let s = Size::new(100.0, 50.0);
        assert!(s.contains(Offset::new(50.0, 25.0)));
        assert!(!s.contains(Offset::new(101.0, 25.0)));
        assert_eq!(s.area(), 5000.0);
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
}
