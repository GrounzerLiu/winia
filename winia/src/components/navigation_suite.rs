//! NavigationSuiteScaffold——自适应导航套件（对齐 androidx
//! material3-adaptive-navigation-suite 的 NavigationSuiteScaffold /
//! NavigationSuiteType）。
//!
//! 一套 items 定义，按窗口尺寸类自动切换导航形态：
//!
//! - `ShortNavigationBarCompact`：窄窗（宽 Compact）——底栏 + Top 图标位 +
//!   EqualWeight 均布
//! - `ShortNavigationBarMedium`：矮窗（高 Compact）——底栏 + Start 图标位 +
//!   Centered 分组
//! - `WideNavigationRailCollapsed`：中宽（宽 Medium）——收起宽轨
//! - `WideNavigationRailExpanded`：宽窗（宽 Expanded）——展开宽轨
//! - `None`：不渲染导航
//!
//! 默认推算（[`navigation_suite_type`]）：窄窗→Compact 底栏；否则矮窗→Medium
//! 底栏；否则→**收起**宽轨（上游从不默认展开——展开是 rail 自己的状态，或显式
//! `.layout_type(WideNavigationRailExpanded)`）。窗口 resize 由 app 层触发重组，形态实时切换。
//!
//! `primaryActionContent`（FAB 槽）已实现：`.primary_action(..)` 浮在底栏上方 16dp，
//! 默认靠结束边 16dp，可用 `.primary_action_alignment(..)` 改。**上游只在底栏形态放置它**
//! （rail 分支只测量不摆放），这里照做——rail 形态要 FAB 请放进 rail 自己的 header。
//!
//! Each positional item's icon and label use separate movable handles. Their remembered values and
//! node identities survive bar/rail switches, and their own State reads still invalidate the current
//! host. The latest callback payloads are staged through a Backchannel; a scaffold build re-enters the
//! shape hosts so changed captures reach the retained content rather than remaining behind Skip.
//!
//! Verified by production-key tests and a deterministic UI round trip: all twelve tagged payload
//! nodes retain their identities, independent counters keep updating before/after moves, and labels
//! display new caller-captured values. Initializer counts or requested-shape text alone are not evidence.
//!
//! Deliberate differences: handles are positional (there is no keyed-destination API), the optional
//! collapse/swap/expand morph is winia-specific, and only icon/label payload state moves, not the
//! shape-specific item wrapper. Compose uses one retained content lambda and a plain `when`
//! (`NavigationSuiteScaffold.kt:577-578`). A fully absent placement is disposed, not cached.

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::movable::MovableContent;
use crate::runtime::state::State;
use crate::layout::{Arrangement, Constraints, LayoutDirection, MeasurePolicy};
use crate::layout::node::{measure_node, Placement};
use crate::unit::{Offset, Size};
use crate::layout::SizeValue;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::layout::adaptive::{window_height_size_class, window_width_size_class, HeightSizeClass, WidthSizeClass};
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

/// `NavigationSuiteColors`: the container/content pair each shape draws with (`NavigationSuiteDefaults
/// .colors()`).
///
/// Upstream carries a whole `WideNavigationRailColors` for the rail; winia's rail takes its container
/// and its ITEMS' colours separately, so the items live in [`NavigationSuiteItemColors`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationSuiteColors {
    pub short_navigation_bar_container: Color,
    pub short_navigation_bar_content: Color,
    pub wide_navigation_rail_container: Color,
    /// The legacy `NavigationBar` / `NavigationRail` / `NavigationDrawer` shapes.
    pub navigation_bar_container: Color,
    pub navigation_bar_content: Color,
    pub navigation_rail_container: Color,
    pub navigation_rail_content: Color,
    pub navigation_drawer_container: Color,
    pub navigation_drawer_content: Color,
}

/// `NavigationSuiteItemColors`: the per-shape item colour sets, passed straight through to the items
/// the suite builds (`ShortNavigationBarItem` / `WideNavigationRailItem`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationSuiteItemColors {
    pub short_navigation_bar: crate::components::navigation_bar::NavigationBarItemColors,
    pub wide_navigation_rail: crate::components::navigation_rail::NavigationRailItemColors,
}

/// `NavigationSuiteDefaults`.
pub struct NavigationSuiteDefaults;

impl NavigationSuiteDefaults {
    /// `NavigationSuiteDefaults.colors()`.
    pub fn colors(theme: &crate::theme::ThemeColors) -> NavigationSuiteColors {
        NavigationSuiteColors {
            // `ShortNavigationBarDefaults.containerColor` / `.contentColor`.
            short_navigation_bar_container: theme.surface_container,
            short_navigation_bar_content: theme.on_surface,
            // `WideNavigationRailDefaults.colors()`.
            wide_navigation_rail_container: theme.surface,
            // `NavigationBarDefaults.containerColor` / `contentColorFor(..)`.
            navigation_bar_container: theme.surface_container,
            navigation_bar_content: theme.on_surface,
            // `NavigationRailDefaults.ContainerColor` / `contentColorFor(..)`.
            navigation_rail_container: theme.surface,
            navigation_rail_content: theme.on_surface,
            // `DrawerDefaults.containerColor` (= the standard container).
            navigation_drawer_container: theme.surface,
            navigation_drawer_content: theme.on_surface,
        }
    }

    /// `NavigationSuiteDefaults.verticalArrangement`: where the rail's items sit (upstream's
    /// `Arrangement.Top`; winia's shared enum spells it `Start`).
    pub fn vertical_arrangement() -> Arrangement {
        Arrangement::Start
    }

    /// `NavigationSuiteDefaults.itemColors()`.
    pub fn item_colors(theme: &crate::theme::ThemeColors) -> NavigationSuiteItemColors {
        NavigationSuiteItemColors {
            short_navigation_bar: crate::components::navigation_bar::NavigationBarItemColors::from_theme(
                theme,
            ),
            wide_navigation_rail: crate::components::navigation_rail::NavigationRailItemColors::from_theme(
                theme,
            ),
        }
    }
}

/// 导航套件形态（对齐 androidx NavigationSuiteType 的 Expressive 子集）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationSuiteType {
    /// 底栏 + Top 图标位 + EqualWeight（窄窗）
    ShortNavigationBarCompact,
    /// 底栏 + Start 图标位 + Centered（矮窗）
    ShortNavigationBarMedium,
    /// 收起宽轨（中宽窗）
    WideNavigationRailCollapsed,
    /// 展开宽轨（宽窗）
    WideNavigationRailExpanded,
    /// 不渲染导航
    None,
    /// The legacy `NavigationBar` shape: the classic bottom bar (`NavigationSuiteType.NavigationBar`).
    /// Upstream advises against it ("use ShortNavigationBarCompact instead") and renders it as the
    /// short bar in the classic bar's colours (`NavigationSuite.kt:620-631`); so does this.
    NavigationBar,
    /// The legacy `NavigationRail` shape: the classic rail, with the primary action as its header
    /// (`NavigationSuite.kt:632-658`).
    NavigationRail,
    /// The legacy `NavigationDrawer` shape: a permanent drawer sheet, with the primary action on top
    /// (`NavigationSuite.kt:659-683`).
    NavigationDrawer,
}

