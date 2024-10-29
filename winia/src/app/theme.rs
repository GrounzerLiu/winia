use std::collections::HashMap;
use skia_safe::Color;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ThemeColor{
    Primary,
    OnPrimary,
    PrimaryContainer,
    OnPrimaryContainer,
    Secondary,
    OnSecondary,
    SecondaryContainer,
    OnSecondaryContainer,
    Tertiary,
    OnTertiary,
    TertiaryContainer,
    OnTertiaryContainer,
    Error,
    OnError,
    ErrorContainer,
    OnErrorContainer,
    Background,
    OnBackground,
    Surface,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    Outline,
    OutlineVariant,
    Shadow,
    Scrim,
    InverseSurface,
    InverseOnSurface,
    InversePrimary,
    
    WindowBackground,
    
    Custom(String),
}

impl From<&str> for ThemeColor{
    fn from(value: &str) -> Self {
        ThemeColor::Custom(value.to_string())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ThemeDimension{
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ThemeBool{
}

#[derive(Clone)]
pub struct Style {
    colors: HashMap<String, Color>,
    dimensions: HashMap<String, f32>,
    bools: HashMap<String, bool>,
}

impl Default for Style {
    fn default() -> Self {
        Self::new()
    }
}

impl Style {
    pub fn new() -> Self{
        Self{
            colors: HashMap::new(),
            dimensions: HashMap::new(),
            bools: HashMap::new(),
        }
    }

    pub fn set_color(mut self, id: impl Into<String>, color: Color) -> Self{
        self.colors.insert(id.into(), color);
        self
    }

    pub fn set_dimension(mut self, id: impl Into<String>, dimension: f32) -> Self{
        self.dimensions.insert(id.into(), dimension);
        self
    }

    pub fn set_bool(mut self, id: impl Into<String>, boolean: bool) -> Self{
        self.bools.insert(id.into(), boolean);
        self
    }

    pub fn get_color(&self, id: impl Into<String>) -> Option<Color>{
        self.colors.get(&id.into()).cloned()
    }

    pub fn get_dimension(&self, id: impl Into<String>) -> Option<f32>{
        self.dimensions.get(&id.into()).cloned()
    }

    pub fn get_bool(&self, id: impl Into<String>) -> Option<bool>{
        self.bools.get(&id.into()).cloned()
    }
}

pub struct Theme{
    colors: HashMap<ThemeColor, Color>,
    dimensions: HashMap<ThemeDimension, f32>,
    bools: HashMap<ThemeBool, bool>,
    styles: HashMap<String, Style>,
    is_dark: bool,
}

impl Theme{
    pub fn new(is_dark:bool) -> Self{
        Self{
            colors: HashMap::new(),
            dimensions: HashMap::new(),
            bools: HashMap::new(),
            styles: HashMap::new(),
            is_dark,
        }
    }

    pub fn is_dark(&self) -> bool{
        self.is_dark
    }

    pub fn get_color(&self, id: impl Into<ThemeColor>) -> Color{
        // *self.colors.get(&id).expect(match id {
        //     ThemeColor::Custom(color)=>{
        //         &format!("The color {} is not defined in the theme", color)
        //     }
        //     _=>{
        //         "The theme must derive from material theme. If you want to use a custom theme, use custom components"
        //     }
        // })
        *self.colors.get(&id.into()).unwrap_or(&Color::TRANSPARENT)
    }

    pub fn get_dimension(&self, id: ThemeDimension) -> f32{
        *self.dimensions.get(&id).unwrap_or(&0.0)
    }

    pub fn get_bool(&self, id: ThemeBool) -> bool{
        *self.bools.get(&id).unwrap_or(&false)
    }
    
    pub fn get_style(&self, id: impl Into<String>) -> Option<Style>{
        self.styles.get(&id.into()).cloned()
    }

    pub fn set_color(mut self, id: ThemeColor, color: Color) -> Self{
        self.colors.insert(id, color);
        self
    }

    pub fn set_dimension(mut self, id: ThemeDimension, dimension: f32) -> Self{
        self.dimensions.insert(id, dimension);
        self
    }

    pub fn set_bool(mut self, id: ThemeBool, boolean: bool) -> Self{
        self.bools.insert(id, boolean);
        self
    }
    
    pub fn set_style(mut self, id: impl Into<String>, style: Style) -> Self{
        self.styles.insert(id.into(), style);
        self
    }
}