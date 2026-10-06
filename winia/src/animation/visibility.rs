//! 进出场过渡配置——`AnimatedVisibility` 与共享元素过渡两端共用的那份"动作规格"。
//!
//! 它是动作规格、不是组件：Modifier 链携带它，`nav.rs` 按过渡存它，`shared_transition` 用它
//! 配对两端。它原先住在 `components/animated_visibility.rs`，那等于用某个组件的名字去描述
//! "东西怎么淡出"——放回 `animation` 层才对。

use crate::animation::AnimationSpec;

/// Slide 方向（`VisibilityTransition::slide_in/slide_out`）。
/// 与 [`SlideOffset`] 配合：方向决定轴与正负，offset 决定距离。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Slide 距离（对标 Compose `slideInHorizontally(initialOffsetX: (fullWidth) -> Int)`）。
///
/// Compose 收一个"按内容尺寸算"的 lambda；常见情形只有固定值或比例两种，这两个变体不用闭包
/// 就能表达。两处用 slide 的地方共用这个类型：[`AnimatedVisibility`](crate::components::animated_visibility)
/// 与 [`crate::nav`] 的场景过渡——后者原先自己有一个几乎一样的枚举。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlideOffset {
    /// 固定逻辑像素——默认 48，M3 共享轴是 30。
    Fixed(f32),
    /// 沿 slide 轴的内容尺寸的**比例**（1.0 = 整段滑入，即 Compose 的
    /// `initialOffsetX = { fullWidth }`；-0.3 = 反向视差 `{ -it / 3 }`）。
    Fraction(f32),
}

impl SlideOffset {
    /// 该滑多远，`extent` 是**沿 slide 轴**的容器尺寸——调用方知道那是宽还是高。
    pub fn resolve(&self, extent: f32) -> f32 {
        match self {
            SlideOffset::Fixed(px) => *px,
            SlideOffset::Fraction(f) => f * extent,
        }
    }
}

impl Default for SlideOffset {
    fn default() -> Self {
        Self::Fixed(48.0)
    }
}

/// 纵向展开的锚点（对标 Compose `expandVertically(expandFrom: Alignment.Top)`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandFrom {
    /// 内容从顶部往下长（沿用现状）。
    #[default]
    Top,
    /// 内容从底部往上长。
    Bottom,
}

/// 横向展开的锚点（对标 Compose `expandHorizontally(expandFrom: Alignment.Start)`）。
/// 注意：Start 一律是左边、End 一律是右边（没有 RTL 镜像——对齐 Compose 的欠账，
/// RTL 调用方自己显式选锚点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandFromH {
    /// 内容从起点（左）往右长。
    #[default]
    Start,
    /// 内容从终点（右）往左长。
    End,
}

/// 进出场过渡配置：fade / slide / expand / scale 效果 + 动画规格。
/// 用 `with_*` 组合（对标 Compose 的 `fadeIn() + expandVertically()`）。
#[derive(Debug, Clone)]
pub struct VisibilityTransition {
    /// 淡入/淡出（alpha 0<->1）
    pub fade: bool,
    /// 滑入/滑出（方向 + 距离）
    pub slide: Option<(SlideDirection, SlideOffset)>,
    /// 纵向展开/收起（容器高 0<->全高，布局层——后面的内容跟着走）
    pub expand: bool,
    /// 纵向展开锚点
    pub expand_from: ExpandFrom,
    /// 横向展开/收起（容器宽 0<->全宽，布局层）
    pub expand_h: bool,
    /// 横向展开锚点
    pub expand_from_h: ExpandFromH,
    /// 缩放（scale_from <-> 1.0，绕 transform_origin）
    pub scale: bool,
    /// 缩放的起始值（现状 0.8——对标 Compose `scaleIn(initialScale)`）
    pub scale_from: f32,
    /// 缩放轴心，归一化（0.5, 0.5）= 中心（对标 Compose `transformOrigin`）
    pub transform_origin: (f32, f32),
    /// 动画规格
    pub spec: AnimationSpec,
}

impl VisibilityTransition {
    /// 空过渡（所有通道关闭——只有 slide/scale 的自定义过渡、或共享元素两端
    /// 完全由飞行层负责的场合）。
    pub fn empty() -> Self {
        Self {
            fade: false,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Tween(Default::default()),
        }
    }

