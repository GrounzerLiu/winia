// use winapi::shared::windef::HWND__;
use std::{ffi::CString, num::NonZeroU32};

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
    window::{Window, WindowBuilder},
};
use winit::dpi::{LogicalPosition, PhysicalPosition};
use winit::event_loop::{EventLoopBuilder, EventLoopWindowTarget};
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;

use crate::app::{SharedApp, Theme, UserEvent};
use crate::ui::{ButtonState, Item, MeasureMode, MouseEvent};
use crate::widget::{Rectangle, RectangleExt};

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

    let surface = create_surface(&window, fb_info, &mut gr_context, num_samples, stencil_size);

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

fn run(app: SharedApp, event_loop: EventLoop<UserEvent>, window_builder: WindowBuilder, ui_generate:Box<dyn Fn(SharedApp)->Item>) {

    let mut env = None;

    let mut ui = app.rectangle().item();
    
    let mut cursor_position = LogicalPosition::new(0.0_f32, 0.0_f32);
    let mut pressed_mouse_button = Vec::new();

    event_loop.run(move |event, elwt| {
        if let Event::Resumed = event {
            if env.is_none() {
                let (inited_env, window) = init_env(elwt, window_builder.clone());
                env = Some(inited_env);
                window.set_transparent(true);
                app.set_window(window);
                ui = ui_generate(app.clone());
            }        
            if env.is_none() {
                panic!("Env is not initialized");
            }
        }

        match event {
            Event::UserEvent(_user_event) => {
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
                            app.lock().unwrap().window(),
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

                        ui.measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
                        ui.layout(0.0, 0.0);
                    }
                    WindowEvent::CursorMoved { device_id, position, .. } => {
                        let position = PhysicalPosition::new(position.x as f32, position.y as f32);
                        let scale_factor = app.scale_factor();
                        cursor_position = position.to_logical(scale_factor as f64);
                        for pressed_button in pressed_mouse_button.iter() {
                            ui.mouse_input(
                                MouseEvent{
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
                        ui.mouse_input(
                            MouseEvent{
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
                    } => {

                    }

                    WindowEvent::Ime(ime) => {
                    }

                    WindowEvent::RedrawRequested => {
                        // app.lock().unwrap().need_redraw = true;
                        let env = env.as_mut().unwrap();
                        let scale_factor = app.scale_factor();

                        let canvas = env.surface.canvas();

                        canvas.clear(Color::TRANSPARENT);

                        canvas.save();
                        canvas.scale((scale_factor, scale_factor));

                        ui.draw(canvas);

                        canvas.restore();

                        env.gr_context.flush_and_submit();
                        env.gl_surface.swap_buffers(&env.gl_context).unwrap();
                        app.redraw_done();
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

        if app.lock().unwrap().need_layout {
            let (width, height): (f32, f32) = app.lock().unwrap().window().inner_size().into();
            let scale_factor = app.scale_factor();
            let width = width / scale_factor;
            let height = height / scale_factor;
            // pages.iter_mut().for_each(|page_item| {
            //     page_item.root_item_mut().measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
            //     page_item.root_item_mut().layout(0.0, 0.0);
            // });
            ui.measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
            ui.layout(0.0, 0.0);
            app.re_layout_done();
            app.request_redraw();
        }


        // if app.lock().unwrap().need_redraw {
        //     let env = env.as_mut().unwrap();
        //     let scale_factor = app.scale_factor();
        // 
        //     let canvas = env.surface.canvas();
        // 
        //     canvas.clear(Color::BLACK);
        // 
        //     canvas.save();
        //     canvas.scale((scale_factor, scale_factor));
        // 
        //     ui.draw(canvas);
        // 
        //     canvas.restore();
        // 
        //     env.gr_context.flush_and_submit();
        //     env.gl_surface.swap_buffers(&env.gl_context).unwrap();
        //     app.redraw_done();
        // }

        {
            // let width = app.lock().unwrap().content_width();
            // let height = app.lock().unwrap().content_height();
            // let mut animations = app.lock().unwrap().animations.clone();
            // if !animations.lock().unwrap().is_empty() {
            //     let item = pages.current_page().unwrap().root_item_mut();
            //     let mut animations = animations.lock().unwrap();
            //     for animation in animations.iter_mut() {
            //         if !animation.is_finished() {
            //             if animation.from.is_none() {
            //                 animation.from = Some(Animation::item_to_layout_params(item));
            //                 animation.layout_transition.run();
            //                 item.measure(MeasureMode::Specified(width), MeasureMode::Specified(height));
            //                 item.layout(0.0, 0.0);
            //                 animation.to = Some(Animation::item_to_layout_params(item));
            //                 app.lock().unwrap().need_layout = false;
            //             }
            //             animation.update(item, Instant::now());
            //         }
            //     }
            //     animations.retain(|animation| !animation.is_finished());
            //     app.lock().unwrap().request_redraw();
            // }
        }
        //println!("loop, {:?}", event_clone);
    }).unwrap();
}

#[cfg(not(target_os = "android"))]
pub fn run_app(window_builder: WindowBuilder, theme: Theme, ui:impl Fn(SharedApp)->Item + 'static){
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = SharedApp::new(event_loop.create_proxy(), theme);

    run(app, event_loop, window_builder, Box::new(ui));
}

#[cfg(target_os = "android")]
pub fn create_window(android_app: AndroidApp, window_builder: WindowBuilder, launch_page: Box<dyn Page>) {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().with_android_app(android_app).build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = SharedApp::new(event_loop.create_proxy());
    new_app(app.clone());

    run(app, event_loop, launch_page);
}

fn create_surface(
    window: &Window,
    fb_info: FramebufferInfo,
    gr_context: &mut gpu::DirectContext,
    num_samples: usize,
    stencil_size: usize,
) -> Surface {
    let size = window.inner_size();
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
}