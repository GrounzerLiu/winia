//! Material 3 TopAppBar with a single moving title slot.

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::{Color, GraphicsLayerParams, Modifier, ScrollState, Shape};
use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource, ScrollDelta, ScrollVelocity};
use crate::ui::text::{ProvideTextStyle, TextOverflow, TextStyle};
use crate::ui::theme::WiniaTheme;

pub const TOP_APP_BAR_HEIGHT: f32 = 64.0;
pub const TOP_APP_BAR_MEDIUM_HEIGHT: f32 = 112.0;
pub const TOP_APP_BAR_LARGE_HEIGHT: f32 = 152.0;
pub const TOP_APP_BAR_HORIZONTAL_PADDING: f32 = 4.0;
pub const TOP_APP_BAR_TITLE_INSET: f32 = 12.0;
pub const TOP_APP_BAR_ICON_SLOT_SIZE: f32 = 48.0;
pub const TOP_APP_BAR_MEDIUM_TITLE_BOTTOM_INSET: f32 = 24.0;
pub const TOP_APP_BAR_LARGE_TITLE_BOTTOM_INSET: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopAppBarVariant { Standard, CenterAligned, Medium, Large }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopAppBarColors {
    pub container: Color, pub scrolled_container: Color, pub title: Color,
    pub subtitle: Color, pub navigation: Color, pub actions: Color,
}
impl TopAppBarColors {
    pub fn new(container: Color, title: Color, subtitle: Color, navigation: Color, actions: Color) -> Self { Self { container, scrolled_container: container, title, subtitle, navigation, actions } }
    pub fn scrolled_container(mut self, color: Color) -> Self { self.scrolled_container = color; self }
    pub fn from_theme(theme: &crate::ui::theme::ThemeColors) -> Self { Self::new(theme.surface, theme.on_surface, theme.on_surface_variant, theme.on_surface, theme.on_surface_variant).scrolled_container(theme.surface_container) }
    pub fn container_color(&self, variant: TopAppBarVariant, scroll_offset: f32, collapse_fraction: f32) -> Color {
        match variant {
            TopAppBarVariant::Standard | TopAppBarVariant::CenterAligned => if scroll_offset > 0.0 { self.scrolled_container } else { self.container },
            TopAppBarVariant::Medium | TopAppBarVariant::Large => color_lerp(self.container, self.scrolled_container, fast_out_linear_in(collapse_fraction)),
        }
    }
}

