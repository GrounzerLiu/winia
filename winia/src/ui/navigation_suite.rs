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
//! 默认推算（[`navigation_suite_type`]，M3 规范的桌面近似）：矮窗优先底栏
//! （横排 item），其次按宽度 Expanded→展开轨 / Medium→收起轨 / Compact→底栏。
//! 窗口 resize 由 app 层触发重组，形态实时切换。
//!
//! `primaryActionContent`（androidx 的 FAB 槽）暂未实现——FAB 请放入
//! WideNavigationRail 的 header（套件内建 rail 暂无 header 槽，后续版本补）。

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::{Constraints, MeasurePolicy};
use crate::layout::node::{measure_node, Placement, Point, Size};
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::adaptive::{window_height_size_class, window_width_size_class, HeightSizeClass, WidthSizeClass};
use crate::ui::navigation_bar::NavigationItemIconPosition;
use crate::ui::short_navigation_bar::{ShortNavigationBar, ShortNavigationBarArrangement, ShortNavigationBarItem};
use crate::ui::theme::WiniaTheme;
use crate::ui::layout_components::{Column, Row};
use crate::ui::navigation_rail::{WideNavigationRail, WideNavigationRailItem, WideNavigationRailState};
use std::sync::Arc;

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
}

/// 默认形态推算（M3 规范桌面近似）：矮窗 → 横排底栏；宽 Expanded → 展开轨；
/// 宽 Medium → 收起轨；其余 → 竖排底栏
pub fn navigation_suite_type() -> NavigationSuiteType {
    if window_height_size_class() == HeightSizeClass::Compact {
        return NavigationSuiteType::ShortNavigationBarMedium;
    }
    match window_width_size_class() {
        WidthSizeClass::Expanded => NavigationSuiteType::WideNavigationRailExpanded,
        WidthSizeClass::Medium => NavigationSuiteType::WideNavigationRailCollapsed,
        WidthSizeClass::Compact => NavigationSuiteType::ShortNavigationBarCompact,
    }
}

/// 套件 item 数据（形态无关——由 scaffold 按当前形态渲染成
/// ShortNavigationBarItem 或 WideNavigationRailItem）
pub struct NavigationSuiteItem {
    pub(crate) selected: bool,
    pub(crate) on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) label: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
}

/// items 收集器（`NavigationSuiteScaffold::new` 的 items 闭包参数）
#[derive(Default)]
pub struct NavigationSuiteItems(Vec<NavigationSuiteItem>);

impl NavigationSuiteItems {
    /// 追加一个目的地
    pub fn item(
        &mut self,
        selected: bool,
        icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        on_click: impl Fn() + Send + Sync + 'static,
    ) {
        self.0.push(NavigationSuiteItem {
            selected,
            on_click: Some(Arc::new(on_click)),
            icon: Box::new(icon),
            label: Some(Box::new(label)),
        });
    }

    /// 无 label 的目的地（rail 中渲染为空标签——WideNavigationRailItem label 必选）
    pub fn item_without_label(
        &mut self,
        selected: bool,
        icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        on_click: impl Fn() + Send + Sync + 'static,
    ) {
        self.0.push(NavigationSuiteItem {
            selected,
            on_click: Some(Arc::new(on_click)),
            icon: Box::new(icon),
            label: None,
        });
    }
}

