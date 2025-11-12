use winit::dpi::{LogicalSize, Size};
use crate::shared::{SharedDerived, SharedDerivedBool, SharedDerivedString};

#[derive(Clone)]
pub struct WindowAttributes {
    title: SharedDerived<String>,
    preferred_size: Option<(f32, f32)>,
    min_width: SharedDerived<f32>,
    min_height: SharedDerived<f32>,
    max_width: SharedDerived<f32>,
    max_height: SharedDerived<f32>,
    maximized: SharedDerivedBool,
}

impl Into<winit::window::WindowAttributes> for WindowAttributes {
    fn into(self) -> winit::window::WindowAttributes {
        let mut window_attributes = winit::window::WindowAttributes::default();
        self.apply_to_window_attributes(&mut window_attributes);
        window_attributes
    }
}

impl Default for WindowAttributes {
    fn default() -> Self {
        Self {
            title: "Winia".to_string().into(),
            preferred_size: None,
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: (u16::MAX as f32).into(),
            max_height: (u16::MAX as f32).into(),
            maximized: false.into(),
        }
    }
}

impl WindowAttributes {
    fn apply_to_window_attributes(&self, window_attributes: &mut winit::window::WindowAttributes) {
        if let Some((width, height)) = self.preferred_size {
            window_attributes.inner_size = Some(Size::Logical(LogicalSize::new(
                width.clamp(0.0, u16::MAX as f32) as f64,
                height.clamp(0.0, u16::MAX as f32) as f64,
            )));
        }
        window_attributes.title = self.title.get();
        window_attributes.min_inner_size = Some(Size::Logical(LogicalSize::new(
            self.min_width.get().clamp(0.0, u16::MAX as f32) as f64,
            self.min_height.get().clamp(0.0, u16::MAX as f32) as f64,
        )));
        window_attributes.max_inner_size = Some(Size::Logical(LogicalSize::new(
            self.max_width.get().clamp(0.0, u16::MAX as f32) as f64,
            self.max_height.get().clamp(0.0, u16::MAX as f32) as f64,
        )));
        window_attributes.maximized = self.maximized.get();
    }

    pub fn preferred_size(mut self, width: f32, height: f32) -> Self {
        self.preferred_size = Some((width, height));
        self
    }
    
    pub fn title(mut self, title: impl Into<SharedDerivedString>) -> Self {
        self.title = title.into().into();
        self
    }
}