#[derive(Clone)]
pub struct TopAppBarState {
    pub height_offset_limit: crate::State<f32>,
    pub height_offset: crate::State<f32>,
    pub content_offset: crate::State<f32>,
}
impl TopAppBarState {
    pub fn new(expanded_height: f32) -> Self {
        Self { height_offset_limit: crate::State::new(-(expanded_height - TOP_APP_BAR_HEIGHT).max(0.0)), height_offset: crate::State::new(0.0), content_offset: crate::State::new(0.0) }
    }
    pub fn collapsed_fraction(&self) -> f32 { let limit = self.height_offset_limit.get(); if limit >= 0.0 { 0.0 } else { (self.height_offset.get() / limit).clamp(0.0, 1.0) } }
    pub fn current_height(&self, expanded_height: f32) -> f32 { (expanded_height + self.height_offset.get()).max(TOP_APP_BAR_HEIGHT) }
    pub fn is_collapsed(&self) -> bool { self.collapsed_fraction() >= 1.0 }
    pub fn overlapped_fraction(&self) -> f32 { let limit = self.height_offset_limit.get(); if limit >= 0.0 { 0.0 } else { (1.0 - ((limit + self.content_offset.get().abs()).clamp(limit, 0.0) / limit)).clamp(0.0, 1.0) } }
    pub fn is_overlapped(&self) -> bool { self.overlapped_fraction() > 0.01 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopAppBarScrollMode { Pinned, EnterAlways, ExitUntilCollapsed }

#[derive(Clone)]
pub struct TopAppBarNestedConnection { state: TopAppBarState, mode: TopAppBarScrollMode }
impl TopAppBarNestedConnection {
    pub fn new(state: TopAppBarState, mode: TopAppBarScrollMode) -> Self { Self { state, mode } }
    pub fn state(&self) -> TopAppBarState { self.state.clone() }
}
impl NestedScrollConnection for TopAppBarNestedConnection {
    fn on_pre_scroll(&self, available: ScrollDelta, source: NestedScrollSource) -> ScrollDelta {
        if matches!(self.mode, TopAppBarScrollMode::Pinned) || !matches!(source, NestedScrollSource::Wheel | NestedScrollSource::Drag) { return ScrollDelta::ZERO; }
        let limit = self.state.height_offset_limit.get();
        if limit >= 0.0 || available.y == 0.0 { return ScrollDelta::ZERO; }
        let current = self.state.height_offset.get();
        let target = (current + available.y).clamp(limit, 0.0);
        let consumed = target - current;
        self.state.height_offset.set(target);
        self.state.content_offset.update(|value| *value += available.y);
        ScrollDelta::new(0.0, consumed)
    }
    fn on_post_scroll(&self, consumed: ScrollDelta, available: ScrollDelta, source: NestedScrollSource) -> ScrollDelta {
        if matches!(self.mode, TopAppBarScrollMode::Pinned) || !matches!(source, NestedScrollSource::Wheel | NestedScrollSource::Drag) { return ScrollDelta::ZERO; }
        if self.mode == TopAppBarScrollMode::ExitUntilCollapsed && available.y > 0.0 && consumed.y == 0.0 { return self.on_pre_scroll(available, source); }
        ScrollDelta::ZERO
    }
    fn on_pre_fling(&self, available: ScrollVelocity) -> ScrollVelocity { let delta = ScrollDelta::new(0.0, available.y / 60.0); let consumed = self.on_pre_scroll(delta, NestedScrollSource::Fling); ScrollVelocity { x: 0.0, y: consumed.y * 60.0 } }
}


#[derive(Clone)]
enum TopAppBarBehaviorKind { Legacy(ScrollState), Nested(TopAppBarState, TopAppBarScrollMode) }

#[derive(Clone)]
pub struct TopAppBarScrollBehavior { kind: TopAppBarBehaviorKind, expanded_height: f32, collapsed_height: f32 }

impl TopAppBarScrollBehavior {
    pub fn new(scroll: ScrollState, expanded_height: f32) -> Self {
        assert!(expanded_height >= TOP_APP_BAR_HEIGHT, "TopAppBar expanded height ({expanded_height}) must be at least {TOP_APP_BAR_HEIGHT}");
        Self { kind: TopAppBarBehaviorKind::Legacy(scroll), expanded_height, collapsed_height: TOP_APP_BAR_HEIGHT }
    }
    pub fn pinned(state: TopAppBarState, expanded_height: f32) -> Self { Self { kind: TopAppBarBehaviorKind::Nested(state, TopAppBarScrollMode::Pinned), expanded_height, collapsed_height: TOP_APP_BAR_HEIGHT } }
    pub fn enter_always(state: TopAppBarState, expanded_height: f32) -> Self { Self { kind: TopAppBarBehaviorKind::Nested(state, TopAppBarScrollMode::EnterAlways), expanded_height, collapsed_height: TOP_APP_BAR_HEIGHT } }
    pub fn exit_until_collapsed(state: TopAppBarState, expanded_height: f32) -> Self { Self { kind: TopAppBarBehaviorKind::Nested(state, TopAppBarScrollMode::ExitUntilCollapsed), expanded_height, collapsed_height: TOP_APP_BAR_HEIGHT } }
    pub fn expanded_height(&self) -> f32 { self.expanded_height }
    pub fn collapsed_height(&self) -> f32 { self.collapsed_height }
    pub fn collapse_range(&self) -> f32 { self.expanded_height - self.collapsed_height }
    pub fn collapse_fraction(&self) -> f32 { match &self.kind { TopAppBarBehaviorKind::Legacy(scroll) => { let r = self.collapse_range(); if r <= 0.0 { 0.0 } else { (scroll.offset.get() / r).clamp(0.0, 1.0) } }, TopAppBarBehaviorKind::Nested(state, _) => state.collapsed_fraction() } }
    pub fn current_height(&self) -> f32 { match &self.kind { TopAppBarBehaviorKind::Legacy(_) => self.expanded_height - self.collapse_range() * self.collapse_fraction(), TopAppBarBehaviorKind::Nested(state, _) => state.current_height(self.expanded_height) } }
    pub fn is_collapsed(&self) -> bool { self.collapse_fraction() >= 1.0 }
    pub fn state(&self) -> Option<TopAppBarState> { match &self.kind { TopAppBarBehaviorKind::Nested(state, _) => Some(state.clone()), _ => None } }
    pub fn nested_scroll_connection(&self) -> Option<TopAppBarNestedConnection> { match &self.kind { TopAppBarBehaviorKind::Nested(state, mode) => Some(TopAppBarNestedConnection::new(state.clone(), *mode)), _ => None } }
    pub fn scroll_state(&self) -> Option<ScrollState> { match &self.kind { TopAppBarBehaviorKind::Legacy(scroll) => Some(scroll.clone()), _ => None } }
    fn scroll_offset(&self) -> f32 { match &self.kind { TopAppBarBehaviorKind::Legacy(scroll) => scroll.offset.get(), TopAppBarBehaviorKind::Nested(state, _) => state.content_offset.get() } }
}

pub struct TopAppBar {
    variant: TopAppBarVariant,
    title: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    subtitle: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    navigation_icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    actions: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    colors: Option<TopAppBarColors>, modifier: Modifier, scroll_behavior: Option<TopAppBarScrollBehavior>,
}
impl TopAppBar {
    pub fn new(title: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { Self::with_variant(TopAppBarVariant::Standard, title) }
    pub fn center_aligned(title: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { Self::with_variant(TopAppBarVariant::CenterAligned, title) }
    pub fn medium(title: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { Self::with_variant(TopAppBarVariant::Medium, title) }
    pub fn large(title: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { Self::with_variant(TopAppBarVariant::Large, title) }
    fn with_variant(variant: TopAppBarVariant, title: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { Self { variant, title: Box::new(title), subtitle: None, navigation_icon: None, actions: None, colors: None, modifier: Modifier::new(), scroll_behavior: None } }
    pub fn navigation_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.navigation_icon = Some(Box::new(f)); self }
    pub fn actions(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.actions = Some(Box::new(f)); self }
    pub fn subtitle(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.subtitle = Some(Box::new(f)); self }
    pub fn colors(mut self, colors: TopAppBarColors) -> Self { self.colors = Some(colors); self }
    pub fn modifier(mut self, modifier: Modifier) -> Self { self.modifier = self.modifier.then(modifier); self }
    pub fn scroll_behavior(mut self, behavior: TopAppBarScrollBehavior) -> Self { self.scroll_behavior = Some(behavior); self }
    pub fn variant(&self) -> TopAppBarVariant { self.variant }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.variant);
        if let Some(behavior) = &self.scroll_behavior {
            match &behavior.kind {
                TopAppBarBehaviorKind::Legacy(scroll) => { ctx.changed(&scroll.offset); }
                TopAppBarBehaviorKind::Nested(state, _) => { ctx.changed(&state.height_offset); ctx.changed(&state.content_offset); }
            }
        }
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| TopAppBarColors::from_theme(&theme));
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);
        let expanded = expanded_height(self.variant);
        let fraction = self.scroll_behavior.as_ref().map(|behavior| { debug_assert!((behavior.expanded_height() - expanded).abs() < f32::EPSILON); behavior.collapse_fraction() }).unwrap_or(0.0);
        let height = expanded - (expanded - TOP_APP_BAR_HEIGHT) * fraction;
        let navigation_present = self.navigation_icon.is_some();
        let actions_present = self.actions.is_some();
        let subtitle_present = self.subtitle.is_some();
        let bottom_inset = if self.variant == TopAppBarVariant::Large { TOP_APP_BAR_LARGE_TITLE_BOTTOM_INSET } else { TOP_APP_BAR_MEDIUM_TITLE_BOTTOM_INSET };
        let policy = TopAppBarLayoutPolicy { variant: self.variant, fraction, direction, navigation_present, actions_present, subtitle_present, expanded_height: expanded, bottom_inset };
        let title_style = title_style(if matches!(self.variant, TopAppBarVariant::Medium | TopAppBarVariant::Large) { expanded_title_style(self.variant) } else { WiniaTheme::typography().title_large }, colors.title);
        let scroll_offset = self.scroll_behavior.as_ref().map(|behavior| behavior.scroll_offset()).unwrap_or(0.0);
        let target_container = colors.container_color(self.variant, scroll_offset, fraction);
        let container_color = if matches!(self.variant, TopAppBarVariant::Standard | TopAppBarVariant::CenterAligned) {
            ctx.animate_color_as_state(
                target_container,
                crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                    std::time::Duration::from_millis(180),
                    crate::animation::interpolator::EaseOutCubic::new(),
                )),
            )
        } else {
            crate::core::state::State::new(target_container)
        };
        let rendered_container = container_color.clone();
        let root_modifier = Modifier::new().fill_max_width().height(height).background(move || rendered_container.peek(), Shape::Rectangle).clip(Shape::Rectangle).then(self.modifier);
        let title = self.title; let subtitle = self.subtitle; let navigation = self.navigation_icon; let actions = self.actions;
        match ctx.start_restartable_group(key, root_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                slot(ctx, Modifier::new().size(TOP_APP_BAR_ICON_SLOT_SIZE, TOP_APP_BAR_ICON_SLOT_SIZE).test_tag("top-app-bar-navigation"), |ctx| { if let Some(navigation) = navigation { WiniaTheme::with_content_color(colors.navigation, ctx, navigation); } });
                slot(ctx, Modifier::new().test_tag("top-app-bar-title"), |ctx| { ProvideTextStyle(title_style, ctx, title); });
                slot(ctx, alpha(fraction).test_tag("top-app-bar-subtitle"), |ctx| { if let Some(subtitle) = subtitle { let mut style = WiniaTheme::typography().body_medium; style.color = Some(colors.subtitle); ProvideTextStyle(style, ctx, subtitle); } });
                slot(ctx, Modifier::new().min_height(TOP_APP_BAR_ICON_SLOT_SIZE).test_tag("top-app-bar-actions"), |ctx| { if let Some(actions) = actions { WiniaTheme::with_content_color(colors.actions, ctx, actions); } });
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

fn fast_out_linear_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn color_lerp(from: Color, to: Color, fraction: f32) -> Color {
    let t = fraction.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color::from_argb(lerp(from.a, to.a), lerp(from.r, to.r), lerp(from.g, to.g), lerp(from.b, to.b))
}

fn slot(ctx: &mut ComposeCtx, modifier: Modifier, content: impl FnOnce(&mut ComposeCtx)) { let key = ctx.next_key(); match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(Alignment::Center)) { GroupStatus::Skip => {}, GroupStatus::Enter => content(ctx) }; ctx.end_restartable_group(); }
fn alpha(fraction: f32) -> Modifier { Modifier::new().graphics_layer(move || GraphicsLayerParams { alpha: 1.0 - fraction, clip: true, ..Default::default() }) }
fn expanded_height(v: TopAppBarVariant) -> f32 { match v { TopAppBarVariant::Standard | TopAppBarVariant::CenterAligned => TOP_APP_BAR_HEIGHT, TopAppBarVariant::Medium => TOP_APP_BAR_MEDIUM_HEIGHT, TopAppBarVariant::Large => TOP_APP_BAR_LARGE_HEIGHT } }
fn expanded_title_style(v: TopAppBarVariant) -> TextStyle { let t = WiniaTheme::typography(); match v { TopAppBarVariant::Medium => t.headline_small, TopAppBarVariant::Large => t.headline_medium, _ => t.title_large } }
fn title_style(mut s: TextStyle, color: Color) -> TextStyle { s.color = Some(color); s.max_lines = Some(1); s.overflow = Some(TextOverflow::Ellipsis); s }

#[derive(Debug, Clone)]
struct TopAppBarLayoutPolicy { variant: TopAppBarVariant, fraction: f32, direction: LayoutDirection, navigation_present: bool, actions_present: bool, subtitle_present: bool, expanded_height: f32, bottom_inset: f32 }
impl MeasurePolicy for TopAppBarLayoutPolicy {
    fn measure(&self, nodes: &mut Vec<LayoutNode>, policies: &[Box<dyn MeasurePolicy>], children: &[usize], c: Constraints) -> (Size, Vec<Placement>) {
        let width = c.max_width; let height = if c.max_height < f32::MAX { c.max_height } else { self.expanded_height - (self.expanded_height - TOP_APP_BAR_HEIGHT) * self.fraction };
        let (nav, _) = measure_node(nodes, policies, children[0], Constraints::fixed(TOP_APP_BAR_ICON_SLOT_SIZE, TOP_APP_BAR_ICON_SLOT_SIZE));
        let (actions, _) = measure_node(nodes, policies, children[3], Constraints::new(0.0, width, 0.0, TOP_APP_BAR_ICON_SLOT_SIZE));
        let nav_w = if self.navigation_present { nav.width } else { 0.0 }; let actions_w = if self.actions_present { actions.width } else { 0.0 };
        let start = TOP_APP_BAR_HORIZONTAL_PADDING + nav_w.max(TOP_APP_BAR_TITLE_INSET); let end = (width - TOP_APP_BAR_HORIZONTAL_PADDING - actions_w).max(start);
        let title_c = Constraints::new(0.0, (end - start).max(0.0), 0.0, self.expanded_height);
        let (title, _) = measure_node(nodes, policies, children[1], title_c);
        let subtitle_c = Constraints::new(0.0, (width - 2.0 * (TOP_APP_BAR_HORIZONTAL_PADDING + TOP_APP_BAR_TITLE_INSET)).max(0.0), 0.0, self.expanded_height);
        let (subtitle, _) = measure_node(nodes, policies, children[2], subtitle_c);
        let gap = if self.subtitle_present { 4.0 } else { 0.0 };
        let collapsed_x_ltr = if self.variant == TopAppBarVariant::CenterAligned { ((width - title.width) / 2.0).clamp(start, (end - title.width).max(start)) } else { start };
        let expanded_x_ltr = TOP_APP_BAR_HORIZONTAL_PADDING + TOP_APP_BAR_TITLE_INSET;
        let collapsed_y = (TOP_APP_BAR_HEIGHT - title.height).max(0.0) / 2.0;
        let expanded_y = (self.expanded_height - self.bottom_inset - title.height - if self.subtitle_present { gap + subtitle.height } else { 0.0 }).max(TOP_APP_BAR_HEIGHT);
        let x_ltr = if matches!(self.variant, TopAppBarVariant::Medium | TopAppBarVariant::Large) { expanded_x_ltr + (collapsed_x_ltr - expanded_x_ltr) * self.fraction } else { collapsed_x_ltr };
        let title_x = if self.direction == LayoutDirection::Ltr { x_ltr } else { width - x_ltr - title.width };
        let title_y = if matches!(self.variant, TopAppBarVariant::Medium | TopAppBarVariant::Large) { expanded_y + (collapsed_y - expanded_y) * self.fraction } else { collapsed_y };
        let subtitle_x = if self.direction == LayoutDirection::Ltr { expanded_x_ltr } else { width - expanded_x_ltr - subtitle.width };
        let subtitle_y = expanded_y + title.height + gap;
        let nav_x = if self.direction == LayoutDirection::Ltr { TOP_APP_BAR_HORIZONTAL_PADDING } else { width - TOP_APP_BAR_HORIZONTAL_PADDING - nav.width };
        let action_x = if self.direction == LayoutDirection::Ltr { width - TOP_APP_BAR_HORIZONTAL_PADDING - actions.width } else { TOP_APP_BAR_HORIZONTAL_PADDING };
        let slot_y = (TOP_APP_BAR_HEIGHT - TOP_APP_BAR_ICON_SLOT_SIZE) / 2.0;
        (Size::new(width, height), vec![Placement { size: nav, position: Point::new(nav_x, slot_y) }, Placement { size: title, position: Point::new(title_x, title_y) }, Placement { size: subtitle, position: Point::new(subtitle_x, subtitle_y) }, Placement { size: actions, position: Point::new(action_x, slot_y) }])
    }
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) { for (i, &child) in children.iter().enumerate() { nodes[child].position = placements[i].position; nodes[child].measured_size = placements[i].size; } }
}

#[cfg(test)]
mod tests {
    use super::*; use crate::core::composer::Composer;
    fn layout(bar: TopAppBar, w: f32) -> Composer { let mut c = Composer::new(); c.compose(|ctx| bar.build(ctx)); c.layout(Constraints::new(0.0, w, 0.0, 200.0)); c }
    fn leaf(ctx: &mut ComposeCtx, w: f32, h: f32) { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new().size(w, h)); ctx.end_node(); }
    #[test] fn variants_have_expected_heights() { for (bar,h) in [(TopAppBar::new(|_| {}),64.),(TopAppBar::center_aligned(|_| {}),64.),(TopAppBar::medium(|_| {}),112.),(TopAppBar::large(|_| {}),152.)] { let c=layout(bar,400.); assert_eq!(c.arena_nodes()[c.layout_root_idx().unwrap()].measured_size.height,h); } }
    #[test] fn scroll_behavior_clamps_fraction() { let s=ScrollState::new(); assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| TopAppBarScrollBehavior::new(s.clone(),63.))).is_err()); let b=TopAppBarScrollBehavior::new(s.clone(),152.); s.offset.set(44.); assert!((b.collapse_fraction() - 0.5).abs()< 0.001); assert_eq!(b.current_height(),108.); }
    #[test] fn single_title_moves_to_collapsed_row_without_subtitle_offset() { let s=ScrollState::new(); s.offset.set(1000.); let with_sub=layout(TopAppBar::large(|ctx| leaf(ctx,100.,20.)).subtitle(|ctx| leaf(ctx,80.,16.)).navigation_icon(|ctx| leaf(ctx,24.,24.)).actions(|ctx| leaf(ctx,72.,24.)).scroll_behavior(TopAppBarScrollBehavior::new(s.clone(),152.)),400.); let without_sub=layout(TopAppBar::large(|ctx| leaf(ctx,100.,20.)).navigation_icon(|ctx| leaf(ctx,24.,24.)).actions(|ctx| leaf(ctx,72.,24.)).scroll_behavior(TopAppBarScrollBehavior::new(s,152.)),400.); let a=with_sub.layout_root_idx().unwrap(); let b=without_sub.layout_root_idx().unwrap(); let an=with_sub.arena_nodes(); let bn=without_sub.arena_nodes(); assert_eq!(an[a].measured_size.height,64.); assert_eq!(an[an[a].children[1]].position.y,22.); assert_eq!(an[an[a].children[1]].position.y,bn[bn[b].children[1]].position.y); assert_eq!(an[an[a].children[0]].position.y,8.); }
    #[test] fn title_y_moves_continuously_at_mid_fraction() { let s=ScrollState::new(); s.offset.set(44.); let c=layout(TopAppBar::large(|ctx| leaf(ctx,100.,20.)).subtitle(|ctx| leaf(ctx,80.,16.)).scroll_behavior(TopAppBarScrollBehavior::new(s,152.)),400.); let root=c.layout_root_idx().unwrap(); let n=c.arena_nodes(); let y=n[n[root].children[1]].position.y; assert!(y>22. && y<80.); assert_eq!(n[root].measured_size.height,108.); }
    #[test] fn center_and_long_title_keep_safe_bounds() { let c=layout(TopAppBar::center_aligned(|ctx| leaf(ctx,100.,20.)).navigation_icon(|ctx| leaf(ctx,24.,24.)).actions(|ctx| leaf(ctx,72.,24.)),400.); let r=c.layout_root_idx().unwrap(); let n=c.arena_nodes(); let t=&n[n[r].children[1]]; assert!(((t.position.x+t.measured_size.width/2.)-200.).abs()< 0.01); let c=layout(TopAppBar::large(|ctx| crate::ui::Text::new("A deliberately long title that must not cover actions").build(ctx)).navigation_icon(|ctx| leaf(ctx,24.,24.)).actions(|ctx| leaf(ctx,72.,24.)),240.); let r=c.layout_root_idx().unwrap(); assert_eq!(c.arena_nodes()[r].measured_size.height,152.); }
    #[test] fn colors_use_material_surface_tokens_and_compat_constructor() { let theme=crate::ui::theme::ThemeColors::default_light(); let colors=TopAppBarColors::from_theme(&theme); assert_eq!(colors.container,theme.surface); assert_eq!(colors.scrolled_container,theme.surface_container); assert_eq!(colors.title,theme.on_surface); assert_eq!(colors.navigation,theme.on_surface); let custom=TopAppBarColors::new(Color::RED,Color::WHITE,Color::WHITE,Color::WHITE,Color::WHITE); assert_eq!(custom.scrolled_container,Color::RED); assert_eq!(custom.scrolled_container(Color::BLUE).scrolled_container,Color::BLUE); }
    #[test] fn standard_scroll_color_is_independent_from_collapse_fraction() { let scroll=ScrollState::new(); let colors=TopAppBarColors::new(Color::RED,Color::WHITE,Color::WHITE,Color::WHITE,Color::WHITE).scrolled_container(Color::BLUE); assert_eq!(colors.container_color(TopAppBarVariant::Standard,0.,0.),Color::RED); assert_eq!(colors.container_color(TopAppBarVariant::Standard,1.,0.),Color::BLUE); let behavior=TopAppBarScrollBehavior::new(scroll,TOP_APP_BAR_HEIGHT); assert_eq!(behavior.collapse_fraction(),0.); }
    #[test] fn collapsible_colors_interpolate_from_base_to_scrolled() { let colors=TopAppBarColors::new(Color::from_argb(255,0,0,0),Color::WHITE,Color::WHITE,Color::WHITE,Color::WHITE).scrolled_container(Color::from_argb(255,200,100,0)); assert_eq!(colors.container_color(TopAppBarVariant::Large,0.,0.),colors.container); let middle=colors.container_color(TopAppBarVariant::Large,44.,0.5); assert!(middle.r>0 && middle.r<200); assert_eq!(colors.container_color(TopAppBarVariant::Large,88.,1.),colors.scrolled_container); }
    #[test] fn nested_behavior_consumes_and_clamps_height_offset() { let state=TopAppBarState::new(TOP_APP_BAR_LARGE_HEIGHT); let connection=TopAppBarScrollBehavior::enter_always(state.clone(), TOP_APP_BAR_LARGE_HEIGHT).nested_scroll_connection().unwrap(); let consumed=connection.on_pre_scroll(ScrollDelta::new(0.0, -60.0), NestedScrollSource::Drag); assert_eq!(consumed.y, -60.0); assert_eq!(state.height_offset.get(), -60.0); let consumed=connection.on_pre_scroll(ScrollDelta::new(0.0, 100.0), NestedScrollSource::Drag); assert_eq!(consumed.y, 60.0); assert_eq!(state.height_offset.get(), 0.0); }
    #[test] fn top_app_bar_state_reports_overlap_separately() { let state=TopAppBarState::new(TOP_APP_BAR_LARGE_HEIGHT); state.content_offset.set(20.0); assert!(state.overlapped_fraction() > 0.0); assert!(!state.is_collapsed()); }
}
