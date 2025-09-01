use std::any::Any;
use crate::ui::app::WindowContext;
use crate::ui::Item;
use parking_lot::Mutex;
use skia_safe::Color;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use crate::ui::theme::shape::Corner;

#[derive(Clone)]
pub enum ThemeValue<T> {
    Ref(String),
    Direct(T),
}

impl From<&str> for ThemeValue<Color> {
    fn from(s: &str) -> Self {
        ThemeValue::Ref(s.to_string())
    }
}

impl From<String> for ThemeValue<Color> {
    fn from(s: String) -> Self {
        ThemeValue::Ref(s)
    }
}

impl From<&str> for ThemeValue<f32> {
    fn from(s: &str) -> Self {
        ThemeValue::Ref(s.to_string())
    }
}

impl From<String> for ThemeValue<f32> {
    fn from(s: String) -> Self {
        ThemeValue::Ref(s)
    }
}

impl From<&str> for ThemeValue<bool> {
    fn from(s: &str) -> Self {
        ThemeValue::Ref(s.to_string())
    }
}

impl From<String> for ThemeValue<bool> {
    fn from(s: String) -> Self {
        ThemeValue::Ref(s)
    }
}

impl From<&str> for ThemeValue<Arc<Mutex<dyn Fn() -> Item>>> {
    fn from(s: &str) -> Self {
        ThemeValue::Ref(s.to_string())
    }
}

impl From<Color> for ThemeValue<Color> {
    fn from(c: Color) -> Self {
        ThemeValue::Direct(c)
    }
}

impl From<f32> for ThemeValue<f32> {
    fn from(f: f32) -> Self {
        ThemeValue::Direct(f)
    }
}

impl From<bool> for ThemeValue<bool> {
    fn from(b: bool) -> Self {
        ThemeValue::Direct(b)
    }
}

impl From<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>
    for ThemeValue<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>
{
    fn from(f: Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>) -> Self {
        ThemeValue::Direct(f)
    }
}

impl From<&str> for ThemeValue<Corner> {
    fn from(s: &str) -> Self {
        ThemeValue::Ref(s.to_string())
    }
}

impl From<Corner> for ThemeValue<Corner> {
    fn from(c: Corner) -> Self {
        ThemeValue::Direct(c)
    }
}

