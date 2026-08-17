//! Modifier 系统 — 不可变链式修饰符 + 自定义扩展
//!
//! 类似 Jetpack Compose 的 Modifier，用于解耦外观/行为/布局。
//! - 链式 API: `Modifier::new().size(100, 100).padding(10).background(Color::RED)`
//! - 左到右 = 外到内
//! - 分为三类: LayoutModifier / DrawModifier / PointerInputModifier

use std::sync::Arc;
use std::ops::Range;
use std::fmt::{self, Debug};
use std::sync::atomic::{AtomicU64, Ordering};
use crate::layout::LayoutDirection;
use crate::ui::interaction::MutableInteractionSource;

// ── Dimension ──

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

/// 尺寸值：静态 `Dimension` 或动态求值（布局属性动画用）。
///
/// `size()` 统一入口——传 `f32`/`Dimension`（静态）或 `State<f32>`/闭包（动态）：
/// - `.size(50.0, 24.0)` 静态
/// - `.size(&scale, 24.0)` 动画（State 直接传，测量时 `get()` 注册依赖到本节点）
/// - `.size(|| scale.get() * 2.0, 24.0)` 复杂表达式（闭包）
pub enum SizeValue {
    Static(Dimension),
    Dynamic(Arc<dyn Fn() -> f32 + Send + Sync>),
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

impl From<crate::core::state::State<f32>> for SizeValue {
    fn from(s: crate::core::state::State<f32>) -> Self {
        SizeValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<&crate::core::state::State<f32>> for SizeValue {
    fn from(s: &crate::core::state::State<f32>) -> Self {
        let s = s.clone();
        SizeValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<crate::core::state::DerivedValue<f32>> for SizeValue {
    fn from(d: crate::core::state::DerivedValue<f32>) -> Self {
        SizeValue::Dynamic(Arc::new(move || d.get()))
    }
}

impl From<&crate::core::state::DerivedValue<f32>> for SizeValue {
    fn from(d: &crate::core::state::DerivedValue<f32>) -> Self {
        let d = d.clone();
        SizeValue::Dynamic(Arc::new(move || d.get()))
    }
}

impl From<&crate::core::state::State<crate::unit::Dp>> for SizeValue {
    fn from(s: &crate::core::state::State<crate::unit::Dp>) -> Self {
        let s = s.clone();
        SizeValue::Dynamic(Arc::new(move || s.get().value()))
    }
}

impl<F: Fn() -> f32 + Send + Sync + 'static> From<F> for SizeValue {
    fn from(f: F) -> Self { SizeValue::Dynamic(Arc::new(f)) }
}

impl Clone for SizeValue {
    fn clone(&self) -> Self {
        match self {
            SizeValue::Static(d) => SizeValue::Static(*d),
            SizeValue::Dynamic(f) => SizeValue::Dynamic(f.clone()),
        }
    }
}

impl std::fmt::Debug for SizeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SizeValue::Static(d) => write!(f, "{:?}", d),
            SizeValue::Dynamic(_) => write!(f, "<dynamic>"),
        }
    }
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
            Dimension::Px(p) => p.to_logical(crate::unit::current_density()),
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

// ── Shape ──

/// 形状描述（用于 background / border / clip）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// 矩形（可带圆角）
    RoundedRect { corner_radius: f32 },
    /// 胶囊（圆角 = 短边一半——对标 Compose `CornerFull`，material3
    /// Button 默认形状；宽高变化时自动跟随）
    Pill,
    /// 圆形
    Circle,
    /// 直角矩形
    Rectangle,
}

impl Shape {
    pub fn rounded(corner_radius: f32) -> Self {
        Shape::RoundedRect { corner_radius }
    }

    /// 胶囊形状（对标 Compose `RoundedCornerShape(50)`——短边一半圆角）
    pub fn pill() -> Self {
        Shape::Pill
    }
}

// ── Color (占位) ──

/// 颜色（占位，后续由 skia Color 或 material theme 替代）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0, a: 255 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255, a: 255 };

    pub fn from_argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a }
    }
}

impl Color {
    /// 状态层叠加（对标 Material3 state layer）：
    /// 把 `overlay` 以 `alpha` 透明度叠到当前颜色上——hover 8% / press/focus 12% /
    /// drag 16% 的近似实现（Material3 的容器状态层）。
    pub fn overlay(&self, overlay: Color, alpha: f32) -> Color {
        let a = alpha.clamp(0.0, 1.0);
        let lerp = |b: u8, o: u8| (b as f32 * (1.0 - a) + o as f32 * a).round() as u8;
        Color::from_argb(
            self.a,
            lerp(self.r, overlay.r),
            lerp(self.g, overlay.g),
            lerp(self.b, overlay.b),
        )
    }
}

// ── Modifier ──

// ── KbEvent ──

#[derive(Debug, Clone)]
pub struct KbEvent {
    pub key: winit::keyboard::Key,
    pub event_type: KbEventType,
    pub is_alt_pressed: bool,
    pub is_ctrl_pressed: bool,
    pub is_shift_pressed: bool,
    pub is_meta_pressed: bool,
    pub repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbEventType {
    Unknown,
    KeyDown,
    KeyUp,
}

// ── PointerEvent ──

/// 指针按钮
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

impl PointerKind {
    pub fn from_button_source(button: &winit::event::ButtonSource) -> Self {
        match button {
            winit::event::ButtonSource::Mouse(m) => PointerKind::Mouse {
                button: match m {
                    winit::event::MouseButton::Left => PointerButton::Primary,
                    winit::event::MouseButton::Right => PointerButton::Secondary,
                    winit::event::MouseButton::Middle => PointerButton::Middle,
                    other => PointerButton::Other(*other as u16),
                },
            },
            winit::event::ButtonSource::Touch { finger_id, force } => PointerKind::Touch {
                finger_id: finger_id.into_raw() as u64,
                force: force.map(|f| f.normalized(None) as f32),
            },
            winit::event::ButtonSource::TabletTool { kind, data, .. } => PointerKind::Pen {
                kind: match kind {
                    winit::event::TabletToolKind::Eraser => PenKind::Eraser,
                    _ => PenKind::Stylus,
                },
                pressure: data.force.map(|f| f.normalized(None) as f32),
            },
            _ => PointerKind::Mouse { button: PointerButton::Primary },
        }
    }
}

/// 指针事件类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEventType {
    Down,
    Up,
    Move,
    Scroll { delta: f32, is_vertical: bool },
}

/// 指针类型（对齐 Compose PointerType）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerKind {
    Mouse { button: PointerButton },
    Touch { finger_id: u64, force: Option<f32> },
    Pen { kind: PenKind, pressure: Option<f32> },
}

/// 触控笔类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenKind {
    Stylus,
    Eraser,
    Unknown,
}

/// 指针事件
#[derive(Debug, Clone)]
pub struct PointerEvent {
    pub event_type: PointerEventType,
    pub position: (f32, f32),
    pub scene_position: (f32, f32),
    pub kind: PointerKind,
    pub is_alt_pressed: bool,
    pub is_ctrl_pressed: bool,
    pub is_shift_pressed: bool,
    pub is_meta_pressed: bool,
}

// ── 图片绘制类型（ColorFilter / FilterQuality / BlendMode——对齐 Compose ui.graphics）──

/// 混合模式（对标 Compose `BlendMode`，与 skia 同源 29 值——
/// 渲染期映射 `skia_safe::BlendMode`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Clear, Src, Dst, SrcOver, DstOver, SrcIn, DstIn, SrcOut, DstOut,
    SrcATop, DstATop, Xor, Plus, Modulate, Screen, Overlay, Darken, Lighten,
    ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Multiply,
    Hue, Saturation, Color, Luminosity,
}

/// 颜色滤镜（对标 Compose `ColorFilter`——Image/Icon 渲染期挂到 paint）
#[derive(Debug, Clone, PartialEq)]
pub enum ColorFilter {
    /// 染色（对标 `ColorFilter.tint`——默认 SrcIn 保留形状 alpha）
    Tint { color: Color, blend_mode: BlendMode },
    /// 颜色矩阵（20 值行主序——对标 `ColorFilter.colorMatrix`）
    Matrix([f32; 20]),
    /// 光照效果（像素 × multiply + add——对标 `ColorFilter.lighting`）
    Lighting { multiply: Color, add: Color },
}

/// 采样质量（对标 Compose `FilterQuality`）——缩放位图时的过滤策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterQuality {
    /// 最近邻（无过滤——像素风/精确采样）
    None,
    /// 双线性（默认——缩小放大平滑）
    Low,
    /// 双线性 + 最近 mipmap（缩小更平滑）
    Medium,
    /// 三线性（双线性 + 线性 mipmap——最高质量）
    High,
}

impl Default for FilterQuality {
    fn default() -> Self { Self::Low }
}

// ── ModifierElement ──

/// Modifier 链中的单个元素。
///
/// 按类别分为 Layout / Draw / Input 三类。
/// 用 enum 而非 trait object，便于后续 Layout 系统分类提取。
#[derive(Clone)]
pub(crate) enum ModifierElement {
    // ── Layout 类 ──
    /// 固定尺寸
    /// 尺寸（静态 Dimension 或动态求值 SizeValue——布局属性动画用 State/闭包）
    Size { width: SizeValue, height: SizeValue },
    /// 最小宽度（对标 Compose `Modifier.widthIn(min=...)`——仅提升 incoming
    /// min 约束；宽度超限时被 max 夹住；支持动态值——动画可作用于 min）
    MinWidth { value: SizeValue },
    /// 最小高度（对标 Compose `Modifier.heightIn(min=...)`）
    MinHeight { value: SizeValue },
    /// 四边 padding（每边独立，支持动态 SizeValue——动画可作用于 padding）
    PaddingSides {
        start: SizeValue,
        top: SizeValue,
        end: SizeValue,
        bottom: SizeValue,
    },
    /// 填满最大宽度
    FillMaxWidth,
    /// 填满最大高度
    FillMaxHeight,
    /// 填满最大尺寸
    FillMaxSize,
    /// 位置偏移（不影响布局尺寸，仅移动绘制位置；RTL 下 x 镜像——
    /// 对标 Compose `Modifier.offset`；值支持动态——动画可作用于 offset）
    Offset { x: SizeValue, y: SizeValue },
    /// 绝对偏移（RTL 下**不**镜像——对标 Compose `Modifier.absoluteOffset`）
    AbsoluteOffset { x: SizeValue, y: SizeValue },
    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    AlignSelf { alignment: crate::layout::Alignment },
    /// 布局权重（Row 中分配宽度，Column 中分配高度）
    LayoutWeight { weight: f32 },
    /// 宽高比约束（对标 Compose `Modifier.aspectRatio`——ratio = 宽/高）
    AspectRatio { ratio: f32, match_height_first: bool },
    /// 强制尺寸（对标 Compose `Modifier.requiredSize`——忽略 incoming
    /// constraints 的收缩，允许溢出父约束）
    RequiredSize { width: Option<f32>, height: Option<f32> },
    /// 测试标记（对标 Compose `Modifier.testTag`——UI 测试定位；
    /// 调试树 JSON 暴露 tag 字段）
    TestTag { tag: String },
    /// 布局方向——本节点 padding start/end 的解析方向；Row/Column 组件
    /// 优先读自身此元素（其次 CompositionLocal 全局方向）。
    /// ⚠ 当前**不**向子树继承（对标 Compose 的子树级方向用
    /// `WiniaTheme::with_theme_and_direction`——CompositionLocal 作用域；
    /// 全 demo 切换也走它）。此元素是节点级便捷覆盖。
    LayoutDirection(crate::layout::LayoutDirection),
    /// TextField 容器子节点角色标记（text-field-v2 容器化——自定义
    /// MeasurePolicy 按角色布局：leading/label/placeholder/prefix/
    /// input/suffix/trailing；仅标记，不参与测量/绘制）
    TextFieldSlot { role: crate::ui::text_field::TextFieldSlotRole },
    /// 阴影（对标 Compose `Modifier.shadow`——elevation 模糊 + 内容裁剪）
    /// 阴影（对标 Compose `Modifier.shadow`——单层参数；elevation 便捷版
    /// 展开为 ambient+spot 两层元素）
    Shadow { params: ShadowParams, shape: Shape, clip: bool },

