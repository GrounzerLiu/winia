/*use std::sync::{Arc, Mutex, Weak};
use winit::event_loop::{EventLoopClosed, EventLoopProxy};
use winit::window::Window;
use crate::app::UserEvent;

pub struct AppProxy {
    window: Weak<Mutex<Window>>,
    event_loop_proxy: EventLoopProxy<UserEvent>,
}

impl AppProxy {
    pub fn new(window: &Arc<Mutex<Window>>, event_loop_proxy: &EventLoopProxy<UserEvent>) -> Self {
        Self {
            window: Arc::downgrade(window),
            event_loop_proxy: event_loop_proxy.clone(),
        }
    }
    
    pub fn is_window_alive(&self) -> bool {
        self.window.strong_count() > 0
    }

    pub fn request_redraw(&self) {
        if let Some(window) = self.window.upgrade() {
            if let Ok(window) = window.lock() {
                window.request_redraw();
            }
        }
    }
    
    pub fn send_event(&self, event: UserEvent) -> Result<(), EventLoopClosed<UserEvent>> {
        self.event_loop_proxy.send_event(event)
    }
}*/