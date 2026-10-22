//! Modifier 系统 — 不可变链式修饰符 + 自定义扩展
//!
//! 类似 Jetpack Compose 的 Modifier，用于解耦外观/行为/布局。
//! - 链式 API: `Modifier::new().size(100, 100).padding(10).background(Color::RED)`
//! - 左到右 = 外到内
//! - 分为三类: LayoutModifier / DrawModifier / PointerInputModifier

use std::cell::RefCell;
use std::sync::Arc;
use std::ops::Range;
use std::fmt::{self, Debug};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use crate::fling::SnapInfo;
use crate::graphics::{DEFAULT_AMBIENT_SHADOW_COLOR, DEFAULT_SPOT_SHADOW_COLOR};
use crate::input::{KbEvent, PointerEvent};
use crate::layout::{Dimension, IntrinsicSize, SizeValue};
use crate::text::RichSpanStyle;
use crate::interaction::MutableInteractionSource;
use crate::graphics::{
    BackgroundColor, Color, ColorFilter, FilterQuality, GraphicsLayerParams,
    GraphicsLayerSpec, ShadowParams, Shape, TransformOrigin,
};


// ── Dimension ──


// ── Shape ──


// ── Color (占位) ──


// ── Modifier ──

// ── KbEvent ──


// ── PointerEvent ──


// ── 图片绘制类型（ColorFilter / FilterQuality / BlendMode——对齐 Compose ui.graphics）──


// ── ModifierElement ──

// ═══════════════════════════════════════════════════════════
// Modifier Node（实验性开放扩展点，分支 exp/modifier-node）
// ───────────────────────────────────────────────────────────
// 目标：把 Modifier 从"封闭枚举"变成"开放节点"。枚举每加一种行为
// 就要改 measure/render/hit/app 四处 match；node 化之后，第三方可以
// 不改核心实现自定义行为。
//
// 设计（双轨，零破坏）：
// - `Modifier` 内部新增 `nodes: Vec<ModifierNode>`，与 `elements` 并存。
//   旧 builder（background/clickable/…）照旧 push 枚举；新扩展走 node。
// - 每种行为是一个 `Arc<dyn XxxNode>`，核心管线（measure/render/input/
//   debug）在走完枚举 match 后，再走一遍 node 链（同语义合并）。
// - Node 要求 Debug + Send + Sync（与现有元素约束一致）；Click 参数相等
//   走 `node_key()`（默认按 TypeId + 可选 id，保证 param_eq 可判定）。
// - 试点：BackgroundNode（绘制）+ ClickableNode（输入）——验证"枚举
//   不动、node 链并行生效"后，再逐个迁移。
//
// 有状态节点（StatefulNode 约定）：node 本身不持有组合期 State（remember
// 必须在 `#[composable] build` 内调用——宏语句 key 上下文要求）。约定：
// A component builds a State with `ctx.remember(...)`, clones it into the node (a State is an Arc
// wrapper, so the clone is cheap), reads it with `get()` at build time to register the
// compose/layout dependency, and evaluates it with `peek()` at paint time.
//
// A paint-time `get()` registers nothing (the dependency frame is gone by then: DEP_MODE is None -
// the same rule the draw-layer elements follow). Repaint comes from the build-time `get()` (a change
// recomposes -> new node -> repaint) or from an animation writing through `Animating` and requesting
// a redraw. The node has no private channel; it follows the elements (see
// `node_track_stateful_draw_follows_state`).
// 例：`let src = ctx.remember(|| MutableInteractionSource::new()).get();`
//     `Modifier::new().click_node(MyClick { source: src.clone() })`。
// 状态生命周期跟组合槽走（remember 语义），node 只是"行为 + 状态引用"
// 的载体——节点移除时 State 随槽回收，无需 onAttach/onDetach。
// ═══════════════════════════════════════════════════════════

/// Draw node: invoked with the node rect at render time (cf. Compose DrawModifierNode).
/// The legacy `render_modifier_element` returning `Option<TextParams>` stays in core;
/// a node only paints (background/decoration) and never extracts text.
///
/// Ordering (measured): nodes paint uniformly at the background layer (after the enum
/// chain), with no chain-order interleaving. For above-content painting use
/// [`DrawWrapNode::draw_after`].
/// What a [`Modifier::draw_with_cache`] build closure sees: the inputs that invalidate its cache.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawCacheScope {
    pub width: f32,
    pub height: f32,
    pub density: f32,
}

/// The node behind `draw_with_cache`: rebuilds its cache when the drawn size or density changes,
/// then hands the cached value to the draw closure on every paint — Compose's `drawWithCache`.
struct DrawCacheNode<C> {
    build: std::sync::Arc<dyn Fn(DrawCacheScope) -> C + Send + Sync>,
    draw: std::sync::Arc<dyn Fn(&C, &skia_safe::Canvas, skia_safe::Rect) + Send + Sync>,
    cache: std::sync::Mutex<Option<(DrawCacheScope, C)>>,
}

impl<C> std::fmt::Debug for DrawCacheNode<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DrawCacheNode")
    }
}

impl<C: Send + Sync + 'static> DrawNode for DrawCacheNode<C> {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        let scope = DrawCacheScope {
            width: rect.width(),
            height: rect.height(),
            density: crate::runtime::density::current_density().density,
        };
        let mut cache = self.cache.lock().expect("draw cache");
        let rebuild = cache.as_ref().map(|(cached, _)| *cached != scope).unwrap_or(true);
        if rebuild {
            *cache = Some((scope, (self.build)(scope)));
        }
        let (_, value) = cache.as_ref().expect("just built");
        (self.draw)(value, canvas, rect);
    }
}

