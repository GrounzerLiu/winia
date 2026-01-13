use std::sync::Arc;
use clonelet::clone;
use getset::Getters;
use winit::{dpi::{LogicalSize, Size}, window::Window};
use crate::core::next_id;
use crate::shared::{SharedDerived, SharedDerivedBool, SharedDerivedString};

#[derive(Clone, Getters)]
pub struct WindowAttributes {
    #[get = "pub with_prefix"]
    title: SharedDerived<String>,
    #[get = "pub with_prefix"]
    preferred_size: Option<(f32, f32)>,
    #[get = "pub with_prefix"]
    min_width: SharedDerived<f32>,
    #[get = "pub with_prefix"]
    min_height: SharedDerived<f32>,
    #[get = "pub with_prefix"]
    max_width: SharedDerived<f32>,
    #[get = "pub with_prefix"]
    max_height: SharedDerived<f32>,
    #[get = "pub with_prefix"]
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
            window_attributes.surface_size = Some(Size::Logical(LogicalSize::new(
                width.clamp(0.0, u16::MAX as f32) as f64,
                height.clamp(0.0, u16::MAX as f32) as f64,
            )));
        }
        window_attributes.title = self.title.get();
        window_attributes.min_surface_size = Some(Size::Logical(LogicalSize::new(
            self.min_width.get().clamp(0.0, u16::MAX as f32) as f64,
            self.min_height.get().clamp(0.0, u16::MAX as f32) as f64,
        )));
        window_attributes.max_surface_size = Some(Size::Logical(LogicalSize::new(
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

    pub fn maximized(mut self, maximized: impl Into<SharedDerivedBool>) -> Self {
        self.maximized = maximized.into();
        self
    }

    pub fn bind_window(&self, window: Arc<Box<dyn Window>>) {
        let window_weak = Arc::downgrade(&window);
        self.title.subscribe(
            next_id(),
            {
                clone!(window_weak, self.title);
                move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.set_title(&title.get());
                    }
                }
            }
        );
        self.maximized.subscribe(
            next_id(),
            {
                clone!(window_weak, self.maximized);
                move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.set_maximized(maximized.get());
                    }
                }
            }
        );
    }
}