/*// use winapi::shared::windef::HWND__;
use std::{ffi::CString, num::NonZeroU32};
use std::rc::Rc;
use std::sync::{mpsc, Mutex};
use std::time::Instant;

use gl::types::*;
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder,
        PossiblyCurrentContext,
    },
    display::{GetGlDisplay, GlDisplay},
    prelude::GlSurface,
    surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin::context::NotCurrentGlContext;
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{Color, ColorType, gpu::{self, backend_render_targets, gl::FramebufferInfo, SurfaceOrigin}, Surface};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window},
};
use winit::dpi::{LogicalPosition, PhysicalPosition, PhysicalSize};
use winit::event::Ime;
use winit::event_loop::{EventLoopBuilder};
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::window::WindowAttributes;

use crate::app::{SharedApp, Theme, UserEvent};
use crate::Let;
use crate::uib::{ButtonState, ImeAction, Item, MeasureMode, MouseEvent, Orientation};
use crate::widgetb::{Rectangle, RectangleExt};

fn layout_item(item: &mut Item, width: f32, height: f32) {
    item.let_mut(|item| {
        item.measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
        item.layout(0.0, 0.0);
    });
}

fn run<U>(event_loop: EventLoop<UserEvent>, window_builder: WindowAttributes, ui_generate: Box<U>, theme: Theme)
    where U: Fn(SharedApp) -> Item
{
    let mut env = None;

    let mut app = SharedApp::new(event_loop.create_proxy(), theme);
    let mut uib = Rc::new(Mutex::new(app.rectangle().item()));

    let mut cursor_position = LogicalPosition::new(0.0_f32, 0.0_f32);
    let mut pressed_mouse_button = Vec::new();
    
    let mut orientation:Option<Orientation> = None;

    event_loop.run(move |event, elwt| {
        if let Event::Resumed = event {
            if env.is_none() {
                let (inited_env, window) = init_env(elwt, window_builder.clone());
                env = Some(inited_env);
                window.set_transparent(true);
                app.set_window(window);
                uib = Rc::new(Mutex::new(ui_generate(app.clone())));
                app.uib = Some(Rc::downgrade(&uib));
            }
            if env.is_none() {
                panic!("Env is not initialized");
            }
        }

        match event {
            Event::UserEvent(user_event) => {
                match user_event {
                    UserEvent::Empty => {}
                    UserEvent::StartAnimation => {
                        let has_animation_detecting_diff = app.animations.lock().unwrap().iter().any(|animation| animation.lock().unwrap().is_detecting_diff);
                        if has_animation_detecting_diff {
                            uib.lock().unwrap().record_display_parameter_start();
                            for animation in app.animations.clone().lock().unwrap().iter_mut() {
                                if animation.lock().unwrap().is_detecting_diff{
                                    (animation.lock().unwrap().layout_transition)();
                                }
                                layout_item(&mut uib.lock().unwrap(), app.content_width(), app.content_height());
                                app.layout_done();
                                uib.lock().unwrap().record_display_parameter_end(animation.weak());
                                animation.lock().unwrap().is_detecting_diff = false;
                            }
                        }
                    }
                    UserEvent::TimerExpired(_, _) => {}
                }
            }
            Event::WindowEvent { window_id: _window_id, event } => {
                match event {
                    WindowEvent::CloseRequested => {
                        elwt.exit();
                    }
                    WindowEvent::Moved(_) => {
                        app.request_redraw();
                    }
                    WindowEvent::Resized(physical_size) => {
                        let env = env.as_mut().unwrap();
                        env.surface = create_surface(
                            app.window().unwrap().inner_size(),
                            env.fb_info,
                            &mut env.gr_context,
                            env.num_samples,
                            env.stencil_size,
                        );
                        /* First resize the opengl drawable */
                        let (width, height): (u32, u32) = physical_size.into();

                        env.gl_surface.resize(
                            &env.gl_context,
                            NonZeroU32::new(width.max(1)).unwrap(),
                            NonZeroU32::new(height.max(1)).unwrap(),
                        );

                        let width = width as f32 / app.scale_factor();
                        let height = height as f32 / app.scale_factor();

                        uib.lock().unwrap().let_mut(|uib| {
                            uib.measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
                            uib.layout(0.0, 0.0);
                            if orientation.is_none() {
                                orientation = if width > height {
                                    Some(Orientation::Landscape)
                                } else {
                                    Some(Orientation::Portrait)
                                };
                                uib.orientation_changed(orientation.unwrap());
                            }else { 
                                let new_orientation = if width > height {
                                    Orientation::Landscape
                                } else {
                                    Orientation::Portrait
                                };
                                if new_orientation != orientation.unwrap() {
                                    orientation = Some(new_orientation);
                                    uib.orientation_changed(orientation.unwrap());
                                }
                            }
                        });
                    }
                    WindowEvent::CursorMoved { device_id, position, .. } => {
                        let position = PhysicalPosition::new(position.x as f32, position.y as f32);
                        let scale_factor = app.scale_factor();
                        cursor_position = position.to_logical(scale_factor as f64);
                        for pressed_button in pressed_mouse_button.iter() {
                            uib.lock().unwrap().mouse_input(
                                MouseEvent {
                                    device_id,
                                    state: ButtonState::Moved,
                                    button: *pressed_button,
                                    x: cursor_position.x,
                                    y: cursor_position.y,
                                }
                            );
                        }
                    }
                    WindowEvent::MouseInput { device_id, state, button } => {
                        match state {
                            winit::event::ElementState::Pressed => {
                                pressed_mouse_button.push(button);
                            }
                            winit::event::ElementState::Released => {
                                pressed_mouse_button.retain(|pressed_button| *pressed_button != button);
                            }
                        }
                        uib.lock().unwrap().mouse_input(
                            MouseEvent {
                                device_id,
                                state: match state {
                                    winit::event::ElementState::Pressed => ButtonState::Pressed,
                                    winit::event::ElementState::Released => ButtonState::Released,
                                },
                                button,
                                x: cursor_position.x,
                                y: cursor_position.y,
                            }
                        );
                    }

                    WindowEvent::KeyboardInput {
                        device_id: _device_id, event: _event, is_synthetic: _is_synthetic
                    } => {}

                    WindowEvent::Ime(ime) => {
                        uib.lock().unwrap().ime_input(
                            match ime {
                                Ime::Enabled => {
                                    ImeAction::Enabled
                                }
                                Ime::Preedit(text,range) => {
                                    ImeAction::Preedit(text,range)
                                }
                                Ime::Commit(text) => {
                                    ImeAction::Commit(text)
                                }
                                Ime::Disabled => {
                                    ImeAction::Disabled
                                }
                            }
                        )
                    }

                    WindowEvent::RedrawRequested => {
                        app.animations.lock().unwrap().retain(|animation| !animation.is_finished());
                        // app.lock().unwrap().need_redraw = true;
                        let env = env.as_mut().unwrap();
                        let scale_factor = app.scale_factor();

                        let canvas = env.surface.canvas();

                        canvas.clear(Color::TRANSPARENT);

                        canvas.save();
                        canvas.scale((scale_factor, scale_factor));

                        uib.lock().unwrap().draw(canvas);

                        canvas.restore();

                        env.gr_context.flush_and_submit();
                        env.gl_surface.swap_buffers(&env.gl_context).unwrap();
                        app.redraw_done();
                        // println!("Redraw requested {:?}", Instant::now());
                        if !app.animations.lock().unwrap().is_empty() {
                            app.request_redraw();
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }


        // if app.lock().unwrap().need_rebuild {
        //     let mut page_item = pages.current_page().unwrap();
        //     let mut old_item = page_item.root_item_mut();
        //     let item = page_item.page.build(app.clone());
        //     page_item.page.on_create(app.clone());
        //     page_item.root_item = Some(item);
        //     app.rebuild_done();
        //     app.request_layout();
        // }

        if app.need_layout() {
            let (width, height): (f32, f32) = app.window().unwrap().inner_size().into();
            let scale_factor = app.scale_factor();
            let width = width / scale_factor;
            let height = height / scale_factor;
            uib.lock().unwrap().let_mut(|uib| {
                uib.measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
                uib.layout(0.0, 0.0);
            });

            app.layout_done();
            app.request_redraw();
            println!("layout");
            
        }

    }).unwrap();
}

