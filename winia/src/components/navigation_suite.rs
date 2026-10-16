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
use crate::layout::{Constraints, MeasurePolicy};
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
        let theme = WiniaTheme::colors();
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
                let mut rail = NavigationRail::new(move |ctx| {
                    for item in items_for_shape.clone() {
                        let icon = item.icon.clone();
                        let mut b = NavigationRailItem::new(item.selected, move |ctx| icon(ctx))
                            .colors(rail_item_colors)
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
                .container_color(suite_colors.navigation_rail_container);
                if let Some(primary_action) = primary_action {
                    rail = rail.header(primary_action);
                }
                Row::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        rail.build(ctx);
                        WiniaTheme::with_content_color(content_color, ctx, |ctx| content(ctx));
                    });
            }
            NavigationSuiteType::NavigationDrawer => {
                let sheet = DrawerSheet::permanent(move |ctx| {
                    if let Some(primary_action) = primary_action {
                        primary_action(ctx);
                    }
                    for item in items_for_shape.clone() {
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
                .container_color(suite_colors.navigation_drawer_container);
                Row::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        sheet.build(ctx);
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
                                ShortNavigationBar::new(move |ctx| {
                                    for item in items_for_shape.clone() {
                                        render_bar_item(ctx, item, icon_position, bar_item_colors);
                                    }
                                })
                                .arrangement(arrangement)
                                .container_color(bar_container)
                                .build(ctx);
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
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .padding_sides(
                                    0.0,
                                    0.0,
                                    PRIMARY_ACTION_PADDING,
                                    NAVIGATION_BAR_HEIGHT + PRIMARY_ACTION_PADDING,
                                )
                                // The action RIDES the navigation's animation — upstream places it at
                                // `layoutHeight - h - 16 - navigationHeight * animationProgress`
                                // (`:461-468`), so as the bar slides away it comes down to 16dp from the
                                // bottom edge and goes back up with the bar.
                                .absolute_offset_y(SizeValue::Dynamic(Arc::new(move || {
                                    NAVIGATION_BAR_HEIGHT * (1.0 - anim.get().clamp(0.0, 1.0))
                                }))),
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
                                let mut rail = WideNavigationRail::new(rail_state, move |ctx| {
                                    for item in items {
                                        let icon = item.icon.clone();
                                        let label = item
                                            .label
                                            .clone()
                                            .unwrap_or_else(|| Arc::new(|ctx: &mut ComposeCtx| {
                                                crate::components::Text::new("").build(ctx);
                                            }));
                                        let b = WideNavigationRailItem::new(
                                            item.selected,
                                            move |ctx| icon(ctx),
                                            move |ctx| label(ctx),
                                        )
                                        .colors(rail_item_colors)
                                        .progress(rail_item_progress.clone())
                                        .on_click(move || {
                                            if let Some(cb) = item.on_click.as_ref() {
                                                cb()
                                            }
                                        });
                                        b.build(ctx);
                                    }
                                })
                                .container_color(rail_container);
                                // Upstream's contract for a VERTICAL shape: the primary action goes in
                                // as the component's header (`NavigationSuite.kt:183-187`, `:612-620`) —
                                // the same slot the rail demos put their FAB in. Leaving it out is why
                                // a wide window showed no FAB at all.
                                if let Some(primary_action) = primary_action {
                                    rail = rail.header(primary_action);
                                }
                                rail.build(ctx);
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
    #[test]
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
