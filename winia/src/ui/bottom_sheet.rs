//! `ModalBottomSheet`——底部模态面板（完整复刻 Compose Material3
//! `ModalBottomSheet` 语义）。
//!
//! 结构（对齐 Compose `ModalBottomSheet.kt` + `BottomSheet.kt`）：
//! - **ModalBottomSheetDialog**：winia 用 `open_overlay`（modal 遮罩 +
//!   全屏内容）承载
//! - **Scrim**：overlay `modal` 自动渲染半透明遮罩（淡入淡出）
//! - **BottomSheet 面板**：`SheetState` 驱动 offset → `graphics_layer`
//!   `translation_y` 实现上下滑动（Hidden/PartiallyExpanded/Expanded 三态锚点）
//! - **dragHandle**：顶部把手（M3 视觉特征）
//! - **拖拽**：`sheet_gestures_enabled`——面板 `on_drag` 增量喂入
//!   `SheetState::drag_delta`，`on_drag_end` 吸附
//!
//! 锚点计算（对齐 Compose `BottomSheetImpl` 的 `draggableAnchors`）：
//! - `Hidden at fullHeight`（面板顶滑出视口底部）
//! - `PartiallyExpanded at fullHeight - min(fullHeight/2, sheetHeight)`
//! - `Expanded at max(0, fullHeight - sheetHeight)`
//!
//! 与 Compose 的差异（winia 降级）：
//! - 无 suspend——`show/hide/expand/partial_expand` 用 `push_animatable`
//!   动画驱动（非挂起）
//! - `onDismissRequest`：点击遮罩触发 sheet 下滑动画 + 回调（overlay 移除时
//!   exit fade 与下滑重叠——非严格"动画完成后再回调"）
//! - `sheetMaxWidth` 已对齐（默认 `640.dp`，`Dp::from(f32::INFINITY)` 表铺满，居中）
//! - 拖拽 velocity 用内部估算（winia 拖拽事件无原生 velocity）

use std::sync::Arc;
use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::overlay::{next_overlay_id, OverlayAnimSpec, OverlayDesc, PopupPosition};
use crate::ui::sheet_state::{SheetState, SheetValue};
use crate::unit::Dp;

/// 默认 sheet 顶部圆角（M3 `BottomSheetDefaults.ExpandedShape` 28dp）
pub const SHEET_TOP_CORNER_RADIUS: f32 = 28.0;

/// 底部模态面板（对标 Compose Material3 `ModalBottomSheet`）。
pub struct ModalBottomSheet {
    on_dismiss_request: Option<Arc<dyn Fn() + Send + Sync>>,
    sheet_state: Option<SheetState>,
    /// 是否启用拖拽手势（Compose `sheetGesturesEnabled`，默认 true）
    sheet_gestures_enabled: bool,
    /// 面板顶部圆角
    corner_radius: f32,
    /// 面板背景色（默认主题 surface）
    container_color: Option<Color>,
    /// 是否显示顶部把手（默认 true）
    drag_handle: bool,
    /// 最大宽度（对标 `sheetMaxWidth = 640.dp`，`Dp::from(f32::INFINITY)` 表 `Unspecified` 铺满）
    sheet_max_width: Option<Dp>,
    /// 是否跳过半展开锚点（对齐 Compose `sheetState: rememberModalBottomSheetState(skipPartiallyExpanded=...)`）
    skip_partially_expanded: bool,
    /// 是否可见（true 时注册 overlay）
    visible: bool,
}

impl ModalBottomSheet {
    pub fn new(visible: bool) -> Self {
        Self {
            on_dismiss_request: None,
            sheet_state: None,
            sheet_gestures_enabled: true,
            corner_radius: SHEET_TOP_CORNER_RADIUS,
            container_color: None,
            drag_handle: true,
            sheet_max_width: Some(Dp(640.0)),
            skip_partially_expanded: false,
            visible,
        }
    }

