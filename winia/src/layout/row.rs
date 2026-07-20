//! Row 布局 — 水平排列子节点
//!
//! 主轴 = 水平方向（宽度），交叉轴 = 垂直方向（高度）

use super::constraints::Constraints;
use super::node::*;
use super::column::measure_node;

/// Row 布局策略
#[derive(Debug, Clone)]
pub struct RowLayout {
    /// 主轴排列方式
    pub arrangement: Arrangement,
    /// 交叉轴对齐方式
    pub alignment: Alignment,
}

impl RowLayout {
    pub fn new() -> Self {
        RowLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self {
        self.arrangement = a;
        self
    }

    pub fn alignment(mut self, a: Alignment) -> Self {
        self.alignment = a;
        self
    }
}

impl Default for RowLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurePolicy for RowLayout {
    fn measure(
        &self,
        children: &mut [LayoutNode],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let mut total_width: f32 = 0.0;
        let mut max_height: f32 = 0.0;
        let mut placements: Vec<Placement> = Vec::with_capacity(children.len());

        let mut child_sizes: Vec<Size> = Vec::with_capacity(children.len());
        for child in children.iter_mut() {
            let child_constraints = Constraints {
                min_width: 0.0,
                max_width: (constraints.max_width - total_width).max(0.0),
                min_height: constraints.min_height,
                max_height: constraints.max_height,
            };
            let (child_size, _) = measure_node(child, child_constraints);
            total_width += child_size.width;
            max_height = max_height.max(child_size.height);
            child_sizes.push(child_size);
        }

        let remaining_width = (constraints.max_width - total_width).max(0.0);
        let gap_count = if children.len() > 1 { children.len() - 1 } else { 0 };

        // Stretch 对齐时扩展宽度
        let final_widths: Vec<f32> = if self.alignment == Alignment::Stretch {
            child_sizes
                .iter()
                .map(|s| s.width + remaining_width / children.len() as f32)
                .collect()
        } else {
            child_sizes.iter().map(|s| s.width).collect()
        };

        let (spacing, leading_space) = compute_row_spacing(
            self.arrangement,
            remaining_width,
            gap_count,
        );

        let mut x_offset = leading_space;
        for (i, (_child, child_size)) in children.iter_mut().zip(child_sizes.iter()).enumerate() {
            let width = if self.alignment == Alignment::Stretch {
                final_widths[i]
            } else {
                child_size.width
            };

            let y = match self.alignment {
                Alignment::Start => 0.0,
                Alignment::End => max_height - child_size.height,
                Alignment::Center => (max_height - child_size.height) / 2.0,
                Alignment::Stretch => 0.0,
            };

            let height = if self.alignment == Alignment::Stretch {
                max_height
            } else {
                child_size.height
            };

            placements.push(Placement {
                size: Size::new(width, height),
                position: Point::new(x_offset, y),
            });

            x_offset += width + spacing;
        }

        let measured_width = constraints.constrain_width(
            final_widths.iter().sum::<f32>() + spacing * gap_count as f32,
        );
        let measured_height = constraints.constrain_height(max_height);

        (Size::new(measured_width, measured_height), placements)
    }

    fn place(&self, children: &mut [LayoutNode], placements: &[Placement]) {
        for (child, placement) in children.iter_mut().zip(placements.iter()) {
            child.position = placement.position;
            child.measured_size = placement.size;
        }
    }
}

fn compute_row_spacing(
    arrangement: Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        Arrangement::Start => (0.0, 0.0),
        Arrangement::End => (0.0, remaining),
        Arrangement::Center => (0.0, remaining / 2.0),
        Arrangement::SpaceBetween => {
            if gap_count > 0 {
                (remaining / gap_count as f32, 0.0)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        Arrangement::SpaceAround => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        Arrangement::SpaceEvenly => {
            let total_gaps = gap_count + 2;
            if total_gaps > 0 {
                let space = remaining / total_gaps as f32;
                (space, space)
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(w: f32, h: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(
            Modifier::new().size(w, h),
        );
        node.measured_size = Size::new(w, h);
        node
    }

    #[test]
    fn test_row_simple() {
        let row = RowLayout::new();
        let mut children = vec![
            make_leaf(20.0, 100.0),
            make_leaf(30.0, 80.0),
            make_leaf(10.0, 120.0),
        ];

        let (size, placements) = row.measure(
            &mut children,
            Constraints::UNBOUNDED,
        );

        assert_eq!(size, Size::new(60.0, 120.0));
        assert_eq!(placements.len(), 3);
        assert_eq!(placements[0].position, Point::new(0.0, 0.0));
        assert_eq!(placements[1].position, Point::new(20.0, 0.0));
        assert_eq!(placements[2].position, Point::new(50.0, 0.0));
    }

    #[test]
    fn test_row_space_between() {
        let row = RowLayout::new().arrangement(Arrangement::SpaceBetween);
        let mut children = vec![
            make_leaf(20.0, 50.0),
            make_leaf(20.0, 50.0),
        ];

        let (size, placements) = row.measure(
            &mut children,
            Constraints::new(0.0, 200.0, 0.0, f32::INFINITY),
        );

        // 总宽 40，剩余 160，1 个 gap → spacing = 160
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 20.0 + 160.0);
    }

    #[test]
    fn test_row_center_alignment() {
        let row = RowLayout::new().alignment(Alignment::Center);
        let mut children = vec![
            make_leaf(50.0, 20.0),
            make_leaf(50.0, 100.0),
        ];

        let (_, placements) = row.measure(
            &mut children,
            Constraints::UNBOUNDED,
        );

        // 矮的 item 在交叉轴居中
        assert_eq!(placements[0].position.y, (100.0 - 20.0) / 2.0);
        assert_eq!(placements[1].position.y, 0.0);
    }
}
