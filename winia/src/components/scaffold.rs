//! Compose-style page scaffold with top/bottom bars, content, snackbar host, system insets, and FAB overlay.

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::{Modifier};
use crate::window::insets::Insets;

pub const SCAFFOLD_FAB_MARGIN: f32 = 16.0;

/// Defaults for Scaffold, mirroring Compose's ScaffoldDefaults.
pub struct ScaffoldDefaults;

impl ScaffoldDefaults {
    /// The system-bar insets the enclosing window currently reports.
    ///
    /// Scaffold uses these as the default content-window insets; unlike a fixed zero value,
    /// this keeps content clear of the status and navigation bars on edge-to-edge windows.
    pub fn content_window_insets() -> Insets {
        crate::window::insets::current_insets().system_bars()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScaffoldContentPadding { pub start: f32, pub top: f32, pub end: f32, pub bottom: f32 }
impl ScaffoldContentPadding { pub const fn new(start: f32, top: f32, end: f32, bottom: f32) -> Self { Self { start, top, end, bottom } } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldFabPosition { End, Center }

pub struct Scaffold {
    content: Box<dyn FnOnce(&mut ComposeCtx, ScaffoldContentPadding) + Send + Sync>,
    top_bar: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    bottom_bar: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    snackbar_host: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    floating_action_button: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    content_padding: ScaffoldContentPadding,
    content_window_insets: Option<Insets>,
    fab_position: ScaffoldFabPosition,
    modifier: Modifier,
}

impl Scaffold {
    pub fn new(content: impl FnOnce(&mut ComposeCtx, ScaffoldContentPadding) + Send + Sync + 'static) -> Self {
        Self { content: Box::new(content), top_bar: None, bottom_bar: None, snackbar_host: None, floating_action_button: None, content_padding: ScaffoldContentPadding::default(), content_window_insets: None, fab_position: ScaffoldFabPosition::End, modifier: Modifier::new() }
    }
    pub fn top_bar(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.top_bar = Some(Box::new(content)); self }
    pub fn bottom_bar(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.bottom_bar = Some(Box::new(content)); self }
    pub fn snackbar_host(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.snackbar_host = Some(Box::new(content)); self }
    pub fn floating_action_button(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.floating_action_button = Some(Box::new(content)); self }
    pub fn content_padding(mut self, padding: ScaffoldContentPadding) -> Self { self.content_padding = padding; self }
    /// Set the system inset area the scaffold content must avoid.
    ///
    /// The default is ScaffoldDefaults::content_window_insets(), read from the enclosing
    /// window at build time. Pass Insets::ZERO to disable automatic system-bar padding.
    pub fn content_window_insets(mut self, insets: Insets) -> Self { self.content_window_insets = Some(insets); self }
    pub fn fab_position(mut self, position: ScaffoldFabPosition) -> Self { self.fab_position = position; self }
    pub fn modifier(mut self, modifier: Modifier) -> Self { self.modifier = self.modifier.then(modifier); self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        let window_insets = self
            .content_window_insets
            .unwrap_or_else(ScaffoldDefaults::content_window_insets);
        let policy = ScaffoldLayoutPolicy {
            direction,
            padding: self.content_padding,
            window_insets,
            fab_position: self.fab_position,
            top_present: self.top_bar.is_some(),
            bottom_present: self.bottom_bar.is_some(),
            snackbar_present: self.snackbar_host.is_some(),
            fab_present: self.floating_action_button.is_some(),
        };
        let top = self.top_bar;
        let bottom = self.bottom_bar;
        let snackbar = self.snackbar_host;
        let fab = self.floating_action_button;
        let content = self.content;
        let modifier = Modifier::new().fill_max_size().then(self.modifier);
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                slot(ctx, Modifier::new(), Alignment::Start, |ctx| { if let Some(top) = top { top(ctx); } });
                slot(ctx, Modifier::new(), Alignment::Start, |ctx| { if let Some(bottom) = bottom { bottom(ctx); } });
                slot(ctx, Modifier::new(), Alignment::Start, |ctx| content(ctx, self.content_padding));
                slot(ctx, Modifier::new(), Alignment::Start, |ctx| { if let Some(fab) = fab { fab(ctx); } });
                slot(ctx, Modifier::new(), Alignment::Center, |ctx| { if let Some(snackbar) = snackbar { snackbar(ctx); } });
            }
        }
        ctx.end_restartable_group();
    }
}

fn slot(ctx: &mut ComposeCtx, modifier: Modifier, alignment: Alignment, content: impl FnOnce(&mut ComposeCtx)) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(alignment)) { GroupStatus::Skip => {}, GroupStatus::Enter => content(ctx) }
    ctx.end_restartable_group();
}