pub trait DrawNode: std::fmt::Debug + Send + Sync {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect);
    /// Skip fingerprint MUST rule (P1-3): nodes with params MUST override, covering
    /// **all output-affecting static params** (color/shape/value/flags); write-back
    /// channels (State write-back) and transient animation values (per-frame
    /// progress/alpha) MUST NOT enter the key (they travel the dependency/repaint
    /// channel, otherwise every animation frame Enters); callback fields count as
    /// equal (rebuild does not Enter). Default = TypeId name (fine for paramless
    /// nodes; forgetting the override on a param node silently mis-hits Skip and
    /// leaves stale output).
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// Wrapping draw node (cf. Compose `DrawModifierNode` `drawWithContent`: paint before
/// and after content; Compose `drawBehind` = before only, above-content decor = after only).
///
/// Position (`render_pass1`):
/// - `draw_before`: same layer as [`DrawNode`] (after the enum chain, before text/children).
/// - `draw_after`: after children and after `draw_ripple`, before scroll/clip restore —
///   above content (covers text and children), inside the scroll translate (same stack
///   as ripple).
///
/// `modifier` param: for reading chain enums when needed (e.g. inferring the shape from
/// the nearest Background/Border — same logic as inside `draw_ripple`). The render side
/// passes the owning node's `&Modifier`. Ignore it when unneeded.
///
/// `node_key` MUST rule is the same as [`DrawNode`] (all static visual params in;
/// write-back/transient values out).
pub trait DrawWrapNode: std::fmt::Debug + Send + Sync {
    fn draw_before(
        &self,
        _canvas: &skia_safe::Canvas,
        _rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
    }
    fn draw_after(
        &self,
        _canvas: &skia_safe::Canvas,
        _rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
    }
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 点击节点：输入期沿命中路径查询（对标 Compose 点击语义）。
/// 与 `Clickable` 枚举同优先级——核心 `on_click()` 先查枚举、再查 node。
///
/// 单首语义（P1-2 结论）：每 Modifier 仅首个 ClickNode 生效（与枚举 `on_click()`
/// 取首个 Clickable 一致）。多 push 两个 click_node 时第二个静默不触发——
/// 如需多回调请合并进一个 node 的 `on_click`。`node_clicks()` 迭代器保留供
/// 未来多播语义（暂未启用）。
pub trait ClickNode: std::fmt::Debug + Send + Sync {
    fn on_click(&self);
    /// 绑定的交互源（press 波纹用；无则 None）。
    fn interaction(&self) -> Option<crate::interaction::MutableInteractionSource> {
        None
    }
    /// Skip 指纹 MUST 规范（同 DrawNode）：静态参数进 key；回写/瞬态动画值
    /// 不进；回调视为相同。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 指针节点：原始指针事件（对标 Compose PointerInputModifierNode）。
/// 与 `PointerEvent` 枚举同语义（pre = 外→内隧道，on = 内→外冒泡），返回 true
/// 即消费（停止冒泡）。第三方自定义手势/悬停/拖拽识别挂这里，无需改 app.rs。
///
/// ⚠ 手势路由隔离（P0-1 教训）：PointerNode **不参与** `has_gesture()` 手势目标
/// 选择——手势竞技场（tap/drag 识别）只认 TapOn/DragOn 枚举。PointerNode 只收
/// `dispatch_ptr_event` 的原始 down/move/up（hover 日志、自定义坐标换算等装饰性
/// 用途）。需要手势回调的 node 应同时挂对应枚举（如 on_tap），或等手势竞技场
/// 落地后的 GestureNode。把 PointerNode 纳入 has_gesture 会静默偷走外层 tap
/// （内层装饰性 node 锁定 gesture 目标，fire_gesture_action 只查枚举 → 外层
/// TapOnTap 永不触发）。
pub trait PointerNode: std::fmt::Debug + Send + Sync {
    /// 隧道阶段（外→内）：对应 `on_pre_ptr`。
    fn on_pre(&self, _ev: &PointerEvent) -> bool {
        false
    }
    /// 冒泡阶段（内→外）：对应 `on_ptr`。
    fn on_event(&self, _ev: &PointerEvent) -> bool {
        false
    }
    /// Skip 指纹 MUST 规范（同 DrawNode）。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 键盘节点：按键事件（对标 Compose KeyInputModifierNode）。
/// 与 `KbEvent` 枚举同语义（pre = root→focused 隧道，on = focused→root 冒泡），
/// 返回 true 即消费。第三方自定义快捷键/输入拦截挂这里，无需改 app.rs。
pub trait KeyNode: std::fmt::Debug + Send + Sync {
    /// 隧道阶段（root→focused）：对应 `on_pre_key`。
    fn on_pre(&self, _ev: &KbEvent) -> bool {
        false
    }
    /// 冒泡阶段（focused→root）：对应 `on_key`。
    fn on_event(&self, _ev: &KbEvent) -> bool {
        false
    }
    /// Skip 指纹 MUST 规范（同 DrawNode）。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 布局节点 A 型：约束变换（对标 Compose LayoutModifier）。
/// 输入 incoming 约束，输出给下一环节的约束。多个 A 型 node 按挂载序串行。
/// 插入点 = resolved_size 之后、padding 之前（与现有链序同，见 measure_node）。
/// 动态值在此求值（measure 期读 State 注册布局依赖——与 Dimension::Dynamic 同）。
///
/// 纯度 MUST（P1-6）：`transform` MUST 为“key 参数 + State::get”的纯函数——
/// 禁止读外部可变（Atomic/时钟/RefCell/全局）。常量折叠按 incoming 缓存，
/// 同 key 同约束直接返回旧尺寸；非纯读取即 stale（枚举侧无此口子，node 独有）。
pub trait LayoutModifierNode: std::fmt::Debug + Send + Sync {
    fn transform(&self, inner: crate::layout::Constraints) -> crate::layout::Constraints;
    /// Skip 指纹 MUST 规范（同 DrawNode）。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// Open node container (second track alongside `ModifierElement`).
#[derive(Debug, Clone)]
pub enum ModifierNode {
    Draw(std::sync::Arc<dyn DrawNode>),
    /// Wrapping draw node (before + after content). Renders at the same slots
    /// as Draw for before, and after children/ripple for after.
    DrawWrap(std::sync::Arc<dyn DrawWrapNode>),
    Click(std::sync::Arc<dyn ClickNode>),
    Pointer(std::sync::Arc<dyn PointerNode>),
    Key(std::sync::Arc<dyn KeyNode>),
    Layout(std::sync::Arc<dyn LayoutModifierNode>),
}

/// Modifier 链中的单个元素。
///
/// 按类别分为 Layout / Draw / Input 三类。
/// 用 enum 而非 trait object，便于后续 Layout 系统分类提取。
/// One resolved size-shaping step, in chain order — see [`Modifier::size_steps`].
#[derive(Debug, Clone, Copy)]
pub(crate) enum SizeStep {
    /// A `Size` element: resolved fixed axes. `None` is Auto/Intrinsic — those say nothing here
    /// (an intrinsic axis is resolved after the walk, by `intrinsic_*_request`).
    Size { width: Option<f32>, height: Option<f32> },
    /// A `RequiredSize` element: resolved numeric axes; the intrinsic overloads are handled later.
    RequiredSize { width: Option<f32>, height: Option<f32> },
    /// A `fillMax*` element, applied where it stands: the share of the CURRENT maximum.
    Fill { width: Option<f32>, height: Option<f32> },
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    /// A resolved `SizeIn`; `None` is Compose's `Dp.Unspecified`.
    SizeIn {
        min_width: Option<f32>,
        min_height: Option<f32>,
        max_width: Option<f32>,
        max_height: Option<f32>,
        enforce_incoming: bool,
    },
    /// `defaultMinSize` — the minimum applies only where the incoming one is 0.
    DefaultMin {
        min_width: Option<f32>,
        min_height: Option<f32>,
    },
    /// A `width/height(IntrinsicSize)` request, kept in chain order so the measure pipeline can
    /// snapshot the constraints the request saw. A `fillMax` outside the request tightens those
    /// constraints (and the intrinsic answer honours them); a `fillMax` inside it does not.
    Intrinsic { width: bool, height: bool },
    /// `wrapContent*` — relax the incoming minimum (and max when unbounded) for what follows.
    Wrap {
        width: bool,
        height: bool,
        align: crate::layout::ContentAlignment,
        unbounded: bool,
    },
}

#[derive(Clone)]
pub(crate) enum ModifierElement {
    // ── Layout 类 ──
    /// 固定尺寸
    /// 尺寸（静态 Dimension 或动态求值 SizeValue——布局属性动画用 State/闭包）
    Size { width: SizeValue, height: SizeValue },
    /// 最小宽度（对标 Compose `Modifier.widthIn(min=...)`——仅提升 incoming
    /// min 约束；宽度超限时被 max 夹住；支持动态值——动画可作用于 min）
    MinWidth { value: Dimension },
    /// 最小高度（对标 Compose `Modifier.heightIn(min=...)`）
    MinHeight { value: Dimension },
    /// Maximum width (Compose `Modifier.widthIn(max = ...)`) — lowers the incoming max
    /// constraint, so content that would be wider is constrained to it, and is then held at or
    /// above the min. Supports a dynamic value.
    ///
    /// A min and a max on the same axis may conflict; which one wins is Compose's chain-order rule,
    /// replayed by [`Modifier::size_steps`] — the max written first yields the max, the min
    /// written first yields the min.
    MaxWidth { value: Dimension },
    /// Maximum height (Compose `Modifier.heightIn(max = ...)`)
    MaxHeight { value: Dimension },
    /// 四边 padding（每边独立，支持动态 SizeValue——动画可作用于 padding）
    PaddingSides {
        start: Dimension,
        top: Dimension,
        end: Dimension,
        bottom: Dimension,
    },
    /// Padding equal to a part of the window's insets (Compose's windowInsetsPadding). The applied
    /// insets are consumed for the subtree, so a nested call pads by nothing.
    WindowInsetsPadding {
        kind: crate::insets::InsetsKind,
        controller: crate::insets::WindowInsetsController,
    },
    /// Consume a part of the window's insets without padding (Compose's consumeWindowInsets).
    ConsumeWindowInsets {
        kind: crate::insets::InsetsKind,
        controller: crate::insets::WindowInsetsController,
    },
    /// Fill the incoming maximum width — Compose `fillMaxWidth`. `fraction < 1` takes that share
    /// of the maximum; an unbounded maximum is a no-op (Compose's `FillNode`).
    FillMaxWidth { fraction: f32 },
    /// Fill the incoming maximum height (see `FillMaxWidth` for the fraction rules).
    FillMaxHeight { fraction: f32 },
    /// Fill the incoming maximum on both axes.
    FillMaxSize { fraction: f32 },
    /// A size RANGE on one or both axes — Compose's `widthIn`/`heightIn`/`sizeIn`
    /// (`enforce_incoming = true`) and `requiredWidthIn`/`requiredSizeIn` (`false`). Bounds are
    /// resolved during measure, so a `Dimension::Dynamic` animates through layout.
    SizeIn {
        min_width: Option<Dimension>,
        min_height: Option<Dimension>,
        max_width: Option<Dimension>,
        max_height: Option<Dimension>,
        enforce_incoming: bool,
    },
    /// Compose's `defaultMinSize`: a minimum that applies only where the incoming minimum is 0.
    DefaultMinSize {
        min_width: Option<Dimension>,
        min_height: Option<Dimension>,
    },
    /// Compose's `wrapContentWidth`/`wrapContentHeight`/`wrapContentSize`: measure without the
    /// incoming minimum (and maximum, when `unbounded`) and align the content inside the resulting box.
    WrapContent {
        width: bool,
        height: bool,
        align: crate::layout::ContentAlignment,
        unbounded: bool,
    },
    /// 位置偏移（不影响布局尺寸，仅移动绘制位置；RTL 下 x 镜像——
    /// 对标 Compose `Modifier.offset`；值支持动态——动画可作用于 offset）
    Offset { x: Dimension, y: Dimension },
    /// 绝对偏移（RTL 下**不**镜像——对标 Compose `Modifier.absoluteOffset`）
    AbsoluteOffset { x: Dimension, y: Dimension },
    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    AlignSelf { alignment: crate::layout::Alignment },
    /// Align this child by one of its own alignment lines instead of by its edge (Compose's
    /// `RowScope`/`ColumnScope` `Modifier.alignBy`). See [`Modifier::align_by`].
    AlignBy { line: crate::layout::AlignmentLine },
    /// A line a node reports, computed from its measured size — see
    /// [`Modifier::alignment_line`]. Compose's `layout(alignmentLines = …)` in modifier form.
    AlignmentLineValue {
        line: crate::layout::AlignmentLine,
        value: std::sync::Arc<dyn Fn(crate::unit::Size) -> f32 + Send + Sync>,
    },
    /// Reserve a touch target of at least this many logical pixels on BOTH axes, centring the content
    /// inside it — Compose's `Modifier.minimumInteractiveComponentSize`. `size <= 0` is off.
    MinimumInteractiveSize { size: f32 },
    /// Pad this node so its content's alignment line sits `before` from the near edge and `after`
    /// from the far one (Compose's `Modifier.paddingFrom`). `None` is Compose's
    /// `Dp.Unspecified` — that side is not constrained.
    ///
    /// The axis follows the line's orientation, and the padding is worked out AFTER the content is
    /// measured, because it is the content's line that says how much padding is needed.
    PaddingFrom {
        line: crate::layout::AlignmentLine,
        before: Option<f32>,
        after: Option<f32>,
    },
    /// 布局权重（Row 中分配宽度，Column 中分配高度）
    ///
    /// `fill` mirrors Compose's `weight(weight, fill)`: with `fill = true` the child is measured
    /// tight to its share and the parent keeps that share (`ColumnScope.weight`'s documented
    /// behaviour). With `fill = false` the child is measured with its share as the MAXIMUM and the
    /// parent keeps the size the child actually asked for. That is the half material3's date picker
    /// dialog depends on to collapse in input mode — `DatePickerDialog.android.kt:95` wraps its
    /// content in `Box(Modifier.weight(1f, fill = false))`, commented "Fill is false to support
    /// collapsing the dialog's height when switching to input mode". The arrangement does not matter:
    /// since the `Arrangement` variants were aligned with Compose (`layout/flex.rs`'s
    /// `measured_main`), a spreading arrangement no longer grows a content-sized container to the
    /// maximum its parent offers, so the collapse survives `SpaceBetween` as well — which is what
    /// material3 itself relies on.
    ///
    /// The share really is a MAXIMUM for a `fill = false` child: a child asking for more is coerced
    /// into it, because a `Size` is clamped into the incoming range (`layout/node.rs`'s measure, the
    /// same rule `Modifier::size` documents) before anything flex computes. `Modifier::required_size`
    /// is the escape hatch for a child that must leave the share on purpose.
    LayoutWeight { weight: f32, fill: bool },
    /// 宽高比约束（对标 Compose `Modifier.aspectRatio`——ratio = 宽/高）
    AspectRatio { ratio: f32, match_height_first: bool },
    /// 强制尺寸（对标 Compose `Modifier.requiredSize`——忽略 incoming
    /// constraints 的收缩，允许溢出父约束）
    RequiredSize { width: Option<SizeValue>, height: Option<SizeValue> },
    /// 尺寸上报（对标 Compose `Modifier.onSizeChanged`）——节点测量完成后
    /// 以逻辑像素回调 (width, height)；元素内部去重，尺寸未变化不重复回调
    OnSizeChanged { callback: std::sync::Arc<dyn Fn(f32, f32) + Send + Sync> },
    /// Fires with the placed box, in window coordinates, when it changes — Compose's
    /// `onGloballyPositioned`. Delivered after `OnPlaced` for the same pass.
    OnGloballyPositioned { callback: std::sync::Arc<dyn Fn(crate::layout::GlobalPosition) + Send + Sync> },
    /// Compose's `onPlaced`: the same box, delivered before the `OnGloballyPositioned` callbacks.
    OnPlaced { callback: std::sync::Arc<dyn Fn(crate::layout::GlobalPosition) + Send + Sync> },
    /// Compose's `layoutId`: a tag for this layout node, visible in the debug tree.
    LayoutId { id: std::sync::Arc<str> },
    /// Compose's `bringIntoViewRequester`: this node's requests are served after the layout pass.
    BringIntoViewRequester { requester: crate::layout::BringIntoViewRequester },
    /// 测试标记（对标 Compose `Modifier.testTag`——UI 测试定位；
    /// 调试树 JSON 暴露 tag 字段）
    TestTag { tag: String },
    /// Accessibility declaration (role / name / state) — resolved into the semantics tree by
    /// `crate::semantics`. A node with this element still does layout as usual; the element exists
    /// only to be read (like `TestTag`), so it is a no-op for layout, drawing and input.
    Semantics(crate::semantics::SemanticsConfig),
    /// Compose's `clearAndSetSemantics`: REPLACES this node's declaration and hides its whole
    /// subtree from the semantics tree.
    ClearAndSetSemantics(crate::semantics::SemanticsConfig),
    /// Compose's `focusGroup()`: the node is a traversal group that remembers which child had focus.
    FocusGroup,
    /// Compose's `focusProperties { canFocus = … }` — the part that gates focus eligibility.
    FocusProperties { can_focus: Option<bool> },
    /// Compose's `onFocusChanged`: the callback receives `true` when the node gains focus, `false`
    /// when it loses it. Dispatched when the flag changes, including a loss caused by removal.
    OnFocusChanged { callback: std::sync::Arc<dyn Fn(bool) + Send + Sync> },
    /// 布局方向——本节点 padding start/end 的解析方向；Row/Column 组件
    /// 优先读自身此元素（其次 CompositionLocal 全局方向）。
    /// ⚠ 当前**不**向子树继承（对标 Compose 的子树级方向用
    /// `WiniaTheme::with_theme_and_direction`——CompositionLocal 作用域；
    /// 全 demo 切换也走它）。此元素是节点级便捷覆盖。
    LayoutDirection(crate::layout::LayoutDirection),
    /// Sibling paint order within this node (`Modifier::z_index`) — Compose's `Modifier.zIndex`.
    /// A higher value paints later, i.e. on top, and the hit test walks the same order in reverse so
    /// the topmost sibling receives a press first; equal values keep tree order.
    ZIndex(f32),
    /// TextField 容器子节点角色标记（text-field-v2 容器化——自定义
    /// MeasurePolicy 按角色布局：leading/label/placeholder/prefix/
    /// input/suffix/trailing；仅标记，不参与测量/绘制）
    TextFieldSlot { role: crate::text::field::TextFieldSlotRole },
    /// 阴影（对标 Compose `Modifier.shadow`——elevation 模糊 + 内容裁剪）
    /// 阴影（对标 Compose `Modifier.shadow`——单层参数；elevation 便捷版
    /// 展开为 ambient+spot 两层元素）
    Shadow {
        params: crate::graphics::Dyn<ShadowParams>,
        shape: crate::graphics::Dyn<Shape>,
        clip: bool,
    },

