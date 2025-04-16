use crate::ui::app::WindowContext;
use crate::ui::Item;
use parking_lot::Mutex;
use skia_safe::Color;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Clone)]
pub enum Value<T: Clone> {
    Reference(String),
    Direct(T),
}

impl From<&str> for Value<Color> {
    fn from(s: &str) -> Self {
        Value::Reference(s.to_string())
    }
}

impl From<String> for Value<Color> {
    fn from(s: String) -> Self {
        Value::Reference(s)
    }
}

impl From<&str> for Value<f32> {
    fn from(s: &str) -> Self {
        Value::Reference(s.to_string())
    }
}

impl From<String> for Value<f32> {
    fn from(s: String) -> Self {
        Value::Reference(s)
    }
}

impl From<&str> for Value<bool> {
    fn from(s: &str) -> Self {
        Value::Reference(s.to_string())
    }
}

impl From<String> for Value<bool> {
    fn from(s: String) -> Self {
        Value::Reference(s)
    }
}

impl From<&str> for Value<String> {
    fn from(s: &str) -> Self {
        Value::Reference(s.to_string())
    }
}
impl From<&str> for Value<Arc<Mutex<dyn Fn() -> Item>>> {
    fn from(s: &str) -> Self {
        Value::Reference(s.to_string())
    }
}

impl From<Color> for Value<Color> {
    fn from(c: Color) -> Self {
        Value::Direct(c)
    }
}

impl From<f32> for Value<f32> {
    fn from(f: f32) -> Self {
        Value::Direct(f)
    }
}

impl From<bool> for Value<bool> {
    fn from(b: bool) -> Self {
        Value::Direct(b)
    }
}

impl From<String> for Value<String> {
    fn from(s: String) -> Self {
        Value::Direct(s)
    }
}

impl From<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>
    for Value<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>
{
    fn from(f: Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>) -> Self {
        Value::Direct(f)
    }
}

pub struct Theme {
    colors: HashMap<String, Value<Color>>,
    pub dimensions: HashMap<String, Value<f32>>,
    bools: HashMap<String, Value<bool>>,
    strings: HashMap<String, Value<String>>,
    items: HashMap<String, Value<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

impl Theme {
    pub(crate) fn new() -> Self {
        Self {
            colors: HashMap::new(),
            dimensions: HashMap::new(),
            bools: HashMap::new(),
            strings: HashMap::new(),
            items: HashMap::new(),
        }
    }

    pub fn set_color(
        &mut self,
        key: impl Into<String>,
        color: impl Into<Value<Color>>,
    ) -> &mut Self {
        self.colors.insert(key.into(), color.into());
        self
    }

    pub fn set_dimension(
        &mut self,
        key: impl Into<String>,
        dimension: impl Into<Value<f32>>,
    ) -> &mut Self {
        self.dimensions.insert(key.into(), dimension.into());
        self
    }

    pub fn set_bool(
        &mut self,
        key: impl Into<String>,
        boolean: impl Into<Value<bool>>,
    ) -> &mut Self {
        self.bools.insert(key.into(), boolean.into());
        self
    }

    pub fn set_string(
        &mut self,
        key: impl Into<String>,
        string: impl Into<Value<String>>,
    ) -> &mut Self {
        self.strings.insert(key.into(), string.into());
        self
    }

    pub fn set_item(
        &mut self,
        key: impl Into<String>,
        item: impl Into<Value<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>>,
    ) -> &mut Self {
        self.items.insert(key.into(), item.into());
        self
    }

    fn get_value<T: Clone, R>(
        map: &HashMap<String, Value<T>>,
        key: impl Into<String>,
        mut f: impl FnMut(&T) -> R,
    ) -> Option<R> {
        let key = key.into();
        let mut keys = vec![key];
        loop {
            let key = keys.last().unwrap();
            if let Some(value) = map.get(key) {
                match value {
                    Value::Reference(key) => {
                        if keys.contains(&key) {
                            return None;
                        }
                        keys.push(key.clone());
                    }
                    Value::Direct(value) => {
                        return Some(f(value));
                    }
                }
            } else {
                return None;
            }
        }
    }

    pub fn get_color(&self, key: impl Into<String>) -> Option<Color> {
        Self::get_value(&self.colors, key, |c| *c)
    }

    pub fn get_dimension(&self, key: impl Into<String>) -> Option<f32> {
        Self::get_value(&self.dimensions, key, |d| *d)
    }

    pub fn get_bool(&self, key: impl Into<String>) -> Option<bool> {
        Self::get_value(&self.bools, key, |b| *b)
    }

    pub fn get_string(&self, key: impl Into<String>) -> Option<String> {
        Self::get_value(&self.strings, key, |s| s.clone())
    }

    pub fn get_item(&self, key: impl Into<String>, app: WindowContext) -> Option<Item> {
        Self::get_value(&self.items, key, move |f| {
            let f = f.lock();
            f.deref()(app.clone())
        })
    }
}
