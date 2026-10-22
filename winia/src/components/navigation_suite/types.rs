use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::layout::Arrangement;
use crate::layout::adaptive::{window_height_size_class, window_width_size_class, HeightSizeClass, WidthSizeClass};
use crate::graphics::Color;
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
pub struct NavigationSuiteItems(pub(super) Vec<NavigationSuiteItem>);

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
    /// The scaffold's layout progress, mounted by its `build` — the source `is_animating` reads,
    /// exactly as `WideNavigationRailState` carries its morph progress.
    progress: Arc<parking_lot::Mutex<Option<State<f32>>>>,
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
        Self {
            value: State::new(initial),
            confirm: Arc::new(confirm),
            progress: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    pub fn current_value(&self) -> NavigationSuiteScaffoldValue {
        self.value.get()
    }

    pub fn is_visible(&self) -> bool {
        self.value.get() == NavigationSuiteScaffoldValue::Visible
    }

    /// `targetValue`. Upstream's state owns an `Animatable`, so a change leaves `currentValue`
    /// trailing `targetValue` while the animation runs. Here the animation lives in the SCAFFOLD's
    /// layout progress (`NavigationSuiteScaffoldLayout`'s `animationProgress`), not in this value, so
    /// the two are equal by construction — this accessor exists because upstream's interface has it,
    /// and it is honest about answering the same thing rather than pretending to a second state.
    pub fn target_value(&self) -> NavigationSuiteScaffoldValue {
        self.value.get()
    }

    /// `isAnimating`: whether the show/hide animation is running. The scaffold mounts its layout
    /// progress here from its `build`, so this reports the real thing rather than a constant. The
    /// read is `peek`ed: asking must not subscribe the caller to every animation frame.
    pub fn is_animating(&self) -> bool {
        match &*self.progress.lock() {
            Some(p) => {
                let v = p.peek();
                v > 0.01 && v < 0.99
            }
            None => false,
        }
    }

    /// Mount the scaffold's layout progress. `pub(crate)`: the progress belongs to the component,
    /// exactly as `WideNavigationRailState::attach_progress` does.
    pub(crate) fn attach_progress(&self, p: State<f32>) {
        *self.progress.lock() = Some(p);
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