    // ── Draw 类 ──
    /// 背景色 + 形状（color_fn 渲染时求值——静态色或动画闭包统一为闭包）
    Background { color_fn: Arc<dyn Fn() -> Color + Send + Sync>, shape: Shape },
    /// 边框
    Border { width: f32, color: Color, shape: Shape },
    BorderDynamic { width: f32, color_fn: Arc<dyn Fn() -> Color + Send + Sync>, shape: Shape },
    /// 裁剪
    Clip { shape: Shape },
    /// 内容模糊（GPU 原生，零回读）
    Blur { radius: f32 },
    /// 背景模糊（毛玻璃，单 snapshot 多节点共享）
    BackdropBlur { radius: f32 },
    /// 文本输入框容器视觉（TextField 组件——M3 Filled/Outlined 容器：    /// 背景/指示线/边框/label/支持文本）。组合期解析全部视觉状态
    /// （enabled/focused/is_error/label 悬浮），渲染期静态绘制——
    /// 状态过渡动画由后续迭代接入。
    TextFieldVisual {
        variant: crate::ui::TextFieldVariant,
        shape: Shape,
        colors: crate::ui::TextFieldColors,
        enabled: bool,
        focused: bool,
        is_error: bool,
        /// 光标色（组合期解析：error → error_cursor，否则 primary）
        cursor_color: Color,
        /// 指示线/边框颜色（动画 State——`animate_color_as_state` 驱动，
        /// CAM16-UCS 插值；渲染期 peek 读取）
        indicator_color: crate::core::state::State<crate::modifier::Color>,
        /// 焦点过渡进度（0 = unfocused，1 = focused——宽度 1↔2px 动画）
        focus_progress: crate::core::state::State<f32>,
        /// 视觉变换偏移映射（密码掩码/格式化——渲染/定位跨界转换；
        /// None = 恒等）
        offset_mapping: Option<std::sync::Arc<dyn crate::ui::text_transformation::OffsetMapping>>,
        /// 支持文本（画在容器底部外侧 4dp）
        supporting: Option<SupportingVisual>,
    },
    /// 文本输入框偏移映射（TextField 组件内部使用——**无 M3 容器视觉**的
    /// 裸 TextField 挂载点）。TextFieldVisual 只在有 variant（Filled/Outlined）
    /// 时挂载；裸 TextField + visual_transformation 时 offset_mapping 无处
    /// 可达（点击定位/渲染光标查找 None → 显示偏移直写 selection 越界）。
    /// 仅存映射、无渲染/绘制副作用（render 各 match 走 `_ =>` 兜底）。
    TextFieldOffsetMapping {
        offset_mapping: std::sync::Arc<dyn crate::ui::text_transformation::OffsetMapping>,
    },

    // ── Content 类 ──
    /// 文本内容（由 Text 组件设置，渲染阶段消费）
    TextContent { content: String, font_size: f32, color: Color, font_weight: crate::ui::text::FontWeight, font_style: crate::ui::text::FontSlant, max_lines: usize, align: crate::ui::TextAlign, overflow: crate::ui::TextOverflow, soft_wrap: bool, letter_spacing: f32, line_height: Option<f32> },
    /// 富文本内容（含内联 drawable，由 RichText 组件设置）
    RichTextContent {
        content: String,
        drawables: Vec<std::sync::Arc<dyn crate::text::InlineDrawable>>,
        /// 内联元素在 content 中的字符范围（长度 > 1 表示多字符占位符）
        drawable_ranges: Vec<Range<usize>>,
        /// 已解析的样式范围列表
        spans: Vec<RichSpanStyle>,
    },

    // ── Input 类 ──
    /// 可点击
    /// 可点击——`interaction` 为 Some 时自动发射 press/focus/hover 交互
    /// （对标 Compose clickable(interactionSource)）
    Clickable { on_click: Arc<dyn Fn() + Send + Sync>, interaction: Option<MutableInteractionSource> },
    /// 点按手势回调（对标 Compose `detectTapGestures` 的 onTap）——
    /// 位置参数为组件本地坐标
    TapOnTap { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 双击回调
    TapOnDoubleTap { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 长按回调（当前在 up 时判定——与 Compose 即时触发有差异）
    TapOnLongPress { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 按下回调（down 立即触发）
    TapOnPress { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 拖拽开始（首次超过 touch slop）
    DragOnStart { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 拖拽移动（当前位置, 增量）
    DragOnMove { cb: Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync> },
    /// 拖拽结束
    DragOnEnd { cb: Arc<dyn Fn() + Send + Sync> },
    /// 拖拽取消（系统打断）
    DragOnCancel { cb: Arc<dyn Fn() + Send + Sync> },
    /// 可获得焦点——`interaction` 为 Some 时焦点变化自动发射 Focus/Unfocus
    Focusable { interaction: Option<MutableInteractionSource> },
    /// 悬停——指针进入/离开自动发射 Hover Enter/Exit（对标 Compose hoverable）
    Hoverable { interaction: MutableInteractionSource },
    /// 水波纹指示（对标 Compose indication/ripple）——按下时从按压点扩散的
    /// 径向渐变圆，释放后淡出；渲染期按时间计算，无额外动画状态。
    /// `shape = Some` 时波纹裁剪到该形状（Button 传入容器 shape——
    /// Outlined/Text 无背景元素时也能正确裁剪）；None 则从 Background/Border 推断。
    Ripple { source: MutableInteractionSource, color: Color, bounded: bool, shape: Option<Shape> },
    /// 图标绘制（Icon 组件内部使用）——source/tint/autoMirror/可变轴
    CustomDraw { f: Arc<dyn Fn(&skia_safe::Canvas, skia_safe::Rect) + Send + Sync> },
    /// 禁用框架焦点环（组件自绘焦点环时用——如 Slider 焦点环包围 thumb 而非整组件）
    NoFocusRing,
    DrawIcon { spec: crate::ui::icon::IconSpec },
    /// 图片内容（Image 组件——位图/SVG，ContentScale + 对齐 + alpha；
    /// 与 DrawIcon 的区别：不染色、按 ContentScale 缩放、对齐可控）
    ImageContent {
        source: crate::ui::icon::IconSource,
        content_scale: crate::ui::image::ContentScale,
        alignment: crate::ui::image::ImageAlignment,
        alpha: f32,
        color_filter: Option<ColorFilter>,
        filter_quality: FilterQuality,
    },
    /// 焦点请求器 ID（与 FocusRequester 关联）
    FocusRequesterId { id: u64 },
    /// 键盘事件
    KbEvent {
        on_key: Option<Arc<dyn Fn(&KbEvent) -> bool + Send + Sync>>,
        on_pre_key: Option<Arc<dyn Fn(&KbEvent) -> bool + Send + Sync>>,
    },
    /// 指针事件
    PointerEvent {
        on_ptr: Option<Arc<dyn Fn(&PointerEvent) -> bool + Send + Sync>>,
        on_pre_ptr: Option<Arc<dyn Fn(&PointerEvent) -> bool + Send + Sync>>,
    },
    /// 垂直滚动（绑定偏移 State）
    /// 完整 ScrollState（offset + is_scroll_in_progress + fling_limit）——
    /// 输入路径（app.rs 拖拽/wheel）经节点取整状态做 fling/极限
    VerticalScroll { state: ScrollState },
    /// Lazy 列表内容高度标记（LazyColumn 用——apply_scroll_delta 计算 max_offset；
    /// 节点自身高度是视口，内容总高由测量回写到此 State）
    LazyScroll { content_height: crate::core::state::State<f32>, reverse: bool },
    /// 水平滚动
    HorizontalScroll { state: ScrollState },
    /// 图形层变换（scale/alpha/rotation/translation——只触发重绘，不触发布局）
    GraphicsLayer { params_fn: Arc<dyn Fn() -> GraphicsLayerParams + Send + Sync> },
}

// ── Modifier ──

/// 不可变的 Modifier 链。
///
/// 每次调用修饰方法返回一个新的 Modifier（内部 elements 是 clone-on-write 式的追加）。
/// Clone 是廉价操作。
#[derive(Debug, Clone)]
pub struct Modifier {
    elements: Vec<ModifierElement>,
}

impl Modifier {
    /// 创建空的 Modifier 链
    pub fn new() -> Self {
        Modifier {
            elements: Vec::new(),
        }
    }

    /// 内部方法：追加一个元素并返回新 Modifier
    pub(crate) fn push(mut self, element: ModifierElement) -> Self {
        self.elements.push(element);
        self
    }

    /// 合并另一个 Modifier 链的所有元素（追加到末尾）。
    ///
    /// 等价于 Compose 的 `Modifier.then(other)`，用于叠加两个独立的 Modifier 链。
    ///
    /// # 示例
    /// ```ignore
    /// let base = Modifier::new().size(100, 50).background(RED);
    /// let extra = Modifier::new().padding(8).clickable(|| {});
    /// let combined = base.then(extra);
    /// // combined = size(100,50) → background(RED) → padding(8) → clickable
    /// ```
    pub fn then(mut self, other: Modifier) -> Self {
        self.elements.extend(other.elements);
        self
    }

    /// 返回所有元素的只读引用
    pub(crate) fn elements(&self) -> &[ModifierElement] {
        &self.elements
    }
}

impl Default for Modifier {
    fn default() -> Self {
        Self::new()
    }
}

// ── Layout Modifier 方法 ──

impl Modifier {
    /// 设置固定宽高
    pub fn size(self, width: impl Into<SizeValue>, height: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Size {
            width: width.into(),
            height: height.into(),
        })
    }

    /// 仅设置宽度（支持动态：`State<f32>`/`DerivedValue`/闭包——measure 时读）
    pub fn width(self, w: impl Into<SizeValue>) -> Self {
        // 使用 Auto 占位高度，表示不约束
        self.push(ModifierElement::Size {
            width: w.into(),
            height: SizeValue::Static(Dimension::Auto),
        })
    }

    /// 仅设置高度（支持动态：`State<f32>`/`DerivedValue`/闭包——measure 时读）
    pub fn height(self, h: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Size {
            width: SizeValue::Static(Dimension::Auto),
            height: h.into(),
        })
    }

    /// 四边等距 padding（支持动态：`State<f32>`/`DerivedValue`/闭包——
    /// 动画可作用于 padding，对标 Compose `animateDpAsState` + `padding`）
    pub fn padding(self, all: impl Into<SizeValue>) -> Self {
        let v = all.into();
        self.push(ModifierElement::PaddingSides {
            start: v.clone(),
            top: v.clone(),
            end: v.clone(),
            bottom: v,
        })
    }

    /// 水平方向 padding（start + end）
    pub fn padding_horizontal(self, value: impl Into<SizeValue>) -> Self {
        let v = value.into();
        self.push(ModifierElement::PaddingSides {
            start: v.clone(),
            top: SizeValue::Static(Dimension::Auto),
            end: v,
            bottom: SizeValue::Static(Dimension::Auto),
        })
    }

    /// 垂直方向 padding（top + bottom）
    pub fn padding_vertical(self, value: impl Into<SizeValue>) -> Self {
        let v = value.into();
        self.push(ModifierElement::PaddingSides {
            start: SizeValue::Static(Dimension::Auto),
            top: v.clone(),
            end: SizeValue::Static(Dimension::Auto),
            bottom: v,
        })
    }

    /// 起始边（左）padding——单边，支持动态
    pub fn padding_start(self, value: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: value.into(),
            top: SizeValue::Static(Dimension::Auto),
            end: SizeValue::Static(Dimension::Auto),
            bottom: SizeValue::Static(Dimension::Auto),
        })
    }

    /// 末尾边（右）padding——单边，支持动态
    pub fn padding_end(self, value: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: SizeValue::Static(Dimension::Auto),
            top: SizeValue::Static(Dimension::Auto),
            end: value.into(),
            bottom: SizeValue::Static(Dimension::Auto),
        })
    }

    /// 顶部 padding——单边，支持动态
    pub fn padding_top(self, value: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: SizeValue::Static(Dimension::Auto),
            top: value.into(),
            end: SizeValue::Static(Dimension::Auto),
            bottom: SizeValue::Static(Dimension::Auto),
        })
    }

    /// 底部 padding——单边，支持动态
    pub fn padding_bottom(self, value: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: SizeValue::Static(Dimension::Auto),
            top: SizeValue::Static(Dimension::Auto),
            end: SizeValue::Static(Dimension::Auto),
            bottom: value.into(),
        })
    }

    /// 四边各自独立 padding（对标 Compose
    /// `Modifier.padding(start = .., top = .., end = .., bottom = ..)`）。
    /// 参数顺序：(start, top, end, bottom)——全部支持动态。
    /// 与其它 padding 方法可叠加（累积）。
    pub fn padding_sides(
        self,
        start: impl Into<SizeValue>,
        top: impl Into<SizeValue>,
        end: impl Into<SizeValue>,
        bottom: impl Into<SizeValue>,
    ) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: start.into(),
            top: top.into(),
            end: end.into(),
            bottom: bottom.into(),
        })
    }

    /// 宽度填满可用空间
    pub fn fill_max_width(self) -> Self {
        self.push(ModifierElement::FillMaxWidth)
    }

    /// 最小宽度（对标 Compose `Modifier.widthIn(min = ...)`）——提升布局
    /// 最小约束，内容不足时撑到该宽度；受父 max 约束夹住。支持动态值。
    pub fn min_width(self, value: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::MinWidth { value: value.into() })
    }

    /// 最小高度（对标 Compose `Modifier.heightIn(min = ...)`）
    pub fn min_height(self, value: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::MinHeight { value: value.into() })
    }

    /// 高度填满可用空间
    pub fn fill_max_height(self) -> Self {
        self.push(ModifierElement::FillMaxHeight)
    }

    /// 宽高填满可用空间
    pub fn fill_max_size(self) -> Self {
        self.push(ModifierElement::FillMaxSize)
    }

    /// 位置偏移（不影响布局尺寸，仅移动绘制位置；RTL 下 x 镜像——
    /// 对标 Compose `Modifier.offset`）。
    /// 值支持动态（`State`/`DerivedValue`/闭包——动画可作用于 offset）。
    pub fn offset(self, x: impl Into<SizeValue>, y: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Offset { x: x.into(), y: y.into() })
    }

    /// 仅 x 方向偏移（RTL 下镜像）
    pub fn offset_x(self, x: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Offset { x: x.into(), y: SizeValue::Static(Dimension::Fixed(0.0)) })
    }

    /// 仅 y 方向偏移（不受 RTL 影响——垂直方向）
    pub fn offset_y(self, y: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Offset { x: SizeValue::Static(Dimension::Fixed(0.0)), y: y.into() })
    }

    /// 绝对偏移（RTL 下**不**镜像——对标 Compose `Modifier.absoluteOffset`）。
    /// 值支持动态。
    pub fn absolute_offset(self, x: impl Into<SizeValue>, y: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: x.into(), y: y.into() })
    }

    /// 仅 x 方向绝对偏移（RTL 下不镜像）
    pub fn absolute_offset_x(self, x: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: x.into(), y: SizeValue::Static(Dimension::Fixed(0.0)) })
    }

    /// 仅 y 方向绝对偏移
    pub fn absolute_offset_y(self, y: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: SizeValue::Static(Dimension::Fixed(0.0)), y: y.into() })
    }

    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    pub fn align_self(self, alignment: crate::layout::Alignment) -> Self {
        self.push(ModifierElement::AlignSelf { alignment })
    }

    /// 布局权重（Row 中按比例分配宽度，Column 中按比例分配高度）
    pub fn layout_weight(self, weight: f32) -> Self {
        self.push(ModifierElement::LayoutWeight { weight })
    }

    /// `aspect_ratio(ratio)`（对标 Compose `Modifier.aspectRatio`）——
    /// 约束本节点宽高比（ratio = 宽/高，必须 > 0）。
    ///
    /// `match_height_first = true` 时优先按高度约束推导宽度
    /// （对标 `matchHeightConstraintsFirst`）。
    pub fn aspect_ratio(mut self, ratio: f32, match_height_first: bool) -> Self {
        assert!(ratio > 0.0, "aspectRatio {ratio} must be > 0（Compose 前置校验）");
        self.push(ModifierElement::AspectRatio { ratio, match_height_first })
    }

    /// `required_size(w, h)`（对标 Compose `Modifier.requiredSize`）——
    /// 强制本节点为该尺寸，**忽略 incoming constraints 的收缩**（允许
    /// 溢出父约束——enforceIncoming=false 语义）。单轴用
    /// `required_width` / `required_height`。
    pub fn required_size(self, width: f32, height: f32) -> Self {
        self.push(ModifierElement::RequiredSize { width: Some(width), height: Some(height) })
    }

    /// 仅强制宽度
    pub fn required_width(self, width: f32) -> Self {
        self.push(ModifierElement::RequiredSize { width: Some(width), height: None })
    }

    /// 仅强制高度
    pub fn required_height(self, height: f32) -> Self {
        self.push(ModifierElement::RequiredSize { width: None, height: Some(height) })
    }

    /// `test_tag(tag)`（对标 Compose `Modifier.testTag`）——给节点打测试
    /// 标记，UI 测试/调试树用其定位节点（树 JSON 的 `tag` 字段）。
    pub fn test_tag(self, tag: impl Into<String>) -> Self {
        self.push(ModifierElement::TestTag { tag: tag.into() })
    }

    /// 布局方向——本节点 padding start/end 解析 + Row/Column 容器布局方向。
    /// 节点级便捷覆盖；子树级方向切换用
    /// `WiniaTheme::with_theme_and_direction`（CompositionLocal 作用域）。
    pub fn layout_direction(self, d: crate::layout::LayoutDirection) -> Self {
        self.push(ModifierElement::LayoutDirection(d))
    }

    /// TextField 容器子节点角色标记（text-field-v2 容器化内部使用——
    /// TextFieldLayout policy 按角色布局）
    pub(crate) fn text_field_slot(self, role: crate::ui::text_field::TextFieldSlotRole) -> Self {
        self.push(ModifierElement::TextFieldSlot { role })
    }

    /// `shadow(elevation, shape, clip, color)`（对标 Compose `Modifier.shadow`）——
    /// elevation 便捷版：展开为 **ambient + spot 两层**（Compose DropShadow
    /// 物理阴影——ambient 无偏移大模糊低 alpha、spot 偏移 e*0.5 小模糊中
    /// alpha；alpha 随 elevation 增强——Material 层级感）。
    /// 默认：RectangleShape、`clip = elevation > 0`、黑色阴影。
    /// elevation <= 0 且 !clip 时返回自身（Compose early-return 语义）。
    pub fn shadow(
        self,
        elevation: f32,
        shape: impl Into<Shape>,
        clip: bool,
        color: Color,
    ) -> Self {
        if elevation <= 0.0 && !clip {
            return self;
        }
        let strength = (elevation / 12.0).min(1.0);
        // ambient：无偏移、模糊半径 = e（SkShadowUtils：ambient blur = elevation——
        // 大范围环境光）、alpha 0.18（桌面观感——宽而淡；之前 0.25 偏深）
        let ambient = ShadowParams::new(
            elevation, 0.0, 0.0,
            color, 0.18 * strength,
        );
        // spot：偏移 (0, e*0.5)、模糊半径 = e*0.25（SkShadowUtils 同款——
        // 边缘清晰的下阴影）、alpha 0.30
        let spot = ShadowParams::new(
            elevation * 0.25, 0.0, elevation * 0.5,
            color, 0.30 * strength,
        );
        let shape = shape.into();
        self.push(ModifierElement::Shadow { params: ambient, shape: shape.clone(), clip })
            .push(ModifierElement::Shadow { params: spot, shape, clip })
    }

    /// `shadow(elevation)`——便捷版（默认形状/黑色/自动 clip）
    pub fn shadow_default(self, elevation: f32) -> Self {
        self.shadow(elevation, Shape::Rectangle, elevation > 0.0, Color::from_argb(255, 0, 0, 0))
    }

    /// `drop_shadow(shape, params)`（对标 Compose `Modifier.dropShadow`）——
    /// 完全自定义单层阴影（radius/spread/offset/color/alpha）。多个
    /// drop_shadow 可叠加（多阴影，Compose vararg 语义）。
    pub fn drop_shadow(self, shape: impl Into<Shape>, params: ShadowParams) -> Self {
        self.push(ModifierElement::Shadow {
            params,
            shape: shape.into(),
            clip: false,
        })
    }
}

