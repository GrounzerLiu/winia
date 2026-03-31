use crate::shared::{AnimatableValue, Animation, AnimationSpec, Tween, TweenSpec};

pub enum RepeatMode {
    Restart,
    Reverse,
}
pub enum RepeatCount {
    Finite(u32),
    Infinite,
}

pub struct RepeatSpec<T: AnimatableValue> {
    pub tween_spec: TweenSpec<T>,
    pub repeat_mode: RepeatMode,
    pub repeat_count: RepeatCount,
    pub on_repeat_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}

impl<T: AnimatableValue> RepeatSpec<T> {
    pub fn new(
        tween_spec: TweenSpec<T>,
        repeat_mode: RepeatMode,
        repeat_count: RepeatCount,
    ) -> Self {
        Self {
            tween_spec,
            repeat_mode,
            repeat_count,
            on_repeat_callbacks: vec![],
        }
    }

    pub fn on_repeat<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_repeat_callbacks.push(Box::new(callback));
        self
    }
}

impl<T: AnimatableValue + 'static> AnimationSpec<T> for RepeatSpec<T> {
    fn build(self, from: T, to: T) -> Box<dyn Animation<T>> {
        let tween = self.tween_spec.build_tween(from, to);
        Box::new(Repeat {
            is_finished: false,
            tween,
            repeat_mode: self.repeat_mode,
            repeat_count: self.repeat_count,
            on_repeat_callbacks: self.on_repeat_callbacks,
            current_repeat: 0,
        })
    }
}

pub struct Repeat<T: AnimatableValue> {
    is_finished: bool,
    pub tween: Tween<T>,
    pub repeat_mode: RepeatMode,
    pub repeat_count: RepeatCount,
    pub on_repeat_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    pub current_repeat: u32,
}

impl<T: AnimatableValue> Repeat<T> {
    pub fn new(
        tween: Tween<T>,
        repeat_mode: RepeatMode,
        repeat_count: RepeatCount,
    ) -> Self {
        Self {
            is_finished: false,
            tween,
            repeat_mode,
            repeat_count,
            on_repeat_callbacks: vec![],
            current_repeat: 0,
        }
    }

    pub fn on_repeat<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_repeat_callbacks.push(Box::new(callback));
        self
    }
}

impl<T: AnimatableValue> Animation<T> for Repeat<T> {
    fn update(&mut self) -> Option<T> {
        if self.is_finished {
            return None;
        }

        if let Some(value) = self.tween.update() {
            Some(value)
        } else if self.tween.check_finished() {
            self.current_repeat += 1;
            match self.repeat_count {
                RepeatCount::Finite(count) if self.current_repeat >= count => {
                    self.is_finished = true;
                    None
                }
                _ => {
                    for callback in &self.on_repeat_callbacks {
                        callback();
                    }
                    match self.repeat_mode {
                        RepeatMode::Restart => {
                            self.tween.restart();
                        }
                        RepeatMode::Reverse => {
                            let current_to = self.tween.to.clone();
                            let current_from = self.tween.from.clone();
                            self.tween.animate_to(current_from);
                            self.tween.from = current_to;
                        }
                    }
                    self.tween.update()
                }
            }
        } else {
            None
        }
    }

    fn check_finished(&mut self) -> bool {
        if self.is_finished {
            return true;
        }
        if self.tween.check_finished() {
            if let RepeatCount::Finite(count) = self.repeat_count {
                if self.current_repeat >= count {
                    self.is_finished = true;
                    return true;
                }
            }
        }
        false
    }

    fn animate_to(&mut self, target: T) {
        self.tween.animate_to(target);
        self.is_finished = false;
    }

    fn stop(&mut self) {
        self.tween.stop();
        self.is_finished = true;
    }
}