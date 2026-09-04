//! 布局系统 — MeasurePolicy 驱动的声明式布局
//!
//! 三阶段: Constraints → Measure → Place
//!
//! 布局原语:
//! - Column: 垂直排列
//! - Row: 水平排列
//! - Box: 层叠排列
//! - FlowRow / FlowColumn: 流式换行/换列（对标 Compose foundation）

pub mod constraints;
pub mod node;
pub(crate) mod flex;
pub(crate) mod column;
mod row;
pub(crate) mod box_layout;
pub mod flow;

pub use constraints::Constraints;
pub use node::*;
pub use column::ColumnLayout;
pub use row::RowLayout;
pub use box_layout::BoxLayout;
pub use flow::{FlowRowLayout, FlowColumnLayout};

// re-export measure_node 供外部使用
pub(crate) use node::measure_node;
