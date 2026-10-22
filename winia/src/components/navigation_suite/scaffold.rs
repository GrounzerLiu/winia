use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::movable::MovableContent;
use crate::runtime::state::State;
use crate::layout::{Arrangement, Constraints, LayoutDirection, MeasurePolicy};
use crate::layout::node::{measure_node, Placement};
use crate::unit::{Offset, Size};
use crate::modifier::Modifier;
use crate::graphics::{Color, Shape};
use crate::components::navigation_bar::NavigationItemIconPosition;
use crate::components::short_navigation_bar::{ShortNavigationBar, ShortNavigationBarArrangement, ShortNavigationBarItem};
use crate::theme::WiniaTheme;
use crate::layout::components::{Column, Row, Stack};
use crate::layout::Alignment;
use crate::components::navigation_bar::NAVIGATION_BAR_HEIGHT;
use crate::components::navigation_rail::{
    NavigationRail, NavigationRailItem, WideNavigationRail, WideNavigationRailItem,
    WideNavigationRailState,
};
use crate::components::navigation_drawer::{DrawerSheet, NavigationDrawerItem};
use std::sync::Arc;

use super::types::{
    navigation_suite_type, NavigationSuiteColors, NavigationSuiteDefaults, NavigationSuiteItem,
    NavigationSuiteItems, NavigationSuiteScaffoldState, NavigationSuiteType,
};

/// 自适应导航套件脚手架：items + content，按窗口尺寸类选择导航形态。
pub struct NavigationSuiteScaffold {
    items: Vec<NavigationSuiteItem>,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    /// None = 按窗口尺寸自动推算
    layout_type: Option<NavigationSuiteType>,
    /// `state`: hide/show the whole suite (upstream `NavigationSuiteScaffoldState`).
    state: Option<NavigationSuiteScaffoldState>,
    /// `navigationSuiteColors`: the per-shape container/content pairs.
    navigation_suite_colors: Option<NavigationSuiteColors>,
    /// `contentColor`: what the app content is drawn in.
    content_color: Option<Color>,
    /// `navigationItemVerticalArrangement`: where the rail's items sit.
    vertical_arrangement: Arrangement,
    container_color: Option<Color>,
    /// `primaryActionContent`: a FAB floated above the bar, or `None`.
    primary_action: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    /// `primaryActionContentHorizontalAlignment` (default `Alignment::End`).
    primary_action_alignment: Alignment,
    /// 形态切换过渡（收拢→换形→展开）；默认开启
    transition: bool,
    modifier: Modifier,
}

/// 过渡阶段（私有状态机）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SuitePhase {
    /// 稳定（progress 目标 1.0——含换形后的展开期）
    Idle,
    /// 收拢中（progress 目标 0.0——到 0 换形态）
    Collapsing,
}

/// morph 轴（rail 收宽度 / bar 收高度）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MorphAxis {
    Width,
    Height,
}

impl NavigationSuiteScaffold {
    pub fn new(
        items: impl FnOnce(&mut NavigationSuiteItems),
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        let mut collector = NavigationSuiteItems::default();
        items(&mut collector);
        Self {
            items: collector.0,
            content: Box::new(content),
            layout_type: None,
            state: None,
            navigation_suite_colors: None,
            content_color: None,
            vertical_arrangement: NavigationSuiteDefaults::vertical_arrangement(),
            container_color: None,
            primary_action: None,
            primary_action_alignment: Alignment::End,
            transition: true,
            modifier: Modifier::new(),
        }
    }

    /// The FAB slot (`primaryActionContent`). WHERE it goes depends on the shape, which is upstream's
    /// rule (`NavigationSuite.kt:183-187`): "inside vertical navigation components as part of their
    /// header, and above horizontal navigation components".
    ///
    /// - a RAIL layout passes it as the rail's `header` (`NavigationSuite`, `:612-620`) — the same
    ///   slot the rail demos put their FAB in;
    /// - a BAR layout floats it 16dp above the bar, at `primary_action_alignment` (default `End`, 16dp
    ///   in from that edge).
    pub fn primary_action(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.primary_action = Some(Box::new(content));
        self
    }

    /// Where the primary action sits horizontally (`Start` / `Center` / `End`), 16dp from the edge
    /// for the two edge alignments.
    pub fn primary_action_alignment(mut self, alignment: Alignment) -> Self {
        self.primary_action_alignment = alignment;
        self
    }

