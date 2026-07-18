//! 布局约束模型 — 类似 Jetpack Compose 的 Constraints
//!
//! Constraints 定义了父节点给予子节点的可用空间范围。
//! 子节点必须遵守这些约束（不能超出），但可以在范围内自由选择尺寸。

/// 布局约束：父节点告诉子节点的可用空间范围
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Constraints {
    /// 无限制约束（子节点自由决定尺寸）
    pub const UNBOUNDED: Constraints = Constraints {
        min_width: 0.0,
        max_width: f32::INFINITY,
        min_height: 0.0,
        max_height: f32::INFINITY,
    };

    /// 创建固定尺寸的约束
    pub fn fixed(width: f32, height: f32) -> Self {
        Constraints {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
        }
    }

    /// 创建指定范围的约束
    pub fn new(min_width: f32, max_width: f32, min_height: f32, max_height: f32) -> Self {
        Constraints {
            min_width,
            max_width,
            min_height,
            max_height,
        }
    }

    /// 约束一个尺寸值到合法范围内
    pub fn constrain_width(&self, width: f32) -> f32 {
        width.clamp(self.min_width, self.max_width)
    }

    pub fn constrain_height(&self, height: f32) -> f32 {
        height.clamp(self.min_height, self.max_height)
    }

    /// 是否在宽度上有确定的值（min == max）
    pub fn has_fixed_width(&self) -> bool {
        self.min_width == self.max_width
    }

    /// 是否在高度上有确定的值（min == max）
    pub fn has_fixed_height(&self) -> bool {
        self.min_height == self.max_height
    }

    /// 宽松化：将 min 设为 0，保留 max
    pub fn loosen(&self) -> Constraints {
        Constraints {
            min_width: 0.0,
            max_width: self.max_width,
            min_height: 0.0,
            max_height: self.max_height,
        }
    }

    /// 收紧宽度
    pub fn tighten_width(&self, width: f32) -> Constraints {
        Constraints {
            min_width: width,
            max_width: width,
            min_height: self.min_height,
            max_height: self.max_height,
        }
    }

    /// 收紧高度
    pub fn tighten_height(&self, height: f32) -> Constraints {
        Constraints {
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: height,
            max_height: height,
        }
    }

    /// 偏移：减去已使用的空间（用于测量剩余子节点）
    pub fn offset(&self, dx: f32, dy: f32) -> Constraints {
        Constraints {
            min_width: (self.min_width - dx).max(0.0),
            max_width: (self.max_width - dx).max(0.0),
            min_height: (self.min_height - dy).max(0.0),
            max_height: (self.max_height - dy).max(0.0),
        }
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Constraints::UNBOUNDED
    }
}
