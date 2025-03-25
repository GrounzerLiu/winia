use crate::ui::Item;
use skia_safe::Color;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use parking_lot::Mutex;
use crate::ui::app::AppContext;

#[derive(Clone)]
pub enum Value<T: Clone> {
    Ref(String),
    Value(T),
}

impl From<&str> for Value<Color> {
    fn from(s: &str) -> Self {
        Value::Ref(s.to_string())
    }
}

impl From<&str> for Value<f32> {
    fn from(s: &str) -> Self {
        Value::Ref(s.to_string())
    }
}

impl From<&str> for Value<bool> {
    fn from(s: &str) -> Self {
        Value::Ref(s.to_string())
    }
}

impl From<&str> for Value<Arc<Mutex<dyn Fn() -> Item>>> {
    fn from(s: &str) -> Self {
        Value::Ref(s.to_string())
    }
}

impl From<Color> for Value<Color> {
    fn from(c: Color) -> Self {
        Value::Value(c)
    }
}

impl From<f32> for Value<f32> {
    fn from(f: f32) -> Self {
        Value::Value(f)
    }
}

impl From<bool> for Value<bool> {
    fn from(b: bool) -> Self {
        Value::Value(b)
    }
}

impl From<Arc<Mutex<dyn Fn(AppContext) -> Item +Send>>> for Value<Arc<Mutex<dyn Fn(AppContext) -> Item + Send>>> {
    fn from(f: Arc<Mutex<dyn Fn(AppContext) -> Item + Send>>) -> Self {
        Value::Value(f)
    }
}

pub struct Theme {
    colors: HashMap<String, Value<Color>>,
    dimensions: HashMap<String, Value<f32>>,
    bools: HashMap<String, Value<bool>>,
    items: HashMap<String, Value<Arc<Mutex<dyn Fn(AppContext) -> Item + Send>>>>,
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
            items: HashMap::new(),
        }
    }

    pub fn set_color(&mut self, key: impl Into<String>, color: impl Into<Value<Color>>) -> &mut Self {
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

    pub fn set_bool(&mut self, key: impl Into<String>, boolean: impl Into<Value<bool>>) -> &mut Self {
        self.bools.insert(key.into(), boolean.into());
        self
    }

    pub fn set_item(
        &mut self,
        key: impl Into<String>,
        item: impl Into<Value<Arc<Mutex<dyn Fn(AppContext) -> Item +Send>>>>,
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
                    Value::Ref(key) => {
                        if keys.contains(&key) {
                            return None;
                        }
                        keys.push(key.clone());
                    }
                    Value::Value(value) => {
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

    pub fn get_item(&self, key: impl Into<String>, app: AppContext) -> Option<Item> {
        Self::get_value(&self.items, key, move |f| {
            let f = f.lock();
            f.deref()(app.clone())
        })
    }
}
