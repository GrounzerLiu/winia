//! Tooltip — 提示框（对齐 Jetpack Compose material3 TooltipBox + PlainTooltip/RichTooltip）
//!
//! 结构（对齐 Compose TooltipBox）：锚点 content + tooltip 浮层——锚点挂
//! hover 交互（进入显示、离开隐藏），浮层经 overlay 机制定位在锚点上方。
//!
//! 两种内容模式：
//! - `Tooltip::new(text)`：Plain 样式（inverse_surface 容器 + inverse_on_surface
//!   文字 + 8dp 圆角 + 8dp padding + 180dp 最大宽折行）——M3 plain tooltip
//! - `.content(|ctx| ...)`：自定义内容（Rich tooltip 等）——M3 rich tooltip
//!   样式（surface_container 容器 + on_surface_variant）由调用方构造
//!
//! 触发：
//! - 默认 hover（锚点挂 hoverable，进入显示/离开隐藏）
//! - `.visible(State<bool>)` 外部控制合并（hover 或外部 true 都显示）

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::modifier::Modifier;

/// 提示框（对齐 Compose `TooltipBox`）。
pub struct Tooltip {
    /// 外部可见性控制（None = 仅 hover 触发）
    external_visible: Option<crate::core::state::State<bool>>,
    /// 内容：None = 文本 Plain 样式；Some = 自定义内容闭包
    text: Option<String>,
    content: Option<Box<dyn Fn(&mut ComposeCtx) + 'static>>,
    position: crate::ui::overlay::PopupPosition,
    offset: (f32, f32),
    /// 是否启用 hover 触发（默认 true）
    hover_trigger: bool,
}

impl Tooltip {
    /// Plain 提示框（text 显示在锚点上方）
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            external_visible: None,
            text: Some(text.into()),
            content: None,
            position: crate::ui::overlay::PopupPosition::TopCenter,
            offset: (0.0, 8.0),
            hover_trigger: true,
        }
    }

    /// 自定义内容（Rich tooltip——标题/正文/按钮由调用方构造）
    pub fn content(mut self, f: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.text = None;
        self.content = Some(Box::new(f));
        self
    }

    /// 外部可见性控制（与 hover 合并：任一 true 显示）
    pub fn visible(mut self, v: crate::core::state::State<bool>) -> Self {
        self.external_visible = Some(v);
        self
    }

    /// 定位（默认 TopCenter——锚点上方居中）
    pub fn position(mut self, p: crate::ui::overlay::PopupPosition) -> Self {
        self.position = p;
        self
    }

    /// 定位后偏移（默认 (0, 8)——与锚点 8dp 间距）
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// 禁用 hover 触发（仅外部 visible 控制）
    pub fn no_hover(mut self) -> Self {
        self.hover_trigger = false;
        self
    }

    /// 锚点 + 浮层组合（锚点 content 挂 hover 交互；浮层经 overlay 定位）
    #[composable]
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        anchor: impl FnOnce(&mut ComposeCtx),
    ) {
        // 锚点容器（普通组合——挂主树；tooltip 锚定其位置，对齐 DropdownMenu）
        let anchor_key = ctx.next_key();
        let id = ctx.remember(|| crate::ui::overlay::next_overlay_id());
        // hover 交互源：锚点挂 hoverable，进入/离开自动发射 hover 事件
        let interaction = ctx.remember(|| crate::ui::interaction::MutableInteractionSource::new()).get();
        let mut anchor_modifier = Modifier::new();
        if self.hover_trigger {
            anchor_modifier = anchor_modifier.hoverable(&interaction);
        }
        match ctx.start_restartable_group(anchor_key, anchor_modifier, crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                anchor(ctx);
            }
        }
        let anchor_slot = ctx.composer_slot_key();
        ctx.end_restartable_group();

        // 可见性：hover 状态（注册依赖——hover 进出触发重组）+ 外部控制合并
        let hovered = interaction.is_hovered();
        let external = self.external_visible.as_ref().map(|s| s.get()).unwrap_or(false);
        let show = self.hover_trigger && hovered || external;

        // build 总执行（show 参数化）——记录 active 供 sync 删除（对齐
        // Popup/Dialog：show=false 记录 false → 删除；true 注册 overlay）
        ctx.record_overlay_active(id.get(), show);
        if show {
            let content = if let Some(text) = self.text.as_ref() {
                let text = text.clone();
                Box::new(move |ctx: &mut ComposeCtx| plain_tooltip_content(ctx, &text))
                    as Box<dyn Fn(&mut ComposeCtx) + 'static>
            } else {
                self.content.expect("Tooltip 无内容")
            };
            ctx.open_overlay(crate::ui::overlay::OverlayDesc {
                id: id.get(),
                anchor_slot: Some(anchor_slot),
                position: self.position,
                offset: self.offset,
                modal: false,
                dismiss_on_outside: false,
                on_dismiss: None,
                content: Box::new(content),
            });
        }
    }
}

/// Plain tooltip 内容（M3 plain tooltip specs）：
/// - 容器：inverse_surface + 8dp 圆角 + 8dp padding + 180dp 最大宽折行
/// - 文字：inverse_on_surface + 14sp + 4 行
///
/// ⚠ overlay 内容在**独立 composer** 中组合，闭包不在 #[composable] 注入内——
/// 用固定 `ctx.key` 包裹（不能用 next_key()：会 panic「无法获得稳定 key」；
/// 每帧 recompose 重跑，固定 key 保证 slot 稳定复用）。
fn plain_tooltip_content(ctx: &mut ComposeCtx, text: &str) {
    ctx.key(0, |ctx| {
        let theme = crate::ui::theme::WiniaTheme::colors();
        let shape = crate::modifier::Shape::RoundedRect { corner_radius: 8.0 };
        let m = Modifier::new()
            .background(theme.inverse_surface, shape)
            .padding(8.0)
            .width(180.0);
        let sk = ctx.next_key();
        match ctx.start_restartable_group(sk, m, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                crate::ui::theme::WiniaTheme::with_content_color(theme.inverse_on_surface, ctx, |ctx| {
                    crate::ui::Text::new(text)
                        .font_size(14.0)
                        .max_lines(4)
                        .build(ctx);
                });
            }
        }
        ctx.end_restartable_group();
    });
}
