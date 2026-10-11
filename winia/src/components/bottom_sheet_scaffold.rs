//! `BottomSheetScaffold` — a persistent bottom panel that drags out to show more, mirroring
//! Compose's `BottomSheetScaffold`.
//!
//! Differences from [`crate::components::bottom_sheet::ModalBottomSheet`]:
//! - **Not a popup**: part of the host's layout (always present), no `Overlay`, no scrim, and
//!   the page stays visible.
//! - **Peek height**: `sheetPeekHeight` (56dp by default) sets how much shows when collapsed —
//!   `PartiallyExpanded at layoutH - peek`, `Expanded at layoutH - sheetH`.
//! - **Shares its three states with the modal**: the same `SheetState`
//!   (`Hidden`/`PartiallyExpanded`/`Expanded`); the scaffold starts `PartiallyExpanded`
//!   (peeking), the modal starts `Hidden`.
//!
//! Layout (mirrors `BottomSheetScaffoldLayout`):
//! - A fill-max-size root built by `build_container` with `ScaffoldStackPolicy` (a `BoxLayout` plus the anchor
//!   step); the page at the bottom of it, the sheet above it, pinned to the bottom edge by its `offset_y`.
//! - The anchors are computed inside that stack's OWN layout, by `ScaffoldStackPolicy`, from the height the
//!   layout was handed (`constraints.max_height` — Compose's `val layoutHeight = constraints.maxHeight`,
//!   `:272-277`), plus `peekPx` from `sheetPeekHeight` used as a LOGICAL length (one dp is one logical px —
//!   see `Dp::to_logical`) and `sheetH` as measured. A scaffold nested in a box that is not the window
//!   therefore anchors from its own box; the window height is only the pre-layout fallback the snackbar rule
//!   starts from.
//! - Dragging: `on_drag`/`drag_end` feed `SheetState` directly.
//!
//! Differences from Compose (audited against `material3/BottomSheetScaffold.kt`, 494 lines):
//! - The settle is a 250 ms `EaseOutCubic` tween. Compose settles with the motion scheme's `DefaultSpatial`
//!   spring and expands/shows with the same token (`:238-240`). A tween cannot overshoot, which is why
//!   Compose's `verticalScaleUp`/`verticalScaleDown` (`:461-492`, a drawing-layer scale that hides the gap a
//!   bouncy settle leaves between the sheet's bottom edge and the screen edge) has no counterpart here — it
//!   becomes necessary the moment the settle turns into a spring.
//! - The `Expanded` anchor's condition is `sheetH != 0`, where Compose uses `sheetH != peekPx`
//!   (`BottomSheetScaffold.kt:282`); the two agree for any content taller than the peek, and keeping the anchor
//!   when it COINCIDES with the partial one (short content, `peekPx == sheetH`) is deliberate: the two positions
//!   are then the same, so the value is what keeps `expand()` from silently doing nothing. Compose does not have
//!   to decide that tie at all — with `sheetH == peekPx` it never adds the `Expanded` anchor — and where it does
//!   have candidates at one offset its `closestAnchor` KDoc says "this will be the last anchor of the candidates
//!   at this offset" (`foundation/AnchoredDraggable.kt:571-575`, implemented with `distance <= minDistance` at
//!   `:1620-1631`), so a tie there resolves towards the later anchor. Measured: `update_anchors_scaffold(720,
//!   56, 56)` puts both anchors at 664 and leaves the value at `PartiallyExpanded`.
//! - The anchors skip `Hidden` for the state the scaffold creates itself, as
//!   `rememberStandardBottomSheetState(skipHiddenState = true)` does (`:207-215`); a caller-provided state
//!   decides for itself, exactly like the two `remember…SheetState` helpers.
//! - `sheetPeekHeight` is applied as a `min_height`, where Compose uses `requiredHeightIn(min = …)`
//!   (`:262-264`) — Compose's form wins even when the incoming maximum is smaller.
//! - `topBar` and the snackbar host are slots here too (`top_bar`, `snackbar`). Structurally the page is a
//!   Column whose body is weighted: the bar takes its natural height and the body gets the rest, which is
//!   Compose's `bodyConstraints = looseConstraints.copy(maxHeight = layoutHeight - topBarHeight)` (`:427-431`)
//!   expressed with the tools winia's layout has instead of a dedicated `Layout` pass. The snackbar follows
//!   Compose's placement rule — centred, just above the sheet while it is partially expanded and on the
//!   bottom edge otherwise (`:441-446`) — but its own height comes from the PREVIOUS frame, so the first
//!   frame of a snackbar's life places it by its bottom edge (Compose measures every child before placing
//!   any).
//! - `sheetShape` and the composable `sheetDragHandle` are supported (`sheet_shape`,
//!   `sheet_drag_handle_content`). An explicit shape is used as given — the flattening winia does when the
//!   sheet fills the screen applies only to the built-in one, Compose's scaffold using its shape as given.
//! - `content` receives no `PaddingValues`: Compose hands the caller `PaddingValues(bottom = sheetPeekHeight)`
//!   (`:143`) and lets it apply the padding, while winia applies the same inset itself.
//! - The drag handle's `semantics` actions (expand/collapse/dismiss, `:350-385`) are not ported; the click
//!   behaviour is.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::unit::Dp;
use crate::components::sheet_state::{SheetState, SheetValue};
use crate::layout::node::ContentAlignment;

pub const SCAFFOLD_SHEET_PEEK_HEIGHT: Dp = Dp(56.0);
pub const SCAFFOLD_SHEET_SHAPE_RADIUS: f32 = 28.0;
/// Compose's `DragHandleVerticalPadding` (`SheetDefaults.kt:543`).
pub const SCAFFOLD_DRAG_HANDLE_VERTICAL_PADDING: f32 = 22.0;

/// 常驻式底部面板宿主（对标 `BottomSheetScaffold`）
pub struct BottomSheetScaffold {
    sheet_state: Option<SheetState>,
    sheet_peek_height: Dp,
    sheet_shape_radius: f32,
    /// Compose's `sheetShape`: when set, it is used as-is (no flattening when the sheet fills the screen).
    sheet_shape: Option<Shape>,
    /// Compose's `sheetDragHandle` is a composable slot; this replaces the built-in handle.
    sheet_drag_handle_content: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    sheet_container_color: Option<Color>,
    /// Compose's `sheetContentColor = contentColorFor(sheetContainerColor)` (`:126`).
    sheet_content_color: Option<Color>,
    /// Compose's `sheetShadowElevation = BottomSheetDefaults.Elevation` = `ElevationTokens.Level1` = 1 dp.
    sheet_shadow_elevation: Dp,
    /// Compose's `sheetTonalElevation = 0.dp` (`:127`).
    sheet_tonal_elevation: Dp,
    container_color: Option<Color>,
    sheet_swipe_enabled: bool,
    sheet_drag_handle: bool,
    sheet_max_width: Option<Dp>,
    top_bar: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    snackbar: Option<Box<dyn Fn(&mut ComposeCtx)>>,
}