    /// 点击遮罩回调（对标 Compose `onDismissRequest`——点击外部后触发。
    /// Compose 先动画到 Hidden 再回调；winia 触发 sheet 下滑动画 + 回调）
    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss_request = Some(Arc::new(cb));
        self
    }

    /// 显式传入 SheetState（对标 Compose `sheetState`；不传则内部 remember）
    pub fn sheet_state(mut self, s: SheetState) -> Self {
        self.sheet_state = Some(s);
        self
    }

    /// 是否启用拖拽手势
    pub fn sheet_gestures_enabled(mut self, v: bool) -> Self {
        self.sheet_gestures_enabled = v;
        self
    }

    /// 面板顶部圆角
    pub fn corner_radius(mut self, r: f32) -> Self {
        self.corner_radius = r;
        self
    }

    /// 面板背景色（默认主题 surface）
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// 是否显示顶部把手
    pub fn drag_handle(mut self, v: bool) -> Self {
        self.drag_handle = v;
        self
    }

    /// 最大宽度（对标 `sheetMaxWidth`，默认 `640.dp`，`Dp(f32::INFINITY)` 表铺满）
    pub fn sheet_max_width(mut self, w: Dp) -> Self {
        if w.0.is_infinite() {
            self.sheet_max_width = None;
        } else {
            self.sheet_max_width = Some(w);
        }
        self
    }

    /// 是否跳过半展开锚点（对齐 Compose `rememberModalBottomSheetState(skipPartiallyExpanded)`）。
    /// true：面板只在 Expanded 与 Hidden 之间切换（无 Partial 中间态，下滑直接折到关闭）。
    pub fn skip_partially_expanded(mut self, skip: bool) -> Self {
        self.skip_partially_expanded = skip;
        self
    }

    /// #[composable]：`visible` 为 true 时注册 overlay（模态底部面板）。
    /// 面板外壳（底部对齐 + 圆角 + 把手 + offset 滑动 + 拖拽）由本组件
    /// 自动包装——用户只需提供面板主体内容。
    /// ⚠ `visible` 参数化（对齐 Popup/Dialog）：build 总执行并记录 active——
    /// `sync_overlays` 按 active=false 删除 overlay（主动关闭），无记录保留（Skip）
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        // ⚠ 这两个 remember 必须在 `if !visible {return}` 之前——否则 visible=false
        // 时不执行，`prev_visible` 永远卡在 true，第二次 visible=true 时边沿检测
        // `!prev_visible` 为 false → 不再 show() → 面板停在 Hidden(560) 不显示。
        // 放在记录前保证 Slot 稳定且每帧更新。
        let prev_visible: State<bool> = ctx.remember(|| false);
        let holder = ctx.remember(|| SheetState::new(SheetValue::Hidden));
        let should_show = self.visible && !prev_visible.get();
        ctx.record_overlay_active(id.get(), self.visible);
        prev_visible.set(self.visible);
        if !self.visible {
            return;
        }
        // SheetState：外部传入或内部 remember（holder 在上方已稳定 remember）
        let sheet_state = match self.sheet_state {
            Some(s) => s,
            None => holder.get(),
        };
        // 应用 skipPartiallyExpanded（每次 build 刷新——方便外部在 show 前设置；
        // SheetState.set_skip_partially_expanded 是在 update_anchors 时生效）
        sheet_state.set_skip_partially_expanded(self.skip_partially_expanded);
        if should_show {
            sheet_state.show();
        }
        let gestures = self.sheet_gestures_enabled;
        let radius = self.corner_radius;
        let container_color = self.container_color;
        let drag_handle = self.drag_handle;
        let sheet_max_width = self.sheet_max_width; // 对齐 Compose sheetMaxWidth=640.dp，平板居中；手机 480<640 时铺满
        let on_dismiss_req = self.on_dismiss_request;
        // content 闭包也要用 on_dismiss——clone 一份（map 会 move 走原值）
        let content_dismiss = on_dismiss_req.clone();
        ctx.open_overlay(OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            position: PopupPosition::Center, // 面板 fill_max_size 占满 overlay；内容 Stack(End) 贴底
            offset: (0.0, 0.0),
            modal: true,
            dismiss_on_outside: true,
            click_passthrough: false,
            on_dismiss: on_dismiss_req.map(|cb| {
                let st = sheet_state.clone();
                let f: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                    // 仅 hide，visible=false 由内部 `shown && settled==Hidden` 观察者
                    // 在 hide 动画完成后触发（保证下滑可见；之前 hide+cb 立即重叠
                    // 且 closing 跳过 recompose 导致下滑不可见，仅淡出）
                    st.hide();
                    let _ = cb; // 保留 cb 供观察者用（on_dismiss 仅 hide）
                });
                f
            }),
            enter_anim: Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(200))),
            exit_anim: Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(200))),
            content: Box::new(move |ctx| {
                let theme = crate::ui::theme::WiniaTheme::colors();
                // 对齐 Compose BottomSheetDefaults.ContainerColor = surfaceContainerLow
                // + ExpandedShape 28dp + Elevation 1dp（SheetBottomTokens.DockedModalContainerElevation）
                let bg = container_color.unwrap_or(theme.surface_container_low);
                let st = sheet_state.clone();
                let anchors_st = sheet_state.clone();
                let drag_st = sheet_state.clone();
                let dismiss_cb = content_dismiss.clone();
                let sheet_h: State<f32> = ctx.remember(|| 0.0);
                // 仅全屏（sheetH≈fullH）时 28→0 过渡，非全屏保持 28（M3 满屏变直角）
                // progress 基准用 Partial→Expanded 才能半展开保持 28（Hidden→Expanded 会在 Partial 已掉角）
                let full_h = crate::ui::window_size().1;
                let is_full = sheet_h.get() >= full_h - 1.0;
                let from = if st.has_partially_expanded_state() {
                    SheetValue::PartiallyExpanded
                } else {
                    SheetValue::Hidden
                };
                let p = st.progress(from, SheetValue::Expanded);
                let off = st.offset();
                let cur_radius = if is_full {
                    if off.is_nan() { radius } else { radius * (1.0 - p.clamp(0.0, 1.0)) }
                } else {
                    radius
                };
                let cur_shape = Shape::TopRoundedRect { radius: cur_radius };
                // 拖拽/动画到 Hidden → 触发 on_dismiss_request（对齐 Compose
                // `if (!state.isVisible) onDismissRequest()`）。用 remember + once 门闩
                // 保证 cb 仅在 settled 到 Hidden 的首帧触发一次（之前每帧 settled==Hidden
                // 都会 cb()，父 pop 两次）
                let shown: State<bool> = ctx.remember(|| false);
                let fired: State<bool> = ctx.remember(|| false);
                if sheet_state.settled_value() != SheetValue::Hidden {
                    shown.set(true);
                    fired.set(false);
                }
                if shown.get()
                    && sheet_state.settled_value() == SheetValue::Hidden
                    && !fired.get()
                {
                    fired.set(true);
                    if let Some(cb) = &dismiss_cb {
                        (cb)();
                    }
                }
                // 全屏容器：Scrim（可点击关闭）+ 面板（offset 定位——面板布局在
                // 顶部，translation_y = offset 推下：Expanded=fullHeight-sheetHeight
                // 贴底，Hidden=fullHeight 滑出视口。⚠ 不能用 Stack(End) 贴底——
                // 会与 offset 双重偏移，面板被推到屏幕外）
                crate::ui::layout_components::Stack::new()
                    .modifier(crate::modifier::Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        // Scrim 层：占满全屏、透明、点击关闭（面板之上由 overlay
                        // 自动画遮罩；此层只捕获点击——Compose Scrim 语义）
                        // 仅 hide，下滑完成后由 `shown && settled==Hidden` 观察者
                        // 触发 onDismiss（保证下滑可见；之前 hide+cb 立即
                        // visible=false 导致 closing 冻结，下滑不可见仅淡出）
                        if dismiss_cb.is_some() {
                            let st_hide = st.clone();
                            crate::ui::layout_components::Stack::new()
                                .modifier(Modifier::new()
                                    .fill_max_size()
                                    .clickable(move || {
                                        st_hide.hide();
                                    }))
                                .build(ctx, |_| {});
                        }
                        // 面板（布局在顶部，offset 推下——用布局 offset 而非
                        // graphics_layer：布局位置与渲染一致，hit_test 命中正确）
                        // ⚠ graphics_layer 位移不参与 hit_test → 面板布局在顶部、
                        // 渲染在底部，点击命中错位（面板外点不到 Scrim）。
                        // sheetMaxWidth 640.dp 平板居中（Compose 语义），手机 480 铺满
                        let window_w = crate::ui::window_size().0;
                        let max_w_px = sheet_max_width.map(|dp| dp.to_px(crate::unit::current_density()));
                        let sheet_w = max_w_px.map(|w| w.min(window_w)).unwrap_or(window_w);
                        let sheet_pad_x = (window_w - sheet_w) / 2.0;
                        let mut panel_mod = Modifier::new()
                            .width(sheet_w)
                            .offset(sheet_pad_x, st.offset_state())
                            .shadow(
                                1.0,
                                cur_shape,
                                false,
                                Color::from_argb(40, 0, 0, 0),
                            )
                            .background(bg, cur_shape)
                            .clip(cur_shape);
                        // 高度上报 → 更新锚点（window_size 需在回调内重读，捕获值 resize 后 stale）
                        let up_st = anchors_st.clone();
                        let sheet_h_for_size = sheet_h.clone();
                        panel_mod = panel_mod.on_size_changed(move |_w, h| {
                            sheet_h_for_size.set(h);
                            let mut s = up_st.clone();
                            s.update_anchors(crate::ui::window_size().1, h);
                        });
                        let mut panel_mod_with_nested = panel_mod;
                        if gestures {
                            #[derive(Clone)]
                            struct SheetNested {
                                st: crate::ui::sheet_state::SheetState,
                            }
                            impl SheetNested {
                                /// sheet 实际消费的 deltay（offset 前后差——drag_delta
                                /// clamp 到锚点[min,max]，已在 Expanded 时 offset 不动
                                /// → consumed=0，剩余自动放行给列表）
                                fn consume(
                                    st: &crate::ui::sheet_state::SheetState,
                                    delta_y: f32,
                                ) -> crate::nested_scroll::ScrollDelta {
                                    if delta_y == 0.0 {
                                        return crate::nested_scroll::ScrollDelta::ZERO;
                                    }
                                    let before = st.offset();
                                    st.drag_delta(delta_y);
                                    let consumed = st.offset() - before;
                                    crate::nested_scroll::ScrollDelta::new(0.0, consumed)
                                }
                            }
                            impl crate::nested_scroll::NestedScrollConnection for SheetNested {
                                fn on_pre_scroll(
                                    &self,
                                    available: crate::nested_scroll::ScrollDelta,
                                    _source: crate::nested_scroll::NestedScrollSource,
                                ) -> crate::nested_scroll::ScrollDelta {
                                    // 对齐 Compose M3：向上拖(available.y<0)先展开 sheet
                                    // （expands-first），列表只在 sheet 已到 Expanded 后滚动。
                                    if available.y >= 0.0 {
                                        return crate::nested_scroll::ScrollDelta::ZERO;
                                    }
                                    Self::consume(&self.st, available.y)
                                }
                                fn on_post_scroll(
                                    &self,
                                    _consumed: crate::nested_scroll::ScrollDelta,
                                    available: crate::nested_scroll::ScrollDelta,
                                    _source: crate::nested_scroll::NestedScrollSource,
                                ) -> crate::nested_scroll::ScrollDelta {
                                    // 向下拖/列表到顶后的剩余增量（折叠/关闭方向）。
                                    // 用 consumed 度量——avoid over-consumption
                                    Self::consume(&self.st, available.y)
                                }
                                fn on_post_fling(
                                    &self,
                                    _consumed: crate::nested_scroll::ScrollVelocity,
                                    available: crate::nested_scroll::ScrollVelocity,
                                ) -> crate::nested_scroll::ScrollVelocity {
                                    // 向下滑松手：dispatch 传 available.y = -vy（手指向下→负）。
                                    // sheet 折叠（收向 Hidden）需正 velocity（offset 增大），故取反。
                                    // ⚠ 直接用 available.y 会得到负 velocity → 朝 Expanded → 回弹。
                                    self.st.settle_with_velocity(-available.y);
                                    crate::nested_scroll::ScrollVelocity::default()
                                }
                                fn on_pre_fling(
                                    &self,
                                    available: crate::nested_scroll::ScrollVelocity,
                                ) -> crate::nested_scroll::ScrollVelocity {
                                    // 方向约定：dispatch 传 available.y = -vy（手指向上→正）。
                                    // sheet 展开需 velocity<0（offset 减小），故取反。
                                    // 向上拖(available.y>0)且 sheet 未到 Expanded → 吸附展开；
                                    // 向下(available.y<=0)或已展开 → 放行（给 on_post_fling/列表）。
                                    if available.y <= 0.0 {
                                        return crate::nested_scroll::ScrollVelocity::default();
                                    }
                                    let expanded_pos = self.st.anchored_draggable().position_of(&SheetValue::Expanded);
                                    if !expanded_pos.is_nan() && self.st.offset() <= expanded_pos {
                                        return crate::nested_scroll::ScrollVelocity::default();
                                    }
                                    // 吸附展开并消费速度（阻止列表抢——Compose M3 语义）
                                    self.st.settle_with_velocity(-available.y);
                                    crate::nested_scroll::ScrollVelocity { x: 0.0, y: available.y }
                                }
                            }
                            let sheet_conn = SheetNested { st: drag_st.clone() };
                            panel_mod_with_nested = panel_mod_with_nested.nested_scroll(sheet_conn);
                        }
                        let panel_mod_with_gesture = panel_mod_with_nested;
                        // 面板任意位置可拖（对齐 Compose anchoredDraggable——整个面板 Surface 可拖，
                        // 不只把手）。背景/顶部文字/空白区拖拽也驱动 sheet；列表区由内层 scroll 优先
                        //（overlay_down 命中滚动优先，见 app.rs），此处 on_drag 作为非滚动区 fallback。
                        let pd = drag_st.clone();
                        let pe = drag_st.clone();
                        let panel_mod_with_panel_drag = panel_mod_with_gesture
                            .on_drag(move |_pos, (_dx, dy)| pd.drag_delta(dy))
                            .on_drag_end(move || {
                                pe.settle_with_velocity(pe.last_velocity());
                            });
                        crate::ui::layout_components::Column::new()
                            .modifier(panel_mod_with_panel_drag)
                            .build(ctx, |ctx| {
                                if drag_handle {
                                    let d = drag_st.clone();
                                    let e = drag_st.clone();
                                    crate::ui::layout_components::Row::new()
                                        .modifier(
                                            Modifier::new()
                                                .fill_max_width()
                                                .padding_vertical(12.0)
                                                .on_drag(move |_pos, (_dx, dy)| d.drag_delta(dy))
                                                .on_drag_end(move || {
                                                    e.settle_with_velocity(e.last_velocity());
                                                }),
                                        )
                                        .arrangement(crate::layout::Arrangement::Center)
                                        .build(ctx, |ctx| {
                                            crate::ui::layout_components::Stack::new()
                                                .modifier(Modifier::new().size(32.0, 4.0).background(
                                                    theme.outline_variant,
                                                    Shape::RoundedRect { corner_radius: 2.0 },
                                                ))
                                                .build(ctx, |_| {});
                                        });
                                }
                                content(ctx);
                            });
                    });
            }),
            local_snapshot: Vec::new(),
        });
    }
}

impl Default for ModalBottomSheet {
    fn default() -> Self { Self::new(false) }
}
