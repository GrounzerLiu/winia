//! The shared per-item animation runtime for lazy lists and flow layouts.
//!
//! The layout pass knows an item's current slot but not where it was painted on the previous pass.
//! An `ItemAnimationHost` keeps that by stable item key and writes one `ItemPaint` onto the item
//! root; the renderer applies it around the whole subtree. Lazy and Flow share this host, so the
//! placement and fade semantics live in one place.
//!
//! Removed items are retained as a Skia picture: the last rendered frame of an item with a
//! `fade_out` curve is captured before it leaves the tree, then the exit layer draws that picture
//! with a fading alpha. This keeps enter/exit independent of whether the item's layout node still
//! exists.

use crate::animation::item::ItemAnimationSpec;
use crate::layout::node::LayoutNode;
use crate::unit::{Offset, Size};
use skia_safe::Picture;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// What the renderer applies around one item subtree.
#[derive(Clone)]
pub(crate) struct ItemPaint {
    /// Paint offset from the old slot towards the new one.
    pub(crate) offset: crate::runtime::state::State<Offset>,
    /// Opacity of the whole item.
    pub(crate) alpha: crate::runtime::state::State<f32>,
    /// Stable item key, used by the exit picture store.
    pub(crate) key: u64,
    /// The fade-out curve; `None` means the item is not captured for an exit.
    pub(crate) fade_out: Option<crate::animation::AnimationSpec>,
    /// Shared picture/exits store for the container's host.
    pub(crate) exit_store: Arc<Mutex<ExitStore>>,
}

impl std::fmt::Debug for ItemPaint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ItemPaint")
    }
}

/// The last rendered picture of an item, in the owning container's paint coordinates.
pub(crate) struct CapturedItem {
    pub(crate) picture: Picture,
    pub(crate) position: Offset,
    pub(crate) size: Size,
}

/// A removed item still playing its fade-out.
pub(crate) struct ExitedItem {
    pub(crate) picture: Picture,
    pub(crate) position: Offset,
    pub(crate) alpha: crate::runtime::state::State<f32>,
}

/// Pictures captured for live items plus exits currently on screen.
#[derive(Default)]
pub(crate) struct ExitStore {
    pub(crate) pictures: HashMap<u64, CapturedItem>,
    pub(crate) exits: HashMap<u64, ExitedItem>,
}

struct ItemState {
    offset: crate::runtime::state::State<Offset>,
    alpha: crate::runtime::state::State<f32>,
    last_position: Offset,
    last_frame: u64,
    offscreen: bool,
    fade_out: Option<crate::animation::AnimationSpec>,
}

#[derive(Default)]
struct HostState {
    frame: u64,
    items: HashMap<u64, ItemState>,
}

/// A cloneable host shared by a container and its measure policy.
#[derive(Clone)]
pub(crate) struct ItemAnimationHost {
    inner: Arc<Mutex<HostState>>,
    store: Arc<Mutex<ExitStore>>,
}

impl Default for ItemAnimationHost {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HostState::default())),
            store: Arc::new(Mutex::new(ExitStore::default())),
        }
    }
}

impl std::fmt::Debug for ItemAnimationHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ItemAnimationHost")
    }
}

impl ItemAnimationHost {
    /// The exit store this host owns; the container's render pass draws it inside its own clip.
    pub(crate) fn store(&self) -> &Arc<Mutex<ExitStore>> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::Modifier;

    fn leaf() -> LayoutNode {
        LayoutNode::leaf(Modifier::new().animate_item())
    }

    /// A key that is still in the data but outside the Lazy window must not become an exit ghost.
    #[test]
    fn live_keys_that_left_the_window_are_not_exits() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = ItemAnimationHost::default();
        let spec = ItemAnimationSpec::new()
            .fade_in(crate::animation::TweenSpec::default())
            .fade_out(crate::animation::TweenSpec::default());
        let mut nodes = vec![leaf(), leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec);
        host.place(&mut nodes, 1, 2, Offset::new(0.0, 10.0), &spec);
        host.finish_frame(Some(&[1, 2]));

        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec);
        host.finish_frame(Some(&[1, 2]));
        assert!(
            host.store.lock().unwrap_or_else(|e| e.into_inner()).exits.is_empty(),
            "scrolling key 2 out of the window must not start an exit"
        );

        host.begin_frame();
        host.place(&mut nodes, 1, 2, Offset::new(0.0, 10.0), &spec);
        host.finish_frame(Some(&[1, 2]));
        let paint = nodes[1].item_paint.as_ref().expect("the returned item is painted");
        assert_eq!(paint.alpha.peek(), 1.0, "a window return must not replay a fade-in");
        assert_eq!(paint.offset.peek(), Offset::ZERO, "a window return must not spring");
    }

    #[test]
    fn a_new_item_fades_in_after_the_first_layout() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let host = ItemAnimationHost::default();
        let spec = ItemAnimationSpec::new()
            .fade_in(crate::animation::TweenSpec::default());
        let mut nodes = vec![leaf(), leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec);
        host.finish_frame(None);

        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec);
        host.place(&mut nodes, 1, 2, Offset::ZERO, &spec);
        host.finish_frame(None);
        let paint = nodes[1].item_paint.as_ref().expect("the new item is painted");
        assert!(paint.alpha.peek() < 0.001, "the new key starts transparent");
        crate::animation::clear_all_animations();
    }
}

