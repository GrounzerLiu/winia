//! Flow 流式布局 — 对标 Compose foundation `FlowRow` / `FlowColumn`
 //!（`FlowLayout.kt` + `FlowLayoutBuildingBlocks.kt`，源码见 `tmp/flow-src/`）。
//!
//! 主轴填满即换行（`getWrapInfo` 断行语义）：行首不换、超 `max_items` 换、
//! 主轴剩余放不下即换。行高 = 行内最高项；行内交叉轴按 alignment 对齐；
//! 行内主轴按 arrangement 分配剩余（复用 `compute_spacing`）；行间 spacing。
//!
//! v1 范围（与 Compose 差异，文档注明）：
//! - 无 weight（Compose 按行内剩余二次分配，需两阶段重测——后续加）
//! - 无 maxLines/overflow（情境 API——后续加）
//! - 无 intrinsic（winia 无 intrinsic 体系）

use super::constraints::Constraints;
use super::flex::FlexAxis;
use super::node::*;

/// 参数化 flow 测量。FlowRow（水平主轴）/ FlowColumn（垂直主轴）委托到此函数。
pub(crate) fn measure_flow<A: FlexAxis>(
    arrangement: Arrangement,
    alignment: Alignment,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_main_axis: usize,
    direction: LayoutDirection,
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    children: &[usize],
    constraints: &Constraints,
) -> (Size, Vec<Placement>) {
    let n = children.len();
    if n == 0 {
        return (
            Size::new(
                constraints.constrain_width(0.0),
                constraints.constrain_height(0.0),
            ),
            Vec::new(),
        );
    }

    let max_main = A::main_max(constraints);
    // 无界主轴：永不换行（退化为 Row/Column 单行——Compose 同：约束无界即单行）
    let can_wrap = max_main.is_finite();

    // ── 逐项 loose 测量（无 weight——v1 直接测自然尺寸）──
    let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
    for (i, &c) in children.iter().enumerate() {
        let cc = A::build_phase1(constraints, f32::MAX);
        let (size, _) = measure_node(nodes, policies, c, cc);
        child_sizes[i] = size;
    }

    // ── 断行（对标 getWrapInfo shouldWrapItem：行首不换、超数换、放不下换）──
    // lines: 每行 (start, end) 半开区间
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0usize;
    let mut line_main = 0.0f32; // 当前行已占主轴（含行内 spacing）
    let max_items = max_items_in_main_axis.max(1);
    for i in 0..n {
        let item_main = A::main_size(child_sizes[i]);
        let index_in_line = i - line_start;
        let need = if index_in_line == 0 { item_main } else { main_spacing + item_main };
        let should_wrap = can_wrap
            && (index_in_line >= max_items
                || (index_in_line > 0 && line_main + need > max_main));
        if should_wrap {
            lines.push((line_start, i));
            line_start = i;
            line_main = item_main;
        } else {
            line_main += need;
        }
    }
    lines.push((line_start, n));

    // ── 行尺寸：行主轴 = 内容宽（clamp 进约束），行交叉 = 行内最高 ──
    struct LineInfo {
        main: f32,
        cross: f32,
    }
    let mut line_infos: Vec<LineInfo> = Vec::with_capacity(lines.len());
    let mut content_cross = 0.0f32;
    for &(s, e) in &lines {
        let mut main = 0.0f32;
        let mut cross = 0.0f32;
        for i in s..e {
            let sz = child_sizes[i];
            if i > s {
                main += main_spacing;
            }
            main += A::main_size(sz);
            cross = cross.max(A::cross_size(sz));
        }
        // 行主轴 clamp 进约束（单项超宽时行宽 = 约束宽，不溢出容器尺寸）
        main = A::constrain_main(constraints, main);
        line_infos.push(LineInfo { main, cross });
        content_cross += cross;
    }
    let total_cross_spacing = cross_spacing * (lines.len() as f32 - 1.0).max(0.0);
    content_cross += total_cross_spacing;

    // ── 容器尺寸 ──
    let container_main = A::constrain_main(
        constraints,
        line_infos.iter().map(|l| l.main).fold(0.0f32, f32::max),
    );
    let container_cross = A::constrain_cross(constraints, content_cross);

    // ── 放置 ──
    let mut placements: Vec<Placement> = vec![
        Placement {
            size: Size::ZERO,
            position: Point::new(0.0, 0.0),
        };
        n
    ];
    let mut cross_offset = 0.0f32;
    for (li, &(s, e)) in lines.iter().enumerate() {
        let info = &line_infos[li];
        let count = e - s;
        // 行内剩余（行宽用容器主轴——SpaceBetween 等填满容器；普通行内容即行宽时 remaining=0）
        // ⚠ Compose 行内 arrangement 基于行布局尺寸：行布局尺寸 = max(内容, 容器约束min)。
        // winia 简化：行布局尺寸 = 容器主轴（fill 语义）——Start/Center/End 在容器内对齐，
        // 与 Compose wrap-content 下行内对齐有差异（Compose 行宽=内容宽时对齐无可见差）。
        // 为保持 wrap-content 下行为直观（内容即行），行布局尺寸取 max(内容, min约束)：
        let row_layout_main = info.main.max(A::constrain_main(constraints, 0.0));
        // 实际上：fill 父（min=max）→ 行宽=容器宽，对齐在容器内；wrap 父（min=0）→ 行宽=内容宽。
        // 上式 constrain_main(constraints, 0.0) = min（fill 时=min=max=容器宽，wrap 时=0）。
        // 故 row_layout_main = max(内容, min)——fill 时容器宽，wrap 时内容宽。正确。
        let remaining = (row_layout_main - info.main).max(0.0);
        let gap_count = if count > 1 { count - 1 } else { 0 };
        let (spacing_extra, leading) = compute_spacing(arrangement, remaining, gap_count);
        let eff_spacing = main_spacing + spacing_extra;

        let mut main_offset = leading;
        for i in s..e {
            let sz = child_sizes[i];
            let child_main = A::main_size(sz);
            let child_cross_full = A::cross_size(sz);
            // 行内交叉轴对齐（align_self 覆盖容器 alignment——复用 flex 语义）
            let align = nodes[children[i]]
                .modifier
                .get_align_self()
                .unwrap_or(alignment);
            let child_cross = if align == Alignment::Stretch {
                info.cross
            } else {
                child_cross_full
            };
            let cross_in_line = match align {
                Alignment::Start => 0.0,
                Alignment::End => info.cross - child_cross,
                Alignment::Center => (info.cross - child_cross) / 2.0,
                Alignment::Stretch => 0.0,
            };
            placements[i] = Placement {
                size: A::size(child_main, child_cross),
                position: A::point(main_offset, cross_offset + cross_in_line),
            };
            main_offset += child_main + eff_spacing;
        }
        cross_offset += info.cross + cross_spacing;
    }

    // ── RTL 镜像（复用 flex 终镜模式：容器宽 - x - w）──
    if direction == LayoutDirection::Rtl {
        let container_x = A::rtl_container_width(
            A::main_size(A::size(container_main, container_cross)),
            container_cross,
        );
        for p in &mut placements {
            p.position.x = container_x - p.position.x - p.size.width;
        }
    }

    (
        A::size(container_main, container_cross),
        placements,
    )
}