/// The default shape, upstream's rule (`NavigationSuiteScaffoldDefaults.navigationSuiteType`):
///
/// ```text
/// width Compact                        -> ShortNavigationBarCompact
/// else tabletop || height Compact      -> ShortNavigationBarMedium
/// else                                 -> WideNavigationRailCollapsed
/// ```
///
/// The order matters: a window that is BOTH narrow and short takes the COMPACT bar, because the
/// width test comes first. And the last branch is the COLLAPSED rail — upstream never returns
/// `WideNavigationRailExpanded` here. Expanding is the rail's own business (its state, driven by the
/// caller's menu button) or an explicit `layout_type(WideNavigationRailExpanded)`.
///
/// The tabletop posture of the first `else if` is a foldable's half-open state; winia has no posture
/// concept, so only the height class is tested.
pub fn navigation_suite_type() -> NavigationSuiteType {
    if window_width_size_class() == WidthSizeClass::Compact {
        return NavigationSuiteType::ShortNavigationBarCompact;
    }
    if window_height_size_class() == HeightSizeClass::Compact {
        return NavigationSuiteType::ShortNavigationBarMedium;
    }
    NavigationSuiteType::WideNavigationRailCollapsed
}

/// 套件 item 数据（形态无关——由 scaffold 按当前形态渲染成
/// ShortNavigationBarItem 或 WideNavigationRailItem）
///
/// The payloads are `Arc<dyn Fn>` rather than `Box<dyn FnOnce>` because each one is composed as movable
/// content: a handle per item, invoked by whichever shape is showing, so the state a CALLER `remember`s
/// inside an `icon`/`label` payload moves with the item instead of being rebuilt when the shape changes
/// its item subtree.
#[derive(Clone)]
pub struct NavigationSuiteItem {
    pub(crate) selected: bool,
    pub(crate) on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) icon: Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>,
    pub(crate) label: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
}

/// items 收集器（`NavigationSuiteScaffold::new` 的 items 闭包参数）
#[derive(Default)]
pub struct NavigationSuiteItems(Vec<NavigationSuiteItem>);

impl NavigationSuiteItems {
    /// 追加一个目的地
    pub fn item(
        &mut self,
        selected: bool,
        icon: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
        label: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
        on_click: impl Fn() + Send + Sync + 'static,
    ) {
        self.0.push(NavigationSuiteItem {
            selected,
            on_click: Some(Arc::new(on_click)),
            icon: Arc::new(icon),
            label: Some(Arc::new(label)),
        });
    }

    /// 无 label 的目的地（rail 中渲染为空标签——WideNavigationRailItem label 必选）
    pub fn item_without_label(
        &mut self,
        selected: bool,
        icon: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
        on_click: impl Fn() + Send + Sync + 'static,
    ) {
        self.0.push(NavigationSuiteItem {
            selected,
            on_click: Some(Arc::new(on_click)),
            icon: Arc::new(icon),
            label: None,
        });
    }
}

/// `NavigationSuiteScaffoldValue`: whether the suite is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationSuiteScaffoldValue {
    /// The navigation is laid out.
    Visible,
    /// It is animated away and the content takes the whole box.
    Hidden,
}

/// `NavigationSuiteScaffoldState`: show/hide, with upstream's confirm hook.
///
/// Upstream's `rememberNavigationSuiteScaffoldState` also runs `confirmValueChange` before it commits,
/// so a caller can veto ("do not hide while a form is dirty"). Same here: `show`/`hide` ask first and
/// do nothing when the answer is no.
#[derive(Clone)]
pub struct NavigationSuiteScaffoldState {
    value: State<NavigationSuiteScaffoldValue>,
    confirm: Arc<dyn Fn(NavigationSuiteScaffoldValue) -> bool + Send + Sync>,
}

impl NavigationSuiteScaffoldState {
    /// `rememberNavigationSuiteScaffoldState(initialValue)`, with `confirmValueChange = { true }`.
    pub fn new(initial: NavigationSuiteScaffoldValue) -> Self {
        Self::with_confirm(initial, |_| true)
    }

    /// The same, with a veto.
    pub fn with_confirm(
        initial: NavigationSuiteScaffoldValue,
        confirm: impl Fn(NavigationSuiteScaffoldValue) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self { value: State::new(initial), confirm: Arc::new(confirm) }
    }

    pub fn current_value(&self) -> NavigationSuiteScaffoldValue {
        self.value.get()
    }

    pub fn is_visible(&self) -> bool {
        self.value.get() == NavigationSuiteScaffoldValue::Visible
    }

    /// Ask to move to `target`; a vetoed request leaves everything as it was.
    pub fn set(&self, target: NavigationSuiteScaffoldValue) {
        if (self.confirm)(target) {
            self.value.set(target);
        }
    }

    pub fn show(&self) {
        self.set(NavigationSuiteScaffoldValue::Visible);
    }

    pub fn hide(&self) {
        self.set(NavigationSuiteScaffoldValue::Hidden);
    }
}

/// `rememberNavigationSuiteScaffoldState()`.
#[composable]
pub fn remember_navigation_suite_scaffold_state(
    ctx: &mut ComposeCtx,
) -> NavigationSuiteScaffoldState {
    ctx.remember(|| NavigationSuiteScaffoldState::new(NavigationSuiteScaffoldValue::Visible))
        .get()
}

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
enum SuitePhase {
    /// 稳定（progress 目标 1.0——含换形后的展开期）
    Idle,
    /// 收拢中（progress 目标 0.0——到 0 换形态）
    Collapsing,
}