/// The first `animate_item` declaration in this subtree. The marker may sit on the item root or on
/// one of its descendants; the outer item is what moves, so the first declaration wins.
pub(crate) fn subtree_item_spec(
    nodes: &[LayoutNode],
    index: usize,
) -> Option<ItemAnimationSpec> {
    if let Some(spec) = nodes[index].modifier.animate_item_spec() {
        return Some(spec);
    }
    nodes[index]
        .children
        .iter()
        .find_map(|&child| subtree_item_spec(nodes, child))
}

impl ItemAnimationHost {
    /// Start one placement pass. Layout policies call this once before walking their items.
    pub(crate) fn begin_frame(&self) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .frame += 1;
    }

    /// Record an item and attach its paint channels. Returns true when the item has a declaration.
    pub(crate) fn place(
        &self,
        nodes: &mut [LayoutNode],
        child: usize,
        key: u64,
        position: Offset,
        spec: &ItemAnimationSpec,
    ) -> bool {
        if spec.is_inert() {
            return false;
        }
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let frame = state.frame;
        let is_new = !state.items.contains_key(&key);
        let item = state.items.entry(key).or_insert_with(|| ItemState {
            offset: crate::runtime::state::State::new(Offset::ZERO),
            alpha: crate::runtime::state::State::new(1.0),
            last_position: position,
            last_frame: frame,
            offscreen: false,
            fade_out: spec.fade_out.clone(),
        });

        // A key that comes back cancels a pending ghost rather than leaving two copies on screen.
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(exit) = store.exits.remove(&key) {
            crate::animation::cancel_animation(&exit.alpha);
        }
        if spec.fade_out.is_none() {
            store.pictures.remove(&key);
        }

        if item.offscreen {
            // The key was only outside the Lazy window. Returning must not replay an enter.
            item.offset.set(Offset::ZERO);
            item.alpha.set(1.0);
            item.offscreen = false;
        } else if is_new || item.last_frame + 1 != frame {
            // First appearance, or an item that was away for at least a frame: start at the new
            // slot, never from a stale visual position.
            item.offset.set(Offset::ZERO);
            if is_new && frame > 1 {
                if let Some(fade) = &spec.fade_in {
                    item.alpha.set(0.0);
                    crate::animation::push_animatable(
                        item.alpha.clone(),
                        1.0,
                        fade.clone(),
                    );
                } else {
                    item.alpha.set(1.0);
                }
            } else {
                item.alpha.set(1.0);
            }
        } else {
            let delta = item.last_position + item.offset.peek() - position;
            if delta.x.abs() > 0.01 || delta.y.abs() > 0.01 {
                if let Some(placement) = &spec.placement {
                    crate::animation::cancel_animation(&item.offset);
                    item.offset.set(delta);
                    crate::animation::push_animatable_restart(
                        item.offset.clone(),
                        Offset::ZERO,
                        placement.clone(),
                    );
                } else {
                    item.offset.set(Offset::ZERO);
                }
            }
        }

        item.last_position = position;
        item.last_frame = frame;
        item.fade_out = spec.fade_out.clone();
        nodes[child].item_paint = Some(ItemPaint {
            offset: item.offset.clone(),
            alpha: item.alpha.clone(),
            key,
            fade_out: spec.fade_out.clone(),
            exit_store: self.store.clone(),
        });
        true
    }

    /// Drop states that have been absent for long enough, and promote just-left items to the exit
    /// layer when their picture from the previous render is available.
    pub(crate) fn finish_frame(&self, live_keys: Option<&[u64]>) {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let frame = state.frame;
        let mut kept: Vec<(u64, ItemState)> = Vec::new();
        let mut left: Vec<(u64, ItemState)> = Vec::new();
        for (key, mut item) in state.items.drain() {
            if item.last_frame == frame {
                kept.push((key, item));
            } else if live_keys.is_some_and(|keys| keys.contains(&key)) {
                // The data still owns this key; it is only outside the Lazy window. Keep it so a
                // scroll back does not replay a fade-in or spring from an off-screen slot.
                item.offscreen = true;
                kept.push((key, item));
            } else if item.fade_out.is_some() {
                left.push((key, item));
            }
        }
        let live_with_fade: std::collections::HashSet<u64> = kept
            .iter()
            .filter(|(_, item)| item.fade_out.is_some())
            .map(|(key, _)| *key)
            .collect();
        state.items.extend(kept);
        drop(state);

        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        for (key, item) in left {
            if let Some(captured) = store.pictures.remove(&key) {
                let alpha = crate::runtime::state::State::new(1.0);
                crate::animation::push_animatable(
                    alpha.clone(),
                    0.0,
                    item.fade_out.clone().unwrap(),
                );
                store.exits.insert(
                    key,
                    ExitedItem {
                        picture: captured.picture,
                        position: captured.position,
                        alpha,
                    },
                );
            }
        }
        // A key that lost its fade_out, or left for good, must not leave a stale picture behind.
        let exiting: std::collections::HashSet<u64> = store.exits.keys().copied().collect();
        store
            .pictures
            .retain(|key, _| live_with_fade.contains(key) || exiting.contains(key));
    }
}
