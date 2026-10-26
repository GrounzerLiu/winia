//! `Spacer` — a blank of a size the caller chooses, for explicit gaps.
//!
//! Compose's `Spacer` is a `Layout` with no children whose measure policy reads its own constraints
//! (`Spacer.kt:42-53`): a FIXED axis (min == max) takes that value, every other axis is 0. The size
//! therefore comes entirely from the modifier — `Spacer(Modifier.size(8.dp))`,
//! `Spacer(Modifier.height(8.dp))`, `Spacer(Modifier.weight(1f))` and `Spacer(Modifier.fillMaxHeight())`
//! are all the same component with a different chain.
//!
//! winia expresses that with an empty `BoxLayout`: with no children the box reports its own minimum,
//! which is exactly the fixed value a weight or a fill hands it.

use super::box_layout::BoxLayout;
use super::container::build_container;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

// ── Spacer ──

/// A blank of a fixed size — Compose's `Spacer(modifier)`.
///
/// Use it for an explicit gap between siblings (where `.spacing()` is not enough) or as a filler
/// inside a box. It paints nothing and composes no children.
pub struct Spacer {
    modifier: Modifier,
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new()
    }
}

impl Spacer {
    /// A spacer whose size is decided entirely by its modifier — Compose's `Spacer(modifier)`.
    ///
    /// `Spacer::new().modifier(Modifier::new().size(8.0, 8.0))` is the direct equivalent; a
    /// `layout_weight` in a Row/Column or a `fill_max_*` also works, because nothing else in the chain
    /// pins the other axis.
    pub fn new() -> Self {
        Spacer {
            modifier: Modifier::new(),
        }
    }

    /// A vertical gap: `h` high, width left to the caller (0 by default, as in Compose).
    pub fn vertical(h: f32) -> Self {
        Spacer::new().modifier(Modifier::new().height(h))
    }

    /// A horizontal gap: `w` wide, height left to the caller (0 by default, as in Compose).
    pub fn horizontal(w: f32) -> Self {
        Spacer::new().modifier(Modifier::new().width(w))
    }

    /// Appends a modifier. It runs AFTER the size this builder already set, so a `fill_max_*` or a
    /// `layout_weight` in it can still take the axis the builder left open.
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx) {
        // An empty container: BoxLayout has no children to measure, so it reports its own minimum —
        // which is the fixed value a size, a weight or a fill handed it.
        build_container(ctx, self.modifier, BoxLayout::new(), |_| {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::text::Text;
    use crate::layout::{Column, Constraints, Row};
    use crate::runtime::composer::Composer;
    use crate::unit::Size;

    fn row_constraints() -> Constraints {
        Constraints::new(0.0, 300.0, 0.0, 100.0)
    }

    /// The measured sizes of a Row's children; the row is a fixed 300x100 box.
    fn row_with(spacer: Spacer) -> Vec<Size> {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width().height(100.0))
                .build(ctx, |ctx| {
                    Text::new("a").modifier(Modifier::new().size(50.0, 20.0)).build(ctx);
                    spacer.build(ctx);
                    Text::new("b").modifier(Modifier::new().size(50.0, 20.0)).build(ctx);
                });
        }));
        composer.layout(row_constraints());
        let row = composer.layout_root().unwrap();
        row.children
            .iter()
            .map(|c| composer.arena_nodes()[*c].measured_size)
            .collect()
    }

    /// The measured size of one spacer alone inside a Column over a 300x100 root.
    fn alone(spacer: Spacer) -> Size {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            Column::new().build(ctx, |ctx| spacer.build(ctx));
        }));
        composer.layout(row_constraints());
        let root = composer.layout_root().unwrap();
        composer.arena_nodes()[root.children[0]].measured_size
    }

    #[test]
    fn a_vertical_spacer_is_a_zero_wide_gap() {
        assert_eq!(row_with(Spacer::vertical(10.0))[1], Size::new(0.0, 10.0));
    }

    #[test]
    fn a_horizontal_spacer_is_a_zero_high_gap() {
        assert_eq!(row_with(Spacer::horizontal(10.0))[1], Size::new(10.0, 0.0));
    }

    /// The regression this round fixed: the constructors used to bake a TIGHT 0 on the other axis
    /// (size(w, 0.0)), so a fill_max_* in the caller's modifier measured against a maximum of 0 and the
    /// spacer stayed 0 high. Compose's width(w) followed by fillMaxHeight() fills.
    #[test]
    fn a_spacer_fills_the_axis_its_constructor_left_open() {
        let sizes = row_with(Spacer::horizontal(10.0).modifier(Modifier::new().fill_max_height()));
        assert_eq!(sizes[1], Size::new(10.0, 100.0), "{sizes:?}");
    }

    #[test]
    fn a_weighted_spacer_takes_its_share() {
        let sizes = row_with(Spacer::new().modifier(Modifier::new().layout_weight(1.0)));
        assert_eq!(sizes[1], Size::new(200.0, 0.0), "{sizes:?}");
    }

    #[test]
    fn a_spacer_sized_only_by_its_modifier_takes_that_size() {
        let spacer = Spacer::new().modifier(Modifier::new().size(10.0, 10.0));
        assert_eq!(alone(spacer), Size::new(10.0, 10.0));
    }

    #[test]
    fn a_bare_spacer_takes_no_space() {
        assert_eq!(alone(Spacer::new()), Size::new(0.0, 0.0));
    }

    /// Chain order decides, like every other size step: a later size overrides the axis the
    /// constructor set and leaves the other one alone.
    #[test]
    fn a_later_size_overrides_only_the_axis_it_names() {
        let spacer = Spacer::vertical(8.0).modifier(Modifier::new().size(20.0, 20.0));
        assert_eq!(alone(spacer), Size::new(20.0, 8.0));
    }

    /// A DELIBERATE difference: Compose's Spacer reports 0 on an axis that is not FIXED, because its
    /// policy reads the maximum only when the minimum equals it (Spacer.kt:48-51). winia composes an
    /// empty box, and an empty box reports its own minimum, so width_in is honoured up to the minimum.
    /// Recorded here so nobody turns it into a special case.
    #[test]
    fn an_empty_spacer_reports_the_incoming_minimum() {
        let spacer = Spacer::new().modifier(Modifier::new().width_in(100.0, 200.0));
        assert_eq!(alone(spacer), Size::new(100.0, 0.0));
    }
}
