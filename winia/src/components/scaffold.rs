//! Compose-style page scaffold with top/bottom bars, content, snackbar host, system insets, and FAB overlay.
//!
//! The content slot is measured full-size and receives a [ScaffoldContentPadding] containing the
//! resolved top/bottom bar heights (or system insets when a bar is absent); the content root applies
//! it, matching Compose's PaddingValues contract.

use crate::composable;
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement, measure_node};
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::unit::{Offset, Size};
use crate::window::insets::Insets;

pub const SCAFFOLD_FAB_MARGIN: f32 = 16.0;

// Compose's sibling paint order for the scaffold slots (`Scaffold.kt:373-377`). The content is
// measured FULL-SIZE at (0,0) and therefore covers the top bar; without these the bar was both
// painted under the content and, because the hit test walks paint order in reverse, unable to
// receive a click — the settings button in `examples/common/settings.rs` could not be pressed.
const SCAFFOLD_CONTENT_Z: f32 = 0.0;
const SCAFFOLD_TOP_BAR_Z: f32 = 1.0;
const SCAFFOLD_SNACKBAR_Z: f32 = 2.0;
const SCAFFOLD_BOTTOM_BAR_Z: f32 = 3.0;
const SCAFFOLD_FAB_Z: f32 = 4.0;

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
pub struct ScaffoldContentPadding {
    pub start: f32,
    pub top: f32,
    pub end: f32,
    pub bottom: f32,
}
impl ScaffoldContentPadding {
    pub const fn new(start: f32, top: f32, end: f32, bottom: f32) -> Self {
        Self {
            start,
            top,
            end,
            bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldFabPosition {
    End,
    Center,
}

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
    /// Build a scaffold whose content receives the resolved inner padding.
    ///
    /// Apply the padding to the content root, for example
    /// Modifier::new().padding_sides(padding.start, padding.top, padding.end, padding.bottom).
    pub fn new(
        content: impl FnOnce(&mut ComposeCtx, ScaffoldContentPadding) + Send + Sync + 'static,
    ) -> Self {
        Self {
            content: Box::new(content),
            top_bar: None,
            bottom_bar: None,
            snackbar_host: None,
            floating_action_button: None,
            content_padding: ScaffoldContentPadding::default(),
            content_window_insets: None,
            fab_position: ScaffoldFabPosition::End,
            modifier: Modifier::new(),
        }
    }
    pub fn top_bar(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.top_bar = Some(Box::new(content));
        self
    }
    pub fn bottom_bar(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.bottom_bar = Some(Box::new(content));
        self
    }
    pub fn snackbar_host(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.snackbar_host = Some(Box::new(content));
        self
    }
    pub fn floating_action_button(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.floating_action_button = Some(Box::new(content));
        self
    }
    pub fn content_padding(mut self, padding: ScaffoldContentPadding) -> Self {
        self.content_padding = padding;
        self
    }
    /// Set the system inset area the scaffold content must avoid.
    ///
    /// The default is ScaffoldDefaults::content_window_insets(), read from the enclosing
    /// window at build time. Pass Insets::ZERO to disable automatic system-bar padding.
    pub fn content_window_insets(mut self, insets: Insets) -> Self {
        self.content_window_insets = Some(insets);
        self
    }
    pub fn fab_position(mut self, position: ScaffoldFabPosition) -> Self {
        self.fab_position = position;
        self
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        let window_insets = self
            .content_window_insets
            .unwrap_or_else(ScaffoldDefaults::content_window_insets);
        let content_padding_state = ctx.remember(|| ScaffoldContentPadding::default());
        let policy = ScaffoldLayoutPolicy {
            direction,
            padding: self.content_padding,
            content_padding_state: content_padding_state.clone(),
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
        let content_padding = content_padding_state.clone();
        let modifier = Modifier::new().fill_max_size().then(self.modifier);
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                slot(
                    ctx,
                    Modifier::new().z_index(SCAFFOLD_TOP_BAR_Z),
                    Alignment::Start,
                    |ctx| {
                        if let Some(top) = top {
                            top(ctx);
                        }
                    },
                );
                slot(
                    ctx,
                    Modifier::new().z_index(SCAFFOLD_BOTTOM_BAR_Z),
                    Alignment::Start,
                    |ctx| {
                        if let Some(bottom) = bottom {
                            bottom(ctx);
                        }
                    },
                );
                slot(
                    ctx,
                    Modifier::new().z_index(SCAFFOLD_CONTENT_Z),
                    Alignment::Start,
                    |ctx| content(ctx, content_padding.get()),
                );
                slot(
                    ctx,
                    Modifier::new().z_index(SCAFFOLD_FAB_Z),
                    Alignment::Start,
                    |ctx| {
                        if let Some(fab) = fab {
                            fab(ctx);
                        }
                    },
                );
                slot(
                    ctx,
                    Modifier::new().z_index(SCAFFOLD_SNACKBAR_Z),
                    Alignment::Center,
                    |ctx| {
                        if let Some(snackbar) = snackbar {
                            snackbar(ctx);
                        }
                    },
                );
            }
        }
        ctx.end_restartable_group();
    }
}

fn slot(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    alignment: Alignment,
    content: impl FnOnce(&mut ComposeCtx),
) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(alignment)) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