/// morph 轴（rail 收宽度 / bar 收高度）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MorphAxis {
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
                        // The classic rail takes no arrangement, so upstream spaces it by hand
                        // (`NavigationSuite.kt:640-657`): a leading spacer to push the items down.
                        if matches!(
                            vertical_arrangement,
                            Arrangement::Center | Arrangement::End
                        ) {
                            crate::layout::Spacer::vertical(0.0)
                                .modifier(Modifier::new().layout_weight(1.0))
                                .build(ctx);
                        }
                        build_classic_rail_shape(
                            ctx,
                            items_for_shape.clone(),
                            suite_colors.navigation_rail_container,
                            rail_item_colors,
                            primary_action,
                        );
                        WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx));
                    });
            }
            NavigationSuiteType::NavigationDrawer => {
                Row::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        // Same hand-spacing as the legacy rail, and the same reason — upstream puts the
                        // action first and then a leading spacer for Center/Bottom
                        // (`NavigationSuite.kt:659-683`).
                        if matches!(
                            vertical_arrangement,
                            Arrangement::Center | Arrangement::End
                        ) {
                            crate::layout::Spacer::vertical(0.0)
                                .modifier(Modifier::new().layout_weight(1.0))
                                .build(ctx);
                        }
                        build_drawer_shape(
                            ctx,
                            items_for_shape.clone(),
                            suite_colors.navigation_drawer_container,
                            primary_action,
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
                    let x = match (primary_action_alignment, rtl) {
                        (Alignment::Center, _) => 0.0,
                        (Alignment::Start, false) | (Alignment::End, true) => PRIMARY_ACTION_PADDING,
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
                                    SizeValue::Dynamic(Arc::new(move || {
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
const PRIMARY_ACTION_PADDING: f32 = 16.0;


/// morph 容器：子项按全尺寸松约束测量、原点放置，容器报告按 progress 缩放
/// 的尺寸——溢出内容由节点 clip 裁剪。progress 读取注册 layout_deps——
/// 过渡帧只重测不重组。
#[derive(Debug)]
struct SuiteMorphPolicy {
    progress: State<f32>,
    axis: MorphAxis,
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

#[composable]
/// The classic `NavigationRail` (the legacy `NavigationRail` shape). It takes no arrangement — the caller
/// spaces it, as upstream does — and its action is its header, like the wide rail's.
fn build_classic_rail_shape(
    ctx: &mut ComposeCtx,
    items: Vec<NavigationSuiteItem>,
    container: Color,
    item_colors: crate::components::navigation_rail::NavigationRailItemColors,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
) {
    let mut rail = NavigationRail::new(move |ctx| {
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
    })
    .container_color(container);
    if let Some(header) = header {
        rail = rail.header(header);
    }
    rail.build(ctx);
}

/// The permanent drawer sheet (the legacy `NavigationDrawer` shape): the action on top, then the rows.
fn build_drawer_shape(
    ctx: &mut ComposeCtx,
    items: Vec<NavigationSuiteItem>,
    container: Color,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
) {
    DrawerSheet::permanent(move |ctx| {
        if let Some(header) = header {
            header(ctx);
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
    })
    .container_color(container)
    .build(ctx);
}

/// The wide rail. Shared like [`build_bar_shape`]; `progress` is the item morph progress the caller
/// already runs (the rail's own `FastSpatial` token), and `header` is where a VERTICAL shape takes the
/// primary action (`NavigationSuite.kt:183-187`, `:612-620`).
fn build_rail_shape(
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
fn build_bar_shape(
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

// ═══════════════ NavigationSuiteScaffoldLayout ═══════════════

/// `NavigationSuiteScaffoldLayout`: the placement half of the scaffold, on its own.
///
/// Upstream splits the component so a caller can bring its OWN navigation and content and still get the
/// scaffold's geometry (`NavigationSuiteScaffoldLayout`, `:377-490`). The three slots are placed by
/// shape, and the numbers are upstream's:
///
/// - a BAR layout puts the content at the top with the height the navigation leaves, the navigation at
///   the bottom, and the primary action `PrimaryActionContentPadding` (16dp) above the navigation at the
///   caller's horizontal alignment;
/// - a RAIL layout puts the navigation at the start, full height, and the content beside it. The action
///   is NOT placed: for a vertical shape it belongs to the navigation itself (upstream's `NavigationSuite`
///   passes it as the rail's header), so the layout measures it and parks it off-view — the same thing
///   upstream's rail branch does, minus the composition staying alive.
pub struct NavigationSuiteScaffoldLayout {
    navigation_suite: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    primary_action: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    primary_action_alignment: Alignment,
    layout_type: NavigationSuiteType,
    content_color: Option<Color>,
    modifier: Modifier,
}

impl NavigationSuiteScaffoldLayout {
    /// `navigation_suite` is the navigation itself (`NavigationSuite`'s job upstream, or yours),
    /// `content` the app content beside/above it.
    pub fn new(
        navigation_suite: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            navigation_suite: Box::new(navigation_suite),
            content: Box::new(content),
            primary_action: None,
            primary_action_alignment: Alignment::End,
            layout_type: NavigationSuiteType::ShortNavigationBarCompact,
            content_color: None,
            modifier: Modifier::new(),
        }
    }

    /// The shape the placement follows (default `ShortNavigationBarCompact` = the bar geometry).
    pub fn layout_type(mut self, layout_type: NavigationSuiteType) -> Self {
        self.layout_type = layout_type;
        self
    }

    /// An action to float above a BAR layout; a rail layout leaves it to the navigation.
    pub fn primary_action(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.primary_action = Some(Box::new(content));
        self
    }

    pub fn primary_action_alignment(mut self, alignment: Alignment) -> Self {
        self.primary_action_alignment = alignment;
        self
    }

    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.layout_type);
        ctx.changed(&self.primary_action_alignment);
        let theme = WiniaTheme::colors();
        let content_color = self.content_color.unwrap_or(theme.on_background);
        let navigation_suite = self.navigation_suite;
        let content = self.content;
        let primary_action = self.primary_action;
        let policy = SuiteScaffoldLayoutPolicy {
            layout_type: self.layout_type,
            alignment: self.primary_action_alignment,
        };
        let key = ctx.next_key();
        let modifier = Modifier::new().fill_max_size().then(self.modifier);
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                navigation_suite(ctx);
                match primary_action {
                    Some(action) => action(ctx),
                    // The policy indexes children[1] as the action, so the slot stays even without one.
                    None => {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().size(0.0, 0.0));
                        ctx.end_node();
                    }
                }
                WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx));
            }
        }
        ctx.end_restartable_group();
    }
}

/// Places the three slots of [`NavigationSuiteScaffoldLayout`].
#[derive(Debug)]
struct SuiteScaffoldLayoutPolicy {
    layout_type: NavigationSuiteType,
    alignment: Alignment,
}

impl MeasurePolicy for SuiteScaffoldLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        c: Constraints,
    ) -> (Size, Vec<Placement>) {
        let is_bar = matches!(
            self.layout_type,
            NavigationSuiteType::ShortNavigationBarCompact
                | NavigationSuiteType::ShortNavigationBarMedium
                | NavigationSuiteType::NavigationBar
        );
        let (nav, _) = measure_node(
            nodes,
            policies,
            children[0],
            Constraints::new(0.0, c.max_width, 0.0, c.max_height),
        );
        // The content gets what the navigation leaves on the axis it took.
        let content_c = if is_bar {
            Constraints::new(0.0, c.max_width, 0.0, (c.max_height - nav.height).max(0.0))
        } else {
            Constraints::new(0.0, (c.max_width - nav.width).max(0.0), 0.0, c.max_height)
        };
        let (content, _) = measure_node(nodes, policies, children[2], content_c);
        let (action, _) = measure_node(
            nodes,
            policies,
            children[1],
            Constraints::new(0.0, c.max_width, 0.0, c.max_height),
        );
        let placements = if is_bar {
            let action_x = match self.alignment {
                Alignment::Start => PRIMARY_ACTION_PADDING,
                Alignment::Center => (c.max_width - action.width) / 2.0,
                _ => c.max_width - action.width - PRIMARY_ACTION_PADDING,
            };
            vec![
                Placement {
                    size: nav,
                    position: Offset::new(0.0, c.max_height - nav.height),
                },
                Placement {
                    size: action,
                    position: Offset::new(action_x, c.max_height - action.height - PRIMARY_ACTION_PADDING - nav.height),
                },
                Placement { size: content, position: Offset::new(0.0, 0.0) },
            ]
        } else {
            vec![
                Placement { size: nav, position: Offset::new(0.0, 0.0) },
                // A vertical shape keeps its action in its own header, so this slot is parked off-view
                // rather than placed (upstream's rail branch does not place it at all).
                Placement {
                    size: action,
                    position: Offset::new(c.max_width + 1000.0, 0.0),
                },
                Placement {
                    size: content,
                    position: Offset::new(nav.width, 0.0),
                },
            ]
        };
        (Size::new(c.max_width, c.max_height), placements)
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

// ═══════════════ NavigationSuite ═══════════════

/// `NavigationSuite`: the shape host on its own — one shape, its items, no scaffold.
///
/// Upstream's `NavigationSuite` (`:569-693`) exists so a caller can put the navigation into THEIR OWN
/// layout: it takes a type, the colours, the vertical arrangement, the primary action and the items, and
/// renders that one shape. winia's shapes were reachable only through the scaffold; this is the same
/// construction, and both now go through the same `build_*_shape` helpers.
///
/// It has no morph, no visibility state and no content: that is the scaffold's business. For a BAR shape
/// the action is ignored here — upstream floats it above the bar from the LAYOUT
/// ([`NavigationSuiteScaffoldLayout`]), not from the navigation itself.
pub struct NavigationSuite {
    items: Vec<NavigationSuiteItem>,
    layout_type: NavigationSuiteType,
    container_color: Option<Color>,
    vertical_arrangement: Arrangement,
    primary_action: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
}

impl NavigationSuite {
    pub fn new(
        items: impl FnOnce(&mut NavigationSuiteItems),
        layout_type: NavigationSuiteType,
    ) -> Self {
        let mut collector = NavigationSuiteItems::default();
        items(&mut collector);
        Self {
            items: collector.0,
            layout_type,
            container_color: None,
            vertical_arrangement: NavigationSuiteDefaults::vertical_arrangement(),
            primary_action: None,
            modifier: Modifier::new(),
        }
    }

    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }

    pub fn vertical_arrangement(mut self, arrangement: Arrangement) -> Self {
        self.vertical_arrangement = arrangement;
        self
    }

    /// For a VERTICAL shape this becomes the navigation's header; a bar shape has no slot for it here.
    pub fn primary_action(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.primary_action = Some(Box::new(content));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.layout_type);
        ctx.changed(&self.vertical_arrangement);
        ctx.changed(&self.container_color);
        let theme = WiniaTheme::colors();
        let colors = NavigationSuiteDefaults::colors(&theme);
        let item_colors = NavigationSuiteDefaults::item_colors(&theme);
        let container = self.container_color;
        let items = self.items;
        let action = self.primary_action;
        let arrangement = self.vertical_arrangement;
        let modifier = self.modifier;
        // A hugging wrapper carries the caller's modifier: the shapes each take their own, and the
        // helper signatures stay the shapes' business rather than this component's.
        crate::layout::components::Stack::new()
            .modifier(modifier)
            .build(ctx, |ctx| {
        match self.layout_type {
            NavigationSuiteType::ShortNavigationBarCompact
            | NavigationSuiteType::ShortNavigationBarMedium
            | NavigationSuiteType::NavigationBar => {
                let (bar_arrangement, icon_position, default_container) = match self.layout_type {
                    NavigationSuiteType::ShortNavigationBarCompact => (
                        ShortNavigationBarArrangement::EqualWeight,
                        NavigationItemIconPosition::Top,
                        colors.short_navigation_bar_container,
                    ),
                    NavigationSuiteType::NavigationBar => (
                        ShortNavigationBarArrangement::EqualWeight,
                        NavigationItemIconPosition::Top,
                        colors.navigation_bar_container,
                    ),
                    _ => (
                        ShortNavigationBarArrangement::Centered,
                        NavigationItemIconPosition::Start,
                        colors.short_navigation_bar_container,
                    ),
                };
                build_bar_shape(
                    ctx,
                    items,
                    bar_arrangement,
                    icon_position,
                    container.unwrap_or(default_container),
                    item_colors.short_navigation_bar,
                );
            }
            NavigationSuiteType::WideNavigationRailCollapsed
            | NavigationSuiteType::WideNavigationRailExpanded => {
                let expanded = self.layout_type == NavigationSuiteType::WideNavigationRailExpanded;
                let rail_state = WideNavigationRailState::new(ctx);
                if expanded {
                    rail_state.expand();
                } else {
                    rail_state.collapse();
                }
                // The rail's own item token (`material3/NavigationRail.kt:271`: `FastSpatial`).
                let progress = ctx.animate_float_as_state(
                    if expanded { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast()),
                );
                build_rail_shape(
                    ctx,
                    items,
                    rail_state,
                    progress,
                    container.unwrap_or(colors.wide_navigation_rail_container),
                    arrangement,
                    item_colors.wide_navigation_rail,
                    action,
                );
            }
            NavigationSuiteType::NavigationRail => build_classic_rail_shape(
                ctx,
                items,
                container.unwrap_or(colors.navigation_rail_container),
                item_colors.wide_navigation_rail,
                action,
            ),
            NavigationSuiteType::NavigationDrawer => build_drawer_shape(
                ctx,
                items,
                container.unwrap_or(colors.navigation_drawer_container),
                action,
            ),
            NavigationSuiteType::None => {}
        }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::constraints::Constraints;
    use crate::layout::node::LayoutNode;
    use crate::modifier::Modifier as M;

    fn icon(ctx: &mut ComposeCtx, size: f32) {
        let k = ctx.next_key();
        ctx.start_leaf(k, M::new().size(size, size));
        ctx.end_node();
    }

    fn scaffold() -> NavigationSuiteScaffold {
        NavigationSuiteScaffold::new(
            |items| {
                for i in 0..3 {
                    let _ = i;
                    items.item(
                        i == 0,
                        |ctx| icon(ctx, 24.0),
                        |ctx| crate::components::Text::new("Home").build(ctx),
                        || {},
                    );
                }
            },
            |ctx| {
                let k = ctx.next_key();
                ctx.start_leaf(k, M::new());
                ctx.end_node();
            },
        )
    }

    #[test]
    fn default_type_follows_window_size_classes() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
        crate::layout::adaptive::set_window_size(700.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        // Expanded width still gets the COLLAPSED rail: upstream's rule never returns the expanded
        // one — expanding is the rail's own state, or an explicit `layout_type(..)`.
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        // A short window gets the horizontal bar ...
        crate::layout::adaptive::set_window_size(1000.0, 400.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarMedium);
        // ... but the WIDTH test comes first: a window that is both narrow and short is Compact.
        crate::layout::adaptive::set_window_size(500.0, 400.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
    }

    /// The primary action follows the SHAPE. Upstream's contract is "inside vertical navigation
    /// components as part of their header, and above horizontal navigation components"
    /// (`NavigationSuite.kt:183-187`), so a rail layout passes it as the rail's `header` and a bar
    /// layout floats it over the bar. Bar-only was wrong, and it is why a wide window showed no FAB.
    #[test]
    fn the_primary_action_becomes_the_rails_header() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut found: Option<(f32, f32)> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            y: f32,
            out: &mut Option<(f32, f32)>,
        ) {
            let n = &nodes[idx];
            let (ax, ay) = (x + n.position.x, y + n.position.y);
            if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                *out = Some((ax, ay));
            }
            for &child in &n.children {
                walk(nodes, child, ax, ay, out);
            }
        }
        walk(nodes, root, 0.0, 0.0, &mut found);
        let (x, y) = found.expect("the rail layout carries the primary action in its header");
        assert!(x >= 0.0 && x <= 96.0, "inside the 96dp rail, x={x}");
        assert!(y < 200.0, "above the rail's items, y={y}");
    }

    /// `primaryActionContent` floats above the bar: 16dp clear of it, 16dp in from the trailing edge
    /// with the default `Alignment::End` (`NavigationSuiteScaffoldLayout` `:456-470`).
    #[test]
    /// The action must never steal clicks from what sits beside it. It used to: the insets were PADDING,
    /// padding grows the node, and hit testing walks node rects (`content_box()` is `measured_size`) — so
    /// the container reached down over the bar and the part of the last item to its right went dead
    /// (reported on the demo, `Profile`). The insets are an offset now, and this is the guard.
    #[test]
    /// RTL: the action is inset from the LOGICAL end, which is the LEFT edge when the layout runs
    /// right-to-left. The alignment mirrors itself, `absolute_offset` does not, so the sign is flipped by
    /// hand — without that it was pushed 16dp off the left edge instead of inset from it.
    #[test]
    fn the_primary_action_is_inset_from_the_logical_end_under_rtl() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        crate::layout::direction::provides(LayoutDirection::Rtl, || {
            composer.compose(|ctx| {
                scaffold()
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut found: Option<f32> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            out: &mut Option<f32>,
        ) {
            let n = &nodes[idx];
            let ax = x + n.position.x;
            if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                *out = Some(ax);
            }
            for &child in &n.children {
                walk(nodes, child, ax, out);
            }
        }
        walk(nodes, root, 0.0, &mut found);
        let x = found.expect("the action is composed");
        assert!(
            (x - PRIMARY_ACTION_PADDING).abs() < 0.5,
            "16dp in from the LEFT edge under RTL, got {x}"
        );
    }
    fn the_primary_action_does_not_swallow_the_item_beside_it() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0).test_tag("suite-action"));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let action = nodes
            .iter()
            .position(|n| n.modifier.get_test_tag() == Some("suite-action"))
            .expect("the action is composed");
        let hits = |x: f32, y: f32| crate::layout::node::hit_test(nodes, root, x, y);
        // On the action itself (it is 448..504 x 568..624): hittable.
        assert!(
            hits(470.0, 590.0).contains(&action),
            "the action answers for its own rect"
        );
        // On the LAST bar item, to the right of the action's x-range in the bar's own row (y 640..720):
        // the action must not be in that path.
        let beside = hits(470.0, 680.0);
        assert!(beside.contains(&root), "the point is inside the layout");
        assert!(
            !beside.contains(&action),
            "a click beside the action reaches what is there, not the action"
        );
    }
    fn the_primary_action_floats_above_the_bar() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // The Stack's children are [the Column, the action's padded container]; the container is
        // END-aligned and carries the insets, so the payload's ABSOLUTE position is the two
        // positions added up.
        let children = &nodes[root].children;
        assert_eq!(children.len(), 2, "the stack holds the bar layout and the action");
        let container = &nodes[children[1]];
        let fab = &nodes[container.children[0]];
        assert_eq!(fab.measured_size.width, 56.0);
        let abs_x = container.position.x + fab.position.x;
        let abs_y = container.position.y + fab.position.y;
        assert!(
            (abs_x - (500.0 - 56.0 - 16.0)).abs() < 0.01,
            "16dp in from the trailing edge, x={abs_x}"
        );
        assert!(
            (abs_y - (700.0 - 56.0 - 16.0 - 80.0)).abs() < 0.01,
            "16dp above the 80dp bar, y={abs_y}"
        );
    }

    /// `NavigationSuiteDefaults::colors()` delegates to each shape's own component default, which is what
    /// upstream's `colors()` does — and those defaults are NOT all the same value.
    /// The two legacy types that had NO coverage at all.
    ///
    /// `NavigationBar` is the short bar with the icon ABOVE the label (the only visible difference from
    /// `ShortNavigationBarMedium`, whose icon sits to the left), and `NavigationDrawer` is a permanent
    /// drawer sheet: a 240..360dp column at the leading edge whose rows are the drawer's own items.
    #[test]
    /// The RAIL layout under RTL — the half the earlier RTL bug's class had not been checked on. The Row
    /// mirrors, so the rail docks on the RIGHT, and the primary action keeps its inset inside it.
    ///
    /// Measured in the demo first (921x544): LTR rail at x=0 with the action at 20; RTL rail at 825
    /// (= 921 - 96) with the action at 845, i.e. the same 20dp.
    #[test]
    /// `vertical_arrangement` reaches the rail: `End` pushes the header + items to the bottom of the
    /// rail, `Start` (the default, upstream's `Top`) leaves them at `TopSpace`. Only a rail with SLACK
    /// can tell them apart, which is why the suite's rail fills the window's height.
    #[test]
    /// `NavigationSuiteScaffoldLayout` places three slots by shape, with upstream's numbers.
    ///
    /// A BAR layout gives the content the height the navigation leaves, pins the navigation to the bottom
    /// and floats the action 16dp above it at the caller's alignment. A RAIL layout puts the navigation at
    /// the start and the content beside it, and PARKS the action off-view — a vertical shape keeps it in
    /// its own header, which is what upstream's rail branch means by never placing it.
    #[test]
    /// `NavigationSuite` renders the ONE shape it is given, with no scaffold around it, through the same
    /// helpers the scaffold's shape host uses: a bar type gives the 80dp bar, a rail type the 96dp
    /// collapsed rail, and the classic type the 80dp classic rail.
    #[test]
    fn the_standalone_suite_renders_the_shape_it_is_given() {
        let size = |t: NavigationSuiteType| -> (f32, f32) {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                NavigationSuite::new(
                    |items| {
                        for _ in 0..3 {
                            items.item(
                                true,
                                |ctx| icon(ctx, 24.0),
                                |ctx| crate::components::Text::new("A").build(ctx),
                                || {},
                            );
                        }
                    },
                    t,
                )
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let s = nodes[root].measured_size;
            (s.width, s.height)
        };
        let bar = size(NavigationSuiteType::ShortNavigationBarCompact);
        assert!((bar.1 - 80.0).abs() < 0.5, "a bar is 80dp tall, got {bar:?}");
        let rail = size(NavigationSuiteType::WideNavigationRailCollapsed);
        assert!((rail.0 - 96.0).abs() < 0.5, "a collapsed wide rail is 96dp wide, got {rail:?}");
        let classic = size(NavigationSuiteType::NavigationRail);
        assert!((classic.0 - 80.0).abs() < 0.5, "the classic rail is 80dp wide, got {classic:?}");
    }

    fn the_scaffold_layout_places_the_three_slots() {
        let probe = |t: NavigationSuiteType| -> Vec<(f32, f32, f32, f32)> {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                NavigationSuiteScaffoldLayout::new(
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                        ctx.end_node();
                    },
                    // Fills, so the height it is given is the height it reports.
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().fill_max_size());
                        ctx.end_node();
                    },
                )
                .layout_type(t)
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .map(|&c| {
                    let n = &nodes[c];
                    (
                        n.position.x,
                        n.position.y,
                        n.measured_size.width,
                        n.measured_size.height,
                    )
                })
                .collect()
        };

        let bar = probe(NavigationSuiteType::ShortNavigationBarCompact);
        assert_eq!(bar.len(), 3, "navigation, action, content");
        assert_eq!((bar[0].0, bar[0].1), (0.0, 620.0), "the navigation is pinned to the bottom");
        assert_eq!(bar[2].3, 620.0, "and the content gets the height it leaves");
        assert_eq!(
            (bar[1].0, bar[1].1),
            (428.0, 548.0),
            "the action is 16dp above the navigation and 16dp in from the end"
        );

        let rail = probe(NavigationSuiteType::WideNavigationRailCollapsed);
        assert_eq!((rail[0].0, rail[0].1), (0.0, 0.0), "the rail sits at the start");
        assert_eq!(rail[2].0, 96.0, "and the content starts after it");
        assert!(
            rail[1].0 > 500.0,
            "the action is parked off-view for a vertical shape, got {}",
            rail[1].0
        );
    }

    fn the_vertical_arrangement_moves_the_rails_items() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let first_item_y = |arrangement: Arrangement| -> f32 {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                scaffold()
                    .layout_type(NavigationSuiteType::WideNavigationRailCollapsed)
                    .vertical_arrangement(arrangement)
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut y: Option<f32> = None;
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                x: f32,
                cy: f32,
                out: &mut Option<f32>,
            ) {
                let n = &nodes[idx];
                let (ax, ay) = (x + n.position.x, cy + n.position.y);
                let s = n.measured_size;
                if out.is_none() && ax < 96.0 && s.width > 40.0 && s.height == 64.0 {
                    *out = Some(ay);
                }
                for &child in &n.children {
                    walk(nodes, child, ax, ay, out);
                }
            }
            walk(nodes, root, 0.0, 0.0, &mut y);
            y.expect("the rail's first item is laid out")
        };
        let start = first_item_y(Arrangement::Start);
        let end = first_item_y(Arrangement::End);
        assert!(
            end > start + 100.0,
            "End pushes the stack to the bottom: start {start}, end {end}"
        );
    }

    fn the_rail_layout_docks_on_the_right_under_rtl() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        crate::layout::direction::provides(LayoutDirection::Rtl, || {
            composer.compose(|ctx| {
                scaffold()
                    .layout_type(NavigationSuiteType::WideNavigationRailCollapsed)
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut rail_x: Option<f32> = None;
        let mut action_x: Option<f32> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            rail: &mut Option<f32>,
            action: &mut Option<f32>,
        ) {
            let n = &nodes[idx];
            let ax = x + n.position.x;
            let s = n.measured_size;
            if rail.is_none() && s.width == 96.0 && s.height > 300.0 {
                *rail = Some(ax);
            }
            if action.is_none() && s.width == 56.0 && s.height == 56.0 {
                *action = Some(ax);
            }
            for &child in &n.children {
                walk(nodes, child, ax, rail, action);
            }
        }
        walk(nodes, root, 0.0, &mut rail_x, &mut action_x);
        let rail = rail_x.expect("the rail is laid out");
        assert!(
            (rail - (1000.0 - 96.0)).abs() < 1.0,
            "the rail docks on the RIGHT edge under RTL, got {rail}"
        );
        let action = action_x.expect("the action is composed");
        assert!(
            (action - (rail + 20.0)).abs() < 1.0,
            "and the action keeps its 20dp inset inside it, rail={rail} action={action}"
        );
    }

    fn the_legacy_bar_and_drawer_render_their_classic_shapes() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        // (the first 24x24 icon, the first text, a 56-tall row that spans >=200, the widest 80-tall box)
        type Seen = (Option<(f32, f32)>, Option<(f32, f32)>, Option<(f32, f32)>, Option<f32>);
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            y: f32,
            out: &mut Seen,
        ) {
            let n = &nodes[idx];
            let (ax, ay) = (x + n.position.x, y + n.position.y);
            let s = n.measured_size;
            let has_text = n.modifier.elements().iter().any(|el| {
                matches!(el, crate::modifier::ModifierElement::TextContent { .. })
            });
            if out.0.is_none() && s.width == 24.0 && s.height == 24.0 {
                out.0 = Some((ax, ay));
            }
            if out.1.is_none() && has_text {
                out.1 = Some((ax, ay));
            }
            if out.2.is_none() && s.height == 56.0 && s.width >= 200.0 {
                out.2 = Some((ax, ay));
            }
            if s.height == 80.0 && s.width > 900.0 {
                out.3 = Some(s.width);
            }
            for &child in &n.children {
                walk(nodes, child, ax, ay, out);
            }
        }
        let probe = |t: NavigationSuiteType| -> Seen {
            let mut composer = Composer::new();
            composer.compose(|ctx| scaffold().layout_type(t).build(ctx));
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out: Seen = (None, None, None, None);
            walk(nodes, root, 0.0, 0.0, &mut out);
            out
        };

        let (icon, label, _, bar_width) = probe(NavigationSuiteType::NavigationBar);
        let icon = icon.expect("the legacy bar draws its icons");
        let label = label.expect("and its labels");
        assert!(
            icon.1 < label.1,
            "the icon is ABOVE the label (the Top position), icon y={} label y={}",
            icon.1,
            label.1
        );
        assert!(
            bar_width.is_some_and(|w| w > 900.0),
            "and the classic bar spans the window, got {bar_width:?}"
        );

        let (_, label, row, bar) = probe(NavigationSuiteType::NavigationDrawer);
        let row = row.expect("the drawer renders its own item rows");
        assert!(row.0 < 360.0, "in a leading-edge column, x={}", row.0);
        assert!(label.is_some(), "and those rows carry labels");
        assert!(bar.is_none(), "and there is no 80dp bar in this shape");
    }

    /// The legacy types render the CLASSIC components, not the short bar / wide rail: `NavigationRail`
    /// is the 80dp classic rail and takes the primary action as its header, as upstream's own mapping
    /// does (`NavigationSuite.kt:632-658`).
    #[test]
    fn the_legacy_navigation_rail_uses_the_classic_component() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .layout_type(NavigationSuiteType::NavigationRail)
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let rail = &nodes[nodes[root].children[0]];
        assert!(
            (rail.measured_size.width - 80.0).abs() < 0.5,
            "the classic rail is 80dp wide (the wide one is 96), got {}",
            rail.measured_size.width
        );
        let mut action_x: Option<f32> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            out: &mut Option<f32>,
        ) {
            let n = &nodes[idx];
            let ax = x + n.position.x;
            if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                *out = Some(ax);
            }
            for &child in &n.children {
                walk(nodes, child, ax, out);
            }
        }
        walk(nodes, root, 0.0, &mut action_x);
        let x = action_x.expect("the classic rail carries the primary action as its header");
        assert!(x < 80.0, "inside the rail, x={x}");
    }

    /// `NavigationSuiteDefaults::colors()` delegates to each shape's own component default, which is
    /// what upstream's `colors()` does — and those defaults are NOT all the same value.
    #[test]
    fn the_suite_colors_default_to_each_shapes_own_component_default() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let c = NavigationSuiteDefaults::colors(&theme);
        assert_eq!(
            c.short_navigation_bar_container,
            theme.surface_container,
            "ShortNavigationBarDefaults.containerColor"
        );
        assert_eq!(
            c.wide_navigation_rail_container,
            theme.surface,
            "WideNavigationRailDefaults.colors()"
        );
        assert_eq!(c.short_navigation_bar_content, theme.on_surface);
        assert_eq!(c.navigation_drawer_container, theme.surface, "the standard container");
        let items = NavigationSuiteDefaults::item_colors(&theme);
        assert_eq!(
            items.short_navigation_bar,
            crate::components::navigation_bar::NavigationBarItemColors::from_theme(&theme),
            "the item colours are the bar's own"
        );
    }

    /// `NavigationSuiteScaffoldState` animates the navigation away and back, and the PRIMARY ACTION
    /// rides that animation: upstream places it at
    /// `layoutHeight - h - 16 - navigationHeight * animationProgress` (`:461-468`), so hiding a bar
    /// layout leaves it floating 16dp above the bottom edge instead of removing it with the bar.
    #[test]
    fn the_state_hides_the_suite_and_the_action_rides_the_animation() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let state = NavigationSuiteScaffoldState::new(NavigationSuiteScaffoldValue::Visible);
        let mut composer = Composer::new();
        let mut build = |c: &mut Composer, settle: bool| {
            let st = state.clone();
            c.compose(move |ctx| {
                scaffold()
                    .state(st.clone())
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            c.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            if settle {
                for _ in 0..400 {
                    if !crate::animation::update_animations() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    let st = state.clone();
                    c.compose(move |ctx| {
                        scaffold()
                            .state(st.clone())
                            .primary_action(|ctx| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                    });
                    c.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
                }
            }
        };
        // (the bar CONTAINER's height, the action's y). The bar itself keeps its 80dp measured size —
        // the morph container around it is what scales, and that is what the eye sees.
        let probe = |c: &Composer| -> (f32, Option<f32>) {
            let mut bar_h = -1.0f32;
            let mut action_y: Option<f32> = None;
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                y: f32,
                parent_h: f32,
                bar_h: &mut f32,
                action: &mut Option<f32>,
            ) {
                let n = &nodes[idx];
                let ay = y + n.position.y;
                if n.measured_size.height == 80.0 && n.measured_size.width > 300.0 {
                    *bar_h = parent_h;
                }
                if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                    *action = Some(ay);
                }
                for &child in &n.children {
                    walk(nodes, child, ay, n.measured_size.height, bar_h, action);
                }
            }
            let root = c.layout_root_idx().unwrap();
            walk(c.arena_nodes(), root, 0.0, -1.0, &mut bar_h, &mut action_y);
            (bar_h, action_y)
        };
        build(&mut composer, false);
        assert_eq!(probe(&composer).0, 80.0, "visible: the bar's container is full height");
        assert_eq!(probe(&composer).1, Some(548.0), "action 16dp above the 80dp bar");

        state.hide();
        build(&mut composer, true);
        // The tween settles within its threshold, so the container keeps a sliver rather than an exact
        // zero — the tolerance is the engine's, not the layout's.
        assert!(
            probe(&composer).0 < 4.0,
            "hidden: the bar's container has animated away, got {}",
            probe(&composer).0
        );
        let y = probe(&composer).1.expect("the action is still there when the suite is hidden");
        assert!(
            (y - 628.0).abs() < 4.0,
            "and it came down with the bar: 16dp from the bottom edge, got {y}"
        );

        state.show();
        build(&mut composer, true);
        assert!(
            probe(&composer).0 > 76.0,
            "shown again, got {}",
            probe(&composer).0
        );
        let y = probe(&composer).1.expect("the action is still there");
        assert!((y - 548.0).abs() < 4.0, "and it went back up with the bar, got {y}");
    }

    /// `confirmValueChange`: a vetoed request changes nothing (upstream's hook, same contract).
    #[test]
    fn a_veto_keeps_the_suite_visible() {
        let vetoing = NavigationSuiteScaffoldState::with_confirm(
            NavigationSuiteScaffoldValue::Visible,
            |target| target == NavigationSuiteScaffoldValue::Visible,
        );
        vetoing.hide();
        assert!(vetoing.is_visible(), "the veto held");
        assert_eq!(vetoing.current_value(), NavigationSuiteScaffoldValue::Visible);
    }

    /// 窄窗 → 底栏形态：Column 根，bar 在底部（高 80、贴底）
    #[test]
    fn narrow_window_renders_bottom_bar() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // Stack 根（primary action 的宿主）→ Column → 最后子项 = bar（高 80、y = 700-80 = 620）
        let column = &nodes[*nodes[root].children.last().unwrap()];
        let bar = &nodes[*column.children.last().unwrap()];
        assert_eq!(bar.measured_size.height, 80.0);
        assert!((bar.position.y - 620.0).abs() < 0.01, "bar 贴底 y={}", bar.position.y);
    }

    /// 宽窗 → **收起**宽轨（上游默认从不展开）：Row 根，rail 在前（宽 96），内容填余下
    #[test]
    fn wide_window_renders_a_collapsed_rail() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = &nodes[root].children;
        // Row 根：首子 = rail（宽 220），末子 = content（填余下 780）
        let rail = &nodes[children[0]];
        let content = &nodes[children[children.len() - 1]];
        assert_eq!(rail.measured_size.width, 96.0, "收起轨宽 96");
        assert!((content.measured_size.width - 904.0).abs() < 0.01,
            "内容填余下 width={}", content.measured_size.width);
    }

    /// 中宽窗 → 收起轨（96）
    #[test]
    fn medium_window_renders_collapsed_rail() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(700.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 700.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let rail = &nodes[nodes[root].children[0]];
        assert_eq!(rail.measured_size.width, 96.0, "收起轨宽 96");
    }

    /// 尺寸 State 驱动形态切换（resize 响应式通路——app 层 set() → 依赖方重组）
    #[test]
    fn window_size_state_drives_suite_type() {
        crate::layout::adaptive::reset_window_size_state();
        let size = crate::runtime::state::State::new((500.0f32, 700.0f32));
        crate::layout::adaptive::set_window_size_state(size.clone());
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
        size.set((1000.0, 700.0));
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        size.set((700.0, 700.0));
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        crate::layout::adaptive::reset_window_size_state();
    }

    /// 显式 layout_type 覆盖自动推算
    #[test]
    fn explicit_layout_type_overrides_auto() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0); // 自动会选展开轨
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold().layout_type(NavigationSuiteType::None).build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // None：根 = content 容器（仅 1 个内容子节点——无 rail/bar 子树）
        assert_eq!(nodes[root].children.len(), 1, "None 形态仅 content，无导航子树");
    }

    /// morph policy：容器尺寸 = 子尺寸 × progress（宽度轴），子全尺寸原点放置。
    /// progress 须在组合内创建（owner queue——set 通知才能路由到 layout 重测）
    #[test]
    fn morph_policy_scales_container_by_progress() {
        let holder: std::cell::RefCell<Option<State<f32>>> = std::cell::RefCell::new(None);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let p = ctx.key("morph_progress", |ctx| {
                    let existing = holder.borrow().clone();
                    existing.unwrap_or_else(|| {
                        let s = ctx.remember(|| 0.5f32);
                        *holder.borrow_mut() = Some(s.clone());
                        s
                    })
                });
                let k = ctx.next_key();
                let policy = SuiteMorphPolicy { progress: p, axis: MorphAxis::Width };
                match ctx.start_restartable_group(
                    k,
                    M::new().clip(Shape::Rectangle),
                    policy,
                ) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        icon(ctx, 24.0);
                    }
                }
                ctx.end_restartable_group();
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
        };
        build(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // 子项 = 24x24 图标：容器宽 = 24 × progress
        assert!((nodes[root].measured_size.width - 12.0).abs() < 0.01,
            "progress 0.5 → 容器宽 = 24x0.5，实际 {}", nodes[root].measured_size.width);
        assert!((nodes[root].measured_size.height - 24.0).abs() < 0.01,
            "高度轴不缩放（宽度轴 morph）");

        // set 通知 → pending → layout 重测（layout_deps 通路）
        holder.borrow().as_ref().unwrap().set(0.25);
        build(&mut composer);
        let nodes = composer.arena_nodes();
        assert!((nodes[root].measured_size.width - 6.0).abs() < 0.01,
            "progress 0.25 → 容器宽 6，实际 {}", nodes[root].measured_size.width);
    }

    /// 异形切换（rail→bar）：收拢（rail 宽度收缩）→ 换形 → 展开（bar 高度生长）
    #[test]
    fn suite_type_switch_collapses_then_expands() {
        crate::layout::adaptive::reset_window_size_state();
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, lt: NavigationSuiteType| {
            composer.compose(|ctx| {
                NavigationSuiteScaffold::new(
                    |items| {
                        items.item(
                            true,
                            |ctx| icon(ctx, 24.0),
                            |ctx| crate::components::Text::new("A").build(ctx),
                            || {},
                        );
                    },
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new());
                        ctx.end_node();
                    },
                )
                .layout_type(lt)
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 900.0, 0.0, 700.0));
        };

        build(&mut composer, NavigationSuiteType::WideNavigationRailExpanded);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[nodes[root].children[0]].measured_size.width, 220.0, "初始展开轨");

        // 切换 → Collapsing：rail 宽度开始收缩（< 220）
        build(&mut composer, NavigationSuiteType::ShortNavigationBarCompact);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut saw_shrinking = false;
        let mut saw_bar = false;
        loop {
            crate::animation::update_animations();
            build(&mut composer, NavigationSuiteType::ShortNavigationBarCompact);
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let column = &nodes[*nodes[root].children.last().unwrap()];
            let bar = &nodes[*column.children.last().unwrap()];
            if bar.measured_size.height == 80.0 && nodes[root].children.len() >= 1 {
                // bar 完整出现（高 80 贴底）→ 过渡完成
                saw_bar = true;
                break;
            }
            // 收拢期：rail 分支仍在，宽度应小于全宽
            if nodes[root].measured_size.width > 0.0 {
                let first = &nodes[nodes[root].children[0]];
                if first.measured_size.width < 220.0 && first.measured_size.width > 0.0 {
                    saw_shrinking = true;
                }
            }
            if std::time::Instant::now() > deadline {
                panic!("过渡 1.5s 未完成");
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        assert!(saw_shrinking, "应观察到 rail 收缩中间帧（非跳变）");
        assert!(saw_bar, "过渡完成应渲染底栏");
    }
}
