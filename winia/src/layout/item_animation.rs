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
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// How many off-screen Lazy items may keep their paint channels so a scroll back does not replay
/// an enter. A long virtualized list must not grow this without bound.
const MAX_OFFSCREEN_STATES: usize = 256;

/// What the renderer applies around one item subtree.
#[derive(Clone)]
pub(crate) struct ItemPaint {
    /// Paint offset from the old slot towards the new one.
    pub(crate) offset: crate::runtime::state::State<Offset>,
    /// Opacity of the whole item.
    pub(crate) alpha: crate::runtime::state::State<f32>,
    /// Stable item key, used by the exit picture store.
    pub(crate) key: u64,
    /// The item's measure epoch when its paint was attached; invalidates stale pictures.
    pub(crate) content_epoch: u64,
    /// The global draw-layer epoch; captures paint-only changes that never re-measure.
    pub(crate) paint_epoch: u64,
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
    pub(crate) content_epoch: u64,
    pub(crate) paint_epoch: u64,
}

/// A removed item still playing its fade-out.
pub(crate) struct ExitedItem {
    pub(crate) picture: Picture,
    pub(crate) position: Offset,
    pub(crate) alpha: crate::runtime::state::State<f32>,
    /// Creation order, so overlapping ghosts paint in a stable order.
    pub(crate) order: u64,
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
    /// The order this item had in its last placement pass; used to draw simultaneous exits in
    /// layout order rather than HashMap order.
    order: u64,
    offscreen: bool,
    fade_out: Option<crate::animation::AnimationSpec>,
}

#[derive(Default)]
struct HostState {
    frame: u64,
    items: HashMap<u64, ItemState>,
    next_place_order: u64,

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

impl Drop for HostState {
    fn drop(&mut self) {
        for item in self.items.values() {
            crate::animation::cancel_animation(&item.offset);
            crate::animation::cancel_animation(&item.alpha);
        }
    }
}

impl Drop for ExitStore {
    fn drop(&mut self) {
        for exit in self.exits.values() {
            crate::animation::cancel_animation(&exit.alpha);
        }
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
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec, true);
        host.place(&mut nodes, 1, 2, Offset::new(0.0, 10.0), &spec, true);
        let live = HashSet::from([1u64, 2]);
        host.finish_frame(Some(&live));

        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec, true);
        host.finish_frame(Some(&live));
        assert!(
            host.store.lock().unwrap_or_else(|e| e.into_inner()).exits.is_empty(),
            "scrolling key 2 out of the window must not start an exit"
        );

