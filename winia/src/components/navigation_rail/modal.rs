use super::*;


// ── ModalWideNavigationRail（M3 Expressive 模态宽轨）──
//
// 对齐 androidx-main `WideNavigationRail.kt` 的 `ModalWideNavigationRail`：
// - 折叠时是一个普通折叠宽轨，留在布局流内；展开时抬起为一层 modal。上游用
//   `ModalWideNavigationRailDialog` 包住展开态，winia 走自己的 `open_overlay(modal: true)`
// - 展开容器的形状 = ModalContainerShape（CornerLarge 16dp）、颜色 = ModalContainerColor
//   （SurfaceContainer）；折叠态沿用普通轨的形状与容器色
// - 遮罩不自己画：overlay 的 `modal: true` 已经渲染半透明 scrim（110/255，与
//   `ModalNavigationDrawer` 自绘的那层同一个 alpha），点击遮罩与 Escape 都走 overlay 的
//   dismiss 通道，落到 `on_dismiss` 上收起同一份 state
// - 内容因此是 `Fn` 而不是 `FnOnce`：overlay 每帧重新组合它的内容
//   （`alert_dialog` 的注释记着同一件事），折叠那条路径每帧也只调用一次

/// Modal 展开容器的形状 = NavigationRailExpandedTokens.ModalContainerShape（CornerLarge 16dp）。
/// 折叠态沿用普通宽轨的形状（ContainerShape = CornerNone，即 `Shape::Rectangle`）。
pub fn modal_rail_expanded_shape() -> Shape {
    Shape::rounded(16.0)
}

pub struct ModalWideNavigationRail {
    pub(crate) state: WideNavigationRailState,
    pub(crate) content: Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>,
    pub(crate) header: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    pub(crate) hide_on_collapse: bool,
    pub(crate) collapsed_shape: Shape,
    pub(crate) expanded_shape: Shape,
    pub(crate) container_color: Option<Color>,
    pub(crate) modal_container_color: Option<Color>,
    pub(crate) expanded_header_top_padding: f32,
    pub(crate) modifier: Modifier,
}

