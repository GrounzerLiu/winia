use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::layout::{Arrangement, Constraints, LayoutDirection, MeasurePolicy};
use crate::layout::node::{measure_node, Placement};
use crate::unit::{Offset, Size};
use crate::modifier::Modifier;
use crate::components::navigation_bar::NavigationItemIconPosition;
use crate::theme::WiniaTheme;
use crate::layout::Alignment;
use crate::graphics::Color;
use crate::components::short_navigation_bar::ShortNavigationBarArrangement;
use crate::components::navigation_rail::WideNavigationRailState;

use super::scaffold::{
    build_bar_shape, build_classic_rail_shape, build_drawer_shape, build_rail_shape,
    PRIMARY_ACTION_PADDING,
};
use super::types::{
    NavigationSuiteDefaults, NavigationSuiteItem, NavigationSuiteItems, NavigationSuiteType,
};

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
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or_else(crate::layout::direction::current);
        let rtl = direction == LayoutDirection::Rtl;
        ctx.changed(&direction);
        ctx.changed(&self.content_color);
        let policy = SuiteScaffoldLayoutPolicy {
            layout_type: self.layout_type,
            alignment: self.primary_action_alignment,
            rtl,
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
    /// The placement mirrors: `Start` is the leading edge, which is the RIGHT one under RTL. Captured
    /// at compose time because placement runs after composition, where the direction local is gone.
    rtl: bool,
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
            // A floating child has no horizontal stretch, so `Stretch` takes the trailing edge like
            // `End` — and "trailing" is the LOGICAL one: under RTL that is the LEFT edge.
            let action_x = match (self.alignment, self.rtl) {
                (Alignment::Center, _) => (c.max_width - action.width) / 2.0,
                (Alignment::Start, false)
                | (Alignment::End, true)
                | (Alignment::Stretch, true) => PRIMARY_ACTION_PADDING,
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
            let (nav_x, content_x) = if self.rtl {
                (c.max_width - nav.width, 0.0)
            } else {
                (0.0, nav.width)
            };
            vec![
                Placement { size: nav, position: Offset::new(nav_x, 0.0) },
                // A vertical shape keeps its action in its own header, so this slot is parked off-view
                // rather than placed (upstream's rail branch does not place it at all).
                Placement {
                    size: action,
                    position: Offset::new(c.max_width + 1000.0, 0.0),
                },
                Placement { size: content, position: Offset::new(content_x, 0.0) },
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
                arrangement,
            ),
            NavigationSuiteType::NavigationDrawer => build_drawer_shape(
                ctx,
                items,
                container.unwrap_or(colors.navigation_drawer_container),
                action,
                arrangement,
            ),
            NavigationSuiteType::None => {}
        }
            });
    }
}
