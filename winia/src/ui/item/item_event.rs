use crate::shared::{Gettable, Shared};
use crate::ui::item::InnerPosition;
use crate::ui::Item;
use crate::OptionalInvoke;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Not;
// use skia_bindings::SkTileMode;
use skia_safe::image_filters::CropRect;
use skia_safe::{image_filters, Canvas, IRect, Paint, Point, Rect, Surface, TileMode, Vector};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::event::{DeviceId, Force, KeyEvent, MouseButton, TouchPhase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Not for Orientation {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::Vertical => Self::Horizontal,
            Self::Horizontal => Self::Vertical,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MeasureMode {
    /// Indicates that the parent has determined an exact size for the child.
    Specified(f32),
    /// Indicates that the child can determine its own size. The value of this enum is the maximum size the child can use.
    Unspecified(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerState {
    Started,
    Moved,
    Ended,
    Canceled,
}

impl From<TouchPhase> for PointerState {
    fn from(value: TouchPhase) -> Self {
        match value {
            TouchPhase::Started => PointerState::Started,
            TouchPhase::Moved => PointerState::Moved,
            TouchPhase::Ended => PointerState::Ended,
            TouchPhase::Cancelled => PointerState::Canceled,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub device_id: DeviceId,
    pub x: f32,
    pub y: f32,
    pub button: MouseButton,
    pub pointer_state: PointerState,
}

#[derive(Clone, Copy, Debug)]
pub struct TouchEvent {
    pub device_id: DeviceId,
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub pointer_state: PointerState,
    pub force: Option<Force>,
}

#[derive(Clone, Copy, Debug)]
pub enum Pointer {
    Touch { id: u64 },
    Mouse { button: MouseButton },
}

#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    pub device_id: DeviceId,
    pub pointer: Pointer,
    pub x: f32,
    pub y: f32,
    pub pointer_state: PointerState,
    pub force: Option<Force>,
}

impl Into<PointerEvent> for TouchEvent {
    fn into(self) -> PointerEvent {
        PointerEvent {
            device_id: self.device_id,
            pointer: Pointer::Touch { id: self.id },
            x: self.x,
            y: self.y,
            pointer_state: self.pointer_state,
            force: self.force,
        }
    }
}

impl Into<PointerEvent> for MouseEvent {
    fn into(self) -> PointerEvent {
        PointerEvent {
            device_id: self.device_id,
            pointer: Pointer::Mouse {
                button: self.button,
            },
            x: self.x,
            y: self.y,
            pointer_state: self.pointer_state,
            force: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ImeAction {
    Enabled,
    Enter,
    Delete,
    PreEdit(String, Option<(usize, usize)>),
    Commit(String),
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClickSource {
    Mouse(MouseButton),
    Touch,
    LongTouch,
}

impl MeasureMode {
    pub fn value(self) -> f32 {
        match self {
            MeasureMode::Specified(value) => value,
            MeasureMode::Unspecified(value) => value,
        }
    }
}

macro_rules! impl_get_set {
    ($name:ident, $set_type:ty, $set_doc:expr, $get_name:ident, $get_type:ty, $get_doc:expr) => {
        impl ItemEvent {
            #[doc = $set_doc]
            pub fn $get_name(&self) -> Arc<Mutex<$get_type>> {
                self.$name.clone()
            }

            #[doc = $get_doc]
            pub fn $name(mut self, $name: $set_type) -> Self {
                self.$name = Arc::new(Mutex::new($name));
                self
            }
        }
    };
}

macro_rules! impl_add_and_get {
    ($p_name:ident, $add_name:ident, $add_type:ty, $add_doc:expr, $get_name:ident, $get_type:ty, $get_doc:expr) => {
        impl ItemEvent {
            #[doc = $add_doc]
            pub fn $add_name(&self, f: $add_type) {
                self.$p_name.add(f);
            }

            #[doc = $get_doc]
            pub fn $get_name(&self) -> $get_type {
                self.$p_name.clone()
            }
        }
    };
}

macro_rules! multicast_event {
    ($event_name:ident, $($name:ident:$type:ty),*) => {
        #[derive(Clone, Default)]
        pub struct $event_name{
            inner: Arc<Mutex<Vec<Box<dyn FnMut($($type)*)>>>>
        }

        impl $event_name{
            pub fn new() -> Self {
                Self::default()
            }

            pub fn add(&self, f: impl FnMut($($type)*) + 'static) {
                let mut inner = self.inner.lock().unwrap();
                inner.push(Box::new(f));
            }

            pub fn invoke(&self, $($name:$type),*) {
                let mut inner = self.inner.lock().unwrap();
                inner.iter_mut().for_each(|f| f($($name),*));
            }
        }
    };
}

multicast_event!(OnClickEvent, source:ClickSource);

#[derive(Clone)]
pub struct ItemEvent {
    cursor_move: Arc<Mutex<dyn FnMut(&mut Item, f32, f32)>>,
    dispatch_cursor_move: Arc<Mutex<dyn FnMut(&mut Item, f32, f32)>>,
    dispatch_draw: Arc<Mutex<dyn FnMut(&mut Item, &mut Surface, f32, f32)>>,
    dispatch_focus: Arc<Mutex<dyn FnMut(&mut Item)>>,
    dispatch_keyboard_input: Arc<Mutex<dyn FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool>>,
    dispatch_layout: Arc<Mutex<dyn FnMut(&mut Item, f32, f32, f32, f32)>>,
    dispatch_mouse_input: Arc<Mutex<dyn FnMut(&mut Item, MouseEvent)>>,
    dispatch_timer: Arc<Mutex<dyn FnMut(&mut Item, usize) -> bool>>,
    dispatch_touch_input: Arc<Mutex<dyn FnMut(&mut Item, TouchEvent)>>,
    draw: Arc<Mutex<dyn FnMut(&mut Item, &Canvas)>>,
    ime_input: Arc<Mutex<dyn FnMut(&mut Item, ImeAction)>>,
    keyboard_input: Arc<Mutex<dyn FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool>>,
    layout: Arc<Mutex<dyn FnMut(&mut Item, f32, f32)>>,
    measure: Arc<Mutex<dyn FnMut(&mut Item, MeasureMode, MeasureMode)>>,
    mouse_input: Arc<Mutex<dyn FnMut(&mut Item, MouseEvent)>>,
    on_click: Arc<Mutex<dyn FnMut(&mut Item, ClickSource)>>,
    on_focus: Arc<Mutex<dyn FnMut(&mut Item, bool)>>,
    on_hover: Arc<Mutex<dyn FnMut(&mut Item, bool)>>,
    pointer_input: Arc<Mutex<dyn FnMut(&mut Item, PointerEvent)>>,
    timer: Arc<Mutex<dyn FnMut(&mut Item, usize) -> bool>>,
    touch_input: Arc<Mutex<dyn FnMut(&mut Item, TouchEvent)>>,
}

fn draw_item(item: &mut Item, surface: &mut Surface, x: f32, y: f32) {
    let clip = item.get_clip().get();
    if clip {
        let display_parameter = item.get_display_parameter();
        let x = display_parameter.x();
        let y = display_parameter.y();
        let width = display_parameter.width;
        let height = display_parameter.height;
        let canvas = surface.canvas();
        canvas.save();
        canvas.clip_rect(Rect::from_xywh(x, y, width, height), None, None);
    }
    item.dispatch_draw(surface, x, y);
    if clip {
        let canvas = surface.canvas();
        canvas.restore();
    }
}

impl ItemEvent {
    pub fn new() -> Self {
        Self {
            cursor_move: Arc::new(Mutex::new(|_item: &mut Item, _x: f32, _y: f32| {})),
            dispatch_cursor_move: Arc::new(Mutex::new({
                let mut is_hovered = false;
                move |item: &mut Item, x: f32, y: f32| {
                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.value().as_mut() {
                        foreground.dispatch_cursor_move(x, y);
                    }
                    let background = item.get_background();
                    if let Some(background) = background.value().as_mut() {
                        background.dispatch_cursor_move(x, y);
                    }

                    if let Some(on_cursor_move) = item.get_on_cursor_move() {
                        on_cursor_move(x, y);
                    }

                    item.get_item_event().get_cursor_move().lock().unwrap()(item, x, y);

                    if item.get_display_parameter().is_inside(x, y) {
                        if !is_hovered {
                            is_hovered = true;
                            item.get_item_event().get_on_hover().lock().unwrap()(item, true);
                            if let Some(on_hover) = item.get_on_hover() {
                                on_hover(true);
                            }
                        }
                    } else {
                        if is_hovered {
                            is_hovered = false;
                            item.get_item_event().get_on_hover().lock().unwrap()(item, false);
                            if let Some(on_hover) = item.get_on_hover() {
                                on_hover(false);
                            }
                        }
                    }

                    item.get_children().items().iter_mut().for_each(|child| {
                        child.dispatch_cursor_move(x, y);
                    });
                }
            })),
            dispatch_draw: Arc::new(Mutex::new(
                |item: &mut Item, surface: &mut Surface, parent_x: f32, parent_y: f32| {
                    {
                        // Set the parent position of the target parameter of the item.
                        // It's child items can use the parent position to calculate their own position.
                        // Why not update the parent position of the target parameter of the item in the layout event?
                        // Because animation can change the position of the item without notifying the layout event.
                        let target_parameter = item.get_target_parameter();
                        target_parameter.set_parent_position(parent_x, parent_y);
                    }

                    let display_parameter = item.get_display_parameter().clone();

                    {
                        // Draw the background blur effect.

                        let blur = 35.0;
                        let margin = blur * 2.0;
                        if item.get_enable_background_blur().get() {
                            let scale_factor = item.get_app_context().scale_factor();
                            let left = (display_parameter.x() * scale_factor - margin) as i32;
                            let top = (display_parameter.y() * scale_factor - margin) as i32;
                            let right = ((display_parameter.x() + display_parameter.width)
                                * scale_factor
                                + margin) as i32;
                            let bottom = ((display_parameter.y() + display_parameter.height)
                                * scale_factor
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
                            let mut paint = Paint::default();
                            paint.set_image_filter(image_filters::blur(
                                (blur, blur),
                                TileMode::Clamp,
                                None,
                                CropRect::from(Rect::from_wh(width as f32, height as f32)),
                            ));

                            let d = margin / scale_factor;
                            let mut x = display_parameter.x() - d;
                            let mut y = display_parameter.y() - d;
                            if x < 0.0 {
                                x = 0.0;
                            }
                            if y < 0.0 {
                                y = 0.0;
                            }

                            canvas.save();

                            canvas.clip_rect(
                                Rect::from_xywh(
                                    display_parameter.x(),
                                    display_parameter.y(),
                                    display_parameter.width,
                                    display_parameter.height,
                                ),
                                None,
                                None,
                            );
                            canvas.translate(Vector::new(x, y));
                            canvas.scale((1.0 / scale_factor, 1.0 / scale_factor));
                            canvas.draw_image(background, Point::new(0.0, 0.0), Some(&paint));
                            canvas.restore();
                        }
                    }

                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let rotation = display_parameter.rotation;

                    let skew_x = display_parameter.skew_x;
                    let skew_y = display_parameter.skew_y;
                    let skew_center_x = display_parameter.skew_center_x;
                    let skew_center_y = display_parameter.skew_center_y;
                    let scale_center_x = display_parameter.scale_center_x;
                    let scale_center_y = display_parameter.scale_center_y;

                    {
                        // Apply the transformation matrix to the canvas.
                        let canvas = surface.canvas();
                        canvas.save();
                        canvas.rotate(
                            rotation,
                            Some(Point::new(
                                display_parameter.rotation_center_x,
                                display_parameter.rotation_center_y,
                            )),
                        );

                        canvas.translate((skew_center_x, skew_center_y));
                        canvas.skew((skew_x, skew_y));
                        canvas.translate((-skew_center_x, -skew_center_y));

                        canvas.translate((scale_center_x, scale_center_y));
                        canvas.scale((display_parameter.scale_x, display_parameter.scale_y));
                        canvas.translate((-scale_center_x, -scale_center_y));
                    }

                    {
                        // Draw the background
                        let shared_background = item.get_background();
                        let mut background = shared_background.value();
                        if let Some(background) = background.as_mut() {
                            draw_item(background, surface, x, y);
                        }
                    }
                    {
                        // Draw the item itself.
                        let canvas = surface.canvas();
                        item.draw(canvas);
                    }

                    // Draw the children of the item.
                    item.get_children().items().iter_mut().for_each(|child| {
                        draw_item(child, surface, x, y);
                    });

                    {
                        // Draw the foreground
                        let shared_foreground = item.get_foreground();
                        let mut foreground = shared_foreground.value();
                        if let Some(foreground) = foreground.as_mut() {
                            draw_item(foreground, surface, x, y);
                        }
                    }
                    {
                        // Restore the transformation matrix of the canvas.
                        let canvas = surface.canvas();
                        canvas.restore();
                    }
                },
            )),
            dispatch_focus: Arc::new(Mutex::new(|item: &mut Item| {
                let focus_changed_items = item.get_app_context().focus_changed_items;
                let focused = item.get_focused().get();
                {
                    let focus_changed_items = focus_changed_items.value();
                    if focus_changed_items.contains(&item.get_id()) {
                        item.focus(focused)
                    }
                }
                item.get_children().items().iter_mut().for_each(|child| {
                    child.dispatch_focus();
                });
            })),
            dispatch_keyboard_input: Arc::new(Mutex::new(
                |item: &mut Item, device_id: DeviceId, event: KeyEvent, is_synthetic: bool| {
                    let keyboard_input = item.get_item_event().get_keyboard_input();
                    if keyboard_input.lock().unwrap()(item, device_id, event.clone(), is_synthetic)
                    {
                        return true;
                    }
                    item.get_children().items().iter_mut().any(|child| {
                        let dispatch_keyboard_input =
                            child.get_item_event().get_dispatch_keyboard_input();
                        let r = dispatch_keyboard_input.lock().unwrap()(
                            child,
                            device_id,
                            event.clone(),
                            is_synthetic,
                        );
                        r
                    })
                },
            )),
            dispatch_layout: Arc::new(Mutex::new(
                |item: &mut Item, relative_x: f32, relative_y: f32, width: f32, height: f32| {
                    let offset_x = item.get_offset_x().get();
                    let offset_y = item.get_offset_y().get();
                    let opacity = item.get_opacity().get();
                    let rotation = item.get_rotation().get();
                    let scale_x = item.get_scale_x().get();
                    let scale_y = item.get_scale_y().get();
                    let skew_x = item.get_skew_x().get();
                    let skew_y = item.get_skew_y().get();

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
                        let rotation_center_x = center(item.get_rotation_center_x().get(), width);
                        let rotation_center_y = center(item.get_rotation_center_y().get(), height);
                        let scale_center_x = center(item.get_scale_center_x().get(), width);
                        let scale_center_y = center(item.get_scale_center_y().get(), height);
                        let skew_center_x = center(item.get_skew_center_x().get(), width);
                        let skew_center_y = center(item.get_skew_center_y().get(), height);

                        {
                            let mut target_parameter = item.get_target_parameter();
                            target_parameter.set_relative_position(relative_x, relative_y);
                            target_parameter.width = width;
                            target_parameter.height = height;
                            target_parameter.opacity = opacity;
                            target_parameter.rotation = rotation;
                            target_parameter
                                .set_rotation_center(rotation_center_x, rotation_center_y);
                            target_parameter.set_scale(scale_x, scale_y);
                            target_parameter.set_scale_center(scale_center_x, scale_center_y);
                            target_parameter.set_offset(offset_x, offset_y);
                            target_parameter.set_skew(skew_x, skew_y);
                            target_parameter.set_skew_center(skew_center_x, skew_center_y);
                            // item.set_target_parameter(target_parameter);
                        }
                    }

                    let horizontal_padding = item.get_padding(Orientation::Horizontal);
                    let vertical_padding = item.get_padding(Orientation::Vertical);
                    item.layout_layers(width - horizontal_padding, height - vertical_padding);

                    item.layout(width, height);
                },
            )),
            dispatch_mouse_input: Arc::new(Mutex::new({
                // The mouse button that the item has captured.
                // When the item captures a mouse button, the item can receive mouse input
                // events even if the mouse pointer is outside the item.
                let mut captured_mouse_button: HashSet<(usize, MouseButton)> = HashSet::new();
                // The source of the click event.
                let mut click_source: Option<ClickSource> = None;
                move |item: &mut Item, event: MouseEvent| {
                    let x = event.x;
                    let y = event.y;

                    // If the item captures the mouse button,
                    // the foreground and background of the item can receive all mouse input events.
                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.value().as_mut() {
                        foreground.dispatch_mouse_input(event);
                    }

                    let background = item.get_background();
                    if let Some(background) = background.value().as_mut() {
                        background.dispatch_mouse_input(event);
                    }

                    // Call the on_mouse_input event of the item.
                    if let Some(on_mouse_input) = item.get_on_mouse_input() {
                        on_mouse_input(event);
                    }

                    {
                        // Call the mouse_input event of the item_event.
                        // Why there are two mouse_input events?
                        // Because winia don't want to expose item object to the user.
                        let f = item.get_item_event().mouse_input.clone();
                        let mut mouse_input = f.lock().unwrap();
                        mouse_input(item, event);
                    }

                    {
                        // Dispatch the mouse input events to the child items.
                        let children = item.get_children();
                        for child in children.items().iter_mut().rev() {
                            let display_parameter = child.get_display_parameter();
                            match event.pointer_state {
                                PointerState::Started => {
                                    // If the mouse pointer is inside the child item,
                                    if display_parameter.is_inside(x, y) {
                                        // The child item captures the mouse button.
                                        captured_mouse_button
                                            .insert((child.get_id(), event.button));
                                        child.dispatch_mouse_input(event);
                                        // Other child items can't receive the mouse input events.
                                        return;
                                    }
                                }
                                PointerState::Moved => {
                                    // If the child item captures the mouse button
                                    if captured_mouse_button
                                        .contains(&(child.get_id(), event.button))
                                    {
                                        child.dispatch_mouse_input(event);
                                        return;
                                    }
                                }
                                PointerState::Ended | PointerState::Canceled => {
                                    // If the child item captures the mouse button
                                    if captured_mouse_button
                                        .contains(&(child.get_id(), event.button))
                                    {
                                        // The child item releases the mouse button.
                                        captured_mouse_button
                                            .remove(&(child.get_id(), event.button));
                                        child.dispatch_mouse_input(event);
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    // Handle the click event.
                    match event.pointer_state {
                        PointerState::Started => {
                            click_source.replace(ClickSource::Mouse(event.button));
                        }
                        PointerState::Ended => {
                            if item.get_display_parameter().is_inside(x, y) {
                                let is_clicked = {
                                    click_source.map_or(false, |click_source| {
                                        click_source == ClickSource::Mouse(event.button)
                                    })
                                };
                                if is_clicked {
                                    item.get_item_event().get_on_click().lock().unwrap()(
                                        item,
                                        click_source.unwrap(),
                                    );
                                    if let Some(on_click) = item.get_on_click() {
                                        on_click(click_source.unwrap());
                                    }
                                }
                                click_source.take();
                            }
                        }
                        _ => {}
                    }

                    item.get_item_event().get_pointer_input().lock().unwrap()(item, event.into());
                    if let Some(on_pointer_input) = item.get_on_pointer_input() {
                        on_pointer_input(event.into())
                    }
                }
            })),
            dispatch_timer: Arc::new(Mutex::new(|item: &mut Item, id: usize| {
                let timer = item.get_item_event().get_timer();
                if timer.lock().unwrap()(item, id) {
                    return true;
                }
                item.get_children().items().iter_mut().any(|child| {
                    let dispatch_timer = child.get_item_event().get_dispatch_timer();
                    let r = dispatch_timer.lock().unwrap()(child, id);
                    r
                })
            })),
            dispatch_touch_input: Arc::new(Mutex::new({
                // item_id, touch_id
                let mut captured_touch_pointer: HashSet<(usize, u64)> = HashSet::new();
                let mut touch_start_time = Instant::now();
                move |item: &mut Item, event: TouchEvent| {
                    let x = event.x;
                    let y = event.y;

                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.value().as_mut() {
                        foreground.dispatch_touch_input(event);
                    }
                    let background = item.get_background();
                    if let Some(background) = background.value().as_mut() {
                        background.dispatch_touch_input(event);
                    }

                    if let Some(on_touch) = &mut item.get_on_touch_input() {
                        on_touch(event);
                    }

                    {
                        let children = item.get_children();
                        for child in children.items().iter_mut().rev() {
                            let display_parameter = child.get_display_parameter();
                            match event.pointer_state {
                                PointerState::Started => {
                                    if display_parameter.is_inside(x, y) {
                                        captured_touch_pointer.insert((child.get_id(), event.id));
                                        child.dispatch_touch_input(event);
                                        return;
                                    }
                                }
                                PointerState::Moved => {
                                    if captured_touch_pointer.contains(&(child.get_id(), event.id))
                                    {
                                        child.dispatch_touch_input(event);
                                        return;
                                    }
                                }
                                PointerState::Ended | PointerState::Canceled => {
                                    if captured_touch_pointer.contains(&(child.get_id(), event.id))
                                    {
                                        let child_id = child.get_id();
                                        captured_touch_pointer
                                            .retain(|&(item_id, touch_id)| item_id != child_id);
                                        child.dispatch_touch_input(event);
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    match event.pointer_state {
                        PointerState::Started => {
                            touch_start_time = Instant::now();
                        }
                        PointerState::Ended => {
                            let elapsed_time = touch_start_time.elapsed().as_millis();
                            let click_source = if elapsed_time < 300 {
                                ClickSource::Touch
                            } else {
                                ClickSource::LongTouch
                            };
                            if let Some(on_click) = item.get_on_click() {
                                on_click(click_source);
                            }
                        }
                        _ => {}
                    }

                    item.get_item_event().get_pointer_input().lock().unwrap()(item, event.into());
                    if let Some(on_pointer_input) = item.get_on_pointer_input() {
                        on_pointer_input(event.into());
                    }
                }
            })),
            draw: Arc::new(Mutex::new(|_item: &mut Item, _canvas: &Canvas| {})),
            ime_input: Arc::new(Mutex::new(|_item: &mut Item, _action: ImeAction| {})),
            keyboard_input: Arc::new(Mutex::new(
                |_item: &mut Item, _device_id: DeviceId, _event: KeyEvent, _is_synthetic: bool| {
                    false
                },
            )),
            layout: Arc::new(Mutex::new(|_item: &mut Item, _width: f32, _height: f32| {})),
            measure: Arc::new(Mutex::new(|item: &mut Item, width_mode, height_mode| {
                item.measure_children(width_mode, height_mode);
                fn get_size(measure_mode: MeasureMode) -> f32 {
                    match measure_mode {
                        MeasureMode::Specified(value) => value,
                        MeasureMode::Unspecified(max_size) => max_size,
                    }
                }

                let max_width = item.get_max_width().get();
                let max_height = item.get_max_height().get();
                let min_width = item.get_min_width().get();
                let min_height = item.get_min_height().get();
                let measure_parameter = item.get_measure_parameter();
                measure_parameter.width = get_size(width_mode).clamp(min_width, max_width);
                measure_parameter.height = get_size(height_mode).clamp(min_height, max_height);
            })),
            mouse_input: Arc::new(Mutex::new(|_item: &mut Item, _event: MouseEvent| {})),
            on_click: Arc::new(Mutex::new(|_item: &mut Item, _source: ClickSource| {})),
            on_focus: Arc::new(Mutex::new(|_item: &mut Item, _focused: bool| {})),
            on_hover: Arc::new(Mutex::new(|_item: &mut Item, _hover: bool| {})),
            pointer_input: Arc::new(Mutex::new(|_item: &mut Item, _event: PointerEvent| {})),
            timer: Arc::new(Mutex::new(|_item: &mut Item, _id: usize| false)),
            touch_input: Arc::new(Mutex::new(|_item: &mut Item, _event: TouchEvent| {})),
        }
    }
}

impl_get_set!(
    cursor_move,
    impl FnMut(&mut Item, f32, f32) + 'static,
    "item, x, y",
    get_cursor_move,
    dyn FnMut(&mut Item, f32, f32),
    "item, x, y"
);

impl_get_set!(
    dispatch_cursor_move,
    impl FnMut(&mut Item, f32, f32) + 'static,
    "item, x, y",
    get_dispatch_cursor_move,
    dyn FnMut(&mut Item, f32, f32),
    "item, x, y"
);

impl_get_set!(
    dispatch_draw,
    impl FnMut(&mut Item, &mut Surface, f32, f32) + 'static,
    "item, surface, x, y",
    get_dispatch_draw,
    dyn FnMut(&mut Item, &mut Surface, f32, f32),
    "item, surface, x, y"
);

impl_get_set!(
    dispatch_focus,
    impl FnMut(&mut Item) + 'static,
    "item",
    get_dispatch_focus,
    dyn FnMut(&mut Item),
    "item"
);

impl_get_set!(
    dispatch_keyboard_input,
    impl FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool + 'static,
    "item, device_id, event, is_synthetic",
    get_dispatch_keyboard_input,
    dyn FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool,
    "item, device_id, event, is_synthetic"
);

impl_get_set!(
    dispatch_layout,
    impl FnMut(&mut Item, f32, f32, f32, f32) + 'static,
    "item, relative_x, relative_y, width, height",
    get_dispatch_layout,
    dyn FnMut(&mut Item, f32, f32, f32, f32),
    "item, relative_x, relative_y, width, height"
);

impl_get_set!(
    dispatch_mouse_input,
    impl FnMut(&mut Item, MouseEvent) + 'static,
    "item, event",
    get_dispatch_mouse_input,
    dyn FnMut(&mut Item, MouseEvent),
    "item, event"
);

impl_get_set!(
    dispatch_timer,
    impl FnMut(&mut Item, usize) -> bool + 'static,
    "item, id",
    get_dispatch_timer,
    dyn FnMut(&mut Item, usize) -> bool,
    "item, id"
);

impl_get_set!(
    dispatch_touch_input,
    impl FnMut(&mut Item, TouchEvent) + 'static,
    "item, event",
    get_dispatch_touch_input,
    dyn FnMut(&mut Item, TouchEvent),
    "item, event"
);

impl_get_set!(
    draw,
    impl FnMut(&mut Item, &Canvas) + 'static,
    "item, canvas",
    get_draw,
    dyn FnMut(&mut Item, &Canvas),
    "item, canvas"
);

impl_get_set!(
    ime_input,
    impl FnMut(&mut Item, ImeAction) + 'static,
    "item, action",
    get_ime_input,
    dyn FnMut(&mut Item, ImeAction),
    "item, action"
);

impl_get_set!(
    keyboard_input,
    impl FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool + 'static,
    "item, device_id, event, is_synthetic",
    get_keyboard_input,
    dyn FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool,
    "item, device_id, event, is_synthetic"
);

impl_get_set!(
    layout,
    impl FnMut(&mut Item, f32, f32) + 'static,
    "item, width, height",
    get_layout,
    dyn FnMut(&mut Item, f32, f32),
    "item, width, height"
);

impl_get_set!(
    measure,
    impl FnMut(&mut Item, MeasureMode, MeasureMode) + 'static,
    r#"item, width_mode, height_mode
Do not retain any state in this closure, except for the `measure_parameter`.
Because this closure is used to calculate the recommended size of the item,
the `layout` closure is actually responsible for setting the actual size of the item.
"#,
    get_measure,
    dyn FnMut(&mut Item, MeasureMode, MeasureMode) + 'static,
    "item, width_mode, height_mode"
);

impl_get_set!(
    mouse_input,
    impl FnMut(&mut Item, MouseEvent) + 'static,
    "item, event",
    get_mouse_input,
    dyn FnMut(&mut Item, MouseEvent),
    "item, event"
);

impl_get_set!(
    on_click,
    impl FnMut(&mut Item, ClickSource) + 'static,
    "item, source",
    get_on_click,
    dyn FnMut(&mut Item, ClickSource),
    "item, source"
);

impl_get_set!(
    on_focus,
    impl FnMut(&mut Item, bool) + 'static,
    "item, focused",
    get_on_focus,
    dyn FnMut(&mut Item, bool),
    "item, focused"
);

impl_get_set!(
    on_hover,
    impl FnMut(&mut Item, bool) + 'static,
    "item, hover_state",
    get_on_hover,
    dyn FnMut(&mut Item, bool),
    "item, hover_state"
);

impl_get_set!(
    pointer_input,
    impl FnMut(&mut Item, PointerEvent) + 'static,
    "item, event",
    get_pointer_input,
    dyn FnMut(&mut Item, PointerEvent),
    "item, event"
);

impl_get_set!(
    timer,
    impl FnMut(&mut Item, usize) -> bool + 'static,
    "item, id",
    get_timer,
    dyn FnMut(&mut Item, usize) -> bool,
    "item, id"
);

impl_get_set!(
    touch_input,
    impl FnMut(&mut Item, TouchEvent) + 'static,
    "item, event",
    get_touch_input,
    dyn FnMut(&mut Item, TouchEvent),
    "item, event"
);

impl Default for ItemEvent {
    fn default() -> Self {
        Self::new()
    }
}
