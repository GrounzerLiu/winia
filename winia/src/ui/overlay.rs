//! 顶层弹出层——Popup / Dialog / DropdownMenu（对标 Compose）。
//!
//! 机制：弹出内容**不参与主树布局**——组合期注册 `OverlayDesc` 到 Composer，
//! app.rs 用**独立 Composer** 物化/布局/渲染（渲染在主树之后 = 上层）；
//! 指针命中优先 overlay（最上层先测），点击外部触发 `on_dismiss_request`。
//!
//! 当前限制（v1）：
//! - overlay 内容只支持 clickable（Button/菜单项）——手势/文本选择后续
//! - 单层弹出（嵌套弹出后续）

use std::sync::Arc;
use crate::composable;

/// 弹出定位（对标 Compose `PopupPosition`——相对锚点/窗口）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// 弹出层**进入动画**规格（对标 Compose 内容层 `AnimatedVisibility` 的
/// `enter = scaleIn + fadeIn`——Compose Dialog 本身无内置动画，material2 的
/// 固定 scale+fade 由内容层实现；winia 把动画下沉到 overlay 容器层，统一
/// 帧驱动，不依赖内容层的动画组件）。
///
/// - `scale_from`：起始缩放（默认 0.8——Compose `scaleIn(initialScale=0.8f)`）
/// - `fade`：是否淡入（默认 true——Compose `fadeIn()`）
/// - `slide_from_y`：起始垂直位移（overlay 高度的倍数；负 = 从上方滑入——
///   对标 docked 下拉 `slideIn(initialOffset = { IntOffset(0, -it.height / 2) })`；
///   默认 0.0 = 无位移）
/// - `duration`：时长（默认 200ms）
/// - `interpolator`：缓动曲线（默认 EaseOutCubic——Compose `easeOut`）
///
/// `None`（OverlayDesc.enter_anim / exit_anim = None）= 无对应动画（瞬时出现/
/// 消失——菜单类默认）。
#[derive(Clone)]
pub struct OverlayAnimSpec {
    pub(crate) scale_from: f32,
    pub(crate) fade: bool,
    pub(crate) slide_from_y: f32,
    pub(crate) duration: std::time::Duration,
    pub(crate) interpolator: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>,
}

impl OverlayAnimSpec {
    /// 默认进入动画（scale 0.8→1 + fade，200ms EaseOutCubic——对齐 Compose
    /// material2 Dialog 的经典打开效果）
    pub fn default_enter() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            duration: std::time::Duration::from_millis(200),
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
        }
    }

    /// 默认退出动画（scale 1→0.8 + fade out，200ms EaseInCubic——进入动画的
    /// 反向；对齐 Compose `AnimatedVisibility(exit = scaleOut + fadeOut)`）
    pub fn default_exit() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            duration: std::time::Duration::from_millis(200),
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseInCubic::new()),
        }
    }

    /// 仅缩放（无淡入）
    pub fn scale_only(from: f32, duration: std::time::Duration) -> Self {
        Self { scale_from: from, fade: false, slide_from_y: 0.0, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
    }

    /// 仅淡入
    pub fn fade_only(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
    }

    /// 下拉进入（下滑 + 淡入——对标 docked 下拉 slideIn(-height/2) + fadeIn）
    pub fn slide_down_fade(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: -0.5, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
    }

    /// 下拉滑动（无淡入——对齐上游 docked 下拉 AnimatedVisibility：
    /// 只有 slideIn/slideOut(initialOffset y=-height/2)，alpha 由内容进度
    /// 驱动而非淡入补间；曲线用 EaseOutCubic 干脆收尾——M3 spatial 实为弹簧
    /// （snap 感），emphasized bezier 长尾会有"末段慢吞吞"的拖沓感）
    pub fn slide_down(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: false, slide_from_y: -0.5, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
    }

    /// 自定义缓动曲线
    pub fn with_interpolator(mut self, interp: impl crate::animation::interpolator::Interpolator + 'static) -> Self {
        self.interpolator = std::sync::Arc::new(interp);
        self
    }

    /// 起始缩放（0 附近时对话框从中心放大出现；1.0 = 无缩放）
    pub fn scale_from(mut self, v: f32) -> Self {
        self.scale_from = v;
        self
    }

    /// 起始垂直位移（overlay 高度倍数；-0.5 = 从半高上方滑入）
    pub fn slide_from_y(mut self, v: f32) -> Self {
        self.slide_from_y = v;
        self
    }

    /// 淡入开关
    pub fn fade(mut self, v: bool) -> Self {
        self.fade = v;
        self
    }

    /// 时长
    pub fn duration(mut self, d: std::time::Duration) -> Self {
        self.duration = d;
        self
    }

    /// 动画进度（0..=1）→ (scale, alpha, dy)——渲染期调用（每帧，无 State 依赖）
    /// t=0 起点，t=1 终点（Compose 语义：t 是动画进度）；dy 为 overlay 高度
    /// 倍数（slide_from_y 插值结果——渲染端乘以内容高度换算像素）
    pub(crate) fn apply(&self, t: f32) -> (f32, f32, f32) {
        let e = self.interpolator.interpolate(t.clamp(0.0, 1.0));
        let scale = self.scale_from + (1.0 - self.scale_from) * e;
        let alpha = if self.fade { e } else { 1.0 };
        let dy = self.slide_from_y * (1.0 - e);
        (scale, alpha, dy)
    }

    /// 退出动画进度（1→0 反向）→ (scale, alpha, dy)——t=1 起点（完整显示），
    /// t=0 终点（隐藏）：scale 1→scale_from（缩小）、alpha 1→0（淡出）、
    /// dy 0→slide_from_y（滑回）。
    /// 公式与 apply() 相同（t=1 → e=1 → 完整；t=0 → e=0 → 起点态）——
    /// progress 从 1 动画到 0 即自然反向。
    pub(crate) fn apply_exit(&self, t: f32) -> (f32, f32, f32) {
        self.apply(t)
    }
}

