use std::collections::LinkedList;
use crate::animation::Animation;

pub struct AnimationSet {
    animations: LinkedList<Animation>
}

impl AnimationSet {
    pub fn new() -> Self {
        Self {
            animations: LinkedList::new(),
        }
    }

    pub fn with(mut self, animation: Animation) -> Self {
        self.animations.push_front(animation);
        self
    }

    pub fn start(mut self) {
        while let Some(animation) = self.animations.pop_front() {
            animation.start();
        }
    }
}