    // ── Draw 类 ──
    /// Background colour + shape. The colour is a [`Dyn`](crate::graphics::Dyn): a literal is stored
    /// inline, a closure is evaluated per paint.
    Background { color: crate::graphics::Dyn<Color>, shape: crate::graphics::Dyn<Shape> },
    /// A gradient or brush fill. Separate from `Background` because a brush is not a color: it needs a
    /// shader, and its geometry is resolved against the node's bounds at paint time.
    BackgroundBrush {
        brush: crate::graphics::Dyn<crate::graphics::Brush>,
        shape: crate::graphics::Dyn<Shape>,
    },
    /// A border stroke. The colour and the shape are carriers: a literal is inline, a state or a
    /// closure is read at paint time. One element covers both - there is no `BorderDynamic` anymore.
    Border {
        width: Dimension,
        color: crate::graphics::Dyn<Color>,
        shape: crate::graphics::Dyn<Shape>,
    },
    /// 裁剪
    Clip { shape: crate::graphics::Dyn<Shape> },
    /// 内容模糊（GPU 原生，零回读）。The radius is a carrier: a state or closure is read per paint.
    Blur { radius: crate::graphics::Dyn<f32> },
    /// 背景模糊（毛玻璃，单 snapshot 多节点共享）。Carrier radius, same as `Blur`.
    BackdropBlur { radius: crate::graphics::Dyn<f32> },
    /// 文本输入框容器视觉（TextField 组件——M3 Filled/Outlined 容器：    /// 背景/指示线/边框/label/支持文本）。组合期解析全部视觉状态
    /// （enabled/focused/is_error/label 悬浮），渲染期静态绘制——
    /// 状态过渡动画由后续迭代接入。
    TextFieldVisual {
        variant: crate::text::field::TextFieldVariant,
        shape: crate::graphics::Dyn<Shape>,
        colors: crate::text::field::TextFieldColors,
        enabled: bool,
        /// material3's `readOnly`, carried here because the RENDER needs it: material3 draws its caret as
        /// `showCursor = enabled && !readOnly && …` (`foundation/text/CoreTextField.kt`), so a read-only
        /// field shows no caret. winia drew one whenever the field had focus.
        read_only: bool,
        /// 光标色（组合期解析：error → error_cursor，否则 primary）
        cursor_color: Color,
        /// The indicator/border colour: an animation `State` driven by `animate_color_as_state`,
        /// interpolated in Oklab, peeked during rendering.
        indicator_color: crate::runtime::state::State<crate::graphics::Color>,
        /// 焦点过渡进度（0 = unfocused，1 = focused——宽度 1↔2px 动画）
        focus_progress: crate::runtime::state::State<f32>,
        /// 视觉变换偏移映射（密码掩码/格式化——渲染/定位跨界转换；
        /// None = 恒等）
        offset_mapping: Option<std::sync::Arc<dyn crate::text::transformation::OffsetMapping>>,
        /// 支持文本（画在容器底部外侧 4dp）
        supporting: Option<SupportingVisual>,
    },
    /// 文本输入框偏移映射（TextField 组件内部使用——**无 M3 容器视觉**的
    /// 裸 TextField 挂载点）。TextFieldVisual 只在有 variant（Filled/Outlined）
    /// 时挂载；裸 TextField + visual_transformation 时 offset_mapping 无处
    /// 可达（点击定位/渲染光标查找 None → 显示偏移直写 selection 越界）。
    /// 仅存映射、无渲染/绘制副作用（render 各 match 走 `_ =>` 兜底）。
    TextFieldOffsetMapping {
        offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping>,
    },

    // ── Content 类 ──
    /// 文本内容（由 Text 组件设置，渲染阶段消费）
    TextContent { content: String, font_size: f32, color: Color, font_weight: crate::text::FontWeight, font_style: crate::text::FontSlant, max_lines: usize, align: crate::text::TextAlign, overflow: crate::text::TextOverflow, soft_wrap: bool, letter_spacing: f32, line_height: Option<f32> },
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
    /// The cursor this node asks for while hovered — Compose Desktop's `pointerHoverIcon`.
    /// `override_descendants` makes this node's icon win over any icon on a descendant.
    PointerHoverIcon {
        icon: crate::input::PointerIcon,
        override_descendants: bool,
    },
    /// 水波纹指示（对标 Compose indication/ripple）——按下时从按压点扩散的
    /// 径向渐变圆，释放后淡出；渲染期按时间计算，无额外动画状态。
    /// `shape = Some` 时波纹裁剪到该形状（Button 传入容器 shape——
    /// Outlined/Text 无背景元素时也能正确裁剪）；None 则从 Background/Border 推断。
    Ripple {
        source: MutableInteractionSource,
        /// The ripple colour — a literal, a state or a closure read per paint.
        color: crate::graphics::Dyn<Color>,
        bounded: bool,
        shape: Option<crate::graphics::Dyn<Shape>>,
        /// The BACKGROUND state layer's diameter when it is fixed (Compose's `ripple(bounded, radius)` names a
        /// radius; material3's checkbox and radio pass `StateLayerSize`, 40 dp). `None` = derive it from the
        /// node's diagonal, which is this framework's original behaviour.
        state_layer_diameter: Option<crate::graphics::Dyn<f32>>,
    },
    /// 图标绘制（Icon 组件内部使用）——source/tint/autoMirror/可变轴
    CustomDraw { f: Arc<dyn Fn(&skia_safe::Canvas, skia_safe::Rect) + Send + Sync> },
    /// 禁用框架焦点环（组件自绘焦点环时用——如 Slider 焦点环包围 thumb 而非整组件）
    NoFocusRing,
    DrawIcon { spec: crate::graphics::IconSpec },
    /// 图片内容（Image 组件——位图/SVG，ContentScale + 对齐 + alpha；
    /// 与 DrawIcon 的区别：不染色、按 ContentScale 缩放、对齐可控）
    ImageContent {
        source: crate::graphics::IconSource,
        content_scale: crate::graphics::ContentScale,
        alignment: crate::graphics::ImageAlignment,
        /// Image opacity — a literal, a state or a closure read per paint (a fade-in image).
        alpha: crate::graphics::Dyn<f32>,
        /// Optional colour filter — a literal or a paint-time carrier.
        color_filter: Option<crate::graphics::Dyn<ColorFilter>>,
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
    LazyScroll { content_height: crate::runtime::state::Backchannel<f32>, reverse: bool },
    /// Compose's `LazyItemScope.animateItem()`: a marker the lazy list reads. The list springs the
    /// item from its previous slot to the new one when the data reorders. Inert outside a lazy list.
    AnimateItem,
    /// 水平滚动
    HorizontalScroll { state: ScrollState, reverse: bool },
    /// Compose's inner shadow: a rim drawn inside the shape, above the content.
    InnerShadow {
        shape: crate::graphics::Dyn<Shape>,
        color: Color,
        blur: f32,
        offset_x: f32,
        offset_y: f32,
    },
    /// Compose's `clipScrollableContainer(orientation)`: the main axis is clipped to the container,
    /// the cross axis keeps 30 dp of slack so a shadow there is not cut off.
    ClipScrollableContainer { vertical: bool },
    /// Compose's `overscroll(effect)`: the container absorbs the delta it cannot scroll into the
    /// effect instead of letting it fall through.
    Overscroll { effect: crate::overscroll::OverscrollEffect },
    /// 嵌套滚动连接（祖先可在 child 前后部分消费 delta/velocity）。
    NestedScroll { connection: Arc<dyn crate::nested_scroll::NestedScrollConnection> },
    /// 图形层变换（scale/alpha/rotation/translation——只触发重绘，不触发布局）
    GraphicsLayer { params: crate::graphics::Dyn<GraphicsLayerParams> },
    /// 共享元素转场标记（components::shared_transition——纯数据标记：配对身份 + 变形规格；
    /// 组合期注册端点（Phase 2），渲染期忽略（`_ =>` 兜底）。bounds 不进
    /// modifier——飞行是 render-phase 行为，bounds 变化永不强制 Enter）
    SharedTransition {
        scope_id: u64,
        key: String,
        kind: crate::transition::SharedKind,
        transform: crate::transition::BoundsTransform,
        path: crate::transition::PathMotion,
        /// Overlay z-order for the flying pair (Compose `zIndexInOverlay`,
        /// default 0). Orders retained ghosts back-to-front; in-tree targets
        /// keep tree order (documented Tier 0 limitation).
        z_index: f32,
        /// sharedBounds enter/exit (Compose `enter`/`exit` — target plays
        /// enter, source plays exit; `None` on sharedElement markers, which
        /// have no such parameters and always crossfade).
        enter: Option<crate::animation::visibility::VisibilityTransition>,
        exit: Option<crate::animation::visibility::VisibilityTransition>,
        /// Render this endpoint in the transition layer during the flight
        /// (Compose `renderInOverlayDuringTransition`, default `true`): the
        /// flying element escapes ancestor clips and ancestor layer
        /// transforms and paints above non-shared content. `false` keeps the
        /// pre-Phase-6 in-tree painting (ancestors keep clipping it). Frozen
        /// at flight resolve — flipping it mid-flight must not move the
        /// element between passes.
        render_in_overlay: bool,
        /// Which SCENE this end belongs to, when it is composed inside a scene host that publishes
        /// one (winia's nav: each transition layer provides its scene id, see
        /// `components::shared_transition::provide_nav_scene`). `None` outside such a host. The flight
        /// system uses it to pair ends that are BOTH alive — during a nav transition the outgoing
        /// and incoming scenes both carry the same shared key, and without a scene the winner is
        /// whichever the tree walk happened to visit last, which reads as a new switch every frame.
        scene: Option<u64>,
    },
    /// Elevate a **non-shared** subtree into the layer for the duration of a
    /// transition (Compose `Modifier.renderInSharedTransitionScopeOverlay`):
    /// pinned app bars / FABs keep their spatial relationship while shared
    /// elements fly over them, and return to ordinary tree order when the
    /// transition ends. `z_index` is Compose's `zIndexInOverlay` — shared
    /// endpoints default to 0, so a bar that must stay on top passes more.
    SharedScopeOverlay {
        scope_id: u64,
        z_index: f32,
    },
    /// Marks the subtree a scene host composed for ONE scene (winia's nav layers): `id` is that
    /// scene's stable id. Scene membership is read from ANCESTRY when it is needed instead of being
    /// stored on each shared marker, because a marker's modifier element is built once and then reused
    /// across frames, so a scene id captured there freezes its first value (measured: one end reported
    /// `scene=None` while the other reported the LEAVING scene, so a nav flight paired the leaving hero
    /// with itself and never grew geometrically).
    SceneTag {
        id: u64,
    },
}

// ── Modifier ──

/// 不可变的 Modifier 链。
///
/// 每次调用修饰方法返回一个新的 Modifier（内部 elements 是 clone-on-write 式的追加）。
/// Clone 是廉价操作。
#[derive(Debug, Clone)]
pub struct Modifier {
    elements: Vec<ModifierElement>,
    nodes: Vec<ModifierNode>,
}

impl Modifier {
    /// 创建空的 Modifier 链
    pub fn new() -> Self {
        Modifier {
            elements: Vec::new(),
            nodes: Vec::new(),
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
        self.nodes.extend(other.nodes);
        self
    }

    /// 返回所有元素的只读引用
    pub(crate) fn elements(&self) -> &[ModifierElement] {
        &self.elements
    }

    // ── Node 轨道（实验性开放扩展点） ──

    /// Append a draw node (renders at the background layer, after the enum chain).
    pub fn draw_node(self, node: impl DrawNode + 'static) -> Self {
        self.push_node(ModifierNode::Draw(std::sync::Arc::new(node)))
    }

    /// Compose's `Modifier.drawWithCache`: `build` runs once per (size, density) and `draw` receives
    /// the cached value on every paint. What the cache is for: a Skia shader, a paragraph, a
    /// geometry path — anything whose construction should not repeat 60 times a second.
    pub fn draw_with_cache<C: Send + Sync + 'static>(
        self,
        build: impl Fn(DrawCacheScope) -> C + Send + Sync + 'static,
        draw: impl Fn(&C, &skia_safe::Canvas, skia_safe::Rect) + Send + Sync + 'static,
    ) -> Self {
        self.draw_node(DrawCacheNode {
            build: std::sync::Arc::new(build),
            draw: std::sync::Arc::new(draw),
            cache: std::sync::Mutex::new(None),
        })
    }

