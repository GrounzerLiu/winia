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
pub mod direction;
pub mod components;
pub mod box_with_constraints;
pub mod lazy_column;
pub mod adaptive;
pub mod subcompose;
pub(crate) mod subcompose_probe;
pub mod node;
pub(crate) mod flex;
pub(crate) mod column;
mod row;
pub(crate) mod box_layout;
pub mod flow;

pub use constraints::Constraints;
// The layout primitives re-exported at the layer root: `Row`/`Column`/`Stack` are how a caller names
// the layout, not which file they live in.
pub use components::{Column, FlowColumn, FlowRow, Row, Spacer, Stack};
pub use box_with_constraints::{BoxWithConstraints, BoxWithConstraintsScope};
pub use lazy_column::{ItemHeightCache, LazyColumn, LazyListState, LazyRow};
pub use adaptive::{set_window_size, window_size, HeightSizeClass, WidthSizeClass};
pub use node::*;
pub use column::ColumnLayout;
pub use row::RowLayout;
pub use box_layout::BoxLayout;
pub use flow::{FlowRowLayout, FlowColumnLayout};

// re-export measure_node 供外部使用
pub(crate) use node::measure_node;
