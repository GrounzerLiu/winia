use crate::ui::item::{FocusState, ItemData, ItemState, Size};
use skia_safe::image_filters::CropRect;
use skia_safe::{
    Canvas, IRect, Paint, PictureRecorder, Point, Rect, Surface, TileMode, Vector, image_filters,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use tklog::debug;
use winit::dpi::LogicalPosition;
use winit::event::{DeviceId, Force, KeyEvent, Modifiers, MouseButton, TouchPhase};
use crate::dpi::PhysicalPosition;
use crate::event::{ButtonSource, ElementState, PointerSource};
use crate::{Let, With};
use crate::shared::{SharedBool, SharedSource};
use crate::ui::InnerPosition;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeasureMode {
    /// Indicates that the parent has determined an exact size for the child.
    Specified(f32),
    /// Indicates that the child can determine its own size. The value of this enum is the maximum size the child can use.
    Unspecified(f32),
}

impl MeasureMode {
    pub fn from_size(size: Size, max: f32) -> Self {
        match size {
            Size::Auto => MeasureMode::Unspecified(max),
            Size::Fill => MeasureMode::Specified(max),
            Size::Fixed(size) => MeasureMode::Specified(size),
            Size::Relative(ratio) => MeasureMode::Specified(max * ratio.clamp(0.0, f32::MAX)),
        }
    }
}

impl Into<f32> for MeasureMode {
    fn into(self) -> f32 {
        match self {
            MeasureMode::Specified(v) => v,
            MeasureMode::Unspecified(v) => v,
        }
    }
}


#[derive(Clone, Debug)]
pub struct PointerButton {
    pub device_id: Option<DeviceId>,
    pub state: ElementState,
    pub position: LogicalPosition<f32>,
    pub primary: bool,
    pub button: ButtonSource,
}

#[derive(Clone, Debug)]
pub struct PointerMoved {
    pub device_id: Option<DeviceId>,
    pub position: LogicalPosition<f32>,
    pub primary: bool,
    pub source: PointerSource,
}

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

#[derive(Clone, Debug)]
pub enum ImeAction {
    Enabled,
    Enter,
    Delete,
    PreEdit(String, Option<(usize, usize)>),
    Commit(String),
    Disabled,
    DeleteSurrounding {
        /// Bytes to remove before the selection
        before_bytes: usize,
        /// Bytes to remove after the selection
        after_bytes: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseScrollDelta {
    /// Amount in lines or rows to scroll in the horizontal
    /// and vertical directions.
    ///
    /// Positive values indicate that the content that is being scrolled should move
    /// right and down (revealing more content left and up).
    LineDelta(f32),

    /// Amount in pixels to scroll in the horizontal and
    /// vertical direction.
    ///
    /// Scroll events are expressed as a `LogicalDelta` if
    /// supported by the device (e.g. a touchpad) and
    /// platform.
    ///
    /// Positive values indicate that the content being scrolled should
    /// move right/down.
    ///
    /// For a 'natural scrolling' touchpad (that acts like a touch screen)
    /// this means moving your fingers right and down should give positive values,
    /// and move the content right and down (to reveal more things left and up).
    LogicalDelta(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseWheel {
    pub device_id: Option<DeviceId>,
    pub delta: MouseScrollDelta,
    pub phase: TouchPhase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorMove {
    pub device_id: DeviceId,
    pub x: f32,
    pub y: f32,
    pub is_left_window: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardInput {
    pub device_id: Option<DeviceId>,
    pub key_event: KeyEvent,
    pub is_synthetic: bool,
}

impl MeasureMode {
    pub fn value(self) -> f32 {
        match self {
            MeasureMode::Specified(value) => value,
            MeasureMode::Unspecified(value) => value,
        }
    }
}

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
    pub cursor_move: Event<dyn FnMut(&mut ItemData, &CursorMove)>,
    pub click_input: Event<dyn FnMut(&mut ItemData, &ButtonSource)>,
    // pub dispatch_cursor_move: Event<dyn FnMut(&mut ItemData, &CursorMove)>,
    pub dispatch_draw: Event<dyn FnMut(&mut ItemData, &mut Surface, f32, f32)>,
    pub dispatch_focus: Event<dyn FnMut(&mut ItemData, u32, bool) -> bool>,
    pub dispatch_ime_input: Event<dyn FnMut(&mut ItemData, &ImeAction) -> bool>,
    pub dispatch_keyboard_input: Event<dyn FnMut(&mut ItemData, &KeyboardInput) -> bool>,
    pub dispatch_layout: Event<dyn FnMut(&mut ItemData, f32, f32, f32, f32)>,
    pub dispatch_measure: Event<dyn FnMut(&mut ItemData, MeasureMode, MeasureMode)>,
    pub dispatch_modifiers_change: Event<dyn FnMut(&mut ItemData, &Modifiers)>,
    // pub dispatch_mouse_input: Event<dyn FnMut(&mut ItemData, &MouseInput) -> bool>,
    pub dispatch_mouse_wheel: Event<dyn FnMut(&mut ItemData, Option<MouseWheel>, Option<MouseWheel>) -> (Option<MouseWheel>, Option<MouseWheel>)>,
    // pub dispatch_touch_input: Event<dyn FnMut(&mut ItemData, &TouchInput) -> bool>,
    pub dispatch_pointer_button: Event<dyn FnMut(&mut ItemData, &PointerButton) -> bool>,
    pub dispatch_pointer_moved: Event<dyn FnMut(&mut ItemData, &PointerMoved)>,
    pub draw: Event<dyn FnMut(&mut ItemData, &Canvas)>,
    pub hover_changed: Event<dyn FnMut(&mut ItemData, bool)>,
    pub focus_changed: Event<dyn FnMut(&mut ItemData, &FocusState)>,
    pub ime_input: Event<dyn FnMut(&mut ItemData, &ImeAction)>,
    pub keyboard_input: Event<dyn FnMut(&mut ItemData, &KeyboardInput)>,
    pub layout: Event<dyn FnMut(&mut ItemData, f32, f32)>,
    pub measure: Event<dyn FnMut(&mut ItemData, MeasureMode, MeasureMode)>,
    pub modifiers_change: Event<dyn FnMut(&mut ItemData, &Modifiers)>,
    // pub mouse_input: Event<dyn FnMut(&mut ItemData, &MouseInput) -> bool>,
    pub mouse_wheel: Event<dyn FnMut(&mut ItemData, Option<MouseWheel>, Option<MouseWheel>) -> (Option<MouseWheel>, Option<MouseWheel>)>,
    pub pointer_button: Event<dyn FnMut(&mut ItemData, &PointerButton) -> bool>,
    pub pointer_moved: Event<dyn FnMut(&mut ItemData, &PointerMoved)>,
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
            cursor_move: event!({ |_item: &mut ItemData, _cursor_move: &CursorMove| {} }),
            click_input: event!({ |_item: &mut ItemData, _source: &ButtonSource| {} }),
            /*            dispatch_cursor_move: event!({
                            let mut is_hovered = false;
                            move |item: &mut ItemData, cursor_move: &CursorMove| {
                                if !item.props().enable.get() {
                                    return;
                                }
                                {
                                    let foreground = item.props().foreground.clone();
                                    let mut foreground_lock = foreground.lock();
                                    if let Some(fg) = foreground_lock.as_mut() {
                                        fg.data().dispatch_cursor_move(cursor_move);
                                    }
                                }
                                {
                                    let background = item.props().background.clone();
                                    let mut background_lock = background.lock();
                                    if let Some(bg) = background_lock.as_mut() {
                                        bg.data().dispatch_cursor_move(cursor_move);
                                    }
                                }
                                item.on_cursor_move(cursor_move);
                                item.cursor_move(cursor_move);

                                if item.current_frame().contains(cursor_move.x, cursor_move.y)
                                    && !cursor_move.is_left_window
                                {
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
            /*                        if item.props().item_state.get() == ItemState::Hovered {
                                        if item.focus_state.is_focused {
                                            item.on_state_changed(ItemState::Focused);
                                        } else {
                                            item.on_state_changed(ItemState::Enabled);
                                        }
                                    }*/
                                }

                                item.children.lock().iter_mut().for_each(|child| {
                                    child.data().dispatch_cursor_move(cursor_move);
                                });
                            }
                        }),*/
            dispatch_draw: event!({
                let mut is_animating = false;
                let mut image_filter_paint = Paint::default();
                let mut shadow_paint = Paint::default();
                let mut last_parent_x: f32 = 0.0;
                let mut last_parent_y: f32 = 0.0;
                move |item: &mut ItemData, surface: &mut Surface, parent_x: f32, parent_y: f32| {
                    let current_frame = item.current_frame();
                    item.target_frame.set_parent_position(parent_x, parent_y);

                    let clipped = item.props.clipped.get();
                    let clip_shape = {
                        let current_frame = item.current_frame();
                        let shape = item.props.clip_shape.lock();
                        shape.as_ref().map(|shape| shape(&current_frame))
                    };
                    {
                        // Draw the background blur effect.
                        let blur = current_frame.get_float_param("blur").unwrap_or(35.0);
                        // let blur = /*35.0*/item.props.blur.get();
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
                            // canvas.save();
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

                        // canvas.translate((-scale_center_x, -scale_center_y));
                        canvas.scale((scale_x, scale_y));
                        canvas.translate((
                            -(scale_x - 1.0) * scale_center_x / scale_x,
                            -(scale_y - 1.0) * scale_center_y / scale_y,
                        ));
                        // if item.get_name() == "blue" {
                        //     canvas.scale((scale_x, scale_y));
                        //     canvas.translate((-(scale_x * 150.0 - 150.0) / scale_x, 0.0));
                        // }
                    }

                    let clipped = item.props.clipped.get();
                    let clip_shape = {
                        let current_frame = item.current_frame();
                        let shape = item.props.clip_shape.lock();
                        shape.as_ref().map(|shape| shape(&current_frame))
                    };

                    if is_animating != item.animations.is_animating() {
                        item.props.item_updater.lock().need_redraw = true;
                        is_animating = item.animations.is_animating();
                    }
                    if (item.props.item_updater.lock().need_redraw && !item.animations.is_animating())
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
                            surface.canvas().save();
                            surface.canvas().clip_path(clip_shape, None, true);
                        }
                    }
                    {
                        let background = item.props.background.clone();
                        let mut background_lock = background.lock();
                        if let Some(bg) = background_lock.as_mut() {
                            bg.data().dispatch_draw(
                                surface,
                                item.target_frame.x(),
                                item.target_frame.y(),
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
                        let mut children_lock = item.children.lock();
                        for child in children_lock.iter_mut() {
                            child.data().dispatch_draw(
                                surface,
                                item.target_frame.x(),
                                item.target_frame.y(),
                            );
                        }
                    }
                    {
                        let foreground = item.props.foreground.clone();
                        let mut foreground_lock = foreground.lock();
                        if let Some(fg) = foreground_lock.as_mut() {
                            fg.data().dispatch_draw(
                                surface,
                                item.target_frame.x(),
                                item.target_frame.y(),
                            );
                        }
                    }

                    {
                        if let Some(_clip_shape) = &clip_shape
                            && clipped
                        {
                            surface.canvas().restore();
                        }
                    }

                    {
                        // Restore the transformation matrix of the canvas.
                        let canvas = surface.canvas();
                        canvas.restore();
                    }

                    item.props.item_updater.lock().need_redraw = false;
                }
            }),
            dispatch_focus: event!({
                |item: &mut ItemData, item_id: u32, has_parent_focus: bool| {
                    let mut is_focus_changed = false;
                    if item.id == item_id && !item.focus_state.is_focused {
                        item.focus_state.is_focused = true;
                        item.focus_state.has_focus = true;
                        is_focus_changed = true;
                    } else if item.id != item_id && item.focus_state.is_focused {
                        item.focus_state.is_focused = false;
                        is_focus_changed = true;
                    }
                    if has_parent_focus && !item.focus_state.has_parent_focus {
                        item.focus_state.has_parent_focus = true;
                        is_focus_changed = true;
                    } else if !has_parent_focus && item.focus_state.has_parent_focus {
                        item.focus_state.has_parent_focus = false;
                        is_focus_changed = true;
                    }
                    let mut has_focus = false;
                    let mut children_lock = item.children.lock();
                    let is_focused = item.focus_state.is_focused;
                    for child in children_lock.iter_mut() {
                        has_focus |= child
                            .data()
                            .dispatch_focus(item_id, has_parent_focus || is_focused);
                    }
                    drop(children_lock);
                    if has_focus && !item.focus_state.has_focus {
                        item.focus_state.has_focus = true;
                        is_focus_changed = true;
                    } else if !has_focus
                        && item.focus_state.has_focus
                        && !item.focus_state.is_focused
                    {
                        item.focus_state.has_focus = false;
                        is_focus_changed = true;
                    }
                    if is_focus_changed {
                        let focus_state = item.focus_state;
                        item.focus_changed(&focus_state);
                        item.on_focus_changed(&focus_state);
                        if focus_state.is_focused {
                            item.on_state_changed(ItemState::Focused);
                        } else if item.props().item_state.get() == ItemState::Focused {
                            item.on_state_changed(ItemState::Enabled);
                        }
                    }
                    item.focus_state.has_focus
                }
            }),
            dispatch_ime_input: event!({
                |item: &mut ItemData, action: &ImeAction| {
                    if item.focus_state.is_focused {
                        item.ime_input(action);
                        return true;
                    }
                    let children = item.children.lock();
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
                    if item.focus_state.is_focused {
                        item.keyboard_input(input);
                        return true;
                    }
                    let children = item.children.lock();
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
                        let children = item.children.lock();
                        for child in children.iter() {
                            child.data().re_layout()
                        }
                        let background = item.props.background.lock();
                        if let Some(bg) = background.as_ref() {
                            bg.data().re_layout();
                        }
                        let foreground = item.props.foreground.lock();
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
                        let target_frame = &mut item.target_frame;
                        let blur = item.props.blur.get();
                        target_frame.set_float_param("blur", blur);
                    }

                    {
                        let background = item.props.background.clone();
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
                        let foreground = item.props.foreground.clone();
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
                }
            }),
            dispatch_measure: event!({
                move |item: &mut ItemData, width_mode: MeasureMode, height_mode: MeasureMode| {
                    let needs_redraw = item.props().item_updater.lock().need_redraw;
                    if !needs_redraw
                        && item.last_measure_width_mode == Some(width_mode)
                        && item.last_measure_height_mode == Some(height_mode)
                    {
                        let children = item.children.lock();
                        for child in children.iter() {
                            child.data().re_measure(width_mode.value(), height_mode.value());
                        }
                        let background = item.props.background.lock();
                        if let Some(bg) = background.as_ref() {
                            bg.data().re_measure(width_mode.value(), height_mode.value());
                        }
                        let foreground = item.props.foreground.lock();
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
            dispatch_modifiers_change: event!({
                |_item: &mut ItemData, _modifiers: &Modifiers| {}
            }),
            /*            dispatch_mouse_input: event!({
                            // The mouse button that the item has captured.
                            // When the item captures a mouse button, the item can receive mouse input
                            // events even if the mouse pointer is outside the item.
                            let mut captured_mouse_button: HashSet<MouseButton> = HashSet::new();
                            // The source of the click event.
                            let mut click_source: Option<ClickSource> = None;
                            move |item: &mut ItemData, input: &MouseInput| {
                                if !item.props.enable.get() {
                                    return false;
                                }

                                match input.pointer_state {
                                    PointerState::Started => {
                                        if item.props.enable.get() {
                                            item.on_state_changed(ItemState::Pressed);
                                        }
                                    }
                                    PointerState::Ended | PointerState::Cancelled => {
                                        let item_state = item.props.item_state.clone();
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
                                    let foreground = item.props.foreground.clone();
                                    let mut foreground_lock = foreground.lock();
                                    if let Some(fg) = foreground_lock.as_mut() {
                                        fg.data().dispatch_mouse_input(input);
                                    }
                                    let background = item.props.background.clone();
                                    let mut background_lock = background.lock();
                                    if let Some(bg) = background_lock.as_mut() {
                                        bg.data().dispatch_mouse_input(input);
                                    }
                                }

                                let mut children = item.children.lock();
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
                                        return item.props.on_click.is_some();
                                    }
                                    PointerState::Moved => {
                                        if captured_mouse_button.contains(&input.button) {
                                            return item.props.on_click.is_some();
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
                                        return item.props.on_click.is_some();
                                    }
                                }
                                false
                            }
                        }),*/
            dispatch_mouse_wheel: event!({
                |item: &mut ItemData,
                 mouse_wheel_x: Option<MouseWheel>,
                 mouse_wheel_y: Option<MouseWheel>| {
                    if mouse_wheel_x.is_none() && mouse_wheel_y.is_none() {
                        (None, None)
                    } else {
                        let children = item.children().lock();
                        let mut mouse_wheel_x = mouse_wheel_x;
                        let mut mouse_wheel_y = mouse_wheel_y;
                        for child in children.iter().rev() {
                            let (new_mouse_wheel_x, new_mouse_wheel_y) = child
                                .data()
                                .dispatch_mouse_wheel(mouse_wheel_x, mouse_wheel_y);
                            mouse_wheel_x = new_mouse_wheel_x;
                            mouse_wheel_y = new_mouse_wheel_y;
                            if mouse_wheel_x.is_none() && mouse_wheel_y.is_none() {
                                break;
                            }
                        }
                        drop(children);
                        if mouse_wheel_x.is_some() || mouse_wheel_y.is_some() {
                            item.mouse_wheel(mouse_wheel_x, mouse_wheel_y);
                        }
                        (mouse_wheel_x, mouse_wheel_y)
                    }
                }
            }),
            // dispatch_touch_input: event!({ |_item: &mut ItemData, _input: &TouchInput| { false } }),
            dispatch_pointer_button: event!({
                let mut pressed_pointers: HashSet<ButtonSourceHashWrapper> = HashSet::new();
                move |item: &mut ItemData, pointer_button: &PointerButton| {
                    let current_frame = item.current_frame();
                    let button_wrapper = ButtonSourceHashWrapper(pointer_button.button.clone());
                    if !item.props.enable.get() {
                        return false;
                    }

                    match pointer_button.state {
                        ElementState::Pressed => {
                            pressed_pointers.remove(&button_wrapper);
                            if !current_frame.contains(pointer_button.position.x, pointer_button.position.y) {
                                return false;
                            }
                            pressed_pointers.insert(button_wrapper.clone());
                            item.on_state_changed(ItemState::Pressed);
                            {
                                let foreground = item.props.foreground.clone();
                                let mut foreground_lock = foreground.lock();
                                if let Some(fg) = foreground_lock.as_mut() {
                                    fg.data().dispatch_pointer_button(pointer_button);
                                }
                                let background = item.props.background.clone();
                                let mut background_lock = background.lock();
                                if let Some(bg) = background_lock.as_mut() {
                                    bg.data().dispatch_pointer_button(pointer_button);
                                }
                            }
                            let mut children = item.children.lock();
                            for child in children.iter_mut().rev() {
                                if child.data().dispatch_pointer_button(pointer_button) {
                                    return true;
                                }
                            }
                            drop(children);
                            if item.on_pointer_button(pointer_button) {
                                return true;
                            }
                            if item.pointer_button(pointer_button) {
                                return true;
                            }
                            return item.props.on_click.is_some();
                        }
                        ElementState::Released => {
                            if !pressed_pointers.contains(&button_wrapper) {
                                return false;
                            }
                            pressed_pointers.remove(&button_wrapper);
                            let item_state = item.props.item_state.clone();
                            if item_state.get() == ItemState::Pressed {
                                if item.focus_state.is_focused {
                                    item.on_state_changed(ItemState::Focused);
                                } else {
                                    item.on_state_changed(ItemState::Enabled);
                                }
                            }
                            {
                                let foreground = item.props.foreground.clone();
                                let mut foreground_lock = foreground.lock();
                                if let Some(fg) = foreground_lock.as_mut() {
                                    fg.data().dispatch_pointer_button(pointer_button);
                                }
                                let background = item.props.background.clone();
                                let mut background_lock = background.lock();
                                if let Some(bg) = background_lock.as_mut() {
                                    bg.data().dispatch_pointer_button(pointer_button);
                                }
                            }
                            let mut children = item.children.lock();
                            for child in children.iter_mut().rev() {
                                if child.data().dispatch_pointer_button(pointer_button) {
                                    return true;
                                }
                            }
                            drop(children);
                            if item.on_pointer_button(pointer_button) {
                                return true;
                            }
                            if item.pointer_button(pointer_button) {
                                return true;
                            }
                            if current_frame.contains(pointer_button.position.x, pointer_button.position.y) {
                                item.on_click(&pointer_button.button);
                                item.click_input(&pointer_button.button);
                            }
                            return item.props.on_click.is_some();
                        }
                    }
                    false
                }
            }),
            dispatch_pointer_moved: event!({
                let mut is_hovered = false;
                move |item: &mut ItemData, pointer_moved: &PointerMoved| {
                    if !item.props().enable.get() {
                        return;
                    }
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

                    if item.current_frame().contains(pointer_moved.position.x, pointer_moved.position.y)
                    {
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

                    item.children.lock().iter_mut().for_each(|child| {
                        child.data().dispatch_pointer_moved(pointer_moved);
                    });
                }
            }),
            draw: event!({ |_item: &mut ItemData, _canvas: &Canvas| {} }),
            hover_changed: event!({ |_item: &mut ItemData, _is_hovered: bool| {} }),
            focus_changed: event!({ |_item: &mut ItemData, _focus_state: &FocusState| {} }),
            ime_input: event!({ |_item: &mut ItemData, _action: &ImeAction| {} }),
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
                    let width = item.props.clamp_width(get_size(width_mode));
                    let height = item.props.clamp_height(get_size(height_mode));
                    let measure_frame = &mut item.measure_frame;
                    measure_frame.width = width;
                    measure_frame.height = height;
                }
            }),
            modifiers_change: event!({ |_item: &mut ItemData, _modifiers: &Modifiers| {} }),
            // mouse_input: event!({ |_item: &mut ItemData, _input: &MouseInput| { false } }),
            mouse_wheel: event!({
                |_item: &mut ItemData,
                 mouse_wheel_x: Option<MouseWheel>,
                 mouse_wheel_y: Option<MouseWheel>| {
                    (mouse_wheel_x, mouse_wheel_y)
                }
            }),
            pointer_button: event!({ |_item: &mut ItemData, _input: &PointerButton| { false } }),
            pointer_moved: event!({
                |_item: &mut ItemData, _input: &PointerMoved| {}
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
                    let event = self.event.$invoke.clone();
                    let mut f = event.borrow_mut();
                    f(self, $($arg_name),*)
                }
            }
        )*
    }
}

impl_noop!(
    set_cursor_move|impl FnMut(&mut ItemData, &CursorMove) + 'static|cursor_move|()|cursor_move:&CursorMove,
    set_click_input|impl FnMut(&mut ItemData, &ButtonSource) + 'static|click_input|()|source:&ButtonSource,
    set_dispatch_draw|impl FnMut(&mut ItemData, &mut Surface, f32, f32) + 'static|dispatch_draw|()|surface:&mut Surface;parent_x:f32;parent_y:f32,
    set_dispatch_focus|impl FnMut(&mut ItemData, u32, bool) -> bool + 'static|dispatch_focus|bool|item_id:u32;has_parent_focus:bool,
    set_dispatch_ime_input|impl FnMut(&mut ItemData, &ImeAction) -> bool + 'static|dispatch_ime_input|bool|action:&ImeAction,
    set_dispatch_keyboard_input|impl FnMut(&mut ItemData, &KeyboardInput) -> bool + 'static|dispatch_keyboard_input|bool|input:&KeyboardInput,
    set_dispatch_layout|impl FnMut(&mut ItemData, f32, f32, f32, f32) + 'static|dispatch_layout|()|x:f32;y:f32;width:f32;height:f32,
    set_dispatch_measure|impl FnMut(&mut ItemData, MeasureMode, MeasureMode) + 'static|dispatch_measure|()|width:MeasureMode;height:MeasureMode,
    set_dispatch_modifiers_change|impl FnMut(&mut ItemData, &Modifiers) + 'static|dispatch_modifiers_change|()|modifiers:&Modifiers,
    // set_dispatch_mouse_input|impl FnMut(&mut ItemData, &MouseInput) -> bool + 'static|dispatch_mouse_input|bool|input:&MouseInput,
    set_dispatch_mouse_wheel|impl FnMut(&mut ItemData, Option<MouseWheel>, Option<MouseWheel>) -> (Option<MouseWheel>, Option<MouseWheel>) + 'static|dispatch_mouse_wheel|(Option<MouseWheel>, Option<MouseWheel>)|mouse_wheel_x:Option<MouseWheel>;mouse_wheel_y:Option<MouseWheel>,
    // set_dispatch_touch_input|impl FnMut(&mut ItemData, &TouchInput) -> bool + 'static|dispatch_touch_input|bool|input:&TouchInput,
    set_dispatch_pointer_button|impl FnMut(&mut ItemData, &PointerButton) -> bool + 'static|dispatch_pointer_button|bool|input:&PointerButton,
    set_dispatch_pointer_moved|impl FnMut(&mut ItemData, &PointerMoved) + 'static|dispatch_pointer_moved|()|input:&PointerMoved,
    set_draw|impl FnMut(&mut ItemData, &Canvas) + 'static|draw|()|canvas:&Canvas,
    set_focus_changed|impl FnMut(&mut ItemData, &FocusState) + 'static|focus_changed|()|focus_state:&FocusState,
    set_hover_changed|impl FnMut(&mut ItemData, bool) + 'static|hover_changed|()|is_hovered:bool,
    set_ime_input|impl FnMut(&mut ItemData, &ImeAction) + 'static|ime_input|()|action:&ImeAction,
    set_keyboard_input|impl FnMut(&mut ItemData, &KeyboardInput) + 'static|keyboard_input|()|input:&KeyboardInput,
    set_layout|impl FnMut(&mut ItemData, f32, f32) + 'static|layout|()|width:f32;height:f32,
    set_measure|impl FnMut(&mut ItemData, MeasureMode, MeasureMode) + 'static|measure|()|width:MeasureMode;height:MeasureMode,
    set_modifiers_change|impl FnMut(&mut ItemData, &Modifiers) + 'static|modifiers_change|()|modifiers:&Modifiers,
    // set_mouse_input|impl FnMut(&mut ItemData, &MouseInput) -> bool + 'static|mouse_input|bool|input:&MouseInput,
    set_mouse_wheel|impl FnMut(&mut ItemData, Option<MouseWheel>, Option<MouseWheel>) -> (Option<MouseWheel>, Option<MouseWheel>) + 'static|mouse_wheel|(Option<MouseWheel>, Option<MouseWheel>)|mouse_wheel_x:Option<MouseWheel>;mouse_wheel_y:Option<MouseWheel>,
    set_pointer_button|impl FnMut(&mut ItemData, &PointerButton) -> bool + 'static|pointer_button|bool|input:&PointerButton,
    set_pointer_moved|impl FnMut(&mut ItemData, &PointerMoved) + 'static|pointer_moved|()|input:&PointerMoved
);