    /// Append a wrapping draw node (before at the background layer, after above
    /// content — after children and ripple, inside the scroll translate).
    pub fn draw_wrap_node(self, node: impl DrawWrapNode + 'static) -> Self {
        self.push_node(ModifierNode::DrawWrap(std::sync::Arc::new(node)))
    }

    /// 追加一个点击节点（与 Clickable 枚举同优先级参与点击分发）。
    pub fn click_node(self, node: impl ClickNode + 'static) -> Self {
        self.push_node(ModifierNode::Click(std::sync::Arc::new(node)))
    }

    /// 追加一个指针节点（与 PointerEvent 枚举同语义参与隧道/冒泡分发）。
    pub fn pointer_node(self, node: impl PointerNode + 'static) -> Self {
        self.push_node(ModifierNode::Pointer(std::sync::Arc::new(node)))
    }

    /// 追加一个键盘节点（与 KbEvent 枚举同语义参与隧道/冒泡分发）。
    pub fn key_node(self, node: impl KeyNode + 'static) -> Self {
        self.push_node(ModifierNode::Key(std::sync::Arc::new(node)))
    }

    /// 追加一个布局节点 A 型（约束变换——resolved_size 之后、padding 之前串行）。
    pub fn layout_node(self, node: impl LayoutModifierNode + 'static) -> Self {
        self.push_node(ModifierNode::Layout(std::sync::Arc::new(node)))
    }

    pub(crate) fn push_node(mut self, node: ModifierNode) -> Self {
        self.nodes.push(node);
        self
    }

    /// 返回所有开放节点的只读引用
    #[allow(dead_code)] // the tests in this file call it
    pub(crate) fn modifier_nodes(&self) -> &[ModifierNode] {
        &self.nodes
    }

    /// Open draw-node iteration (render pipeline — background layer, after the enum chain).
    pub(crate) fn draw_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn DrawNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Draw(d) => Some(d),
            _ => None,
        })
    }

    /// Open wrapping-draw-node iteration (render pipeline — before at the background
    /// layer, after above content).
    pub(crate) fn draw_wrap_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn DrawWrapNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::DrawWrap(w) => Some(w),
            _ => None,
        })
    }

    /// 开放指针节点迭代（输入管线用——与枚举 PointerEvent 同序交织分发）。
    pub(crate) fn pointer_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn PointerNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Pointer(p) => Some(p),
            _ => None,
        })
    }

    /// 开放键盘节点迭代（输入管线用——与枚举 KbEvent 同序交织分发）。
    pub(crate) fn key_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn KeyNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Key(k) => Some(k),
            _ => None,
        })
    }

    /// 开放布局节点迭代（测量管线用——resolved_size 之后串行变换约束）。
    pub(crate) fn layout_nodes(&self) -> impl Iterator<Item = &std::sync::Arc<dyn LayoutModifierNode>> {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Layout(l) => Some(l),
            _ => None,
        })
    }

    /// 是否含布局节点（measure 早退用——P2-2）。
    pub(crate) fn has_layout_nodes(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n, ModifierNode::Layout(_)))
    }

    // ── 试点节点（exp/modifier-node）：Background/Clickable 的 node 等价物 ──
    // 用法示例见本文件末尾测试 `node_track_*`。第三方自定义行为照此形状实现
    // DrawNode/ClickNode trait 即可，无需改核心枚举与管线 match。
}

impl Default for Modifier {
    fn default() -> Self {
        Self::new()
    }
}

// ── Layout Modifier 方法 ──