pub struct Theme {
    colors: HashMap<String, ThemeValue<Color>>,
    dimensions: HashMap<String, ThemeValue<f32>>,
    bools: HashMap<String, ThemeValue<bool>>,
    strings: HashMap<String, ThemeValue<String>>,
    styles: HashMap<String, Box<dyn Any + Send>>,
    items: HashMap<String, ThemeValue<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>>,
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
            styles: HashMap::new(),
            items: HashMap::new(),
        }
    }

    pub fn set_color(
        &mut self,
        key: impl Into<String>,
        color: impl Into<ThemeValue<Color>>,
    ) -> &mut Self {
        self.colors.insert(key.into(), color.into());
        self
    }

    pub fn set_dimension(
        &mut self,
        key: impl Into<String>,
        dimension: impl Into<ThemeValue<f32>>,
    ) -> &mut Self {
        self.dimensions.insert(key.into(), dimension.into());
        self
    }

    pub fn set_bool(
        &mut self,
        key: impl Into<String>,
        boolean: impl Into<ThemeValue<bool>>,
    ) -> &mut Self {
        self.bools.insert(key.into(), boolean.into());
        self
    }

    pub fn set_string(
        &mut self,
        key: impl Into<String>,
        string: ThemeValue<String>
    ) -> &mut Self {
        self.strings.insert(key.into(), string);
        self
    }

    pub fn set_item(
        &mut self,
        key: impl Into<String>,
        item: impl Into<ThemeValue<Arc<Mutex<dyn Fn(WindowContext) -> Item + Send>>>>,
    ) -> &mut Self {
        self.items.insert(key.into(), item.into());
        self
    }
    
    pub fn set_style(
        &mut self,
        key: impl Into<String>,
        style: Box<dyn Any + Send>,
    ) -> &mut Self {
        self.styles.insert(key.into(), style);
        self
    }

    fn get_value<T>(
        map: &HashMap<String, ThemeValue<T>>,
        key: impl Into<String>,
    ) -> Option<&T> {
        let key = key.into();
        let mut keys = vec![key];
        loop {
            let key = keys.last().unwrap();
            if let Some(value) = map.get(key) {
                match value {
                    ThemeValue::Ref(key) => {
                        if keys.contains(&key) {
                            return None;
                        }
                        keys.push(key.clone());
                    }
                    ThemeValue::Direct(value) => {
                        return Some(value);
                    }
                }
            } else {
                return None;
            }
        }
    }

    pub fn get_color(&self, key: impl Into<String>) -> Option<&Color> {
        Self::get_value(&self.colors, key)
    }

    pub fn get_dimension(&self, key: impl Into<String>) -> Option<&f32> {
        Self::get_value(&self.dimensions, key)
    }

    pub fn get_bool(&self, key: impl Into<String>) -> Option<&bool> {
        Self::get_value(&self.bools, key)
    }

    pub fn get_string(&self, key: impl Into<String>) -> Option<&String> {
        Self::get_value(&self.strings, key)
    }

    pub fn get_item(&self, key: impl Into<String>, app: WindowContext) -> Option<Item> {
        if let Some(item_generator) = Self::get_value(&self.items, key) {
            let item = item_generator.lock().deref()(app);
            Some(item)
        } else {
            None
        }
    }
    
    pub fn get_style<T:Any + Send>(&self, key: impl Into<String>) -> Option<&T> {
        let style = self.styles.get(&key.into())?;
        Some(style.downcast_ref::<T>()?)
    }
}

#[derive(Clone)]
pub struct State<T: Clone> {
    pub enabled: ThemeValue<T>,
    pub disabled: Option<ThemeValue<T>>,
    pub hovered: Option<ThemeValue<T>>,
    pub focused: Option<ThemeValue<T>>,
    pub pressed: Option<ThemeValue<T>>,
}

impl<T: Clone> State<T> {
    pub fn new(enabled: impl Into<ThemeValue<T>>) -> Self {
        Self {
            enabled: enabled.into(),
            disabled: None,
            hovered: None,
            focused: None,
            pressed: None,
        }
    }
    
    pub fn enabled(mut self, enabled: impl Into<ThemeValue<T>>) -> Self {
        self.enabled = enabled.into();
        self
    }
    
    pub fn disabled(mut self, disabled: impl Into<ThemeValue<T>>) -> Self {
        self.disabled = Some(disabled.into());
        self
    }
    
    pub fn hovered(mut self, hovered: impl Into<ThemeValue<T>>) -> Self {
        self.hovered = Some(hovered.into());
        self
    }
    
    pub fn focused(mut self, focused: impl Into<ThemeValue<T>>) -> Self {
        self.focused = Some(focused.into());
        self
    }
    
    pub fn pressed(mut self, pressed: impl Into<ThemeValue<T>>) -> Self {
        self.pressed = Some(pressed.into());
        self
    }
    
    pub fn get_enabled(&self) -> &ThemeValue<T> {
        &self.enabled
    }
    
    pub fn get_disabled(&self) -> &ThemeValue<T> {
        self.disabled.as_ref().unwrap_or(&self.enabled)
    }
    
    pub fn get_hovered(&self) -> &ThemeValue<T> {
        self.hovered.as_ref().unwrap_or(&self.enabled)
    }
    
    pub fn get_focused(&self) -> &ThemeValue<T> {
        self.focused.as_ref().unwrap_or(&self.enabled)
    }
    
    pub fn get_pressed(&self) -> &ThemeValue<T> {
        self.pressed.as_ref().unwrap_or(&self.enabled)
    }
}