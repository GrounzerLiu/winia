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
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// 矩形（可带圆角）
    RoundedRect { corner_radius: f32 },
    /// 圆形
    Circle,
    /// 直角矩形
    Rectangle,
}

impl Shape {
    pub fn rounded(corner_radius: f32) -> Self {
        Shape::RoundedRect { corner_radius }
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
    /// 全方向 padding
    Padding { all: f32 },
    /// 水平 padding
    PaddingHorizontal { value: f32 },
    /// 垂直 padding
    PaddingVertical { value: f32 },
    /// 填满最大宽度
    FillMaxWidth,
    /// 填满最大高度
    FillMaxHeight,
    /// 填满最大尺寸
    FillMaxSize,
    /// 位置偏移（不影响布局尺寸，仅移动绘制位置）
    Offset { x: f32, y: f32 },
    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    AlignSelf { alignment: crate::layout::Alignment },
    /// 布局权重（Row 中分配宽度，Column 中分配高度）
    LayoutWeight { weight: f32 },

    // ── Draw 类 ──
    /// 背景色 + 形状（color_fn 渲染时求值——静态色或动画闭包统一为闭包）
    Background { color_fn: Arc<dyn Fn() -> Color + Send + Sync>, shape: Shape },
    /// 边框
    Border { width: f32, color: Color, shape: Shape },
    /// 裁剪
    Clip { shape: Shape },
    /// 内容模糊（GPU 原生，零回读）
    Blur { radius: f32 },
    /// 背景模糊（毛玻璃，单 snapshot 多节点共享）
    BackdropBlur { radius: f32 },

    // ── Content 类 ──
    /// 文本内容（由 Text 组件设置，渲染阶段消费）
    TextContent { content: String, font_size: f32, color: Color, font_weight: crate::ui::text::FontWeight, font_style: crate::ui::text::FontSlant, max_lines: usize, align: crate::ui::TextAlign, overflow: crate::ui::TextOverflow, soft_wrap: bool },
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
    Clickable { on_click: Arc<dyn Fn() + Send + Sync> },
    /// 可获得焦点
    Focusable,
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
    VerticalScroll { state: crate::core::state::State<f32> },
    /// 水平滚动
    HorizontalScroll { state: crate::core::state::State<f32> },
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

    /// 仅设置宽度
    pub fn width(self, w: impl Into<Dimension>) -> Self {
        // 使用 Auto 占位高度，表示不约束
        self.push(ModifierElement::Size {
            width: SizeValue::Static(w.into()),
            height: SizeValue::Static(Dimension::Auto),
        })
    }

    /// 仅设置高度
    pub fn height(self, h: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::Size {
            width: SizeValue::Static(Dimension::Auto),
            height: SizeValue::Static(h.into()),
        })
    }

    /// 四边等距 padding
    pub fn padding(self, all: f32) -> Self {
        self.push(ModifierElement::Padding { all })
    }

    /// 水平方向 padding
    pub fn padding_horizontal(self, value: f32) -> Self {
        self.push(ModifierElement::PaddingHorizontal { value })
    }

    /// 垂直方向 padding
    pub fn padding_vertical(self, value: f32) -> Self {
        self.push(ModifierElement::PaddingVertical { value })
    }

    /// 宽度填满可用空间
    pub fn fill_max_width(self) -> Self {
        self.push(ModifierElement::FillMaxWidth)
    }

    /// 高度填满可用空间
    pub fn fill_max_height(self) -> Self {
        self.push(ModifierElement::FillMaxHeight)
    }

    /// 宽高填满可用空间
    pub fn fill_max_size(self) -> Self {
        self.push(ModifierElement::FillMaxSize)
    }

    /// 位置偏移（不影响布局尺寸，仅移动绘制位置）
    pub fn offset(self, x: f32, y: f32) -> Self {
        self.push(ModifierElement::Offset { x, y })
    }

    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    pub fn align_self(self, alignment: crate::layout::Alignment) -> Self {
        self.push(ModifierElement::AlignSelf { alignment })
    }

    /// 布局权重（Row 中按比例分配宽度，Column 中按比例分配高度）
    pub fn layout_weight(self, weight: f32) -> Self {
        self.push(ModifierElement::LayoutWeight { weight })
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
    /// .background(Color::RED, Shape::Circle)
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```
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

    /// 背景模糊（毛玻璃，单 snapshot 多节点共享）
    pub fn backdrop_blur(self, radius: f32) -> Self {
        self.push(ModifierElement::BackdropBlur { radius })
    }
}

// ── Input Modifier 方法 ──

impl Modifier {
    /// 添加点击行为
    pub fn clickable(self, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::Clickable {
            on_click: Arc::new(on_click),
        })
    }

    /// 标记为可获焦点
    pub fn focusable(self) -> Self {
        self.push(ModifierElement::Focusable)
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
    /// .graphics_layer(GraphicsLayerParams { alpha: 0.5, ..Default::default() })
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```
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

    /// 垂直滚动（绑定 ScrollState）
    pub fn vertical_scroll(self, state: ScrollState) -> Self {
        // 读取 offset 以注册 State→Slot 依赖，确保滚动时触发增量重组
        let _ = state.offset.get();
        self.push(ModifierElement::VerticalScroll { state: state.offset })
    }

    /// 水平滚动
    pub fn horizontal_scroll(self, state: ScrollState) -> Self {
        let _ = state.offset.get();
        self.push(ModifierElement::HorizontalScroll { state: state.offset })
    }
}

// ── 查询方法: 布局参数提取 ──

impl Modifier {
    /// 累积的 padding 值（水平 + 垂直分别计算）
    ///
    /// 返回 (horizontal_padding, vertical_padding)，每边的 padding 值
    pub fn get_padding_values(&self) -> (f32, f32) {
        let mut h = 0.0;
        let mut v = 0.0;
        for el in &self.elements {
            match el {
                ModifierElement::Padding { all } => { h += all; v += all; }
                ModifierElement::PaddingHorizontal { value } => { h += value; }
                ModifierElement::PaddingVertical { value } => { v += value; }
                _ => {}
            }
        }
        (h, v)
    }