impl Modifier {
    /// 设置固定宽高
    ///
    /// 对标 Compose `Modifier.size`：请求值会被**夹进** incoming 范围（`enforceIncoming = true`
    /// ——`SizeNode.measure` 对 `Constraints.fixed(w, h)` 做 `constraints.constrain`）。所以
    /// 请求超出父给的 max 时结果取 max，而不是溢出；想真的溢出用 [`Modifier::required_size`]。
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
            height: SizeValue::Auto,
        })
    }

    /// 仅设置高度（支持动态：`State<f32>`/`DerivedValue`/闭包——measure 时读）
    pub fn height(self, h: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Size {
            width: SizeValue::Auto,
            height: h.into(),
        })
    }

    /// 四边等距 padding（支持动态：`State<f32>`/`DerivedValue`/闭包——
    /// 动画可作用于 padding，对标 Compose `animateDpAsState` + `padding`）
    pub fn padding(self, all: impl Into<Dimension>) -> Self {
        let v = all.into();
        self.push(ModifierElement::PaddingSides {
            start: v.clone(),
            top: v.clone(),
            end: v.clone(),
            bottom: v,
        })
    }

    /// 水平方向 padding（start + end）
    pub fn padding_horizontal(self, value: impl Into<Dimension>) -> Self {
        let v = value.into();
        self.push(ModifierElement::PaddingSides {
            start: v.clone(),
            top: Dimension::Fixed(0.0),
            end: v,
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 垂直方向 padding（top + bottom）
    pub fn padding_vertical(self, value: impl Into<Dimension>) -> Self {
        let v = value.into();
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: v.clone(),
            end: Dimension::Fixed(0.0),
            bottom: v,
        })
    }

    /// 起始边（左）padding——单边，支持动态
    pub fn padding_start(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: value.into(),
            top: Dimension::Fixed(0.0),
            end: Dimension::Fixed(0.0),
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 末尾边（右）padding——单边，支持动态
    pub fn padding_end(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: Dimension::Fixed(0.0),
            end: value.into(),
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 顶部 padding——单边，支持动态
    pub fn padding_top(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: value.into(),
            end: Dimension::Fixed(0.0),
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 底部 padding——单边，支持动态
    pub fn padding_bottom(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: Dimension::Fixed(0.0),
            end: Dimension::Fixed(0.0),
            bottom: value.into(),
        })
    }

    /// 四边各自独立 padding（对标 Compose
    /// `Modifier.padding(start = .., top = .., end = .., bottom = ..)`）。
    /// 参数顺序：(start, top, end, bottom)——全部支持动态。
    /// 与其它 padding 方法可叠加（累积）。
    pub fn padding_sides(
        self,
        start: impl Into<Dimension>,
        top: impl Into<Dimension>,
        end: impl Into<Dimension>,
        bottom: impl Into<Dimension>,
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
        self.push(ModifierElement::FillMaxWidth { fraction: 1.0 })
    }

    /// Compose `fillMaxWidth(fraction)`: take that share of the incoming maximum. An unbounded
    /// maximum leaves the content untouched.
    pub fn fill_max_width_fraction(self, fraction: f32) -> Self {
        self.push(ModifierElement::FillMaxWidth { fraction })
    }

    /// A width RANGE — Compose's `Modifier.widthIn(min, max)`: the content is constrained into
    /// `[min, max]` as far as the incoming constraints allow (they win where they are tighter).
    /// One element, so its two bounds are applied together — unlike `min_width(..).max_width(..)`,
    /// which are two nodes and resolve by chain order.
    pub fn width_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min.into()),
            min_height: None,
            max_width: Some(max.into()),
            max_height: None,
            enforce_incoming: true,
        })
    }

    /// A height RANGE — Compose's `Modifier.heightIn(min, max)`; see [`width_in`](Self::width_in).
    pub fn height_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: None,
            min_height: Some(min.into()),
            max_width: None,
            max_height: Some(max.into()),
            enforce_incoming: true,
        })
    }

    /// Both axes in one node — Compose's `Modifier.sizeIn(minWidth, minHeight, maxWidth, maxHeight)`.
    pub fn size_in(
        self,
        min_width: impl Into<Dimension>,
        min_height: impl Into<Dimension>,
        max_width: impl Into<Dimension>,
        max_height: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min_width.into()),
            min_height: Some(min_height.into()),
            max_width: Some(max_width.into()),
            max_height: Some(max_height.into()),
            enforce_incoming: true,
        })
    }

    /// Width range that IGNORES incoming constraints — Compose's `requiredWidthIn(min, max)`.
    /// The incoming range still decides any bound this one leaves unspecified.
    pub fn required_width_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min.into()),
            min_height: None,
            max_width: Some(max.into()),
            max_height: None,
            enforce_incoming: false,
        })
    }

    /// Height range that ignores incoming constraints — Compose's `requiredHeightIn(min, max)`.
    pub fn required_height_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: None,
            min_height: Some(min.into()),
            max_width: None,
            max_height: Some(max.into()),
            enforce_incoming: false,
        })
    }

    /// The four-bound escape hatch — Compose's `requiredSizeIn`.
    pub fn required_size_in(
        self,
        min_width: impl Into<Dimension>,
        min_height: impl Into<Dimension>,
        max_width: impl Into<Dimension>,
        max_height: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min_width.into()),
            min_height: Some(min_height.into()),
            max_width: Some(max_width.into()),
            max_height: Some(max_height.into()),
            enforce_incoming: false,
        })
    }

    /// A minimum that only applies where the incoming minimum is 0 — Compose's
    /// `defaultMinSize`: a component's own floor that a caller can lower from outside.
    pub fn default_min_size(
        self,
        min_width: impl Into<Dimension>,
        min_height: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::DefaultMinSize {
            min_width: Some(min_width.into()),
            min_height: Some(min_height.into()),
        })
    }

    /// Measure without the incoming minimum width (and, with `unbounded`, without the incoming
    /// maximum), then align the content inside the resulting box — Compose's
    /// `Modifier.wrapContentWidth(align, unbounded)`.
    pub fn wrap_content_width(self, align: crate::layout::Alignment, unbounded: bool) -> Self {
        self.push(ModifierElement::WrapContent {
            width: true,
            height: false,
            align: crate::layout::ContentAlignment::new(align, crate::layout::Alignment::Center),
            unbounded,
        })
    }

    /// The height twin of [`wrap_content_width`](Self::wrap_content_width).
    pub fn wrap_content_height(self, align: crate::layout::Alignment, unbounded: bool) -> Self {
        self.push(ModifierElement::WrapContent {
            width: false,
            height: true,
            align: crate::layout::ContentAlignment::new(crate::layout::Alignment::Center, align),
            unbounded,
        })
    }

    /// Both axes — Compose's `Modifier.wrapContentSize(align, unbounded)`.
    pub fn wrap_content_size(
        self,
        align: crate::layout::ContentAlignment,
        unbounded: bool,
    ) -> Self {
        self.push(ModifierElement::WrapContent {
            width: true,
            height: true,
            align,
            unbounded,
        })
    }

    /// 最小宽度（对标 Compose `Modifier.widthIn(min = ...)`）——提升布局
    /// 最小约束，内容不足时撑到该宽度；受父 max 约束夹住。支持动态值。
    pub fn min_width(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MinWidth { value: value.into() })
    }

    /// 最小高度（对标 Compose `Modifier.heightIn(min = ...)`）
    pub fn min_height(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MinHeight { value: value.into() })
    }

    /// Maximum width (Compose `Modifier.widthIn(max = ...)`) — content that would be wider
    /// is constrained to it. A minimum above it wins, as in Compose's `sizeIn`. Supports a
    /// dynamic value, so an animation can drive the cap.
    pub fn max_width(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MaxWidth { value: value.into() })
    }

    /// Maximum height (Compose `Modifier.heightIn(max = ...)`; a conflicting min resolves by chain
    /// order, see [`Self::max_width`])
    pub fn max_height(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MaxHeight { value: value.into() })
    }

    /// 高度填满可用空间
    pub fn fill_max_height(self) -> Self {
        self.push(ModifierElement::FillMaxHeight { fraction: 1.0 })
    }

    /// Compose `fillMaxHeight(fraction)`; see [`fill_max_width_fraction`](Self::fill_max_width_fraction).
    pub fn fill_max_height_fraction(self, fraction: f32) -> Self {
        self.push(ModifierElement::FillMaxHeight { fraction })
    }

    /// 宽高填满可用空间
    pub fn fill_max_size(self) -> Self {
        self.push(ModifierElement::FillMaxSize { fraction: 1.0 })
    }

    /// Compose `fillMaxSize(fraction)`.
    pub fn fill_max_size_fraction(self, fraction: f32) -> Self {
        self.push(ModifierElement::FillMaxSize { fraction })
    }

    /// 位置偏移（不影响布局尺寸，仅移动绘制位置；RTL 下 x 镜像——
    /// 合并式 offset：若链上已有 Offset 元素则更新其 x/y 分量，否则 push 新的。
    /// 这样 `.offset_x(a).offset_y(b)` 得到一个 `Offset{a,b}`（而非两个 Offset——
    /// get_offset 只取第一个，连用会静默丢分量）。对齐 `merge_graphics_layer` 模式。
    fn merge_offset(mut self, x: Option<Dimension>, y: Option<Dimension>) -> Self {
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements.iter_mut().find(|e| matches!(e, ModifierElement::Offset { .. })) {
            if let ModifierElement::Offset { x: ex, y: ey } = el {
                if let Some(nx) = x { *ex = nx; }
                if let Some(ny) = y { *ey = ny; }
            }
        } else {
            elements.push(ModifierElement::Offset {
                x: x.unwrap_or(Dimension::Fixed(0.0)),
                y: y.unwrap_or(Dimension::Fixed(0.0)),
            });
        }
        self.elements = elements;
        self
    }

    /// 对标 Compose `Modifier.offset`）。
    /// 值支持动态（`State`/`DerivedValue`/闭包——动画可作用于 offset）。
    pub fn offset(self, x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self {
        // 已有 Offset（如前面 offset_x）时合并分量，而非再 push 一个
        self.merge_offset(Some(x.into()), Some(y.into()))
    }

    /// 仅 x 方向偏移（RTL 下镜像）。与 `offset_y` 连用合并为单个 Offset 元素。
    pub fn offset_x(self, x: impl Into<Dimension>) -> Self {
        self.merge_offset(Some(x.into()), None)
    }

    /// 仅 y 方向偏移（不受 RTL 影响——垂直方向）。与 `offset_x` 连用合并为单个 Offset 元素。
    pub fn offset_y(self, y: impl Into<Dimension>) -> Self {
        self.merge_offset(None, Some(y.into()))
    }

    /// 绝对偏移（RTL 下**不**镜像——对标 Compose `Modifier.absoluteOffset`）。
    /// 值支持动态。
    pub fn absolute_offset(self, x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: x.into(), y: y.into() })
    }

    /// 仅 x 方向绝对偏移（RTL 下不镜像）
    pub fn absolute_offset_x(self, x: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: x.into(), y: Dimension::Fixed(0.0) })
    }

    /// 仅 y 方向绝对偏移
    pub fn absolute_offset_y(self, y: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: Dimension::Fixed(0.0), y: y.into() })
    }

    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    pub fn align_self(self, alignment: crate::layout::Alignment) -> Self {
        self.push(ModifierElement::AlignSelf { alignment })
    }

    /// Align this child by one of its own alignment lines instead of by its edge — Compose's
    /// `RowScope.Modifier.alignBy` / `ColumnScope.Modifier.alignBy`.
    ///
    /// The child reports the line (a text leaf reports its first baseline) and the parent offsets it
    /// so that every such child's line lands on the same cross-axis position, growing the cross axis
    /// if the line plus what hangs below it needs more room. It takes precedence over
    /// [`Modifier::align_self`] and the parent's own alignment, as Compose's `getCrossAxisPosition`
    /// does (`RowColumnMeasurePolicy.kt:228-251`, `Row.kt:216-231`).
    pub fn align_by(self, line: crate::layout::AlignmentLine) -> Self {
        self.push(ModifierElement::AlignBy { line })
    }

    /// `Modifier.alignByBaseline()` — the common case: align this child's first text baseline with
    /// its siblings', which is how a label lines up with a taller icon or a larger font beside it.
    pub fn align_by_baseline(self) -> Self {
        self.align_by(crate::layout::AlignmentLine::FIRST_BASELINE)
    }

    /// Report an alignment line for this node: the line sits `f(measured size)` from the node's top
    /// (a horizontal line) or start (a vertical one).
    ///
    /// This is where a custom line is PUBLISHED, and it is the stand-in for Compose declaring a line
    /// as the layout is produced — `Modifier.layout { measurable, constraints -> layout(w, h,
    /// alignmentLines = mapOf(line to offset)) { … } }`. winia cannot do it from a `MeasurePolicy`
    /// (a policy is never told its own node index), and a `Modifier` is the dataclass a component
    /// hands to its node, so a closure over the measured size belongs here.
    ///
    /// The line then behaves like any other: a container inherits it through the merger, a parent
    /// aligns a child by it with [`Modifier::align_by`], and [`Modifier::padding_from`] measures
    /// against it. Publish several by calling this several times.
    pub fn alignment_line(
        self,
        line: crate::layout::AlignmentLine,
        f: impl Fn(crate::unit::Size) -> f32 + Send + Sync + 'static,
    ) -> Self {
        self.push(ModifierElement::AlignmentLineValue { line, value: std::sync::Arc::new(f) })
    }

    /// Pad this node so the distance from its near edge to its content's alignment line is `before`
    /// and from the line to the far edge is `after` — Compose's `Modifier.paddingFrom`
    /// (`foundation/layout/AlignmentLine.kt:65`).
    ///
    /// Which edges those are follows the line's orientation: a horizontal line (the text baselines)
    /// means top and bottom. The other axis is untouched, and `None` is Compose's `Dp.Unspecified` —
    /// that side is unconstrained, and the content is then placed against the other one.
    ///
    /// The padding is capped by the incoming maximum on that axis, and `before` wins when both cannot
    /// fit (Compose's contract, `:44-55`).
    /// Compose's `Modifier.windowInsetsPadding(insets)`: pad by the window's current insets of this
    /// kind, resolved at measure time (an inset change re-measures only the nodes that asked), and
    /// consume them for the subtree.
    pub fn window_insets_padding(self, kind: crate::insets::InsetsKind) -> Self {
        self.push(ModifierElement::WindowInsetsPadding {
            kind,
            controller: crate::insets::current(),
        })
    }

    /// Compose's `Modifier.consumeWindowInsets(insets)`: mark the insets consumed for the subtree
    /// without padding by them.
    pub fn consume_window_insets(self, kind: crate::insets::InsetsKind) -> Self {
        self.push(ModifierElement::ConsumeWindowInsets {
            kind,
            controller: crate::insets::current(),
        })
    }

    /// Compose's `Modifier.statusBarsPadding()`.
    pub fn status_bars_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::StatusBars)
    }

    /// Compose's `Modifier.navigationBarsPadding()`.
    pub fn navigation_bars_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::NavigationBars)
    }

    /// Compose's `Modifier.systemBarsPadding()`.
    pub fn system_bars_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::SystemBars)
    }

    /// Compose's `Modifier.imePadding()`.
    pub fn ime_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::Ime)
    }

    /// Compose's `Modifier.displayCutoutPadding()`.
    pub fn display_cutout_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::DisplayCutout)
    }

    /// Compose's `Modifier.systemGesturesPadding()`.
    pub fn system_gestures_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::SystemGestures)
    }

    /// Compose's `Modifier.tappableElementPadding()`.
    pub fn tappable_element_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::TappableElements)
    }

    /// Compose's `Modifier.safeDrawingPadding()`.
    pub fn safe_drawing_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::SafeDrawing)
    }

    /// Compose's `Modifier.safeGesturesPadding()`.
    pub fn safe_gestures_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::SafeGestures)
    }

    /// Compose's `Modifier.safeContentPadding()`.
    pub fn safe_content_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::SafeContent)
    }

    /// Compose's `Modifier.mandatorySystemGesturesPadding()`.
    pub fn mandatory_system_gestures_padding(self) -> Self {
        self.window_insets_padding(crate::insets::InsetsKind::MandatorySystemGestures)
    }

    pub fn padding_from(
        self,
        line: crate::layout::AlignmentLine,
        before: Option<f32>,
        after: Option<f32>,
    ) -> Self {
        self.push(ModifierElement::PaddingFrom { line, before, after })
    }

    /// material3's default minimum touch target, `LocalMinimumInteractiveComponentSize`'s 48.dp
    /// (`material3/InteractiveComponentSize.kt:206-210`).
    pub const MINIMUM_INTERACTIVE_SIZE_DP: f32 = 48.0;

    /// Reserve at least a touch target's worth of space around a small component, centring it —
    /// Compose's `Modifier.minimumInteractiveComponentSize()` (`material3/InteractiveComponentSize.kt:69`),
    /// which material3's own Button, IconButton, Checkbox and friends apply for you.
    ///
    /// The box becomes `max(content, 48)` on both axes with the content centred inside it, and the two
    /// "where the visual content starts" lines are published
    /// ([`AlignmentLine::minimum_interactive_left`] / [`AlignmentLine::minimum_interactive_top`]) so a
    /// parent can align the small component by its real edge rather than by its invisible padding.
    ///
    /// Deviation, recorded: Compose reads the size from `LocalMinimumInteractiveComponentSize`, a
    /// CompositionLocal a screen can set to 0 to switch enforcement off for a whole subtree; winia takes
    /// the value from the modifier, with the 48 dp default in one place, and has no per-subtree switch.
    /// [`Modifier::minimum_interactive_size`] is there for a caller that wants another number.
    pub fn minimum_interactive_component_size(self) -> Self {
        self.minimum_interactive_size(Self::MINIMUM_INTERACTIVE_SIZE_DP)
    }

    /// [`Modifier::minimum_interactive_component_size`] with an explicit size. `size <= 0` disables it,
    /// which is Compose's "unspecified or 0.dp" case.
    pub fn minimum_interactive_size(self, size: f32) -> Self {
        self.push(ModifierElement::MinimumInteractiveSize { size })
    }

    /// Compose's `Modifier.paddingFromBaseline(top, bottom)`: `top` from the container's top edge to
    /// the FIRST text baseline, `bottom` from the LAST one to the container's bottom. Either side may
    /// be `None` (Compose's `Dp.Unspecified`), and Compose expresses it as one `paddingFrom` per
    /// specified side, which is what this does — the two compose, so both can be in force.
    pub fn padding_from_baseline(self, top: Option<f32>, bottom: Option<f32>) -> Self {
        let m = match top {
            Some(top) => self.padding_from(
                crate::layout::AlignmentLine::FIRST_BASELINE,
                Some(top),
                None,
            ),
            None => self,
        };
        match bottom {
            Some(bottom) => m.padding_from(
                crate::layout::AlignmentLine::LAST_BASELINE,
                None,
                Some(bottom),
            ),
            None => m,
        }
    }

    /// 布局权重（Row 中按比例分配宽度，Column 中按比例分配高度）
    ///
    /// The child occupies its whole share, which is Compose's `weight(weight, fill = true)`.
    pub fn layout_weight(self, weight: f32) -> Self {
        self.push(ModifierElement::LayoutWeight { weight, fill: true })
    }

    /// 布局权重，是否填满自己的份额（对标 Compose `Modifier.weight(weight, fill)`）。
    ///
    /// With `fill = false` the share becomes the child's MAXIMUM main-axis size instead of an exact
    /// one, and the parent uses the size the child measured, so a child shorter than its share lets
    /// the container be shorter too. The arrangement is free to be a spreading one as well, because a
    /// content-sized container no longer grows to the maximum its parent offers.
    pub fn layout_weight_fill(self, weight: f32, fill: bool) -> Self {
        self.push(ModifierElement::LayoutWeight { weight, fill })
    }

    /// `aspect_ratio(ratio)`（对标 Compose `Modifier.aspectRatio`）——
    /// 约束本节点宽高比（ratio = 宽/高，必须 > 0）。
    ///
    /// `match_height_first = true` 时优先按高度约束推导宽度
    /// （对标 `matchHeightConstraintsFirst`）。
    pub fn aspect_ratio(self, ratio: f32, match_height_first: bool) -> Self {
        assert!(ratio > 0.0, "aspectRatio {ratio} must be > 0（Compose 前置校验）");
        self.push(ModifierElement::AspectRatio { ratio, match_height_first })
    }

    /// `required_size(w, h)`（对标 Compose `Modifier.requiredSize`）——
    /// 强制本节点为该尺寸，**忽略 incoming constraints 的收缩**（允许
    /// 溢出父约束——enforceIncoming=false 语义）。单轴用
    /// `required_width` / `required_height`。
    pub fn required_size(self, width: impl Into<SizeValue>, height: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::RequiredSize { width: Some(width.into()), height: Some(height.into()) })
    }

    /// 仅强制宽度
    pub fn required_width(self, width: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::RequiredSize { width: Some(width.into()), height: None })
    }

    /// 仅强制高度
    pub fn required_height(self, height: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::RequiredSize { width: None, height: Some(height.into()) })
    }

    /// `test_tag(tag)`（对标 Compose `Modifier.testTag`）——给节点打测试
    /// 标记，UI 测试/调试树用其定位节点（树 JSON 的 `tag` 字段）。
    pub fn test_tag(self, tag: impl Into<String>) -> Self {
        self.push(ModifierElement::TestTag { tag: tag.into() })
    }

    /// Declare what this node is, for a screen reader and for tests (Compose
    /// `Modifier.semantics { ... }`).
    ///
    /// A node that declares a role, a name or a state appears in the semantics tree
    /// (`crate::semantics::semantics_tree`); a click target claims its whole subtree and reports as
    /// ONE element named by the text it contains. Everything else stays transparent.
    ///
    /// Declarations along one chain MERGE by position: a field set later wins, and a field left
    /// unset keeps the earlier value. That is what lets a component state its role while a caller
    /// adds only a content description, and it is why components declare before they append the
    /// caller's modifier (`modifier.semantics(role).then(user_modifier)`) — the caller's declaration
    /// then wins where the two overlap.
    ///
    /// ```ignore
    /// Modifier::new()
    ///     .semantics(SemanticsConfig::new().role(SemanticsRole::Tab))
    ///     .then(Modifier::new().semantics(SemanticsConfig::new().content_description("Home")))
    /// // → role = Tab, description = Home
    /// ```
    pub fn semantics(self, config: crate::semantics::SemanticsConfig) -> Self {
        self.push(ModifierElement::Semantics(config))
    }

    /// Compose's `clearAndSetSemantics { … }`: replace this node's semantics with `config` and hide
    /// its entire subtree from the semantics tree. A config that declares nothing makes the node
    /// (and its subtree) invisible to accessibility, which is the Compose contract.
    pub fn clear_and_set_semantics(self, config: crate::semantics::SemanticsConfig) -> Self {
        self.push(ModifierElement::ClearAndSetSemantics(config))
    }

    /// Compose's `focusGroup()`: Tab treats this subtree as one group and remembers which child was
    /// focused, so re-entering the group restores that child.
    pub fn focus_group(self) -> Self {
        self.push(ModifierElement::FocusGroup)
    }

    /// Compose's `focusProperties { canFocus = … }`: `false` keeps the node out of focus search
    /// (Tab, arrow keys and programmatic focus requests) even when it carries `focusable`.
    pub fn focus_properties_can_focus(self, can_focus: bool) -> Self {
        self.push(ModifierElement::FocusProperties {
            can_focus: Some(can_focus),
        })
    }

    /// Compose's `onFocusChanged { … }`: `true` when this node gains focus, `false` when it loses it.
    pub fn on_focus_changed(self, callback: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::OnFocusChanged {
            callback: std::sync::Arc::new(callback),
        })
    }

    /// Compose's `Modifier.selectableGroup()`
    /// (`foundation/selection/SelectableGroup.kt:30`): one line, `semantics { selectableGroup() }`. Wrap a
    /// group of selectable children (radio buttons, tabs) so a reader announces them as a collection whose
    /// items are this node's children, instead of as unrelated controls.
    pub fn selectable_group(self) -> Self {
        self.semantics(crate::semantics::SemanticsConfig::new().selectable_group())
    }

    /// This chain's merged accessibility declaration: every `semantics` element folded in chain
    /// order, later fields winning over earlier ones.
    pub fn semantics_config(&self) -> crate::semantics::SemanticsConfig {
        self.elements
            .iter()
            .filter_map(|el| match el {
                ModifierElement::Semantics(config) => Some(config),
                _ => None,
            })
            .fold(crate::semantics::SemanticsConfig::default(), |merged, config| {
                config.clone().or(merged)
            })
    }

    /// Whether this chain carries `clearAndSetSemantics`; the measure/report side reads the config.
    pub fn clear_and_set_semantics_config(&self) -> Option<&crate::semantics::SemanticsConfig> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::ClearAndSetSemantics(config) => Some(config),
            _ => None,
        })
    }

    /// Whether this node is a focus traversal group.
    pub fn is_focus_group(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::FocusGroup))
    }

    /// `focusProperties { canFocus }` — `None` when the chain does not say.
    pub fn focus_can_focus(&self) -> Option<bool> {
        self.elements.iter().rev().find_map(|el| match el {
            ModifierElement::FocusProperties { can_focus } => *can_focus,
            _ => None,
        })
    }

    /// 布局方向——本节点 padding start/end 解析 + Row/Column 容器布局方向。
    /// 节点级便捷覆盖；子树级方向切换用
    /// `WiniaTheme::with_theme_and_direction`（CompositionLocal 作用域）。
    pub fn layout_direction(self, d: crate::layout::LayoutDirection) -> Self {
        self.push(ModifierElement::LayoutDirection(d))
    }

    /// Sibling paint order (Compose's `Modifier.zIndex`): a node with a higher `z` paints above its
    /// siblings, and a press that lands on several of them goes to the highest one. Read at paint and
    /// hit-test time, so it needs no layout pass of its own; the layout pass does record whether any
    /// child sets it (`LayoutNode::children_have_z`), which keeps the common case free.
    pub fn z_index(self, z: f32) -> Self {
        self.push(ModifierElement::ZIndex(z))
    }

    /// TextField 容器子节点角色标记（text-field-v2 容器化内部使用——
    /// TextFieldLayout policy 按角色布局）
    pub(crate) fn text_field_slot(self, role: crate::text::field::TextFieldSlotRole) -> Self {
        self.push(ModifierElement::TextFieldSlot { role })
    }

    /// `shadow(elevation, shape, clip, color)` — Compose's `Modifier.shadow`, which is a graphics
    /// layer: `graphicsLayer { shadowElevation = …; shape = …; clip = … }`. Skia's `SkShadowUtils`
    /// then derives the blur and the offset from the elevation as a physical Z height (see
    /// `render.rs::draw_elevation_shadow`) — the path FAB/Button/Card/Chip already take.
    ///
    /// This used to expand the call into winia's own analytic ambient+spot pair, whose alpha
    /// `render.rs` faded a second time; measured on a white surface that shadow was all but
    /// invisible beside the same colour on this path.
    ///
    /// `color` becomes both the ambient and the spot colour, as Compose's single
    /// `DefaultShadowColor` does. Returns `self` unchanged when there is nothing to draw
    /// (`elevation <= 0` and `clip == false`) — Compose's early return.
    pub fn shadow(
        self,
        elevation: f32,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        clip: bool,
        color: Color,
    ) -> Self {
        if elevation <= 0.0 && !clip {
            return self;
        }
        let shape = shape.into();
        let place = move |p: &mut GraphicsLayerParams, shape: Shape| {
            p.shadow_elevation = elevation;
            p.shadow_shape = Some(shape);
            p.clip = clip;
            p.ambient_shadow_color = color;
            p.spot_shadow_color = color;
        };
        // A dynamic shape has to be read at paint time; a static one is applied now, which keeps the
        // layer a plain value instead of a closure.
        if shape.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| {
                let shape = shape.eval();
                place(p, shape);
            })
        } else {
            let value = shape.eval();
            self.merge_graphics_layer(move |p| place(p, value))
        }
    }

    /// `shadow(elevation)`——便捷版（默认形状/黑色/自动 clip）
    pub fn shadow_default(self, elevation: f32) -> Self {
        self.shadow(elevation, Shape::Rectangle, elevation > 0.0, Color::from_argb(255, 0, 0, 0))
    }

    /// `drop_shadow(shape, params)`（对标 Compose `Modifier.dropShadow`）——
    /// 完全自定义单层阴影（radius/spread/offset/color/alpha）。多个
    /// drop_shadow 可叠加（多阴影，Compose vararg 语义）。
    pub fn drop_shadow(
        self,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        params: impl Into<crate::graphics::Dyn<ShadowParams>>,
    ) -> Self {
        self.push(ModifierElement::Shadow {
            params: params.into(),
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
    /// 动画值由 `Visual::set` 写入（不 notify），配合动画引擎每帧 `request_redraw` 实现零重组。
    pub fn background(self, color: impl Into<BackgroundColor>, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        self.push(ModifierElement::Background {
            color: color.into(),
            shape: shape.into(),
        })
    }

    /// Fill the node with a [`crate::graphics::Brush`] — a gradient, or a solid color chosen at
    /// runtime (Compose `Modifier.background(brush, shape)`).
    ///
    /// ```ignore
    /// Modifier::new().background_brush(
    ///     Brush::linear_gradient([colors.primary, colors.tertiary]).diagonal(),
    ///     Shape::rounded(16.0),
    /// )
    /// ```
    ///
    /// Accepts a value or a closure, so a gradient can follow animated state the same way
    /// `background` accepts an animated color. Gradient coordinates are FRACTIONS of the node's
    /// bounds — see the module docs on [`crate::graphics::brush`] for why that differs from Compose.
    pub fn background_brush(
        self,
        brush: impl Into<crate::graphics::BrushSource>,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
    ) -> Self {
        self.push(ModifierElement::BackgroundBrush {
            brush: brush.into(),
            shape: shape.into(),
        })
    }

    /// 统一构造 TextContent 元素（Text/TextField 共用——字段单一来源，P3-6）
    #[allow(dead_code)] // the tests in this file call it
    pub(crate) fn text_content(
        self,
        content: String,
        font_size: f32,
        color: crate::graphics::Color,
        font_weight: crate::text::FontWeight,
        font_style: crate::text::FontSlant,
        max_lines: usize,
        align: crate::text::TextAlign,
        overflow: crate::text::TextOverflow,
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
        self,
        content: String,
        font_size: f32,
        color: crate::graphics::Color,
        font_weight: crate::text::FontWeight,
        font_style: crate::text::FontSlant,
        max_lines: usize,
        align: crate::text::TextAlign,
        overflow: crate::text::TextOverflow,
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

    /// 设置边框。颜色可以是一个 `Color`，也可以是渲染期求值的闭包/状态
    /// （`impl Into<BorderColor>`）——同一个元素存载体：静态值内联，闭包在绘制期求值
    /// （配合 `peek()` 做颜色过渡）。
    ///
    /// One element for both: a changed STATIC colour is a changed parameter (the node re-enters),
    /// while a dynamic one is a parameter read at paint time.
    pub fn border(
        self,
        width: impl Into<Dimension>,
        color: impl Into<crate::graphics::BorderColor>,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
    ) -> Self {
        self.push(ModifierElement::Border {
            width: width.into(),
            color: color.into(),
            shape: shape.into(),
        })
    }

    /// 设置裁剪形状
    pub fn clip(self, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        self.push(ModifierElement::Clip {
            shape: shape.into(),
        })
    }

    /// 内容模糊（GPU saveLayer，零回读）
    pub fn blur(self, radius: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.push(ModifierElement::Blur { radius: radius.into() })
    }

    /// 背景模糊（毛玻璃）——渲染期在节点内容绘制前即时 snapshot+blur
    pub fn backdrop_blur(self, radius: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.push(ModifierElement::BackdropBlur { radius: radius.into() })
    }

    /// 文本输入框容器视觉（TextField 组件内部使用——M3 容器绘制参数）
    pub fn text_field_visual(
        self,
        variant: crate::text::field::TextFieldVariant,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        colors: crate::text::field::TextFieldColors,
        enabled: bool,
        read_only: bool,
        cursor_color: Color,
        indicator_color: crate::runtime::state::State<crate::graphics::Color>,
        focus_progress: crate::runtime::state::State<f32>,
        offset_mapping: Option<std::sync::Arc<dyn crate::text::transformation::OffsetMapping>>,
        supporting: Option<SupportingVisual>,
    ) -> Self {
        self.push(ModifierElement::TextFieldVisual {
            variant,
            shape: shape.into(),
            colors,
            enabled,
            read_only,
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
        offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping>,
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

    /// Compose Desktop's `Modifier.pointerHoverIcon(icon)`: while the pointer is over this node
    /// (or its descendants), the window's cursor is `icon`. A closer descendant setting its own
    /// icon wins; see [`pointer_hover_icon_override_descendants`](Self::pointer_hover_icon_override_descendants).
    pub fn pointer_hover_icon(self, icon: impl Into<crate::input::PointerIcon>) -> Self {
        self.push(ModifierElement::PointerHoverIcon {
            icon: icon.into(),
            override_descendants: false,
        })
    }

    /// Compose Desktop's `pointerHoverIcon(icon, overrideDescendants = true)`: this node's icon
    /// wins over any `pointer_hover_icon` set by a descendant.
    pub fn pointer_hover_icon_override_descendants(
        self,
        icon: impl Into<crate::input::PointerIcon>,
    ) -> Self {
        self.push(ModifierElement::PointerHoverIcon {
            icon: icon.into(),
            override_descendants: true,
        })
    }

    /// 悬停——指针进入/离开自动发射 Hover Enter/Exit（对标 Compose hoverable）
    pub fn hoverable(self, source: &MutableInteractionSource) -> Self {
        self.push(ModifierElement::Hoverable { interaction: source.clone() })
    }

    /// 水波纹指示（对标 Compose `indication` + Material ripple）。
    /// `bounded=true` 时裁剪到节点形状（默认）；`color` 为波纹颜色
    /// （Material 用 content 色——Button 已自动附加，通用 clickable 可手动加）。
    pub fn ripple(
        self,
        source: &MutableInteractionSource,
        color: impl Into<crate::graphics::Dyn<Color>>,
        bounded: bool,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color: color.into(),
            bounded,
            shape: None,
            state_layer_diameter: None,
        })
    }

    /// Ripple indication whose BACKGROUND state layer has an explicit DIAMETER — Compose's
    /// `ripple(bounded, radius)`, where `radius` is half that. material3 hands it `StateLayerSize / 2`: the
    /// checkbox/radio state layer is 40 dp while the node (the touch target) is 48 dp, so the halo cannot be
    /// derived from the node. The spreading foreground layer is unaffected and still grows to the node's
    /// diagonal.
    pub fn ripple_with_state_layer_size(
        self,
        source: &MutableInteractionSource,
        color: impl Into<crate::graphics::Dyn<Color>>,
        bounded: bool,
        diameter: impl Into<crate::graphics::Dyn<f32>>,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color: color.into(),
            bounded,
            shape: None,
            state_layer_diameter: Some(diameter.into()),
        })
    }

    /// 水波纹指示——显式指定裁剪形状（对标 Compose Surface indication：
    /// 容器 shape 同时决定波纹裁剪；Outlined/Text 等无背景元素的按钮
    /// 必须显式传 shape，否则裁剪回退为矩形）
    pub fn ripple_with_shape(
        self,
        source: &MutableInteractionSource,
        color: impl Into<crate::graphics::Dyn<Color>>,
        bounded: bool,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color: color.into(),
            bounded,
            shape: Some(shape.into()),
            state_layer_diameter: None,
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

pub fn draw_icon(self, spec: crate::graphics::IconSpec) -> Self {
        self.push(ModifierElement::DrawIcon { spec })
    }

    /// 图片内容元素（Image 组件用——绘制按 ContentScale/对齐/alpha）
    pub fn image_content(
        self,
        source: crate::graphics::IconSource,
        content_scale: crate::graphics::ContentScale,
        alignment: crate::graphics::ImageAlignment,
        alpha: impl Into<crate::graphics::Dyn<f32>>,
        color_filter: Option<crate::graphics::Dyn<ColorFilter>>,
        filter_quality: FilterQuality,
    ) -> Self {
        self.push(ModifierElement::ImageContent {
            source,
            content_scale,
            alignment,
            alpha: alpha.into(),
            color_filter,
            filter_quality,
        })
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
    /// # use winia::graphics::GraphicsLayerParams;
    /// let _m = Modifier::new().graphics_layer(GraphicsLayerParams { alpha: 0.5, ..Default::default() });
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```ignore
    /// .graphics_layer(|| GraphicsLayerParams { alpha: pulse.peek(), ..Default::default() })
    /// ```
    ///
    /// **原则**：与 `background` 相同——静态传值，动画传闭包（`peek()` 读取，
    /// 配合 `Visual::set` 零重组）。仅影响绘制层，不触发布局。
    pub fn graphics_layer(self, params: impl Into<GraphicsLayerSpec>) -> Self {
        self.push(ModifierElement::GraphicsLayer {
            params: params.into(),
        })
    }

    /// `on_size_changed(f)`（对标 Compose `Modifier.onSizeChanged`）——节点
    /// 测量完成后回调逻辑像素尺寸 `(width, height)`。**尺寸变化才回调**
    /// （去重作用域 = 元素实例：modifier 链每次重组会重建元素并重置去重状态，
    /// 跨帧防重由回调内 `State::set` 的 PartialEq 承担）。窗口 resize/内容变化
    /// 触发重测时回调。回调内可写 State（如动画位移基准）——写相同值会被
    /// 去重，不会引发重组循环。
    pub fn on_size_changed(self, f: impl Fn(f32, f32) + Send + Sync + 'static) -> Self {
        let last: std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>> = Default::default();
        self.push(ModifierElement::OnSizeChanged {
            callback: std::sync::Arc::new(move |w, h| {
                let mut last = last.lock().unwrap();
                if *last != Some((w, h)) {
                    *last = Some((w, h));
                    drop(last);
                    f(w, h);
                }
            }),
        })
    }

    /// 测量完成后调用链上全部 `on_size_changed` 回调（元素内各自去重）
    pub(crate) fn report_measured_size(&self, width: f32, height: f32) {
        for el in &self.elements {
            if let ModifierElement::OnSizeChanged { callback } = el {
                callback(width, height);
            }
        }
    }

    /// Compose's `Modifier.onGloballyPositioned`: the callback receives the node's placed box in
    /// window coordinates (scroll offsets included) whenever that box changes.
    pub fn on_globally_positioned(
        self,
        callback: impl Fn(crate::layout::GlobalPosition) + Send + Sync + 'static,
    ) -> Self {
        self.push(ModifierElement::OnGloballyPositioned {
            callback: std::sync::Arc::new(callback),
        })
    }

    /// Compose's `Modifier.onPlaced`: the same box, delivered before the `on_globally_positioned`
    /// callbacks of the same pass.
    pub fn on_placed(
        self,
        callback: impl Fn(crate::layout::GlobalPosition) + Send + Sync + 'static,
    ) -> Self {
        self.push(ModifierElement::OnPlaced {
            callback: std::sync::Arc::new(callback),
        })
    }

    /// Compose's `Modifier.layoutId`: a tag for this layout node (debug tree and
    /// [`Composer::find_by_layout_id`](crate::runtime::composer::Composer::find_by_layout_id)).
    pub fn layout_id(self, id: impl Into<std::sync::Arc<str>>) -> Self {
        self.push(ModifierElement::LayoutId { id: id.into() })
    }

    /// Compose's `Modifier.bringIntoViewRequester`: attach the handle to this node. A request made
    /// through it scrolls the nearest scrolling ancestors so the node is visible.
    pub fn bring_into_view_requester(
        self,
        requester: &crate::layout::BringIntoViewRequester,
    ) -> Self {
        // Subscribe the enclosing group to the request counter: a request then wakes a frame even
        // when nothing else changed, which is what gets the post-layout pass to serve it.
        requester.subscribe();
        self.push(ModifierElement::BringIntoViewRequester {
            requester: requester.clone(),
        })
    }

    /// Whether this chain carries an `on_globally_positioned` / `on_placed` callback.
    pub(crate) fn has_position_callbacks(&self) -> bool {
        self.elements.iter().any(|el| {
            matches!(
                el,
                ModifierElement::OnGloballyPositioned { .. } | ModifierElement::OnPlaced { .. }
            )
        })
    }

    /// Whether this chain carries a bring-into-view requester (requests may be queued on it).
    pub(crate) fn has_bring_into_view_requester(&self) -> bool {
        self.elements
            .iter()
            .any(|el| matches!(el, ModifierElement::BringIntoViewRequester { .. }))
    }

    /// The chain's `layout_id`, if one was declared.
    pub fn layout_id_value(&self) -> Option<&str> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::LayoutId { id } => Some(&**id),
            _ => None,
        })
    }

    /// `alpha(a)`（对标 Compose `Modifier.alpha`）——透明度便捷包装：
    /// `a != 1.0` 时应用 `graphicsLayer(alpha = a, clip = true)`（**alpha<1
    /// 隐式裁剪到 bounds**——Compose 语义）。已存在 GraphicsLayer 元素时
    /// **合并**（alpha 相乘 + 开 clip）而非叠加嵌套层。
    pub fn alpha(self, alpha: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let alpha = alpha.into();
        if !alpha.is_dynamic() && alpha.eval() == 1.0 {
            return self;
        }
        self.merge_layer_f32(alpha, |p, a| {
            p.alpha *= a;
            p.clip = true;
        })
    }

    /// `rotate(degrees)`（对标 Compose `Modifier.rotate`）——绕**中心**
    /// 顺时针旋转（默认 transformOrigin=Center）。`degrees != 0` 才应用。
    pub fn rotate(self, degrees: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let degrees = degrees.into();
        if !degrees.is_dynamic() && degrees.eval() == 0.0 {
            return self;
        }
        self.merge_layer_f32(degrees, |p, d| p.rotation_z += d)
    }

    /// `scale(sx, sy)`（对标 Compose `Modifier.scale`）——绕**中心**缩放。
    /// 非 1 才应用。
    pub fn scale(
        self,
        sx: impl Into<crate::graphics::Dyn<f32>>,
        sy: impl Into<crate::graphics::Dyn<f32>>,
    ) -> Self {
        let (sx, sy) = (sx.into(), sy.into());
        if !sx.is_dynamic() && !sy.is_dynamic() && sx.eval() == 1.0 && sy.eval() == 1.0 {
            return self;
        }
        if sx.is_dynamic() || sy.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| {
                p.scale_x *= sx.eval();
                p.scale_y *= sy.eval();
            })
        } else {
            let (x, y) = (sx.eval(), sy.eval());
            self.merge_graphics_layer(move |p| {
                p.scale_x *= x;
                p.scale_y *= y;
            })
        }
    }

    /// `rotationX(degrees)`（对标 Compose `Modifier.rotationX`）——绕 X 轴
    /// 3D 旋转（带 cameraDistance 透视；绕 transformOrigin）。`degrees != 0`
    /// 才应用。
    pub fn rotation_x(self, degrees: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let degrees = degrees.into();
        if !degrees.is_dynamic() && degrees.eval() == 0.0 {
            return self;
        }
        self.merge_layer_f32(degrees, |p, d| p.rotation_x += d)
    }

    /// `rotationY(degrees)`（对标 Compose `Modifier.rotationY`）——绕 Y 轴
    /// 3D 旋转（带 cameraDistance 透视；绕 transformOrigin）。
    pub fn rotation_y(self, degrees: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let degrees = degrees.into();
        if !degrees.is_dynamic() && degrees.eval() == 0.0 {
            return self;
        }
        self.merge_layer_f32(degrees, |p, d| p.rotation_y += d)
    }

    /// `cameraDistance(distance)`（对标 Compose `Modifier.cameraDistance`）——
    /// 3D 旋转的相机距离（逻辑 px；越大透视越平，Compose 默认 8.dp）。
    pub fn camera_distance(self, distance: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.merge_layer_f32(distance.into(), |p, d| p.camera_distance = d)
    }

    /// `shadowElevation(elevation)`（对标 Compose `graphicsLayer.shadowElevation`）——
    /// 图层阴影高度（逻辑 px；>0 时在图层内容后画 ambient+spot 阴影）。
    pub fn shadow_elevation(self, elevation: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let elevation = elevation.into();
        if !elevation.is_dynamic() && elevation.eval() <= 0.0 {
            return self;
        }
        self.merge_layer_f32(elevation, |p, e| p.shadow_elevation = e)
    }

    /// 图层阴影形状（对标 Compose `graphicsLayer.shape`——默认矩形）
    pub fn shadow_shape(self, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        let shape = shape.into();
        if shape.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| p.shadow_shape = Some(shape.eval()))
        } else {
            let value = shape.eval();
            self.merge_graphics_layer(move |p| p.shadow_shape = Some(value))
        }
    }

    /// 图层环境光阴影颜色（对标 Compose `ambientShadowColor`）。
    pub fn ambient_shadow_color(self, color: impl Into<crate::graphics::Dyn<Color>>) -> Self {
        self.merge_layer_color(color.into(), |p, c| p.ambient_shadow_color = c)
    }

    /// 图层投射光阴影颜色（对标 Compose `spotShadowColor`）。
    pub fn spot_shadow_color(self, color: impl Into<crate::graphics::Dyn<Color>>) -> Self {
        self.merge_layer_color(color.into(), |p, c| p.spot_shadow_color = c)
    }

    /// 便捷包装合并：已有 GraphicsLayer 元素 → 在其参数上叠加；否则 push 新元素。
    ///
    /// A static parameter set stays static (the mutator is applied now, at build time); a dynamic one
    /// stays a closure (the mutator runs inside it, per paint). The mutators here only assign captured
    /// values, so this loses no dynamism.
    fn merge_graphics_layer(mut self, f: impl Fn(&mut GraphicsLayerParams) + Send + Sync + 'static) -> Self {
        let f = Arc::new(f);
        // mem::take 取出 elements（空 Vec 占位）——局部修改后再放回，
        // 完全避开 self 借用与 push 的冲突
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements
            .iter_mut()
            .find(|e| matches!(e, ModifierElement::GraphicsLayer { .. }))
        {
            if let ModifierElement::GraphicsLayer { params } = el {
                let old = params.clone();
                let f = f.clone();
                *params = match old {
                    crate::graphics::Dyn::Static(mut p) => {
                        f(&mut p);
                        crate::graphics::Dyn::Static(p)
                    }
                    crate::graphics::Dyn::Dynamic(g) => {
                        crate::graphics::Dyn::Dynamic(Arc::new(move || {
                            let mut p = g();
                            f(&mut p);
                            p
                        }))
                    }
                };
            }
        } else {
            let mut p = GraphicsLayerParams::default();
            f(&mut p);
            elements.push(ModifierElement::GraphicsLayer {
                params: crate::graphics::Dyn::Static(p),
            });
        }
        self.elements = elements;
        self
    }

    /// Merge one scalar layer parameter that may be read per paint: a dynamic carrier takes the
    /// per-paint path, a literal is applied now (and keeps the plain, allocation-free value).
    fn merge_layer_f32(
        self,
        value: crate::graphics::Dyn<f32>,
        apply: impl Fn(&mut GraphicsLayerParams, f32) + Send + Sync + 'static,
    ) -> Self {
        if value.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| apply(p, value.eval()))
        } else {
            let v = value.eval();
            self.merge_graphics_layer(move |p| apply(p, v))
        }
    }

    /// The colour twin of [`merge_layer_f32`](Self::merge_layer_f32).
    fn merge_layer_color(
        self,
        value: crate::graphics::Dyn<Color>,
        apply: impl Fn(&mut GraphicsLayerParams, Color) + Send + Sync + 'static,
    ) -> Self {
        if value.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| apply(p, value.eval()))
        } else {
            let v = value.eval();
            self.merge_graphics_layer(move |p| apply(p, v))
        }
    }

    /// Like [`merge_graphics_layer`](Self::merge_graphics_layer), but the mutator runs on EVERY paint:
    /// the element always becomes (or stays) a dynamic layer. Use it when the mutator reads something
    /// that changes per frame (a dynamic shape); a static value keeps the plain form and its no-alloc
    /// path.
    fn merge_graphics_layer_per_paint(
        mut self,
        f: impl Fn(&mut GraphicsLayerParams) + Send + Sync + 'static,
    ) -> Self {
        let f = Arc::new(f);
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements
            .iter_mut()
            .find(|e| matches!(e, ModifierElement::GraphicsLayer { .. }))
        {
            if let ModifierElement::GraphicsLayer { params } = el {
                let old = params.clone();
                let f = f.clone();
                *params = match old {
                    crate::graphics::Dyn::Static(base) => {
                        crate::graphics::Dyn::Dynamic(Arc::new(move || {
                            let mut p = base.clone();
                            f(&mut p);
                            p
                        }))
                    }
                    crate::graphics::Dyn::Dynamic(g) => crate::graphics::Dyn::Dynamic(Arc::new(move || {
                        let mut p = g();
                        f(&mut p);
                        p
                    })),
                };
            }
        } else {
            elements.push(ModifierElement::GraphicsLayer {
                params: crate::graphics::Dyn::Dynamic(Arc::new(move || {
                    let mut p = GraphicsLayerParams::default();
                    f(&mut p);
                    p
                })),
            });
        }
        self.elements = elements;
        self
    }

    /// 垂直滚动（绑定 ScrollState）
    pub fn vertical_scroll(self, state: ScrollState) -> Self {
        // 无 builder 期 get 副作用（State Phase4）：依赖由物化期
        // register_modifier_deps_recursive 统一注册到实际 node slot
        //（composer.rs compose 末尾，take_deps 之前）。builder 期 get 会把
        // 依赖注册到调用 scope，与 slot 注册重复。
        self.push(ModifierElement::VerticalScroll { state })
    }

    /// 水平滚动
    /// 标记为 lazy 滚动容器（LazyColumn 内部使用——内容总高 State）
    pub fn lazy_scroll(self, content_height: crate::runtime::state::Backchannel<f32>) -> Self {
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

    /// Compose's `LazyItemScope.animateItem()`: place this on the item's root. When the lazy list
    /// reorders its data, the item springs from the slot it used to occupy to the new one (keyed by
    /// the item key). Outside a lazy list the marker does nothing.
    pub fn animate_item(self) -> Self {
        self.push(ModifierElement::AnimateItem)
    }

    /// 绑定嵌套滚动连接。连接会在 descendant scrollable 的 pre/post 阶段被调度。
    /// Compose's `clipScrollableContainer(orientation)`: clip the scroll container on its SCROLL
    /// axis while leaving 30 dp of room on the cross axis, so a shadow drawn there survives.
    /// winia always clips its scroll viewport; this modifier makes the clip cross-axis aware.
    pub fn clip_scrollable_container(self, orientation: crate::overscroll::Orientation) -> Self {
        self.push(ModifierElement::ClipScrollableContainer {
            vertical: orientation == crate::overscroll::Orientation::Vertical,
        })
    }

    /// Compose's inner shadow: a rim inside `shape`, painted above the content. `blur` is the mask's
    /// sigma in logical pixels; `offset` moves the cut-out shape, which is what puts the shadow on
    /// one side.
    pub fn inner_shadow(
        self,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        color: Color,
        blur: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> Self {
        self.push(ModifierElement::InnerShadow {
            shape: shape.into(),
            color,
            blur,
            offset_x,
            offset_y,
        })
    }

    /// Compose's `Modifier.overscroll(effect)`: the delta a scroll container cannot consume (it is
    /// at the start or the end) is pulled into the effect instead of falling through to an ancestor,
    /// which translates the content and returns with a short spring.
    pub fn overscroll(self, effect: &crate::overscroll::OverscrollEffect) -> Self {
        self.push(ModifierElement::Overscroll {
            effect: effect.clone(),
        })
    }

    pub fn nested_scroll(self, connection: impl crate::nested_scroll::NestedScrollConnection + 'static) -> Self {
        self.push(ModifierElement::NestedScroll { connection: Arc::new(connection) })
    }

    /// Attach an already-shared nested-scroll connection - for a component that keeps a handle of its
    /// own (the pager's `page_nested_scroll_connection`) and attaches it on every build.
    pub(crate) fn nested_scroll_arc(
        self,
        connection: Arc<dyn crate::nested_scroll::NestedScrollConnection>,
    ) -> Self {
        self.push(ModifierElement::NestedScroll { connection })
    }

    /// 水平滚动（绑定 ScrollState——对齐 vertical_scroll）
    pub fn horizontal_scroll(self, state: ScrollState) -> Self {
        // 无 builder 期 get 副作用（同 vertical_scroll，State Phase4）。
        self.push(ModifierElement::HorizontalScroll { state, reverse: false })
    }

    /// 水平滚动是否反向（RTL：render 滚动平移镜像——offset 0 显示内容末端）
    pub fn is_horizontal_scroll_reverse(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::HorizontalScroll { reverse: true, .. }))
    }

    /// 水平滚动反向标记（RTL：`scroll_reverse` 语义——render 镜像平移；
    /// 同 lazy_scroll_reverse，但作用于 HorizontalScroll 元素）
    pub fn horizontal_scroll_reverse(mut self, reverse: bool) -> Self {
        if let Some(el) = self.elements.last_mut() {
            if let ModifierElement::HorizontalScroll { reverse: r, .. } = el {
                *r = reverse;
            }
        }
        self
    }
}

