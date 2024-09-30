use crate::property::FloatProperty;

#[derive(Clone)]
pub struct LTRB {
    pub left: FloatProperty,
    pub top: FloatProperty,
    pub right: FloatProperty,
    pub bottom: FloatProperty,
}

#[derive(Clone)]
pub struct STEB {
    pub start: FloatProperty,
    pub top: FloatProperty,
    pub end: FloatProperty,
    pub bottom: FloatProperty,
}

impl LTRB {
    pub fn new() -> Self {
        LTRB::default()
    }
    
    pub fn all(value: impl Into<FloatProperty>) -> Self {
        let value = value.into();
        LTRB {
            left: value.clone(),
            top: value.clone(),
            right: value.clone(),
            bottom: value,
        }
    }
    
    pub fn left(mut self, value: impl Into<FloatProperty>) -> Self {
        self.left = value.into();
        self
    }
    
    pub fn top(mut self, value: impl Into<FloatProperty>) -> Self {
        self.top = value.into();
        self
    }
    
    pub fn right(mut self, value: impl Into<FloatProperty>) -> Self {
        self.right = value.into();
        self
    }
    
    pub fn bottom(mut self, value: impl Into<FloatProperty>) -> Self {
        self.bottom = value.into();
        self
    }
    
    pub fn vertical(mut self, value: impl Into<FloatProperty>) -> Self {
        let value = value.into();
        self.top = value.clone();
        self.bottom = value;
        self
    }
    
    pub fn horizontal(mut self, value: impl Into<FloatProperty>) -> Self {
        let value = value.into();
        self.left = value.clone();
        self.right = value;
        self
    }
}

impl Default for LTRB {
    fn default() -> Self {
        LTRB {
            left: 0.0.into(),
            top: 0.0.into(),
            right: 0.0.into(),
            bottom: 0.0.into(),
        }
    }
}

impl STEB {
    pub fn new() -> Self {
        STEB::default()
    }
    
    pub fn all(value: impl Into<FloatProperty>) -> Self {
        let value = value.into();
        STEB {
            start: value.clone(),
            top: value.clone(),
            end: value.clone(),
            bottom: value,
        }
    }
    
    pub fn start(mut self, value: impl Into<FloatProperty>) -> Self {
        self.start = value.into();
        self
    }
    
    pub fn top(mut self, value: impl Into<FloatProperty>) -> Self {
        self.top = value.into();
        self
    }
    
    pub fn end(mut self, value: impl Into<FloatProperty>) -> Self {
        self.end = value.into();
        self
    }
    
    pub fn bottom(mut self, value: impl Into<FloatProperty>) -> Self {
        self.bottom = value.into();
        self
    }
    
    pub fn vertical(mut self, value: impl Into<FloatProperty>) -> Self {
        let value = value.into();
        self.top = value.clone();
        self.bottom = value;
        self
    }
    
    pub fn horizontal(mut self, value: impl Into<FloatProperty>) -> Self {
        let value = value.into();
        self.start = value.clone();
        self.end = value;
        self
    }
}

impl Default for STEB {
    fn default() -> Self {
        STEB {
            start: 0.0.into(),
            top: 0.0.into(),
            end: 0.0.into(),
            bottom: 0.0.into(),
        }
    }
}

#[derive(Clone)]
pub enum Edges {
    LTRB(LTRB),
    STEB(STEB),
}

impl From<LTRB> for Edges {
    fn from(value: LTRB) -> Self {
        Edges::LTRB(value)
    }
}

impl From<STEB> for Edges {
    fn from(value: STEB) -> Self {
        Edges::STEB(value)
    }
}