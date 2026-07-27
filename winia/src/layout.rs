//! 布局系统 — MeasurePolicy 驱动的声明式布局
//!
//! 三阶段: Constraints → Measure → Place
//!
//! 布局原语:
//! - Column: 垂直排列
//! - Row: 水平排列
//! - Box: 层叠排列

pub mod constraints;
pub mod node;
pub(crate) mod flex;
pub(crate) mod column;
mod row;
pub(crate) mod box_layout;

pub use constraints::Constraints;
pub use node::*;
pub use column::ColumnLayout;
pub use row::RowLayout;
pub use box_layout::BoxLayout;

// re-export measure_node 供外部使用
pub(crate) use node::measure_node;