impl BottomSheetScaffold {
    /// 新建（需提供片高度，内容与主内容在 `build` 时传入）
    pub fn new() -> Self {
        Self {
            sheet_state: None,
            sheet_peek_height: SCAFFOLD_SHEET_PEEK_HEIGHT,
            sheet_shape_radius: SCAFFOLD_SHEET_SHAPE_RADIUS,
            sheet_shape: None,
            sheet_drag_handle_content: None,
            sheet_container_color: None,
            sheet_content_color: None,
            sheet_shadow_elevation: Dp(1.0),
            sheet_tonal_elevation: Dp(0.0),
            container_color: None,
            sheet_swipe_enabled: true,
            sheet_drag_handle: true,
            sheet_max_width: Some(Dp(640.0)),
            top_bar: None,
            snackbar: None,
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

    /// Compose's `sheetShape`: an explicit shape replaces the built-in top-rounded rectangle, and with it the
    /// flattening winia does when the sheet fills the screen (Compose's scaffold uses the shape as given).
    pub fn sheet_shape(mut self, s: Shape) -> Self {
        self.sheet_shape = Some(s);
        self
    }

    /// Compose's `sheetDragHandle` slot: this replaces the built-in 32x4 handle. `sheet_drag_handle(false)`
    /// removes the handle entirely, and a slot takes precedence over it.
    pub fn sheet_drag_handle_content(mut self, f: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.sheet_drag_handle_content = Some(Box::new(f));
        self
    }

    pub fn sheet_container_color(mut self, c: Color) -> Self {
        self.sheet_container_color = Some(c);
        self
    }

    /// The sheet's content colour. Unset, it is derived from the container colour the way Compose's
    /// `contentColorFor(sheetContainerColor)` does — a scheme container maps to its paired content colour
    /// (`surfaceContainerLow -> onSurface`, `primary -> onPrimary`, …) and anything else falls back to the
    /// ambient content colour.
    pub fn sheet_content_color(mut self, c: Color) -> Self {
        self.sheet_content_color = Some(c);
        self
    }

    /// The sheet's shadow elevation (Compose's `sheetShadowElevation`, 1 dp by default = `ElevationTokens`
    /// `Level1`). Zero removes the shadow.
    pub fn sheet_shadow_elevation(mut self, e: Dp) -> Self {
        self.sheet_shadow_elevation = e;
        self
    }

    /// The sheet's tonal elevation (Compose's `sheetTonalElevation`, 0 by default). Only applies when the
    /// container colour IS the scheme's `surface`, which is Compose's `surfaceColorAtElevation` rule, and
    /// only while the theme's tonal-elevation switch is on.
    pub fn sheet_tonal_elevation(mut self, e: Dp) -> Self {
        self.sheet_tonal_elevation = e;
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

    /// Compose's `topBar` slot: it is measured FIRST and the body gets what is left below it
    /// (`BottomSheetScaffold.kt:427-431`, `bodyConstraints = looseConstraints.copy(maxHeight = layoutHeight
    /// - topBarHeight)`), then the bar is placed at the top and the body under it.
    pub fn top_bar(mut self, f: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.top_bar = Some(Box::new(f));
        self
    }

    /// Compose's `snackbarHost` slot. The snackbar sits centred, just above the sheet while it is partially
    /// expanded, and on the bottom edge otherwise (`:441-446`).
    pub fn snackbar(mut self, f: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.snackbar = Some(Box::new(f));
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
                .remember(|| {
                    // Compose's `rememberStandardBottomSheetState(skipHiddenState = true)`
                    // (`BottomSheetScaffold.kt:207-215`): a standard sheet cannot be swiped off the
                    // screen — swiping down stops at the peek, and the drag handle collapses rather
                    // than hides. A caller-provided state decides for itself, as it does in Compose.
                    let s = SheetState::new(SheetValue::PartiallyExpanded);
                    s.set_skip_hidden_state(true);
                    s
                })
                .get(),
        };
        // The height this scaffold's own layout was handed, written by `ScaffoldStackPolicy` every pass — the
        // number Compose calls `layoutHeight` (`BottomSheetScaffold.kt:272-277`). `None` only before the first
        // layout, where the window height is the fallback every `on_size_changed`-driven anchor starts from.
        let measured_layout_h: crate::runtime::state::Backchannel<Option<f32>> =
            ctx.remember_backchannel(|| None);
        let peek = self.sheet_peek_height;
        let radius = self.sheet_shape_radius;
        let explicit_shape = self.sheet_shape;
        let sheet_drag_handle_content_slot = self.sheet_drag_handle_content;
        // The sheet's own color, distinct from the scaffold's: this local used to be shadowed
        // by the `container_color` below, which made `sheet_container_color()` a silent no-op
        // and painted the sheet with the scaffold's color.
        let sheet_container_color = self.sheet_container_color;
        let sheet_content_color = self.sheet_content_color;
        let sheet_shadow_elevation = self.sheet_shadow_elevation.0;
        let sheet_tonal_elevation = self.sheet_tonal_elevation.0;
        let swipe = self.sheet_swipe_enabled;
        let drag_handle = self.sheet_drag_handle;
        // sheet_max_width：对齐 Compose 640.dp 居中（与 Modal 同语义——手机 480 铺满、
        // 平板按 max 宽居中）。Dp(f32::INFINITY) 表 Unspecified 铺满。
        // Same geometry as the modal sheet (and the same unit rule — see the helper).
        // Only the width: the box's centring supplies the inset now, so the padding the geometry
        // hands back is not used here (the modal sheet still applies it itself).
        let (sheet_w, _sheet_pad_x) =
            crate::components::bottom_sheet::sheet_panel_geometry(self.sheet_max_width, crate::layout::window_size().0);
        let container_color = self.container_color;
        // The sheet's own colours, resolved once: the container (Compose's
        // `SheetBottomTokens.DockedContainerColor` = `surfaceContainerLow`), its tonal overlay, and the
        // content colour derived from it.
        let theme_for_sheet = crate::theme::WiniaTheme::colors();
        let sheet_bg = sheet_container_color.unwrap_or(theme_for_sheet.surface_container_low);
        let sheet_bg = if sheet_tonal_elevation > 0.0
            && sheet_bg == theme_for_sheet.surface
            && crate::theme::WiniaTheme::tonal_elevation_enabled()
        {
            // Compose's published alpha over the `ElevationTokens` levels (`ElevationTokens.kt:24-29`),
            // the same rule `Surface` applies.
            let alpha = ((4.5 * (sheet_tonal_elevation + 1.0).ln()) + 2.0) / 100.0;
            sheet_bg.overlay(theme_for_sheet.surface_tint, alpha)
        } else {
            sheet_bg
        };
        let sheet_content_colour = sheet_content_color
            .or_else(|| theme_for_sheet.content_color_for(sheet_bg))
            .unwrap_or_else(crate::theme::WiniaTheme::content_color);

        let top_bar = self.top_bar;
        let snackbar = self.snackbar;
        // 外层 Stack：主内容底层，片上层
        let mut root_mod = Modifier::new().fill_max_size();
        if let Some(cc) = container_color {
            // 整体背景（对齐 Compose BottomSheetScaffold containerColor）
            root_mod = root_mod.background(cc, Shape::Rectangle);
        }
        // The scaffold's own box: a `Stack`'s placement rules (the sheet is a CENTRED panel — naming the centre
        // lets the box do the centring, which is direction-symmetric, so a leading-edge inset would move the
        // panel to the other edge under RTL; measured: 390 where 130 is centred in a 900-wide RTL window), plus
        // Compose's anchor step inside the layout (`val layoutHeight = constraints.maxHeight`,
        // `BottomSheetScaffold.kt:272-277`).
        let root_dir = root_mod
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        // The panel's own height: reported by the sheet every time it is measured, and read by the policy's
        // anchor step and by the is-full rule below.
        let sheet_h: State<f32> = ctx.remember(|| 0.0);
        let layout_h_state: State<f32> = ctx.remember(|| 0.0);
        let anchors_input: crate::runtime::state::Backchannel<Option<(f32, f32, f32)>> =
            ctx.remember_backchannel(|| None);
        let root_policy = ScaffoldStackPolicy {
            inner: crate::layout::box_layout::BoxLayout::new()
                .content_alignment(ContentAlignment::TOP_CENTER)
                .direction(root_dir),
            sheet_state: sheet_state.clone(),
            peek_px: peek.to_logical(),
            sheet_h: sheet_h.clone(),
            measured_layout_h: measured_layout_h.clone(),
            layout_h_state: layout_h_state.clone(),
            anchors_input: anchors_input.clone(),
        };
        crate::layout::components::build_container(ctx, root_mod, root_policy, |ctx| {
                // The page: Compose measures the top bar first and gives the body what is left below it
                // (`BottomSheetScaffold.kt:427-431`). A Column with a weighted body does exactly that —
                // the bar takes its natural height, the body takes the rest — and the body keeps the peek
                // inset of `PaddingValues(bottom = sheetPeekHeight)` (`:143`).
                // Same unit rule: `padding_bottom` takes a layout length, so the dp value is
                // the logical value (the sheet's own height is reported in logical px too).
                let peek_px_pre = peek.to_logical();
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        if let Some(bar) = &top_bar {
                            bar(ctx);
                        }
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().fill_max_size().layout_weight(1.0))
                            .build(ctx, |ctx| {
                                crate::layout::components::Stack::new()
                                    .modifier(
                                        Modifier::new().fill_max_size().padding_bottom(peek_px_pre),
                                    )
                                    .build(ctx, |ctx| {
                                        content(ctx);
                                    });
                            });
                    });

                // 片：offset_y 驱动，高度由内容决定，锚点在 on_size_changed 中更新
                // The anchors themselves are computed in `ScaffoldStackPolicy`, from the height this
                // scaffold's own layout was handed (Compose's `layoutHeight`, `BottomSheetScaffold.kt:275-276`)
                // — that is what makes a scaffold in a box that is not the window anchor from its own box
                // (measured in `a_nested_scaffold_anchors_from_its_own_height`: the first layout's partial
                // anchor is 344 for a 400-tall box where the window's height would put it at 664).
                //
                // Read the TRACKED copy first: the rules below run in the BUILD, so they need a dependency on
                // the layout's height — the backchannel is the same number without one, and the window is the
                // frame before any layout has run.
                let tracked_h = layout_h_state.get();
                let layout_h = if tracked_h > 0.0 {
                    tracked_h
                } else {
                    measured_layout_h
                        .peek()
                        .unwrap_or_else(|| crate::layout::window_size().1)
                };
                let peek_px = peek.to_logical();
                // The snackbar rule reads the sheet state after the handle closure has taken a clone.
                let st_for_snackbar_snapshot = sheet_state.clone();

                let st_for_offset = sheet_state.clone();
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
                let cur_shape = explicit_shape.unwrap_or(Shape::TopRoundedRect { radius: cur_radius });

                // The x/y pair must go through ONE element: `get_offset` and
                // `get_absolute_offset` both return only the FIRST element of their kind, so
                // chaining `offset_x(..).offset_y(..)` silently drops the y and pins the sheet
                // to the top of the window. `absolute_offset` rather than `offset` because the
                // x is a CENTRING inset, which is direction-independent, while a plain offset
                // mirrors its x under RTL (`layout/node.rs` placement) — that put the sheet at
                // -pad_x in RTL, with as much clipped off the left as left dead on the right.
                let sheet_mod = Modifier::new()
                    .width(sheet_w)
                    // At least as tall as the peek: Compose's `requiredHeightIn(min = peekHeight)` on the
                    // sheet's Surface (`BottomSheetScaffold.kt:262-264`). Without it a content shorter than
                    // the peek left the sheet only as tall as that content while its anchor still put the
                    // TOP at `layoutHeight - peek`, so the page showed through the strip under the
                    // collapsed sheet (measured: an 11 px text and a 96 px peek left an 85 px gap).
                    .min_height(peek_px)
                    // x = 0: the box's centre alignment owns the horizontal placement now.
                    .absolute_offset(0.0, st_for_offset.offset_state())
                    .shadow(
                        sheet_shadow_elevation,
                        cur_shape,
                        false,
                        Color::from_argb(40, 0, 0, 0),
                    )
                    .background(sheet_bg, cur_shape)
                    .on_size_changed({
                        let sheet_h_for_cb = sheet_h.clone();
                        // The panel's height only: the ANCHORS belong to `ScaffoldStackPolicy`, which computes
                        // them from the height this scaffold's layout was handed. This callback used to
                        // recompute them from `window_size().1` as well, and it runs AFTER the policy (the
                        // children are measured inside it), so it put the window's answer back on top — the
                        // reason a scaffold inside a 400-tall box still anchored at 664, and why the demo's
                        // numbers never moved when the policy was introduced.
                        move |_w, h| {
                            sheet_h_for_cb.set(h);
                        }
                    });

                let mut sheet_mod_with_nested = sheet_mod;
                if swipe {
                    #[derive(Clone)]
                    struct SheetNested {
                        st: SheetState,
                    }
                    impl SheetNested {
                        /// How much of `delta_y` the sheet actually took, measured as the offset's change:
                        /// `scroll_delta` clamps to the anchors' [min, max], so a sheet already at
                        /// `Expanded` consumes 0 and the list gets the rest.
                        /// `scroll_delta`, not `drag_delta`: a list scrolling inside the sheet is not a
                        /// gesture on the sheet, and marking it as one left `is_dragging` set after any
                        /// scroll that started no fling — after which a resize never moved the sheet again.
                        fn consume(
                            st: &SheetState,
                            delta_y: f32,
                        ) -> crate::nested_scroll::ScrollDelta {
                            if delta_y == 0.0 {
                                return crate::nested_scroll::ScrollDelta::ZERO;
                            }
                            let before = st.offset();
                            st.anchored_draggable().scroll_delta(delta_y);
                            let consumed = st.offset() - before;
                            crate::nested_scroll::ScrollDelta::new(0.0, consumed)
                        }
                    }
                    impl crate::nested_scroll::NestedScrollConnection for SheetNested {
                        fn on_pre_scroll(
                            &self,
                            available: crate::nested_scroll::ScrollDelta,
                            source: crate::nested_scroll::NestedScrollSource,
                        ) -> crate::nested_scroll::ScrollDelta {
                            // Compose's `ConsumeSwipeWithinBottomSheetBoundsNestedScrollConnection`
                            // (`SheetDefaults.kt:454-470`): the sheet answers USER INPUT only, and only an
                            // upward delta in the pre-scroll phase. A wheel counts as user input there — it
                            // is dispatched with `NestedScrollSource.UserInput`
                            // (`MouseWheelScrollingLogic.kt:303-307`) — so an upward wheel expands the sheet
                            // first, exactly like a finger; a fling or an animation must not move it.
                            if !source.is_user_input() || available.y >= 0.0 {
                                return crate::nested_scroll::ScrollDelta::ZERO;
                            }
                            Self::consume(&self.st, available.y)
                        }
                        fn on_post_scroll(
                            &self,
                            _consumed: crate::nested_scroll::ScrollDelta,
                            available: crate::nested_scroll::ScrollDelta,
                            source: crate::nested_scroll::NestedScrollSource,
                        ) -> crate::nested_scroll::ScrollDelta {
                            if !source.is_user_input() {
                                return crate::nested_scroll::ScrollDelta::ZERO;
                            }
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
                crate::layout::components::Column::new()
                    .modifier(sheet_mod_with_panel_drag)
                    .build(ctx, move |ctx| {
                        // The sheet's content colour, as a scope the whole panel inherits from.
                        crate::theme::WiniaTheme::with_content_color(sheet_content_colour, ctx, |ctx| {
                        // A caller-supplied handle replaces the built-in one, exactly as Compose's
                        // composable `sheetDragHandle` slot does.
                        if let Some(slot) = &sheet_drag_handle_content_slot {
                            slot(ctx);
                        } else if drag_handle {
                            // The handle is CLICKABLE in Compose (`StandardBottomSheet`, the
                            // `DragHandleWithTooltip { Box(Modifier.clickable { … }) }`): a click expands
                            // a peeking sheet, collapses an expanded one (or hides it when the hidden
                            // state is allowed), and shows a hidden one. `semantics` actions
                            // (expand/collapse/dismiss) are not ported — winia's semantics API has no
                            // custom-action channel yet, which the module doc records.
                            let handle_state = sheet_state.clone();
                            crate::layout::components::Row::new()
                                .modifier(
                                    Modifier::new()
                                        .fill_max_width()
                                        // Compose's `DragHandleVerticalPadding = 22.dp`
                                        // (`SheetDefaults.kt:543`) — winia used 12, which made the collapsed
                                        // sheet 20 px shorter than Compose's.
                                        .padding_vertical(SCAFFOLD_DRAG_HANDLE_VERTICAL_PADDING)
                                        // Compose's drag handle reads `currentValue`, which is the state the
                                        // sheet's POSITION is closest to during an animation
                                        // (`AnchoredDraggableState.updateIfNeeded`), so a click mid-flight
                                        // asks for the state it is already heading to and does nothing. With
                                        // `current_value()` (which flips when the animation is requested)
                                        // the click reversed the animation instead.
                                        .clickable(move || match handle_state.value_for_offset() {
                                            SheetValue::Expanded => {
                                                if handle_state.skip_hidden_state() {
                                                    handle_state.partial_expand();
                                                } else {
                                                    handle_state.hide();
                                                }
                                            }
                                            SheetValue::PartiallyExpanded => handle_state.expand(),
                                            _ => handle_state.show(),
                                        }),
                                )
                                .arrangement(crate::layout::Arrangement::Center)
                                .build(ctx, |ctx| {
                                    crate::layout::components::Stack::new()
                                        .modifier(Modifier::new().size(32.0, 4.0).background(
                                            // `SheetBottomTokens.DockedDragHandleColor` = `OnSurfaceVariant`
                                            // (winia painted it with `outline_variant`, a dimmer token).
                                            crate::theme::WiniaTheme::colors().on_surface_variant,
                                            Shape::RoundedRect { corner_radius: 2.0 },
                                        ))
                                        .build(ctx, |_| {});
                                });
                        }
                        sheet_content(ctx);
                        });
                    });

                // The snackbar: centred, just above the sheet while it is partially expanded, on the bottom
                // edge otherwise — Compose's rule (`BottomSheetScaffold.kt:441-446`). Compose knows the
                // snackbar's height because its layout measures every child before placing any; here the
                // height is remembered from the previous frame, so the first frame of a snackbar's life
                // places it by its bottom edge. The rule reads the state that drives the sheet, so it
                // re-runs as the sheet moves.
                if let Some(host) = &snackbar {
                    let snack_h: State<f32> = ctx.remember(|| 0.0);
                    // Compose's rule reads `currentValue` (`BottomSheetScaffold.kt:441-446`), which during an
                    // animation is the state the sheet's POSITION is closest to — so the snackbar stays on the
                    // panel's top edge until the panel has actually travelled past the midpoint between two
                    // anchors. `current_value()` flips at the request instead, which put the snackbar on the
                    // window's bottom edge while the panel was still where it started (measured: panel at
                    // 663.7 of 664 while the snackbar had already moved).
                    //
                    // The offset is resolved AT LAYOUT TIME from the height the slot has just measured, instead
                    // of from a value read in this build. Reading it here left the placement on the first
                    // frame's value (0, before the slot had ever been measured) with nothing to re-run this
                    // build afterwards — traced `h=96 y=528` here while the DRAWN card sat at the panel's top
                    // edge, confirmed by a pixel probe. The slot is measured and placed in the same pass, so the
                    // fresh height is available by placement time.
                    let snack_h_for_offset = snack_h.clone();
                    let st_for_offset = st_for_snackbar_snapshot.clone();
                    // The scaffold's own height comes from the policy, which writes it before measuring the
                    // children — so this closure reads the CURRENT pass's number (traced: `h=0 y=624` on the
                    // first frames, `h=96 y=528` once the slot measured 96, `h=96 y=408` as the sheet moved),
                    // and takes the window only until the first layout has run.
                    let h_for_offset = measured_layout_h.clone();
                    let y = move || {
                        let h_self = h_for_offset.peek().unwrap_or(layout_h);
                        match st_for_offset.value_for_offset() {
                            SheetValue::PartiallyExpanded => {
                                st_for_offset.offset() - snack_h_for_offset.get()
                            }
                            _ => h_self - snack_h_for_offset.get(),
                        }
                    };
                    let snack_mod = Modifier::new()
                        .absolute_offset(0.0, y)
                        .on_size_changed({
                            let h = snack_h.clone();
                            move |_w, height| h.set(height)
                        });
                    crate::layout::components::Stack::new()
                        .modifier(snack_mod)
                        .build(ctx, |ctx| {
                            host(ctx);
                        });
                }
            });
    }
}

/// The scaffold's own box: `BoxLayout`'s placement rules, plus Compose's anchor step run INSIDE the layout
/// with the height that layout was handed.
///
/// Compose's scaffold computes its anchors in its layout lambda — `val layoutHeight = constraints.maxHeight`
/// (`BottomSheetScaffold.kt:272-277`) — and returns them together with the anchor the sheet keeps. Doing the same
/// here is what makes a scaffold nested in another one (the demo sits in the settings shell's `Scaffold`, which
/// spends 64 px on a top bar) anchor from its own box instead of the window: measured in
/// `a_nested_scaffold_anchors_from_its_own_height`, the partial anchor landed on 664 (720 minus the peek) where
/// its own 656 px box puts it at 600.
///
/// The height is written to a `Backchannel` BEFORE the children are measured, so the snackbar's offset closure
/// — which runs while the children are measured — sees the same pass's number rather than a frame-old one.
#[derive(Clone)]
struct ScaffoldStackPolicy {
    /// The placement rules the scaffold's root `Stack` always had: measure every child with the incoming
    /// constraints, size to the largest, and place each by the content alignment.
    inner: crate::layout::box_layout::BoxLayout,
    sheet_state: SheetState,
    peek_px: f32,
    sheet_h: State<f32>,
    measured_layout_h: crate::runtime::state::Backchannel<Option<f32>>,
    /// The same height as a tracked `State`, for the build-time rules (see the set site).
    layout_h_state: State<f32>,
    /// The last `(layout height, peek, panel height)` the anchors were computed for, so a pass that changes
    /// none of them does not re-target the sheet.
    anchors_input: crate::runtime::state::Backchannel<Option<(f32, f32, f32)>>,
}

impl std::fmt::Debug for ScaffoldStackPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScaffoldStackPolicy")
    }
}