    /// `state`: an explicit show/hide state. `NavigationSuiteScaffoldState::Hidden` animates the
    /// suite away through the same collapse the shape switch uses, and the content takes the box.
    pub fn state(mut self, state: NavigationSuiteScaffoldState) -> Self {
        self.state = Some(state);
        self
    }

    /// 覆盖自动推算（对齐 androidx layoutType 参数）
    pub fn layout_type(mut self, layout_type: NavigationSuiteType) -> Self {
        self.layout_type = Some(layout_type);
        self
    }

    /// `navigationSuiteColors`: override the per-shape container/content pairs.
    pub fn navigation_suite_colors(mut self, colors: NavigationSuiteColors) -> Self {
        self.navigation_suite_colors = Some(colors);
        self
    }

    /// `navigationItemVerticalArrangement`: where the rail's header and items sit vertically
    /// (`NavigationSuiteDefaults::vertical_arrangement()`, upstream's `Arrangement.Top`).
    pub fn vertical_arrangement(mut self, arrangement: Arrangement) -> Self {
        self.vertical_arrangement = arrangement;
        self
    }

    /// `contentColor`: the app content's colour (`onBackground` by default).
    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }

    /// 导航容器底色（content 区背景 = theme.background）
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// The shape-transition switch (on by default): collapse, swap the layout type, expand (220 ms
    /// linear).
    ///
    /// This is a winia extra, not an alignment: Compose switches the layout type with a plain `when`
    /// and moves the items with `movableContentOf` (`NavigationSuiteScaffold.kt:577-578`), with no
    /// transition at all. See the state machine below for why holding both shapes at once is not
    /// available to this component yet.
    pub fn transition(mut self, enabled: bool) -> Self {
        self.transition = enabled;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.layout_type);
        ctx.changed(&self.transition);
        ctx.changed(&self.primary_action_alignment);
        ctx.changed(&self.navigation_suite_colors);
        ctx.changed(&self.content_color);
        ctx.changed(&self.vertical_arrangement);
        let theme = WiniaTheme::colors();
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or_else(crate::layout::direction::current);
        let rtl = direction == LayoutDirection::Rtl;
        ctx.changed(&direction);
        // Each shape keeps ITS OWN default container, which is what upstream's
        // `NavigationSuiteDefaults.colors()` does by delegating to `ShortNavigationBarDefaults` for the
        // bar and to `WideNavigationRailDefaults` for the rail. Those two defaults differ:
        // `surface_container` for the bar (`short_navigation_bar.rs`) and `surface` for the wide rail
        // (`navigation_rail.rs`). Passing one value to both painted the rail a shade too light.
        // `container_color(..)` is winia's single override and wins over both; otherwise each shape takes
        // its own entry from `NavigationSuiteColors` (`NavigationSuiteDefaults.colors()`).
        let suite_colors = self
            .navigation_suite_colors
            .unwrap_or_else(|| NavigationSuiteDefaults::colors(&theme));
        let item_colors = NavigationSuiteDefaults::item_colors(&theme);
        let bar_item_colors = item_colors.short_navigation_bar;
        let rail_item_colors = item_colors.wide_navigation_rail;
        let content_color = self.content_color.unwrap_or(theme.on_background);
        let bar_container = self.container_color.unwrap_or(suite_colors.short_navigation_bar_container);
        let rail_container = self.container_color.unwrap_or(suite_colors.wide_navigation_rail_container);
        // HIDDEN keeps the SHAPE and only animates the navigation away — upstream's model, where
        // `animationProgress` goes to 0 and the layout type never changes (`NavigationSuiteScaffold
        // Layout`, `:411-475`). The content takes the freed space and the primary action rides the
        // same animation: in a bar layout it ends up 16dp from the bottom edge rather than vanishing.
        let hidden = self.state.as_ref().map(|s| !s.is_visible()).unwrap_or(false);
        let target_type = self.layout_type.unwrap_or_else(navigation_suite_type);
        ctx.changed(&hidden);
        let content = self.content;
        let primary_action = self.primary_action;
        let primary_action_alignment = self.primary_action_alignment;
        let vertical_arrangement = self.vertical_arrangement;

        // Optional winia-only morph: collapse the current container, swap the one active shape, then
        // expand. The branches never overlap. Compose's shape switch uses movableContentOf plus a
        // plain when (NavigationSuiteScaffold.kt:577-578); its scaffold visibility animation is a
        // separate concern. Set transition(false) for the direct switch behavior.
        // Payloads are repeatable Arc<dyn Fn>; the retained handles move their state between branches.
        let phase: State<SuitePhase> = ctx.remember(|| SuitePhase::Idle);
        let current: State<NavigationSuiteType> = ctx.remember(|| target_type);
        let next = ctx.remember_backchannel(|| None);

        let cur = current.get();
        let ph = phase.get();
        if hidden {
            // Nothing to swap: the progress below is what hides the navigation.
        } else if self.transition && form_kind(cur) != form_kind(target_type) {
            // 异形切换（bar↔rail）——morph 过渡
            next.set(Some(target_type));
            if ph == SuitePhase::Idle {
                phase.set(SuitePhase::Collapsing);
            }
        } else {
            // 同形切换（rail Expanded↔Collapsed）——容器宽度动画是组件内部的，
            // 静默换目标即可；过渡中目标变同形 = 取消收拢原地展开
            if cur != target_type {
                current.as_raw().set_backchannel(target_type);
            }
            if ph == SuitePhase::Collapsing {
                phase.set(SuitePhase::Idle);
            }
        }

        // ⚠ 动画目标必须用翻转后的 phase 新鲜读取：若用上方翻转前的旧值，
        // 检测帧 target 仍为 1.0（peek==target 不推送动画）→ is_animating=false
        // → 自驱帧链断裂，morph 冻结到下一个外部事件才走一帧（表现为卡顿）
        let progress = ctx.animate_float_as_state(
            if hidden || phase.get() == SuitePhase::Collapsing { 0.0 } else { 1.0 },
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(220),
                crate::animation::interpolator::Linear::new(),
            )),
        );
        let scale = progress.get().max(0.0).min(1.0);
        // Mount it into the caller's state so `is_animating` answers about THIS animation. Done
        // before the early-outs below, so a suite that is not animating still has a source.
        if let Some(state) = self.state.as_ref() {
            state.attach_progress(progress.clone());
        }

        // 收拢完成：换形态 + 展开（set 通知 → 下帧以新形态渲染）
        if !hidden && ph == SuitePhase::Collapsing && scale <= 0.01 {
            if let Some(t) = next.peek() {
                current.set(t);
            }
            next.set(None);
            phase.set(SuitePhase::Idle);
        }

        // ── items 作为 movable content（每个 item 两个句柄：icon / label）──
        //
        // Compose 让两个分支调用同一个内容（`NavigationSuiteScaffold.kt:577-578`），搬过去的是组合
        // 状态。winia 的粒度放在单个 item 的**载荷**上：两种形态的 item 子树结构不同，一个"整块
        // items"的 movable 内容帮不上忙；把载荷本身做成 movable 内容，形态只决定外面套什么，载荷里
        // remember 的东西就不会因为换形态而重建。
        //
        // 载荷每帧都是新闭包，句柄却必须跨帧稳定 —— 所以句柄在**循环位置**上 remember（同序同位置
        // = 同一个句柄），内容闭包通过 backchannel 读当帧载荷。
        let payloads = ctx.remember_backchannel(|| Vec::<NavigationSuiteItem>::new());
        payloads.set(self.items.clone());
        // This build receives fresh callbacks; their values cannot be compared through Arc<dyn Fn>.
        // Enter the actual shape hosts so the latest payloads reach their stored child scopes. Merely
        // staging a Backchannel would leave a clean bar/rail wrapper displaying the previous closure.
        ctx.mark_subtree_dirty();
        let mut handles: Vec<(MovableContent, MovableContent)> = Vec::with_capacity(self.items.len());
        for index in 0..self.items.len() {
            let icon_source = payloads.clone();
            let icon = ctx.remember_movable_content(move |ctx| {
                ctx.key(("suite-item-icon", index), |ctx| {
                    if let Some(item) = icon_source.get().get(index) {
                        (item.icon)(ctx);
                    }
                });
            });
            let label_source = payloads.clone();
            // Always a handle, even for an item without a label: the handles are positional, and one
            // that appears later must not shift the ones after it. Its content is the empty label a
            // rail needs, which is what the rail branch used to pass inline.
            let label = ctx.remember_movable_content(move |ctx| {
                ctx.key(("suite-item-label", index), |ctx| {
                    match label_source.get().get(index).and_then(|item| item.label.clone()) {
                        Some(label) => label(ctx),
                        None => {
                            crate::components::Text::new("").build(ctx);
                        }
                    }
                });
            });
            handles.push((icon, label));
        }
        let items_for_shape: Vec<NavigationSuiteItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (icon, label) = handles[index].clone();
                NavigationSuiteItem {
                    selected: item.selected,
                    on_click: item.on_click.clone(),
                    icon: Arc::new(move |ctx: &mut ComposeCtx| icon.compose(ctx)),
                    // Only when the caller gave one: an item without a label must stay without one,
                    // or a shape would lay out a label slot that nothing composes into.
                    label: item.label.as_ref().map(|_| {
                        let label = label.clone();
                        Arc::new(move |ctx: &mut ComposeCtx| label.compose(ctx))
                            as Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>
                    }),
                }
            })
            .collect();

        // Without the morph there is nothing to animate through, so render the target at once:
        // reading `cur` here left `transition(false)` a frame behind (the backchannel write below
        // only reaches the next build).
        let suite_type = if self.transition { cur } else { target_type };

        // 内容区背景（对标 scaffold containerColor = background）
        let content_modifier = Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .background(theme.background, crate::graphics::Shape::Rectangle);

        match suite_type {
            // The legacy shapes: the classic components, with the primary action where upstream puts it
            // (`NavigationSuite.kt:632-683`). No morph: upstream switches these with a plain `when`, and
            // the classic rail and drawer carry their own animation.
            NavigationSuiteType::NavigationRail => {

                Row::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        build_classic_rail_shape(
                            ctx,
                            items_for_shape.clone(),
                            suite_colors.navigation_rail_container,
                            rail_item_colors,
                            primary_action,
                            vertical_arrangement,
                        );
                        WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx));
                    });
            }
            NavigationSuiteType::NavigationDrawer => {
                Row::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        build_drawer_shape(
                            ctx,
                            items_for_shape.clone(),
                            suite_colors.navigation_drawer_container,
                            primary_action,
                            vertical_arrangement,
                        );
                        WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx));
                    });
            }
            NavigationSuiteType::None => {
                let k = ctx.next_key();
                match ctx.start_restartable_group(k, content_modifier, crate::layout::BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx)),
                }
                ctx.end_restartable_group();
            }
            // 底栏形态：Column { content(weight 1), bar }
            NavigationSuiteType::ShortNavigationBarCompact
            | NavigationSuiteType::ShortNavigationBarMedium
            | NavigationSuiteType::NavigationBar => {
                let (arrangement, icon_position, bar_container) = match suite_type {
                    NavigationSuiteType::ShortNavigationBarCompact => (
                        ShortNavigationBarArrangement::EqualWeight,
                        NavigationItemIconPosition::Top,
                        bar_container,
                    ),
                    // The legacy bar is the short bar in the classic bar's colours, with the icon above
                    // the label — upstream's own mapping (`NavigationSuite.kt:620-631`).
                    NavigationSuiteType::NavigationBar => (
                        ShortNavigationBarArrangement::EqualWeight,
                        NavigationItemIconPosition::Top,
                        suite_colors.navigation_bar_container,
                    ),
                    _ => (
                        ShortNavigationBarArrangement::Centered,
                        NavigationItemIconPosition::Start,
                        bar_container,
                    ),
                };
                // A Stack even without a primary action: it is what floats one above the bar.
                Stack::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    // A child that does not fill lands at the END, which is where the primary action
                    // goes; the Column below fills, so the alignment does not move it.
                    .alignment(match primary_action_alignment {
                        Alignment::Start => Alignment::Start,
                        Alignment::Center => Alignment::Center,
                        _ => Alignment::End,
                    })
                    .build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx| {
                        let k = ctx.next_key();
                        match ctx.start_restartable_group(k, content_modifier.layout_weight(1.0), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx)),
                        }
                        ctx.end_restartable_group();
                        let mk = ctx.next_key();
                        // morph 包裹（bar：高度轴）。clip 常驻——收拢期裁剪溢出内容，
                        // scale=1 时裁剪到全界无视觉差异（避免 modifier 翻转导致
                        // 槽位重进 → FnOnce items 重复消费）
                        let morph_modifier = if self.transition {
                            Modifier::new().clip(Shape::Rectangle)
                        } else {
                            Modifier::new()
                        };
                        let morph = SuiteMorphPolicy { progress: progress.clone(), axis: MorphAxis::Height };
                        match ctx.start_restartable_group(mk, morph_modifier, morph) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                build_bar_shape(
                                    ctx,
                                    items_for_shape.clone(),
                                    arrangement,
                                    icon_position,
                                    bar_container,
                                    bar_item_colors,
                                );
                            }
                        }
                        ctx.end_restartable_group();
                    });
                // `primaryActionContent`: `NavigationSuiteScaffoldLayout` places it at
                // `layoutHeight - h - PrimaryActionContentPadding(16) - navigationHeight`, with the
                // horizontal alignment picking the x (`:440-470`).
                if let Some(primary_action) = primary_action {
                    // The insets are PADDING, not a policy that reports the whole box: the hit test
                    // tests a node's CONTENT box, so a full-box layer above the content swallowed
                    // every click in the layout (measured: the bar's items and the content's button
                    // both stopped responding). With the insets as padding the node's content box is
                    // exactly the action's own rect, and clicking it still works.
                    let anim = progress.clone();
                    // The insets are an OFFSET, never padding. Padding grows the node, and hit testing
                    // walks NODE rects (`content_box()` IS `measured_size`), so a padded container
                    // reached down over the bar and swallowed the part of the last item to its right
                    // (reported on the demo: `Profile` right of the action was dead). An offset leaves
                    // the node exactly the action's own rect, so only the action is hittable.
                    // The alignment ITSELF mirrors (`Alignment::Start` is the left edge in LTR and the
                    // right one in RTL, `layout/node.rs`'s `anchor`), but `absolute_offset` does NOT —
                    // that is what the modifier documents. So the inset's sign follows the mirrored
                    // side, not the caller's word: without the flip an RTL `End` action was pushed
                    // another 16dp off the left edge instead of being inset from it.
                    // `Stretch` is not a horizontal placement for a floating child, so it takes the
                    // trailing edge like `End` rather than falling through to a sign nobody chose.
                    let x = match (primary_action_alignment, rtl) {
                        (Alignment::Center, _) => 0.0,
                        (Alignment::Start, false) | (Alignment::End, true) => PRIMARY_ACTION_PADDING,
                        (Alignment::End, false) | (Alignment::Start, true) => -PRIMARY_ACTION_PADDING,
                        _ => -PRIMARY_ACTION_PADDING,
                    };
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .absolute_offset(
                                    x,
                                // The action RIDES the navigation's animation — upstream places it at
                                // `layoutHeight - h - 16 - navigationHeight * animationProgress`
                                // (`:461-468`), so as the bar slides away it comes down to 16dp from the
                                // bottom edge and goes back up with the bar.
                                    crate::layout::Dimension::Dynamic(Arc::new(move || {
                                        -(PRIMARY_ACTION_PADDING
                                            + NAVIGATION_BAR_HEIGHT * anim.get().clamp(0.0, 1.0))
                                    })),
                                ),
                        )
                        .build(ctx, primary_action);
                }
                    });
            }
            // 宽轨形态：Row { rail, content(fill) }
            NavigationSuiteType::WideNavigationRailCollapsed | NavigationSuiteType::WideNavigationRailExpanded => {
                let expanded = suite_type == NavigationSuiteType::WideNavigationRailExpanded;
                let rail_state = WideNavigationRailState::new(ctx);
                if expanded { rail_state.expand(); } else { rail_state.collapse(); }
                // The item morph progress, on the same token the rail's own items use
                // (`material3/NavigationRail.kt:271`: `FastSpatial` = 0.9 / 1400). It drives the item
                // shape while the container runs `DefaultSpatial`, exactly as Compose's wide rail
                // splits them (`WideNavigationRail.kt:230`).
                let rail_item_progress = ctx.animate_float_as_state(
                    if expanded { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast()),
                );
                Row::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        // morph 包裹（rail：宽度轴）——clip 常驻理由同 bar 分支
                        let mk = ctx.next_key();
                        let morph_modifier = if self.transition {
                            Modifier::new().clip(Shape::Rectangle)
                        } else {
                            Modifier::new()
                        };
                        let morph = SuiteMorphPolicy { progress: progress.clone(), axis: MorphAxis::Width };
                        match ctx.start_restartable_group(mk, morph_modifier, morph) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                let items = items_for_shape.clone();
                                build_rail_shape(
                                    ctx,
                                    items,
                                    rail_state,
                                    rail_item_progress.clone(),
                                    rail_container,
                                    vertical_arrangement,
                                    rail_item_colors,
                                    primary_action,
                                );
                            }
                        }
                        ctx.end_restartable_group();
                        let k = ctx.next_key();
                        match ctx.start_restartable_group(k, content_modifier, crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx)),
                        }
                        ctx.end_restartable_group();
                    });
            }
        }
    }
}

