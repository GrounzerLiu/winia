//! Material 3 ListItem with one, two, and three-line slot layouts.

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::{Alignment, BoxLayout};
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::WiniaTheme;
use crate::ui::text::{ProvideTextStyle, TextStyle};
use std::sync::Arc;

pub const LIST_ITEM_ONE_LINE_HEIGHT: f32 = 56.0;
pub const LIST_ITEM_TWO_LINE_HEIGHT: f32 = 72.0;
pub const LIST_ITEM_THREE_LINE_HEIGHT: f32 = 88.0;
pub const LIST_ITEM_HORIZONTAL_PADDING: f32 = 16.0;
pub const LIST_ITEM_VERTICAL_PADDING: f32 = 10.0;
pub const LIST_ITEM_SLOT_GAP: f32 = 12.0;
pub const LIST_ITEM_CONTENT_GAP: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListItemColors {
    pub container: Color,
    pub content: Color,
    pub leading: Color,
    pub trailing: Color,
    pub overline: Color,
    pub supporting: Color,
    pub disabled_content: Color,
    pub disabled_leading: Color,
    pub disabled_trailing: Color,
    pub disabled_overline: Color,
    pub disabled_supporting: Color,
}

impl ListItemColors {
    pub fn new(
        container: Color,
        content: Color,
        leading: Color,
        trailing: Color,
        overline: Color,
        supporting: Color,
        disabled_content: Color,
        disabled_leading: Color,
        disabled_trailing: Color,
        disabled_overline: Color,
        disabled_supporting: Color,
    ) -> Self {
        Self { container, content, leading, trailing, overline, supporting, disabled_content, disabled_leading, disabled_trailing, disabled_overline, disabled_supporting }
    }

    pub fn from_theme(theme: &crate::ui::theme::ThemeColors) -> Self {
        let disabled = |color: Color| Color::from_argb((color.a as f32 * 0.38) as u8, color.r, color.g, color.b);
        Self::new(
            theme.surface,
            theme.on_surface,
            theme.on_surface_variant,
            theme.on_surface_variant,
            theme.on_surface_variant,
            theme.on_surface_variant,
            disabled(theme.on_surface),
            disabled(theme.on_surface_variant),
            disabled(theme.on_surface_variant),
            disabled(theme.on_surface_variant),
            disabled(theme.on_surface_variant),
        )
    }
}

pub struct ListItemDefaults;

impl ListItemDefaults {
    pub fn colors(theme: &crate::ui::theme::ThemeColors) -> ListItemColors {
        ListItemColors::from_theme(theme)
    }

    pub fn shape() -> Shape { Shape::Rectangle }
    pub fn one_line_height() -> f32 { LIST_ITEM_ONE_LINE_HEIGHT }
    pub fn two_line_height() -> f32 { LIST_ITEM_TWO_LINE_HEIGHT }
    pub fn three_line_height() -> f32 { LIST_ITEM_THREE_LINE_HEIGHT }
    pub fn headline_style() -> TextStyle { WiniaTheme::typography().body_large }
    pub fn supporting_style() -> TextStyle { WiniaTheme::typography().body_medium }
    pub fn overline_style() -> TextStyle { WiniaTheme::typography().label_small }
}

