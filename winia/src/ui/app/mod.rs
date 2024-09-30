mod app_context;
pub use app_context::*;

use std::sync::{Arc, Mutex};

use crate::core::RefClone;
use crate::dpi::LogicalSize;
use crate::property::{BoolProperty, Gettable, Property};
use skia_safe::Color;
use skiwin::vulkan::{VulkanContext, VulkanSkiaWindow};
use skiwin::SkiaWindow;
use winit::application::ApplicationHandler;
use winit::dpi::Size;
use winit::event::{ElementState, MouseButton, Touch, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{WindowAttributes, WindowId};
use crate::LockUnwrap;
use crate::ui::Item;
use crate::ui::item::{MeasureMode, MouseEvent, Orientation, PointerState, TouchEvent};

macro_rules! ref_clone {
    ($t:ty, $( $x:ident ),+) => {
        impl RefClone for $t {
            fn ref_clone(&self) -> Self {
                Self{
                    $(
                        $x: self.$x.ref_clone(),
                    )*
                }
            }
        }
    }
}

macro_rules! property_get {
    ($st:ident, $($name:ident, $ty:ty),+) =>{
        impl $st{
            $(
                pub fn $name(&self) -> $ty {
                    self.$name.ref_clone()
                }
            )*
        }
    }
}

pub struct AppProperty {
    title: Property<String>,
    min_width: Property<f32>,
    min_height: Property<f32>,
    max_width: Property<f32>,
    max_height: Property<f32>,
    maximized: BoolProperty,
}

impl Default for AppProperty {
    fn default() -> Self {
        Self {
            title: Property::from_static("Winia".to_string()),
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: f32::MAX.into(),
            max_height: f32::MAX.into(),
            maximized: false.into(),
        }
    }
}

impl AppProperty {
    pub(crate) fn apply_to_window_attributes(&self, window_attributes: &mut WindowAttributes) {
        window_attributes.title = self.title.get();
        window_attributes.min_inner_size = Some(
            Size::Logical(
                LogicalSize::new(
                    self.min_width.get().clamp(0.0, u16::MAX as f32) as f64,
                    self.min_height.get().clamp(0.0, u16::MAX as f32) as f64,
                )
            )
        );
        window_attributes.max_inner_size = Some(
            Size::Logical(
                LogicalSize::new(
                    self.max_width.get().clamp(0.0, u16::MAX as f32) as f64,
                    self.max_height.get().clamp(0.0, u16::MAX as f32) as f64,
                )
            )
        );
        window_attributes.maximized = self.maximized.get();
    }
}

ref_clone!(AppProperty, title, min_width, min_height, max_width, max_height, maximized);

property_get!(AppProperty, 
    title, Property<String>,
    min_width, Property<f32>,
    min_height, Property<f32>,
    max_width, Property<f32>,
    max_height, Property<f32>,
    maximized, BoolProperty
);

pub struct App {
    app_context: AppContext,
    app_property: Arc<Mutex<AppProperty>>,
    event_loop_proxy: Option<EventLoopProxy<UserEvent>>,
    item_generator: Option<Box<dyn FnOnce(AppContext, AppProperty) -> Item>>,
    item: Option<Item>,
    window_attributes: WindowAttributes,
    cursor_x: f32,
    cursor_y: f32,
    pressed_mouse_buttons: Vec<MouseButton>,
}

impl App {
    pub fn new(item_generator: impl FnOnce(AppContext, AppProperty) -> Item + 'static) -> Self {
        let app_property = AppProperty::default();
        let title = app_property.title.ref_clone();
        let min_width = app_property.min_width.ref_clone();
        let min_height = app_property.min_height.ref_clone();
        let max_width = app_property.max_width.ref_clone();
        let max_height = app_property.max_height.ref_clone();
        let maximized = app_property.maximized.ref_clone();
        Self {
            app_context: AppContext::new(),
            app_property: Arc::new(Mutex::new(app_property)),
            event_loop_proxy: None,
            item_generator: Some(Box::new(item_generator)),
            item: None,
            cursor_x: 0.0,
            cursor_y: 0.0,
            pressed_mouse_buttons: Vec::new(),
            window_attributes: WindowAttributes::default().with_transparent(true),
        }.title(title)
            .min_width(min_width)
            .min_height(min_height)
            .max_width(max_width)
            .max_height(max_height)
            .maximized(maximized)
    }

    fn id(&self) -> usize {
        std::ptr::addr_of!(self) as usize
    }

    pub(crate) fn window(&self, f: impl FnOnce(&mut dyn SkiaWindow)) {
        self.app_context.window(f);
    }

    pub(crate) fn set_event_loop_proxy(&mut self, event_loop_proxy: EventLoopProxy<UserEvent>) {
        self.event_loop_proxy = Some(event_loop_proxy);
    }

    /*    pub(crate) fn app_context_weak(&self) -> AppContextWeak {
            self.app_context().weak()
        }*/

    pub fn re_layout(&mut self) {
        let mut window_size: Option<(f32, f32)> = None;
        self.window(|window| {
            let scale_factor = window.scale_factor() as f32;
            let size = window.inner_size();
            let size = (size.width as f32 / scale_factor, size.height as f32 / scale_factor);
            window_size = Some(size);
        });
        if let Some(size) = window_size {
            if let Some(item) = &mut self.item {
                item.measure(Orientation::Horizontal, MeasureMode::Specified(size.0));
                item.measure(Orientation::Vertical, MeasureMode::Specified(size.1));
                item.dispatch_layout(0.0, 0.0, size.0, size.1)
            }
        }
    }

    pub fn preferred_size(mut self, width: usize, height: usize) -> Self {
        self.window_attributes.inner_size = Some(Size::Logical(LogicalSize::new(width as f64, height as f64)));
        self
    }

    pub fn title(self, title: impl Into<Property<String>>) -> Self {
        let mut title = title.into();
        self.app_property.lock_unwrap_mut(|app_property| {
            app_property.title = title.ref_clone();
        });
        let app_context = self.app_context.ref_clone();
        title.add_specific_observer(
            self.id(),
            move |title| {
                app_context.window(|window| {
                    window.set_title(title.as_str());
                })
            },
        );
        self
    }

    pub fn min_width(self, min_width: impl Into<Property<f32>>) -> Self {
        let mut min_width = min_width.into();
        self.app_property.lock_unwrap_mut(|app_property| {
            app_property.min_width = min_width.ref_clone();
        });
        let app_context = self.app_context.ref_clone();
        let app_property = Arc::downgrade(&self.app_property);
        min_width.add_specific_observer(
            self.id(),
            move |min_width| {
                app_context.window(|window| {
                    if let Some(app_property) = app_property.upgrade() {
                        let min_height = app_property.lock().unwrap().min_height.get();
                        window.set_min_inner_size(Some(
                            Size::Logical(
                                LogicalSize::new(
                                    *min_width as f64,
                                    min_height as f64,
                                )
                            )
                        ));
                    }
                })
            },
        );
        self
    }

    pub fn min_height(self, min_height: impl Into<Property<f32>>) -> Self {
        let mut min_height = min_height.into();
        self.app_property.lock_unwrap_mut(|app_property| {
            app_property.min_height = min_height.ref_clone();
        });
        let app_context = self.app_context.ref_clone();
        let app_property = Arc::downgrade(&self.app_property);
        min_height.add_specific_observer(
            self.id(),
            move |min_height| {
                app_context.window(|window| {
                    if let Some(app_property) = app_property.upgrade() {
                        let min_width = app_property.lock().unwrap().min_width.get();
                        window.set_min_inner_size(Some(
                            Size::Logical(
                                LogicalSize::new(
                                    min_width as f64,
                                    *min_height as f64,
                                )
                            )
                        ));
                    }
                })
            },
        );
        self
    }

    pub fn max_width(self, max_width: impl Into<Property<f32>>) -> Self {
        let mut max_width = max_width.into();
        self.app_property.lock_unwrap_mut(|app_property| {
            app_property.max_width = max_width.ref_clone();
        });
        let app_context = self.app_context.ref_clone();
        let app_property = Arc::downgrade(&self.app_property);
        max_width.add_specific_observer(
            self.id(),
            move |max_width| {
                app_context.window(|window| {
                    if let Some(app_property) = app_property.upgrade() {
                        let max_height = app_property.lock().unwrap().max_height.get();
                        window.set_max_inner_size(Some(
                            Size::Logical(
                                LogicalSize::new(
                                    *max_width as f64,
                                    max_height as f64,
                                )
                            )
                        ));
                    }
                })
            },
        );
        self
    }

    pub fn max_height(self, max_height: impl Into<Property<f32>>) -> Self {
        let mut max_height = max_height.into();
        self.app_property.lock_unwrap_mut(|app_property| {
            app_property.max_height = max_height.ref_clone();
        });
        let app_context = self.app_context.ref_clone();
        let app_property = Arc::downgrade(&self.app_property);
        max_height.add_specific_observer(
            self.id(),
            move |max_height| {
                app_context.window(|window| {
                    if let Some(app_property) = app_property.upgrade() {
                        let max_width = app_property.lock().unwrap().max_width.get();
                        window.set_max_inner_size(Some(
                            Size::Logical(
                                LogicalSize::new(
                                    max_width as f64,
                                    *max_height as f64,
                                )
                            )
                        ));
                    }
                })
            },
        );
        self
    }

    pub fn maximized(self, maximized: impl Into<BoolProperty>) -> Self {
        let mut maximized = maximized.into();
        self.app_property.lock_unwrap_mut(|app_property| {
            app_property.maximized = maximized.ref_clone();
        });
        let app_context = self.app_context.ref_clone();
        maximized.add_specific_observer(
            self.id(),
            move |maximized| {
                app_context.window(|window| {
                    window.set_maximized(*maximized);
                })
            });
        self
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.app_context.is_window_created() {
            let app_property = self.app_property.clone();
            app_property.lock().unwrap().apply_to_window_attributes(&mut self.window_attributes.clone().with_transparent(true));
            let window = event_loop.create_window(self.window_attributes.clone()).unwrap();
            self.app_context.window.lock().unwrap().replace(Box::new(VulkanSkiaWindow::new(window, None)));
            if let Some(item_generator) = self.item_generator.take() {
                self.item = Some(
                    item_generator(
                        self.app_context.ref_clone(),
                        self.app_property.lock().unwrap().ref_clone(),
                    )
                );
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::ReLayout => {
                self.re_layout();
                self.window(|window| {
                    window.request_redraw();
                });
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                let (width, height): (u32, u32) = size.into();
                let (mut width, mut height) = (width as f32, height as f32);
                self.window(|window| {
                    window.resize().unwrap();
                    width /= window.scale_factor() as f32;
                    height /= window.scale_factor() as f32;
                });

                if let Some(item) = &mut self.item {
                    item.measure(Orientation::Horizontal, MeasureMode::Specified(width));
                    item.measure(Orientation::Vertical, MeasureMode::Specified(height));
                    item.dispatch_layout(0.0, 0.0, width, height)
                }
            }
            WindowEvent::KeyboardInput { device_id, event, is_synthetic } => {}
            WindowEvent::MouseInput { device_id, state, button } => {
                self.window(|window| {});
                if let Some(item) = &mut self.item {
                    let event = MouseEvent {
                        device_id,
                        x: self.cursor_x,
                        y: self.cursor_y,
                        button,
                        pointer_state: match state {
                            ElementState::Pressed => PointerState::Started,
                            ElementState::Released => PointerState::Ended,
                        },
                    };
                    match state {
                        ElementState::Pressed => {
                            self.pressed_mouse_buttons.push(button);
                            item.captured_mouse_button.push(button);
                            item.mouse_input(event);
                        }
                        ElementState::Released => {
                            self.pressed_mouse_buttons.retain(|&b| b != button);
                            item.captured_mouse_button.retain(|&b| b != button);
                            item.mouse_input(event);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { device_id, position } => {
                let (x, y): (f64, f64) = position.into();
                let scale_factor = self.app_context.scale_factor();
                self.cursor_x = x as f32 / scale_factor;
                self.cursor_y = y as f32 / scale_factor;
                let pressed_mouse_buttons = self.pressed_mouse_buttons.clone();
                if let Some(item) = &mut self.item {
                    pressed_mouse_buttons.iter().for_each(|button| {
                        let event = MouseEvent {
                            device_id,
                            x: self.cursor_x,
                            y: self.cursor_y,
                            button: *button,
                            pointer_state: PointerState::Moved,
                        };
                        item.mouse_input(event);
                    });
                }
            }
            WindowEvent::Touch(Touch { device_id, phase, location, force, id }) => {
                if let Some(item) = &mut self.item {
                    let scale_factor = self.app_context.scale_factor();
                    let event = TouchEvent {
                        device_id,
                        id,
                        x: location.x as f32 / scale_factor,
                        y: location.y as f32 / scale_factor,
                        pointer_state: phase.into(),
                        force,
                    };
                    match event.pointer_state {
                        PointerState::Started => {
                            item.captured_touch_id.push((event.device_id, event.id));
                            item.touch_input(event);
                            return;
                        }
                        PointerState::Moved => {
                            if item.captured_touch_id.contains(&(event.device_id, event.id)) {
                                item.touch_input(event);
                                return;
                            }
                        }
                        PointerState::Ended | PointerState::Canceled => {
                            if item.captured_touch_id.contains(&(event.device_id, event.id)) {
                                item.captured_touch_id.retain(|&(device_id, id)| {
                                    device_id != event.device_id || id != event.id
                                });
                                item.touch_input(event);
                                return;
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.app_context.ref_clone().window(|window| {
                    let scale_factor = window.scale_factor() as f32;
                    let surface = window.surface();
                    let canvas = surface.canvas();
                    // canvas.clear(Color::BLACK);
                    canvas.save();
                    canvas.scale((scale_factor, scale_factor));

                    if let Some(item) = &mut self.item {
                        item.dispatch_draw(canvas, 0.0, 0.0);
                    }
                    canvas.restore();
                    window.present();
                });
            }
            _ => {}
        }

        {
            let animations = self.app_context.animations.clone();
            let mut animations = animations.lock().unwrap();
            for animation in animations.iter_mut() {
                (animation.inner.lock().unwrap().transformation)();
                {
                    self.app_context.current_animation.lock().unwrap().replace(animation.ref_clone());
                }
                self.re_layout();
                {
                    self.app_context.current_animation.lock().unwrap().take();
                }
            }
        }
    }
}

pub enum UserEvent {
    ReLayout,
}

fn run_app_with_event_loop(mut app: App, event_loop: EventLoop<UserEvent>) {
    let event_loop_proxy = event_loop.create_proxy();
    app.set_event_loop_proxy(event_loop_proxy);
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app).unwrap();
}

#[cfg(target_os = "linux")]
pub fn run_app(app: App) {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    run_app_with_event_loop(app, event_loop);
}