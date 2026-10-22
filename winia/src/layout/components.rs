//! Compatibility facade for the container builders.
//!
//! The builders now live next to their measure policies (`column.rs`, `row.rs`,
//! `box_layout.rs`, `flow.rs`, `spacer.rs`); this module keeps the historical
//! `layout::components::*` paths working, including `build_container` for the few components
//! that assemble a plain container themselves.

pub use super::box_layout::Stack;
pub use super::column::Column;
pub use super::flow::{FlowColumn, FlowRow};
pub use super::row::Row;
pub use super::spacer::Spacer;

pub(crate) use super::container::build_container;