#[cfg(not(target_os = "android"))]
pub fn run_app(window_builder: WindowBuilder, theme: Theme, uib: impl Fn(SharedApp) -> Item + 'static) {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    run(event_loop, window_builder, Box::new(uib), theme);
}

#[cfg(target_os = "android")]
pub fn create_window(android_app: AndroidApp, window_builder: WindowBuilder, launch_page: Box<dyn Page>) {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().with_android_app(android_app).build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = SharedApp::new(event_loop.create_proxy());
    new_app(app.clone());

    run(app, event_loop, launch_page);
}

struct Env {
    surface: Surface,
    gl_surface: GlutinSurface<WindowSurface>,
    gr_context: gpu::DirectContext,
    gl_context: PossiblyCurrentContext,
    fb_info: FramebufferInfo,
    num_samples: usize,
    stencil_size: usize,
}

fn init_env(elwt: &EventLoopWindowTarget<UserEvent>, window_builder: WindowBuilder) -> (Env, Window) {
    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));
    let (window, gl_config) = display_builder
        .build(elwt, template, |configs| {
            configs
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .unwrap();
    let window = window.expect("Could not create window with OpenGL context");
    let raw_window_handle = window.raw_window_handle();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));
    let not_current_gl_context = unsafe {
        gl_config
            .display()
            .create_context(&gl_config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_config
                    .display()
                    .create_context(&gl_config, &fallback_context_attributes)
                    .expect("failed to create context")
            })
    };

    let (width, height): (u32, u32) = window.inner_size().into();

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    );

    let gl_surface = unsafe {
        gl_config
            .display()
            .create_window_surface(&gl_config, &attrs)
            .expect("Could not create gl window surface")
    };

    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .expect("Could not make GL context current when setting up skia renderer");

    gl::load_with(|s| {
        gl_config
            .display()
            .get_proc_address(CString::new(s).unwrap().as_c_str())
    });
    let interface = gpu::gl::Interface::new_load_with(|name| {
        if name == "eglGetCurrentDisplay" {
            return std::ptr::null();
        }
        gl_config
            .display()
            .get_proc_address(CString::new(name).unwrap().as_c_str())
    })
        .expect("Could not create interface");

    let mut gr_context = gpu::DirectContext::new_gl(interface, None)
        .expect("Could not create direct context");

    let fb_info = {
        let mut fboid: GLint = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

        FramebufferInfo {
            fboid: fboid.try_into().unwrap(),
            format: gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        }
    };


    let num_samples = gl_config.num_samples() as usize;
    let stencil_size = gl_config.stencil_size() as usize;

    let surface = create_surface(window.inner_size(), fb_info, &mut gr_context, num_samples, stencil_size);

    (Env {
        surface,
        gl_surface,
        gl_context,
        gr_context,
        fb_info,
        num_samples,
        stencil_size,
    }, window)
}

fn create_surface(
    size: PhysicalSize<u32>,
    fb_info: FramebufferInfo,
    gr_context: &mut gpu::DirectContext,
    num_samples: usize,
    stencil_size: usize,
) -> Surface {
    let size = (
        size.width.try_into().expect("Could not convert width"),
        size.height.try_into().expect("Could not convert height"),
    );
    let backend_render_target =
        backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);

    gpu::surfaces::wrap_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
        .expect("Could not create skia surface")
}*/