// ── Draw Modifier 方法 ──

impl Modifier {
    /// 设置背景色和形状
    /// 设置背景色和形状。
    ///
    /// 统一入口：静态颜色与动画闭包自动适配（`impl Into<BackgroundColor>`）。
    ///
    /// **静态用法**（构建时固定颜色）：
    /// ```
    /// # use winia::prelude::*;
    /// let _m = Modifier::new().background(Color::RED, Shape::Circle);
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```ignore
    /// .background(|| pulse.peek(), Shape::Circle)
    /// ```
    ///
    /// **原则**：静态绘制属性传 `Color`；需要动画（颜色随帧变化）时传闭包，
    /// 闭包内用 `State::peek()` 读取动画值（不要用 `get()`——会注册依赖触发重组）。
    /// 动画值由 `set_visual` 写入（不 notify），配合动画引擎每帧 `request_redraw` 实现零重组。
    pub fn background(self, color: impl Into<BackgroundColor>, shape: impl Into<Shape>) -> Self {
        let bg = color.into();
        self.push(ModifierElement::Background {
            color_fn: bg.0,
            shape: shape.into(),
        })
    }

    /// 统一构造 TextContent 元素（Text/TextField 共用——字段单一来源，P3-6）
    pub(crate) fn text_content(
        mut self,
        content: String,
        font_size: f32,
        color: crate::modifier::Color,
        font_weight: crate::ui::text::FontWeight,
        font_style: crate::ui::text::FontSlant,
        max_lines: usize,
        align: crate::ui::TextAlign,
        overflow: crate::ui::TextOverflow,
        soft_wrap: bool,
    ) -> Self {
        self.text_content_full(
            content, font_size, color, font_weight, font_style,
            max_lines, align, overflow, soft_wrap,
            0.0, None,
        )
    }

    /// 全参版（含 letter_spacing/line_height——Text 组件用，对标 Compose
    /// TextStyle.letterSpacing/lineHeight）
    pub(crate) fn text_content_full(
        mut self,
        content: String,
        font_size: f32,
        color: crate::modifier::Color,
        font_weight: crate::ui::text::FontWeight,
        font_style: crate::ui::text::FontSlant,
        max_lines: usize,
        align: crate::ui::TextAlign,
        overflow: crate::ui::TextOverflow,
        soft_wrap: bool,
        letter_spacing: f32,
        line_height: Option<f32>,
    ) -> Self {
        self.push(ModifierElement::TextContent {
            content,
            font_size,
            color,
            font_weight,
            font_style,
            max_lines,
            align,
            overflow,
            soft_wrap,
            letter_spacing,
            line_height,
        })
    }

    /// 设置边框
    pub fn border(self, width: f32, color: Color, shape: impl Into<Shape>) -> Self {
        self.push(ModifierElement::Border {
            width,
            color,
            shape: shape.into(),
        })
    }

    /// 动态颜色边框（对标 background 的动态闭包语义——渲染期每帧求值，
    /// 供颜色过渡动画使用；`peek()` 读取动画值）
    pub fn border_dynamic(
        self,
        width: f32,
        color_fn: impl Fn() -> Color + Send + Sync + 'static,
        shape: impl Into<Shape>,
    ) -> Self {
        self.push(ModifierElement::BorderDynamic {
            width,
            color_fn: Arc::new(color_fn),
            shape: shape.into(),
        })
    }

    /// 设置裁剪形状
    pub fn clip(self, shape: impl Into<Shape>) -> Self {
        self.push(ModifierElement::Clip {
            shape: shape.into(),
        })
    }

    /// 内容模糊（GPU saveLayer，零回读）
    pub fn blur(self, radius: f32) -> Self {
        self.push(ModifierElement::Blur { radius })
    }

    /// 背景模糊（毛玻璃）——渲染期在节点内容绘制前即时 snapshot+blur
    pub fn backdrop_blur(self, radius: f32) -> Self {
        self.push(ModifierElement::BackdropBlur { radius })
    }

    /// 文本输入框容器视觉（TextField 组件内部使用——M3 容器绘制参数）
    pub fn text_field_visual(
        self,
        variant: crate::ui::TextFieldVariant,
        shape: Shape,
        colors: crate::ui::TextFieldColors,
        enabled: bool,
        focused: bool,
        is_error: bool,
        cursor_color: Color,
        indicator_color: crate::core::state::State<crate::modifier::Color>,
        focus_progress: crate::core::state::State<f32>,
        offset_mapping: Option<std::sync::Arc<dyn crate::ui::text_transformation::OffsetMapping>>,
        supporting: Option<SupportingVisual>,
    ) -> Self {
        self.push(ModifierElement::TextFieldVisual {
            variant,
            shape,
            colors,
            enabled,
            focused,
            is_error,
            cursor_color,
            indicator_color,
            focus_progress,
            offset_mapping,
            supporting,
        })
    }

    /// 文本输入框偏移映射（TextField 组件内部使用——裸 TextField 无
    /// TextFieldVisual 时挂载；`offset_mapping_for_node` 沿 parent 链向上
    /// 查找 TextFieldVisual.offset_mapping 或本元素）
    pub fn text_field_offset_mapping(
        self,
        offset_mapping: std::sync::Arc<dyn crate::ui::text_transformation::OffsetMapping>,
    ) -> Self {
        self.push(ModifierElement::TextFieldOffsetMapping { offset_mapping })
    }
}

// ── Input Modifier 方法 ──