impl crate::layout::node::MeasurePolicy for ScaffoldStackPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::node::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::node::Placement>) {
        let layout_h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            crate::layout::window_size().1
        };
        self.measured_layout_h.set(Some(layout_h));
        // A TRACKED copy, so the build re-runs when the layout's height changes (a resize — the anchors follow
        // either way because this pass recomputes them, but rules that only run in the build, like the
        // square-corner `is_full` test, need the dependency). Set only on change, so nothing is marked dirty by a
        // pass that found the same number.
        if (self.layout_h_state.get() - layout_h).abs() > f32::EPSILON {
            self.layout_h_state.set(layout_h);
        }

        let peek = self.peek_px;
        // The first frame: the panel's offset has not been set yet, so snap the anchors to the state it is in
        // (normally `PartiallyExpanded`) BEFORE measuring — without this the first layout would place the panel
        // at y=0.
        if self.sheet_state.offset().is_nan() {
            self.sheet_state.update_anchors_scaffold(layout_h, peek, self.sheet_h.peek());
        }

        let out = self.inner.measure(nodes, policies, children, constraints);

        // And once the children are measured the panel's height is known, so the anchors are computed for THIS
        // pass too — the panel itself has already been placed by then, which is why the offset it uses is one
        // pass behind (the same one-pass lag the previous `on_size_changed` path had). A pass that changes none
        // of the three inputs leaves the sheet alone.
        let sheet_h = self.sheet_h.peek();
        if sheet_h > 0.0 && self.anchors_input.peek() != Some((layout_h, peek, sheet_h)) {
            self.anchors_input.set(Some((layout_h, peek, sheet_h)));
            self.sheet_state.update_anchors_scaffold(layout_h, peek, sheet_h);
        }

        out
    }

    fn place(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        children: &[usize],
        placements: &[crate::layout::node::Placement],
    ) {
        self.inner.place(nodes, children, placements);
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
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::components::sheet_state::SheetValue;
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
        crate::runtime::density::with_density(Density::from_density(1.5), || {
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st.clone())
                    .sheet_peek_height(SCAFFOLD_SHEET_PEEK_HEIGHT)
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("content").build(ctx); },
                    );
            });
            // The anchors are computed in the LAYOUT now (`ScaffoldStackPolicy` — Compose's own place for them,
            // `val layoutHeight = constraints.maxHeight`), so a first build no longer touches them and this is
            // where the peek's unit shows up first.
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            assert_eq!(
                state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded),
                720.0 - SCAFFOLD_SHEET_PEEK_HEIGHT.value(),
                "the layout's anchors use the peek as a logical length"
            );
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

        // Structure: root -> page Column -> the weighted body Column -> the peek-inset Stack -> content.
        let root = c.layout_root_idx().expect("laid out");
        let nodes = c.arena_nodes();
        let page = nodes[root].children[0];
        let body = nodes[page].children[0];
        let inset = nodes[body].children[0];
        assert_eq!(
            nodes[inset].modifier.get_padding_vertical().1,
            SCAFFOLD_SHEET_PEEK_HEIGHT.value(),
            "the content reserves exactly one peek at the bottom, in logical px"
        );
    }

    /// A sheet whose CONTENT changes height must re-anchor: recomputing the anchors used to live in the sheet's
    /// own `on_size_changed` (which runs inside the measure, so it always saw the new height), and that job moved
    /// to `ScaffoldStackPolicy`. This pins that the height still reaches the anchors when nothing but the content
    /// changed — the Expanded anchor is `layoutHeight - sheetHeight`, so growth must lower it by the same amount.
    #[test]
    fn a_content_height_change_moves_the_expanded_anchor() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let tall = State::new(false);
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        fn frame(c: &mut Composer, state: &SheetState, tall: &State<bool>) {
            let st = state.clone();
            let tall = tall.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st.clone())
                    .build(
                        ctx,
                        {
                            let tall = tall.clone();
                            move |ctx| {
                                let h = if tall.get() { 300.0 } else { 120.0 };
                                crate::layout::components::Column::new()
                                    .modifier(Modifier::new().fill_max_width().height(h))
                                    .build(ctx, |_| {});
                            }
                        },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        for _ in 0..3 {
            frame(&mut c, &state, &tall);
        }
        let short = state.anchored_draggable().position_of(&SheetValue::Expanded);
        tall.set(true);
        for _ in 0..3 {
            frame(&mut c, &state, &tall);
        }
        let grown = state.anchored_draggable().position_of(&SheetValue::Expanded);
        assert!(
            (short - grown - 180.0).abs() < 1.0,
            "growing the content by 180 must lower the Expanded anchor by 180 ({short} -> {grown})"
        );
        crate::animation::clear_all_animations();
    }

    /// Compose forces the sheet to be at least as tall as its peek — `Modifier.requiredHeightIn(min =
    /// peekHeight)` on the Surface (`BottomSheetScaffold.kt:262-264`). Without it a content shorter than
    /// the peek leaves the sheet just as tall as that content while the anchor still puts its TOP at
    /// `layoutHeight - peek`, so the strip between the sheet's bottom edge and the window's bottom edge
    /// shows the page underneath - the collapsed sheet looks like it is floating.
    #[test]
    fn a_short_sheet_still_reaches_its_peek_height() {
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            BottomSheetScaffold::new()
                .sheet_state(st.clone())
                .sheet_peek_height(Dp(96.0))
                .sheet_drag_handle(false)
                .build(
                    ctx,
                    |ctx| {
                        crate::components::Text::new("s").font_size(8.0).build(ctx);
                    },
                    |ctx| {
                        crate::components::Text::new("page").build(ctx);
                    },
                );
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        let root = c.layout_root_idx().expect("laid out");
        let sheet = c.arena_nodes()[root].children[1];
        assert_eq!(
            c.arena_nodes()[sheet].measured_size.height, 96.0,
            "the sheet is at least as tall as its peek (its content is 11)"
        );
        assert_eq!(
            c.arena_nodes()[sheet].position.y, 720.0 - 96.0,
            "so the collapsed sheet covers the window's bottom edge, with no page showing under it"
        );
    }

    /// Compose measures the top bar FIRST and hands the body `maxHeight = layoutHeight - topBarHeight`
    /// (`BottomSheetScaffold.kt:427-431`) before placing the body under the bar. Structurally the same here:
    /// a Column whose body is weighted takes what the bar leaves, and the body is placed below it.
    #[test]
    fn a_top_bar_takes_its_height_before_the_body() {
        fn body_geometry(with_bar: bool) -> (f32, f32) {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                let mut scaffold = BottomSheetScaffold::new();
                if with_bar {
                    scaffold = scaffold.top_bar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().fill_max_width().height(40.0))
                            .build(ctx, |_| {});
                    });
                }
                scaffold.build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().fill_max_size().test_tag("body"))
                            .build(ctx, |_| {});
                    },
                );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let body = node_with_tag(nodes, "body");
            let (_, y) = absolute_position(nodes, root, body).expect("the body is under the root");
            (y, nodes[body].measured_size.height)
        }

        // The tagged node lives inside the peek-inset Stack, so its height is the scaffold's minus the
        // 56 px the collapsed sheet reserves — what matters here is the 40 px the bar takes off the top.
        let (y_without, h_without) = body_geometry(false);
        assert_eq!(
            (y_without, h_without),
            (0.0, 720.0 - 56.0),
            "with no bar the body starts at the top and is only inset by the peek"
        );
        let (y_with, h_with) = body_geometry(true);
        assert_eq!(y_with, 40.0, "the body starts under the top bar");
        assert_eq!(h_with, 720.0 - 40.0 - 56.0, "and gets exactly what the bar left");
    }

    /// A node's absolute position: the arena stores each position relative to its parent, so a test that
    /// wants to know where a nested node ended up has to walk down from the root adding them up.
    fn absolute_position(
        nodes: &[crate::layout::node::LayoutNode],
        root: usize,
        target: usize,
    ) -> Option<(f32, f32)> {
        if root == target {
            return Some((0.0, 0.0));
        }
        for &child in &nodes[root].children {
            if let Some((x, y)) = absolute_position(nodes, child, target) {
                return Some((nodes[child].position.x + x, nodes[child].position.y + y));
            }
        }
        None
    }

    /// The index of the node carrying `tag`.
    fn node_with_tag(nodes: &[crate::layout::node::LayoutNode], tag: &str) -> usize {
        nodes
            .iter()
            .position(|n| {
                n.modifier.elements().iter().any(|el| {
                    matches!(el, crate::modifier::ModifierElement::TestTag { tag: t } if t == tag)
                })
            })
            .unwrap_or_else(|| panic!("no node tagged {tag}"))
    }

    /// The snackbar sits centred, just above the sheet while it is partially expanded, and on the bottom
    /// edge otherwise — Compose's rule (`BottomSheetScaffold.kt:441-446`). Compose can do it in one pass
    /// because its layout measures every child before placing any; here the snackbar's own height comes from
    /// the previous frame (the documented one-frame caveat), so the helper below composes and lays out
    /// twice, as a real app does.
    #[test]
    fn the_snackbar_sits_above_the_sheet_until_it_is_expanded() {
        // It drives a settle animation, so it holds the animation lock like every other such test.
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        /// One frame of the scaffold: compose, then lay out.
        /// `pad` gives the slot its own vertical padding, the way a real snackbar host does — Compose measures
        /// the host INCLUDING that padding, so the padded box's bottom lands on the sheet's top edge.
        fn frame(c: &mut Composer, state: &SheetState, pad: f32) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(move |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .padding_vertical(pad)
                                    .size(200.0, 48.0)
                                    .test_tag("snack"),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }

        fn snackbar_y(state: &SheetState, settle_expanded: bool) -> f32 {
            let mut c = Composer::new();
            frame(&mut c, state, 0.0);
            if settle_expanded {
                state.expand();
                let start = std::time::Instant::now();
                while start.elapsed() < std::time::Duration::from_millis(600) {
                    crate::animation::update_animations();
                    std::thread::sleep(std::time::Duration::from_millis(8));
                    if (state.offset()
                        - state.anchored_draggable().position_of(&SheetValue::Expanded))
                    .abs()
                        < 0.5
                    {
                        break;
                    }
                }
            }
            // A second frame: the snackbar's height has been measured by now, so the rule can place it.
            frame(&mut c, state, 0.0);
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let snack = node_with_tag(nodes, "snack");
            absolute_position(nodes, root, snack)
                .expect("the snackbar is under the root")
                .1
        }

        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let peek_y = snackbar_y(&state, false);
        assert_eq!(
            peek_y,
            720.0 - 56.0 - 48.0,
            "partially expanded: the snackbar's bottom edge sits on the sheet's top (the default 56 px \
             peek, minus its own 48)"
        );

        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let expanded_y = snackbar_y(&state, true);
        assert_eq!(expanded_y, 720.0 - 48.0, "expanded: it drops to the bottom edge");
    }

    /// A scaffold in a box that is NOT the window must anchor from that box. Compose's scaffold layout takes
    /// `layoutHeight` from the constraints it is given (`BottomSheetScaffold.kt:275-276`); before `4c16daf`
    /// winia read `crate::layout::window_size().1` in the build and only converged once the sheet's own
    /// `on_size_changed` had reported a height, so the panel started on the window's anchor and took about 25
    /// frames to reach its own.
    ///
    /// The assertion is therefore made on the FIRST layout, not on a settled offset: a polling version of this
    /// test passed on the pre-`4c16daf` sources as well (measured there: `early_partial_anchor=664`,
    /// `frames_to_344=25`), which made it pin nothing.
    #[test]
    fn a_nested_scaffold_anchors_from_its_own_height() {
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        /// One frame: compose and lay out. The frame matters because the anchors are computed in the layout.
        fn frame(c: &mut Composer, state: &SheetState) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                // A box that is SHORTER than the window: the case where the scaffold's own height and the
                // window's differ. (The settings shell cannot show it — its body takes the whole window, so
                // there the two are equal and the difference is invisible.)
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width().height(400.0))
                    .build(ctx, move |ctx| {
                        BottomSheetScaffold::new()
                            .sheet_state(st.clone())
                            .build(
                                ctx,
                                |ctx| { crate::components::Text::new("sheet").build(ctx); },
                                |ctx| { crate::components::Text::new("page").build(ctx); },
                            );
                    });
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        frame(&mut c, &state);
        // Its own height is 400, so the partial anchor is 400 - 56 = 344; the window's would be 664.
        let first = state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded);
        assert!(
            (first - 344.0).abs() < 0.5,
            "the FIRST layout's anchors use the scaffold's own height, not the window's ({first} was used)"
        );
        // And the sheet settles onto that anchor.
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(600) {
            frame(&mut c, &state);
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(8));
            if (state.offset() - 344.0).abs() < 0.5 {
                break;
            }
        }
        assert!(
            (state.offset() - 344.0).abs() < 0.5,
            "the panel settles on its own anchor ({} was reached)",
            state.offset()
        );
        crate::animation::clear_all_animations();
    }

    /// A slot that pads ITSELF must be placed by its padded extent: Compose measures whatever host it is given,
    /// and M3's `SnackbarHost` carries its margins inside that host.
    ///
    /// The root cause is NOT in this component, which is why the test is ignored rather than the rule changed:
    /// a chain of `padding_vertical(16).size(200, 48)` reports `200x48` here, where Compose reports
    /// `200x80` — `Modifier::size` sets the node's own size without the earlier `padding` being added around
    /// it (measured 2026-10-11 with two `on_size_changed` probes on that chain: both printed `200x48`). The
    /// component's rule is exact for an unpadded slot (the test above pins both endpoints). Nesting instead of
    /// chaining — `Column(padding(16)) { Column(size(200,48)) }` — measures 80 and places exactly, which is what
    /// `winia/examples/bottom_sheet_scaffold_demo.rs` now does; the deviation itself belongs in the modifier
    /// model (`docs/modifier-node.md`, item 5 of its known-limitations section) and needs its own fix.
    #[test]
    #[ignore = "Modifier::padding before size() is not added to the measured size: 200x48 comes back where Compose measures 200x80"]
    fn a_snackbar_slot_that_pads_itself_is_measured_with_the_padding() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        fn frame(c: &mut Composer, state: &SheetState) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .padding_vertical(16.0)
                                    .size(200.0, 48.0)
                                    .test_tag("snack"),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        frame(&mut c, &state);
        frame(&mut c, &state);
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let card = node_with_tag(nodes, "snack");
        let y = absolute_position(nodes, root, card).expect("under the root").1;
        assert_eq!(
            y,
            720.0 - 56.0 - 16.0 - 48.0,
            "the padded box's bottom sits on the sheet's top, so the card itself ends 16 px above it"
        );
    }

    /// The demo's own slot shape, to bisect where its displacement comes from: a padded Stack holding a
    /// Column holding a Row that carries the card's background, padding, a Text and a Button. The card must sit
    /// exactly where the measured slot puts it — the same relation the plain-Column test above pins.
    #[test]
    fn the_demo_shaped_snackbar_slot_is_placed_by_its_measured_height() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            BottomSheetScaffold::new()
                .sheet_state(st)
                .snackbar(|ctx| {
                    let colors = crate::theme::WiniaTheme::colors();
                    crate::layout::components::Stack::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0).test_tag("snack-slot"))
                        .build(ctx, |ctx| {
                            crate::layout::components::Row::new()
                                .modifier(
                                    Modifier::new()
                                        .fill_max_width()
                                        .background(colors.surface_container_highest, crate::graphics::Shape::rounded(8.0))
                                        .padding(12.0)
                                        .test_tag("snack-card"),
                                )
                                .spacing(12.0)
                                .build(ctx, |ctx| {
                                    crate::components::Text::new("saved").font_size(13.0).build(ctx);
                                    crate::components::Button::text()
                                        .on_click(|| {})
                                        .build(ctx, |ctx| crate::components::Text::new("undo").build(ctx));
                                });
                        });
                })
                .build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
        });
        let constraints = Constraints::new(0.0, 480.0, 0.0, 720.0);
        for _ in 0..3 {
            c.layout(constraints);
        }
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let slot = node_with_tag(nodes, "snack-slot");
        let card = node_with_tag(nodes, "snack-card");
        let slot_h = nodes[slot].measured_size.height;
        let slot_y = absolute_position(nodes, root, slot).expect("under the root").1;
        let card_y = absolute_position(nodes, root, card).expect("under the root").1;
        assert_eq!(
            slot_y,
            720.0 - 56.0 - slot_h,
            "the slot's padded box rests on the panel's top edge"
        );
        assert_eq!(
            card_y,
            slot_y + 16.0,
            "and the card starts inside it, below the slot's own 16 px of padding"
        );
        crate::animation::clear_all_animations();
    }

    /// The layout half of the snackbar's placement: an `absolute_offset` whose value is computed AT LAYOUT
    /// TIME (a closure, the same `SizeValue::Dynamic` path the snackbar uses) must move the NODE, not merely
    /// the number the build computed. Written while chasing the demo, where the rule's `y=528` and the drawn
    /// card disagreed by ~90 px: this says whether the offset reaches the arena at all.
    ///
    /// A CHARACTERISATION test, not a discriminator: `Modifier::get_absolute_offset` already resolved
    /// `SizeValue::Dynamic(f) => f()` before any of the sheet commits (`winia/src/modifier.rs`), so no revert of
    /// theirs can fail it. It pins the behaviour the snackbar's rule depends on, nothing more.
    #[test]
    fn a_layout_time_absolute_offset_moves_the_node() {
        fn placed_with(offset: crate::layout::sizing::SizeValue) -> f32 {
            let mut c = Composer::new();
            let mut slot = Some(offset);
            c.compose(move |ctx| {
                let value = slot.take().expect("one frame");
                crate::layout::components::Stack::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, move |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .absolute_offset(0.0, value)
                                    .size(100.0, 50.0)
                                    .test_tag("moved"),
                            )
                            .build(ctx, |_| {});
                    });
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            absolute_position(nodes, root, node_with_tag(nodes, "moved"))
                .expect("under the root")
                .1
        }

        assert_eq!(placed_with(300.0f32.into()), 300.0, "a literal offset moves the node");
        assert_eq!(
            placed_with(crate::layout::sizing::SizeValue::Dynamic(std::sync::Arc::new(|| 200.0))),
            200.0,
            "and so does one computed at layout time"
        );
    }

    /// The snackbar's offset must follow the slot's height WITHOUT a recompose in between: the slot is measured
    /// during the layout pass, so the frame that measures it is the frame that has to place it. Two layouts in a
    /// row with no `compose` between them is exactly that, and the demo is where it went wrong (rule `y=528`,
    /// drawn card ~90 px lower). This pins the layout half; the demo's remaining displacement is the renderer's.
    #[test]
    fn the_snackbar_offset_follows_a_height_measured_in_the_same_pass() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            // A box that is shorter than the window, so "the scaffold's own height" and "the window's" are
            // different numbers; the second phase below is what reads the difference.
            crate::layout::components::Column::new()
                .modifier(Modifier::new().fill_max_width().height(400.0))
                .build(ctx, move |ctx| {
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().padding_vertical(16.0))
                            .build(ctx, |ctx| {
                                crate::layout::components::Column::new()
                                    .modifier(Modifier::new().size(200.0, 48.0).test_tag("snack"))
                                    .build(ctx, |_| {});
                            });
                    })
                    .build(
                        ctx,
                        |ctx| {
                            // 200 tall, so `Expanded` is an anchor of its own rather than the partial one.
                            crate::layout::components::Column::new()
                                .modifier(Modifier::new().fill_max_width().height(200.0))
                                .build(ctx, |_| {});
                        },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
                });
        });
        let constraints = Constraints::new(0.0, 480.0, 0.0, 720.0);
        c.layout(constraints);
        c.layout(constraints);
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let card = node_with_tag(nodes, "snack");
        let y = absolute_position(nodes, root, card).expect("under the root").1;
        // The slot measures 80 (a 48 px card with 16 px of padding on both sides), the sheet's partial anchor
        // is at 400 - 56 = 344 (the scaffold's OWN height), so the slot's top is 264 and the card inside it
        // starts 16 lower.
        assert_eq!(
            y,
            400.0 - 56.0 - 80.0 + 16.0,
            "the card sits inside a slot placed by the height the same pass measured"
        );

        // The branch that actually READS the scaffold's own height is the expanded one (`h_self - snack_h`), so
        // a box shorter than the window is where the two diverge: 400 - 80, not 720 - 80. The content is 200
        // tall, so `Expanded` is a distinct anchor (400 - 200 = 200) and the rule really switches branches —
        // with content no taller than the peek the two anchors coincide and this could not be asserted. Freezing
        // the height at its build-time value, or taking it from the window, both fail this.
        state.expand();
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(600) {
            c.layout(constraints);
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(8));
            if (state.offset() - (400.0 - 200.0)).abs() < 0.5 {
                break;
            }
        }
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let card = node_with_tag(nodes, "snack");
        let expanded_y = absolute_position(nodes, root, card).expect("under the root").1;
        assert_eq!(
            state.value_for_offset(),
            SheetValue::Expanded,
            "the panel reached the expanded anchor"
        );
        assert_eq!(
            expanded_y,
            400.0 - 80.0 + 16.0,
            "expanded, the card rests on the box's bottom edge (its own 400), not the window's 720"
        );
        crate::animation::clear_all_animations();
    }

    /// While the sheet animates between two anchors, the snackbar stays on the panel's top edge until the
    /// panel has travelled past the MIDPOINT — Compose's rule reads `currentValue`, which is
    /// `anchors.closestAnchor(offset)` there (`AnchoredDraggableState.updateIfNeeded`). winia's
    /// `current_value` flips the moment `expand()` is called, so the snackbar used to jump to the window's
    /// bottom edge while the panel was still where it started (measured in a frame-loop probe: panel at 663.7
    /// of 664, snackbar already at the bottom).
    #[test]
    fn the_snackbar_waits_until_the_sheet_passes_the_midpoint() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        fn frame(c: &mut Composer, state: &SheetState) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().size(200.0, 48.0).test_tag("snack"))
                            .build(ctx, |_| {});
                    })
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        frame(&mut c, &state);
        frame(&mut c, &state);
        let collapsed_y = {
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let snack = node_with_tag(nodes, "snack");
            absolute_position(nodes, root, snack).expect("under the root").1
        };

        state.expand();
        // One frame: the tween has barely moved, so the sheet's position is still at the partial anchor.
        crate::animation::update_animations();
        frame(&mut c, &state);
        let y_after_one_frame = {
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let snack = node_with_tag(nodes, "snack");
            absolute_position(nodes, root, snack).expect("under the root").1
        };
        assert!(
            (y_after_one_frame - collapsed_y).abs() < 1.0,
            "the snackbar stays where it was while the panel is still at its starting anchor \
             ({y_after_one_frame} against {collapsed_y})"
        );
        crate::animation::clear_all_animations();
    }

    /// `sheetShape` and composable `sheetDragHandle`.
    #[test]
    fn an_explicit_shape_and_a_supplied_handle_replace_the_built_ins() {
        let custom_shape = Shape::RoundedRect { corner_radius: 3.0 };

        fn sheet_background_shape(c: &Composer) -> Option<Shape> {
            c.arena_nodes()
                .iter()
                .find(|node| {
                    node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::AbsoluteOffset { .. })
                    })
                })
                .and_then(|node| {
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { shape, .. } => {
                            Some(shape.clone())
                        }
                        _ => None,
                    })
                })
        }

        fn frame(with_custom_shape: bool, with_custom_handle: bool, shape: Option<Shape>) -> Composer {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                let mut scaffold = BottomSheetScaffold::new();
                if with_custom_shape {
                    scaffold = scaffold.sheet_shape(shape.clone().expect("a shape was passed"));
                }
                if with_custom_handle {
                    scaffold = scaffold.sheet_drag_handle_content(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().size(80.0, 6.0).test_tag("custom-handle"))
                            .build(ctx, |_| {});
                    });
                }
                scaffold.build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            c
        }

        let default_shape = frame(false, false, None);
        assert!(
            matches!(
                sheet_background_shape(&default_shape),
                Some(Shape::TopRoundedRect { .. })
            ),
            "the built-in shape stays a top-rounded rectangle"
        );
        let explicit = frame(true, false, Some(custom_shape.clone()));
        assert_eq!(
            sheet_background_shape(&explicit),
            Some(custom_shape.clone()),
            "an explicit shape replaces it"
        );

        // The built-in handle is a 32x4 node with the outline-variant fill; the supplied one carries a tag
        // and that size is gone.
        let builtin = frame(false, false, None);
        let has_32x4 = |c: &Composer| {
            c.arena_nodes().iter().any(|n| {
                n.modifier.elements().iter().any(|el| match el {
                    crate::modifier::ModifierElement::Size { width, height } => {
                        let fixed = |v: &crate::layout::sizing::SizeValue| match v {
                            crate::layout::sizing::SizeValue::Static(
                                crate::layout::sizing::Dimension::Fixed(px),
                            ) => *px,
                            _ => f32::NAN,
                        };
                        fixed(width) == 32.0 && fixed(height) == 4.0
                    }
                    _ => false,
                })
            })
        };
        assert!(has_32x4(&builtin), "the built-in handle is drawn by default");

        let custom = frame(false, true, None);
        node_with_tag(custom.arena_nodes(), "custom-handle");
        assert!(
            !has_32x4(&custom),
            "a supplied handle replaces the built-in one"
        );
    }

    /// The handle's numbers are the tokens Compose names: `DockedDragHandleWidth/Height` 32x4,
    /// `DragHandleVerticalPadding` 22 (`SheetDefaults.kt:410-431`, `:543`) and its colour
    /// `DockedDragHandleColor` = `OnSurfaceVariant` (`tokens/SheetBottomTokens.kt`). winia used a 12 px
    /// vertical padding and `outline_variant`, which made the collapsed sheet 20 px shorter than Compose's
    /// and painted the bar a dimmer grey.
    #[test]
    fn the_drag_handle_numbers_are_the_tokens() {
        let theme = crate::theme::WiniaTheme::colors();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                BottomSheetScaffold::new().build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
            });
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        let nodes = c.arena_nodes();
        let bar = nodes
            .iter()
            .position(|n| {
                n.modifier.elements().iter().any(|el| match el {
                    crate::modifier::ModifierElement::Size { width, height } => {
                        let fixed = |v: &crate::layout::sizing::SizeValue| match v {
                            crate::layout::sizing::SizeValue::Static(
                                crate::layout::sizing::Dimension::Fixed(px),
                            ) => *px,
                            _ => f32::NAN,
                        };
                        fixed(width) == 32.0 && fixed(height) == 4.0
                    }
                    _ => false,
                })
            })
            .expect("the built-in 32x4 bar");
        let colour = nodes[bar]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color_fn, .. } => Some(color_fn()),
                _ => None,
            })
            .expect("the bar paints a background");
        assert_eq!(colour, theme.on_surface_variant, "DockedDragHandleColor is onSurfaceVariant");

        // 4 px of bar plus 22 above and below.
        let row = nodes
            .iter()
            .find(|n| n.children.contains(&bar))
            .expect("the bar's row");
        assert_eq!(
            row.measured_size.height,
            4.0 + 2.0 * SCAFFOLD_DRAG_HANDLE_VERTICAL_PADDING,
            "the handle's box is the bar plus DragHandleVerticalPadding on both sides"
        );
    }

    /// The WHEEL is user input, exactly like a finger: Compose dispatches it with
    /// `NestedScrollSource.UserInput` (`MouseWheelScrollingLogic.kt:303-307`), and the sheet's connection
    /// accepts user input (`SheetDefaults.kt:454-470`). Watch the sign: a NEGATIVE `y` means the content moves
    /// up — a finger dragging up, or the wheel rotated DOWN — and that is the delta which expands the sheet
    /// first; a positive one reaches the sheet only with what the list could not use. Neither marks the sheet
    /// as dragging, because a nested delta is not the panel's own gesture. `Fling` and `SideEffect` must not
    /// move the sheet at all (no production call site passes them today; the guard is insurance and matches M3).
    #[test]
    fn a_wheel_drives_the_sheet_like_a_finger_and_a_fling_does_not() {
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            let scroll = ctx.remember(|| crate::layout::LazyListState::new()).get();
            BottomSheetScaffold::new()
                .sheet_state(st.clone())
                .build(
                    ctx,
                    move |ctx| {
                        // A scrollable inside the sheet: this is what the nested-scroll chain hands the
                        // leftover delta to (`SheetNested`).
                        let rows = ctx.remember(|| std::sync::Arc::new((0..60).map(|i| (i as u64, format!("row {i}"))).collect::<Vec<_>>())).get();
                        crate::layout::LazyColumn::new()
                            .state(scroll.clone())
                            .modifier(Modifier::new().test_tag("sheet-scroll").fill_max_width())
                            .items_from(rows.clone(), |(id, _)| *id, |ctx, _i, (_, label)| {
                                crate::components::Text::new(label.clone())
                                    .modifier(Modifier::new().fill_max_width().height(40.0))
                                    .build(ctx);
                            })
                            .build(ctx);
                    },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        assert!(!state.anchored_draggable().is_dragging(), "nothing drags yet");

        let root = c.layout_root_idx().expect("laid out");
        let target = node_with_tag(c.arena_nodes(), "sheet-scroll");
        let density = crate::unit::Density::from_density(1.0);
        let dispatch = |c: &mut Composer, delta: f32, source| {
            crate::app::scroll::dispatch_nested_scroll_delta(
                c.arena_nodes_mut(),
                root,
                target,
                crate::nested_scroll::ScrollDelta::new(0.0, delta),
                source,
                density,
            )
        };

        // An upward wheel expands the sheet, like a finger (expands-first: the list gets nothing yet).
        let before = state.offset();
        dispatch(&mut c, -20.0, crate::nested_scroll::NestedScrollSource::Wheel);
        assert!(
            (state.offset() - (before - 20.0)).abs() < 0.5,
            "an upward wheel moves the sheet ({} against {})",
            state.offset(),
            before - 20.0
        );
        assert!(
            !state.anchored_draggable().is_dragging(),
            "a nested delta is not the panel's own gesture, so the re-target gate stays open"
        );

        // A FLING is a side effect: it must not move the sheet.
        let settled = state.offset();
        dispatch(&mut c, -20.0, crate::nested_scroll::NestedScrollSource::Fling);
        assert_eq!(state.offset(), settled, "a fling does not move the sheet");
    }

    /// The sheet's shadow follows `sheetShadowElevation` (Compose's default is `ElevationTokens.Level1`,
    /// 1 dp) — zero asks for no shadow at all, which `Modifier::shadow` drops.
    #[test]
    fn the_sheet_shadow_follows_its_elevation() {
        fn sheet_shadow_count(elevation: f32) -> usize {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_shadow_elevation(Dp(elevation))
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            c.arena_nodes()
                .iter()
                .filter(|node| {
                    node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::AbsoluteOffset { .. })
                    })
                })
                .map(|node| {
                    node.modifier
                        .elements()
                        .iter()
                        .filter(|el| matches!(el, crate::modifier::ModifierElement::Shadow { .. }))
                        .count()
                })
                .max()
                .unwrap_or(0)
        }
        // `Modifier::shadow` pushes an ambient and a spot layer per call.
        assert_eq!(sheet_shadow_count(1.0), 2, "the default elevation draws a shadow");
        assert_eq!(sheet_shadow_count(0.0), 0, "zero elevation draws none");
    }

    /// The sheet's content colour follows its container, as Compose's
    /// `sheetContentColor = contentColorFor(sheetContainerColor)` does: a scheme container maps to its
    /// paired content colour, a custom one falls back to the ambient colour, and an explicit
    /// `sheet_content_color` wins. Before this, the panel simply inherited whatever the page provided.
    #[test]
    fn the_sheets_content_colour_follows_its_container() {
        let theme = crate::theme::WiniaTheme::colors();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Color>::new()));
        let observe = |sink: std::rc::Rc<std::cell::RefCell<Vec<Color>>>| {
            move |ctx: &mut ComposeCtx| {
                sink.borrow_mut().push(crate::theme::WiniaTheme::content_color());
            }
        };
        let mut c = Composer::new();
        let s1 = seen.clone();
        let s2 = seen.clone();
        let s3 = seen.clone();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            crate::theme::WiniaTheme::with_content_color(
                theme.tertiary,
                ctx,
                move |ctx| {
                    // A scheme container: its pair decides, not the ambient colour.
                    BottomSheetScaffold::new()
                        .sheet_container_color(theme.surface_container_low)
                        .build(
                            ctx,
                            move |ctx| observe(s1.clone())(ctx),
                            |ctx| { crate::components::Text::new("page").build(ctx); },
                        );
                    // A colour that is not in the scheme: the ambient one is the fallback.
                    BottomSheetScaffold::new()
                        .sheet_container_color(Color::from_argb(255, 1, 2, 3))
                        .build(
                            ctx,
                            move |ctx| observe(s2.clone())(ctx),
                            |ctx| { crate::components::Text::new("page").build(ctx); },
                        );
                    // Explicit wins over both.
                    BottomSheetScaffold::new()
                        .sheet_container_color(theme.surface_container_low)
                        .sheet_content_color(Color::from_argb(255, 9, 9, 9))
                        .build(
                            ctx,
                            move |ctx| observe(s3.clone())(ctx),
                            |ctx| { crate::components::Text::new("page").build(ctx); },
                        );
                },
            );
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        let got = seen.borrow();
        assert_eq!(got.len(), 3, "three sheets composed");
        assert_eq!(got[0], theme.on_surface, "surfaceContainerLow -> onSurface");
        assert_eq!(got[1], theme.tertiary, "a custom container falls back to the ambient colour");
        assert_eq!(got[2], Color::from_argb(255, 9, 9, 9), "an explicit content colour wins");
    }

    /// The sheet's own background: the token colour by default, tinted by `surface_tint` when a tonal
    /// elevation is asked for AND the container is the scheme's `surface` (Compose's
    /// `surfaceColorAtElevation`), untinted otherwise.
    #[test]
    fn the_sheets_tonal_elevation_only_tints_a_surface_coloured_sheet() {
        let theme = crate::theme::WiniaTheme::colors();
        let painted = |container: Color, tonal: f32| -> Color {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_container_color(container)
                    .sheet_tonal_elevation(Dp(tonal))
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            // The sheet's background is the first painted background belonging to a node that also
            // carries the sheet's own absolute offset.
            c.arena_nodes()
                .iter()
                .find_map(|node| {
                    let is_sheet = node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::AbsoluteOffset { .. })
                    });
                    if !is_sheet {
                        return None;
                    }
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { color_fn, .. } => Some(color_fn()),
                        _ => None,
                    })
                })
                .expect("the sheet paints a background")
        };
        let alpha = ((4.5 * (3.0f32 + 1.0).ln()) + 2.0) / 100.0;
        assert_eq!(
            painted(theme.surface, 3.0),
            theme.surface.overlay(theme.surface_tint, alpha),
            "a surface-coloured sheet is tinted, with the published alpha"
        );
        assert_eq!(
            painted(theme.surface_container_low, 3.0),
            theme.surface_container_low,
            "any other container is left alone"
        );
        assert_eq!(painted(theme.surface, 0.0), theme.surface, "and zero elevation tints nothing");
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
            crate::layout::adaptive::set_window_size(900.0, 720.0);
            crate::theme::WiniaTheme::with_theme_and_direction(
                crate::theme::WiniaTheme::colors(),
                crate::layout::LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    BottomSheetScaffold::new()
                        .sheet_state(st.clone())
                        .build(
                            ctx,
                            |ctx| { crate::components::Text::new("sheet").build(ctx); },
                            |ctx| { crate::components::Text::new("content").build(ctx); },
                        );
                },
            );
        });
        c.layout(Constraints::new(0.0, 900.0, 0.0, 720.0));
        // The sheet is the second child of the root — the page is the first. Found by
        // position rather than by width: a width match is ambiguous (the page is just as
        // wide) and stops matching when the window is narrower than the token.
        let root = c.layout_root_idx().expect("laid out");
        let sheet = c.arena_nodes()[root].children[1];
        assert_eq!(
            c.arena_nodes()[sheet].measured_size.width, 640.0,
            "the sheet is 640 wide in a 900-wide window (the token, not the window)"
        );
        assert_eq!(
            c.arena_nodes()[sheet].position.x, 130.0,
            "and it is centred from the left edge in RTL too, not mirrored to -130"
        );
    }
}