impl Default for OverlayAnimSpec {
    fn default() -> Self { Self::default_enter() }
}

/// 弹出层描述——组合期注册（Popup::build 等内部调用 ctx.open_overlay）
pub struct OverlayDesc {
    /// 稳定 id（组件内部 remember 生成——跨帧匹配复用独立 Composer）
    pub(crate) id: u64,
    /// 锚点节点 slot_key（None = 窗口对齐）
    pub(crate) anchor_slot: Option<u64>,
    /// 相对锚点/窗口的定位
    pub(crate) position: PopupPosition,
    /// 定位后的偏移（逻辑像素）
    pub(crate) offset: (f32, f32),
    /// 模态（Dialog）：渲染遮罩 + 事件捕获（点击外部 dismiss）
    pub(crate) modal: bool,
    /// 点击外部时触发 on_dismiss_request（非模态 Popup 默认 true）
    pub(crate) dismiss_on_outside: bool,
    /// 命中 overlay 内容时**放行主树**（不消费事件）——Tooltip 用：浮层盖住
    /// 锚点时点击锚点仍生效（否则 tooltip 挡住锚点按钮 → 关不了）
    pub(crate) click_passthrough: bool,
    /// 外部点击回调
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 进入动画规格（None = 瞬时出现——Popup/DropdownMenu 默认；
    /// Some = 容器层帧驱动动画——Dialog 默认）
    pub(crate) enter_anim: Option<OverlayAnimSpec>,
    /// 退出动画规格（None = 瞬时消失；Some = 关闭时反向播放——Dialog 默认
    /// 与进入动画对称；对齐 Compose `AnimatedVisibility(exit = ...)`）
    pub(crate) exit_anim: Option<OverlayAnimSpec>,
    /// 弹出内容（独立组合单元）
    pub(crate) content: Box<dyn Fn(&mut crate::core::composer::ComposeCtx)>,
    /// 注册时（主树 provides 内）捕获的 CompositionLocal 快照——overlay 独立
    /// Composer recompose 时重放，`WiniaTheme::colors()` 等读主树主题。
    /// 由 [`crate::core::composer::ComposeCtx::open_overlay`] 自动捕获填充。
    pub(crate) local_snapshot: crate::core::composition_local::LocalSnapshot,
}

/// 顶层弹出 id 分配（组合期 remember 用——稳定跨帧）
pub(crate) fn next_overlay_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ═══════════════ Popup ═══════════════

/// 非模态弹出层（对标 Compose `Popup`）——相对锚点/窗口定位，
/// 点击外部触发 `on_dismiss_request`。锚点 = 调用位置的上一个兄弟节点
/// （如 Demo 中的触发按钮——弹出内容紧跟其后）；无兄弟时窗口对齐。
///
/// ```ignore
/// Popup::new()
///     .position(PopupPosition::BottomLeft)
///     .on_dismiss_request(|| show.set(false))
///     .build(ctx, |ctx| { /* 弹出内容 */ });
/// ```
pub struct Popup {
    visible: bool,
    position: PopupPosition,
    offset: (f32, f32),
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    anchor_slot: Option<u64>,
    enter_anim: Option<OverlayAnimSpec>,
    exit_anim: Option<OverlayAnimSpec>,
}