    fn base(spec: AnimationSpec) -> Self {
        Self {
            fade: false,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec,
        }
    }
    pub fn fade_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: true, ..Self::base(spec.into()) }
    }
    pub fn fade_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::fade_in(spec)
    }
    pub fn expand_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { expand: true, ..Self::base(spec.into()) }
    }
    pub fn shrink_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in(spec)
    }
    /// 横向展开（对标 Compose `expandHorizontally`）。
    pub fn expand_in_h(spec: impl Into<AnimationSpec>) -> Self {
        Self { expand_h: true, ..Self::base(spec.into()) }
    }
    /// 横向收起（对标 Compose `shrinkHorizontally`）。
    pub fn shrink_out_h(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in_h(spec)
    }
    pub fn slide_in(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self { slide: Some((dir, SlideOffset::default())), ..Self::base(spec.into()) }
    }
    pub fn slide_out(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self::slide_in(dir, spec)
    }
    /// 指定距离的 slide（对标 Compose 的 `initialOffsetX/Y` lambda）。
    pub fn slide_in_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        Self { slide: Some((dir, offset)), ..Self::base(spec.into()) }
    }
    pub fn slide_out_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        Self::slide_in_offset(dir, offset, spec)
    }
    pub fn scale_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { scale: true, ..Self::base(spec.into()) }
    }
    pub fn scale_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::scale_in(spec)
    }
    /// 叠加：再打开 fade
    pub fn with_fade(mut self) -> Self {
        self.fade = true;
        self
    }
    /// 叠加：再打开纵向展开/收起
    pub fn with_expand(mut self) -> Self {
        self.expand = true;
        self
    }
    /// 叠加：纵向展开/收起 + 显式锚点
    pub fn with_expand_from(mut self, from: ExpandFrom) -> Self {
        self.expand = true;
        self.expand_from = from;
        self
    }
    /// 叠加：再打开横向展开/收起
    pub fn with_expand_h(mut self) -> Self {
        self.expand_h = true;
        self
    }
    /// 叠加：横向展开/收起 + 显式锚点
    pub fn with_expand_h_from(mut self, from: ExpandFromH) -> Self {
        self.expand_h = true;
        self.expand_from_h = from;
        self
    }
    /// 叠加：slide（默认 48px 距离）
    pub fn with_slide(mut self, dir: SlideDirection) -> Self {
        self.slide = Some((dir, SlideOffset::default()));
        self
    }
    /// 叠加：slide + 显式距离
    pub fn with_slide_offset(mut self, dir: SlideDirection, offset: SlideOffset) -> Self {
        self.slide = Some((dir, offset));
        self
    }
    /// 叠加：scale（默认 0.8、以中心为原点）
    pub fn with_scale(mut self) -> Self {
        self.scale = true;
        self
    }
    /// 叠加：scale + 显式起始值与轴心
    /// （对标 Compose `scaleIn(initialScale, transformOrigin)`）
    pub fn with_scale_from(mut self, scale_from: f32, transform_origin: (f32, f32)) -> Self {
        self.scale = true;
        self.scale_from = scale_from;
        self.transform_origin = transform_origin;
        self
    }
    /// 本过渡在 `progress` 处产生的绘制层参数，`content_extent` 是解析 slide 距离用的盒子。
    ///
    /// `progress` 是"落定程度"：`1.0` 是静止态（过渡的各个开关都不贡献任何东西），`0.0` 是
    /// 另一端——还没开始的入场，或已经结束的退场。于是入场的内容跑 0 → 1、离场的内容跑
    /// 1 → 0，两者走的是同一个函数；这正是让 crossfade 的两半能用"同一份代码 + 两个不同
    /// 过渡"实现的原因。
    ///
    /// `content_extent` 只有 `SlideOffset::Fraction` 的 slide 需要，按它解析距离。
    pub fn layer_params(&self, progress: f32, content_extent: (f32, f32)) -> crate::graphics::GraphicsLayerParams {
        let p = progress;
        let mut params = crate::graphics::GraphicsLayerParams::default();
        if self.fade {
            params.alpha = p;
        }
        if self.scale {
            let s = self.scale_from + (1.0 - self.scale_from) * p;
            params.scale_x = s;
            params.scale_y = s;
            params.transform_origin =
                crate::graphics::TransformOrigin(self.transform_origin.0, self.transform_origin.1);
        }
        if let Some((dir, offset)) = self.slide {
            // 沿 slide 轴的长度；剩下的交给 `SlideOffset::resolve`。
            let (w, h) = content_extent;
            let extent = match dir {
                SlideDirection::Left | SlideDirection::Right => w,
                SlideDirection::Up | SlideDirection::Down => h,
            };
            let dist = offset.resolve(extent);
            let off = (1.0 - p) * dist;
            match dir {
                SlideDirection::Left => params.translation_x = -off,
                SlideDirection::Right => params.translation_x = off,
                SlideDirection::Up => params.translation_y = -off,
                SlideDirection::Down => params.translation_y = off,
            }
        }
        params
    }
}

impl Default for VisibilityTransition {
    fn default() -> Self {
        Self {
            fade: true,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Tween(Default::default()),
        }
    }
}

