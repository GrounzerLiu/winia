use crate::app::{Event, WindowAttributes, WindowContext};
use crate::shared::SharedSource;
use crate::ui::item::{ButtonSourceHashWrapper, MeasureMode};
use crate::ui::Item;
use skiwin::SkiaWindow;
use winit::event::{Modifiers, MouseButton};
use winit::event_loop::EventLoopProxy;

pub struct WindowController {
    pub window_context: WindowContext,
    pub window_attributes: WindowAttributes,
    pub event_loop_proxy: EventLoopProxy,
    pub skia_window: Box<dyn SkiaWindow>,
    pub item_generator: Option<Box<dyn FnOnce(WindowContext, WindowAttributes) -> Item>>,
    pub item: Item,
    pub children: SharedSource<Vec<Item>>,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub modifiers: Option<Modifiers>,
}

impl WindowController {
    pub fn new(
        window_context: WindowContext,
        window_attributes: &WindowAttributes,
        event_loop_proxy: EventLoopProxy,
        skia_window: impl SkiaWindow + 'static,
        item_generator: Option<Box<dyn FnOnce(WindowContext, WindowAttributes) -> Item>>,
        item: Item,
        children: SharedSource<Vec<Item>>,
    ) -> Self {
        Self {
            window_context,
            window_attributes: window_attributes.clone(),
            event_loop_proxy,
            skia_window: Box::new(skia_window),
            item_generator,
            item,
            children,
            cursor_x: 0.0,
            cursor_y: 0.0,
            modifiers: None,
        }
    }
    pub fn re_layout(&mut self) {
        let (width, height) = self.window_context.window_size();
        self.item.data().measure(
            MeasureMode::Specified(width),
            MeasureMode::Specified(height),
        );
        self.item.data().dispatch_layout(0.0, 0.0, width, height)
    }

    pub fn add_layer(&mut self, item: Item) {
        self.children.add_item(item);
    }

    pub fn remove_layer(&mut self, id: u32) {
        self.children.remove_by_id(id);
    }
}