#[derive(Debug, Clone)]
struct ScaffoldLayoutPolicy {
    direction: LayoutDirection,
    padding: ScaffoldContentPadding,
    window_insets: Insets,
    fab_position: ScaffoldFabPosition,
    top_present: bool,
    bottom_present: bool,
    snackbar_present: bool,
    fab_present: bool,
}
impl MeasurePolicy for ScaffoldLayoutPolicy {
    fn measure(&self, nodes: &mut Vec<LayoutNode>, policies: &[Box<dyn MeasurePolicy>], children: &[usize], constraints: Constraints) -> (Size, Vec<Placement>) {
        let width = constraints.max_width;
        let height = constraints.max_height;
        let left_inset = if self.direction == LayoutDirection::Ltr { self.window_insets.left } else { self.window_insets.right };
        let right_inset = if self.direction == LayoutDirection::Ltr { self.window_insets.right } else { self.window_insets.left };
        let inner_width = (width - left_inset - right_inset).max(0.0);
        let inner_height = (height - self.window_insets.top - self.window_insets.bottom).max(0.0);

        let (top, _) = measure_node(nodes, policies, children[0], Constraints::new(0.0, width, 0.0, inner_height));
        let (bottom, _) = measure_node(nodes, policies, children[1], Constraints::new(0.0, width, 0.0, inner_height));
        let content_constraints = Constraints::new(
            0.0,
            (inner_width - self.padding.start - self.padding.end).max(0.0),
            0.0,
            (inner_height - top.height - bottom.height - self.padding.top - self.padding.bottom).max(0.0),
        );
        let (content, _) = measure_node(nodes, policies, children[2], content_constraints);
        let (fab, _) = measure_node(nodes, policies, children[3], Constraints::new(0.0, inner_width, 0.0, inner_height));
        let (snackbar, _) = measure_node(nodes, policies, children[4], Constraints::new(0.0, inner_width, 0.0, inner_height));

        let content_x = left_inset + if self.direction == LayoutDirection::Ltr { self.padding.start } else { self.padding.end };
        let content_y = self.window_insets.top + top.height + self.padding.top;
        let fab_x = match self.fab_position {
            ScaffoldFabPosition::Center => left_inset + (inner_width - fab.width) / 2.0,
            ScaffoldFabPosition::End if self.direction == LayoutDirection::Ltr => {
                width - right_inset - self.padding.end - SCAFFOLD_FAB_MARGIN - fab.width
            }
            ScaffoldFabPosition::End => left_inset + self.padding.start + SCAFFOLD_FAB_MARGIN,
        };
        let fab_y = height
            - self.window_insets.bottom
            - bottom.height
            - self.padding.bottom
            - SCAFFOLD_FAB_MARGIN
            - fab.height;
        let snackbar_x = left_inset + (inner_width - snackbar.width) / 2.0;
        let snackbar_y = height
            - self.window_insets.bottom
            - bottom.height
            - self.padding.bottom
            - SCAFFOLD_FAB_MARGIN
            - snackbar.height;

        let zero = Size::ZERO;
        let top_size = if self.top_present { top } else { zero };
        let bottom_size = if self.bottom_present { bottom } else { zero };
        let snackbar_size = if self.snackbar_present { snackbar } else { zero };
        let fab_size = if self.fab_present { fab } else { zero };
        (
            Size::new(width, height),
            vec![
                Placement { size: top_size, position: Offset::new(0.0, self.window_insets.top) },
                Placement { size: bottom_size, position: Offset::new(0.0, height - self.window_insets.bottom - bottom.height) },
                Placement { size: content, position: Offset::new(content_x, content_y) },
                Placement { size: fab_size, position: Offset::new(fab_x, fab_y) },
                Placement { size: snackbar_size, position: Offset::new(snackbar_x, snackbar_y) },
            ],
        )
    }
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) { for (index, &child) in children.iter().enumerate() { nodes[child].position = placements[index].position; nodes[child].measured_size = placements[index].size; } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::WiniaTheme;
    use crate::runtime::composer::Composer;
    use crate::layout::constraints::Constraints;
    use crate::layout::Dimension;

    fn leaf(ctx: &mut ComposeCtx, modifier: Modifier) { let key = ctx.next_key(); ctx.start_leaf(key, modifier); ctx.end_node(); }
    fn layout_with_insets(scaffold: Scaffold, direction: LayoutDirection, insets: Insets) -> Composer {
        let mut c = Composer::new();
        c.compose(|ctx| WiniaTheme::with_theme_and_direction(crate::theme::ThemeColors::default_light(), direction, ctx, |ctx| {
            scaffold.content_window_insets(insets).build(ctx);
        }));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        c
    }
    fn layout(scaffold: Scaffold, direction: LayoutDirection) -> Composer {
        layout_with_insets(scaffold, direction, Insets::ZERO)
    }

    #[test]
    fn reserves_top_bottom_and_positions_fab_overlay() {
        let c=layout(Scaffold::new(|ctx,p| leaf(ctx, Modifier::new().fill_max_size().test_tag("content"))).top_bar(|ctx| leaf(ctx,Modifier::new().size(360.0,64.0).test_tag("top"))).bottom_bar(|ctx| leaf(ctx,Modifier::new().size(360.0,80.0).test_tag("bottom"))).floating_action_button(|ctx| leaf(ctx,Modifier::new().size(56.0,56.0).test_tag("fab"))).content_padding(ScaffoldContentPadding::new(0.0,0.0,0.0,0.0)),LayoutDirection::Ltr);
        let root=c.layout_root_idx().unwrap(); let n=c.arena_nodes(); let r=&n[root]; let content=&n[r.children[2]]; let fab=&n[r.children[3]]; assert_eq!(content.position,Offset::new(0.0,64.0)); assert_eq!(content.measured_size.height,496.0); assert_eq!(fab.position,Offset::new(288.0,488.0));
    }

    #[test]
    fn content_window_insets_shift_the_scaffold_slots() {
        let c = layout_with_insets(
            Scaffold::new(|ctx, _| leaf(ctx, Modifier::new().fill_max_size().test_tag("content")))
                .top_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 64.0).test_tag("top")))
                .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0).test_tag("bottom")))
                .floating_action_button(|ctx| leaf(ctx, Modifier::new().size(56.0, 56.0).test_tag("fab"))),
            LayoutDirection::Ltr,
            Insets::new(10.0, 20.0, 30.0, 40.0),
        );
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let root = &nodes[root];
        let top = &nodes[root.children[0]];
        let bottom = &nodes[root.children[1]];
        let content = &nodes[root.children[2]];
        let fab = &nodes[root.children[3]];
        assert_eq!(top.position, Offset::new(0.0, 20.0));
        assert_eq!(bottom.position, Offset::new(0.0, 520.0));
        assert_eq!(content.position, Offset::new(10.0, 84.0));
        assert_eq!(content.measured_size.width, 320.0);
        assert_eq!(content.measured_size.height, 436.0);
        assert_eq!(fab.position, Offset::new(258.0, 448.0));
    }

    #[test]
    fn snackbar_host_sits_above_the_bottom_bar() {
        let c = layout(
            Scaffold::new(|ctx, _| leaf(ctx, Modifier::new().fill_max_size()))
                .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0)))
                .snackbar_host(|ctx| leaf(ctx, Modifier::new().size(200.0, 48.0).test_tag("snackbar"))),
            LayoutDirection::Ltr,
        );
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let root = &nodes[root];
        let snackbar = &nodes[root.children[4]];
        assert_eq!(snackbar.position, Offset::new(80.0, 496.0));
        assert_eq!(snackbar.measured_size, Size::new(200.0, 48.0));
    }

    #[test]
    fn rtl_bottom_end_fab_mirrors_without_changing_vertical_geometry() {
        let make=|| Scaffold::new(|ctx,_| leaf(ctx,Modifier::new().fill_max_size())).bottom_bar(|ctx| leaf(ctx,Modifier::new().size(360.0,80.0))).floating_action_button(|ctx| leaf(ctx,Modifier::new().size(56.0,56.0)));
        let ltr=layout(make(),LayoutDirection::Ltr); let rtl=layout(make(),LayoutDirection::Rtl); let lr=ltr.layout_root_idx().unwrap(); let rr=rtl.layout_root_idx().unwrap(); let ln=ltr.arena_nodes(); let rn=rtl.arena_nodes(); assert_eq!(ln[ln[lr].children[3]].position.x,288.0); assert_eq!(rn[rn[rr].children[3]].position.x,16.0); assert_eq!(ln[ln[lr].children[3]].position.y,rn[rn[rr].children[3]].position.y);
    }
}
