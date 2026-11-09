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
pub(crate) mod selection_handles;
pub mod sizing;
pub use sizing::{Dimension, IntrinsicSize, SizeValue};
pub mod box_with_constraints;
pub mod components;
pub(crate) mod container;
pub mod flexbox;
pub mod grid;
/// The lazy lists — `LazyColumn` AND `LazyRow`, which share one axis-generic core.
pub mod lazy;
pub mod spacer;
pub mod staggered;
/// The module used to be named `lazy_column`, before `LazyRow` moved in beside it. Kept as an alias
/// so the old `winia::layout::lazy_column::…` paths keep resolving.
pub use lazy as lazy_column;
pub mod adaptive;
pub mod custom;
pub mod node;
pub mod relocation;
pub mod subcompose;

pub(crate) mod box_layout;
pub(crate) mod column;
pub(crate) mod flex;
pub mod flow;
pub(crate) mod item_animation;
mod row;

pub use axis::{Axis, HorizontalAxis, VerticalAxis};
pub use constraints::Constraints;
// The layout primitives re-exported at the layer root: `Row`/`Column`/`Stack` are how a caller names
// the layout, not which file they live in.
pub use box_layout::Stack;
pub use column::Column;
pub use flow::{FlowColumn, FlowOverflow, FlowRow};
pub use row::Row;
pub use spacer::Spacer;
// Compose's `Layout`: a container with a caller-supplied measure policy.
pub use adaptive::{HeightSizeClass, WidthSizeClass, set_window_size, window_size};
pub use box_layout::BoxLayout;
pub use box_with_constraints::{BoxWithConstraints, BoxWithConstraintsScope};
pub use column::ColumnLayout;
pub use custom::Layout;
pub use flexbox::{
    FlexAlignContent, FlexAlignItems, FlexAlignSelf, FlexBasis, FlexBox, FlexBoxConfig,
    FlexBoxConfigInput, FlexBoxLayout, FlexDirection, FlexItemConfig, FlexJustifyContent, FlexWrap,
};
pub use flow::{FlowColumnLayout, FlowRowLayout};
pub use grid::{Grid, GridArea, GridFlow, GridLayout, GridTrackSpec};
pub use lazy::{
    GridCells, ItemHeightCache, LazyColumn, LazyHorizontalGrid, LazyHorizontalStaggeredGrid,
    LazyList, LazyListState, LazyRow, LazyStaggeredGrid, LazyVerticalGrid,
    LazyVerticalStaggeredGrid,
};
pub use node::*;
pub use relocation::BringIntoViewRequester;
pub use row::RowLayout;
pub use staggered::{LaneSlot, assign_lanes, lane_anchors};

// re-export measure_node 供外部使用
pub(crate) use node::measure_node;