/// 形态大类（morph 只在异形间触发；rail Expanded↔Collapsed 同形——
/// 容器宽度动画由 WideNavigationRail 内部进度承担）
fn form_kind(t: NavigationSuiteType) -> u8 {
    match t {
        NavigationSuiteType::ShortNavigationBarCompact
        | NavigationSuiteType::ShortNavigationBarMedium
        | NavigationSuiteType::NavigationBar => 0,
        NavigationSuiteType::WideNavigationRailCollapsed
        | NavigationSuiteType::WideNavigationRailExpanded
        | NavigationSuiteType::NavigationRail
        | NavigationSuiteType::NavigationDrawer => 1,
        NavigationSuiteType::None => 2,
    }
}

/// `PrimaryActionContentPadding` (`NavigationSuiteScaffoldLayout.kt:1591`).
pub(super) const PRIMARY_ACTION_PADDING: f32 = 16.0;


/// morph 容器：子项按全尺寸松约束测量、原点放置，容器报告按 progress 缩放
/// 的尺寸——溢出内容由节点 clip 裁剪。progress 读取注册 layout_deps——
/// 过渡帧只重测不重组。
#[derive(Debug)]
pub(super) struct SuiteMorphPolicy {
    pub(super) progress: State<f32>,
    pub(super) axis: MorphAxis,
}