pub struct ListItem {
    headline: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    overline: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    supporting: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    leading: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    trailing: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<ListItemColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl ListItem {
    pub fn new(headline: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self { headline: Box::new(headline), overline: None, supporting: None, leading: None, trailing: None, on_click: None, enabled: true, colors: None, interaction_source: None, modifier: Modifier::new() }
    }

    pub fn overline_content(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.overline = Some(Box::new(content)); self }
    pub fn supporting_content(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.supporting = Some(Box::new(content)); self }
    pub fn leading_content(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading = Some(Box::new(content)); self }
    pub fn trailing_content(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.trailing = Some(Box::new(content)); self }
    pub fn on_click(mut self, callback: impl Fn() + Send + Sync + 'static) -> Self { self.on_click = Some(Arc::new(callback)); self }
    pub fn enabled(mut self, enabled: bool) -> Self { self.enabled = enabled; self }
    pub fn colors(mut self, colors: ListItemColors) -> Self { self.colors = Some(colors); self }
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self { self.interaction_source = Some(source); self }
    pub fn modifier(mut self, modifier: Modifier) -> Self { self.modifier = self.modifier.then(modifier); self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| ListItemDefaults::colors(&theme));
        let has_two_lines = self.overline.is_some() || self.supporting.is_some();
        let has_three_lines = self.overline.is_some() && self.supporting.is_some();
        let height = if has_three_lines { LIST_ITEM_THREE_LINE_HEIGHT } else if has_two_lines { LIST_ITEM_TWO_LINE_HEIGHT } else { LIST_ITEM_ONE_LINE_HEIGHT };
        let content_color = if self.enabled { colors.content } else { colors.disabled_content };
        let shape = ListItemDefaults::shape();
        let interaction = (self.enabled && self.on_click.is_some()).then(|| {
            self.interaction_source.unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get())
        });
        let mut modifier = Modifier::new()
            .fill_max_width()
            .height(height)
            .padding_sides(LIST_ITEM_HORIZONTAL_PADDING, LIST_ITEM_VERTICAL_PADDING, LIST_ITEM_HORIZONTAL_PADDING, LIST_ITEM_VERTICAL_PADDING)
            .background(colors.container, shape)
            .then(self.modifier);
        if self.enabled {
            if let Some(callback) = self.on_click {
                let interaction = interaction.expect("clickable ListItem requires an interaction source");
                modifier = modifier
                    .clickable_with_source(&interaction, move || callback())
                    .ripple_with_shape(&interaction, content_color, true, shape);
            }
        }

        match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(Alignment::Center)) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let leading = self.leading;
                let trailing = self.trailing;
                let overline = self.overline;
                let supporting = self.supporting;
                let headline = self.headline;
                crate::ui::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    crate::ui::Row::new().alignment(if has_three_lines { Alignment::Start } else { Alignment::Center }).spacing(LIST_ITEM_SLOT_GAP).build(ctx, |ctx| {
                        if let Some(leading) = leading {
                            crate::ui::theme::WiniaTheme::with_content_color(if self.enabled { colors.leading } else { colors.disabled_leading }, ctx, |ctx| {
                                leading(ctx);
                            });
                        }
                        crate::ui::Column::new().modifier(Modifier::new().layout_weight(1.0)).alignment(Alignment::Start).spacing(0.0).build(ctx, |ctx| {
                            if let Some(overline) = overline {
                                let mut text_style = ListItemDefaults::overline_style(); text_style.color = Some(if self.enabled { colors.overline } else { colors.disabled_overline });
                                ProvideTextStyle(text_style, ctx, overline);
                            }
                            let mut text_style = ListItemDefaults::headline_style(); text_style.color = Some(if self.enabled { colors.content } else { colors.disabled_content });
                            ProvideTextStyle(text_style, ctx, headline);
                            if let Some(supporting) = supporting {
                                let mut text_style = ListItemDefaults::supporting_style(); text_style.color = Some(if self.enabled { colors.supporting } else { colors.disabled_supporting });
                                ProvideTextStyle(text_style, ctx, supporting);
                            }
                        });
                        if let Some(trailing) = trailing {
                            crate::ui::theme::WiniaTheme::with_content_color(if self.enabled { colors.trailing } else { colors.disabled_trailing }, ctx, |ctx| {
                                trailing(ctx);
                            });
                        }
                    });
                });
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }

    pub fn get_enabled(&self) -> bool { self.enabled }
    pub fn get_colors(&self) -> Option<ListItemColors> { self.colors }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::constraints::Constraints;

    fn layout_item(item: ListItem, width: f32) -> Composer {
        let mut composer = Composer::new();
        composer.compose(|ctx| item.build(ctx));
        composer.layout(Constraints::new(0.0, width, 0.0, 200.0));
        composer
    }

    fn child<'a>(nodes: &'a [crate::layout::node::LayoutNode], parent: usize, index: usize) -> &'a crate::layout::node::LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn list_item_defaults_match_m3_tokens() {
        assert_eq!(LIST_ITEM_ONE_LINE_HEIGHT, 56.0);
        assert_eq!(LIST_ITEM_TWO_LINE_HEIGHT, 72.0);
        assert_eq!(LIST_ITEM_THREE_LINE_HEIGHT, 88.0);
        assert_eq!(LIST_ITEM_HORIZONTAL_PADDING, 16.0);
        assert_eq!(LIST_ITEM_SLOT_GAP, 12.0);
        assert_eq!(ListItemDefaults::headline_style().font_size, Some(crate::unit::TextUnit::Sp(crate::unit::Sp(16.0))));
        assert_eq!(ListItemDefaults::supporting_style().font_size, Some(crate::unit::TextUnit::Sp(crate::unit::Sp(14.0))));
        assert_eq!(ListItemDefaults::overline_style().font_size, Some(crate::unit::TextUnit::Sp(crate::unit::Sp(11.0))));
    }

    #[test]
    fn list_item_layout_uses_m3_heights_and_horizontal_inset() {
        let cases = [
            (ListItem::new(|ctx| crate::ui::Text::new("Headline").build(ctx)), LIST_ITEM_ONE_LINE_HEIGHT),
            (ListItem::new(|ctx| crate::ui::Text::new("Headline").build(ctx)).supporting_content(|ctx| crate::ui::Text::new("Supporting").build(ctx)), LIST_ITEM_TWO_LINE_HEIGHT),
            (ListItem::new(|ctx| crate::ui::Text::new("Headline").build(ctx)).overline_content(|ctx| crate::ui::Text::new("Overline").build(ctx)).supporting_content(|ctx| crate::ui::Text::new("Supporting").build(ctx)), LIST_ITEM_THREE_LINE_HEIGHT),
        ];
        for (item, expected_height) in cases {
            let composer = layout_item(item, 320.0);
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            assert_eq!(nodes[root].measured_size.width, 320.0);
            assert_eq!(nodes[root].measured_size.height, expected_height);
            let row = child(nodes, root, 0);
            assert_eq!(row.position.x, LIST_ITEM_HORIZONTAL_PADDING);
            assert!(row.position.y >= LIST_ITEM_VERTICAL_PADDING);
            assert!(row.position.y + row.measured_size.height <= expected_height - LIST_ITEM_VERTICAL_PADDING + 0.01);
            assert_eq!(row.measured_size.width, 320.0 - 2.0 * LIST_ITEM_HORIZONTAL_PADDING);
        }
    }

    #[test]
    fn three_line_slots_are_top_aligned_and_do_not_overlap_text_column() {
        let composer = layout_item(
            ListItem::new(|ctx| crate::ui::Text::new("Headline").build(ctx))
                .overline_content(|ctx| crate::ui::Text::new("Overline").build(ctx))
                .supporting_content(|ctx| crate::ui::Text::new("Supporting").build(ctx))
                .leading_content(|ctx| crate::ui::Spacer::vertical(24.0).build(ctx))
                .trailing_content(|ctx| crate::ui::Spacer::vertical(24.0).build(ctx)),
            320.0,
        );
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let row = child(nodes, root, 0);
        assert_eq!(row.children.len(), 3);
        let leading = child(nodes, row_index(nodes, root), 0);
        let text = child(nodes, row_index(nodes, root), 1);
        let trailing = child(nodes, row_index(nodes, root), 2);
        assert_eq!(leading.position.y, 0.0);
        assert_eq!(trailing.position.y, 0.0);
        assert!(leading.position.x + leading.measured_size.width + LIST_ITEM_SLOT_GAP <= text.position.x + 0.01);
        assert!(text.position.x + text.measured_size.width + LIST_ITEM_SLOT_GAP <= trailing.position.x + 0.01);
    }

    fn row_index(nodes: &[crate::layout::node::LayoutNode], root: usize) -> usize {
        nodes[root].children[0]
    }
    #[test]
    fn list_items_measure_inside_lazy_column() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            crate::ui::LazyColumn::new()
                .modifier(Modifier::new().fill_max_width().height(200.0))
                .items_plain(3, |ctx, index| {
                    ListItem::new(move |ctx| crate::ui::Text::new(format!("Item {index}")).build(ctx))
                        .build(ctx);
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 320.0, 0.0, 200.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let lazy = &nodes[root];
        assert_eq!(lazy.measured_size.width, 320.0);
        assert_eq!(lazy.measured_size.height, 200.0);
        assert!(!lazy.children.is_empty(), "LazyColumn should register visible ListItem children");
        for &child_idx in &lazy.children {
            assert_eq!(nodes[child_idx].measured_size.width, 320.0);
            assert_eq!(nodes[child_idx].measured_size.height, LIST_ITEM_ONE_LINE_HEIGHT);
        }
    }
    #[test]
    fn list_item_builder_preserves_slots_and_state() {
        let item = ListItem::new(|_| {})
            .supporting_content(|_| {})
            .leading_content(|_| {})
            .trailing_content(|_| {})
            .enabled(false);
        assert!(!item.get_enabled());
        assert!(item.get_colors().is_none());
    }

    #[test]
    fn static_list_item_has_no_interaction_elements() {
        let mut composer = Composer::new();
        composer.compose(|ctx| ListItem::new(|_| {}).build(ctx));
        let root = composer.layout_root().unwrap();
        assert!(root.modifier.clickable_interaction().is_none());
        assert!(root.modifier.ripple_interaction().is_none());
    }

    #[test]
    fn clickable_list_item_registers_interaction_elements() {
        let mut composer = Composer::new();
        composer.compose(|ctx| ListItem::new(|_| {}).on_click(|| {}).build(ctx));
        let root = composer.layout_root().unwrap();
        assert!(root.modifier.clickable_interaction().is_some());
        assert!(root.modifier.ripple_interaction().is_some());
        assert!(root.modifier.focusable_interaction().is_some());
    }
}
