mod window_context;
mod window_attributes;
mod window_controller;

use crate::shared::{Shared, SharedSource};
use crate::text::StyledText;
use crate::theme::color;
use crate::ui::item::{CursorMove, KeyboardInput, MeasureMode, MouseInput, MouseWheel, PointerState, TouchInput};
use crate::ui::{stack, Color, Item, StackPropsTrait};
use skia_safe::textlayout::{ParagraphStyle, TextStyle};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Instant;
pub use window_attributes::*;
pub use window_context::*;
pub use window_controller::*;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseScrollDelta, StartCause, Touch, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::WindowId;
use skiwin::gl::GlSkiaWindow;
use skiwin::SkiaWindow;
use skiwin::vulkan::VulkanSkiaWindow;

pub struct App {
    windows: HashMap<WindowId, WindowController>,
    pending_windows: Option<(
        Box<dyn FnOnce(&WindowContext) -> Item + 'static>,
        WindowAttributes,
    )>,
    pub(crate) event_loop_proxy: Option<winit::event_loop::EventLoopProxy<Event>>,
    instant: Option<Instant>,
    second_instant: Option<Instant>,
    fps_in_one_second: Vec<f32>,
    average_fps: f32,
}

impl App {
    pub fn new(
        item_generator: impl FnOnce(&WindowContext) -> Item + 'static,
        window_attributes: WindowAttributes,
    ) -> Self {
        Self {
            windows: HashMap::new(),
            pending_windows: Some((Box::new(item_generator), window_attributes)),
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
        item_generator: impl FnOnce(&WindowContext) -> Item + 'static,
        window_attributes: &WindowAttributes,
    ) {
        let window = event_loop
            .create_window(window_attributes.clone().into())
            .unwrap();
        let window_id = window.id();
        let window = Arc::new(window);
        let skia_window = VulkanSkiaWindow::new(window.clone(), Some(Box::new(|d|{
            d.properties().device_type == skiwin::vulkano::device::physical::PhysicalDeviceType::IntegratedGpu
        })));
        let event_loop_proxy = self.event_loop_proxy.as_ref().unwrap().clone();
        let window_context = WindowContext::new(
            window,
            window_attributes,
            event_loop_proxy.clone(),
        );

        let item = item_generator(&window_context);

        let children: SharedSource<Vec<Item>> = Shared::from(vec![item]);
        let stack = stack(window_context.stack_props(), children.clone());
        stack.data().measure(
            MeasureMode::Specified(window_context.window_size().0),
            MeasureMode::Specified(window_context.window_size().1),
        );
        stack.data().dispatch_layout(0.0, 0.0, window_context.window_size().0, window_context.window_size().1);
        {
            /*            stack.data().set_keyboard_input(|item, input| {
                            if keyboard::Key::Named(NamedKey::Tab) == input.key_event.logical_key {
                                if input.key_event.state.is_pressed() {
                                    item.focus_next();
                                }
                                return true;
                            }
                            false
                        });*/
        }
        self.windows.insert(
            window_id,
            /*            WindowController {
                            window_context,
                            window_attr,
                            event_loop_proxy,
                            item: stack,
                            children,
                            cursor_x: 0.0,
                            cursor_y: 0.0,
                            pressed_mouse_buttons: Vec::new(),
                            modifiers: None,
                        },*/
            WindowController::new(
                window_context,
                window_attributes,
                event_loop_proxy,
                skia_window,
                None,
                stack,
                children,
            ),
        );
    }
}

impl ApplicationHandler<Event> for App {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows.is_empty() {
            let (item_generator, window_attributes) = self.pending_windows.take().unwrap();
            self.create_window(event_loop, item_generator, &window_attributes);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if let Some(window_controller) = self.windows.get_mut(&event.window_id) {
            match event.event {
                EventType::RequestFocus(item_id) => {
                    window_controller.item.data().dispatch_focus(item_id, false);
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
                    let window = window_controller.window_context.window.deref();
                    set_window_attributes(Some(window));
                }
                EventType::NewWindow {
                    item_generator,
                    window_attributes,
                } => {
                    self.create_window(event_loop, item_generator, &window_attributes);
                }
                EventType::StartLayoutAnimation(animation) => {
                    // Start animation
                    let (width, height) = window_controller.window_context.window_size();
                    // Get the animation that should be started

                    let item = &mut window_controller.item;
                    item.data().record_frame();
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
                    let layer_controller = LayerController::new(
                        window_controller.window_context.event_loop_proxy.clone(),
                    );
                    let item =
                        item_generator(&window_controller.window_context, layer_controller.clone());
                    layer_controller.set_id(item.id());
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

        { // Request layout
            let request_layout = window_controller
                .window_context
                .request_layout
                .get()
                || window_controller.window_context.request_layout.get();
            if request_layout {
                window_controller.re_layout();
                window_controller.window_context.request_layout.set(false);
            }
        }

        { // Update shared animations
            /*            window_controller
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
                            });*/
        }

        /*        if !window_controller.window_context.starting_local_animations.lock().is_empty(){
                    let mut starting_local_animations = window_controller
                        .window_context
                        .starting_local_animations
                        .clone();
                    window_controller.item.data().record_display_parameter();
                    for animation in starting_local_animations.lock().iter_mut() {
                        (animation.inner.lock().transformation)();
                    }
                    window_controller.re_layout();
                    while let Some(animation) = starting_local_animations.lock().pop_front() {
                        animation.inner.lock().start_time = Instant::now();
                        window_controller.item.data().dispatch_animation(&animation, false);
                        window_controller
                            .window_context
                            .layout_animations
                            .lock()
                            .push(Box::new(animation));
                    }
                }*/

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
                window_controller.skia_window.resize();
                let (width, height) = window_controller.window_context.window_size();
                let item = &mut window_controller.item;
                item.data().measure(
                    MeasureMode::Specified(width),
                    MeasureMode::Specified(height),
                );
                item.data().dispatch_layout(0.0, 0.0, width, height);
                item.data().props().need_redraw.lock().need_redraw = true;
            }

            WindowEvent::KeyboardInput {
                device_id,
                event,
                is_synthetic,
            } => {
                window_controller
                    .item
                    .data()
                    .dispatch_keyboard_input(&KeyboardInput {
                        device_id,
                        key_event: event,
                        is_synthetic,
                    });
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
                    .dispatch_cursor_move(&CursorMove {
                        device_id,
                        x: window_controller.cursor_x,
                        y: window_controller.cursor_y,
                        is_left_window: false,
                    });
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
            WindowEvent::CursorLeft { device_id } => {
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
                    .dispatch_cursor_move(&CursorMove {
                        device_id,
                        x: window_controller.cursor_x,
                        y: window_controller.cursor_y,
                        is_left_window: true,
                    });
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
            WindowEvent::Ime(_ime) => {
                /*                let id = window_controller
                                    .window_context
                                    .item_focused
                                    .read(|(last, new)| {
                                        if let Some((last, id)) = new {}
                                        if let Some((last, id)) = last {
                                            if last.get() {
                                                Some(*id)
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    });
                                if let Some(id) = id {
                                    let ime_action = match ime {
                                        Ime::Enabled => ImeAction::Enabled,
                                        Ime::Preedit(preedit, range) => ImeAction::PreEdit(preedit, range),
                                        Ime::Commit(commit) => ImeAction::Commit(commit),
                                        Ime::Disabled => ImeAction::Disabled,
                                    };
                /*                    window_controller
                                        .item
                                        .data()
                                        .find_item_mut(id, &mut |item: &mut ItemData| {
                                            item.ime_input(&ime_action.clone());
                                        });*/
                                }*/
            }
            WindowEvent::RedrawRequested => {
                let background_color = window_controller
                    .window_context.theme().read()
                    .get_color(color::BACKGROUND).cloned()
                    .unwrap_or(Color::BLACK);
                window_controller.window_context.request_redraw.set(false);
                if let Some(instant) = self.instant {
                    let now = Instant::now();
                    let fps = 1.0 / (now - instant).as_secs_f32();
                    self.fps_in_one_second.push(fps);
                    self.instant = Some(now);
                } else {
                    self.instant = Some(Instant::now());
                };
                /*                let background_color = window_controller
                                    .window_context
                                    .theme
                                    .read(|theme| *theme.get_color(color::WINDOW_BACKGROUND_COLOR).unwrap());*/
                let scale_factor = window_controller.window_context.scale_factor();
                // window_controller.renderer.draw(|surface| {
                //     let canvas = surface.canvas();
                //     canvas.clear(Color::WHITE);
                //     canvas.save();
                // })
                let mut surface_arc = window_controller.skia_window.surface();
                let mut surface = surface_arc.lock();
                {
                    let canvas = surface.canvas();
                    canvas.clear(background_color.to_color4f());
                    canvas.save();
                    canvas.scale((scale_factor, scale_factor));
                }
                window_controller
                    .item
                    .data()
                    .dispatch_draw(surface.deref_mut(), 0.0, 0.0);
                let canvas = surface.canvas();

                if let Some(second_instant) = self.second_instant {
                    let now = Instant::now();
                    if (now - second_instant).as_secs_f32() >= 1.0 {
                        self.average_fps = self.fps_in_one_second.iter().sum::<f32>()
                            / self.fps_in_one_second.len() as f32;
                        self.fps_in_one_second.clear();
                        self.second_instant = Some(now);
                    }
                } else {
                    self.second_instant = Some(Instant::now());
                };
                let mut style_text = StyledText::from(self.average_fps.to_string());
                let mut text_style = TextStyle::new();
                text_style.set_font_size(16.0);
                text_style.set_color(Color::WHITE.to_skia_color());
                let para = style_text.create_paragraph(
                    &ParagraphStyle::new(),
                    &text_style,
                    300.0,
                );
                style_text.get_text_layout(&para).draw(canvas, 16.0, 16.0);
                canvas.restore();
                drop(surface);
                window_controller.skia_window.present();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                // println!("{:?}", modifiers);
                // println!("{:?}", modifiers.lshift_state());
                window_controller.modifiers = Some(modifiers);
                // window_controller.item.data().dispatch_modifiers(&modifiers);
            }
            WindowEvent::MouseWheel {
                device_id,
                delta,
                phase,
            } => {
                let scale_factor = window_controller.window_context.scale_factor();
                let state = match phase {
                    TouchPhase::Started => PointerState::Started,
                    TouchPhase::Moved => PointerState::Moved,
                    TouchPhase::Ended => PointerState::Ended,
                    TouchPhase::Cancelled => PointerState::Cancelled,
                };
                let (mouse_wheel_x, mouse_wheel_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        if let Some(modifiers) = window_controller.modifiers {
                            if modifiers.state() == ModifiersState::SHIFT {
                                (
                                    Some(MouseWheel {
                                        device_id,
                                        delta: crate::ui::item::MouseScrollDelta::LineDelta(y),
                                        state,
                                    }),
                                    None,
                                )
                            } else {
                                (
                                    None,
                                    Some(MouseWheel {
                                        device_id,
                                        delta: crate::ui::item::MouseScrollDelta::LineDelta(y),
                                        state,
                                    }),
                                )
                            }
                        } else {
                            (
                                Some(MouseWheel {
                                    device_id,
                                    delta: crate::ui::item::MouseScrollDelta::LineDelta(x),
                                    state,
                                }),
                                Some(MouseWheel {
                                    device_id,
                                    delta: crate::ui::item::MouseScrollDelta::LineDelta(y),
                                    state,
                                })
                            )
                        }
                    }
                    MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => {
                        (
                            Some(MouseWheel {
                                device_id,
                                delta: crate::ui::item::MouseScrollDelta::LogicalDelta(
                                    x as f32 / scale_factor,
                                ),
                                state,
                            }),
                            Some(MouseWheel {
                                device_id,
                                delta: crate::ui::item::MouseScrollDelta::LogicalDelta(
                                    y as f32 / scale_factor,
                                ),
                                state,
                            }),
                        )
                    }
                };
                if let Some(mouse_wheel_x) = mouse_wheel_x {
                    window_controller
                        .item
                        .data()
                        .dispatch_mouse_wheel_x(&mouse_wheel_x);
                }
                if let Some(mouse_wheel_y) = mouse_wheel_y {
                    window_controller
                        .item
                        .data()
                        .dispatch_mouse_wheel_y(&mouse_wheel_y);
                }
                let cursor_x = window_controller.cursor_x;
                let cursor_y = window_controller.cursor_y;
                window_controller
                    .item
                    .data()
                    .dispatch_cursor_move(&CursorMove {
                        device_id,
                        x: cursor_x,
                        y: cursor_y,
                        is_left_window: false,
                    });
            }
            _ => {}
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
                    running_animations.iter_mut().for_each(|animation| {
                        if animation.is_finished() {
                            animation.finish();
                        }
                    });
                    running_animations.retain(|animation| !animation.is_finished());
                });
        }

        { // Update shared animations
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

        // window_controller.renderer.recreate();

        if !closed {
            self.windows.insert(window_id, window_controller);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
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
    runtime.block_on(async {
        let event_loop = EventLoop::<Event>::with_user_event().build().unwrap();
        run_app_with_event_loop(app, event_loop);
    });
}


#[cfg(target_os = "android")]
pub fn run_app(app: App, android_app: AndroidApp) {
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_android_app(android_app)
        .build()
        .unwrap();
    run_app_with_event_loop(app.into(), event_loop);
}
