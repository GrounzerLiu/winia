mod window_context;
mod window_attributes;
mod window_controller;

use crate::shared::{Shared, SharedSource};
use crate::text::StyledText;
use crate::ui::item::{Children, ImeAction, KeyboardInput, MeasureMode, MouseWheel, PointerButton, PointerMoved};
use crate::ui::{rectangle, stack, Color, Item, RectanglePropsTrait, StackPropsTrait};
use skia_safe::textlayout::{ParagraphStyle, TextStyle};
use skiwin::vulkan::VulkanSkiaWindow;
use skiwin::SkiaWindow;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Instant;
use crossbeam_channel::{Receiver, Sender};
pub use window_attributes::*;
pub use window_context::*;
pub use window_controller::*;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{Ime, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::WindowId;

pub struct App {
    windows: HashMap<WindowId, WindowController>,
    pending_windows: Option<(
        Box<dyn FnOnce(&WindowContext) -> Item + 'static>,
        WindowAttributes,
    )>,
    pub(crate) event_loop_proxy: Option<winit::event_loop::EventLoopProxy>,
    instant: Option<Instant>,
    second_instant: Option<Instant>,
    fps_in_one_second: Vec<f32>,
    average_fps: f32,
    sender: Sender<Event>,
    receiver: Receiver<Event>
}

impl App {
    pub fn new(
        item_generator: impl FnOnce(&WindowContext) -> Item + 'static,
        window_attributes: WindowAttributes,
    ) -> Self {
            let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            windows: HashMap::new(),
            pending_windows: Some((Box::new(item_generator), window_attributes)),
            event_loop_proxy: None,
            instant: None,
            second_instant: None,
            fps_in_one_second: vec![],
            average_fps: 0.0,
            sender,
            receiver,
        }
    }

    fn create_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        item_generator: impl FnOnce(&WindowContext) -> Item + 'static,
        window_attributes: &WindowAttributes,
        sender: Sender<Event>,
    ) {
        let window = event_loop
            .create_window(window_attributes.clone().into())
            .unwrap();
        let window_id = window.id();
        let window = Arc::new(window);
        window_attributes.bind_window(window.clone());
        let skia_window = VulkanSkiaWindow::new(window.clone(), Some(Box::new(|d| {
            d.properties().device_type == skiwin::vulkano::device::physical::PhysicalDeviceType::DiscreteGpu
        })));
        let event_loop_proxy = self.event_loop_proxy.as_ref().unwrap().clone();
        let window_context = WindowContext::new(
            window,
            window_attributes,
            event_loop_proxy.clone(),
            sender
        );

        let item = item_generator(&window_context);
        let children = Children::from(vec![item]);
        let stack = stack(
            window_context.stack_props()
                          .background(rectangle(
                              window_context.rectangle_props(window_context.background_color())
                          )),
            children.clone(),
        );
        stack.data().measure(
            MeasureMode::Specified(window_context.window_size().0),
            MeasureMode::Specified(window_context.window_size().1),
        );
        stack.data().dispatch_layout(0.0, 0.0, window_context.window_size().0, window_context.window_size().1);
        self.windows.insert(
            window_id,
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

impl ApplicationHandler for App {
    fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.windows.is_empty() {
            let (item_generator, window_attributes) = self.pending_windows.take().unwrap();
            self.create_window(event_loop, item_generator, &window_attributes, self.sender.clone());
        }
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
    }

/*    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        if let Some(window_controller) = self.windows.get_mut(&event.window_id) {
            match event.event {
                EventType::RequestFocus(item_id) => {
                    window_controller.item.data().dispatch_focus(item_id, false);
                }
                EventType::RequestUpdateLayout => {
                    window_controller.window_context.request_update_layout()
                }
                EventType::StartSharedAnimation(animation) => {
                    window_controller
                        .window_context
                        .shared_animations()
                        .lock()
                        .push(animation);
                    window_controller.window_context.request_update_layout()
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
                    let window = window_controller.window_context.window().deref();
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
                        .layout_animations()
                        .lock()
                        .push(animation);
                    window_controller.window_context.request_update_layout()
                }
                EventType::NewLayer(item_generator) => {
                    let layer_controller = LayerController::new(
                        window_controller.window_context.event_loop_proxy().clone(),
                    );
                    let item =
                        item_generator(&window_controller.window_context, layer_controller.clone());
                    layer_controller.set_id(item.id());
                    window_controller.add_layer(item);
                }
                EventType::RemoveLayer(id) => {
                    window_controller.remove_layer(id);
                }
            }
        }
    }*/

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.receiver.try_recv() {
            if let Some(window_controller) = self.windows.get_mut(&event.window_id) {
                match event.event {
                    EventType::RequestFocus(item_id) => {
                        window_controller.item.data().dispatch_focus(item_id, false);
                    }
                    EventType::RequestUpdateLayout => {
                        window_controller.window_context.request_update_layout();
                    }
                    EventType::StartSharedAnimation(animation) => {
                        window_controller
                            .window_context
                            .shared_animations()
                            .lock()
                            .push(animation);
                        window_controller.window_context.request_update_layout();
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
                        let window = window_controller.window_context.window().deref();
                        set_window_attributes(window);
                    }
                    EventType::NewWindow {
                        item_generator,
                        window_attributes,
                    } => {
                        self.create_window(event_loop, item_generator, &window_attributes, self.sender.clone());
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
                            .layout_animations()
                            .lock()
                            .push(animation);
                        window_controller.window_context.request_update_layout()
                    }
                    EventType::AddLayer(item_generator) => {
                        let layer_controller = LayerController::new(
                            window_controller.window_context.event_loop_proxy().clone(),
                        );
                        let item =
                            item_generator(&window_controller.window_context, layer_controller.clone());
                        layer_controller.set_id(item.id());
                        window_controller.add_layer(item);
                    }
                    EventType::RemoveLayer(id) => {
                        window_controller.remove_layer(id);
                    }
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let window_controller_ = self.windows.remove(&window_id);
        if window_controller_.is_none() {
            return;
        }
        let mut window_controller = window_controller_.unwrap();
        let mut closed = false;

        match event {
            WindowEvent::CloseRequested => {
                if self.windows.is_empty() {
                    // if all windows are closed
                    event_loop.exit();
                } else {
                    closed = true;
                }
            }
            WindowEvent::SurfaceResized(_size) => {
                window_controller.skia_window.resize();
                let (width, height) = window_controller.window_context.window_size();
                let item = &mut window_controller.item;
                item.data().measure(
                    MeasureMode::Specified(width),
                    MeasureMode::Specified(height),
                );
                item.data().dispatch_layout(0.0, 0.0, width, height);
                item.data().props().item_updater.lock().need_redraw = true;
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
            WindowEvent::PointerButton {
                device_id,
                state,
                position,
                primary,
                button,
            } => {
                let scale_factor = window_controller.window_context.scale_factor();
                let logical_position = position.to_logical::<f32>(scale_factor as f64);
                let input = PointerButton {
                    device_id,
                    state,
                    position: logical_position,
                    primary,
                    button,
                };
                window_controller.item.data().dispatch_pointer_button(&input);
            }
            WindowEvent::PointerMoved {
                device_id,
                position,
                primary,
                source
            } => {
                let scale_factor = window_controller.window_context.scale_factor();
                let logical_position = position.to_logical::<f32>(scale_factor as f64);
                let input = PointerMoved {
                    device_id,
                    position: logical_position,
                    primary,
                    source,
                };
                window_controller.item.data().dispatch_pointer_moved(&input);
            }
/*            WindowEvent::CursorLeft { device_id } => {
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
            }*/
            WindowEvent::Ime(ime) => {
                let ime_action = match ime {
                    Ime::Enabled => ImeAction::Enabled,
                    Ime::Preedit(r1, r2) => ImeAction::PreEdit(r1, r2),
                    Ime::Commit(text) => ImeAction::Commit(text),
                    Ime::Disabled => ImeAction::Disabled,
                    Ime::DeleteSurrounding { before_bytes, after_bytes } => ImeAction::DeleteSurrounding {
                        before_bytes,
                        after_bytes,
                    }
                };
                window_controller.item.data().dispatch_ime_input(&ime_action);
            }
            WindowEvent::RedrawRequested => {
                let need_layout = window_controller
                    .window_context
                    .need_layout()
                    .get();
                window_controller.window_context.need_layout().set(false);
                if need_layout {
                    let (width, height) = window_controller.window_context.window_size();
                    window_controller
                        .item
                        .data().dispatch_measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
                    window_controller
                        .item
                        .data()
                        .dispatch_layout(0.0, 0.0, width, height);
                }
                let background_color = window_controller
                    .window_context.background_color().get();
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
                let surface_arc = window_controller.skia_window.surface();
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
/*                let mut style_text = StyledText::from(self.average_fps.to_string());
                let mut text_style = TextStyle::new();
                text_style.set_font_size(16.0);
                text_style.set_color(Color::WHITE.to_skia_color());
                let para = style_text.create_paragraph(
                    &ParagraphStyle::new(),
                    &text_style,
                    300.0,
                );
                style_text.get_text_layout(&para).draw(canvas, 16.0, 16.0);*/
                canvas.restore();
                drop(surface);
                window_controller.skia_window.present();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                // println!("{:?}", modifiers);
                // println!("{:?}", modifiers.lshift_state());
                window_controller.window_context.modifiers.set(modifiers);
                // window_controller.item.data().dispatch_modifiers(&modifiers);
            }
            WindowEvent::MouseWheel {
                device_id,
                delta,
                phase,
            } => {
                let scale_factor = window_controller.window_context.scale_factor();
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        if let Some(modifiers) = window_controller.window_context.modifiers.lock().as_ref() {
                            if modifiers.state() == ModifiersState::SHIFT {
                                window_controller
                                    .item
                                    .data().dispatch_mouse_wheel(
                                    Some(MouseWheel {
                                        device_id,
                                        delta: crate::ui::item::MouseScrollDelta::LineDelta(y),
                                        phase,
                                    }),
                                    None,
                                )
                            } else {
                                window_controller
                                    .item
                                    .data().dispatch_mouse_wheel(
                                    None,
                                    Some(MouseWheel {
                                        device_id,
                                        delta: crate::ui::item::MouseScrollDelta::LineDelta(y),
                                        phase,
                                    }),
                                )
                            }
                        } else {
                            window_controller
                                .item
                                .data().dispatch_mouse_wheel(
                                Some(MouseWheel {
                                    device_id,
                                    delta: crate::ui::item::MouseScrollDelta::LineDelta(x),
                                    phase,
                                }),
                                Some(MouseWheel {
                                    device_id,
                                    delta: crate::ui::item::MouseScrollDelta::LineDelta(y),
                                    phase,
                                }),
                            )
                        }
                    }
                    MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => {
                        window_controller
                            .item
                            .data().dispatch_mouse_wheel(
                            Some(MouseWheel {
                                device_id,
                                delta: crate::ui::item::MouseScrollDelta::LogicalDelta(
                                    x as f32 / scale_factor,
                                ),
                                phase,
                            }),
                            Some(MouseWheel {
                                device_id,
                                delta: crate::ui::item::MouseScrollDelta::LogicalDelta(
                                    y as f32 / scale_factor,
                                ),
                                phase,
                            }),
                        )
                    }
                };
            }
            _ => {}
        }

        // Animation
        {
            // Update running animations
            window_controller
                .window_context
                .layout_animations()
                .write(|running_animations| {
                    if !running_animations.is_empty() {
                        window_controller.window_context.window().request_redraw()
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
                .shared_animations()
                .write(|shared_animations| {
                    shared_animations.iter_mut().for_each(|animation| {
                        animation.update();
                    });
                    shared_animations.retain(|animation| !animation.is_finished());
                    if !shared_animations.is_empty() {
                        window_controller.window_context.window().request_redraw();
                    }
                });
        }

        // window_controller.renderer.recreate();

        if !closed {
            self.windows.insert(window_id, window_controller);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
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

fn run_app_with_event_loop(mut app: App, event_loop: EventLoop) {
    let event_loop_proxy = event_loop.create_proxy();
    app.event_loop_proxy = Some(event_loop_proxy);
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(app).unwrap();
}

#[cfg(not(target_os = "android"))]
pub fn run_app(app: App) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let event_loop = EventLoop::new().unwrap();
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