/// 自适应导航套件脚手架：items + content，按窗口尺寸类选择导航形态。
pub struct NavigationSuiteScaffold {
    items: Vec<NavigationSuiteItem>,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    /// None = 按窗口尺寸自动推算
    layout_type: Option<NavigationSuiteType>,
    container_color: Option<Color>,
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
            container_color: None,
            transition: true,
            modifier: Modifier::new(),
        }
    }

    /// 覆盖自动推算（对齐 androidx layoutType 参数）
    pub fn layout_type(mut self, layout_type: NavigationSuiteType) -> Self {
        self.layout_type = Some(layout_type);
        self
    }

    /// 导航容器底色（content 区背景 = theme.background）
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// 形态切换过渡开关（默认开启）——收拢→换形→展开（220ms 线性）
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
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.surface_container);
        let target_type = self.layout_type.unwrap_or_else(navigation_suite_type);
        let content = self.content;

        // ── 形态过渡状态机（收拢 → 换形 → 展开）──
        // 对标 AnimatedContent 的"淡出→换内容→淡入"，fade 换成尺寸：
        // Collapsing：当前形态容器宽度/高度 → 0（clip 裁剪溢出内容）
        // 到 0：current 切换到新形态（分支换槽，旧槽回收）
        // Idle：新形态从 0 展开到全尺寸
        // 单世代引擎兼容：收拢期旧分支 Skip（节点存活不重组），FnOnce items
        // 只在首次 Enter 消费——无需双世代
        let phase: State<SuitePhase> = ctx.remember(|| SuitePhase::Idle);
        let current: State<NavigationSuiteType> = ctx.remember(|| target_type);
        let next = ctx.remember_backchannel(|| None);

        let cur = current.get();
        let ph = phase.get();
        if self.transition && form_kind(cur) != form_kind(target_type) {
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
            if phase.get() == SuitePhase::Collapsing { 0.0 } else { 1.0 },
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(220),
                crate::animation::interpolator::Linear::new(),
            )),
        );
        let scale = progress.get().max(0.0).min(1.0);

        // 收拢完成：换形态 + 展开（set 通知 → 下帧以新形态渲染）
        if ph == SuitePhase::Collapsing && scale <= 0.01 {
            if let Some(t) = next.peek() {
                current.set(t);
            }
            next.set(None);
            phase.set(SuitePhase::Idle);
        }

        let suite_type = cur;

        // 内容区背景（对标 scaffold containerColor = background）
        let content_modifier = Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .background(theme.background, crate::modifier::Shape::Rectangle);

        match suite_type {
            NavigationSuiteType::None => {
                let k = ctx.next_key();
                match ctx.start_restartable_group(k, content_modifier, crate::layout::BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => content(ctx),
                }
                ctx.end_restartable_group();
            }
            // 底栏形态：Column { content(weight 1), bar }
            NavigationSuiteType::ShortNavigationBarCompact | NavigationSuiteType::ShortNavigationBarMedium => {
                let (arrangement, icon_position) = match suite_type {
                    NavigationSuiteType::ShortNavigationBarCompact => {
                        (ShortNavigationBarArrangement::EqualWeight, NavigationItemIconPosition::Top)
                    }
                    _ => (ShortNavigationBarArrangement::Centered, NavigationItemIconPosition::Start),
                };
                Column::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height().then(self.modifier))
                    .build(ctx, |ctx| {
                        let k = ctx.next_key();
                        match ctx.start_restartable_group(k, content_modifier.layout_weight(1.0), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => content(ctx),
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
                                    for item in self.items {
                                        render_bar_item(ctx, item, icon_position);
                                    }
                                })
                                .arrangement(arrangement)
                                .container_color(container)
                                .build(ctx);
                            }
                        }
                        ctx.end_restartable_group();
                    });
            }
            // 宽轨形态：Row { rail, content(fill) }
            NavigationSuiteType::WideNavigationRailCollapsed | NavigationSuiteType::WideNavigationRailExpanded => {
                let expanded = suite_type == NavigationSuiteType::WideNavigationRailExpanded;
                let rail_state = WideNavigationRailState::new(ctx);
                if expanded { rail_state.expand(); } else { rail_state.collapse(); }
                // item 变形进度（与容器同 spec 同目标——视觉同步，同 demo 模式）
                let rail_item_progress = ctx.animate_float_as_state(
                    if expanded { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 400.0,
                        mass: 1.0,
                        threshold: 0.01,
                    }),
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
                                let items = self.items;
                                WideNavigationRail::new(rail_state, move |ctx| {
                                    for item in items {
                                        let mut b = WideNavigationRailItem::new(
                                            item.selected,
                                            item.icon,
                                            item.label.unwrap_or_else(|| Box::new(|ctx| crate::ui::Text::new("").build(ctx))),
                                        )
                                        .progress(rail_item_progress.clone())
                                        .on_click(move || {
                                            if let Some(cb) = item.on_click.as_ref() {
                                                cb()
                                            }
                                        });
                                        b.build(ctx);
                                    }
                                })
                                .container_color(container)
                                .build(ctx);
                            }
                        }
                        ctx.end_restartable_group();
                        let k = ctx.next_key();
                        match ctx.start_restartable_group(k, content_modifier, crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => content(ctx),
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
        NavigationSuiteType::ShortNavigationBarCompact | NavigationSuiteType::ShortNavigationBarMedium => 0,
        NavigationSuiteType::WideNavigationRailCollapsed | NavigationSuiteType::WideNavigationRailExpanded => 1,
        NavigationSuiteType::None => 2,
    }
}

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
            placements.push(Placement { size, position: Point::new(0.0, 0.0) });
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
) {
    let mut b = ShortNavigationBarItem::new(item.selected, item.icon)
        .icon_position(icon_position)
        .on_click(move || {
            if let Some(cb) = item.on_click.as_ref() {
                cb()
            }
        });
    if let Some(label) = item.label {
        b = b.label(label);
    }
    b.build(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
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
                        |ctx| crate::ui::Text::new("Home").build(ctx),
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
        crate::ui::adaptive::reset_window_size_state();
        crate::ui::adaptive::set_window_size(500.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
        crate::ui::adaptive::set_window_size(700.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        crate::ui::adaptive::set_window_size(1000.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailExpanded);
        // 矮窗优先横排底栏
        crate::ui::adaptive::set_window_size(1000.0, 400.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarMedium);
    }

    /// 窄窗 → 底栏形态：Column 根，bar 在底部（高 80、贴底）
    #[test]
    fn narrow_window_renders_bottom_bar() {
        crate::ui::adaptive::reset_window_size_state();
        crate::ui::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // Column 根：最后子项 = bar（高 80、y = 700-80 = 620）
        let bar = &nodes[*nodes[root].children.last().unwrap()];
        assert_eq!(bar.measured_size.height, 80.0);
        assert!((bar.position.y - 620.0).abs() < 0.01, "bar 贴底 y={}", bar.position.y);
    }

    /// 宽窗 → 展开宽轨形态：Row 根，rail 在前（宽 220），内容填余下
    #[test]
    fn wide_window_renders_expanded_rail() {
        crate::ui::adaptive::reset_window_size_state();
        crate::ui::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = &nodes[root].children;
        // Row 根：首子 = rail（宽 220），末子 = content（填余下 780）
        let rail = &nodes[children[0]];
        let content = &nodes[children[children.len() - 1]];
        assert_eq!(rail.measured_size.width, 220.0, "展开轨宽 220");
        assert!((content.measured_size.width - 780.0).abs() < 0.01,
            "内容填余下 width={}", content.measured_size.width);
    }

    /// 中宽窗 → 收起轨（96）
    #[test]
    fn medium_window_renders_collapsed_rail() {
        crate::ui::adaptive::reset_window_size_state();
        crate::ui::adaptive::set_window_size(700.0, 700.0);
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
        crate::ui::adaptive::reset_window_size_state();
        let size = crate::core::state::State::new((500.0f32, 700.0f32));
        crate::ui::adaptive::set_window_size_state(size.clone());
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
        size.set((1000.0, 700.0));
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailExpanded);
        size.set((700.0, 700.0));
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        crate::ui::adaptive::reset_window_size_state();
    }

    /// 显式 layout_type 覆盖自动推算
    #[test]
    fn explicit_layout_type_overrides_auto() {
        crate::ui::adaptive::reset_window_size_state();
        crate::ui::adaptive::set_window_size(1000.0, 700.0); // 自动会选展开轨
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
        crate::ui::adaptive::reset_window_size_state();
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, lt: NavigationSuiteType| {
            composer.compose(|ctx| {
                NavigationSuiteScaffold::new(
                    |items| {
                        items.item(
                            true,
                            |ctx| icon(ctx, 24.0),
                            |ctx| crate::ui::Text::new("A").build(ctx),
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
            let bar = &nodes[*nodes[root].children.last().unwrap()];
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
