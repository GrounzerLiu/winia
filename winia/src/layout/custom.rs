//! A container whose measure policy the caller supplies — Compose's `Layout`
//! (`androidx.compose.ui.layout.Layout`).
//!
//! Every built-in container (Row, Column, Stack, Flow, Grid, FlexBox) is this plus a policy, so a
//! caller who needs something the library does not ship writes a `MeasurePolicy` and builds it here
//! instead of assembling the composition glue by hand.
//!
//! Two rules come with it, and both are the same rules the built-in containers follow:
//!
//! - The content is a pass-through group: declare nothing and it Skips, so a state read that decides
//!   the CONTENT must happen inside the content closure. A policy that reads state during `measure`
//!   is fine — a measure-time read is a LAYOUT dependency, and a write re-measures.
//! - The policy identifies itself; a different policy on the same call site replaces it, because the
//!   group re-enters whenever the caller does.

use super::container::build_container;
use super::node::MeasurePolicy;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

/// A container with a caller-supplied measure policy — Compose's `Layout`.
///
/// ```ignore
/// Layout::new(StackPolicy)
///     .modifier(Modifier::new().fill_max_width())
///     .build(ctx, |ctx| { Text::new("inside").build(ctx); });
/// ```
pub struct Layout<P> {
    policy: P,
    modifier: Modifier,
}

impl<P: MeasurePolicy + 'static> Layout<P> {
    /// A container measured by `policy`.
    pub fn new(policy: P) -> Self {
        Layout {
            policy,
            modifier: Modifier::new(),
        }
    }

    /// The container's own modifier. The caller's chain comes last, as everywhere else.
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Start the container and compose its content into it.
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        build_container(ctx, self.modifier, self.policy, content);
    }
}

/// A policy that lays every child out at the origin and reports the widest and tallest child — the
/// smallest complete example of what a custom container has to do.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::node::{LayoutNode, MeasurePolicy, Placement};
    use crate::layout::Constraints;
    use crate::modifier::Modifier;
    use crate::runtime::composer::Composer;
    use crate::unit::{Offset, Size};

    #[derive(Debug)]
    struct TopLeftStack;

    impl MeasurePolicy for TopLeftStack {
        fn measure(
            &self,
            nodes: &mut Vec<LayoutNode>,
            policies: &[Box<dyn MeasurePolicy>],
            children: &[usize],
            constraints: Constraints,
        ) -> (Size, Vec<Placement>) {
            let mut size = Size::ZERO;
            let mut placements = Vec::with_capacity(children.len());
            for &child in children {
                let (child_size, _) = crate::layout::node::measure_node(
                    nodes,
                    policies,
                    child,
                    Constraints::new(0.0, constraints.max_width, 0.0, constraints.max_height),
                );
                size = Size::new(size.width.max(child_size.width), size.height.max(child_size.height));
                placements.push(Placement {
                    size: child_size,
                    position: Offset::ZERO,
                });
            }
            (size, placements)
        }

        fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
            for (index, &child) in children.iter().enumerate() {
                if let Some(placement) = placements.get(index) {
                    nodes[child].position = placement.position;
                    nodes[child].measured_size = placement.size;
                }
            }
        }
    }

    #[test]
    fn a_custom_policy_measures_and_places_the_content_it_is_given() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            Layout::new(TopLeftStack)
                .modifier(Modifier::new().test_tag("custom"))
                .build(ctx, |ctx| {
                    crate::components::Text::new("wide content")
                        .font_size(20.0)
                        .build(ctx);
                    crate::components::Text::new("x")
                        .font_size(10.0)
                        .build(ctx);
                });
        }));
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        let root = composer.layout_root().expect("a root");
        let children = root.children.clone();
        assert_eq!(children.len(), 2, "both children are laid out");
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[children[0]].position, Offset::ZERO, "every child sits at the origin");
        assert_eq!(nodes[children[1]].position, Offset::ZERO);
        assert!(
            root.measured_size.width >= nodes[children[0]].measured_size.width,
            "the container is as wide as its widest child: {:?}",
            root.measured_size
        );
    }
}
