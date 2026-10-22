//! The scroll family: what a release turns into, the nested-scroll protocol, and the
//! rubber-band at the end of a container.
//!
//! These three modules used to sit at the crate root next to each other with no parent; grouping
//! them here gives the domain one entry point. The old public paths (`winia::fling`,
//! `winia::nested_scroll`, `winia::overscroll`) are re-exported from the crate root.

pub mod fling;
pub mod nested;
pub mod overscroll;

pub mod state;
