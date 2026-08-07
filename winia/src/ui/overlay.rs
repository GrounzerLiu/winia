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
    /// 外部点击回调
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 弹出内容（独立组合单元）
    pub(crate) content: Box<dyn Fn(&mut crate::core::composer::ComposeCtx)>,
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
    position: PopupPosition,
    offset: (f32, f32),
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Popup {
    pub fn new() -> Self {
        Self {
            position: PopupPosition::BottomLeft,
            offset: (0.0, 4.0),
            on_dismiss: None,
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

    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx, content: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.open_overlay(crate::ui::overlay::OverlayDesc {
            id: id.get(),
            // 锚点 = 当前作用域最后一个兄弟（紧跟其组合位置——Compose Popup 语义）；
            // 无兄弟时 None → 窗口对齐
            anchor_slot: ctx.prev_sibling_slot_key(),
            position: self.position,
            offset: self.offset,
            modal: false,
            dismiss_on_outside: true,
            on_dismiss: self.on_dismiss,
            content: Box::new(content),
        });
    }
}

impl Default for Popup { fn default() -> Self { Self::new() } }

// ═══════════════ Dialog ═══════════════

/// 模态对话框（对标 Compose `Dialog`）——居中 + 遮罩，点击遮罩触发
/// `on_dismiss_request`。
pub struct Dialog {
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_on_outside: bool,
}

impl Dialog {
    pub fn new() -> Self {
        Self {
            on_dismiss: None,
            dismiss_on_outside: true,
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

    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx, content: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.open_overlay(crate::ui::overlay::OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            modal: true,
            dismiss_on_outside: self.dismiss_on_outside,
            on_dismiss: self.on_dismiss,
            content: Box::new(content),
        });
    }
}

impl Default for Dialog { fn default() -> Self { Self::new() } }

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

        if expanded {
            ctx.open_overlay(crate::ui::overlay::OverlayDesc {
                id: id.get(),
                anchor_slot: Some(anchor_slot),
                position: PopupPosition::BottomLeft,
                offset: (0.0, 4.0),
                modal: false,
                dismiss_on_outside: true,
                on_dismiss: self.on_dismiss,
                content: Box::new(menu),
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
