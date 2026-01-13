use std::ops::{Range, RangeInclusive};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LazyListState {
    pub offset: f32,
    pub visible_range: Range<usize>,
}

impl LazyListState {
    pub fn new() -> Self {
        Self {
            offset: 0.0,
            visible_range: 0..0,
        }
    }
    
    pub fn visible_count(&self) -> usize {
        self.visible_range.end - self.visible_range.start
    }
}