impl MeasurePolicy for SuiteMorphPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        c: Constraints,
    ) -> (Size, Vec<Placement>) {
        let p = self.progress.get().max(0.0).min(1.0);
        let mut placements = Vec::new();
        let mut out = Size::new(0.0, 0.0);
        if let Some(&child) = children.first() {
            let loose = Constraints::new(0.0, c.max_width, 0.0, c.max_height);
            let (size, _) = measure_node(nodes, policies, child, loose);
            let sw = size.width * if self.axis == MorphAxis::Width { p } else { 1.0 };
            let sh = size.height * if self.axis == MorphAxis::Height { p } else { 1.0 };
            placements.push(Placement { size, position: Offset::new(0.0, 0.0) });
            out = Size::new(sw.min(c.max_width), sh.min(c.max_height));
        }
        (out, placements)
    }

    fn place(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        children: &[usize],
        placements: &[Placement],
    ) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}

/// The classic `NavigationRail` (the legacy `NavigationRail` shape). It takes no arrangement — the caller
/// spaces it, as upstream does — and its action is its header, like the wide rail's.
pub(super) fn build_classic_rail_shape(
    ctx: &mut ComposeCtx,
    items: Vec<NavigationSuiteItem>,
    container: Color,
    item_colors: crate::components::navigation_rail::NavigationRailItemColors,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    arrangement: Arrangement,
) {
    let mut rail = NavigationRail::new(move |ctx| {
        // Upstream spaces the classic rail BY HAND, inside the rail's own column
        // (`NavigationSuite.kt:640-657`): a leading spacer for Center/Bottom and a trailing one for
        // Center alone. The column is vertical, so the weighted spacer really does take height — the
        // first version of this put it in the scaffold's ROW, where the weight is a WIDTH.
        if matches!(arrangement, Arrangement::Center | Arrangement::End) {
            crate::layout::Spacer::vertical(0.0)
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx);
        }
        for item in items {
            let icon = item.icon.clone();
            let mut b = NavigationRailItem::new(item.selected, move |ctx| icon(ctx))
                .colors(item_colors)
                .on_click(move || {
                    if let Some(cb) = item.on_click.as_ref() {
                        cb()
                    }
                });
            if let Some(label) = item.label.clone() {
                b = b.label(move |ctx| label(ctx));
            }
            b.build(ctx);
        }
        if matches!(arrangement, Arrangement::Center) {
            crate::layout::Spacer::vertical(0.0)
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx);
        }
    })
    .container_color(container);
    if let Some(header) = header {
        rail = rail.header(header);
    }
    rail.build(ctx);
}

