//! Column 布局 — 垂直排列子节点（对齐 Compose Column）
//!
//! 主轴 = 垂直（高度），交叉轴 = 水平（宽度）
//!
//! 特性:
//! - per-child 对齐: `Modifier::align_self(Alignment::Center)` 覆盖 Column 默认对齐
//! - weight 权重: `Modifier::weight(2.0)` 按比例分配剩余高度
//! - spacing: 子节点间固定间距

use super::constraints::Constraints;
use super::node::*;
use super::node::measure_node;
use crate::modifier::ModifierElement;

/// Column 布局策略
#[derive(Debug, Clone)]
pub struct ColumnLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub spacing: f32,
    pub direction: LayoutDirection,
}

impl ColumnLayout {
    pub fn new() -> Self {
        ColumnLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for ColumnLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for ColumnLayout {
    fn measure(
        &self,
        children: &mut [LayoutNode],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let n = children.len();
        if n == 0 {
            return (Size::new(constraints.constrain_width(0.0), constraints.constrain_height(0.0)), Vec::new());
        }

        // ── 读取 per-child 数据 ──
        let weights: Vec<Option<f32>> = children.iter().map(|c| {
            c.modifier.elements().iter().find_map(|el| match el {
                ModifierElement::LayoutWeight { weight } => Some(*weight),
                _ => None,
            })
        }).collect();

        let aligns: Vec<Alignment> = children.iter().map(|c| {
            c.modifier.elements().iter().find_map(|el| match el {
                ModifierElement::AlignSelf { alignment } => Some(*alignment),
                _ => None,
            }).unwrap_or(self.alignment)
        }).collect();

        let total_spacing = self.spacing * (n as f32 - 1.0).max(0.0);

        // ── Phase 1: 测量无 weight 的子节点 ──
        let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
        let mut total_fixed_height: f32 = 0.0;
        let mut max_width: f32 = 0.0;
        let mut total_weight: f32 = 0.0;

        for (i, child) in children.iter_mut().enumerate() {
            if let Some(w) = weights[i] {
                total_weight += w;
                continue;
            }
            // 逐减间距：已测子节点数 × spacing
            let measured_count = child_sizes.iter().take(i).filter(|s| s.width > 0.0 || s.height > 0.0).count() as f32;
            let spacing_deduct = measured_count * self.spacing;
            let ch = constraints.max_height - total_fixed_height - spacing_deduct;
            let cc = Constraints {
                min_width: constraints.min_width,
                max_width: constraints.max_width,
                min_height: 0.0,
                max_height: ch.max(0.0),
            };
            let (size, _) = measure_node(child, cc);
            total_fixed_height += size.height;
            max_width = max_width.max(size.width);
            child_sizes[i] = size;
        }

        let remaining = if constraints.max_height.is_finite() {
            (constraints.max_height - total_fixed_height - total_spacing).max(0.0)
        } else {
            0.0
        };

        // ── Phase 2: 测量有 weight 的子节点 ──
        for (i, child) in children.iter_mut().enumerate() {
            if let Some(w) = weights[i] {
                let allocated = if total_weight > 0.0 { remaining * w / total_weight } else { 0.0 };
                let cc = Constraints {
                    max_width: constraints.max_width,
                    min_width: if self.alignment == Alignment::Stretch || aligns[i] == Alignment::Stretch {
                        constraints.min_width
                    } else {
                        0.0
                    },
                    min_height: allocated,
                    max_height: allocated,
                };
                let (size, _) = measure_node(child, cc);
                max_width = max_width.max(size.width);
                child_sizes[i] = size;
            }
        }

        // ── Phase 3: 确定最终尺寸和位置 ──
        let total_content_height: f32 = child_sizes.iter().map(|s| s.height).sum::<f32>() + total_spacing;

        let remaining_height = if constraints.max_height.is_finite() {
            (constraints.max_height - total_content_height).max(0.0)
        } else {
            0.0
        };
        let gap_count = if n > 1 { n - 1 } else { 0 };
        let (spacing_extra, leading_space) = compute_spacing(self.arrangement, remaining_height, gap_count);
        let effective_spacing = self.spacing + spacing_extra;

        let col_width = if self.alignment == Alignment::Stretch && constraints.max_width < f32::MAX {
            constraints.max_width
        } else {
            constraints.constrain_width(max_width)
        };

        let mut placements = Vec::with_capacity(n);
        let mut y_offset = leading_space;

        for (i, child_size) in child_sizes.iter().enumerate() {
            let align = aligns[i];
            let width = match align {
                Alignment::Stretch => col_width,
                _ => child_size.width,
            };
            let height = child_size.height;
            let x = match align {
                Alignment::Start => 0.0,
                Alignment::End => col_width - width,
                Alignment::Center => (col_width - width) / 2.0,
                Alignment::Stretch => 0.0,
            };

            placements.push(Placement {
                size: Size::new(width, height),
                position: Point::new(x, y_offset),
            });
            y_offset += height + effective_spacing;
        }

        // ── RTL 镜像：交叉轴 x → col_width - x - width ──
        if self.direction == LayoutDirection::Rtl {
            for p in &mut placements {
                p.position.x = col_width - p.position.x - p.size.width;
            }
        }

        let measured_height = match self.arrangement {
            Arrangement::SpaceBetween | Arrangement::SpaceAround | Arrangement::SpaceEvenly => {
                constraints.constrain_height(total_content_height + remaining_height)
            }
            _ => constraints.constrain_height(total_content_height),
        };
        (Size::new(col_width, measured_height), placements)
    }

    fn place(&self, children: &mut [LayoutNode], placements: &[Placement]) {
        for (child, placement) in children.iter_mut().zip(placements.iter()) {
            child.position = placement.position;
            child.measured_size = placement.size;
        }
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(width: f32, height: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
        node.measured_size = Size::new(width, height);
        node
    }

    #[test]
    fn test_column_simple() {
        let column = ColumnLayout::new();
        let mut children = vec![make_leaf(100.0, 20.0), make_leaf(80.0, 30.0), make_leaf(120.0, 10.0)];
        let (size, placements) = column.measure(&mut children, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(120.0, 60.0));
        assert_eq!(placements[0].position, Point::new(0.0, 0.0));
        assert_eq!(placements[1].position, Point::new(0.0, 20.0));
        assert_eq!(placements[2].position, Point::new(0.0, 50.0));
    }

    #[test]
    fn test_column_spacing() {
        let column = ColumnLayout::new().spacing(10.0);
        let mut children = vec![make_leaf(100.0, 30.0), make_leaf(100.0, 30.0)];
        let (size, placements) = column.measure(&mut children, Constraints::UNBOUNDED);
        assert_eq!(size.height, 70.0); // 30 + 10 + 30
        assert_eq!(placements[1].position.y, 40.0);
    }

    #[test]
    fn test_column_center_arrangement() {
        let column = ColumnLayout::new().arrangement(Arrangement::Center);
        let mut children = vec![make_leaf(100.0, 20.0), make_leaf(100.0, 10.0)];
        let (size, placements) = column.measure(&mut children, Constraints::new(0.0, f32::INFINITY, 0.0, 100.0));
        assert_eq!(size.height, 30.0);
        assert_eq!(placements[0].position.y, 35.0);
        assert_eq!(placements[1].position.y, 55.0);
    }

    #[test]
    fn test_column_end_alignment() {
        let column = ColumnLayout::new().alignment(Alignment::End);
        let mut children = vec![make_leaf(50.0, 20.0), make_leaf(100.0, 20.0)];
        let (size, placements) = column.measure(&mut children, Constraints::UNBOUNDED);
        assert_eq!(placements[0].position.x, 100.0 - 50.0);
        assert_eq!(placements[1].position.x, 0.0);
    }
}