impl Modifier {
    /// 添加点击行为
    pub fn clickable(self, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::Clickable {
            on_click: Arc::new(on_click),
            interaction: None,
        })
    }

    /// 添加点击行为并绑定交互源——按下/释放/聚焦/悬停自动发射到 `source`
    /// （对标 Compose `clickable(interactionSource)`：内部组合 focusable + hover）
    pub fn clickable_with_source(
        self,
        source: &MutableInteractionSource,
        on_click: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let source = source.clone();
        self.push(ModifierElement::Clickable {
            on_click: Arc::new(on_click),
            interaction: Some(source.clone()),
        })
        .push(ModifierElement::Focusable { interaction: Some(source.clone()) })
        .push(ModifierElement::Hoverable { interaction: source })
    }

    // ── 手势（对标 Compose detectTapGestures / detectDragGestures） ──

    /// 点按回调（up 且未超过 touch slop）——参数为本地坐标
    pub fn on_tap(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnTap { cb: Arc::new(cb) })
    }

    /// 双击回调（两次 tap 间隔 < 300ms 且位置差 < 50px）
    pub fn on_double_tap(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnDoubleTap { cb: Arc::new(cb) })
    }

    /// 长按回调（down 持续 > 500ms 且未移动）——当前在 up 时判定
    pub fn on_long_press(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnLongPress { cb: Arc::new(cb) })
    }

    /// 按下回调（down 立即触发——onPress 语义）
    pub fn on_press(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnPress { cb: Arc::new(cb) })
    }

    /// 拖拽开始回调（首次超过 touch slop）
    pub fn on_drag_start(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnStart { cb: Arc::new(cb) })
    }

    /// 拖拽移动回调（当前位置, 增量）
    pub fn on_drag(self, cb: impl Fn((f32, f32), (f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnMove { cb: Arc::new(cb) })
    }

    /// 拖拽结束回调
    pub fn on_drag_end(self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnEnd { cb: Arc::new(cb) })
    }

    /// 拖拽取消回调（系统打断）
    pub fn on_drag_cancel(self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnCancel { cb: Arc::new(cb) })
    }

    /// 标记为可获焦点
    pub fn focusable(self) -> Self {
        self.push(ModifierElement::Focusable { interaction: None })
    }

    /// 标记为可获焦点并绑定交互源——焦点变化自动发射 Focus/Unfocus
    pub fn focusable_with_source(self, source: &MutableInteractionSource) -> Self {
        self.push(ModifierElement::Focusable { interaction: Some(source.clone()) })
    }

    /// 悬停——指针进入/离开自动发射 Hover Enter/Exit（对标 Compose hoverable）
    pub fn hoverable(self, source: &MutableInteractionSource) -> Self {
        self.push(ModifierElement::Hoverable { interaction: source.clone() })
    }

    /// 水波纹指示（对标 Compose `indication` + Material ripple）。
    /// `bounded=true` 时裁剪到节点形状（默认）；`color` 为波纹颜色
    /// （Material 用 content 色——Button 已自动附加，通用 clickable 可手动加）。
    pub fn ripple(self, source: &MutableInteractionSource, color: Color, bounded: bool) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color,
            bounded,
            shape: None,
        })
    }

    /// 水波纹指示——显式指定裁剪形状（对标 Compose Surface indication：
    /// 容器 shape 同时决定波纹裁剪；Outlined/Text 等无背景元素的按钮
    /// 必须显式传 shape，否则裁剪回退为矩形）
    pub fn ripple_with_shape(
        self,
        source: &MutableInteractionSource,
        color: Color,
        bounded: bool,
        shape: Shape,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color,
            bounded,
            shape: Some(shape),
        })
    }

    /// 绘制图标（Icon 组件内部使用）——tint/autoMirror/可变轴在渲染期求值
    /// 自定义绘制：渲染期以节点 rect 调用闭包（Canvas 原语——轨道/刻度/
/// thumb 等动态绘制；对齐 Compose Canvas/drawWithCache 的轻量替代）。
/// 闭包在渲染期调用——读 State::peek() 不触发重组（值变化由外部 notify 驱动重绘）。
pub fn draw(self, f: impl Fn(&skia_safe::Canvas, skia_safe::Rect) + Send + Sync + 'static) -> Self {
    self.push(ModifierElement::CustomDraw { f: Arc::new(f) })
}

/// 禁用框架自动焦点环（组件自绘焦点环时用——如 Slider 焦点环包围 thumb 胶囊，
/// 而非整个组件 rect）
pub fn no_focus_ring(self) -> Self {
    self.push(ModifierElement::NoFocusRing)
}

pub fn draw_icon(self, spec: crate::ui::icon::IconSpec) -> Self {
        self.push(ModifierElement::DrawIcon { spec })
    }

    /// 图片内容元素（Image 组件用——绘制按 ContentScale/对齐/alpha）
    pub fn image_content(
        self,
        source: crate::ui::icon::IconSource,
        content_scale: crate::ui::image::ContentScale,
        alignment: crate::ui::image::ImageAlignment,
        alpha: f32,
        color_filter: Option<ColorFilter>,
        filter_quality: FilterQuality,
    ) -> Self {
        self.push(ModifierElement::ImageContent { source, content_scale, alignment, alpha, color_filter, filter_quality })
    }

    /// 图片固有尺寸（位图像素尺寸 / SVG viewBox）——测量期调用
    pub(crate) fn image_intrinsic_size(&self) -> Option<(f32, f32)> {
        for el in &self.elements {
            if let ModifierElement::ImageContent { source, .. } = el {
                return source.intrinsic_size();
            }
        }
        None
    }

    /// 关联 FocusRequester（不消耗所有权）
    pub fn on_key_event(self, handler: impl Fn(&KbEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::KbEvent { on_key: Some(Arc::new(handler)), on_pre_key: None })
    }

    /// 预拦截按键事件
    pub fn on_pre_key_event(self, handler: impl Fn(&KbEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::KbEvent { on_key: None, on_pre_key: Some(Arc::new(handler)) })
    }

    /// 关联 FocusRequester

    /// 指针事件（inner→outer 冒泡）
    pub fn on_pointer_event(self, handler: impl Fn(&PointerEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::PointerEvent { on_ptr: Some(Arc::new(handler)), on_pre_ptr: None })
    }

    /// 预拦截指针事件（outer→inner）
    pub fn on_pre_pointer_event(self, handler: impl Fn(&PointerEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::PointerEvent { on_ptr: None, on_pre_ptr: Some(Arc::new(handler)) })
    }

    /// 关联 FocusRequester
    pub fn focus_requester(self, fr: impl Into<FocusRequester>) -> Self {
        let fr = fr.into();
        self.push(ModifierElement::FocusRequesterId { id: fr.id })
    }

    /// 图形层变换（scale/alpha/rotation/translation）。
    ///
    /// 统一入口：静态参数与动画闭包自动适配（`impl Into<GraphicsLayerSpec>`）。
    ///
    /// **静态用法**：
    /// ```
    /// # use winia::prelude::*;
    /// # use winia::modifier::GraphicsLayerParams;
    /// let _m = Modifier::new().graphics_layer(GraphicsLayerParams { alpha: 0.5, ..Default::default() });
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```ignore
    /// .graphics_layer(|| GraphicsLayerParams { alpha: pulse.peek(), ..Default::default() })
    /// ```
    ///
    /// **原则**：与 `background` 相同——静态传值，动画传闭包（`peek()` 读取，
    /// 配合 `set_visual` 零重组）。仅影响绘制层，不触发布局。
    pub fn graphics_layer(self, params: impl Into<GraphicsLayerSpec>) -> Self {
        let spec = params.into();
        self.push(ModifierElement::GraphicsLayer {
            params_fn: spec.0,
        })
    }

    /// `alpha(a)`（对标 Compose `Modifier.alpha`）——透明度便捷包装：
    /// `a != 1.0` 时应用 `graphicsLayer(alpha = a, clip = true)`（**alpha<1
    /// 隐式裁剪到 bounds**——Compose 语义）。已存在 GraphicsLayer 元素时
    /// **合并**（alpha 相乘 + 开 clip）而非叠加嵌套层。
    pub fn alpha(self, alpha: f32) -> Self {
        if alpha == 1.0 {
            return self;
        }
        self.merge_graphics_layer(move |p| {
            p.alpha *= alpha;
            p.clip = true;
        })
    }

    /// `rotate(degrees)`（对标 Compose `Modifier.rotate`）——绕**中心**
    /// 顺时针旋转（默认 transformOrigin=Center）。`degrees != 0` 才应用。
    pub fn rotate(self, degrees: f32) -> Self {
        if degrees == 0.0 {
            return self;
        }
        self.merge_graphics_layer(move |p| p.rotation_z += degrees)
    }

    /// `scale(sx, sy)`（对标 Compose `Modifier.scale`）——绕**中心**缩放。
    /// 非 1 才应用。
    pub fn scale(self, sx: f32, sy: f32) -> Self {
        if sx == 1.0 && sy == 1.0 {
            return self;
        }
        self.merge_graphics_layer(move |p| {
            p.scale_x *= sx;
            p.scale_y *= sy;
        })
    }

    /// `rotationX(degrees)`（对标 Compose `Modifier.rotationX`）——绕 X 轴
    /// 3D 旋转（带 cameraDistance 透视；绕 transformOrigin）。`degrees != 0`
    /// 才应用。
    pub fn rotation_x(self, degrees: f32) -> Self {
        if degrees == 0.0 {
            return self;
        }
        self.merge_graphics_layer(move |p| p.rotation_x += degrees)
    }

    /// `rotationY(degrees)`（对标 Compose `Modifier.rotationY`）——绕 Y 轴
    /// 3D 旋转（带 cameraDistance 透视；绕 transformOrigin）。
    pub fn rotation_y(self, degrees: f32) -> Self {
        if degrees == 0.0 {
            return self;
        }
        self.merge_graphics_layer(move |p| p.rotation_y += degrees)
    }

    /// `cameraDistance(distance)`（对标 Compose `Modifier.cameraDistance`）——
    /// 3D 旋转的相机距离（逻辑 px；越大透视越平，Compose 默认 8.dp）。
    pub fn camera_distance(self, distance: f32) -> Self {
        self.merge_graphics_layer(move |p| p.camera_distance = distance)
    }

    /// `shadowElevation(elevation)`（对标 Compose `graphicsLayer.shadowElevation`）——
    /// 图层阴影高度（逻辑 px；>0 时在图层内容后画 ambient+spot 阴影）。
    pub fn shadow_elevation(self, elevation: f32) -> Self {
        if elevation <= 0.0 {
            return self;
        }
        self.merge_graphics_layer(move |p| p.shadow_elevation = elevation)
    }

    /// 图层阴影形状（对标 Compose `graphicsLayer.shape`——默认矩形）
    pub fn shadow_shape(self, shape: impl Into<Shape>) -> Self {
        let shape = shape.into();
        self.merge_graphics_layer(move |p| p.shadow_shape = Some(shape.clone()))
    }

    /// 图层环境光阴影颜色（对标 Compose `ambientShadowColor`）。
    pub fn ambient_shadow_color(self, color: Color) -> Self {
        self.merge_graphics_layer(move |p| p.ambient_shadow_color = color)
    }

    /// 图层投射光阴影颜色（对标 Compose `spotShadowColor`）。
    pub fn spot_shadow_color(self, color: Color) -> Self {
        self.merge_graphics_layer(move |p| p.spot_shadow_color = color)
    }

    /// 便捷包装合并：已有 GraphicsLayer 元素 → 包装其 params_fn（叠加）；
    /// 否则 push 新元素。
    fn merge_graphics_layer(mut self, f: impl Fn(&mut GraphicsLayerParams) + Send + Sync + 'static) -> Self {
        let f = Arc::new(f);
        // mem::take 取出 elements（空 Vec 占位）——局部修改后再放回，
        // 完全避开 self 借用与 push 的冲突
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements
            .iter_mut()
            .find(|e| matches!(e, ModifierElement::GraphicsLayer { .. }))
        {
            if let ModifierElement::GraphicsLayer { params_fn } = el {
                let old = params_fn.clone();
                let f = f.clone();
                *params_fn = Arc::new(move || {
                    let mut p = (old)();
                    f(&mut p);
                    p
                });
            }
        } else {
            elements.push(ModifierElement::GraphicsLayer {
                params_fn: Arc::new(move || {
                    let mut p = GraphicsLayerParams::default();
                    f(&mut p);
                    p
                }),
            });
        }
        self.elements = elements;
        self
    }

    /// 垂直滚动（绑定 ScrollState）
    pub fn vertical_scroll(self, state: ScrollState) -> Self {
        // 读取 offset 以注册 State→Slot 依赖，确保滚动时触发增量重组
        let _ = state.offset.get();
        self.push(ModifierElement::VerticalScroll { state })
    }

    /// 水平滚动
    /// 标记为 lazy 滚动容器（LazyColumn 内部使用——内容总高 State）
    pub fn lazy_scroll(self, content_height: crate::core::state::State<f32>) -> Self {
        self.push(ModifierElement::LazyScroll { content_height, reverse: false })
    }

    /// lazy 列表是否反向布局（reverseLayout：render 滚动平移镜像）
    pub fn is_lazy_scroll_reverse(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::LazyScroll { reverse: true, .. }))
    }

    /// lazy 列表反向布局标记（reverseLayout：render 侧镜像滚动平移）
    pub fn lazy_scroll_reverse(mut self, reverse: bool) -> Self {
        if let Some(el) = self.elements.last_mut() {
            if let ModifierElement::LazyScroll { reverse: r, .. } = el {
                *r = reverse;
            }
        }
        self
    }

    /// 水平滚动（绑定 ScrollState——对齐 vertical_scroll）
    pub fn horizontal_scroll(self, state: ScrollState) -> Self {
        let _ = state.offset.get();
        self.push(ModifierElement::HorizontalScroll { state })
    }
}

// ── 查询方法: 布局参数提取 ──