// ── 查询方法: 布局参数提取 ──


// ── ScrollState ──


/// TextField 支持文本绘制参数（画在容器底部外侧 4dp）
#[derive(Clone, Debug)]
pub struct SupportingVisual {
    pub content: String,
    pub font_size: f32,
    pub font_weight: crate::text::FontWeight,
    pub font_style: crate::text::FontSlant,
    pub letter_spacing: f32,
    pub line_height: Option<f32>,
    pub color: Color,
}

impl SupportingVisual {
    pub fn height(&self) -> f32 {
        4.0 + self.line_height.unwrap_or(self.font_size * 1.4)
    }
}


/// 滚动状态，对齐 Compose ScrollState

#[cfg(test)]
pub(crate) use crate::scroll::state::snap_target;
pub use crate::scroll::state::ScrollState;
// ── FocusRequester ──

static NEXT_FOCUS_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CURRENT_FOCUS_WINDOW: RefCell<u64> = const { RefCell::new(0) };
}

static FOCUS_REQUESTS: std::sync::Mutex<Vec<FocusRequest>> = std::sync::Mutex::new(Vec::new());

#[derive(Debug, Clone)]
struct FocusRequest {
    window_id: u64,
    id: u64,
}

pub(crate) struct FocusWindowGuard {
    prev: u64,
}

