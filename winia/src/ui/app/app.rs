use parking_lot::Mutex;
use skia_safe::Color;
use skiwin::vulkan::VulkanSkiaWindow;
use skiwin::SkiaWindow;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Instant;
use skia_safe::textlayout::{ParagraphBuilder, ParagraphStyle, TextAlign};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Size};
use winit::event::{
    ElementState, Ime, Modifiers, MouseButton, MouseScrollDelta, StartCause, Touch, TouchPhase,
    WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{WindowAttributes, WindowId};

macro_rules! property_get {
    ($st:ident, $($name:ident, $fn_name:ident, $ty:ty),+) =>{
        impl $st{
            $(
                pub fn $fn_name(&self) -> $ty {
                    self.$name.clone()
                }
            )*
        }
    }
}

macro_rules! property_set {
    ($st:ident, $($name:ident: $ty:ty),+) =>{
        impl $st{
            $(
                pub fn $name(mut self, value: impl Into<$ty>) -> Self {
                    self.$name = value.into();
                    self
                }
            )*
        }
    }
}

#[derive(Clone, AsRef)]
pub struct 
WindowAttr {
    title: Shared<String>,
    preferred_size: Option<(f32, f32)>,
    min_width: Shared<f32>,
    min_height: Shared<f32>,
    max_width: Shared<f32>,
    max_height: Shared<f32>,
    maximized: SharedBool,
}

impl Into<WindowAttributes> for WindowAttr {
    fn into(self) -> WindowAttributes {
        let mut window_attributes = WindowAttributes::default();
        self.apply_to_window_attributes(&mut window_attributes);
        window_attributes
    }
}

impl Default for WindowAttr {
    fn default() -> Self {
        Self {
            title: Shared::from_static("Winia".to_string()),
            preferred_size: None,
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: (u16::MAX as f32).into(),
            max_height: (u16::MAX as f32).into(),
            maximized: false.into(),
        }
    }
}

impl WindowAttr {
    fn apply_to_window_attributes(&self, window_attributes: &mut WindowAttributes) {
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
}

property_get!(
    WindowAttr,
    title,
    get_title,
    Shared<String>,
    min_width,
    get_min_width,
    Shared<f32>,
    min_height,
    get_min_height,
    Shared<f32>,
    max_width,
    get_max_width,
    Shared<f32>,
    max_height,
    get_max_height,
    Shared<f32>,
    maximized,
    get_maximized,
    SharedBool
);

property_set!(
    WindowAttr,
    title: Shared<String>,
    min_width: Shared<f32>,
    min_height: Shared<f32>,
    max_width: Shared<f32>,
    max_height: Shared<f32>,
    maximized: SharedBool
);

pub struct WindowController {
    window_context: WindowContext,
    window_attr: Arc<Mutex<WindowAttr>>,
    event_loop_proxy: EventLoopProxy<Event>,
    // item_generator: Option<Box<dyn FnOnce(WindowContext, WindowAttr) -> Item>>,
    item: Item,
    children: Children,
    // window_attributes: WindowAttributes,
    cursor_x: f32,
    cursor_y: f32,
    pressed_mouse_buttons: Vec<MouseButton>,
    modifiers: Option<Modifiers>,
}

impl WindowController {
    pub fn re_layout(&mut self) {
        let (width, height) = self.window_context.window_size();
        self.item.data().measure(
            MeasureMode::Specified(width),
            MeasureMode::Specified(height),
        );
        self.item.data().dispatch_layout(0.0, 0.0, width, height)
    }

    pub fn add_layer(&mut self, item: Item) {
        self.children.add(item);
    }

    pub fn remove_layer(&mut self, id: usize) {
        self.children.remove_by_id(id);
    }
}

pub struct App {
    windows: HashMap<WindowId, WindowController>,
    pending_windows: Option<(
        Box<dyn FnOnce(&WindowContext, &WindowAttr) -> Item + 'static>,
        WindowAttr,
    )>,
    pub(crate) event_loop_proxy: Option<EventLoopProxy<Event>>,
    instant: Option<Instant>,
    second_instant: Option<Instant>,
    fps_in_one_second: Vec<f32>,
    average_fps: f32,
}

impl App {
    pub fn new(
        item_generator: impl FnOnce(&WindowContext, &WindowAttr) -> Item + 'static,
        window_attr: WindowAttr,
    ) -> Self {
        Self {
            windows: HashMap::new(),
            pending_windows: Some((Box::new(item_generator), window_attr)),
            event_loop_proxy: None,
            instant: None,
            second_instant: None,
            fps_in_one_second: vec![],
            average_fps: 0.0,
        }
    }

    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        item_generator: impl FnOnce(&WindowContext, &WindowAttr) -> Item + 'static,
        window_attr: WindowAttr,
    ) {
        let window = event_loop
            .create_window(window_attr.clone().into())
            .unwrap();
        let window_id = window.id();
        let event_loop_proxy = self.event_loop_proxy.as_ref().unwrap().clone();
        let window_context =
            WindowContext::new(VulkanSkiaWindow::new(window, None), event_loop_proxy.clone());
        window_context.window.lock().resize();
        let item = item_generator(&window_context, &window_attr)
            .size(crate::ui::item::Size::Fill, crate::ui::item::Size::Fill);

        let children = Children::new();
        let stack = window_context.stack(children.clone() + item).item();
        {
            let theme_ = window_context.theme();
            let theme = theme_.lock();
            stack.data().dispatch_apply_theme(theme.deref());
        }
        self.windows.insert(
            window_id,
            WindowController {
                window_context,
                window_attr: Arc::new(Mutex::new(window_attr)),
                event_loop_proxy,
                item: stack,
                children,
                cursor_x: 0.0,
                cursor_y: 0.0,
                pressed_mouse_buttons: Vec::new(),
                modifiers: None,
            },
        );
    }
}

impl ApplicationHandler<Event> for App {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows.is_empty() {
            let (item_generator, window_attr) = self.pending_windows.take().unwrap();
            self.create_window(event_loop, item_generator, window_attr);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if let Some(window_controller) = self.windows.get_mut(&event.window_id) {
            match event.event {
                EventType::RequestFocus => {
                    let is_focus_changed = window_controller
                        .window_context
                        .focus_changed_items
                        .read(|focus_changed_items| focus_changed_items.is_empty());
                    if is_focus_changed {
                        window_controller.item.data().dispatch_focus();
                    }
                    window_controller.window_context.focus_changed_items.write(
                        |focus_changed_items| {
                            focus_changed_items.clear();
                        },
                    );
                }
                EventType::RequestLayout => {
                    window_controller.window_context.request_layout();
                }
                EventType::RequestRedraw => {
                    window_controller.window_context.request_redraw();
                }
                EventType::StartSharedAnimation(animation) => {
                    window_controller
                        .window_context
                        .shared_animations
                        .lock()
                        .push(animation);
                    window_controller.window_context.request_redraw();
                }
                EventType::Timer(_id) => {
                    // let timers = window_controller.window_context.timers.value();
                    // if let Some(timer) = timers.iter().find(|timer| timer.id == id) {
                    //     window_controller.item.data().dispatch_timer(id);
                    // }
                    // window_controller.window_context
                    //     .timers
                    //     .write(|timers| timers.retain(|timer| timer.id != id));
                }
                EventType::SetWindowAttribute(set_window_attributes) => {
                    let window = window_controller.window_context.window.lock();
                    set_window_attributes(Some(window.deref()));
                }
                EventType::NewWindow {
                    item_generator,
                    window_attr,
                } => {
                    self.create_window(event_loop, item_generator, window_attr);
                }
                EventType::StartLayoutAnimation(animation) => {
                    // Start animation
                    let (width, height) = window_controller.window_context.window_size();
                    // Get the animation that should be started

                    let item = &mut window_controller.item;
                    item.data().record_display_parameter();
                    (animation.inner.lock().transformation)();
                    item.data().measure(
                        MeasureMode::Specified(width),
                        MeasureMode::Specified(height),
                    );
                    item.data().dispatch_layout(0.0, 0.0, width, height);
                    animation.inner.lock().start_time = Instant::now();
                    item.data().dispatch_animation(&animation, false);
                    window_controller
                        .window_context
                        .layout_animations
                        .lock()
                        .push(animation);
                }
                EventType::NewLayer(item_generator) => {
                    let item = item_generator(
                        &window_controller.window_context,
                        window_controller.window_attr.lock().deref(),
                    );
                    window_controller.add_layer(item);
                    window_controller.window_context.request_layout()
                }
                EventType::RemoveLayer(id) => {
                    window_controller.remove_layer(id);
                    window_controller.window_context.request_layout()
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let window_controller_ = self.windows.remove(&window_id);
        if window_controller_.is_none() {
            return;
        }
        let mut window_controller = window_controller_.unwrap();
        let mut closed = false;

        {
            let request_layout = window_controller
                .window_context
                .request_layout
                .read(|request_layout| *request_layout)
                || window_controller.window_context.request_layout.get();
            if request_layout {
                window_controller.re_layout();
                window_controller.window_context.request_layout.set(false);
            }
        }

        // Animation
        {
            // Update running animations
            window_controller
                .window_context
                .layout_animations
                .write(|running_animations| {
                    if !running_animations.is_empty() {
                        window_controller.window_context.request_redraw()
                    }
                    running_animations.retain(|animation| !animation.is_finished());
                });
        }

        {
            window_controller
                .window_context
                .shared_animations
                .write(|shared_animations| {
                    shared_animations.iter_mut().for_each(|animation| {
                        animation.update();
                    });
                    shared_animations.retain(|animation| !animation.is_finished());
                    if !shared_animations.is_empty() {
                        window_controller.window_context.request_redraw()
                    }
                });
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.windows.is_empty() {
                    // if all windows are closed
                    event_loop.exit();
                } else {
                    closed = true;
                }
            }
            WindowEvent::Resized(_size) => {
                window_controller
                    .window_context
                    .window
                    .lock()
                    .resize()
                    .unwrap();
                let (width, height) = window_controller.window_context.window_size();
                let item = &mut window_controller.item;
                item.data().measure(
                    MeasureMode::Specified(width),
                    MeasureMode::Specified(height),
                );
                item.data().dispatch_layout(0.0, 0.0, width, height)
            }

            WindowEvent::KeyboardInput {
                device_id,
                event,
                is_synthetic,
            } => {
                window_controller.item.data().dispatch_keyboard_input(
                    &KeyboardInput {
                        device_id,
                        key_event: event,
                        is_synthetic,
                    }
                );
            }
            WindowEvent::MouseInput {
                device_id,
                state,
                button,
            } => {
                let event = MouseInput {
                    device_id,
                    x: window_controller.cursor_x,
                    y: window_controller.cursor_y,
                    button,
                    pointer_state: match state {
                        ElementState::Pressed => PointerState::Started,
                        ElementState::Released => PointerState::Ended,
                    },
                };
                match state {
                    ElementState::Pressed => {
                        window_controller.pressed_mouse_buttons.push(button);
                        window_controller.item.data().dispatch_mouse_input(&event);
                    }
                    ElementState::Released => {
                        window_controller
                            .pressed_mouse_buttons
                            .retain(|&b| b != button);
                        window_controller.item.data().dispatch_mouse_input(&event);
                    }
                }
            }
            WindowEvent::CursorMoved {
                device_id,
                position,
            } => {
                let (x, y): (f64, f64) = position.into();
                let scale_factor = window_controller.window_context.scale_factor();
                window_controller.cursor_x = x as f32 / scale_factor;
                window_controller.cursor_y = y as f32 / scale_factor;
                window_controller
                    .window_context
                    .cursor_position
                    .set((window_controller.cursor_x, window_controller.cursor_y));
                let pressed_mouse_buttons = window_controller.pressed_mouse_buttons.clone();
                window_controller
                    .item
                    .data()
                    .dispatch_cursor_move(
                        &CursorMove {
                            device_id,
                            x: window_controller.cursor_x,
                            y: window_controller.cursor_y,
                            is_left_window: false,
                        }
                    );
                pressed_mouse_buttons.iter().for_each(|button| {
                    let event = MouseInput {
                        device_id,
                        x: window_controller.cursor_x,
                        y: window_controller.cursor_y,
                        button: *button,
                        pointer_state: PointerState::Moved,
                    };
                    window_controller.item.data().dispatch_mouse_input(&event);
                });
            }
            WindowEvent::CursorLeft {
                device_id
            } => {
                // let event = MouseInput {
                //     device_id,
                //     x: window_controller.cursor_x,
                //     y: window_controller.cursor_y,
                //     button: MouseButton::Left,
                //     pointer_state: PointerState::Cancelled,
                // };
                // window_controller.item.data().dispatch_mouse_input(event);
                window_controller
                    .item
                    .data()
                    .dispatch_cursor_move(
                        &CursorMove {
                            device_id,
                            x: window_controller.cursor_x,
                            y: window_controller.cursor_y,
                            is_left_window: true,
                        }
                    );
            }
            WindowEvent::Touch(Touch {
                                   device_id,
                                   phase,
                                   location,
                                   force,
                                   id,
                               }) => {
                let scale_factor = window_controller.window_context.scale_factor();
                let event = TouchInput {
                    device_id,
                    id,
                    x: location.x as f32 / scale_factor,
                    y: location.y as f32 / scale_factor,
                    pointer_state: phase.into(),
                    force,
                };
                window_controller.item.data().dispatch_touch_input(&event);
            }
            WindowEvent::Ime(ime) => {
                let id = window_controller
                    .window_context
                    .focused_property
                    .read(|f| f.as_ref().map(|(_, id)| *id));
                if let Some(id) = id {
                    let ime_action = match ime {
                        Ime::Enabled => ImeAction::Enabled,
                        Ime::Preedit(preedit, range) => ImeAction::PreEdit(preedit, range),
                        Ime::Commit(commit) => ImeAction::Commit(commit),
                        Ime::Disabled => ImeAction::Disabled,
                    };
                    window_controller
                        .item
                        .data()
                        .find_item_mut(id, &mut |item: &mut ItemData| {
                            item.ime_input(&ime_action.clone());
                        });
                }
            }
            WindowEvent::RedrawRequested => {
                window_controller.window_context.request_redraw.set(false);
                if let Some(instant) = self.instant {
                    let now = Instant::now();
                    let fps = 1.0 / (now - instant).as_secs_f32();
                    self.fps_in_one_second.push(fps);
                    self.instant = Some(now);
                } else {
                    self.instant = Some(Instant::now());
                };
                let background_color = window_controller
                    .window_context
                    .theme
                    .read(|theme| theme.get_color(color::WINDOW_BACKGROUND_COLOR))
                    .unwrap_or(Color::WHITE);
                let scale_factor = window_controller.window_context.scale_factor();
                let window = window_controller.window_context.window.lock();
                let surface_ref = window.surface();
                drop(window);
                {
                    let mut surface = surface_ref.lock();

                    {
                        let canvas = surface.canvas();
                        canvas.clear(background_color);
                        canvas.save();
                        canvas.scale((scale_factor, scale_factor));
                    }

                    window_controller
                        .item
                        .data()
                        .dispatch_draw(surface.deref_mut(), 0.0, 0.0);

                    let canvas = surface.canvas();

                    let text_color = window_controller
                        .window_context
                        .theme
                        .read(|theme| theme.get_color(color::ON_SURFACE))
                        .unwrap_or(Color::WHITE);
                    
                    if !self.fps_in_one_second.is_empty() {
                        let fps = self.fps_in_one_second.iter().sum::<f32>() / self.fps_in_one_second.len() as f32;
                        let now = Instant::now();
                        if let Some(instant) = self.second_instant {
                            if now - instant > std::time::Duration::from_secs(1) {
                                self.fps_in_one_second.clear();
                                self.second_instant = Some(now);
                                self.average_fps = fps;
                            }
                        } else {
                            self.second_instant = Some(now);
                        }
                    }

                    let fps_text = format!("FPS: {:.2}", self.average_fps);
                    let mut styled_text = StyledText::from(fps_text);
                    styled_text.set_style(
                        TextStyle::TextColor(text_color),
                        0..styled_text.len(),
                        false
                    );
                    styled_text.set_style(
                        TextStyle::FontSize(12.0),
                        0..styled_text.len(),
                        false
                    );

                    let mut paragraph_style = ParagraphStyle::default();
                    paragraph_style.set_text_align(TextAlign::Justify);

                    let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection());

                    create_segments(&styled_text, &(0..styled_text.len()), &skia_safe::textlayout::TextStyle::default())
                        .iter()
                        .for_each(|style_segment| {
                            paragraph_builder.add_style_segment(style_segment);
                        });

                    let mut paragraph = paragraph_builder.build();
                    paragraph.layout(100.0);
                    paragraph.paint(canvas, (10.0, 10.0));

                    canvas.restore();
                }

                window_controller.window_context.window.lock().present();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                //println!("{:?}", modifiers);
                // println!("{:?}", modifiers.lshift_state());
                window_controller.modifiers = Some(modifiers);
            }
            WindowEvent::MouseWheel {
                device_id,
                delta,
                phase,
            } => {
                let scale_factor = window_controller.window_context.scale_factor();
                window_controller
                    .item
                    .data()
                    .dispatch_mouse_wheel(&MouseWheel {
                        device_id,
                        delta: match delta {
                            MouseScrollDelta::LineDelta(x, y) => {
                                if let Some(modifiers) = window_controller.modifiers {
                                    if modifiers.state() == ModifiersState::SHIFT {
                                        crate::ui::item::MouseScrollDelta::LineDelta(y, 0.0)
                                    } else {
                                        crate::ui::item::MouseScrollDelta::LineDelta(0.0, y)
                                    }
                                } else {
                                    crate::ui::item::MouseScrollDelta::LineDelta(x, y)
                                }
                            }
                            MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => {
                                crate::ui::item::MouseScrollDelta::LogicalDelta(
                                    x as f32 / scale_factor,
                                    y as f32 / scale_factor,
                                )
                            }
                        },
                        state: match phase {
                            TouchPhase::Started => PointerState::Started,
                            TouchPhase::Moved => PointerState::Moved,
                            TouchPhase::Ended => PointerState::Ended,
                            TouchPhase::Cancelled => PointerState::Cancelled,
                        },
                    });
                let cursor_x = window_controller.cursor_x;
                let cursor_y = window_controller.cursor_y;
                window_controller
                    .item
                    .data()
                    .dispatch_cursor_move(
                        &CursorMove {
                            device_id,
                            x: cursor_x,
                            y: cursor_y,
                            is_left_window: false,
                        }
                    );
            }
            _ => {}
        }

        if !closed {
            self.windows.insert(window_id, window_controller);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Timer
        // {
        //     let timers = self.window_context.timers.read(|timers| timers.clone());
        //     if !timers.is_empty() {
        //         let mut most_recent_timer = timers[0].start_time + timers[0].duration;
        //         let now = Instant::now();
        //         if let Some(item) = &mut self.item {
        //             for timer in timers.iter() {
        //                 if timer.start_time + timer.duration < most_recent_timer {
        //                     most_recent_timer = timer.start_time + timer.duration;
        //                 }
        //                 if now - timer.start_time >= timer.duration {
        //                     item.data().dispatch_timer(timer.id);
        //                 }
        //             }
        //         }
        //         if most_recent_timer > now {
        //             event_loop.set_control_flow(ControlFlow::WaitUntil(most_recent_timer));
        //         }
        //         self.window_context
        //             .timers
        //             .write(|timers| timers.retain(|timer| now - timer.start_time < timer.duration));
        //     }
        // }
    }
}

fn run_app_with_event_loop(mut app: App, event_loop: EventLoop<Event>) {
    let event_loop_proxy = event_loop.create_proxy();
    app.event_loop_proxy = Some(event_loop_proxy);
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app).unwrap();
}

#[cfg(not(target_os = "android"))]
pub fn run_app(app: App) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
/*    let event_loop = EventLoop::<Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    run_app_with_event_loop(app, event_loop);*/
    runtime.block_on(async {
        let event_loop = EventLoop::<Event>::with_user_event()
            .with_any_thread(true)
            .build()
            .unwrap();
        run_app_with_event_loop(app, event_loop);
    });
}

use crate::shared::{Children, Gettable, Settable, Shared, SharedBool};
use crate::ui::app::{Event, EventType, WindowContext};
use crate::ui::item::{CursorMove, ImeAction, ItemData, KeyboardInput, MeasureMode, MouseInput, MouseWheel, PointerState, TouchInput};
use crate::ui::theme::color;
use crate::ui::{theme, Item};
use skiwin::cpu::SoftSkiaWindow;
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::wayland::EventLoopBuilderExtWayland;
use proc_macro::AsRef;
use crate::text::{create_segments, font_collection, AddStyleSegment, StyledText, TextStyle};
use crate::ui::layout::StackExt;

#[cfg(target_os = "android")]
pub fn run_app(app: App, android_app: AndroidApp) {
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_android_app(android_app)
        .build()
        .unwrap();
    run_app_with_event_loop(app.into(), event_loop);
}
