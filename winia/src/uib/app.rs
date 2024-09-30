use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use skia_safe::{Color, Paint};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, Size};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{WindowAttributes, WindowId};

use ganvas::vulkan::WindowWrapper;

use crate::dpi::LogicalSize;
use crate::property::ObservableProperty;

struct AppContextInner {
    window: WindowWrapper,
}

impl Deref for AppContextInner {
    type Target = WindowWrapper;

    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

impl DerefMut for AppContextInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.window
    }
}

#[derive(Clone)]
pub struct AppContextWeak {
    inner: Weak<Mutex<AppContextInner>>,
}

impl AppContextWeak {
    pub fn upgrade(&self) -> Option<AppContext> {
        let app_context = self.inner.upgrade();
        app_context.map(|app_context|
        AppContext {
            inner: app_context
        }
        )
    }
}

#[derive(Clone)]
pub struct AppContext {
    inner: Arc<Mutex<AppContextInner>>,
}

impl AppContext {
    pub fn new(window: WindowWrapper) -> Self {
        Self {
            inner: Arc::new(Mutex::new(
                AppContextInner {
                    window
                }
            ))
        }
    }

    fn lock<F>(&mut self, f: F)
    where
        F: FnOnce(MutexGuard<AppContextInner>),
    {
        f(self.inner.lock().unwrap())
    }

    pub fn weak(&self) -> AppContextWeak {
        AppContextWeak {
            inner: Arc::downgrade(&self.inner)
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.inner.lock().unwrap().window.scale_factor() as f32
    }
}

pub struct App {
    app_context: Option<AppContext>,
    window_attributes: WindowAttributes,
    title: ObservableProperty<String>,
    max_width: ObservableProperty<usize>,
    max_height: ObservableProperty<usize>,
    preferred_size: Size,
}

impl App {
    pub fn new() -> Self {
        Self {
            app_context: None,
            window_attributes: WindowAttributes::default().with_inner_size(LogicalSize::new(200, 200)),
            title: ObservableProperty::from_value("".to_string()),
            max_width: usize::MAX.into(),
            max_height: usize::MAX.into(),
            preferred_size: LogicalSize::new(800, 600).into(),
        }
    }

    fn id(&self) -> usize {
        std::ptr::addr_of!(self) as usize
    }

    pub(crate) fn app_context(&self) -> AppContext {
        self.app_context.as_ref().unwrap().clone()
    }

    pub(crate) fn app_context_weak(&self) -> AppContextWeak {
        self.app_context().weak()
    }

    pub fn title(mut self, title: impl Into<ObservableProperty<String>>) -> Self {
        self.title = title.into();
        let app_context_weak = self.app_context_weak();
        self.title.add_specific_observer(
            move |title| {
                if let Some(app_context) = app_context_weak.upgrade() {
                    app_context.inner.lock().unwrap().window.set_title(title.as_str());
                }
            },
            0,
        );
        self
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app_context.is_none() {
            let window = event_loop.create_window(self.window_attributes.clone()).unwrap();
            self.app_context = Some(AppContext::new(WindowWrapper::wrap(window)));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                let inner_size = self.app_context().inner.lock().unwrap().inner_size();
                self.app_context().lock(|mut ap|{
                    ap.surface_resize(inner_size).unwrap();
                });

                /* First resize the opengl drawable */
                let (width, height): (u32, u32) = size.into();


                let width = width as f32 / self.app_context().scale_factor();
                let height = height as f32 / self.app_context().scale_factor();
            }
            WindowEvent::KeyboardInput { device_id, event, is_synthetic } => {
                self.app_context().inner.lock().unwrap().set_outer_position(PhysicalPosition::new(100, 100));
                println!("Keyboard input");
            }
            WindowEvent::RedrawRequested => {
                let scale_factor = self.app_context().scale_factor();
                
                self.app_context().lock(|mut ap|{
                    let surface = ap.surface();
                    let canvas = surface.canvas();
                    canvas.clear(Color::BLACK);

                    canvas.save();
                    canvas.scale((scale_factor, scale_factor));

                    canvas.draw_circle((100, 100), 100.0, Paint::default().set_color(Color::RED));

                    canvas.restore();
                    ap.present();
                });


            }
            _ => {}
        }
    }
}

pub enum UserEvent {}

fn run_app_with_event_loop(mut app: App, event_loop: EventLoop<UserEvent>) {
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app).unwrap();
}

#[cfg(target_os = "linux")]
pub fn run_app(app: App) {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    run_app_with_event_loop(app, event_loop);
}