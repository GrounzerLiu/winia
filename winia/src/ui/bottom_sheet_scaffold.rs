//! `BottomSheetScaffold`——可拖动显示更多内容的常驻底部面板（对标 Compose `BottomSheetScaffold`）
//!
//! 与 `ModalBottomSheet` 的差异：
//! - **非弹窗**：作为宿主布局的一部分（常驻），不走 `Overlay`，不带 `Scrim`，主内容常显
//! - **peek 高度**：`sheetPeekHeight`（默认 56dp）决定折叠时露头高度，
//!   `PartiallyExpanded at layoutH - peek`，`Expanded at layoutH - sheetH`
//! - **三态与 Modal 共享**：`SheetState(Hidden/Partial/Expanded)` 复用，
//!   Scaffold 初始 `Partial`（露头），Modal 初始 `Hidden`
//!
//! 布局（对标 `BottomSheetScaffoldLayout`）：
//! - 外层 `Stack.fill_max_size`，底层主内容 `fill_max_size`，上层 sheet `offset_y` 贴底
//! - 锚点计算在 `on_size_changed` 中：`layoutH` 取窗口高（`window_size().1`），
//!   `peekPx = sheetPeekHeight.to_px(density)`，`sheetH` 实测
//! - 拖拽：`on_drag/drag_end` 直连 `SheetState`

use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::modifier::{Color, Modifier, Shape};
use crate::unit::Dp;
use crate::ui::sheet_state::{SheetState, SheetValue};

pub const SCAFFOLD_SHEET_PEEK_HEIGHT: Dp = Dp(56.0);
pub const SCAFFOLD_SHEET_SHAPE_RADIUS: f32 = 28.0;

/// 常驻式底部面板宿主（对标 `BottomSheetScaffold`）
pub struct BottomSheetScaffold {
    sheet_state: Option<SheetState>,
    sheet_peek_height: Dp,
    sheet_shape_radius: f32,
    sheet_container_color: Option<Color>,
    container_color: Option<Color>,
    sheet_swipe_enabled: bool,
    sheet_drag_handle: bool,
    sheet_max_width: Option<Dp>,
}

impl BottomSheetScaffold {
    /// 新建（需提供片高度，内容与主内容在 `build` 时传入）
    pub fn new() -> Self {
        Self {
            sheet_state: None,
            sheet_peek_height: SCAFFOLD_SHEET_PEEK_HEIGHT,
            sheet_shape_radius: SCAFFOLD_SHEET_SHAPE_RADIUS,
            sheet_container_color: None,
            container_color: None,
            sheet_swipe_enabled: true,
            sheet_drag_handle: true,
            sheet_max_width: Some(Dp(640.0)),
        }
    }

    pub fn sheet_state(mut self, s: SheetState) -> Self {
        self.sheet_state = Some(s);
        self
    }

    pub fn sheet_peek_height(mut self, peek: Dp) -> Self {
        self.sheet_peek_height = peek;
        self
    }

    pub fn sheet_shape_radius(mut self, r: f32) -> Self {
        self.sheet_shape_radius = r;
        self
    }

    pub fn sheet_container_color(mut self, c: Color) -> Self {
        self.sheet_container_color = Some(c);
        self
    }

    /// 整体背景色（对齐 Compose `BottomSheetScaffold(containerColor)`——覆盖主内容底）。
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    pub fn sheet_swipe_enabled(mut self, v: bool) -> Self {
        self.sheet_swipe_enabled = v;
        self
    }

    pub fn sheet_drag_handle(mut self, v: bool) -> Self {
        self.sheet_drag_handle = v;
        self
    }

    pub fn sheet_max_width(mut self, w: Dp) -> Self {
        if w.0.is_infinite() {
            self.sheet_max_width = None;
        } else {
            self.sheet_max_width = Some(w);
        }
        self
    }

