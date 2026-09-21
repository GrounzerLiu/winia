//! `BottomSheetScaffold`——可拖动显示更多内容的常驻底部面板（对标 Compose `BottomSheetScaffold`）
//!
//! 与 `ModalBottomSheet` 的差异：
//! - **非弹窗**：作为宿主布局的一部分（常驻），不走 `Overlay`，不带 `Scrim`，主内容常显
//! - **peek 高度**：`sheetPeekHeight`（默认 56dp）决定折叠时露头高度，
//!   `PartiallyExpanded at layoutH - peek`，`Expanded at layoutH - sheetH`
//! - **三态与 Modal 共享**：`SheetState(Hidden/Partial/Expanded)` 复用，
//!   Scaffold 初始 `Partial`（露头），Modal 初始 `Hidden`
//!
//! Layout (mirrors `BottomSheetScaffoldLayout`):
//! - An outer `Stack.fill_max_size`; the page at the bottom of it, the sheet above it,
//!   pinned to the bottom edge by its `offset_y`.
//! - The anchors are computed in `on_size_changed`: `layoutH` from the window height
//!   (`window_size().1`), `peekPx` from `sheetPeekHeight` used as a LOGICAL length (one dp
//!   is one logical px — see `Dp::to_logical`), and `sheetH` as measured.
//! - Dragging: `on_drag`/`drag_end` feed `SheetState` directly.

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
        // The sheet's own color, distinct from the scaffold's: this local used to be shadowed
        // by the `container_color` below, which made `sheet_container_color()` a silent no-op
        // and painted the sheet with the scaffold's color.
        let sheet_container_color = self.sheet_container_color;
        let swipe = self.sheet_swipe_enabled;
        let drag_handle = self.sheet_drag_handle;
        // sheet_max_width：对齐 Compose 640.dp 居中（与 Modal 同语义——手机 480 铺满、
        // 平板按 max 宽居中）。Dp(f32::INFINITY) 表 Unspecified 铺满。
        // Same geometry as the modal sheet (and the same unit rule — see the helper).
        let (sheet_w, sheet_pad_x) =
            crate::ui::bottom_sheet::sheet_panel_geometry(self.sheet_max_width, crate::ui::window_size().0);
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
                // Same unit rule: `padding_bottom` takes a layout length, so the dp value is
                // the logical value (the sheet's own height is reported in logical px too).
                let peek_px_pre = peek.to_logical();
                crate::ui::layout_components::Stack::new()
                    .modifier(Modifier::new().fill_max_size().padding_bottom(peek_px_pre))
                    .build(ctx, |ctx| {
                        content(ctx);
                    });

                // 片：offset_y 驱动，高度由内容决定，锚点在 on_size_changed 中更新
                // 用窗口高近似 layoutH（占满时正确），响应式订阅 window_size 以跟随 resize
                let layout_h = crate::ui::window_size().1;
                let peek_px = peek.to_logical();

                let st_for_offset = sheet_state.clone();
                let st_for_anchors = sheet_state.clone();
                let sheet_h: State<f32> = ctx.remember(|| 0.0);
                // 首帧初始化锚点：offset 尚未设置（NaN）时用当前 peek/layout 立即 snap 到
                // current_value（通常 PartiallyExpanded→peek 位），否则首帧布局 offset 用
                // NaN→0 导致片贴顶（y=0）。sheet_h=0 时 Expanded 锚点为 NaN，后续
                // on_size_changed 补全（offset 已初始化则保持）。
                if st_for_offset.offset().is_nan() {
                    st_for_offset.update_anchors_scaffold(layout_h, peek_px, sheet_h.get());
                }
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
                    .width(sheet_w)
                    // ⚠ 必须用单元素 .offset(x, y)——offset_x/offset_y 连用会 push
                    // 两个 Offset 元素，get_offset（modifier.rs:1845）只取第一个
                    // → y 恒为 0（片贴顶 bug）。
                    // `absolute_offset`, not `offset`: the x here is a CENTRING inset, which
                    // is direction-independent, while a plain offset mirrors its x under RTL
                    // (`layout/node.rs` placement) — which put the sheet at -pad_x in RTL, with
                    // that much clipped off the left and as much dead space on the right.
                    .absolute_offset(sheet_pad_x, st_for_offset.offset_state())
                    .shadow(
                        1.0,
                        cur_shape,
                        false,
                        Color::from_argb(40, 0, 0, 0),
                    )
                    .background(
                        sheet_container_color.unwrap_or_else(|| {
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
                            let peek_now = peek.to_logical();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::Constraints;
    use crate::ui::sheet_state::SheetValue;
    use crate::unit::Density;

    #[test]
    fn the_peek_anchor_is_the_token_in_logical_px() {
        // `sheetPeekHeight` reaches the anchors as a height, so it must be a logical length.
        // The bug this pins converted it with `Dp::to_px`, which at 1.5x asked for 84 logical
        // px instead of 56 — the sheet peeked 1.5x too far and the content reserved 1.5x too
        // much space for it.
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        let st = state.clone();
        let mut c = Composer::new();
        // The density has to cover the LAYOUT as well as the compose: the sheet's
        // `on_size_changed` callback runs during measure (as it does in the app, where
        // `with_density` wraps both phases), and a compose-only wrapper left that callback
        // reading density 1.0 — where `to_px` happens to equal the correct value, hiding a
        // regression in exactly the site this test is meant to catch.
        crate::unit::with_density(Density::from_density(1.5), || {
            c.compose(move |ctx| {
                crate::ui::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st.clone())
                    .sheet_peek_height(SCAFFOLD_SHEET_PEEK_HEIGHT)
                    .build(
                        ctx,
                        |ctx| { crate::ui::Text::new("sheet").build(ctx); },
                        |ctx| { crate::ui::Text::new("content").build(ctx); },
                    );
            });
            // Between compose and layout: what build()'s first-frame anchor initialisation
            // produced. (Asserting this after the layout would read the callback's value
            // instead, which is a different site.)
            assert_eq!(
                state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded),
                720.0 - SCAFFOLD_SHEET_PEEK_HEIGHT.value(),
                "first-frame anchors use the peek as a logical length"
            );
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        });
        // The peek reaches the layout three ways, so all three are asserted: the first-frame
        // anchors (before layout), the ones the sheet's own size callback re-computes, and the
        // space the content reserves for the collapsed sheet. A single-site regression has to
        // fail here — the first version of this test only checked the anchor, which the size
        // callback overwrites, so re-introducing the bug at the initialisation site left it
        // green.
        let expected_anchor = 720.0 - SCAFFOLD_SHEET_PEEK_HEIGHT.value();
        assert_eq!(
            state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded),
            expected_anchor,
            "the sheet's size callback (during measure) uses the peek as a logical length"
        );

        let root = c.layout_root_idx().expect("laid out");
        let content = c.arena_nodes()[root].children[0];
        assert_eq!(
            c.arena_nodes()[content].modifier.get_padding_vertical().1,
            SCAFFOLD_SHEET_PEEK_HEIGHT.value(),
            "the content reserves exactly one peek at the bottom, in logical px"
        );
    }
    #[test]
    fn the_centred_sheet_is_not_mirrored_in_rtl() {
        // `Modifier::offset` mirrors its x under RTL, but a centring inset is
        // direction-independent: the panel must sit `pad_x` from the LEFT edge in both
        // directions. The bug this pins put it at -pad_x in RTL (measured -130 for a 640-wide
        // sheet in a 900-wide window, i.e. 130px clipped off the left and 130px of dead space
        // at the right). It was unreachable before the `to_logical` fix only because a HiDPI
        // window used to clamp the sheet to its full width, leaving pad_x at 0.
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::ui::adaptive::set_window_size(900.0, 720.0);
            crate::ui::theme::WiniaTheme::with_theme_and_direction(
                crate::ui::theme::WiniaTheme::colors(),
                crate::layout::LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    BottomSheetScaffold::new()
                        .sheet_state(st.clone())
                        .build(
                            ctx,
                            |ctx| { crate::ui::Text::new("sheet").build(ctx); },
                            |ctx| { crate::ui::Text::new("content").build(ctx); },
                        );
                },
            );
        });
        c.layout(Constraints::new(0.0, 900.0, 0.0, 720.0));
        let sheet = c
            .arena_nodes()
            .iter()
            .find(|n| (n.measured_size.width - 640.0).abs() < 0.5)
            .expect("the sheet is the 640-wide node in a 900-wide window");
        assert_eq!(
            sheet.position.x, 130.0,
            "the sheet is centred from the left edge in RTL too, not mirrored to -130"
        );
    }
}