        host.begin_frame();
        host.place(&mut nodes, 1, 2, Offset::new(0.0, 10.0), &spec, true);
        host.finish_frame(Some(&live));
        let paint = nodes[1].item_paint.as_ref().expect("the returned item is painted");
        assert_eq!(paint.alpha.peek(), 1.0, "a window return must not replay a fade-in");
        assert_eq!(paint.offset.peek(), Offset::ZERO, "a window return must not spring");
    }

    #[test]
    fn retire_drops_state_without_creating_an_exit() {
        let host = ItemAnimationHost::default();
        let spec = ItemAnimationSpec::new()
            .fade_out(crate::animation::TweenSpec::default());
        let mut nodes = vec![leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 7, Offset::ZERO, &spec, true);
        assert_eq!(host.inner.lock().unwrap_or_else(|e| e.into_inner()).items.len(), 1);

        host.retire(7);
        let state = host.inner.lock().unwrap_or_else(|e| e.into_inner());
        assert!(state.items.is_empty(), "retire must drop the live state");
        drop(state);
        let store = host.store.lock().unwrap_or_else(|e| e.into_inner());
        assert!(store.exits.is_empty(), "retire must not create a ghost");
        assert!(store.pictures.is_empty(), "retire must drop the picture");
        drop(store);

        let picture = {
            let mut recorder = skia_safe::PictureRecorder::new();
            {
                let _canvas = recorder.begin_recording(skia_safe::Rect::new(0.0, 0.0, 1.0, 1.0), false);
            }
            recorder.finish_recording_as_picture(None).expect("a picture")
        };
        host.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pictures
            .insert(7, CapturedItem { picture, position: Offset::ZERO, size: Size::new(1.0, 1.0), content_epoch: 0, paint_epoch: 0 });
        host.retire(7);
        assert!(
            host.store.lock().unwrap_or_else(|e| e.into_inner()).pictures.is_empty(),
            "retire must remove the cached exit picture"
        );
    }

    #[test]
    fn an_unsupported_public_channel_is_dropped_without_losing_the_item() {
        let host = ItemAnimationHost::default();
        let mut spec = ItemAnimationSpec::new();
        spec.fade_out = Some(crate::animation::AnimationSpec::Decay(
            crate::animation::DecaySpec::new(4.2, 0.1),
        ));
        let mut nodes = vec![leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 7, Offset::ZERO, &spec, true);
        let paint = nodes[0].item_paint.as_ref().expect("the valid placement is kept");
        assert!(paint.fade_out.is_none(), "the unsupported fade is dropped");
    }

    #[test]
    fn an_exit_starts_at_the_current_alpha() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let host = ItemAnimationHost::default();
        let spec = ItemAnimationSpec::new()
            .fade_in(crate::animation::TweenSpec::default())
            .fade_out(crate::animation::TweenSpec::default());
        let mut nodes = vec![leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 7, Offset::ZERO, &spec, true);
        host.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .items
            .get_mut(&7)
            .expect("the item state")
            .alpha
            .set(0.25);

        let picture = {
            let mut recorder = skia_safe::PictureRecorder::new();
            {
                let _canvas = recorder.begin_recording(skia_safe::Rect::new(0.0, 0.0, 1.0, 1.0), false);
            }
            recorder.finish_recording_as_picture(None).expect("a picture")
        };
        host.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pictures
            .insert(7, CapturedItem { picture, position: Offset::ZERO, size: Size::new(1.0, 1.0), content_epoch: 0, paint_epoch: 0 });

        host.begin_frame();
        host.finish_frame(None);
        let state = host.store.lock().unwrap_or_else(|e| e.into_inner());
        let exit = state.exits.get(&7).expect("the removed item becomes an exit");
        assert!(
            (exit.alpha.peek() - 0.25).abs() < 0.001,
            "the ghost must start at the live alpha, not 1: {}",
            exit.alpha.peek()
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn offscreen_states_are_bounded() {
        let host = ItemAnimationHost::default();
        let spec = ItemAnimationSpec::new();
        let mut nodes = vec![leaf()];
        host.begin_frame();
        for key in 1..=300u64 {
            host.place(&mut nodes, 0, key, Offset::ZERO, &spec, true);
        }
        host.finish_frame(None);
        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec, true);
        let live: HashSet<u64> = (1..=300u64).collect();
        host.finish_frame(Some(&live));
        let state = host.inner.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            state.items.len() <= MAX_OFFSCREEN_STATES + 1,
            "offscreen state grew to {} (bound {})",
            state.items.len(),
            MAX_OFFSCREEN_STATES
        );
        assert!(
            state.items.contains_key(&1),
            "a visible key must not be evicted by the off-screen bound"
        );
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
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec, true);
        host.finish_frame(None);

        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &spec, true);
        host.place(&mut nodes, 1, 2, Offset::ZERO, &spec, true);
        host.finish_frame(None);
        let paint = nodes[1].item_paint.as_ref().expect("the new item is painted");
        assert!(paint.alpha.peek() < 0.001, "the new key starts transparent");
        crate::animation::clear_all_animations();
    }
    /// Compose's `Modifier.animateItem()` animates appearance by default (fadeInSpec is a spring, not
    /// null), so the bare marker must fade a new item in — and `without_fade_in()` must turn that off.
    #[test]
    fn the_default_marker_fades_a_new_item_in() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let host = ItemAnimationHost::default();
        let default_spec = ItemAnimationSpec::new();
        let mut nodes = vec![leaf(), leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &default_spec, true);
        host.finish_frame(None);
        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &default_spec, true);
        host.place(&mut nodes, 1, 2, Offset::ZERO, &default_spec, true);
        host.finish_frame(None);
        let paint = nodes[1].item_paint.as_ref().expect("the new item is painted");
        assert!(
            paint.alpha.peek() < 0.001,
            "the default contract fades a new key in (Compose's fadeInSpec default is not null)"
        );
        assert!(paint.fade_out.is_some(), "and it captures an exit picture for a later removal");

        // The same frame with the channel dropped: no fade, no exit picture.
        crate::animation::clear_all_animations();
        let host = ItemAnimationHost::default();
        let no_fade = ItemAnimationSpec::new().without_fade_in().without_fade_out();
        let mut nodes = vec![leaf(), leaf()];
        host.begin_frame();
        host.place(&mut nodes, 0, 1, Offset::ZERO, &no_fade, true);
        host.finish_frame(None);
        host.begin_frame();
        host.place(&mut nodes, 1, 2, Offset::ZERO, &no_fade, true);
        host.finish_frame(None);
        let paint = nodes[1].item_paint.as_ref().expect("the new item is painted");
        assert_eq!(paint.alpha.peek(), 1.0, "without_fade_in starts opaque");
        assert!(paint.fade_out.is_none(), "without_fade_out captures nothing");
        crate::animation::clear_all_animations();
    }
}