impl Modifier {
    /// 累积的四边 padding（start, top, end, bottom 各自独立）——
    /// 非对称 padding 的基础查询，其余查询方法由其派生。
    /// 动态值（State/闭包）在此求值（measure/layout 期间读 State 会
    /// 注册布局依赖——动画更新 State → 节点重测）
    /// 布局方向元素：返回自身覆盖的方向（无则 None——继承父作用域）
    pub fn get_layout_direction(&self) -> Option<crate::layout::LayoutDirection> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::LayoutDirection(d) => Some(*d),
            _ => None,
        })
    }

    pub fn get_padding_sides(&self) -> (f32, f32, f32, f32) {
        use crate::unit::{current_density, Dp, Px};
        let resolve = |sv: &SizeValue| -> f32 {
            match sv {
                SizeValue::Static(Dimension::Fixed(v)) | SizeValue::Static(Dimension::Dp(Dp(v))) => *v,
                SizeValue::Static(Dimension::Px(p)) => p.to_logical(current_density()),
                SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => 0.0,
                SizeValue::Dynamic(f) => f(),
            }
        };
        let (mut s, mut t, mut e, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for el in &self.elements {
            if let ModifierElement::PaddingSides { start, top, end, bottom } = el {
                s += resolve(start);
                t += resolve(top);
                e += resolve(end);
                b += resolve(bottom);
            }
        }
        (s, t, e, b)
    }

    /// 水平方向 padding——返回 (start, end)（各自独立，非对称时不同）
    pub fn get_padding_horizontal(&self) -> (f32, f32) {
        let (s, _, e, _) = self.get_padding_sides();
        (s, e)
    }

    /// 垂直方向 padding——返回 (top, bottom)（各自独立，非对称时不同）
    pub fn get_padding_vertical(&self) -> (f32, f32) {
        let (_, t, _, b) = self.get_padding_sides();
        (t, b)
    }

    /// 累积 padding 总量——返回 (start+end, top+bottom)
    pub fn get_padding_values(&self) -> (f32, f32) {
        let (s, t, e, b) = self.get_padding_sides();
        (s + e, t + b)
    }

    /// 固定尺寸（从 Size modifier 提取）
    pub fn fixed_size(&self) -> Option<(Dimension, Dimension)> {
        for el in &self.elements {
            if let ModifierElement::Size { width: SizeValue::Static(w), height: SizeValue::Static(h) } = el {
                return Some((*w, *h));
            }
        }
        None
    }

    /// 解析 Size 元素的尺寸（静态/动态单轴独立解析）——返回 (width, height) 解析值，
    /// None 表示该轴不约束（Auto/Fill）。
    pub fn resolved_size(&self) -> Option<(Option<f32>, Option<f32>)> {
        use crate::unit::{current_density, Dp, Px};
        // 合并所有 Size 元素（链序：后 push 的外层胜出——非 None 覆盖）。
        // ⚠ 不能只返回第一个：`width(300).height(dyn)` 是两个 Size 元素，
        // 只取第一个会丢 height（min_lines 动态高度失效的根因）
        let mut out: Option<(Option<f32>, Option<f32>)> = None;
        for el in &self.elements {
            if let ModifierElement::Size { width, height } = el {
                let resolve = |sv: &SizeValue| -> Option<f32> {
                    match sv {
                        SizeValue::Static(Dimension::Fixed(v)) | SizeValue::Static(Dimension::Dp(Dp(v))) => Some(*v),
                        SizeValue::Static(Dimension::Px(p)) => Some(p.to_logical(current_density())),
                        SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => None,
                        SizeValue::Dynamic(f) => Some(f()),
                    }
                };
                let (w, h) = (resolve(width), resolve(height));
                out = Some(match out {
                    Some((ow, oh)) => (w.or(ow), h.or(oh)),
                    None => (w, h),
                });
            }
        }
        out
    }

    /// 解析 MinWidth/MinHeight 元素——返回 (min_width, min_height)，None 表示
    /// 该轴无最小约束。动态值在布局期求值（State::get 注册 layout_dep——
    /// 动画可驱动 min 尺寸，只重测不重组）。
    pub fn min_size_constraint(&self) -> (Option<f32>, Option<f32>) {
        use crate::unit::{current_density, Dp, Px};
        let resolve = |sv: &SizeValue| -> Option<f32> {
            match sv {
                SizeValue::Static(Dimension::Fixed(v)) | SizeValue::Static(Dimension::Dp(Dp(v))) => Some(*v),
                SizeValue::Static(Dimension::Px(p)) => Some(p.to_logical(current_density())),
                SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => None,
                SizeValue::Dynamic(f) => Some(f()),
            }
        };
        let mut out = (None, None);
        for el in &self.elements {
            match el {
                ModifierElement::MinWidth { value } => {
                    if let Some(v) = resolve(value) { out.0 = Some(v); }
                }
                ModifierElement::MinHeight { value } => {
                    if let Some(v) = resolve(value) { out.1 = Some(v); }
                }
                _ => {}
            }
        }
        out
    }

    /// 是否填满最大宽度
    pub fn is_fill_max_width(&self) -> bool {
        self.elements.iter().any(|el| matches!(el,
            ModifierElement::FillMaxWidth | ModifierElement::FillMaxSize
        ))
    }

    /// 是否填满最大高度
    pub fn is_fill_max_height(&self) -> bool {
        self.elements.iter().any(|el| matches!(el,
            ModifierElement::FillMaxHeight | ModifierElement::FillMaxSize
        ))
    }

    /// 垂直滚动状态（如果有 VerticalScroll modifier）
    pub fn vertical_scroll_state(&self) -> Option<&ScrollState> {
        for el in &self.elements {
            if let ModifierElement::VerticalScroll { state } = el {
                return Some(state);
            }
        }
        None
    }

    /// lazy 列表内容高度 State（如果有 LazyScroll modifier）
    pub fn lazy_scroll_content_height(&self) -> Option<&crate::core::state::State<f32>> {
        for el in &self.elements {
            if let ModifierElement::LazyScroll { content_height, .. } = el {
                return Some(content_height);
            }
        }
        None
    }

    /// 水平滚动状态（如果有 HorizontalScroll modifier）
    pub fn horizontal_scroll_state(&self) -> Option<&ScrollState> {
        for el in &self.elements {
            if let ModifierElement::HorizontalScroll { state } = el {
                return Some(state);
            }
        }
        None
    }

    /// 布局权重（供 Column/Row 使用）
    pub fn get_layout_weight(&self) -> Option<f32> {
        for el in &self.elements {
            if let ModifierElement::LayoutWeight { weight } = el {
                return Some(*weight);
            }
        }
        None
    }

    /// 测试标记（供调试树/UI 测试定位）
    pub fn get_test_tag(&self) -> Option<&str> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::TestTag { tag } = el {
                Some(tag.as_str())
            } else {
                None
            }
        })
    }

    /// 宽高比约束（ratio, match_height_first）
    pub fn aspect_ratio_constraint(&self) -> Option<(f32, bool)> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::AspectRatio { ratio, match_height_first } = el {
                Some((*ratio, *match_height_first))
            } else {
                None
            }
        })
    }

    /// 强制尺寸（单轴 None = 未约束）
    pub fn required_size_constraint(&self) -> Option<(Option<f32>, Option<f32>)> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::RequiredSize { width, height } = el {
                Some((*width, *height))
            } else {
                None
            }
        })
    }

    /// 交叉轴对齐覆盖（供 Column/Row 使用）
    pub fn get_align_self(&self) -> Option<crate::layout::Alignment> {
        for el in &self.elements {
            if let ModifierElement::AlignSelf { alignment } = el {
                return Some(*alignment);
            }
        }
        None
    }

    /// 焦点请求器 ID
    pub fn focus_requester_id(&self) -> Option<u64> {
        for el in &self.elements {
            if let ModifierElement::FocusRequesterId { id } = el {
                return Some(*id);
            }
        }
        None
    }

    /// 点击回调（如果有 Clickable modifier）
    pub fn on_click(&self) -> Option<&Arc<dyn Fn() + Send + Sync>> {
        for el in &self.elements {
            if let ModifierElement::Clickable { on_click, .. } = el {
                return Some(on_click);
            }
        }
        None
    }

    /// Clickable 绑定的交互源（无则 None）
    pub fn clickable_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Clickable { interaction: Some(s), .. } = el {
                Some(s)
            } else {
                None
            }
        })
    }

    /// Focusable 绑定的交互源（无则 None）
    pub fn focusable_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Focusable { interaction: Some(s) } = el {
                Some(s)
            } else {
                None
            }
        })
    }

    /// Hoverable 绑定的交互源
    pub fn hoverable_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Hoverable { interaction } = el {
                Some(interaction)
            } else {
                None
            }
        })
    }

    /// Ripple 绑定的交互源（首个 Ripple 元素——Switch 的 Handle 容器等
    /// ripple 与 clickable 不同节点时，按压坐标要换算到该节点本地空间）
    pub fn ripple_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Ripple { source, .. } = el {
                Some(source)
            } else {
                None
            }
        })
    }

    /// 是否声明了悬停交互（hover 路由判定）
    pub fn has_hoverable(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::Hoverable { .. }))
    }

    /// 是否声明了任意手势回调（tap/drag 系列）——app.rs 手势路由判定用
    pub fn has_gesture(&self) -> bool {
        self.elements.iter().any(|el| matches!(
            el,
            ModifierElement::TapOnTap { .. }
                | ModifierElement::TapOnDoubleTap { .. }
                | ModifierElement::TapOnLongPress { .. }
                | ModifierElement::TapOnPress { .. }
                | ModifierElement::DragOnStart { .. }
                | ModifierElement::DragOnMove { .. }
                | ModifierElement::DragOnEnd { .. }
                | ModifierElement::DragOnCancel { .. }
        ))
    }

    /// 是否声明了拖拽回调（决定 slop 后走 drag 还是取消 tap）
    pub fn has_drag_gesture(&self) -> bool {
        self.elements.iter().any(|el| matches!(
            el,
            ModifierElement::DragOnStart { .. }
                | ModifierElement::DragOnMove { .. }
                | ModifierElement::DragOnEnd { .. }
                | ModifierElement::DragOnCancel { .. }
        ))
    }

    /// 是否声明了双击回调——决定 onTap 是否延迟到双击窗口结束
    /// （Compose detectTapGestures：onDoubleTap 存在时 onTap 延迟触发，
    /// 窗口内第二次按下同节点 → 取消；超时 → 补发）
    pub fn has_double_tap(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::TapOnDoubleTap { .. }))
    }

    pub fn graphics_layer_params(&self) -> Option<GraphicsLayerParams> {
        let mut merged = None;
        for el in self.elements.iter() {
            let ModifierElement::GraphicsLayer { params_fn } = el else { continue };
            let next = (params_fn)();
            if let Some(current) = &mut merged {
                merge_graphics_params(current, next);
            } else {
                merged = Some(next);
            }
        }
        merged
    }

    /// 背景模糊半径（渲染期在节点内容绘制前即时处理）
    pub fn backdrop_blur_radius(&self) -> Option<f32> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::BackdropBlur { radius } = el { Some(*radius) } else { None }
        })
    }

    /// 获取文本对齐方式
    pub fn align(&self) -> Option<crate::ui::TextAlign> {
        for el in &self.elements {
            if let ModifierElement::TextContent { align, .. } = el {
                return Some(*align);
            }
        }
        None
    }

    /// 获取文本内容字节长度（Text / RichText）
    pub fn content_len(&self) -> usize {
        for el in &self.elements {
            if let ModifierElement::TextContent { content, .. } = el {
                return content.len();
            }
            if let ModifierElement::RichTextContent { content, .. } = el {
                return content.len();
            }
        }
        0
    }

    /// 位置偏移（如果有 Offset modifier）
    /// 普通偏移（RTL 下 x 镜像）——动态值在此求值（measure 期读 State 注册布局依赖）
    pub fn get_offset(&self) -> Option<(f32, f32)> {
        let resolve = |sv: &SizeValue| -> f32 {
            match sv {
                SizeValue::Static(Dimension::Fixed(v)) | SizeValue::Static(Dimension::Dp(crate::unit::Dp(v))) => *v,
                SizeValue::Static(Dimension::Px(p)) => p.to_logical(crate::unit::current_density()),
                SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => 0.0,
                SizeValue::Dynamic(f) => f(),
            }
        };
        for el in &self.elements {
            if let ModifierElement::Offset { x, y } = el {
                return Some((resolve(x), resolve(y)));
            }
        }
        None
    }

    /// 绝对偏移（RTL 下不镜像）——动态值求值同 `get_offset`
    pub fn get_absolute_offset(&self) -> Option<(f32, f32)> {
        let resolve = |sv: &SizeValue| -> f32 {
            match sv {
                SizeValue::Static(Dimension::Fixed(v)) | SizeValue::Static(Dimension::Dp(crate::unit::Dp(v))) => *v,
                SizeValue::Static(Dimension::Px(p)) => p.to_logical(crate::unit::current_density()),
                SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => 0.0,
                SizeValue::Dynamic(f) => f(),
            }
        };
        for el in &self.elements {
            if let ModifierElement::AbsoluteOffset { x, y } = el {
                return Some((resolve(x), resolve(y)));
            }
        }
        None
    }
}

// ── 辅助方法: 分类提取 ──

impl Modifier {
    /// Compose 阶段自动注册所有 ModifierElement 中引用的 State 依赖。
    /// 新增包含 State<T> 的 ModifierElement 变体时，必须在此方法中加对应分支。
    pub(crate) fn register_state_deps(&self) {
        for el in &self.elements {
            match el {
                ModifierElement::VerticalScroll { state } => { let _ = state.offset.get(); }
                ModifierElement::HorizontalScroll { state } => { let _ = state.offset.get(); }
                _ => {}
            }
        }
    }
}

