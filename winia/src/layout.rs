//! 布局系统 — MeasurePolicy 驱动的声明式布局
//!
//! 三阶段: Constraints → Measure → Place
//!
//! 布局原语:
//! - Column: 垂直排列
//! - Row: 水平排列
//! - Box: 层叠排列
//! - FlowRow / FlowColumn: 流式换行/换列（对标 Compose foundation）

pub mod axis;
pub mod constraints;
pub mod direction;
pub mod sizing;
pub use sizing::{Dimension, IntrinsicSize, SizeValue};
pub(crate) mod container;
pub mod components;
pub mod spacer;
pub mod grid;
pub mod staggered;
pub mod box_with_constraints;
/// The lazy lists — `LazyColumn` AND `LazyRow`, which share one axis-generic core.
pub mod lazy;
/// The module used to be named `lazy_column`, before `LazyRow` moved in beside it. Kept as an alias
/// so the old `winia::layout::lazy_column::…` paths keep resolving.
pub use lazy as lazy_column;
pub mod adaptive;
pub mod subcompose;
pub mod node;
pub mod relocation;

pub(crate) mod item_animation;
pub(crate) mod flex;
pub(crate) mod column;
mod row;
pub(crate) mod box_layout;
pub mod flow;

pub use axis::{Axis, HorizontalAxis, VerticalAxis};
pub use constraints::Constraints;
// The layout primitives re-exported at the layer root: `Row`/`Column`/`Stack` are how a caller names
// the layout, not which file they live in.
pub use column::Column;
pub use row::Row;
pub use box_layout::Stack;
pub use flow::{FlowColumn, FlowOverflow, FlowRow};
pub use spacer::Spacer;
pub use grid::{Grid, GridArea, GridFlow, GridLayout, GridTrackSpec};
pub use staggered::{assign_lanes, lane_anchors, LaneSlot};
pub use box_with_constraints::{BoxWithConstraints, BoxWithConstraintsScope};
pub use lazy::{
    GridCells, ItemHeightCache, LazyColumn, LazyHorizontalGrid, LazyList, LazyListState, LazyRow,
    LazyVerticalGrid,
};
pub use adaptive::{set_window_size, window_size, HeightSizeClass, WidthSizeClass};
pub use node::*;
pub use relocation::BringIntoViewRequester;
pub use column::ColumnLayout;
pub use row::RowLayout;
pub use box_layout::BoxLayout;
pub use flow::{FlowRowLayout, FlowColumnLayout};

// re-export measure_node 供外部使用
pub(crate) use node::measure_node;