/// The marker for a Lazy item wrapper. The item's root content is the wrapper's direct child;
/// markers deeper in the content belong to nested Flow/Lazy containers, not to the outer item.
pub(crate) fn wrapper_item_spec(
    nodes: &[LayoutNode],
    index: usize,
) -> Option<ItemAnimationSpec> {
    if let Some(spec) = nodes[index].modifier.animate_item_spec() {
        return Some(spec);
    }
    nodes[index]
        .children
        .iter()
        .find_map(|&child| nodes[child].modifier.animate_item_spec())
}

impl ItemAnimationHost {
    /// Start one placement pass. Layout policies call this once before walking their items.
    /// Starts a layout pass.
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
        // Whether this key is new to the container's DATA, as opposed to merely new to the composed
        // window. A Lazy list decides per key — scrolling composes keys that have been in the data all
        // along, and fading those in makes a scroll look like a strobe (measured: a jump to offset
        // 4000 composed keys 35..39 at alpha 0). A container that composes every child passes `true`.
        new_in_data: bool,
    ) -> bool {
        let sanitized = if spec.is_valid() { None } else { Some(spec.sanitized()) };
        let spec = sanitized.as_ref().unwrap_or(spec);
        if spec.is_inert() {
            self.retire(key);
            return false;
        }
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let frame = state.frame;

        let order = state.next_place_order;
        state.next_place_order += 1;
        let is_new = !state.items.contains_key(&key);
        let item = state.items.entry(key).or_insert_with(|| ItemState {
            offset: crate::runtime::state::State::new(Offset::ZERO),
            alpha: crate::runtime::state::State::new(1.0),
            last_position: position,
            last_frame: frame,
            order,
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
            // The key was only outside the Lazy window. Returning must not replay an enter, and the
            // old enter/move motions must not keep writing into the reset channels.
            crate::animation::cancel_animation(&item.offset);
            crate::animation::cancel_animation(&item.alpha);
            item.offset.set(Offset::ZERO);
            item.alpha.set(1.0);
            item.offscreen = false;
        } else if is_new || item.last_frame + 1 != frame {
            // First appearance, or an item that was away for at least a frame: start at the new
            // slot, never from a stale visual position.
            crate::animation::cancel_animation(&item.offset);
            crate::animation::cancel_animation(&item.alpha);
            item.offset.set(Offset::ZERO);
            // A key new to the WINDOW is not a new item: a scroll composes keys that have always been
            // in the data, and fading them in makes the scroll look like a strobe. The container
            // answers whether its data changed.
            if is_new && frame > 1 && new_in_data {
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
                    crate::animation::cancel_animation(&item.offset);
                    item.offset.set(Offset::ZERO);
                }
            } else if spec.placement.is_none() {
                crate::animation::cancel_animation(&item.offset);
                item.offset.set(Offset::ZERO);
            }
        }

        if !is_new && spec.fade_in.is_none() && item.alpha.peek() < 0.999 {
            // The caller removed the fade-in while it was running; do not let the old tween keep
            // writing into the live item's alpha.
            crate::animation::cancel_animation(&item.alpha);
            item.alpha.set(1.0);
        }

        item.last_position = position;
        item.last_frame = frame;
        item.order = order;
        item.fade_out = spec.fade_out.clone();
        nodes[child].item_paint = Some(ItemPaint {
            offset: item.offset.clone(),
            alpha: item.alpha.clone(),
            key,
            content_epoch: nodes[child].content_epoch,
            paint_epoch: crate::runtime::state::visual_epoch(),
            fade_out: spec.fade_out.clone(),
            exit_store: self.store.clone(),
        });
        true
    }

    /// A live child no longer carries an active marker. Drop its channels, pictures and any exit
    /// without drawing a ghost: the item is still in the tree, only its animation was removed.
    pub(crate) fn retire(&self, key: u64) {
        let removed = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .items
            .remove(&key);
        if let Some(item) = removed {
            crate::animation::cancel_animation(&item.offset);
            crate::animation::cancel_animation(&item.alpha);
        }
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(exit) = store.exits.remove(&key) {
            crate::animation::cancel_animation(&exit.alpha);
        }
        store.pictures.remove(&key);
    }

    /// Drop states that have been absent for long enough, and promote just-left items to the exit
    /// layer when their picture from the previous render is available.
    pub(crate) fn finish_frame(&self, live_keys: Option<&HashSet<u64>>) {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let frame = state.frame;
        let mut kept: Vec<(u64, ItemState)> = Vec::new();
        let mut offscreen: Vec<(u64, ItemState)> = Vec::new();
        let mut left: Vec<(u64, ItemState)> = Vec::new();
        for (key, mut item) in state.items.drain() {
            if item.last_frame == frame {
                kept.push((key, item));
            } else if live_keys.is_some_and(|keys| keys.contains(&key)) {
                // The data still owns this key; it is only outside the Lazy window. Cancel its
                // channels once and retain only a bounded recent set, so a long scroll cannot grow
                // the host without limit.
                if !item.offscreen {
                    crate::animation::cancel_animation(&item.offset);
                    crate::animation::cancel_animation(&item.alpha);
                    item.offscreen = true;
                }
                offscreen.push((key, item));
            } else if item.fade_out.is_some() {
                left.push((key, item));
            } else {
                crate::animation::cancel_animation(&item.offset);
                crate::animation::cancel_animation(&item.alpha);
            }
        }
        if offscreen.len() > MAX_OFFSCREEN_STATES {
            offscreen.sort_by_key(|(_, item)| item.last_frame);
            let excess = offscreen.len() - MAX_OFFSCREEN_STATES;
            offscreen.drain(..excess);
        }
        kept.extend(offscreen);
        // Pictures are for the current screen only. An off-screen item can be seen again later,
        // but keeping one Skia Picture per item ever scrolled through is what turns a long Lazy
        // list into a memory leak.
        let live_with_fade: std::collections::HashSet<u64> = kept
            .iter()
            .filter(|(_, item)| item.last_frame == frame && item.fade_out.is_some())
            .map(|(key, _)| *key)
            .collect();
        state.items.extend(kept);
        drop(state);

        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        for (key, item) in left {
            let start_alpha = item.alpha.peek().clamp(0.0, 1.0);
            crate::animation::cancel_animation(&item.offset);
            crate::animation::cancel_animation(&item.alpha);
            if let Some(captured) = store.pictures.remove(&key) {
                let alpha = crate::runtime::state::State::new(start_alpha);
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
                        order: item.order,
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
