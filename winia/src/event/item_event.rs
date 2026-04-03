use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use skia_safe::{image_filters, Canvas, IRect, Paint, PictureRecorder, Point, Rect, Surface, TileMode, Vector};
use skia_safe::image_filters::CropRect;
use crate::event::{ButtonSource, ElementState, Ime, KeyboardInput, MeasureMode, Modifiers, MouseScrollDelta, MouseWheel, PointerButton, PointerMoved};
use crate::ui::InnerPosition;
use crate::ui::item::{ItemData, ItemKind, ItemState};
use crate::ui::item::FocusState;

#[derive(Clone)]
pub struct Event<T: ?Sized> {
    pub event: Rc<RefCell<T>>,
}
impl<T: ?Sized> Deref for Event<T> {
    type Target = Rc<RefCell<T>>;
    fn deref(&self) -> &Self::Target {
        &self.event
    }
}
impl<T: ?Sized> DerefMut for Event<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.event
    }
}

macro_rules! event {
    ($event:block) => {
        Event {
            event: Rc::new(RefCell::new(Box::new($event))),
        }
    };
}

pub struct ItemEvent {
    pub click_input: Event<dyn FnMut(&mut ItemData, &ButtonSource)>,
    pub dispatch_draw: Event<dyn FnMut(&mut ItemData, &mut Surface, f32, f32)>,
    pub dispatch_focus: Event<dyn FnMut(&mut ItemData, u32, bool) -> bool>,
    pub dispatch_ime_input: Event<dyn FnMut(&mut ItemData, &Ime) -> bool>,
    pub dispatch_keyboard_input: Event<dyn FnMut(&mut ItemData, &KeyboardInput) -> bool>,
    pub dispatch_layout: Event<dyn FnMut(&mut ItemData, f32, f32, f32, f32)>,
    pub dispatch_measure: Event<dyn FnMut(&mut ItemData, MeasureMode, MeasureMode)>,
    pub dispatch_modifiers_changed: Event<dyn FnMut(&mut ItemData, &Modifiers)>,
    pub dispatch_mouse_wheel: Event<dyn FnMut(&mut ItemData, MouseWheel) -> MouseScrollDelta>,
    pub dispatch_pointer_button: Event<dyn FnMut(&mut ItemData, &PointerButton) -> bool>,
    pub dispatch_pointer_moved: Event<dyn FnMut(&mut ItemData, &PointerMoved)>,
    pub draw: Event<dyn FnMut(&mut ItemData, &Canvas)>,
    pub hover_changed: Event<dyn FnMut(&mut ItemData, bool)>,
    pub focus_changed: Event<dyn FnMut(&mut ItemData, &FocusState)>,
    pub focus_next: Event<dyn FnMut(&mut ItemData) -> bool>,
    pub ime_input: Event<dyn FnMut(&mut ItemData, &Ime)>,
    pub keyboard_input: Event<dyn FnMut(&mut ItemData, &KeyboardInput)>,
    pub layout: Event<dyn FnMut(&mut ItemData, f32, f32)>,
    pub measure: Event<dyn FnMut(&mut ItemData, MeasureMode, MeasureMode)>,
    pub modifiers_changed: Event<dyn FnMut(&mut ItemData, &Modifiers)>,
    pub mouse_wheel: Event<dyn FnMut(&mut ItemData, MouseWheel) -> MouseScrollDelta>,
    pub pointer_button: Event<dyn FnMut(&mut ItemData, &PointerButton) -> bool>,
    pub pointer_moved: Event<dyn FnMut(&mut ItemData, &PointerMoved)>,
    pub record_animation_value: Event<dyn FnMut(&mut ItemData)>,
}