impl ModalWideNavigationRail {
    pub fn new(
        state: WideNavigationRailState,
        content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state,
            content: Arc::new(content),
            header: None,
            hide_on_collapse: false,
            collapsed_shape: Shape::Rectangle,
            expanded_shape: modal_rail_expanded_shape(),
            container_color: None,
            modal_container_color: None,
            expanded_header_top_padding: 0.0,
            modifier: Modifier::new(),
        }
    }

    /// Slide offscreen when collapsed instead of staying as a collapsed rail — upstream's dismissible
    /// variant (`hideOnCollapse`). The caller owns moving its own container away; this only decides
    /// whether the collapsed rail is drawn at all.
    pub fn hide_on_collapse(mut self, hide: bool) -> Self {
        self.hide_on_collapse = hide;
        self
    }

    /// Collapsed container shape (default `Shape::Rectangle` = `ContainerShape`/`CornerNone`).
    pub fn collapsed_shape(mut self, shape: Shape) -> Self {
        self.collapsed_shape = shape;
        self
    }

    /// Expanded (modal) container shape — defaults to `ModalContainerShape`.
    pub fn expanded_shape(mut self, shape: Shape) -> Self {
        self.expanded_shape = shape;
        self
    }

    /// Collapsed container colour (default `Surface`, as for the plain rail).
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// Expanded container colour (defaults to `ModalContainerColor` = `surfaceContainer`).
    pub fn modal_container_color(mut self, c: Color) -> Self {
        self.modal_container_color = Some(c);
        self
    }

    /// Padding above the header in the EXPANDED panel — upstream's `expandedHeaderTopPadding`
    /// (`WideNavigationRail.kt:527`), applied as
    /// `Box(modifier = Modifier.padding(top = expandedHeaderTopPadding)) { header() }` at `:666`.
    ///
    /// Upstream's own documentation says what it is for: "It's usually needed in order to align the
    /// content of the rail between the collapsed and expanded animation" (`:508-510`). The two states
    /// do not share a vertical rhythm — a collapsed item is `TopIconItemMinHeight` 64 tall with a 4dp
    /// gap between items, an expanded one is 56 tall with no gap — so their items cannot line up one
    /// for one. What CAN line up is the first item, and this padding is how a caller moves the
    /// expanded panel's content to meet the collapsed rail's.
    ///
    /// Default 0.0, like upstream. Both of upstream's own usages pass 64dp (the height of the app bar
    /// above them): `NavigationRailSamples.kt:206` and `NavigationSuiteScaffoldDemo.kt:164`.
    pub fn expanded_header_top_padding(mut self, padding: f32) -> Self {
        self.expanded_header_top_padding = padding;
        self
    }

    pub fn header(mut self, h: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.header = Some(Arc::new(h));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.hide_on_collapse);
        ctx.changed(&self.collapsed_shape);
        ctx.changed(&self.expanded_shape);
        ctx.changed(&self.container_color);
        ctx.changed(&self.modal_container_color);
        ctx.changed(&self.expanded_header_top_padding);
        let expanded = self.state.is_expanded();
        ctx.changed(&expanded);
        let theme = WiniaTheme::colors();
        let header = self.header;
        let content = self.content;

        // The IN-FLOW rail. Upstream draws it whenever `hideOnCollapse` is false, expanded or not —
        // "Keep this rail even when expanded so that screen layout doesn't change"
        // (`WideNavigationRail.kt:570-591`) — and composes its content only while collapsed. winia
        // used to drop the whole node on expansion, which took a 96dp column out of the caller's
        // layout and pulled every column after it to the left.
        if !self.hide_on_collapse {
            let container = self.container_color.unwrap_or(theme.surface);
            let shape = self.collapsed_shape;
            let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
            let policy = WideNavigationRailLayoutPolicy {
                // Pinned at the collapsed end: this node is the collapsed rail in BOTH states, and
                // only the modal layer animates.
                progress: ctx.animate_float_as_state(0.0, rail_container_spec()),
                has_header: header.is_some(),
                rtl,
                // Pinned collapsed in the modal body; the arrangement is the caller's rail's.
                arrangement: Arrangement::Start,
            };
            let content = content.clone();
            let header = header.clone();
            let key = ctx.next_key();
            let root_modifier = self
                .modifier
                .clone()
                .fill_max_height()
                .background(container, shape);
            match ctx.start_restartable_group(key, root_modifier, policy) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // Content only while collapsed, so that it does not affect this collapsed rail
                    // or the screen layout.
                    if !expanded {
                        WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                            if let Some(header) = &header {
                                header(ctx);
                            }
                            content(ctx);
                        });
                    }
                }
            }
            ctx.end_restartable_group();
        }

        if !expanded {
            // Collapsed and nothing above the page. With `hide_on_collapse` there is not even the
            // rail above: the caller owns the offscreen position.
            return;
        }

        // Expanded: a layer above the page. `modal: true` is what draws the scrim, and a dismiss request
        // (scrim click or Escape) arrives as `on_dismiss`.
        let modal_color = self.modal_container_color.unwrap_or(theme.surface_container);
        let modal_shape = self.expanded_shape;
        let outer = self.modifier;
        let dismiss_state = self.state.clone();
        // The panel's own progress follows this same state (see the content closure): collapsing flips
        // `is_expanded()` while the overlay is still fading out, and that is what runs the width back down.
        let panel_state = self.state.clone();
        let id: State<u64> = ctx.remember(next_overlay_id);
        // Upstream applies the padding to the header INSIDE the expanded panel only
        // (`WideNavigationRail.kt:666`), never to the collapsed rail, so the collapsed branch above
        // keeps composing its header bare.
        let expanded_header_top_padding = self.expanded_header_top_padding;
        // The expanded panel's header slot exists whenever there is a header OR there is padding to
        // apply. Upstream's call site is `header = { Box(Modifier.padding(top = ...)) { header?.invoke() } }`
        // (`:665-669`) — a NON-null lambda even when the caller passed no header — so its layout always
        // measures a header placeable and `headerOffset = height + WNRHeaderPadding` always carries the
        // padding. Without that, a caller who only wants to shift the items would get nothing.
        let has_header = header.is_some() || expanded_header_top_padding > 0.0;
        // Captured out here for the same reason `direction` is passed to the overlay above: the content
        // closure below runs after composition, where `direction::current()` is no longer the truth.
        let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
        ctx.open_overlay(OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            // The rail knows its direction from COMPOSE time, where the composition local is still valid —
            // that is the only way `TopStart` can mirror under RTL: placement-time direction comes from the
            // anchor node, else the root node, and the root sits OUTSIDE the theme's direction provider
            // (settings::shell provides the direction for the window content, not for the window root).
            direction: Some(crate::layout::direction::current()),
            // `TopStart`, not `Center`: a modal rail hangs off the leading edge, and `Center` places the
            // panel by its OWN size in the middle of the window (`overlay.rs`: `Center => ((w - size.0) /
            // 2.0, (h - size.1) / 2.0)`). It is also what lets the container reach full height — the layout
            // policy takes `c.min_height` as its floor, and `fill_max_size` is what sets min = max; a
            // centred, loosely constrained panel never got that.
            position: PopupPosition::TopStart,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            // The modal rail owns the keyboard while it is up: Tab works inside the rail, and the page
            // behind the scrim cannot be reached.
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            dismiss_defers_to_content: false,
            click_passthrough: false,
            on_dismiss: Some(Arc::new(move || dismiss_state.collapse())),
            fit_around_anchor: false,
            match_anchor_width: false,
            // Fade with the scrim. The panel ALSO animates its width and shape out of the collapsed rail
            // (see the content closure below): that is winia's stand-in for upstream's `positionProgress` +
            // `modalState` driving the container out of its collapsed position.
            enter_anim: Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(200))),
            exit_anim: Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(200))),
            local_snapshot: Vec::new(),
            content: Box::new(move |ctx| {
                // The container hugs the leading edge and fills the height; its width is the layout
                // policy's (`min(available, 360)`, the modal cap), and the shape and colour are the modal
                // tokens rather than the plain rail's. Both the modifier and the policy are rebuilt here:
                // this closure is `Fn` (the overlay composes it every frame it is up), so nothing can be
                // moved out of it.
                // Grows out of the collapsed width on the rail's own spring, and shrinks back the same way:
                // the target FOLLOWS the state, so collapsing — which flips `is_expanded()` immediately,
                // while this overlay is still fading out — runs the width back down. NOT
                // `animate_float_as_state`: that seeds its `Animating` with the TARGET
                // (`composer.rs`: `Animating::new(target.clone())`), so its first frame would be at rest.
                let progress: State<f32> = ctx.remember(|| 0.0_f32);
                let target = if panel_state.is_expanded() { 1.0 } else { 0.0 };
                crate::animation::push_animatable(progress.clone(), target, modal_rail_container_spec());
                let panel = outer
                    .clone()
                    .fill_max_size()
                    .background(modal_color, modal_shape);
                let key = ctx.next_key();
                let policy = WideNavigationRailLayoutPolicy {
                    progress,
                    has_header,
                    rtl,
                    // The modal body's in-flow rail is always the collapsed one; the caller's
                    // arrangement belongs to the rail the panel was opened from.
                    arrangement: Arrangement::Start,
                };
                match ctx.start_restartable_group(key, panel, policy) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                            // The header slot carries the padding, so the rail's own
                            // `headerOffset = headerPlaceable.height + WNRHeaderPadding` moves the first
                            // item down by exactly this much — which is the whole point of the
                            // parameter. The slot is emitted even when the caller gave no header, as
                            // upstream's always-present wrapper Box is.
                            if expanded_header_top_padding > 0.0 {
                                wrap_slot(
                                    ctx,
                                    Modifier::new().padding_top(expanded_header_top_padding),
                                    |ctx| {
                                        if let Some(header) = &header {
                                            header(ctx);
                                        }
                                    },
                                );
                            } else if let Some(header) = &header {
                                header(ctx);
                            }
                            content(ctx);
                        });
                    }
                }
                ctx.end_restartable_group();
            }),
        });
    }
}