impl std::fmt::Debug for ScaffoldLayoutPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScaffoldLayoutPolicy")
            .field("direction", &self.direction)
            .field("padding", &self.padding)
            .field("window_insets", &self.window_insets)
            .field("fab_position", &self.fab_position)
            .field("top_present", &self.top_present)
            .field("bottom_present", &self.bottom_present)
            .field("snackbar_present", &self.snackbar_present)
            .field("fab_present", &self.fab_present)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct ScaffoldLayoutPolicy {
    direction: LayoutDirection,
    padding: ScaffoldContentPadding,
    content_padding_state: State<ScaffoldContentPadding>,
    window_insets: Insets,
    fab_position: ScaffoldFabPosition,
    top_present: bool,
    bottom_present: bool,
    snackbar_present: bool,
    fab_present: bool,
}
impl MeasurePolicy for ScaffoldLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let width = constraints.max_width;
        let height = constraints.max_height;
        let (start_inset, end_inset) = if self.direction == LayoutDirection::Ltr {
            (self.window_insets.left, self.window_insets.right)
        } else {
            (self.window_insets.right, self.window_insets.left)
        };

        let (top, _) = measure_node(
            nodes,
            policies,
            children[0],
            Constraints::new(0.0, width, 0.0, height),
        );
        let (bottom, _) = measure_node(
            nodes,
            policies,
            children[1],
            Constraints::new(0.0, width, 0.0, height),
        );
        let (content, _) = measure_node(
            nodes,
            policies,
            children[2],
            Constraints::new(0.0, width, 0.0, height),
        );
        let (fab, _) = measure_node(
            nodes,
            policies,
            children[3],
            Constraints::new(0.0, width, 0.0, height),
        );
        let (snackbar, _) = measure_node(
            nodes,
            policies,
            children[4],
            Constraints::new(0.0, width, 0.0, height),
        );

        // Compose applies the content-window insets only where a bar is absent: a top/bottom bar
        // is expected to handle its own system inset. The content root is measured full-size and
        // applies this padding itself, exactly like Compose's PaddingValues contract.
        self.content_padding_state.set(ScaffoldContentPadding {
            start: self.padding.start + start_inset,
            top: self.padding.top
                + if self.top_present {
                    top.height
                } else {
                    self.window_insets.top
                },
            end: self.padding.end + end_inset,
            bottom: self.padding.bottom
                + if self.bottom_present {
                    bottom.height
                } else {
                    self.window_insets.bottom
                },
        });

        let bottom_anchor = self.padding.bottom
            + if self.bottom_present {
                bottom.height
            } else {
                self.window_insets.bottom
            };
        let fab_x = match self.fab_position {
            ScaffoldFabPosition::Center => {
                start_inset + (width - start_inset - end_inset - fab.width) / 2.0
            }
            ScaffoldFabPosition::End if self.direction == LayoutDirection::Ltr => {
                width - end_inset - self.padding.end - SCAFFOLD_FAB_MARGIN - fab.width
            }
            ScaffoldFabPosition::End => start_inset + self.padding.start + SCAFFOLD_FAB_MARGIN,
        };
        let fab_y = height - bottom_anchor - SCAFFOLD_FAB_MARGIN - fab.height;
        let snackbar_x = start_inset + (width - start_inset - end_inset - snackbar.width) / 2.0;
        let snackbar_fab_offset = if self.fab_present { fab.height } else { 0.0 };
        let snackbar_y =
            height - bottom_anchor - SCAFFOLD_FAB_MARGIN - snackbar_fab_offset - snackbar.height;

        let zero = Size::ZERO;
        let top_size = if self.top_present { top } else { zero };
        let bottom_size = if self.bottom_present { bottom } else { zero };
        let snackbar_size = if self.snackbar_present {
            snackbar
        } else {
            zero
        };
        let fab_size = if self.fab_present { fab } else { zero };
        (
            Size::new(width, height),
            vec![
                Placement {
                    size: top_size,
                    position: Offset::new(0.0, 0.0),
                },
                Placement {
                    size: bottom_size,
                    position: Offset::new(0.0, height - bottom.height),
                },
                Placement {
                    size: content,
                    position: Offset::new(0.0, 0.0),
                },
                Placement {
                    size: fab_size,
                    position: Offset::new(fab_x, fab_y),
                },
                Placement {
                    size: snackbar_size,
                    position: Offset::new(snackbar_x, snackbar_y),
                },
            ],
        )
    }
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Dimension;
    use crate::layout::constraints::Constraints;
    use crate::runtime::composer::Composer;
    use crate::theme::WiniaTheme;

    fn leaf(ctx: &mut ComposeCtx, modifier: Modifier) {
        let key = ctx.next_key();
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }
    fn layout_with_insets(
        scaffold: Scaffold,
        direction: LayoutDirection,
        insets: Insets,
    ) -> Composer {
        let mut c = Composer::new();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                crate::theme::ThemeColors::default_light(),
                direction,
                ctx,
                |ctx| {
                    scaffold.content_window_insets(insets).build(ctx);
                },
            )
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        c
    }
    fn layout(scaffold: Scaffold, direction: LayoutDirection) -> Composer {
        layout_with_insets(scaffold, direction, Insets::ZERO)
    }

    #[test]
    fn reserves_top_bottom_and_positions_fab_overlay() {
        let c = layout(
            Scaffold::new(|ctx, p| leaf(ctx, Modifier::new().fill_max_size().test_tag("content")))
                .top_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 64.0).test_tag("top")))
                .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0).test_tag("bottom")))
                .floating_action_button(|ctx| {
                    leaf(ctx, Modifier::new().size(56.0, 56.0).test_tag("fab"))
                })
                .content_padding(ScaffoldContentPadding::new(0.0, 0.0, 0.0, 0.0)),
            LayoutDirection::Ltr,
        );
        let root = c.layout_root_idx().unwrap();
        let n = c.arena_nodes();
        let r = &n[root];
        let content = &n[r.children[2]];
        let fab = &n[r.children[3]];
        assert_eq!(content.position, Offset::new(0.0, 0.0));
        assert_eq!(content.measured_size, Size::new(360.0, 640.0));
        assert_eq!(fab.position, Offset::new(288.0, 488.0));
    }

    #[test]
    fn content_window_insets_shift_the_scaffold_slots() {
        let c = layout_with_insets(
            Scaffold::new(|ctx, _| leaf(ctx, Modifier::new().fill_max_size().test_tag("content")))
                .top_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 64.0).test_tag("top")))
                .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0).test_tag("bottom")))
                .floating_action_button(|ctx| {
                    leaf(ctx, Modifier::new().size(56.0, 56.0).test_tag("fab"))
                }),
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
        assert_eq!(top.position, Offset::new(0.0, 0.0));
        assert_eq!(bottom.position, Offset::new(0.0, 560.0));
        assert_eq!(content.position, Offset::new(0.0, 0.0));
        assert_eq!(content.measured_size, Size::new(360.0, 640.0));
        assert_eq!(fab.position, Offset::new(258.0, 488.0));
    }

    #[test]
    fn content_receives_bar_heights_as_inner_padding() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_for_build = seen.clone();
        let mut c = Composer::new();
        let mut build = move |ctx: &mut ComposeCtx| {
            let seen = seen_for_build.clone();
            WiniaTheme::with_theme_and_direction(
                crate::theme::ThemeColors::default_light(),
                LayoutDirection::Ltr,
                ctx,
                |ctx| {
                    Scaffold::new(move |ctx, padding| {
                        seen.lock().unwrap().push(padding);
                        leaf(ctx, Modifier::new().fill_max_size());
                    })
                    .top_bar(|ctx| leaf(ctx, Modifier::new().fill_max_width().height(64.0)))
                    .bottom_bar(|ctx| leaf(ctx, Modifier::new().fill_max_width().height(80.0)))
                    .content_window_insets(Insets::new(10.0, 20.0, 30.0, 40.0))
                    .build(ctx);
                },
            );
        };
        c.compose(&mut build);
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        c.compose(&mut build);
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let seen = seen.lock().unwrap();
        assert!(
            seen.iter()
                .any(|p| *p == ScaffoldContentPadding::new(10.0, 64.0, 30.0, 80.0)),
            "content should receive bar heights, not system insets, when bars are present: {seen:?}"
        );
    }

    #[test]
    fn snackbar_host_sits_above_the_bottom_bar_and_fab() {
        let c = layout(
            Scaffold::new(|ctx, _| leaf(ctx, Modifier::new().fill_max_size()))
                .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0)))
                .snackbar_host(|ctx| {
                    leaf(ctx, Modifier::new().size(200.0, 48.0).test_tag("snackbar"))
                })
                .floating_action_button(|ctx| leaf(ctx, Modifier::new().size(56.0, 56.0))),
            LayoutDirection::Ltr,
        );
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let root = &nodes[root];
        let snackbar = &nodes[root.children[4]];
        // Compose: snackbarOffsetFromBottom = snackbar height + FAB offset from bottom.
        assert_eq!(snackbar.position, Offset::new(80.0, 440.0));
        assert_eq!(snackbar.measured_size, Size::new(200.0, 48.0));
    }

    #[test]
    fn rtl_bottom_end_fab_mirrors_without_changing_vertical_geometry() {
        let make = || {
            Scaffold::new(|ctx, _| leaf(ctx, Modifier::new().fill_max_size()))
                .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0)))
                .floating_action_button(|ctx| leaf(ctx, Modifier::new().size(56.0, 56.0)))
        };
        let ltr = layout(make(), LayoutDirection::Ltr);
        let rtl = layout(make(), LayoutDirection::Rtl);
        let lr = ltr.layout_root_idx().unwrap();
        let rr = rtl.layout_root_idx().unwrap();
        let ln = ltr.arena_nodes();
        let rn = rtl.arena_nodes();
        assert_eq!(ln[ln[lr].children[3]].position.x, 288.0);
        assert_eq!(rn[rn[rr].children[3]].position.x, 16.0);
        assert_eq!(
            ln[ln[lr].children[3]].position.y,
            rn[rn[rr].children[3]].position.y
        );
    }

    /// The content is measured full-size at (0,0), so it covers the top bar. Compose keeps the bar
    /// on top with an explicit `zIndex` (`Scaffold.kt:279-280`), and without the same ordering here
    /// the bar is painted under the content AND unreachable: the hit test walks paint order in
    /// reverse, so a press inside the top bar lands on the content instead. That is what made the
    /// settings button in `examples/common/settings.rs` unclickable.
    #[test]
    fn the_top_bar_paints_and_hits_above_the_full_size_content() {
        let mut c = layout_with_insets(
            Scaffold::new(|ctx, _| {
                leaf(
                    ctx,
                    Modifier::new().fill_max_size().test_tag("scaffold-content"),
                )
            })
            .top_bar(|ctx| {
                leaf(
                    ctx,
                    Modifier::new().size(360.0, 64.0).test_tag("app-top-bar"),
                )
            })
            .bottom_bar(|ctx| leaf(ctx, Modifier::new().size(360.0, 80.0))),
            LayoutDirection::Ltr,
            Insets::ZERO,
        );
        let root = c.layout_root_idx().unwrap();
        let n = c.arena_nodes();
        let children = n[root].children.clone();
        // The renderer and the hit test only order siblings when this flag is set, and the flag is
        // computed by the measure pass from the children's modifiers. Assert it here: a test that
        // hand-set the flag would pass even if the measure pass never looked at the z at all.
        assert!(
            n[root].children_have_z,
            "the measure pass must notice the slots carry a z"
        );
        // Find the top bar slot by the z it carries, not by tag: the tag rides the slot's child.
        let top_slot = children
            .iter()
            .copied()
            .find(|&i| n[i].modifier.get_z_index() == Some(SCAFFOLD_TOP_BAR_Z))
            .expect("the top bar slot carries its z");
        let order = crate::layout::node::paint_order(n, &children);
        let zs: Vec<_> = order.iter().map(|&i| n[i].modifier.get_z_index()).collect();
        // Compose's ladder for the slots that are present here: content 0, top bar 1, snackbar 2,
        // bottom bar 3, fab 4. Paint order is ascending z, so the bar is painted after — i.e. above
        // — the full-size content that covers it.
        assert_eq!(
            zs,
            vec![
                Some(SCAFFOLD_CONTENT_Z),
                Some(SCAFFOLD_TOP_BAR_Z),
                Some(SCAFFOLD_SNACKBAR_Z),
                Some(SCAFFOLD_BOTTOM_BAR_Z),
                Some(SCAFFOLD_FAB_Z)
            ],
            "slots must paint in Compose's z ladder"
        );
        let top_index = order.iter().position(|&i| i == top_slot).unwrap();
        let content_index = children
            .iter()
            .position(|&i| n[i].modifier.get_z_index() == Some(SCAFFOLD_CONTENT_Z))
            .and_then(|ci| order.iter().position(|&i| i == children[ci]))
            .expect("the content slot paints");
        assert!(
            top_index > content_index,
            "the top bar must paint after the content that covers it"
        );
        // The hit test walks paint order in reverse, so the same ordering is what lets a press in
        // the bar reach it rather than the content underneath.
        let inside_bar = crate::layout::node::hit_test(n, root, 100.0, 20.0);
        assert!(
            inside_bar.contains(&top_slot),
            "a press inside the top bar must reach it, got {inside_bar:?}"
        );
    }
}