    /// #[composable] 宿主：主内容 + 可拖动底部片
    #[composable]
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        sheet_content: impl Fn(&mut ComposeCtx) + 'static,
        content: impl FnOnce(&mut ComposeCtx) + 'static,
    ) {
        // SheetState：外部传入或内部 remember（初始 Partial 露头，对齐 Scaffold 默认）
        let sheet_state = match self.sheet_state {
            Some(s) => s,
            None => ctx
                .remember(|| SheetState::new(SheetValue::PartiallyExpanded))
                .get(),
        };
        let peek = self.sheet_peek_height;
        let radius = self.sheet_shape_radius;
        let container_color = self.sheet_container_color;
        let swipe = self.sheet_swipe_enabled;
        let drag_handle = self.sheet_drag_handle;
        let _sheet_max_width = self.sheet_max_width; // 保留 API，对齐 Compose 640.dp，手机 480 铺满
        let container_color = self.container_color;

        // 外层 Stack：主内容底层，片上层
        let mut root_mod = Modifier::new().fill_max_size();
        if let Some(cc) = container_color {
            // 整体背景（对齐 Compose BottomSheetScaffold containerColor）
            root_mod = root_mod.background(cc, Shape::Rectangle);
        }
        crate::ui::layout_components::Stack::new()
            .modifier(root_mod)
            .build(ctx, |ctx| {
                // 主内容（占满，片在上层覆盖）——底部留出 sheet peek 高度，
                // 对齐 Compose BottomSheetScaffold 的 contentWindowPadding（sheet 折叠时内容不被遮挡）
                let peek_px_pre = peek.to_px(crate::unit::current_density());
                crate::ui::layout_components::Stack::new()
                    .modifier(Modifier::new().fill_max_size().padding_bottom(peek_px_pre))
                    .build(ctx, |ctx| {
                        content(ctx);
                    });

                // 片：offset_y 驱动，高度由内容决定，锚点在 on_size_changed 中更新
                // 用窗口高近似 layoutH（占满时正确），响应式订阅 window_size 以跟随 resize
                let window_h = crate::ui::window_size().1;
                let _ = window_h; // 注册依赖，resize 触发重组
                let layout_h = window_h;
                let density = crate::unit::current_density();
                let peek_px = peek.to_px(density);

                let st_for_offset = sheet_state.clone();
                let st_for_anchors = sheet_state.clone();
                let sheet_h: State<f32> = ctx.remember(|| 0.0);
                let is_full = sheet_h.get() >= layout_h - 1.0;
                let from = if st_for_offset.has_partially_expanded_state() {
                    SheetValue::PartiallyExpanded
                } else {
                    SheetValue::Hidden
                };
                let p = st_for_offset.progress(from, SheetValue::Expanded);
                let off = st_for_offset.offset();
                let cur_radius = if is_full {
                    if off.is_nan() { radius } else { radius * (1.0 - p.clamp(0.0, 1.0)) }
                } else {
                    radius
                };
                let cur_shape = Shape::TopRoundedRect { radius: cur_radius };

                let mut sheet_mod = Modifier::new()
                    .fill_max_width()
                    .offset_y(st_for_offset.offset_state())
                    .shadow(
                        1.0,
                        cur_shape,
                        false,
                        Color::from_argb(40, 0, 0, 0),
                    )
                    .background(
                        container_color.unwrap_or_else(|| {
                            crate::ui::theme::WiniaTheme::colors().surface_container_low
                        }),
                        cur_shape,
                    )
                    .on_size_changed({
                        let sheet_h_for_cb = sheet_h.clone();
                        // 在回调内重算 peek/layout，避免闭包固化 + Skip 时 stale
                        move |_w, h| {
                            sheet_h_for_cb.set(h);
                            let layout = crate::ui::window_size().1;
                            let d = crate::unit::current_density();
                            let peek_now = peek.to_px(d);
                            let s = st_for_anchors.clone();
                            s.update_anchors_scaffold(layout, peek_now, h);
                        }
                    });

                let mut sheet_mod_with_nested = sheet_mod;
                if swipe {
                    #[derive(Clone)]
                    struct SheetNested {
                        st: SheetState,
                    }
                    impl SheetNested {
                        /// sheet 实际消费的 deltay（offset 前后差——drag_delta clamp
                        /// 到锚点[min,max]，已在 Expanded 时 consumed=0 放行给列表）
                        fn consume(
                            st: &SheetState,
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
                            Self::consume(&self.st, available.y)
                        }
                        fn on_post_fling(
                            &self,
                            _consumed: crate::nested_scroll::ScrollVelocity,
                            available: crate::nested_scroll::ScrollVelocity,
                        ) -> crate::nested_scroll::ScrollVelocity {
                            // 向下滑松手：available.y = -vy（向下→负）。折叠需正 velocity，故取反。
                            self.st.settle_with_velocity(-available.y);
                            crate::nested_scroll::ScrollVelocity::default()
                        }
                        fn on_pre_fling(
                            &self,
                            available: crate::nested_scroll::ScrollVelocity,
                        ) -> crate::nested_scroll::ScrollVelocity {
                            // 方向约定：dispatch 传 available.y = -vy（手指向上→正）。
                            // sheet 展开需 velocity<0，故取反。向上拖且未到 Expanded → 吸附展开并消费。
                            if available.y <= 0.0 {
                                return crate::nested_scroll::ScrollVelocity::default();
                            }
                            let expanded_pos = self.st.anchored_draggable().position_of(&SheetValue::Expanded);
                            if !expanded_pos.is_nan() && self.st.offset() <= expanded_pos {
                                return crate::nested_scroll::ScrollVelocity::default();
                            }
                            self.st.settle_with_velocity(-available.y);
                            crate::nested_scroll::ScrollVelocity { x: 0.0, y: available.y }
                        }
                    }
                    let sheet_conn = SheetNested { st: sheet_state.clone() };
                    sheet_mod_with_nested = sheet_mod_with_nested.nested_scroll(sheet_conn);
                }
                // 面板任意位置可拖（对齐 Compose BottomSheetScaffold——整面板 Surface 可拖，
                // 不只把手）。背景/文字/空白区拖拽也驱动 sheet；列表区由内层 scroll 优先
                //（overlay_down 命中滚动优先，见 app.rs），此处 on_drag 作为非滚动区 fallback。
                let s_d = sheet_state.clone();
                let s_e = sheet_state.clone();
                let sheet_mod_with_panel_drag = sheet_mod_with_nested
                    .on_drag(move |_pos, (_dx, dy)| s_d.drag_delta(dy))
                    .on_drag_end(move || {
                        s_e.settle_with_velocity(s_e.last_velocity());
                    });
                crate::ui::layout_components::Column::new()
                    .modifier(sheet_mod_with_panel_drag)
                    .build(ctx, |ctx| {
                        if drag_handle {
                            crate::ui::layout_components::Row::new()
                                .modifier(Modifier::new().fill_max_width().padding_vertical(12.0))
                                .arrangement(crate::layout::Arrangement::Center)
                                .build(ctx, |ctx| {
                                    crate::ui::layout_components::Stack::new()
                                        .modifier(Modifier::new().size(32.0, 4.0).background(
                                            crate::ui::theme::WiniaTheme::colors().outline_variant,
                                            Shape::RoundedRect { corner_radius: 2.0 },
                                        ))
                                        .build(ctx, |_| {});
                                });
                        }
                        sheet_content(ctx);
                    });
            });
    }
}

impl Default for BottomSheetScaffold {
    fn default() -> Self {
        Self::new()
    }
}