impl Debug for ModifierElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size { width, height } => f.debug_struct("Size").field("width", width).field("height", height).finish(),
            Self::MinWidth { value } => f.debug_struct("MinWidth").field("value", value).finish(),
            Self::MinHeight { value } => f.debug_struct("MinHeight").field("value", value).finish(),
            Self::PaddingSides { start, top, end, bottom } => f
                .debug_struct("PaddingSides")
                .field("start", start)
                .field("top", top)
                .field("end", end)
                .field("bottom", bottom)
                .finish(),
            Self::FillMaxWidth => f.write_str("FillMaxWidth"),
            Self::FillMaxHeight => f.write_str("FillMaxHeight"),
            Self::FillMaxSize => f.write_str("FillMaxSize"),
            Self::Offset { x, y } => f.debug_struct("Offset").field("x", x).field("y", y).finish(),
            Self::AbsoluteOffset { x, y } => f.debug_struct("AbsoluteOffset").field("x", x).field("y", y).finish(),
            Self::AlignSelf { alignment } => f.debug_struct("AlignSelf").field("alignment", alignment).finish(),
            Self::LayoutWeight { weight } => f.debug_struct("LayoutWeight").field("weight", weight).finish(),
            Self::AspectRatio { ratio, .. } => f.debug_struct("AspectRatio").field("ratio", ratio).finish(),
            Self::RequiredSize { width, height } => f
                .debug_struct("RequiredSize")
                .field("width", width)
                .field("height", height)
                .finish(),
            Self::TestTag { tag } => f.debug_struct("TestTag").field("tag", tag).finish(),
            Self::LayoutDirection(d) => f.debug_tuple("LayoutDirection").field(d).finish(),
            Self::TextFieldSlot { role } => f.debug_struct("TextFieldSlot").field("role", role).finish(),
            Self::Shadow { params, .. } => f.debug_struct("Shadow").field("radius", &params.radius).field("spread", &params.spread).finish(),
            Self::Background { .. } => f.debug_struct("Background").finish(),
            Self::Border { width, color, shape } => f.debug_struct("Border").field("width", width).field("color", color).field("shape", shape).finish(),
            Self::BorderDynamic { width, shape, .. } => f
                .debug_struct("BorderDynamic")
                .field("width", width)
                .field("shape", shape)
                .finish(),
            Self::Clip { shape } => f.debug_struct("Clip").field("shape", shape).finish(),
            Self::TextContent { content, font_size, .. } => f
                .debug_struct("TextContent")
                .field("content", content)
                .field("font_size", font_size)
                .finish(),
            Self::RichTextContent { content, drawable_ranges, .. } => f
                .debug_struct("RichTextContent")
                .field("content", content)
                .field("drawables", &format_args!("{} drawables", drawable_ranges.len()))
                .finish(),
            Self::Clickable { .. } => f.write_str("Clickable(<fn>)"),
            Self::TapOnTap { .. } => f.write_str("TapOnTap(<fn>)"),
            Self::TapOnDoubleTap { .. } => f.write_str("TapOnDoubleTap(<fn>)"),
            Self::TapOnLongPress { .. } => f.write_str("TapOnLongPress(<fn>)"),
            Self::TapOnPress { .. } => f.write_str("TapOnPress(<fn>)"),
            Self::DragOnStart { .. } => f.write_str("DragOnStart(<fn>)"),
            Self::DragOnMove { .. } => f.write_str("DragOnMove(<fn>)"),
            Self::DragOnEnd { .. } => f.write_str("DragOnEnd(<fn>)"),
            Self::DragOnCancel { .. } => f.write_str("DragOnCancel(<fn>)"),
            Self::Focusable { .. } => f.write_str("Focusable"),
            Self::Hoverable { .. } => f.write_str("Hoverable"),
            Self::Ripple { color, bounded, .. } => f
                .debug_struct("Ripple")
                .field("color", color)
                .field("bounded", bounded)
                .finish(),
            Self::CustomDraw { .. } => f.write_str("CustomDraw"),
Self::NoFocusRing => f.write_str("NoFocusRing"),
Self::DrawIcon { .. } => f.write_str("DrawIcon"),
            Self::ImageContent { content_scale, alignment, alpha, color_filter, filter_quality, .. } => f
                .debug_struct("ImageContent")
                .field("scale", content_scale)
                .field("align", alignment)
                .field("alpha", alpha)
                .field("color_filter", color_filter)
                .field("filter_quality", filter_quality)
                .finish(),
            Self::KbEvent { on_key, on_pre_key } => f.debug_struct("KbEvent").field("on_key", &on_key.is_some()).field("on_pre_key", &on_pre_key.is_some()).finish(),
            Self::PointerEvent { on_ptr, on_pre_ptr } => f.debug_struct("PointerEvent").field("on_ptr", &on_ptr.is_some()).field("on_pre_ptr", &on_pre_ptr.is_some()).finish(),
            Self::FocusRequesterId { id } => f.debug_tuple("FocusRequesterId").field(id).finish(),
            Self::VerticalScroll { .. } => f.write_str("VerticalScroll(<state>)"),
            Self::LazyScroll { .. } => f.write_str("LazyScroll(<state>)"),
            Self::HorizontalScroll { .. } => f.write_str("HorizontalScroll(<state>)"),
            Self::GraphicsLayer { .. } => f.debug_struct("GraphicsLayer").finish(),
            Self::Blur { radius } => f.debug_struct("Blur").field("radius", radius).finish(),
            Self::BackdropBlur { radius } => f.debug_struct("BackdropBlur").field("radius", radius).finish(),
            Self::TextFieldVisual { variant, .. } => f.debug_struct("TextFieldVisual").field("variant", variant).finish(),
            Self::TextFieldOffsetMapping { .. } => f.write_str("TextFieldOffsetMapping"),
        }
    }
}

// ── ScrollState ──

// Skia's native shadow utility consumes the alpha directly. These values match
// the low-opacity ambient/spot defaults used by Skia's shadow examples.
const DEFAULT_AMBIENT_SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, a: 0x19 };
const DEFAULT_SPOT_SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, a: 0x40 };

/// 图形层变换参数
///
/// ⚠ 只影响**绘制**（外观），不参与布局与命中测试（对标 Compose
/// graphicsLayer：命中区域始终是布局 bounds）。命中测试唯一考虑的
/// 位移是 scroll（布局层）；此处变换（translation/scale/rotate/
/// rotationX/Y/camera）不会改变可点击区域或按压点本地坐标。
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsLayerParams {
    pub scale_x: f32,
    pub scale_y: f32,
    pub alpha: f32,
    pub translation_x: f32,
    pub translation_y: f32,
    pub rotation_z: f32,
    /// 变换原点（pivot 分数——0..1，相对节点宽高）——对标 Compose
    /// `transformOrigin`（默认 Center——scale/rotate 绕中心）
    pub transform_origin: TransformOrigin,
    /// 裁剪到节点 bounds（对标 Compose graphicsLayer `clip`；
    /// `Modifier.alpha` 便捷版默认 clip=true）
    pub clip: bool,
    /// 绕 X 轴 3D 旋转（度——带 cameraDistance 透视）
    pub rotation_x: f32,
    /// 绕 Y 轴 3D 旋转（度——带 cameraDistance 透视）
    pub rotation_y: f32,
    /// 3D 相机距离（逻辑 px——越大透视越平；Compose 默认 8.dp）
    pub camera_distance: f32,
    /// 图层阴影高度（逻辑 px——>0 时由 Skia ShadowUtils 绘制 ambient+spot 阴影，
    /// 对标 Compose graphicsLayer.shadowElevation）
    pub shadow_elevation: f32,
    /// 图层阴影形状（None = 矩形）
    pub shadow_shape: Option<Shape>,
    /// 环境光阴影颜色（默认约 10% 黑，对标 Compose ambientShadowColor）。
    pub ambient_shadow_color: Color,
    /// 投射光阴影颜色（默认约 25% 黑，对标 Compose spotShadowColor）。
    pub spot_shadow_color: Color,
}

impl Default for GraphicsLayerParams {
    fn default() -> Self {
        Self {
            scale_x: 1.0, scale_y: 1.0, alpha: 1.0,
            translation_x: 0.0, translation_y: 0.0, rotation_z: 0.0,
            transform_origin: TransformOrigin::CENTER,
            clip: false,
            rotation_x: 0.0, rotation_y: 0.0,
            camera_distance: 8.0,
            shadow_elevation: 0.0,
            shadow_shape: None,
            ambient_shadow_color: DEFAULT_AMBIENT_SHADOW_COLOR,
            spot_shadow_color: DEFAULT_SPOT_SHADOW_COLOR,
        }
    }
}

/// 变换原点（对标 Compose `TransformOrigin`）——pivot 分数坐标，
/// 相对节点宽高（0.0 = 左/上，0.5 = 中心，1.0 = 右/下）
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransformOrigin(pub f32, pub f32);

/// TextField 支持文本绘制参数（画在容器底部外侧 4dp）
#[derive(Clone, Debug)]
pub struct SupportingVisual {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
}

/// 阴影参数（对标 Compose `graphics.shadow.Shadow`——dropShadow 可配置集）。
/// 绘制对齐 DropShadowPainter：扩边画布 → 形状路径（模糊）画进离屏 mask →
/// 颜色 SrcIn 着色 → 按 offset 平移到画布。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShadowParams {
    /// 模糊半径（逻辑 px，对标 radius）
    pub radius: f32,
    /// 扩展半径（阴影比形状大多少——超出部分另画 stroke，对标 spread）
    pub spread: f32,
    /// 阴影偏移（对标 offset）
    pub offset_x: f32,
    pub offset_y: f32,
    /// 阴影颜色（对标 color，默认黑）
    pub color: Color,
    /// 独立透明度 0-1（对标 alpha）
    pub alpha: f32,
}

impl ShadowParams {
    /// 便捷构造（radius/offset/color/alpha；spread=0）
    pub fn new(radius: f32, offset_x: f32, offset_y: f32, color: Color, alpha: f32) -> Self {
        Self { radius, spread: 0.0, offset_x, offset_y, color, alpha }
    }
}

impl Default for ShadowParams {
    fn default() -> Self {
        Self {
            radius: 0.0,
            spread: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            color: Color::from_argb(255, 0, 0, 0),
            alpha: 1.0,
        }
    }
}

impl TransformOrigin {
    /// 中心（Compose 默认）
    pub const CENTER: Self = Self(0.5, 0.5);
    /// 左上角
    pub const TOP_LEFT: Self = Self(0.0, 0.0);
    /// 右下角
    pub const BOTTOM_RIGHT: Self = Self(1.0, 1.0);
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self::CENTER
    }
}

/// 背景色规格：静态 `Color` 或动态闭包（渲染时每帧求值）。
/// 通过 `impl Into<BackgroundColor>` 统一 `background()` 入口——传 `Color` 或闭包均可。
pub struct BackgroundColor(pub(crate) Arc<dyn Fn() -> Color + Send + Sync>);

impl From<Color> for BackgroundColor {
    fn from(color: Color) -> Self {
        Self(Arc::new(move || color))
    }
}

impl<F: Fn() -> Color + Send + Sync + 'static> From<F> for BackgroundColor {
    fn from(f: F) -> Self {
        Self(Arc::new(f))
    }
}

impl From<crate::core::state::DerivedValue<Color>> for BackgroundColor {
    fn from(d: crate::core::state::DerivedValue<Color>) -> Self {
        Self(Arc::new(move || d.get()))
    }
}

impl From<&crate::core::state::DerivedValue<Color>> for BackgroundColor {
    fn from(d: &crate::core::state::DerivedValue<Color>) -> Self {
        let d = d.clone();
        Self(Arc::new(move || d.get()))
    }
}

/// 图形层规格：静态 `GraphicsLayerParams` 或动态闭包（渲染时每帧求值）。
/// 通过 `impl Into<GraphicsLayerSpec>` 统一 `graphics_layer()` 入口。
pub struct GraphicsLayerSpec(pub(crate) Arc<dyn Fn() -> GraphicsLayerParams + Send + Sync>);

impl From<GraphicsLayerParams> for GraphicsLayerSpec {
    fn from(params: GraphicsLayerParams) -> Self {
        Self(Arc::new(move || params.clone()))
    }
}

impl<F: Fn() -> GraphicsLayerParams + Send + Sync + 'static> From<F> for GraphicsLayerSpec {
    fn from(f: F) -> Self {
        Self(Arc::new(f))
    }
}

/// 滚动状态，对齐 Compose ScrollState
#[derive(Debug, Clone)]
pub struct ScrollState {
    /// 当前偏移
    pub offset: crate::core::state::State<f32>,
    /// 是否正在滚动
    pub is_scroll_in_progress: crate::core::state::State<bool>,
    /// fling 滚动极限（布局期回写 = 内容高 - 视口高；0 = 未知 → fling 只拦下限）
    pub(crate) fling_limit: crate::core::state::State<f32>,
}

impl ScrollState {
    pub fn new() -> Self {
        ScrollState {
            offset: crate::core::state::State::new(0.0),
            is_scroll_in_progress: crate::core::state::State::new(false),
            fling_limit: crate::core::state::State::new(0.0),
        }
    }

    /// 立即滚动到指定位置
    pub fn scroll_to(&self, value: f32, max_offset: f32) {
        self.offset.set(value.clamp(0.0, max_offset));
    }