/// The permanent drawer sheet (the legacy `NavigationDrawer` shape): the action on top, then the rows.
pub(super) fn build_drawer_shape(
    ctx: &mut ComposeCtx,
    items: Vec<NavigationSuiteItem>,
    container: Color,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    arrangement: Arrangement,
) {
    DrawerSheet::permanent(move |ctx| {
        if let Some(header) = header {
            header(ctx);
        }
        // Same hand-spacing, same reason as the classic rail (`NavigationSuite.kt:659-683`).
        if matches!(arrangement, Arrangement::Center | Arrangement::End) {
            crate::layout::Spacer::vertical(0.0)
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx);
        }
        for item in items {
            let icon = item.icon.clone();
            let label = item.label.clone().unwrap_or_else(|| {
                Arc::new(|ctx: &mut ComposeCtx| {
                    crate::components::Text::new("").build(ctx);
                })
            });
            NavigationDrawerItem::new(move |ctx| label(ctx), item.selected)
                .icon(move |ctx| icon(ctx))
                .on_click(move || {
                    if let Some(cb) = item.on_click.as_ref() {
                        cb()
                    }
                })
                .build(ctx);
        }
        if matches!(arrangement, Arrangement::Center) {
            crate::layout::Spacer::vertical(0.0)
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx);
        }
    })
    .container_color(container)
    .build(ctx);
}

