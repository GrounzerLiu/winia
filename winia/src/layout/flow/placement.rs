//! Per-item placement animation shared by FlowRow and FlowColumn.
//!
//! The layout pass knows where a child landed, but it does not know where the child was painted
//! on the previous pass. This table keeps the last target rectangle by the child's stable slot
//! key. When a reflow moves a child, the old visual position becomes a paint offset and springs
//! back to zero; layout itself is not interrupted or measured again.

use crate::layout::node::LayoutNode;
use crate::layout::Placement;
use crate::unit::Offset;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One entry per child that carries a placement-animation marker.
struct PlacementEntry {
    /// Paint offset applied around the node while the layout settles.
    offset: crate::runtime::state::State<Offset>,
    /// Position at the previous layout pass, in the container's coordinate space.
    last_position: Offset,
    /// The layout generation that last saw this key.
    last_frame: u64,
}

#[derive(Default)]
struct PlacementState {
    frame: u64,
    items: HashMap<u64, PlacementEntry>,
}

/// A cloneable table of per-child placement animation states.
#[derive(Clone, Default)]
pub(crate) struct PlacementAnimations {
    /// Shared across clones, allocated only when a Flow actually enables placement animation; a
    /// default table costs no Mutex. The Arc around OnceLock is load-bearing: a clone must see
    /// the same initialized state, otherwise every frame would start from an empty table.
    inner: Arc<std::sync::OnceLock<Arc<Mutex<PlacementState>>>>,
}

impl std::fmt::Debug for PlacementAnimations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlacementAnimations")
    }
}

fn subtree_has_animate_item(nodes: &[LayoutNode], index: usize) -> bool {
    nodes[index].modifier.has_animate_item()
        || nodes[index]
            .children
            .iter()
            .any(|&child| subtree_has_animate_item(nodes, child))
}

impl PlacementAnimations {
    fn state(&self) -> &Arc<Mutex<PlacementState>> {
        self.inner
            .get_or_init(|| Arc::new(Mutex::new(PlacementState::default())))
    }

    /// Record the new placements and turn every moved child into a spring from its old visual
    /// position to its new one.
    ///
    /// The first time a key is seen it starts at the new position. A child that was absent on the
    /// previous layout pass also resets instead of springing from a stale slot.
    pub(crate) fn place(
        &self,
        nodes: &mut [LayoutNode],
        children: &[usize],
        placements: &[Placement],
    ) {
        let mut state = self.state().lock().unwrap_or_else(|e| e.into_inner());
        state.frame += 1;
        let frame = state.frame;

        for (index, &child) in children.iter().enumerate() {
            nodes[child].placement_anim = None;
            if !subtree_has_animate_item(nodes, child) {
                continue;
            }

            let key = if nodes[child].slot_key != 0 {
                nodes[child].slot_key
            } else {
                nodes[child].id
            };
            let new_position = placements[index].position;
            let entry = state.items.entry(key).or_insert_with(|| PlacementEntry {
                offset: crate::runtime::state::State::new(Offset::ZERO),
                last_position: new_position,
                last_frame: frame,
            });

            let previous_visual = entry.last_position + entry.offset.peek();
            let delta = previous_visual - new_position;
            let moved = delta.x.abs() > 0.01 || delta.y.abs() > 0.01;

            if entry.last_frame + 1 == frame && moved {
                crate::animation::cancel_animation(&entry.offset);
                entry.offset.set(delta);
                crate::animation::push_animatable_restart(
                    entry.offset.clone(),
                    Offset::ZERO,
                    crate::animation::AnimationSpec::Spring(
                        crate::animation::SpringSpec::default(),
                    ),
                );
            } else if entry.last_frame + 1 != frame {
                // Away for a frame (or new): do not spring from a stale visual position.
                entry.offset.set(Offset::ZERO);
            }

            entry.last_position = new_position;
            entry.last_frame = frame;
            nodes[child].placement_anim = Some(entry.offset.clone());
        }

        // A removed child's entry cannot animate again; keep the table bounded without making a
        // returning key look like it was present on the previous frame.
        if state.frame % 120 == 0 {
            state
                .items
                .retain(|_, entry| frame.saturating_sub(entry.last_frame) <= 120);
        }
    }
}