// ── FlowRow / FlowColumn 布局策略 ──

/// FlowRow 布局策略（水平主轴，超宽换行）
#[derive(Debug, Clone)]
pub struct FlowRowLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub main_spacing: f32,
    pub cross_spacing: f32,
    pub max_items_in_row: usize,
    pub direction: LayoutDirection,
}

impl FlowRowLayout {
    pub fn new() -> Self {
        FlowRowLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_row: usize::MAX,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_row(mut self, m: usize) -> Self { self.max_items_in_row = m; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for FlowRowLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for FlowRowLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        measure_flow::<super::flex::HorizontalAxis>(
            self.arrangement,
            self.alignment,
            self.main_spacing,
            self.cross_spacing,
            self.max_items_in_row,
            self.direction,
            nodes,
            policies,
            children,
            &constraints,
        )
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

/// FlowColumn 布局策略（垂直主轴，超高换列）
#[derive(Debug, Clone)]
pub struct FlowColumnLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub main_spacing: f32,
    pub cross_spacing: f32,
    pub max_items_in_column: usize,
    pub direction: LayoutDirection,
}

impl FlowColumnLayout {
    pub fn new() -> Self {
        FlowColumnLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_column: usize::MAX,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_column(mut self, m: usize) -> Self { self.max_items_in_column = m; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for FlowColumnLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for FlowColumnLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        measure_flow::<super::flex::VerticalAxis>(
            self.arrangement,
            self.alignment,
            self.main_spacing,
            self.cross_spacing,
            self.max_items_in_column,
            self.direction,
            nodes,
            policies,
            children,
            &constraints,
        )
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(width: f32, height: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        LayoutNode::leaf(Modifier::new().size(width, height))
    }

    #[test]
    fn flow_row_wraps_when_out_of_space() {
        // 容器宽 100 约束（wrap：min=0）：30+30+30=90 放下 3 个，第 4 个换行；
        // 容器宽 = 内容最宽行 90（wrap 语义）
        let policy = FlowRowLayout::new();
        let mut nodes = vec![
            make_leaf(30.0, 20.0),
            make_leaf(30.0, 20.0),
            make_leaf(30.0, 20.0),
            make_leaf(30.0, 20.0),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, f32::MAX),
        );
        assert_eq!(size.width, 90.0, "wrap 下容器宽 = 最宽行 90");
        assert_eq!(size.height, 40.0, "两行 × 20 高");
        assert_eq!(placements[0].position, Point::new(0.0, 0.0));
        assert_eq!(placements[2].position, Point::new(60.0, 0.0));
        assert_eq!(placements[3].position, Point::new(0.0, 20.0), "第 4 个换行");
    }

    #[test]
    fn flow_row_fills_container_when_tight() {
        // tight 约束（min=max=100，fill 语义）：容器宽 100，行内 Start 对齐容器
        let policy = FlowRowLayout::new();
        let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(100.0, 100.0, 0.0, f32::MAX),
        );
        assert_eq!(size.width, 100.0, "fill 下容器宽 = 约束宽");
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 30.0);
    }

    #[test]
    fn flow_row_single_line_when_unbounded() {
        // 主轴无界：永不换行（退化为 Row）
        let policy = FlowRowLayout::new();
        let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, _) = policy.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 60.0);
        assert_eq!(size.height, 20.0);
    }

    #[test]
    fn flow_row_max_items_forces_break() {
        // max_items=2：即使放得下也换行
        let policy = FlowRowLayout::new().max_items_in_row(2);
        let mut nodes = vec![
            make_leaf(10.0, 20.0),
            make_leaf(10.0, 20.0),
            make_leaf(10.0, 20.0),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, f32::MAX),
        );
        assert_eq!(size.height, 40.0, "两行");
        assert_eq!(placements[2].position, Point::new(0.0, 20.0));
    }

    #[test]
    fn flow_row_spacing_and_cross_alignment() {
        // 主轴 spacing 8 + 行间 10 + 行内居中
        let policy = FlowRowLayout::new()
            .main_spacing(8.0)
            .cross_spacing(10.0)
            .alignment(Alignment::Center);
        let mut nodes = vec![
            make_leaf(30.0, 10.0),
            make_leaf(30.0, 30.0), // 行高 30
            make_leaf(30.0, 10.0), // 换行（30+8+30+8+30=106 > 100）
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, f32::MAX),
        );
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 38.0, "30+8");
        // 行高 30，10 高项居中 → y=10
        assert_eq!(placements[0].position.y, 10.0);
        assert_eq!(placements[2].position.y, 40.0, "第二行 y=30+10");
        assert_eq!(size.height, 50.0, "30+10+10");
    }

    #[test]
    fn flow_row_rtl_mirrors() {
        let policy = FlowRowLayout::new().direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, f32::MAX),
        );
        // wrap：容器宽 = 内容 60；镜像基准 60
        assert_eq!(size.width, 60.0);
        // LTR: 0/30；RTL 镜像（容器 60）：30/0
        assert_eq!(placements[0].position.x, 30.0);
        assert_eq!(placements[1].position.x, 0.0);
    }

    #[test]
    fn flow_column_wraps_to_next_column() {
        // 容器高 100 约束（wrap：min=0）：30+30+30=90 放下 3 个，第 4 个换列；
        // 容器高 = 内容最高列 90
        let policy = FlowColumnLayout::new();
        let mut nodes = vec![
            make_leaf(20.0, 30.0),
            make_leaf(20.0, 30.0),
            make_leaf(20.0, 30.0),
            make_leaf(20.0, 30.0),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, f32::MAX, 0.0, 100.0),
        );
        assert_eq!(size.height, 90.0, "wrap 下容器高 = 最高列 90");
        assert_eq!(size.width, 40.0, "两列 × 20 宽");
        assert_eq!(placements[3].position, Point::new(20.0, 0.0), "第 4 个换列");
    }
}