impl Drop for FocusWindowGuard {
    fn drop(&mut self) {
        CURRENT_FOCUS_WINDOW.with(|c| c.replace(self.prev));
    }
}

pub(crate) fn focus_window(window_id: u64) -> FocusWindowGuard {
    let prev = CURRENT_FOCUS_WINDOW.with(|c| c.replace(window_id));
    FocusWindowGuard { prev }
}

fn current_focus_window() -> u64 {
    CURRENT_FOCUS_WINDOW.with(|c| *c.borrow())
}

/// 消费所有排队的焦点请求（供 app.rs RedrawRequested 调用）
pub(crate) fn take_focus_requests() -> Vec<u64> {
    let current = current_focus_window();
    let mut req = FOCUS_REQUESTS.lock().unwrap();
    let mut drained = Vec::new();
    let mut kept = Vec::new();
    for item in req.drain(..) {
        if item.window_id == 0 || item.window_id == current {
            drained.push(item.id);
        } else {
            kept.push(item);
        }
    }
    *req = kept;
    drained
}

/// 焦点请求器——可在代码中调用 request_focus() 让关联组件获得焦点
#[derive(Debug, Clone)]
pub struct FocusRequester {
    id: u64,
    window_id: u64,
}

impl FocusRequester {
    pub fn new() -> Self {
        FocusRequester { id: NEXT_FOCUS_ID.fetch_add(1, Ordering::Relaxed), window_id: current_focus_window() }
    }

    pub fn id(&self) -> u64 { self.id }

    /// 请求焦点。无论是否启用 debug-server，都生效。
    /// 焦点将在下一帧 RedrawRequested 时应用。
    pub fn request_focus(&self) {
        FOCUS_REQUESTS.lock().unwrap().push(FocusRequest { window_id: self.window_id, id: self.id });
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
mod tests;

// ── RichSpanStyle ──



mod debug;
mod param_eq;
mod query;

#[cfg(test)]
pub(crate) use param_eq::node_key_of;

#[cfg(test)]
mod param_eq_tests;