/// The wide rail. Shared like [`build_bar_shape`]; `progress` is the item morph progress the caller
/// already runs (the rail's own `FastSpatial` token), and `header` is where a VERTICAL shape takes the
/// primary action (`NavigationSuite.kt:183-187`, `:612-620`).
pub(super) fn build_rail_shape(
    ctx: &mut ComposeCtx,
    items: Vec<NavigationSuiteItem>,
    rail_state: WideNavigationRailState,
    progress: State<f32>,
    container: Color,
    arrangement: Arrangement,
    item_colors: crate::components::navigation_rail::NavigationRailItemColors,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
) {
    let mut rail = WideNavigationRail::new(rail_state, move |ctx| {
        for item in items {
            let icon = item.icon.clone();
            let label = item.label.clone().unwrap_or_else(|| {
                Arc::new(|ctx: &mut ComposeCtx| {
                    crate::components::Text::new("").build(ctx);
                })
            });
            let b = WideNavigationRailItem::new(item.selected, move |ctx| icon(ctx), move |ctx| label(ctx))
                .colors(item_colors)
                .progress(progress.clone())
                .on_click(move || {
                    if let Some(cb) = item.on_click.as_ref() {
                        cb()
                    }
                });
            b.build(ctx);
        }
    })
    .arrangement(arrangement)
    .container_color(container);
    if let Some(header) = header {
        rail = rail.header(header);
    }
    rail.build(ctx);
}