impl Default for ItemEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemEvent {
    pub fn new() -> Self {
        // let pressed_pointers: SharedSource<HashSet<ButtonSourceHashWrapper>> = SharedSource::new(HashSet::new());
        Self {
            click_input: event!({ |_item: &mut ItemData, _source: &ButtonSource| {} }),
            dispatch_draw: event!({
                let mut is_animating = false;
                let mut image_filter_paint = Paint::default();
                let mut shadow_paint = Paint::default();
                let mut last_parent_x: f32 = 0.0;
                let mut last_parent_y: f32 = 0.0;
                move |item: &mut ItemData, surface: &mut Surface, parent_x: f32, parent_y: f32| {
                    if !item.transition_visible.get() {
                        return;
                    }
                    let current_frame = item.current_frame();
                    item.target_frame.set_parent_position(parent_x, parent_y);

                    let clipped = item.props().clipped.get();
                    let clip_shape = {
                        let current_frame = item.current_frame();
                        let shape = item.props().clip_shape.lock();
                        shape.as_ref().map(|shape| shape(&current_frame))
                    };
                    {
                        // Draw the background blur effect.
                        let blur = current_frame.get_float_param("blur").unwrap_or(35.0);
                        // let blur = /*35.0*/item.props().blur.get();
                        let margin = blur * 2.0;
                        let current_frame = item.current_frame();
                        if item.props().enable_background_blur.get()
                            && !current_frame.is_empty()
                            && blur > 0.0
                        {
                            let scale_factor = item.window_context().scale_factor();
                            let left = (current_frame.x() * scale_factor - margin) as i32;
                            let top = (current_frame.y() * scale_factor - margin) as i32;
                            let right = ((current_frame.x() + current_frame.width) * scale_factor
                                + margin) as i32;
                            let bottom = ((current_frame.y() + current_frame.height) * scale_factor
                                + margin) as i32;

                            let background = surface
                                .image_snapshot_with_bounds(IRect::from_ltrb(
                                    left, top, right, bottom,
                                ))
                                .unwrap();

                            let (width, height) = {
                                let image_info = background.image_info();
                                (image_info.width(), image_info.height())
                            };

                            let canvas = surface.canvas();
                            image_filter_paint.set_image_filter(image_filters::blur(
                                (blur, blur),
                                TileMode::Clamp,
                                None,
                                CropRect::from(Rect::from_wh(width as f32, height as f32)),
                            ));

                            let d = margin / scale_factor;
                            let mut x = current_frame.x() - d;
                            let mut y = current_frame.y() - d;
                            if x < 0.0 {
                                x = 0.0;
                            }
                            if y < 0.0 {
                                y = 0.0;
                            }

                            canvas.save();

                            if let Some(clip_shape) = &clip_shape
                                && clipped
                            {
                                canvas.save();
                                canvas.clip_path(clip_shape, None, true);
                            } else {
                                canvas.clip_rect(
                                    Rect::from_xywh(
                                        current_frame.x(),
                                        current_frame.y(),
                                        current_frame.width,
                                        current_frame.height,
                                    ),
                                    None,
                                    None,
                                );
                            }
                            canvas.translate(Vector::new(x, y));
                            canvas.scale((1.0 / scale_factor, 1.0 / scale_factor));
                            canvas.draw_image(
                                background,
                                Point::new(0.0, 0.0),
                                Some(&image_filter_paint),
                            );
                            canvas.restore();
                        }
                    }

                    let x = current_frame.x();
                    let y = current_frame.y();
                    let rotation = current_frame.rotation;
                    let rotation_center_x = current_frame.rotation_center_x + x;
                    let rotation_center_y = current_frame.rotation_center_y + y;

                    let skew_x = current_frame.skew_x;
                    let skew_y = current_frame.skew_y;
                    let skew_center_x = current_frame.skew_center_x + x;
                    let skew_center_y = current_frame.skew_center_y + y;
                    let scale_x = current_frame.scale_x;
                    let scale_y = current_frame.scale_y;
                    let scale_center_x = current_frame.scale_center_x + x;
                    let scale_center_y = current_frame.scale_center_y + y;

                    {
                        // Apply the transformation matrix to the canvas.
                        let canvas = surface.canvas();
                        if current_frame.opacity < 1.0 {
                            canvas.save_layer_alpha_f(
                                Rect::from_xywh(
                                    current_frame.x(),
                                    current_frame.y(),
                                    current_frame.width,
                                    current_frame.height,
                                ),
                                current_frame.opacity,
                            );
                        } else {
                            canvas.save();
                        }

                        canvas.rotate(
                            rotation,
                            Some(Point::new(
                                rotation_center_x,
                                rotation_center_y,
                            )),
                        );

                        canvas.translate((skew_center_x, skew_center_y));
                        canvas.skew((skew_x, skew_y));
                        canvas.translate((-skew_center_x, -skew_center_y));

                        canvas.scale((scale_x, scale_y));
                        canvas.translate((
                            -(scale_x - 1.0) * scale_center_x / scale_x,
                            -(scale_y - 1.0) * scale_center_y / scale_y,
                        ));
                    }

                    if is_animating != item.animations.is_animating() {
                        item.props().item_updater.lock().need_redraw = true;
                        is_animating = item.animations.is_animating();
                    }
                    if (item.props().item_updater.lock().need_redraw && !item.animations.is_animating())
                    || (last_parent_x != parent_x || last_parent_y != parent_y)
                    {
                        let mut recorder = PictureRecorder::new();
                        let canvas = recorder.begin_recording(
                            Rect::from_wh(
                                item.window_context().window_size().0,
                                item.window_context().window_size().1,
                            ),
                            true,
                        );
                        if let Some(clip_shape) = &clip_shape
                            && clipped
                        {
                            canvas.save();
                            canvas.clip_path(clip_shape, None, true);
                        }
                        item.draw(canvas);
                        let picture = recorder.finish_recording_as_picture(None);
                        item.draw_cache = picture;
                    }
                    last_parent_x = parent_x;
                    last_parent_y = parent_y;

                    {
                        if let Some(clip_shape) = &clip_shape
                            && clipped
                        {
                            surface.canvas().clip_path(clip_shape, None, true);
                        }
                    }
                    {
                        let background = item.props().background.clone();
                        let mut background_lock = background.lock();
                        if let Some(bg) = background_lock.as_mut() {
                            bg.data().dispatch_draw(
                                surface,
                                current_frame.x(),
                                current_frame.y(),
                            );
                        }
                    }

                    if item.animations.is_animating() {
                        item.draw(surface.canvas());
                    } else if let Some(picture) = &item.draw_cache {
                        let canvas = surface.canvas();
                        canvas.draw_picture(picture, None, None);
                    }

                    {
                        let mut children_lock = item.children().lock();
                        for child in children_lock.iter_mut() {
                            child.data().dispatch_draw(
                                surface,
                                current_frame.x(),
                                current_frame.y(),
                            );
                        }
                    }
                    {
                        let foreground = item.props().foreground.clone();
                        let mut foreground_lock = foreground.lock();
                        if let Some(fg) = foreground_lock.as_mut() {
                            fg.data().dispatch_draw(
                                surface,
                                current_frame.x(),
                                current_frame.y(),
                            );
                        }
                    }

                    {
                        // Restore the transformation matrix of the canvas.
                        let canvas = surface.canvas();
                        canvas.restore();
                    }

                    item.props().item_updater.lock().need_redraw = false;
                }
            }),
            dispatch_focus: event!({
                |item: &mut ItemData, item_id: u32, has_parent_focus: bool| {
                    let target_id = if item_id == 0 { None } else { Some(item_id) };
                    item.dispatch_focus_to(target_id, has_parent_focus)
                }
            }),
            dispatch_ime_input: event!({
                |item: &mut ItemData, action: &Ime| {
                    if !item.interaction_enabled {
                        return false;
                    }
                    if item.focus_state.is_focused {
                        item.ime_input(action);
                        return true;
                    }
                    let children = item.children().lock();
                    for child in children.iter() {
                        if child.data().dispatch_ime_input(action) {
                            return true;
                        }
                    }
                    false
                }
            }),
            dispatch_keyboard_input: event!({
                |item: &mut ItemData, input: &KeyboardInput| {
                    if !item.interaction_enabled {
                        return false;
                    }
                    if item.focus_state.is_focused {
                        item.keyboard_input(input);
                        return true;
                    }
                    let children = item.children().lock();
                    for child in children.iter() {
                        if child.data().dispatch_keyboard_input(input) {
                            return true;
                        }
                    }
                    false
                }
            }),
            dispatch_layout: event!({
                let mut last_width: f32 = 0.0;
                let mut last_height: f32 = 0.0;
                let mut last_relative_x: f32 = 0.0;
                let mut last_relative_y: f32 = 0.0;
                move |item: &mut ItemData,
                      relative_x: f32,
                      relative_y: f32,
                      width: f32,
                      height: f32| {
                    let needs_redraw = item.props().item_updater.lock().need_redraw;
                    if !needs_redraw && last_width == width
                        && last_height == height
                        && last_relative_x == relative_x
                        && last_relative_y == relative_y
                    {
                        let children = item.children().lock();
                        for child in children.iter() {
                            child.data().re_layout()
                        }
                        let background = item.props().background.lock();
                        if let Some(bg) = background.as_ref() {
                            bg.data().re_layout();
                        }
                        let foreground = item.props().foreground.lock();
                        if let Some(fg) = foreground.as_ref() {
                            fg.data().re_layout();
                        }
                        return;
                    } else {
                        item.props().item_updater.lock().need_redraw = true;
                    }

                    last_width = width;
                    last_height = height;
                    last_relative_x = relative_x;
                    last_relative_y = relative_y;
                    {
                        let measure_frame = &item.measure_frame;
                        if width != measure_frame.width || height != measure_frame.height {
                            item.measure(
                                MeasureMode::Specified(width),
                                MeasureMode::Specified(height),
                            );
                        }
                    }
                    let visible = item.props().visible.get();
                    let offset_x = item.props().offset_x.get();
                    let offset_y = item.props().offset_y.get();
                    let opacity = if visible {
                        item.props().opacity.get()
                    } else {
                        0.0
                    };
                    let rotation = item.props().rotation.get();
                    let scale_x = if visible {
                        item.props().scale_x.get()
                    } else {
                        0.0
                    };
                    let scale_y = if visible {
                        item.props().scale_y.get()
                    } else {
                        0.0
                    };
                    let skew_x = item.props().skew_x.get();
                    let skew_y = item.props().skew_y.get();

                    fn center(inner_position: InnerPosition, size: f32) -> f32 {
                        match inner_position {
                            InnerPosition::Start(offset) => offset,
                            InnerPosition::Middle(offset) => size / 2.0 + offset,
                            InnerPosition::End(offset) => size + offset,
                            InnerPosition::Relative(fraction) => size * fraction,
                            InnerPosition::Absolute(offset) => offset,
                        }
                    }

                    {
                        let rotation_center_x = center(item.props().rotation_center_x.get(), width);
                        let rotation_center_y = center(item.props().rotation_center_y.get(), height);
                        let scale_center_x = center(item.props().scale_center_x.get(), width);
                        let scale_center_y = center(item.props().scale_center_y.get(), height);
                        let skew_center_x = center(item.props().skew_center_x.get(), width);
                        let skew_center_y = center(item.props().skew_center_y.get(), height);

                        {
                            let target_frame = &mut item.target_frame;
                            target_frame.set_relative_position(relative_x, relative_y);
                            target_frame.width = width;
                            target_frame.height = height;
                            target_frame.opacity = opacity;
                            target_frame.rotation = rotation;
                            target_frame
                                .set_rotation_center(rotation_center_x, rotation_center_y);
                            target_frame.set_scale(scale_x, scale_y);
                            target_frame.set_scale_center(scale_center_x, scale_center_y);
                            target_frame.set_offset(offset_x, offset_y);
                            target_frame.set_skew(skew_x, skew_y);
                            target_frame.set_skew_center(skew_center_x, skew_center_y);
                        }
                    }

                    // item.layout_layers(width, height);

                    item.layout(width, height);

                    {
                        let blur = item.props().blur.get();
                        let target_frame = &mut item.target_frame;
                        target_frame.set_float_param("blur", blur);
                    }

                    {
                        let background = item.props().background.clone();
                        let mut background_lock = background.lock();
                        if let Some(bg) = background_lock.as_mut() {
                            bg.data().measure(
                                MeasureMode::Specified(width),
                                MeasureMode::Specified(height),
                            );
                            bg.data().dispatch_layout(0.0, 0.0, width, height);
                        }
                    }
                    {
                        let foreground = item.props().foreground.clone();
                        let mut foreground_lock = foreground.lock();
                        if let Some(fg) = foreground_lock.as_mut() {
                            fg.data().measure(
                                MeasureMode::Specified(width),
                                MeasureMode::Specified(height),
                            );
                            fg.data().dispatch_layout(0.0, 0.0, width, height);
                        }
                    }
                    if !item.is_mounted {
                        item.is_mounted = true;
                        item.on_mounted();
                    }
                    
                    item.record_animation_value();
/*                    let frame = if let Some(f) = &item.default_recorded_frame && !item.is_entered {
                        Some(f(&item.target_frame))
                    } else {
                        None
                    };
                    if let Some(frame) = frame {
                        item.recorded_frame = Some(frame);
                    }*/
                }
            }),
            dispatch_measure: event!({
                move |item: &mut ItemData, width_mode: MeasureMode, height_mode: MeasureMode| {
                    let needs_redraw = item.props().item_updater.lock().need_redraw;
                    if !needs_redraw
                        && item.last_measure_width_mode == Some(width_mode)
                        && item.last_measure_height_mode == Some(height_mode)
                    {
                        let children = item.children().lock();
                        for child in children.iter() {
                            child.data().re_measure(width_mode.value(), height_mode.value());
                        }
                        let background = item.props().background.lock();
                        if let Some(bg) = background.as_ref() {
                            bg.data().re_measure(width_mode.value(), height_mode.value());
                        }
                        let foreground = item.props().foreground.lock();
                        if let Some(fg) = foreground.as_ref() {
                            fg.data().re_measure(width_mode.value(), height_mode.value());
                        }
                        return;
                    } else {
                        item.props().item_updater.lock().need_redraw = true;
                    }
                    item.last_measure_width_mode = Some(width_mode);
                    item.last_measure_height_mode = Some(height_mode);
                    item.measure(width_mode, height_mode);
                }
            }),
            dispatch_modifiers_changed: event!({
                |item: &mut ItemData, modifiers: &Modifiers| {
                    let children = item.children().lock();
                    for child in children.iter() {
                        child.data().dispatch_modifiers_changed(modifiers);
                    }
                    drop(children);
                    item.modifiers_changed(modifiers);
                }
            }),
            /*            dispatch_mouse_input: event!({
                            // The mouse button that the item has captured.
                            // When the item captures a mouse button, the item can receive mouse input
                            // events even if the mouse pointer is outside the item.
                            let mut captured_mouse_button: HashSet<MouseButton> = HashSet::new();
                            // The source of the click event.
                            let mut click_source: Option<ClickSource> = None;
                            move |item: &mut ItemData, input: &MouseInput| {
                                if !item.props().enable.get() {
                                    return false;
                                }

                                match input.pointer_state {
                                    PointerState::Started => {
                                        if item.props().enable.get() {
                                            item.on_state_changed(ItemState::Pressed);
                                        }
                                    }
                                    PointerState::Ended | PointerState::Cancelled => {
                                        let item_state = item.props().item_state.clone();
                                        if item_state.get() == ItemState::Pressed {
                                            if item.focus_state.is_focused {
                                                item.on_state_changed(ItemState::Focused);
                                            } else {
                                                item.on_state_changed(ItemState::Enabled);
                                            }
                                        }
                                    }
                                    _ => {}
                                }

                                {
                                    let foreground = item.props().foreground.clone();
                                    let mut foreground_lock = foreground.lock();
                                    if let Some(fg) = foreground_lock.as_mut() {
                                        fg.data().dispatch_mouse_input(input);
                                    }
                                    let background = item.props().background.clone();
                                    let mut background_lock = background.lock();
                                    if let Some(bg) = background_lock.as_mut() {
                                        bg.data().dispatch_mouse_input(input);
                                    }
                                }

                                let mut children = item.children().lock();
                                for child in children.iter_mut().rev() {
                                    if child.data().dispatch_mouse_input(input) {
                                        return true;
                                    }
                                }
                                drop(children);

                                let frame = item.current_frame();
                                if !captured_mouse_button.contains(&input.button)
                                    && !frame.contains(input.x, input.y)
                                {
                                    return false;
                                }

                                let pointer_input = PointerInput::from(input);
                                if item.on_pointer_input(&pointer_input) {
                                    return true;
                                }
                                if item.on_mouse_input(input) {
                                    return true;
                                }
                                if item.pointer_input(&pointer_input) {
                                    return true;
                                }
                                if item.mouse_input(input) {
                                    return true;
                                }

                                match input.pointer_state {
                                    PointerState::Started => {
                                        captured_mouse_button.insert(input.button);
                                        click_source = Some(ClickSource::Mouse(input.button));
                                        return item.props().on_click.is_some();
                                    }
                                    PointerState::Moved => {
                                        if captured_mouse_button.contains(&input.button) {
                                            return item.props().on_click.is_some();
                                        }
                                    }
                                    PointerState::Ended | PointerState::Cancelled => {
                                        captured_mouse_button.remove(&input.button);
                                        if frame.contains(input.x, input.y) {
                                            if let Some(click_source) = click_source.take() {
                                                item.on_click(&click_source);
                                                item.click_input(&click_source);
                                            }
                                        }
                                        return item.props().on_click.is_some();
                                    }
                                }
                                false
                            }
                        }),*/
            dispatch_mouse_wheel: event!({
                |item: &mut ItemData,
                 mouse_wheel: MouseWheel | {
                    if !item.interaction_enabled {
                        return mouse_wheel.delta;
                    }
                    if !mouse_wheel.delta.is_scrollable() {
                        mouse_wheel.delta
                    } else {
                        let mut mouse_wheel_delta = mouse_wheel.delta;
                        let children = item.children().lock();
                        for child in children.iter().rev() {
                            let r = child
                                .data()
                                .dispatch_mouse_wheel(MouseWheel {
                                    delta: mouse_wheel_delta,
                                    ..mouse_wheel
                                });
                            mouse_wheel_delta = r;
                            if !mouse_wheel_delta.is_scrollable() {
                                break;
                            }
                        }
                        drop(children);
                        if mouse_wheel_delta.is_scrollable() {
                            item.mouse_wheel(
                                MouseWheel {
                                    delta: mouse_wheel_delta,
                                    ..mouse_wheel
                                }
                            );
                        }
                        mouse_wheel_delta
                    }
                }
            }),
            // dispatch_touch_input: event!({ |_item: &mut ItemData, _input: &TouchInput| { false } }),
            dispatch_pointer_button: event!({
                let mut pressed_pointers: HashSet<ButtonSourceHashWrapper> = HashSet::new();
                move |item: &mut ItemData, pointer_button: &PointerButton| {
                    let current_frame = item.current_frame();
                    let button_wrapper = ButtonSourceHashWrapper(pointer_button.button.clone());
                    if !item.interaction_enabled || !item.props().enable.get() {
                        return false;
                    }

                    match pointer_button.state {
                        ElementState::Pressed => {
                            pressed_pointers.remove(&button_wrapper);
                            if !current_frame.contains(pointer_button.x, pointer_button.y) {
                                return false;
                            }
                            let mut children = item.children().lock();
                            for child in children.iter_mut().rev() {
                                if child.data().dispatch_pointer_button(pointer_button) {
                                    return true;
                                }
                            }
                            drop(children);
                            pressed_pointers.insert(button_wrapper.clone());
                            item.on_state_changed(ItemState::Pressed);
                            {
                                let foreground = item.props().foreground.clone();
                                let mut foreground_lock = foreground.lock();
                                if let Some(fg) = foreground_lock.as_mut() {
                                    fg.data().dispatch_pointer_button(pointer_button);
                                }
                                let background = item.props().background.clone();
                                let mut background_lock = background.lock();
                                if let Some(bg) = background_lock.as_mut() {
                                    bg.data().dispatch_pointer_button(pointer_button);
                                }
                            }
                            if item.on_pointer_button(pointer_button) {
                                return true;
                            }
                            if item.pointer_button(pointer_button) {
                                return true;
                            }
                            return item.props().on_click.is_some();
                        }
                        ElementState::Released => {
                            if !pressed_pointers.contains(&button_wrapper) {
                                let mut children = item.children().lock();
                                for child in children.iter_mut().rev() {
                                    if child.data().dispatch_pointer_button(pointer_button) {
                                        return true;
                                    }
                                }
                                return false;
                            }
                            pressed_pointers.remove(&button_wrapper);
                            let item_state = item.props().item_state.clone();
                            if item_state.get() == ItemState::Pressed {
                                if item.focus_state.is_focused {
                                    item.on_state_changed(ItemState::Focused);
                                } else {
                                    item.on_state_changed(ItemState::Enabled);
                                }
                            }
                            {
                                let foreground = item.props().foreground.clone();
                                let mut foreground_lock = foreground.lock();
                                if let Some(fg) = foreground_lock.as_mut() {
                                    fg.data().dispatch_pointer_button(pointer_button);
                                }
                                let background = item.props().background.clone();
                                let mut background_lock = background.lock();
                                if let Some(bg) = background_lock.as_mut() {
                                    bg.data().dispatch_pointer_button(pointer_button);
                                }
                            }
                            if item.on_pointer_button(pointer_button) {
                                return true;
                            }
                            if item.pointer_button(pointer_button) {
                                return true;
                            }
                            if current_frame.contains(pointer_button.x, pointer_button.y) {
                                item.on_click(&pointer_button.button);
                                item.click_input(&pointer_button.button);
                            }
                            return item.props().on_click.is_some();
                        }
                    }
                    false
                }
            }),
            dispatch_pointer_moved: event!({
                let mut is_hovered = false;
                move |item: &mut ItemData, pointer_moved: &PointerMoved| {
                    if !item.interaction_enabled || !item.props().enable.get() {
                        if is_hovered {
                            is_hovered = false;
                            item.hover_changed(false);
                            item.on_hover_changed(false);
                        }
                        return;
                    }
                    let mut child_hit = false;
                    {
                        let mut children = item.children().lock();
                        for child in children.iter_mut().rev() {
                            if !child_hit {
                                let child_frame = child.data().current_frame();
                                child_hit = child_frame.contains(
                                    pointer_moved.x,
                                    pointer_moved.y,
                                );
                            }
                            child.data().dispatch_pointer_moved(pointer_moved);
                        }
                    }

                    let self_hit = item.current_frame().contains(
                        pointer_moved.x,
                        pointer_moved.y,
                    );
                    let handle_self = self_hit && !child_hit;

                    if handle_self {
                        {
                            let foreground = item.props().foreground.clone();
                            let mut foreground_lock = foreground.lock();
                            if let Some(fg) = foreground_lock.as_mut() {
                                fg.data().dispatch_pointer_moved(pointer_moved);
                            }
                        }
                        {
                            let background = item.props().background.clone();
                            let mut background_lock = background.lock();
                            if let Some(bg) = background_lock.as_mut() {
                                bg.data().dispatch_pointer_moved(pointer_moved);
                            }
                        }
                        item.on_pointer_moved(pointer_moved);
                        item.pointer_moved(pointer_moved);

                        if !is_hovered {
                            is_hovered = true;
                            item.hover_changed(true);
                            item.on_hover_changed(true);
                            if item.props().item_state.get() == ItemState::Enabled {
                                item.on_state_changed(ItemState::Hovered);
                            }
                        }
                    } else if is_hovered {
                        is_hovered = false;
                        item.hover_changed(false);
                        item.on_hover_changed(false);
                    }
                }
            }),
            draw: event!({ |_item: &mut ItemData, _canvas: &Canvas| {} }),
            hover_changed: event!({ |_item: &mut ItemData, _is_hovered: bool| {} }),
            focus_changed: event!({ |_item: &mut ItemData, _focus_state: &FocusState| {} }),
            focus_next: event!({
                let mut current_focus_child_index: Option<usize> = None;
                move |item: &mut ItemData| {
                    if !item.is_mounted
                        || !item.is_exited
                        || !item.focus_enabled
                        || !item.interaction_enabled
                        || !item.transition_visible.get()
                        || !item.props().enable.get()
                        || !item.props().visible.get()
                    {
                        return false;
                    }
                    if item.kind() == ItemKind::Widget {
                        if !item.can_focus() {
                            return false;
                        }
                        if !item.focus_state.is_focused {
                            item.focus_requester.lock().request_focus();
                            return true;
                        } else {
                            return false;
                        }
                    } else {
                        let mut children = item.children().lock();
                        let mut index = if let Some(current_focus_child_index) = current_focus_child_index {
                            current_focus_child_index + 1
                        } else {
                            0
                        };
                        loop {
                            if let Some(child) = children.get_mut(index) {
                                if child.data().focus_next() {
                                    current_focus_child_index = Some(index);
                                    return true;
                                } else {
                                    index += 1;
                                }
                            } else {
                                current_focus_child_index = None;
                                return false;
                            }
                        }
                    }
                    false
            }}),
            ime_input: event!({ |_item: &mut ItemData, _action: &Ime| {} }),
            keyboard_input: event!({ |_item: &mut ItemData, _input: &KeyboardInput| {} }),
            layout: event!({ |_item: &mut ItemData, _width: f32, _height: f32| {} }),
            measure: event!({
                |item: &mut ItemData, width_mode: MeasureMode, height_mode: MeasureMode| {
                    item.measure_children(width_mode, height_mode);
                    fn get_size(measure_mode: MeasureMode) -> f32 {
                        match measure_mode {
                            MeasureMode::Specified(value) => value,
                            MeasureMode::Unspecified(_) => 0.0,
                        }
                    }
                    let width = item.props().clamp_width(get_size(width_mode));
                    let height = item.props().clamp_height(get_size(height_mode));
                    let measure_frame = &mut item.measure_frame;
                    measure_frame.width = width;
                    measure_frame.height = height;
                }
            }),
            modifiers_changed: event!({ |_item: &mut ItemData, _modifiers: &Modifiers| {} }),
            // mouse_input: event!({ |_item: &mut ItemData, _input: &MouseInput| { false } }),
            mouse_wheel: event!({
                |_item: &mut ItemData,
                 mouse_wheel: MouseWheel| {
                    mouse_wheel.delta
                }
            }),
            pointer_button: event!({ |_item: &mut ItemData, _input: &PointerButton| { false } }),
            pointer_moved: event!({
                |_item: &mut ItemData, _input: &PointerMoved| {}
            }),
            record_animation_value: event!({
                |_item: &mut ItemData| {}
            }),
        }
    }
}

