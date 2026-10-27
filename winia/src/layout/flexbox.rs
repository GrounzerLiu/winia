//! FlexBox — CSS flexbox, matching Compose's FlexBox / FlexBoxConfig (foundation-layout 1.13).
//!
//! Compose's FlexBox (FlexBox.kt:133) is a CSS-flex layout: a main axis with grow / shrink / basis,
//! line breaking (FlexWrap), justify-content / align-items / align-self / align-content and a
//! per-item order. It is a DIFFERENT algorithm from Row/Column, which follow Compose's
//! RowColumnMeasurePolicy (weights, no wrapping, no basis), so it lives in its own module instead of
//! being another arm of flex.rs.
//!
//! Where each phase comes from:
//! - items and their basis: createFlexItem (FlexBox.kt:354-406)
//! - line breaking: buildFlexLines (FlexBox.kt:417-496)
//! - flexible lengths: resolveFlexibleLengths (FlexBox.kt:529-676)
//! - line cross sizes: calculateLineCrossAxisSize (FlexBox.kt:916-968)
//! - align-content stretch: applyAlignContentStretch (FlexBox.kt:677-713)
//! - measuring items: measureFlexItems (FlexBox.kt:714-770), measureItem (FlexBox.kt:969-1010)
//! - placing: calculateLineCrossPositions (FlexBox.kt:771-810), positionItemsOnMainAxis
//!   (FlexBox.kt:811-871), calculateItemCrossPosition (FlexBox.kt:872-915), placeFlexItems
//!   (FlexBox.kt:313-353)
//! - container size: measureFlexBox (FlexBox.kt:192-311)
//!
//! What is deliberately not modelled:
//! - Compose resolves the config at MEASURE time through a scope that exposes the incoming
//!   constraints and the density, so a config can be responsive. winia cannot hand a Rust closure a
//!   borrowed scope the way a Kotlin fun interface does, so FlexBox::config_for takes a
//!   FlexBoxConfigInput (constraints + density) BY VALUE — the same information, one copy per
//!   measure.
//! - There is no FlexBoxScope: flex(..) is a plain Modifier, so a config placed on a child of
//!   another container is silently ignored (the same convention layout_weight and align_self follow).
//! - Modifier.weight is not mapped to grow. Compose maps a main-axis fill fraction onto grow when
//!   the grow factor is unset (FlexBox.kt:366-376); winia's weight means "share the leftover", which
//!   is not CSS grow, so mapping it would silently change what weight means inside a FlexBox. Write
//!   grow explicitly instead.
//! - Baseline alignment uses each child's first baseline (AlignmentLine::FIRST_BASELINE) unless the
//!   child sets a line with Modifier::align_by_baseline.
//!
//! Unlike most containers this one is NOT restartable-free: every child reads its flex config from
//! its own modifier chain, so the container declares only its own parameters and the children declare
//! theirs.

mod builder;
pub mod config;
mod intrinsic;
mod policy;

pub use builder::FlexBox;
pub use config::{
    FlexAlignContent, FlexAlignItems, FlexAlignSelf, FlexBasis, FlexBoxConfig, FlexBoxConfigInput,
    FlexDirection, FlexItemConfig, FlexJustifyContent, FlexWrap,
};
pub use policy::FlexBoxLayout;

#[cfg(test)]
mod tests;