    /// 惯性滚动（对标 Compose flingBehavior）：以 `velocity`(px/s) 启动指数衰减
    /// 滚动，撞到滚动极限立即停止（极限由布局期回写——LazyColumn 精确值、
    /// 普通容器由 measure_node 计算）。手动滚动（wheel/拖拽）自动取消进行中
    /// 的 fling（apply_scroll_delta 内 cancel）。滚动期间 `is_scroll_in_progress`
    /// 为 true，动画结束回 false。
    pub fn fling(&self, velocity: f32) {
        if !velocity.is_finite() || velocity.abs() < 1.0 {
            return;
        }
        self.is_scroll_in_progress.set(true);
        let off = self.offset.clone();
        let limit = self.fling_limit.clone();
        let done_flag = self.is_scroll_in_progress.clone();
        crate::animation::push_fling(
            off,
            velocity,
            crate::animation::exponential_decay(4.2),
            move |o| {
                let max = limit.get();
                let max = if max > 0.0 { max } else { f32::MAX };
                o.clamp(0.0, max)
            },
            move || {
                done_flag.set(false);
            },
        );
    }

    /// 取消进行中的惯性滚动（手动输入/程序化跳转前调用）
    pub fn cancel_fling(&self) {
        crate::animation::cancel_animation(&self.offset);
    }
}

impl Default for ScrollState {
    fn default() -> Self { Self::new() }
}

// ── FocusRequester ──

static NEXT_FOCUS_ID: AtomicU64 = AtomicU64::new(1);
static FOCUS_REQUESTS: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

/// 消费所有排队的焦点请求（供 app.rs RedrawRequested 调用）
pub(crate) fn take_focus_requests() -> Vec<u64> {
    std::mem::take(&mut *FOCUS_REQUESTS.lock().unwrap())
}

/// 焦点请求器——可在代码中调用 request_focus() 让关联组件获得焦点
#[derive(Debug, Clone)]
pub struct FocusRequester {
    id: u64,
}

impl FocusRequester {
    pub fn new() -> Self {
        FocusRequester { id: NEXT_FOCUS_ID.fetch_add(1, Ordering::Relaxed) }
    }

    pub fn id(&self) -> u64 { self.id }

    /// 请求焦点。无论是否启用 debug-server，都生效。
    /// 焦点将在下一帧 RedrawRequested 时应用。
    pub fn request_focus(&self) {
        FOCUS_REQUESTS.lock().unwrap().push(self.id);
        // 同时走 debug 通道（兼容旧行为）
        #[cfg(feature = "debug-server")]
        crate::debug::queue_event(crate::debug::DebugEvent::RequestFocus { id: self.id });
    }
}

impl Default for FocusRequester {
    fn default() -> Self { Self::new() }
}

impl From<&FocusRequester> for FocusRequester {
    fn from(fr: &FocusRequester) -> Self { fr.clone() }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_modifier() {
        let m = Modifier::new();
        assert_eq!(m.elements().len(), 0);
    }

    #[test]
    fn test_chain_syntax() {
        let m = Modifier::new()
            .size(100.0, 50.0)
            .padding(8.0)
            .background(Color::RED, Shape::rounded(4.0))
            .clickable(|| println!("clicked"));

        assert_eq!(m.elements().len(), 4);
    }

    #[test]
    fn test_padding_sides_queries() {
        // 非对称 padding：四边独立
        let m = Modifier::new().padding_sides(2.0, 4.0, 6.0, 8.0);
        assert_eq!(m.get_padding_sides(), (2.0, 4.0, 6.0, 8.0));
        assert_eq!(m.get_padding_horizontal(), (2.0, 6.0));
        assert_eq!(m.get_padding_vertical(), (4.0, 8.0));
        assert_eq!(m.get_padding_values(), (8.0, 12.0));

        // 与等距/对称 padding 叠加（累积）
        let m = Modifier::new()
            .padding(1.0)
            .padding_horizontal(10.0)
            .padding_vertical(20.0)
            .padding_sides(2.0, 4.0, 6.0, 8.0);
        assert_eq!(m.get_padding_sides(), (13.0, 25.0, 17.0, 29.0));
    }

    #[test]
    fn test_padding_single_sides() {
        // 单边方法：只影响对应边
        let m = Modifier::new()
            .padding_start(3.0)
            .padding_end(5.0)
            .padding_top(7.0)
            .padding_bottom(9.0);
        assert_eq!(m.get_padding_sides(), (3.0, 7.0, 5.0, 9.0));
    }

    #[test]
    fn test_padding_dynamic_value() {
        // 动态 padding：State 驱动（动画作用于 padding 的机制）
        let s = crate::core::state::State::new(4.0f32);
        let m = Modifier::new().padding_start(s.clone());
        let (start, _, _, _) = m.get_padding_sides();
        assert_eq!(start, 4.0);
        s.set(20.0);
        let (start, _, _, _) = m.get_padding_sides();
        assert_eq!(start, 20.0, "动态 padding 重新求值");
    }

    #[test]
    fn test_offset_basic_and_single_axis() {
        // 全参 + 单轴
        let m = Modifier::new().offset(10.0, 20.0);
        assert_eq!(m.get_offset(), Some((10.0, 20.0)));
        let m = Modifier::new().offset_x(5.0);
        assert_eq!(m.get_offset(), Some((5.0, 0.0)));
        let m = Modifier::new().offset_y(7.0);
        assert_eq!(m.get_offset(), Some((0.0, 7.0)));
        // 无 offset → None
        assert_eq!(Modifier::new().get_offset(), None);
    }

    #[test]
    fn test_offset_dynamic_animation() {
        // 动态 offset：State 驱动（动画作用于 offset——Compose offset 动画语义）
        let s = crate::core::state::State::new(0.0f32);
        let m = Modifier::new().offset(s.clone(), 10.0);
        assert_eq!(m.get_offset(), Some((0.0, 10.0)));
        s.set(50.0);
        assert_eq!(m.get_offset(), Some((50.0, 10.0)), "动态 offset 重新求值");
    }

    #[test]
    fn test_absolute_offset() {
        // absolute_offset：单独查询，与普通 offset 互不影响
        let m = Modifier::new().absolute_offset(3.0, 4.0);
        assert_eq!(m.get_offset(), None, "absolute 不进普通 offset 查询");
        assert_eq!(m.get_absolute_offset(), Some((3.0, 4.0)));
        let m = Modifier::new().absolute_offset_x(9.0);
        assert_eq!(m.get_absolute_offset(), Some((9.0, 0.0)));
        // 两者共存：各查各的
        let m = Modifier::new().offset(1.0, 2.0).absolute_offset(3.0, 4.0);
        assert_eq!(m.get_offset(), Some((1.0, 2.0)));
        assert_eq!(m.get_absolute_offset(), Some((3.0, 4.0)));
    }

    #[test]
    fn test_dimension_conversions() {
        let m = Modifier::new()
            .size(100.0, Dimension::Fill) // f32 → Dimension::Fixed, Dimension::Fill
            .width(Dimension::Auto)
            .height(50.0);

        let elements = m.elements();
        assert_eq!(elements.len(), 3);
        // 第一个是 Size { width: Fixed(100), height: Fill }
        match &elements[0] {
            ModifierElement::Size { width, height } => {
                match (width, height) {
                    (crate::modifier::SizeValue::Static(w), crate::modifier::SizeValue::Static(h)) => {
                        assert_eq!(*w, Dimension::Fixed(100.0));
                        assert_eq!(*h, Dimension::Fill);
                    }
                    _ => panic!("expected static size"),
                }
            }
            _ => panic!("expected Size"),
        }
    }

    #[test]
    fn test_fill_max() {
        let m = Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .fill_max_size();

        assert_eq!(m.elements().len(), 3);
    }

    #[test]
    fn test_modifier_immutable() {
        let a = Modifier::new().size(100.0, 50.0);
        let b = a.clone().padding(8.0);

        // a 不变
        assert_eq!(a.elements().len(), 1);
        // b 追加了
        assert_eq!(b.elements().len(), 2);
    }

    #[test]
    fn test_border_and_focusable() {
        let m = Modifier::new()
            .border(2.0, Color::BLUE, Shape::rounded(8.0))
            .focusable();

        assert_eq!(m.elements().len(), 2);
    }
}

// ── RichSpanStyle ──

/// 装饰线样式（对应 Skia TextDecorationStyle）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoStyle { Solid, Double, Dotted, Dashed, Wavy }

/// 装饰线模式（对应 Skia TextDecorationMode）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoMode { Gaps, Through }

/// 字体渲染边缘
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontEdge { Alias, AntiAlias, SubpixelAntiAlias }

/// 字体提示
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontHint { None, Slight, Normal, Full }

/// 富文本中每段的已解析样式（含范围）。
/// 存储在 RichTextContent modifier 中供测量/渲染使用。
#[derive(Debug, Clone, PartialEq)]
pub struct RichSpanStyle {
    /// 范围起（字符索引，含）
    pub start: usize,
    /// 范围止（字符索引，不含）
    pub end: usize,
    pub font_size: f32,
    pub color: Color,
    pub font_weight: crate::ui::text::FontWeight,
    pub font_style: crate::ui::text::FontSlant,
    // ── 装饰线 ──
    pub underline: bool,
    pub overline: bool,
    pub strikethrough: bool,
    pub decoration_color: Option<Color>,
    pub decoration_style: Option<DecoStyle>,
    pub decoration_mode: Option<DecoMode>,
    // ── 基线 ──
    pub baseline_shift: f32,
    // ── 间距 ──
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub height_multiple: f32,
    pub half_leading: bool,
    // ── 字体 ──
    pub font_families: Vec<String>,
    pub font_width: i32,
    pub font_edging: Option<FontEdge>,
    pub font_hinting: Option<FontHint>,
    pub subpixel: bool,
    // ── 前景/背景 ──
    pub foreground_color: Option<Color>,
    pub background: Option<Color>,
    // ── 其他 ──
    pub locale: Option<String>,
}

// ── 参数相等性（Skip 判定） ──

impl Modifier {
    /// 参数相等性（start_restartable_group 的 Skip 判定用）——可比较元素
    /// （数值/枚举/字符串/颜色）精确比较；**闭包类元素**（背景色/点击回调/
    /// 图形层动态参数/滚动状态/富文本样式）**视为相同**——每次 build 重建的
    /// 闭包无法比较，精确比会破坏 Skip（列表每行永不 Skip）。
    ///
    /// 近似语义：modifier 的**数值参数**变化（width/padding/颜色等）→ 不等 →
    /// Enter（content 重跑）；闭包参数变化（回调重设）→ 相同 → Skip（保持）。
    /// 注意：动画驱动的动态尺寸（SizeValue::Dynamic）视为相同——布局期
    /// layout_dep 每帧重测已覆盖，无需 Enter。
    pub(crate) fn param_eq(&self, other: &Modifier) -> bool {
        if self.elements.len() != other.elements.len() {
            return false;
        }
        self.elements
            .iter()
            .zip(&other.elements)
            .all(|(a, b)| element_param_eq(a, b))
    }
}

