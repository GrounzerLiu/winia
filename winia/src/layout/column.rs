//! Column 布局 — 垂直排列子节点
//!
//! 主轴 = 垂直方向（高度），交叉轴 = 水平方向（宽度）

use super::constraints::Constraints;
use super::node::*;
use crate::modifier::ModifierElement;

/// Column 布局策略
#[derive(Debug, Clone)]
pub struct ColumnLayout {
    /// 主轴排列方式
    pub arrangement: Arrangement,
    /// 交叉轴对齐方式
    pub alignment: Alignment,
}

impl ColumnLayout {
    pub fn new() -> Self {
        ColumnLayout {
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

impl Default for ColumnLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurePolicy for ColumnLayout {
    fn measure(
        &self,
        children: &mut [LayoutNode],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let mut total_height: f32 = 0.0;
        let mut max_width: f32 = 0.0;
        let mut placements: Vec<Placement> = Vec::with_capacity(children.len());

        // 第一次遍历：测量每个子节点
        let mut child_sizes: Vec<Size> = Vec::with_capacity(children.len());
        for child in children.iter_mut() {
            let child_constraints = Constraints {
                min_width: constraints.min_width,
                max_width: constraints.max_width,
                min_height: 0.0,
                max_height: (constraints.max_height - total_height).max(0.0),
            };
            let (child_size, _) = measure_node(child, child_constraints);
            total_height += child_size.height;
            max_width = max_width.max(child_size.width);
            child_sizes.push(child_size);
        }

        // 主轴剩余空间
        let remaining_height = (constraints.max_height - total_height).max(0.0);
        let gap_count = if children.len() > 1 { children.len() - 1 } else { 0 };

        // 计算每个子节点的最终高度（考虑 Stretch 对齐）
        let final_heights: Vec<f32> = if self.alignment == Alignment::Stretch {
            child_sizes
                .iter()
                .map(|s| s.height + remaining_height / children.len() as f32)
                .collect()
        } else {
            child_sizes.iter().map(|s| s.height).collect()
        };

        // 计算主轴间距
        let (spacing, leading_space) = compute_spacing(
            self.arrangement,
            remaining_height,
            gap_count,
            &final_heights,
        );

        // 第二次遍历：确定每个子节点的放置
        let mut y_offset = leading_space;
        for (i, (_child, child_size)) in children.iter_mut().zip(child_sizes.iter()).enumerate() {
            let height = if self.alignment == Alignment::Stretch {
                final_heights[i]
            } else {
                child_size.height
            };

            // 交叉轴对齐
            let x = match self.alignment {
                Alignment::Start => 0.0,
                Alignment::End => max_width - child_size.width,
                Alignment::Center => (max_width - child_size.width) / 2.0,
                Alignment::Stretch => 0.0,
            };

            let width = if self.alignment == Alignment::Stretch {
                max_width
            } else {
                child_size.width
            };

            placements.push(Placement {
                size: Size::new(width, height),
                position: Point::new(x, y_offset),
            });

            y_offset += height + spacing;
        }

        let measured_height = constraints.constrain_height(
            final_heights.iter().sum::<f32>() + spacing * gap_count as f32,
        );
        let measured_width = constraints.constrain_width(max_width);

        (Size::new(measured_width, measured_height), placements)
    }

    fn place(&self, children: &mut [LayoutNode], placements: &[Placement]) {
        for (child, placement) in children.iter_mut().zip(placements.iter()) {
            child.position = placement.position;
            child.measured_size = placement.size;
        }
    }
}

// ── 辅助函数 ──

/// 计算布局方案中的 spacing 和 leading space
fn compute_spacing(
    arrangement: Arrangement,
    remaining: f32,
    gap_count: usize,
    _sizes: &[f32],
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
            let total_gaps = gap_count + 2; // 前后也有间距
            if total_gaps > 0 {
                let space = remaining / total_gaps as f32;
                (space, space)
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}

/// 递归测量节点（处理 modifier 中的约束并调用子节点的 measure_policy）
pub(crate) fn measure_node(
    node: &mut LayoutNode,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // 应用 modifier 中的 Layout 约束
    let mut inner_constraints = constraints;

    for el in node.modifier.elements() {
        match el {
            ModifierElement::Size { width, height } => {
                use crate::modifier::Dimension;
                if let Dimension::Fixed(w) = width {
                    inner_constraints = inner_constraints.tighten_width(*w);
                }
                if let Dimension::Fixed(h) = height {
                    inner_constraints = inner_constraints.tighten_height(*h);
                }
            }
            ModifierElement::Padding { all } => {
                let p = *all;
                inner_constraints = inner_constraints.offset(p * 2.0, p * 2.0);
            }
            ModifierElement::PaddingHorizontal { value } => {
                inner_constraints = inner_constraints.offset(value * 2.0, 0.0);
            }
            ModifierElement::PaddingVertical { value } => {
                inner_constraints = inner_constraints.offset(0.0, value * 2.0);
            }
            ModifierElement::FillMaxWidth => {
                inner_constraints.min_width = inner_constraints.max_width;
            }
            ModifierElement::FillMaxHeight => {
                inner_constraints.min_height = inner_constraints.max_height;
            }
            ModifierElement::FillMaxSize => {
                inner_constraints.min_width = inner_constraints.max_width;
                inner_constraints.min_height = inner_constraints.max_height;
            }
            _ => {}
        }
    }

    // 检查是否包含 scroll 修饰符——给子节点无限约束
    let node_is_scroll_v = node.modifier.elements().iter().any(|el| matches!(el, ModifierElement::VerticalScroll { .. }));
    let node_is_scroll_h = node.modifier.elements().iter().any(|el| matches!(el, ModifierElement::HorizontalScroll { .. }));
    if node_is_scroll_v {
        inner_constraints.max_height = f32::MAX;
    }
    if node_is_scroll_h {
        inner_constraints.max_width = f32::MAX;
    }

    // 实际测量
    if let Some(ref policy) = node.measure_policy {
        let (size, placements) = {
            let children = &mut node.children;
            policy.measure(children, inner_constraints)
        };
        // apply positions
        policy.place(&mut node.children, &placements);
        node.measured_size = size;
        (size, placements)
    } else {
        // 叶子节点
        // 检查是否有 TextContent（文字节点需要根据字体测量尺寸）
        let mut text_content: Option<(&str, f32)> = None;
        for el in node.modifier.elements() {
            if let ModifierElement::TextContent { content, font_size, .. } = el {
                text_content = Some((content.as_str(), *font_size));
                break;
            }
        }

        let (width, height) = if let Some((content, font_size)) = text_content {
            // 用 Skia Paragraph 测量文字尺寸
            let max_w = if inner_constraints.has_fixed_width() {
                inner_constraints.max_width
            } else {
                f32::MAX
            };
            measure_text_size(content, font_size, max_w)
        } else {
            // 普通叶子节点
            let w = inner_constraints.constrain_width(
                if inner_constraints.has_fixed_width() {
                    inner_constraints.min_width
                } else {
                    0.0
                },
            );
            let h = inner_constraints.constrain_height(
                if inner_constraints.has_fixed_height() {
                    inner_constraints.min_height
                } else {
                    0.0
                },
            );
            (w, h)
        };

        node.measured_size = Size::new(width, height);
        (node.measured_size, Vec::new())
    }
}

/// 使用 Skia Paragraph 测量文本的尺寸
fn measure_text_size(text: &str, font_size: f32, _max_width: f32) -> (f32, f32) {
    use skia_safe::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle};
    let para_style = ParagraphStyle::new();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    let mut fc = FontCollection::new();
    fc.set_default_font_manager(skia_safe::FontMgr::default(), None);
    let mut builder = ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(text);
    let mut para = builder.build();
    // 先 layout 到很大宽度（避免换行），再用 intrinsic width 确定实际宽度
    para.layout(10000.0);
    (para.max_intrinsic_width().ceil(), para.height().ceil())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(width: f32, height: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(
            Modifier::new().size(width, height),
        );
        node.measured_size = Size::new(width, height); // 预设叶子尺寸
        node
    }

    #[test]
    fn test_column_simple() {
        let column = ColumnLayout::new();
        let mut children = vec![
            make_leaf(100.0, 20.0),
            make_leaf(80.0, 30.0),
            make_leaf(120.0, 10.0),
        ];

        let (size, placements) = column.measure(
            &mut children,
            Constraints::UNBOUNDED,
        );

        // 总高度 = 20+30+10 = 60, 最大宽度 = 120
        assert_eq!(size, Size::new(120.0, 60.0));
        assert_eq!(placements.len(), 3);
        assert_eq!(placements[0].position, Point::new(0.0, 0.0));
        assert_eq!(placements[1].position, Point::new(0.0, 20.0));
        assert_eq!(placements[2].position, Point::new(0.0, 50.0));
    }

    #[test]
    fn test_column_center_arrangement() {
        let column = ColumnLayout::new().arrangement(Arrangement::Center);
        let mut children = vec![
            make_leaf(100.0, 20.0),
            make_leaf(100.0, 10.0),
        ];

        let (size, placements) = column.measure(
            &mut children,
            Constraints::new(0.0, f32::INFINITY, 0.0, 100.0),
        );

        // 总高度 30, 剩余 70, center → leading = 35
        assert_eq!(size.height, 30.0);
        assert_eq!(placements[0].position.y, 35.0);
        assert_eq!(placements[1].position.y, 55.0);
    }

    #[test]
    fn test_column_end_alignment() {
        let column = ColumnLayout::new().alignment(Alignment::End);
        let mut children = vec![
            make_leaf(50.0, 20.0),
            make_leaf(100.0, 20.0),
        ];

        let (size, placements) = column.measure(
            &mut children,
            Constraints::UNBOUNDED,
        );

        // 小宽度的 item 应该靠右
        let max_w = 100.0;
        assert_eq!(placements[0].position.x, max_w - 50.0);
        assert_eq!(placements[1].position.x, 0.0); // 最宽的 = 100, x = max-100 = 0
    }
}