macro_rules! impl_noop {
    ($($set:ident|$ty:ty|$invoke:ident|$ret:ty|$($arg_name:ident:$arg_type:ty);*),*) => {
        $(
            impl ItemEvent {
                // pub fn $invoke(&self, item: &mut ItemData, $($arg_name: $arg_type),*) -> $ret {
                //     self.$invoke.lock()(item, $($arg_name),*)
                // }
                pub fn $set(mut self, f: $ty) -> Self {
                    self.$invoke = event!({f});
                    self
                }
            }
            impl ItemData {
                pub fn $invoke(&mut self, $($arg_name: $arg_type),*) -> $ret {
                    let event = self.item_event().$invoke.clone();
                    let mut f = event.borrow_mut();
                    f(self, $($arg_name),*)
                }
            }
        )*
    }
}

impl_noop!(
    set_click_input|impl FnMut(&mut ItemData, &ButtonSource) + 'static|click_input|()|source:&ButtonSource,
    set_dispatch_draw|impl FnMut(&mut ItemData, &mut Surface, f32, f32) + 'static|dispatch_draw|()|surface:&mut Surface;parent_x:f32;parent_y:f32,
    set_dispatch_focus|impl FnMut(&mut ItemData, u32, bool) -> bool + 'static|dispatch_focus|bool|item_id:u32;has_parent_focus:bool,
    set_dispatch_ime_input|impl FnMut(&mut ItemData, &Ime) -> bool + 'static|dispatch_ime_input|bool|action:&Ime,
    set_dispatch_keyboard_input|impl FnMut(&mut ItemData, &KeyboardInput) -> bool + 'static|dispatch_keyboard_input|bool|input:&KeyboardInput,
    set_dispatch_layout|impl FnMut(&mut ItemData, f32, f32, f32, f32) + 'static|dispatch_layout|()|x:f32;y:f32;width:f32;height:f32,
    set_dispatch_measure|impl FnMut(&mut ItemData, MeasureMode, MeasureMode) + 'static|dispatch_measure|()|width:MeasureMode;height:MeasureMode,
    set_dispatch_modifiers_changed|impl FnMut(&mut ItemData, &Modifiers) + 'static|dispatch_modifiers_changed|()|modifiers:&Modifiers,
    // set_dispatch_mouse_input|impl FnMut(&mut ItemData, &MouseInput) -> bool + 'static|dispatch_mouse_input|bool|input:&MouseInput,
    set_dispatch_mouse_wheel|impl FnMut(&mut ItemData, MouseWheel) -> MouseScrollDelta + 'static|dispatch_mouse_wheel|MouseScrollDelta|mouse_wheel: MouseWheel,
    // set_dispatch_touch_input|impl FnMut(&mut ItemData, &TouchInput) -> bool + 'static|dispatch_touch_input|bool|input:&TouchInput,
    set_dispatch_pointer_button|impl FnMut(&mut ItemData, &PointerButton) -> bool + 'static|dispatch_pointer_button|bool|input:&PointerButton,
    set_dispatch_pointer_moved|impl FnMut(&mut ItemData, &PointerMoved) + 'static|dispatch_pointer_moved|()|input:&PointerMoved,
    set_draw|impl FnMut(&mut ItemData, &Canvas) + 'static|draw|()|canvas:&Canvas,
    set_focus_changed|impl FnMut(&mut ItemData, &FocusState) + 'static|focus_changed|()|focus_state:&FocusState,
    set_focus_next|impl FnMut(&mut ItemData) -> bool + 'static|focus_next|bool|,
    set_hover_changed|impl FnMut(&mut ItemData, bool) + 'static|hover_changed|()|is_hovered:bool,
    set_ime_input|impl FnMut(&mut ItemData, &Ime) + 'static|ime_input|()|action:&Ime,
    set_keyboard_input|impl FnMut(&mut ItemData, &KeyboardInput) + 'static|keyboard_input|()|input:&KeyboardInput,
    set_layout|impl FnMut(&mut ItemData, f32, f32) + 'static|layout|()|width:f32;height:f32,
    set_measure|impl FnMut(&mut ItemData, MeasureMode, MeasureMode) + 'static|measure|()|width:MeasureMode;height:MeasureMode,
    set_modifiers_changed|impl FnMut(&mut ItemData, &Modifiers) + 'static|modifiers_changed|()|modifiers:&Modifiers,
    // set_mouse_input|impl FnMut(&mut ItemData, &MouseInput) -> bool + 'static|mouse_input|bool|input:&MouseInput,
    set_mouse_wheel|impl FnMut(&mut ItemData, MouseWheel) -> MouseScrollDelta + 'static|mouse_wheel|MouseScrollDelta|mouse_wheel:MouseWheel,
    set_pointer_button|impl FnMut(&mut ItemData, &PointerButton) -> bool + 'static|pointer_button|bool|input:&PointerButton,
    set_pointer_moved|impl FnMut(&mut ItemData, &PointerMoved) + 'static|pointer_moved|()|input:&PointerMoved,
    set_record_animation_value|impl FnMut(&mut ItemData) + 'static|record_animation_value|()|
);