fn element_param_eq(a: &ModifierElement, b: &ModifierElement) -> bool {
    use ModifierElement::*;
    match (a, b) {
        (Size { width: aw, height: ah }, Size { width: bw, height: bh }) => {
            size_value_eq(aw, bw) && size_value_eq(ah, bh)
        }
        (MinWidth { value: av }, MinWidth { value: bv }) => size_value_eq(av, bv),
        (MinHeight { value: av }, MinHeight { value: bv }) => size_value_eq(av, bv),
        (
            PaddingSides { start: as_, top: at, end: ae, bottom: ab },
            PaddingSides { start: bs, top: bt, end: be, bottom: bb },
        ) => {
            size_value_eq(as_, bs) && size_value_eq(at, bt) && size_value_eq(ae, be) && size_value_eq(ab, bb)
        }
        (FillMaxWidth, FillMaxWidth) => true,
        (FillMaxHeight, FillMaxHeight) => true,
        (FillMaxSize, FillMaxSize) => true,
        (Offset { x: ax, y: ay }, Offset { x: bx, y: by }) => size_value_eq(ax, bx) && size_value_eq(ay, by),
        (AbsoluteOffset { x: ax, y: ay }, AbsoluteOffset { x: bx, y: by }) => size_value_eq(ax, bx) && size_value_eq(ay, by),
        (AlignSelf { alignment: aa }, AlignSelf { alignment: ba }) => aa == ba,
        (LayoutWeight { weight: aw }, LayoutWeight { weight: bw }) => aw == bw,
        (AspectRatio { ratio: ar, match_height_first: am }, AspectRatio { ratio: br, match_height_first: bm }) => {
            ar == br && am == bm
        }
        (RequiredSize { width: aw, height: ah }, RequiredSize { width: bw, height: bh }) => {
            aw == bw && ah == bh
        }
        (TestTag { tag: at }, TestTag { tag: bt }) => at == bt,
        (LayoutDirection(ad), LayoutDirection(bd)) => ad == bd,
        (Shadow { params: ap, shape: as_, clip: ac }, Shadow { params: bp, shape: bs, clip: bc }) => {
            ap == bp && as_ == bs && ac == bc
        }
        // 背景色闭包视为相同（渲染期求值——动画颜色不触发 Enter）
        (Background { shape: as_, .. }, Background { shape: bs, .. }) => as_ == bs,
        (Border { width: aw, color: ac, shape: as_ }, Border { width: bw, color: bc, shape: bs }) => {
            aw == bw && ac == bc && as_ == bs
        }
        // 动态颜色视为相同（渲染期求值——动画不触发 Enter）
        (BorderDynamic { width: aw, shape: as_, .. }, BorderDynamic { width: bw, shape: bs, .. }) => {
            aw == bw && as_ == bs
        }
        (Clip { shape: as_ }, Clip { shape: bs }) => as_ == bs,
        (Blur { radius: ar }, Blur { radius: br }) => ar == br,
        (BackdropBlur { radius: ar }, BackdropBlur { radius: br }) => ar == br,
        (TextContent { content: ac, font_size: af, color: acol, font_weight: afw, font_style: afs, max_lines: am, align: aa, overflow: ao, soft_wrap: asw, letter_spacing: als, line_height: alh },
         TextContent { content: bc, font_size: bf, color: bcol, font_weight: bfw, font_style: bfs, max_lines: bm, align: ba, overflow: bo, soft_wrap: bsw, letter_spacing: bls, line_height: blh }) => {
            ac == bc && af == bf && acol == bcol && afw == bfw && afs == bfs && am == bm && aa == ba && ao == bo && asw == bsw && als == bls && alh == blh
        }
        // 富文本：内容 + 内联元素数比较；样式范围视为相同（每次 build 重建）
        (RichTextContent { content: ac, drawables: ad, drawable_ranges: ar, .. },
         RichTextContent { content: bc, drawables: bd, drawable_ranges: br, .. }) => {
            ac == bc && ad.len() == bd.len() && ar == br
        }
        // 点击：交互源身份参与判定（换源需重入 content 捕获新源）；回调视为相同
        (Clickable { interaction: ai, .. }, Clickable { interaction: bi, .. }) => ai == bi,
        (TapOnTap { .. }, TapOnTap { .. }) => true,
        (TapOnDoubleTap { .. }, TapOnDoubleTap { .. }) => true,
        (TapOnLongPress { .. }, TapOnLongPress { .. }) => true,
        (TapOnPress { .. }, TapOnPress { .. }) => true,
        (DragOnStart { .. }, DragOnStart { .. }) => true,
        (DragOnMove { .. }, DragOnMove { .. }) => true,
        (DragOnEnd { .. }, DragOnEnd { .. }) => true,
        (DragOnCancel { .. }, DragOnCancel { .. }) => true,
        (Focusable { interaction: ai }, Focusable { interaction: bi }) => ai == bi,
        (Hoverable { interaction: ai }, Hoverable { interaction: bi }) => ai == bi,
        (Ripple { source: as_, color: ac, bounded: abc, shape: ash }, Ripple { source: bs, color: bc, bounded: bbc, shape: bsh }) => {
            as_ == bs && ac == bc && abc == bbc && ash == bsh
        }
        (DrawIcon { spec: a }, DrawIcon { spec: b }) => a == b,
        (ImageContent { source: a, content_scale: as_, alignment: aa, alpha: aal, color_filter: acf, filter_quality: afq },
         ImageContent { source: b, content_scale: bs, alignment: ba, alpha: bal, color_filter: bcf, filter_quality: bfq }) => {
            a == b && as_ == bs && aa == ba && aal == bal && acf == bcf && afq == bfq
        }
        (FocusRequesterId { id: ai }, FocusRequesterId { id: bi }) => ai == bi,
        (KbEvent { .. }, KbEvent { .. }) => true,
        (PointerEvent { .. }, PointerEvent { .. }) => true,
        (VerticalScroll { state: as_ }, VerticalScroll { state: bs }) => as_.offset.id() == bs.offset.id(),
        (HorizontalScroll { state: as_ }, HorizontalScroll { state: bs }) => as_.offset.id() == bs.offset.id(),
        // 图形层动态参数视为相同（渲染期求值——动画不触发 Enter）
        (GraphicsLayer { .. }, GraphicsLayer { .. }) => true,
        _ => false,
    }
}

fn merge_graphics_params(current: &mut GraphicsLayerParams, next: GraphicsLayerParams) {
    current.scale_x *= next.scale_x;
    current.scale_y *= next.scale_y;
    current.alpha *= next.alpha;
    current.translation_x += next.translation_x;
    current.translation_y += next.translation_y;
    current.rotation_z += next.rotation_z;
    current.rotation_x += next.rotation_x;
    current.rotation_y += next.rotation_y;
    if next.camera_distance != GraphicsLayerParams::default().camera_distance {
        current.camera_distance = next.camera_distance;
    }
    if next.transform_origin != TransformOrigin::CENTER {
        current.transform_origin = next.transform_origin;
    }
    current.clip |= next.clip;
    if next.shadow_elevation > 0.0 {
        current.shadow_elevation = next.shadow_elevation;
    }
    if next.shadow_shape.is_some() {
        current.shadow_shape = next.shadow_shape;
    }
    if next.ambient_shadow_color != DEFAULT_AMBIENT_SHADOW_COLOR {
        current.ambient_shadow_color = next.ambient_shadow_color;
    }
    if next.spot_shadow_color != DEFAULT_SPOT_SHADOW_COLOR {
        current.spot_shadow_color = next.spot_shadow_color;
    }
}

fn size_value_eq(a: &SizeValue, b: &SizeValue) -> bool {
    match (a, b) {
        (SizeValue::Static(ad), SizeValue::Static(bd)) => ad == bd,
        // 动态尺寸（动画 State/闭包）视为相同——布局期 layout_dep 已覆盖
        (SizeValue::Dynamic(_), SizeValue::Dynamic(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod param_eq_tests {
    use super::*;

    #[test]
    fn param_eq_detects_size_change() {
        let a = Modifier::new().width(200.0);
        let b = Modifier::new().width(360.0);
        assert!(!a.param_eq(&b), "width 200 vs 360 必须不等");
        let c = Modifier::new().width(200.0);
        assert!(a.param_eq(&c), "width 相同必须相等");
    }

    #[test]
    fn param_eq_ignores_closure_elements() {
        let a = Modifier::new().width(200.0).background(Color::from_argb(255, 66, 133, 244), Shape::rounded(8.0));
        let b = Modifier::new().width(200.0).background(Color::from_argb(255, 76, 175, 80), Shape::rounded(8.0));
        // 背景色闭包视为相同（渲染期求值）——宽度相同 → 相等
        assert!(a.param_eq(&b), "闭包元素（背景色）应视为相同");
    }

    #[test]
    fn param_eq_element_count_mismatch() {
        let a = Modifier::new().width(200.0);
        let b = Modifier::new().width(200.0).padding(4.0);
        assert!(!a.param_eq(&b), "元素数不同必须不等");
    }

    // ── GraphicsLayer 便捷包装（alpha/rotate/scale + transformOrigin）──

    #[test]
    fn default_transform_origin_is_center() {
        // 对标 Compose：transformOrigin 默认 Center（0.5, 0.5）
        let p = GraphicsLayerParams::default();
        assert_eq!(p.transform_origin, TransformOrigin::CENTER);
        assert!(!p.clip, "graphics_layer 本身默认不 clip（alpha 便捷版才 clip）");
        assert_eq!(p.ambient_shadow_color, DEFAULT_AMBIENT_SHADOW_COLOR);
        assert_eq!(p.spot_shadow_color, DEFAULT_SPOT_SHADOW_COLOR);
    }

    #[test]
    fn graphics_layers_fold_and_preserve_user_colors() {
        let custom = GraphicsLayerParams {
            alpha: 0.5,
            translation_x: 4.0,
            ambient_shadow_color: Color::RED,
            spot_shadow_color: Color::BLUE,
            ..Default::default()
        };
        let p = Modifier::new()
            .graphics_layer(GraphicsLayerParams {
                shadow_elevation: 6.0,
                ..Default::default()
            })
            .graphics_layer(custom)
            .graphics_layer(GraphicsLayerParams {
                translation_y: 3.0,
                ..Default::default()
            })
            .graphics_layer_params()
            .unwrap();
        assert_eq!(p.shadow_elevation, 6.0);
        assert_eq!(p.alpha, 0.5);
        assert_eq!(p.translation_x, 4.0);
        assert_eq!(p.translation_y, 3.0);
        assert_eq!(p.ambient_shadow_color, Color::RED);
        assert_eq!(p.spot_shadow_color, Color::BLUE);
    }

    #[test]
    fn alpha_creates_layer_with_clip() {
        // 对标源码：`alpha(a) = graphicsLayer(alpha=a, clip=true)`
        let m = Modifier::new().alpha(0.5);
        let p = m.graphics_layer_params().unwrap();
        assert_eq!(p.alpha, 0.5);
        assert!(p.clip, "alpha<1 必须隐式 clip（Compose 语义）");
    }

    #[test]
    fn alpha_identity_no_op() {
        let m = Modifier::new().alpha(1.0);
        assert!(m.graphics_layer_params().is_none(), "alpha=1 不应用（源码 early-return）");
        let m = Modifier::new().rotate(0.0).scale(1.0, 1.0);
        assert!(m.graphics_layer_params().is_none(), "rotate=0/scale=1 不应用");
    }

    #[test]
    fn alpha_merges_into_existing_layer() {
        // 已存在 GraphicsLayer → 合并（alpha 相乘 + clip）而非嵌套层
        let m = Modifier::new()
            .graphics_layer(GraphicsLayerParams { alpha: 0.8, ..Default::default() })
            .alpha(0.5);
        let p = m.graphics_layer_params().unwrap();
        assert!((p.alpha - 0.4).abs() < 1e-6, "alpha 相乘（0.8*0.5）");
        assert!(p.clip);
    }

    #[test]
    fn rotate_scale_merge_with_origin() {
        let m = Modifier::new().rotate(45.0).scale(2.0, 3.0);
        let p = m.graphics_layer_params().unwrap();
        assert_eq!(p.rotation_z, 45.0);
        assert_eq!(p.scale_x, 2.0);
        assert_eq!(p.scale_y, 3.0);
        // 便捷包装不动 transform_origin（默认 Center——绕中心，Compose 语义）
        assert_eq!(p.transform_origin, TransformOrigin::CENTER);
    }

    #[test]
    fn chain_alpha_rotate_scale_merge() {
        let m = Modifier::new().alpha(0.5).rotate(90.0).scale(2.0, 2.0);
        let p = m.graphics_layer_params().unwrap();
        assert_eq!(p.alpha, 0.5);
        assert_eq!(p.rotation_z, 90.0);
        assert_eq!(p.scale_x, 2.0);
        assert!(p.clip);
    }

    // ── shadow / drop_shadow ──

    #[test]
    fn shadow_expands_to_ambient_and_spot() {
        // elevation 便捷版展开为两层（ambient 无偏移 + spot 偏移 e*0.5）
        let m = Modifier::new().shadow(8.0, Shape::rounded(4.0), true, Color::from_argb(255, 0, 0, 0));
        let layers: Vec<_> = m.elements().iter().filter_map(|el| {
            if let ModifierElement::Shadow { params, .. } = el {
                Some((params.radius, params.offset_x, params.offset_y, params.alpha))
            } else {
                None
            }
        }).collect();
        assert_eq!(layers.len(), 2, "shadow() 展开 ambient + spot 两层");
        let (r0, _, oy0, a0) = layers[0];
        let (r1, _, oy1, a1) = layers[1];
        // ambient：无偏移、模糊半径 = e（SkShadowUtils）、低 alpha
        assert_eq!(oy0, 0.0);
        assert!((r0 - 8.0).abs() < 1e-6, "ambient 模糊半径 = e");
        assert!(a0 < a1, "ambient alpha 低于 spot");
        // spot：偏移 e*0.5、模糊半径 e*0.25（SkShadowUtils）
        assert!((oy1 - 4.0).abs() < 1e-6, "spot 偏移 e*0.5");
        assert!((r1 - 2.0).abs() < 1e-6, "spot 模糊半径 e*0.25");
        // strength = 8/12 → ambient 0.18×0.667、spot 0.30×0.667
        assert!((a0 - 0.18 * 8.0 / 12.0).abs() < 1e-3);
        assert!((a1 - 0.30 * 8.0 / 12.0).abs() < 1e-3);
    }

    #[test]
    fn drop_shadow_custom_params() {
        let m = Modifier::new().drop_shadow(
            Shape::rounded(8.0),
            ShadowParams { radius: 6.0, spread: 2.0, offset_x: 3.0, offset_y: 5.0, color: Color::from_argb(255, 50, 50, 50), alpha: 0.6 },
        );
        let p = m.elements().iter().find_map(|el| {
            if let ModifierElement::Shadow { params, .. } = el { Some(*params) } else { None }
        }).unwrap();
        assert_eq!(p.radius, 6.0);
        assert_eq!(p.spread, 2.0);
        assert_eq!(p.offset_x, 3.0);
        assert_eq!(p.offset_y, 5.0);
        assert_eq!(p.alpha, 0.6);
    }

    #[test]
    fn multiple_drop_shadows_stack() {
        let m = Modifier::new()
            .drop_shadow(Shape::Rectangle, ShadowParams::new(2.0, 0.0, 1.0, Color::BLACK, 0.3))
            .drop_shadow(Shape::Rectangle, ShadowParams::new(4.0, 0.0, 2.0, Color::BLACK, 0.2));
        let n = m.elements().iter().filter(|el| matches!(el, ModifierElement::Shadow { .. })).count();
        assert_eq!(n, 2, "多个 drop_shadow 叠加（Compose vararg 语义）");
    }
}