    /// 水平方向 padding（左 + 右各自累积后返回 (left, right)）
    pub fn get_padding_horizontal(&self) -> (f32, f32) {
        let mut total = 0.0;
        for el in &self.elements {
            match el {
                ModifierElement::Padding { all } => { total += all; }
                ModifierElement::PaddingHorizontal { value } => { total += value; }
                _ => {}
            }
        }
        (total, total)
    }

    /// 垂直方向 padding（上 + 下各自累积后返回 (top, bottom)）
    pub fn get_padding_vertical(&self) -> (f32, f32) {
        let mut total = 0.0;
        for el in &self.elements {
            match el {
                ModifierElement::Padding { all } => { total += all; }
                ModifierElement::PaddingVertical { value } => { total += value; }
                _ => {}
            }
        }
        (total, total)
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
                return Some((resolve(width), resolve(height)));
            }
        }
        None
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
    pub fn vertical_scroll_state(&self) -> Option<&crate::core::state::State<f32>> {
        for el in &self.elements {
            if let ModifierElement::VerticalScroll { state } = el {
                return Some(state);
            }
        }
        None
    }

    /// 水平滚动状态（如果有 HorizontalScroll modifier）
    pub fn horizontal_scroll_state(&self) -> Option<&crate::core::state::State<f32>> {
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
            if let ModifierElement::Clickable { on_click } = el {
                return Some(on_click);
            }
        }
        None
    }

    pub fn graphics_layer_params(&self) -> Option<GraphicsLayerParams> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::GraphicsLayer { params_fn } = el { Some((params_fn)()) } else { None }
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
    pub fn get_offset(&self) -> Option<(f32, f32)> {
        for el in &self.elements {
            if let ModifierElement::Offset { x, y } = el {
                return Some((*x, *y));
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
                ModifierElement::VerticalScroll { state } => { let _ = state.get(); }
                ModifierElement::HorizontalScroll { state } => { let _ = state.get(); }
                _ => {}
            }
        }
    }
}

impl Debug for ModifierElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size { width, height } => f.debug_struct("Size").field("width", width).field("height", height).finish(),
            Self::Padding { all } => f.debug_struct("Padding").field("all", all).finish(),
            Self::PaddingHorizontal { value } => f.debug_struct("PaddingHorizontal").field("value", value).finish(),
            Self::PaddingVertical { value } => f.debug_struct("PaddingVertical").field("value", value).finish(),
            Self::FillMaxWidth => f.write_str("FillMaxWidth"),
            Self::FillMaxHeight => f.write_str("FillMaxHeight"),
            Self::FillMaxSize => f.write_str("FillMaxSize"),
            Self::Offset { x, y } => f.debug_struct("Offset").field("x", x).field("y", y).finish(),
            Self::AlignSelf { alignment } => f.debug_struct("AlignSelf").field("alignment", alignment).finish(),
            Self::LayoutWeight { weight } => f.debug_struct("LayoutWeight").field("weight", weight).finish(),
            Self::Background { .. } => f.debug_struct("Background").finish(),
            Self::Border { width, color, shape } => f.debug_struct("Border").field("width", width).field("color", color).field("shape", shape).finish(),
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
            Self::Focusable => f.write_str("Focusable"),
            Self::KbEvent { on_key, on_pre_key } => f.debug_struct("KbEvent").field("on_key", &on_key.is_some()).field("on_pre_key", &on_pre_key.is_some()).finish(),
            Self::PointerEvent { on_ptr, on_pre_ptr } => f.debug_struct("PointerEvent").field("on_ptr", &on_ptr.is_some()).field("on_pre_ptr", &on_pre_ptr.is_some()).finish(),
            Self::FocusRequesterId { id } => f.debug_tuple("FocusRequesterId").field(id).finish(),
            Self::VerticalScroll { .. } => f.write_str("VerticalScroll(<state>)"),
            Self::HorizontalScroll { .. } => f.write_str("HorizontalScroll(<state>)"),
            Self::GraphicsLayer { .. } => f.debug_struct("GraphicsLayer").finish(),
            Self::Blur { radius } => f.debug_struct("Blur").field("radius", radius).finish(),
            Self::BackdropBlur { radius } => f.debug_struct("BackdropBlur").field("radius", radius).finish(),
        }
    }
}

// ── ScrollState ──

/// 图形层变换参数
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphicsLayerParams {
    pub scale_x: f32,
    pub scale_y: f32,
    pub alpha: f32,
    pub translation_x: f32,
    pub translation_y: f32,
    pub rotation_z: f32,
}

impl Default for GraphicsLayerParams {
    fn default() -> Self {
        Self {
            scale_x: 1.0, scale_y: 1.0, alpha: 1.0,
            translation_x: 0.0, translation_y: 0.0, rotation_z: 0.0,
        }
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
        Self(Arc::new(move || params))
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
}

impl ScrollState {
    pub fn new() -> Self {
        ScrollState {
            offset: crate::core::state::State::new(0.0),
            is_scroll_in_progress: crate::core::state::State::new(false),
        }
    }

    /// 立即滚动到指定位置
    pub fn scroll_to(&self, value: f32, max_offset: f32) {
        self.offset.set(value.clamp(0.0, max_offset));
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