impl Popup {
    /// ⚠ visible 参数化（对齐 DropdownMenu::new(expanded)）：build **总执行**
    /// 并记录 active 状态——sync_overlays 用"active=false"删除 overlay（主动
    /// 关闭），用"本帧无记录"保留（注册方 Skip）。若调用方用 `if` 包裹（build
    /// 不执行），Skip 帧与主动关闭在 slot 层不可区分 → 无法正确删除/保留。
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            position: PopupPosition::BottomLeft,
            offset: (0.0, 4.0),
            on_dismiss: None,
            anchor_slot: None,
            enter_anim: None,
            exit_anim: None,
        }
    }

    pub fn position(mut self, p: PopupPosition) -> Self {
        self.position = p;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// 进入动画（默认 None = 瞬时出现，菜单类语义；下拉用 slide_down_fade）
    pub fn enter_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.enter_anim = anim;
        self
    }

    /// 退出动画（默认 None = 瞬时消失）
    pub fn exit_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.exit_anim = anim;
        self
    }

    /// Explicit anchor slot override. Needed because `#[composable]` pushes a
    /// fresh scope at `build` entry, so the default `prev_sibling_slot_key()`
    /// capture inside `build` always sees an empty scope (None) and the popup
    /// falls back to window alignment. Callers that need anchoring must capture
    /// `ctx.prev_sibling_slot_key()` in their own scope and pass it here:
    /// ```ignore
    /// Surface::new().build(ctx, |ctx| { /* anchor */ });
    /// let anchor = ctx.prev_sibling_slot_key();
    /// Popup::new(true).anchor_slot(anchor).build(ctx, |ctx| { /* ... */ });
    /// ```
    pub fn anchor_slot(mut self, slot: Option<u64>) -> Self {
        self.anchor_slot = slot;
        self
    }

    /// #[composable]：内部 remember（overlay id）从 build 调用点取稳定 base。
    /// build 总执行（visible=false 也执行）——记录 active=false 供 sync 删除。
    #[composable]
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx, content: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return; // 关闭：不注册 overlay——sync 按 active=false 删除
        }
        // Explicit anchor wins; otherwise fall back to prev-sibling capture
        // (None inside build scope — kept for non-composable callers).
        let anchor = self.anchor_slot.or_else(|| ctx.prev_sibling_slot_key());
        ctx.open_overlay(crate::ui::overlay::OverlayDesc {
            id: id.get(),
            // 锚点 = 调用方显式传入（见 anchor_slot）；默认当前作用域最后一个
            // 兄弟——build 自有 scope 内恒为 None → 窗口对齐
            anchor_slot: anchor,
            position: self.position,
            offset: self.offset,
            modal: false,
            dismiss_on_outside: true,
            click_passthrough: false,
            on_dismiss: self.on_dismiss,
            enter_anim: self.enter_anim,
            exit_anim: self.exit_anim,
            content: Box::new(content),
            local_snapshot: Vec::new(),
        });
    }
}

impl Default for Popup { fn default() -> Self { Self::new(false) } }

// ═══════════════ Dialog ═══════════════

/// 模态对话框（对标 Compose `Dialog`）——居中 + 遮罩，点击遮罩触发
/// `on_dismiss_request`。
pub struct Dialog {
    visible: bool,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_on_outside: bool,
    enter_anim: Option<OverlayAnimSpec>,
    exit_anim: Option<OverlayAnimSpec>,
}

impl Dialog {
    /// ⚠ visible 参数化（同 Popup）：build 总执行并记录 active——sync 按
    /// active=false 删除（主动关闭），无记录保留（注册方 Skip）。
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            on_dismiss: None,
            dismiss_on_outside: true,
            // 默认进入/退出动画（scale 0.8→1 + fade——Compose material2 Dialog
            // 经典效果；关闭反向播放）
            enter_anim: Some(OverlayAnimSpec::default_enter()),
            exit_anim: Some(OverlayAnimSpec::default_exit()),
        }
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// 点击遮罩是否关闭（默认 true——Compose Dialog 默认 dismissOnClickOutside=true）
    pub fn dismiss_on_outside(mut self, v: bool) -> Self {
        self.dismiss_on_outside = v;
        self
    }

    /// 自定义**进入**动画（默认 scale 0.8→1 + fade 200ms EaseOutCubic）。
    /// 传 `None` = 无进入动画（瞬时出现）。对标 Compose 内容层
    /// `AnimatedVisibility(enter = scaleIn(...) + fadeIn(...))`——winia 把
    /// 动画下沉到 overlay 容器层统一帧驱动。
    pub fn enter_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.enter_anim = anim;
        self
    }

    /// 自定义**退出**动画（默认 = 进入动画反向：scale 1→0.8 + fade out，
    /// 200ms EaseInCubic）。传 `None` = 无退出动画（瞬时消失）。对标 Compose
    /// `AnimatedVisibility(exit = scaleOut(...) + fadeOut(...))`。
    pub fn exit_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.exit_anim = anim;
        self
    }

    /// 关闭进入/退出动画（瞬时出现/消失）
    pub fn no_animation(self) -> Self {
        self.enter_animation(None).exit_animation(None)
    }

    /// #[composable]：内部 remember（overlay id）从 build 调用点取稳定 base。
    /// build 总执行（visible=false 也执行）——记录 active=false 供 sync 删除。
    #[composable]
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx, content: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return; // 关闭：不注册 overlay——sync 按 active=false 删除
        }
        ctx.open_overlay(crate::ui::overlay::OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            modal: true,
            dismiss_on_outside: self.dismiss_on_outside,
            click_passthrough: false,
            on_dismiss: self.on_dismiss,
            enter_anim: self.enter_anim,
            exit_anim: self.exit_anim,
            content: Box::new(content),
            local_snapshot: Vec::new(),
        });
    }
}