/// The short bar. Shared by the scaffold's shape host and a standalone [`NavigationSuite`]: the
/// construction is identical, and in the scaffold the items simply carry handle-backed payloads.
pub(super) fn build_bar_shape(
    ctx: &mut ComposeCtx,
    items: Vec<NavigationSuiteItem>,
    arrangement: ShortNavigationBarArrangement,
    icon_position: NavigationItemIconPosition,
    container: Color,
    item_colors: crate::components::navigation_bar::NavigationBarItemColors,
) {
    ShortNavigationBar::new(move |ctx| {
        for item in items {
            render_bar_item(ctx, item, icon_position, item_colors);
        }
    })
    .arrangement(arrangement)
    .container_color(container)
    .build(ctx);
}

#[composable]
fn render_bar_item(
    ctx: &mut ComposeCtx,
    item: NavigationSuiteItem,
    icon_position: NavigationItemIconPosition,
    colors: crate::components::navigation_bar::NavigationBarItemColors,
) {
    // The payloads are `Arc<dyn Fn>` (each composing that item's movable handle) while the item
    // component takes them by value once, so they are handed over as closures that call through.
    let icon = item.icon.clone();
    let mut b = ShortNavigationBarItem::new(item.selected, move |ctx| icon(ctx))
        .icon_position(icon_position)
        .colors(colors)
        .on_click(move || {
            if let Some(cb) = item.on_click.as_ref() {
                cb()
            }
        });
    if let Some(label) = item.label.clone() {
        b = b.label(move |ctx| label(ctx));
    }
    b.build(ctx);
}

