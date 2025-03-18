use crate::ui::Item;
use skia_safe::Color;
use std::collections::HashMap;

pub static WINDOW_BACKGROUND_COLOR: &str = "window_background_color";

pub enum Value<T> {
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

impl From<&str> for Value<Box<dyn Fn() -> Item>> {
    fn from(s: &str) -> Self {
        Value::Ref(s.to_string())
    }
}

impl From<&str> for Value<Style> {
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

impl From<Box<dyn Fn() -> Item>> for Value<Box<dyn Fn() -> Item>> {
    fn from(f: Box<dyn Fn() -> Item>) -> Self {
        Value::Value(f)
    }
}

impl From<Style> for Value<Style> {
    fn from(s: Style) -> Self {
        Value::Value(s)
    }
}

pub struct Style {
    colors: HashMap<String, Value<Color>>,
    dimensions: HashMap<String, Value<f32>>,
    bools: HashMap<String, Value<bool>>,
    items: HashMap<String, Value<Box<dyn Fn() -> Item + Send>>>,
    styles: HashMap<String, Value<Style>>,
}

impl Style {
    pub fn new() -> Self {
        Self {
            colors: HashMap::new(),
            dimensions: HashMap::new(),
            bools: HashMap::new(),
            items: HashMap::new(),
            styles: HashMap::new(),
        }
    }

    pub fn set_color(mut self, key: impl Into<String>, color: impl Into<Value<Color>>) -> Self {
        self.colors.insert(key.into(), color.into());
        self
    }

    pub fn set_dimension(
        mut self,
        key: impl Into<String>,
        dimension: impl Into<Value<f32>>,
    ) -> Self {
        self.dimensions.insert(key.into(), dimension.into());
        self
    }

    pub fn set_bool(mut self, key: impl Into<String>, boolean: impl Into<Value<bool>>) -> Self {
        self.bools.insert(key.into(), boolean.into());
        self
    }

    pub fn set_item(
        mut self,
        key: impl Into<String>,
        item: impl Into<Value<Box<dyn Fn() -> Item + Send>>>,
    ) -> Self {
        self.items.insert(key.into(), item.into());
        self
    }

    pub fn set_style(mut self, key: impl Into<String>, style: impl Into<Value<Style>>) -> Self {
        self.styles.insert(key.into(), style.into());
        self
    }

    fn get_value<T, R>(
        map: &HashMap<String, Value<T>>,
        key: impl Into<String>,
        f: impl Fn(&T) -> R,
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

    pub fn get_item(&self, key: impl Into<String>) -> Option<Item> {
        Self::get_value(&self.items, key, |f| f())
    }

    pub fn get_style(&self, key: impl Into<String>) -> Option<&Style> {
        let key = key.into();
        let mut keys = vec![key];
        loop {
            let key = keys.last().unwrap();
            if let Some(value) = self.styles.get(key) {
                match value {
                    Value::Ref(key) => {
                        if keys.contains(&key) {
                            return None;
                        }
                        keys.push(key.clone());
                    }
                    Value::Value(value) => {
                        return Some(value);
                    }
                }
            } else {
                return None;
            }
        }
    }
}
