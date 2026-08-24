//! Material 3 Expressive ShortNavigationBar / ShortNavigationBarItem。
//!
//! 对齐 androidx-main ShortNavigationBar.kt（material3 1.5.0-alpha，替代经典
//! NavigationBar 的新底栏）：
//!
//! - **arrangement**（`ShortNavigationBarArrangement`）：
//!   - `EqualWeight`：小屏（3-5 项）等分均布——同经典 NavigationBar 的 weight 语义
//!   - `Centered`：中屏（3-6 项）居中分组——item 拥抱内容、整组居中，且组占宽
//!     受百分比约束（androidx CenteredContentMeasurePolicy：3 项占 60%、4 项 70%、
//!     5 项 80%、6 项 90%，超限按比例收缩）
//! - **item 复用 `NavigationBarItem`**：androidx ShortNavigationBarItem 内部即
//!   NavigationItem 同一套 token/数学——Top（图标上）/Start（图标左）两个
//!   图标位、同一 `NavigationBarItemColors` 7 色槽
//! - 容器高 80（TallContainerHeight）、surfaceContainer 底色、item 间距 8、RTL 镜像

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::constraints::Constraints;
use crate::layout::node::{MeasurePolicy, Placement, Point, Size};
use crate::layout::LayoutDirection;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::navigation_bar::{
    NavigationBarItem, NavigationBarItemColors, NavigationItemIconPosition, NAVIGATION_BAR_HEIGHT,
    NAVIGATION_BAR_ITEM_SPACING,
};
use crate::ui::theme::WiniaTheme;
use std::sync::Arc;

/// ShortNavigationBar 的 item 排布（对齐 androidx ShortNavigationBarArrangement）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortNavigationBarArrangement {
    /// 小屏：item 等分均布（3-5 项）
    EqualWeight,
    /// 中屏：item 拥抱内容、整组居中，组占宽受百分比约束（3:60%/4:70%/5:80%/6:90%）
    Centered,
}

/// Centered 模式组占宽比例（androidx CenteredContentMeasurePolicy 注释：
/// "3 items: the items should occupy 60% of the bar's width" …… >6 项不约束）
fn centered_occupancy(n: usize) -> f32 {
    match n {
        3 => 0.6,
        4 => 0.7,
        5 => 0.8,
        6 => 0.9,
        _ => 1.0,
    }
}

// ── ShortNavigationBarItem（NavigationBarItem 的 Expressive 门面）──

/// 底栏 item——androidx ShortNavigationBarItem 内部即 NavigationItem 同一套
/// token/布局数学，这里直接委托 [`NavigationBarItem`]（Top/Start 图标位、
/// 7 色槽 colors、interaction source 语义完全一致）。
pub struct ShortNavigationBarItem {
    item: NavigationBarItem,
}

impl ShortNavigationBarItem {
    pub fn new(selected: bool, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self { item: NavigationBarItem::new(selected, icon) }
    }

    pub fn label(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.item = self.item.label(content);
        self
    }

    pub fn on_click(mut self, callback: impl Fn() + Send + Sync + 'static) -> Self {
        self.item = self.item.on_click(callback);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.item = self.item.enabled(enabled);
        self
    }

    /// 图标位置：Top（默认，小屏竖排）/ Start（中屏横排，图标在左）
    pub fn icon_position(mut self, position: NavigationItemIconPosition) -> Self {
        self.item = self.item.icon_position(position);
        self
    }

    pub fn colors(mut self, colors: NavigationBarItemColors) -> Self {
        self.item = self.item.colors(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.item = self.item.interaction_source(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.item = self.item.modifier(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        self.item.build(ctx);
    }
}

// ── ShortNavigationBar 容器 ──

pub struct ShortNavigationBar {
    arrangement: ShortNavigationBarArrangement,
    container_color: Option<Color>,
    modifier: Modifier,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
}

impl ShortNavigationBar {
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            arrangement: ShortNavigationBarArrangement::EqualWeight,
            container_color: None,
            modifier: Modifier::new(),
            content: Box::new(content),
        }
    }

    /// item 排布：EqualWeight（默认，小屏）/ Centered（中屏）
    pub fn arrangement(mut self, arrangement: ShortNavigationBarArrangement) -> Self {
        self.arrangement = arrangement;
        self
    }

    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.arrangement);
        ctx.changed(&self.container_color);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.surface_container);
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);
        let content = self.content;
        let policy = ShortNavigationBarLayoutPolicy { arrangement: self.arrangement, direction };
        let root_modifier = Modifier::new()
            .fill_max_width()
            .height(NAVIGATION_BAR_HEIGHT)
            .background(container, Shape::Rectangle)
            .then(self.modifier);
        match ctx.start_restartable_group(key, root_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                WiniaTheme::with_content_color(theme.on_surface, ctx, content);
            }
        }
        ctx.end_restartable_group();
    }
}

/// 容器布局：EqualWeight 等分 / Centered 拥抱内容 + 占宽约束 + 居中；RTL 镜像。
#[derive(Debug)]
struct ShortNavigationBarLayoutPolicy {
    arrangement: ShortNavigationBarArrangement,
    direction: LayoutDirection,
}