#[derive(Clone, Debug)]
pub(crate) struct ButtonSourceHashWrapper(ButtonSource);
impl Hash for ButtonSourceHashWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            ButtonSource::Mouse(mouse_button) => mouse_button.hash(state),
            ButtonSource::Touch { finger_id, .. } => finger_id.hash(state),
            ButtonSource::TabletTool { button, .. } => button.hash(state),
            ButtonSource::Unknown(id) => id.hash(state),
        }
    }
}

impl From<ButtonSource> for ButtonSourceHashWrapper {
    fn from(source: ButtonSource) -> Self {
        ButtonSourceHashWrapper(source)
    }
}

impl From<&ButtonSource> for ButtonSourceHashWrapper {
    fn from(source: &ButtonSource) -> Self {
        ButtonSourceHashWrapper(source.clone())
    }
}

impl PartialEq<Self> for ButtonSourceHashWrapper {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (ButtonSource::Mouse(a), ButtonSource::Mouse(b)) => a == b,
            (ButtonSource::Touch { finger_id: a, .. }, ButtonSource::Touch { finger_id: b, .. }) => a == b,
            (ButtonSource::TabletTool { button: a, .. }, ButtonSource::TabletTool { button: b, .. }) => a == b,
            (ButtonSource::Unknown(a), ButtonSource::Unknown(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ButtonSourceHashWrapper {}