use std::collections::LinkedList;
use crate::animation::Animation;

pub struct AnimationQueue {
    animations: LinkedList<Animation>,
}

impl AnimationQueue {
    pub fn new() -> Self {
        Self {
            animations: LinkedList::new(),
        }
    }

    pub fn then(mut self, animation: Animation) -> Self {
        self.animations.push_back(animation);
        self
    }

    pub fn start(&mut self) {

    }
}