impl MeasurePolicy for ShortNavigationBarLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        c: Constraints,
    ) -> (Size, Vec<Placement>) {
        let width = c.max_width;
        let height = c.max_height.min(NAVIGATION_BAR_HEIGHT);
        let n = children.len();
        if n == 0 {
            return (Size::new(width, height), Vec::new());
        }
        let spacing_total = NAVIGATION_BAR_ITEM_SPACING * (n as f32 - 1.0);

        // 各 item 宽度
        let widths: Vec<f32> = match self.arrangement {
            // EqualWeight：等分（同经典 bar 的 weight(1f)——tight 主轴）
            ShortNavigationBarArrangement::EqualWeight => {
                let item_w = ((width - spacing_total) / n as f32).max(0.0);
                vec![item_w; n]
            }
            // Centered：拥抱内容（松约束），组占宽超限时按比例收缩
            ShortNavigationBarArrangement::Centered => {
                let mut ws = Vec::with_capacity(n);
                for &child in children {
                    let (size, _) = crate::layout::node::measure_node(
                        nodes, policies, child,
                        Constraints::new(0.0, width, 0.0, c.max_height),
                    );
                    ws.push(size.width);
                }
                let raw_total: f32 = ws.iter().sum::<f32>() + spacing_total;
                let max_region = width * centered_occupancy(n);
                if raw_total > max_region && raw_total > 0.0 {
                    let factor = (max_region - spacing_total) / (raw_total - spacing_total);
                    ws.into_iter().map(|w| (w * factor).max(0.0)).collect()
                } else {
                    ws
                }
            }
        };

        let total: f32 = widths.iter().sum::<f32>() + spacing_total;
        // Centered：整组居中；EqualWeight：从 leading 起（等分已占满）
        let mut x = match self.arrangement {
            ShortNavigationBarArrangement::Centered => ((width - total) / 2.0).max(0.0),
            ShortNavigationBarArrangement::EqualWeight => 0.0,
        };

        let mut placements = Vec::with_capacity(n);
        for (index, &child) in children.iter().enumerate() {
            let item_w = widths[index];
            let (size, _) = crate::layout::node::measure_node(
                nodes, policies, child,
                Constraints::new(item_w, item_w, 0.0, c.max_height),
            );
            let px = if self.direction == LayoutDirection::Ltr {
                x
            } else {
                width - x - item_w
            };
            placements.push(Placement { size, position: Point::new(px, 0.0) });
            x += item_w + NAVIGATION_BAR_ITEM_SPACING;
        }
        (Size::new(width, height), placements)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::node::LayoutNode;
    use crate::modifier::Modifier as M;

    fn item(ctx: &mut ComposeCtx, selected: bool, start: bool, icon_w: f32) {
        let mut b = ShortNavigationBarItem::new(selected, move |ctx| {
            let k = ctx.next_key();
            ctx.start_leaf(k, M::new().size(icon_w, icon_w));
            ctx.end_node();
        })
        .label(|ctx| crate::ui::Text::new("Home").build(ctx))
        .on_click(|| {});
        if start {
            b = b.icon_position(NavigationItemIconPosition::Start);
        }
        b.build(ctx);
    }

    fn compose_bar(arrangement: ShortNavigationBarArrangement, n: usize, start: bool, icon_w: f32) -> Composer {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let mut bar = ShortNavigationBar::new(move |ctx| {
                for _ in 0..n {
                    item(ctx, true, start, icon_w);
                }
            })
            .arrangement(arrangement);
            bar.build(ctx);
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
        composer
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, i: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[i]]
    }

    #[test]
    fn equal_weight_distributes_items_evenly() {
        // 3 项 360 宽：(360 - 2x8) / 3 = 114.67
        let composer = compose_bar(ShortNavigationBarArrangement::EqualWeight, 3, false, 24.0);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let expected = (360.0 - 2.0 * NAVIGATION_BAR_ITEM_SPACING) / 3.0;
        for i in 0..3 {
            let item = child(nodes, root, i);
            assert!((item.measured_size.width - expected).abs() < 0.01, "item{i} 等分");
        }
        // 首项贴 leading
        assert_eq!(child(nodes, root, 0).position.x, 0.0);
    }

    #[test]
    fn centered_hugs_and_caps_region_by_occupancy() {
        // 3 项 Start（横排内容较宽）：组占宽 ≤ 60% x 360 = 216，整组居中
        let composer = compose_bar(ShortNavigationBarArrangement::Centered, 3, true, 24.0);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let first = child(nodes, root, 0);
        let last = child(nodes, root, 2);
        let total = last.position.x + last.measured_size.width - first.position.x;
        assert!(total <= 360.0 * 0.6 + 0.5, "组占宽 {total} 应 ≤ 216（60%）");
        // 居中：左右边距对称
        let left = first.position.x;
        let right = 360.0 - (last.position.x + last.measured_size.width);
        assert!((left - right).abs() < 0.5, "居中对称 left={left} right={right}");
    }

    #[test]
    fn centered_small_content_stays_centered_without_shrink() {
        // Top 图标位内容窄（24 图标）：不触百分比上限——仅居中
        let composer = compose_bar(ShortNavigationBarArrangement::Centered, 3, false, 24.0);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let first = child(nodes, root, 0);
        let last = child(nodes, root, 2);
        let left = first.position.x;
        let right = 360.0 - (last.position.x + last.measured_size.width);
        assert!((left - right).abs() < 0.5);
        assert!(left > 0.0, "拥抱内容 < 360，应有居中边距");
    }

    #[test]
    fn rtl_mirrors_item_order() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ShortNavigationBar::new(move |ctx| {
                for _ in 0..3 {
                    item(ctx, true, false, 24.0);
                }
            })
            .arrangement(ShortNavigationBarArrangement::EqualWeight)
            .modifier(M::new().layout_direction(LayoutDirection::Rtl))
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let first = child(nodes, root, 0);
        assert!(first.position.x > 0.0, "RTL：首项应镜像到右侧");
    }
}
