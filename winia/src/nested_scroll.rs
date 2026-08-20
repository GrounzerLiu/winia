//! Nested scroll primitives shared by scrollables and app-bar behaviors.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollDelta { pub x: f32, pub y: f32 }

impl ScrollDelta {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub fn new(x: f32, y: f32) -> Self { Self { x, y } }
    pub fn clamp_to(self, available: Self) -> Self {
        let clamp = |value: f32, limit: f32| value.clamp(-limit.abs(), limit.abs());
        Self { x: clamp(self.x, available.x), y: clamp(self.y, available.y) }
    }
}

impl std::ops::Add for ScrollDelta {
    type Output = Self;
    fn add(self, rhs: Self) -> Self { Self::new(self.x + rhs.x, self.y + rhs.y) }
}
impl std::ops::Sub for ScrollDelta {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { Self::new(self.x - rhs.x, self.y - rhs.y) }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollVelocity { pub x: f32, pub y: f32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedScrollSource { Wheel, Drag, Fling, SideEffect }

pub trait NestedScrollConnection: Send + Sync {
    fn on_pre_scroll(&self, available: ScrollDelta, source: NestedScrollSource) -> ScrollDelta { let _ = (available, source); ScrollDelta::ZERO }
    fn on_post_scroll(&self, consumed_by_child: ScrollDelta, available: ScrollDelta, source: NestedScrollSource) -> ScrollDelta { let _ = (consumed_by_child, available, source); ScrollDelta::ZERO }
    fn on_pre_fling(&self, available: ScrollVelocity) -> ScrollVelocity { available }
    fn on_post_fling(&self, consumed_by_child: ScrollVelocity, available: ScrollVelocity) -> ScrollVelocity { let _ = consumed_by_child; available }
}

#[derive(Clone, Default)]
pub struct NestedScrollDispatcher {
    ancestors: std::sync::Arc<parking_lot::RwLock<Vec<std::sync::Arc<dyn NestedScrollConnection>>>>,
}

impl NestedScrollDispatcher {
    pub fn new() -> Self { Self::default() }
    pub fn with_ancestors(ancestors: Vec<std::sync::Arc<dyn NestedScrollConnection>>) -> Self { Self { ancestors: std::sync::Arc::new(parking_lot::RwLock::new(ancestors)) } }
    pub fn set_ancestors(&self, ancestors: Vec<std::sync::Arc<dyn NestedScrollConnection>>) { *self.ancestors.write() = ancestors; }
    pub fn pre_scroll(&self, available: ScrollDelta, source: NestedScrollSource) -> ScrollDelta {
        let mut consumed = ScrollDelta::ZERO;
        let mut remaining = available;
        for connection in self.ancestors.read().iter() {
            let part = connection.on_pre_scroll(remaining, source).clamp_to(remaining);
            consumed = consumed + part;
            remaining = remaining - part;
        }
        consumed
    }
    pub fn post_scroll(&self, consumed_by_child: ScrollDelta, available: ScrollDelta, source: NestedScrollSource) -> ScrollDelta {
        let mut consumed = ScrollDelta::ZERO;
        let mut remaining = available;
        for connection in self.ancestors.read().iter().rev() {
            let part = connection.on_post_scroll(consumed_by_child, remaining, source).clamp_to(remaining);
            consumed = consumed + part;
            remaining = remaining - part;
        }
        consumed
    }
    pub fn pre_fling(&self, available: ScrollVelocity) -> ScrollVelocity {
        let mut consumed = ScrollVelocity::default();
        let mut remaining = available;
        for connection in self.ancestors.read().iter() {
            let part = connection.on_pre_fling(remaining);
            let consumed_x = part.x.clamp(-remaining.x.abs(), remaining.x.abs());
            let consumed_y = part.y.clamp(-remaining.y.abs(), remaining.y.abs());
            consumed.x += consumed_x;
            consumed.y += consumed_y;
            remaining.x -= consumed_x;
            remaining.y -= consumed_y;
        }
        consumed
    }
    pub fn post_fling(&self, consumed_by_child: ScrollVelocity, available: ScrollVelocity) -> ScrollVelocity {
        let mut remaining = available;
        for connection in self.ancestors.read().iter().rev() {
            let part = connection.on_post_fling(consumed_by_child, remaining);
            remaining.x -= part.x.clamp(-remaining.x.abs(), remaining.x.abs());
            remaining.y -= part.y.clamp(-remaining.y.abs(), remaining.y.abs());
        }
        ScrollVelocity { x: available.x - remaining.x, y: available.y - remaining.y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Recorder(Arc<Mutex<Vec<&'static str>>>);
    impl NestedScrollConnection for Recorder {
        fn on_pre_scroll(&self, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta { self.0.lock().unwrap().push("pre"); ScrollDelta::new(available.x / 2.0, available.y / 2.0) }
        fn on_post_scroll(&self, _: ScrollDelta, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta { self.0.lock().unwrap().push("post"); ScrollDelta::new(available.x, available.y) }
    }

    #[test]
    fn dispatcher_consumes_pre_then_post_in_ancestor_order() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let dispatcher = NestedScrollDispatcher::with_ancestors(vec![Arc::new(Recorder(calls.clone()))]);
        assert_eq!(dispatcher.pre_scroll(ScrollDelta::new(10.0, 20.0), NestedScrollSource::Drag), ScrollDelta::new(5.0, 10.0));
        assert_eq!(dispatcher.post_scroll(ScrollDelta::new(5.0, 10.0), ScrollDelta::new(5.0, 10.0), NestedScrollSource::Drag), ScrollDelta::new(5.0, 10.0));
        assert_eq!(*calls.lock().unwrap(), vec!["pre", "post"]);
    }

    struct FlingRecorder(Arc<Mutex<Vec<&'static str>>>);
    impl NestedScrollConnection for FlingRecorder {
        fn on_pre_fling(&self, available: ScrollVelocity) -> ScrollVelocity {
            self.0.lock().unwrap().push("fling-pre");
            ScrollVelocity { x: available.x / 2.0, y: available.y / 2.0 }
        }
        fn on_post_fling(&self, _: ScrollVelocity, available: ScrollVelocity) -> ScrollVelocity {
            self.0.lock().unwrap().push("fling-post");
            ScrollVelocity { x: available.x, y: available.y }
        }
    }

    #[test]
    fn dispatcher_runs_fling_pre_then_post() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let dispatcher = NestedScrollDispatcher::with_ancestors(vec![Arc::new(FlingRecorder(calls.clone()))]);
        assert_eq!(
            dispatcher.pre_fling(ScrollVelocity { x: 100.0, y: 200.0 }),
            ScrollVelocity { x: 50.0, y: 100.0 }
        );
        assert_eq!(
            dispatcher.post_fling(ScrollVelocity { x: 50.0, y: 100.0 }, ScrollVelocity { x: 50.0, y: 100.0 }),
            ScrollVelocity { x: 50.0, y: 100.0 }
        );
        assert_eq!(*calls.lock().unwrap(), vec!["fling-pre", "fling-post"]);
    }
}
