//! Row 布局 — 水平排列子节点（对齐 Compose Row）
//!
//! 主轴 = 水平（宽度），交叉轴 = 垂直（高度）
//!
//! 特性:
//! - per-child 对齐: `Modifier::align_self(Alignment::Center)` 覆盖 Row 默认对齐
//! - weight 权重: `Modifier::weight(2.0)` 按比例分配剩余宽度
//! - spacing: 子节点间固定间距

use super::constraints::Constraints;
use super::node::*;
use super::node::measure_node;
use crate::modifier::ModifierElement;

/// Row 布局策略
#[derive(Debug, Clone)]
pub struct RowLayout {
    /// 主轴排列方式（水平）
    pub arrangement: Arrangement,
    /// 默认交叉轴对齐（垂直），子节点可通过 AlignSelf 覆盖
    pub alignment: Alignment,
    /// 子节点间距
    pub spacing: f32,
}

impl RowLayout {
    pub fn new() -> Self {
        RowLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
}

impl Default for RowLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for RowLayout {
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

        // 计算间距总量
        let total_spacing = self.spacing * (n as f32 - 1.0).max(0.0);

        // ── Phase 1: 测量无 weight 的子节点 ──
        let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
        let mut total_fixed_width: f32 = 0.0;
        let mut max_height: f32 = 0.0;
        let mut total_weight: f32 = 0.0;

        for (i, child) in children.iter_mut().enumerate() {
            if let Some(w) = weights[i] {
                total_weight += w;
                continue; // 稍后处理
            }
            let cw = constraints.max_width - total_fixed_width - total_spacing;
            let cc = Constraints {
                min_width: 0.0,
                max_width: cw.max(0.0),
                min_height: constraints.min_height,
                max_height: constraints.max_height,
            };
            let (size, _) = measure_node(child, cc);
            total_fixed_width += size.width;
            max_height = max_height.max(size.height);
            child_sizes[i] = size;
        }

        // 计算剩余宽度给 weight 子节点
        let remaining = if constraints.max_width.is_finite() {
            (constraints.max_width - total_fixed_width - total_spacing).max(0.0)
        } else {
            0.0
        };

        // ── Phase 2: 测量有 weight 的子节点 ──
        for (i, child) in children.iter_mut().enumerate() {
            if let Some(w) = weights[i] {
                let allocated = if total_weight > 0.0 { remaining * w / total_weight } else { 0.0 };
                let cc = Constraints {
                    min_width: allocated,
                    max_width: allocated,
                    max_height: constraints.max_height,
                    min_height: if self.alignment == Alignment::Stretch || aligns[i] == Alignment::Stretch {
                        constraints.min_height
                    } else {
                        0.0
                    },
                };
                let (size, _) = measure_node(child, cc);
                max_height = max_height.max(size.height);
                child_sizes[i] = size;
            }
        }

        // ── Phase 3: 确定最终尺寸和位置 ──
        let total_content_width: f32 = child_sizes.iter().map(|s| s.width).sum::<f32>() + total_spacing;

        // 主轴间距计算
        let remaining_width = if constraints.max_width.is_finite() {
            (constraints.max_width - total_content_width).max(0.0)
        } else {
            0.0
        };
        let gap_count = if n > 1 { n - 1 } else { 0 };
        let (spacing_extra, leading_space) = compute_spacing(self.arrangement, remaining_width, gap_count);
        let effective_spacing = self.spacing + spacing_extra;

        // 交叉轴高度
        let row_height = if self.alignment == Alignment::Stretch && constraints.max_height < f32::MAX {
            constraints.max_height
        } else {
            constraints.constrain_height(max_height)
        };

        let mut placements = Vec::with_capacity(n);
        let mut x_offset = leading_space;

        for (i, child_size) in child_sizes.iter().enumerate() {
            let align = aligns[i];
            let height = match align {
                Alignment::Stretch => row_height,
                _ => child_size.height,
            };
            let width = child_size.width;
            let y = match align {
                Alignment::Start => 0.0,
                Alignment::End => row_height - height,
                Alignment::Center => (row_height - height) / 2.0,
                Alignment::Stretch => 0.0,
            };

            placements.push(Placement {
                size: Size::new(width, height),
                position: Point::new(x_offset, y),
            });
            x_offset += width + effective_spacing;
        }

        // ── RTL 镜像：x → row_width - x - width ──
        if crate::ui::theme::WiniaTheme::direction() == crate::layout::LayoutDirection::Rtl {
            let row_w = constraints.constrain_width(total_content_width + remaining_width);
            for p in &mut placements {
                p.position.x = row_w - p.position.x - p.size.width;
            }
            // leading_space 已被镜像吸收，effective_spacing 同理
        }

        let measured_width = match self.arrangement {
            Arrangement::SpaceBetween | Arrangement::SpaceAround | Arrangement::SpaceEvenly => {
                constraints.constrain_width(total_content_width + remaining_width)
            }
            _ => constraints.constrain_width(total_content_width),
        };
        (Size::new(measured_width, row_height), placements)
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

    fn make_leaf(w: f32, h: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(Modifier::new().size(w, h));
        node.measured_size = Size::new(w, h);
        node
    }

    #[test]
    fn test_row_simple() {
        let row = RowLayout::new();
        let mut children = vec![make_leaf(20.0, 100.0), make_leaf(30.0, 80.0), make_leaf(10.0, 120.0)];
        let (size, placements) = row.measure(&mut children, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(60.0, 120.0));
        assert_eq!(placements[0].position, Point::new(0.0, 0.0));
        assert_eq!(placements[1].position, Point::new(20.0, 0.0));
        assert_eq!(placements[2].position, Point::new(50.0, 0.0));
    }

    #[test]
    fn test_row_spacing() {
        let row = RowLayout::new().spacing(10.0);
        let mut children = vec![make_leaf(50.0, 30.0), make_leaf(50.0, 30.0)];
        let (size, placements) = row.measure(&mut children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 110.0); // 50 + 10 + 50
        assert_eq!(placements[1].position.x, 60.0);
    }

    #[test]
    fn test_row_space_between() {
        let row = RowLayout::new().arrangement(Arrangement::SpaceBetween);
        let mut children = vec![make_leaf(20.0, 50.0), make_leaf(20.0, 50.0)];
        let (_size, placements) = row.measure(&mut children, Constraints::new(0.0, 200.0, 0.0, f32::INFINITY));
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 20.0 + 160.0);
    }

    #[test]
    fn test_row_center_alignment() {
        let row = RowLayout::new().alignment(Alignment::Center);
        let mut children = vec![make_leaf(50.0, 20.0), make_leaf(50.0, 100.0)];
        let (_, placements) = row.measure(&mut children, Constraints::UNBOUNDED);
        assert_eq!(placements[0].position.y, (100.0 - 20.0) / 2.0);
        assert_eq!(placements[1].position.y, 0.0);
    }
}