impl Default for Dialog { fn default() -> Self { Self::new(false) } }

// ═══════════════ DropdownMenu ═══════════════

/// 下拉菜单（对标 Compose `DropdownMenu`）——锚定触发容器展开菜单列表，
/// 点击外部收起。
///
/// ```ignore
/// let expanded = ctx.remember(|| false);
/// DropdownMenu::new(expanded.clone())
///     .build(ctx,
///         |ctx| { Button::new().on_click(|| expanded.set(true)).build(...) },  // 锚点
///         |ctx| {  // 菜单项
///             DropdownMenuItem::new("选项 A").on_click(|| ...).build(ctx);
///         });
/// ```
pub struct DropdownMenu {
    expanded: crate::core::state::State<bool>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl DropdownMenu {
    pub fn new(expanded: crate::core::state::State<bool>) -> Self {
        Self {
            expanded,
            on_dismiss: None,
        }
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// #[composable]：内部 remember（overlay id）/next_key（锚点容器）
    /// 从 build 调用点取稳定 base
    #[composable]
    pub fn build(
        self,
        ctx: &mut crate::core::composer::ComposeCtx,
        anchor: impl FnOnce(&mut crate::core::composer::ComposeCtx),
        menu: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static,
    ) {
        let expanded = self.expanded.get(); // 注册依赖——expanded 变化触发重组
        // 锚点容器（普通组合——挂主树；菜单锚定其位置）
        let anchor_key = ctx.next_key();
        let modifier = crate::modifier::Modifier::new();
        let id = ctx.remember(|| next_overlay_id());
        match ctx.start_restartable_group(anchor_key, modifier, crate::layout::box_layout::BoxLayout::new()) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                anchor(ctx);
            }
        }
        let anchor_slot = ctx.composer_slot_key(); // 容器 slot_key（锚点）
        ctx.end_restartable_group();

        // build 总执行（expanded 参数化）——记录 active 供 sync 删除（对齐
        // Popup/Dialog 的 visible 参数化：expanded=false 时记录 false → 删除）
        ctx.record_overlay_active(id.get(), expanded);
        if expanded {
            ctx.open_overlay(crate::ui::overlay::OverlayDesc {
                id: id.get(),
                anchor_slot: Some(anchor_slot),
                position: PopupPosition::BottomLeft,
                offset: (0.0, 4.0),
                modal: false,
                dismiss_on_outside: true,
                click_passthrough: false,
                on_dismiss: self.on_dismiss,
                enter_anim: None, // DropdownMenu 默认无进入动画
                exit_anim: None, // DropdownMenu 默认无退出动画
                content: Box::new(menu),
                local_snapshot: Vec::new(),
            });
        }
    }
}

// ═══════════════ DropdownMenuItem ═══════════════

/// 下拉菜单项——文本 + 点击回调
pub struct DropdownMenuItem {
    text: String,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
}

impl DropdownMenuItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_click: None,
            enabled: true,
        }
    }

    pub fn on_click(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(cb));
        self
    }

    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }

    /// #[composable]：与 Popup/Dialog 同契约（组合单元统一标记）
    #[composable]
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx) {
        let modifier = crate::modifier::Modifier::new()
            .size(160.0, 36.0)
            .padding(crate::modifier::SizeValue::Static(crate::modifier::Dimension::Fixed(12.0)))
            .background(
                crate::modifier::Color::from_argb(255, 250, 250, 250),
                crate::modifier::Shape::RoundedRect { corner_radius: 4.0 },
            );
        let on_click = self.on_click;
        let modifier = if self.enabled {
            modifier.clickable(move || {
                if let Some(cb) = &on_click {
                    (cb)();
                }
            })
        } else {
            modifier
        };
        let text = self.text;
        crate::ui::Column::new()
            .modifier(modifier)
            .build(ctx, |ctx| {
                crate::ui::Text::new(text)
                    .font_size(13.0)
                    .color(if self.enabled {
                        crate::modifier::Color::from_argb(255, 60, 60, 60)
                    } else {
                        crate::modifier::Color::from_argb(120, 160, 160, 160)
                    })
                    .build(ctx);
            });
    }
}
