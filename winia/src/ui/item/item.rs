use crate::core::{bind_str_to_id, generate_id};
use crate::shared::{
    Children, Gettable, Observable, Settable, Shared, SharedAlignment, SharedBool, SharedColor,
    SharedF32, SharedInnerPosition, SharedItem, SharedSize, SharedUsize,
};
use crate::ui::app::{AppContext, UserEvent};
use crate::ui::item::{DisplayParameter, InnerPosition, Size};
use crate::ui::theme::Style;
use crate::ui::Animation;
use parking_lot::{Mutex, MutexGuard};
use skia_safe::image_filters::CropRect;
use skia_safe::{
    image_filters, Canvas, Color, IRect, Paint, Path, Point, Rect, Surface, TileMode, Vector,
};
use std::any::Any;
use std::collections::{HashMap, HashSet, LinkedList};
use std::ops::{DerefMut, Not};
use std::sync::Arc;
use std::time::Instant;
use winit::event::{DeviceId, Force, KeyEvent, MouseButton, TouchPhase};


pub fn layout<T>(mut property: Shared<T>, id: usize, app_context: &AppContext) -> Shared<T> {
    let app_context = app_context.clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                app_context.request_layout();
            }),
        )
        .drop();
    property
}

pub fn init_property_layout<T>(property: &mut Shared<T>, id: usize, app_context: &AppContext) {
    let app_context = app_context.clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                app_context.request_layout();
            }),
        )
        .drop();
}

pub fn redraw<T>(mut property: Shared<T>, id: usize, app_context: &AppContext) -> Shared<T> {
    let app_context = app_context.clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                app_context.request_redraw();
            }),
        )
        .drop();
    property
}

pub fn init_property_redraw<T>(property: &mut Shared<T>, id: usize, app_context: &AppContext) {
    let app_context = app_context.clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                app_context.request_redraw();
            }),
        )
        .drop();
}

macro_rules! impl_property_layout {
    ($property_name:ident, $set_property_name:ident, $get_property_name:ident, $property_type:ty, $doc:expr) => {
        impl ItemData {
            #[doc=$doc]
            pub fn $set_property_name(&mut self, $property_name: impl Into<$property_type>) {
                self.$property_name.remove_observer(self.id);
                self.$property_name = $property_name.into();
                init_property_layout(&mut self.$property_name, self.id, &self.app_context);
            }

            pub fn $get_property_name(&self) -> $property_type {
                self.$property_name.clone()
            }
        }

        impl Item {
            #[doc=$doc]
            pub fn $property_name(self, $property_name: impl Into<$property_type>) -> Self {
                let mut item = self.data.lock();
                item.$set_property_name($property_name);
                drop(item);
                self
            }
        }
    };
}

macro_rules! impl_property_redraw {
    ($property_name:ident, $set_property_name:ident, $get_property_name:ident, $property_type:ty, $doc:expr) => {
        impl ItemData {
            #[doc=$doc]
            pub fn $set_property_name(&mut self, $property_name: impl Into<$property_type>) {
                self.$property_name.remove_observer(self.id);
                self.$property_name = $property_name.into();
                init_property_redraw(&mut self.$property_name, self.id, &self.app_context);
            }

            pub fn $get_property_name(&self) -> $property_type {
                self.$property_name.clone()
            }
        }

        impl Item {
            #[doc=$doc]
            pub fn $property_name(self, $property_name: impl Into<$property_type>) -> Self {
                let mut item = self.data.lock();
                item.$set_property_name($property_name);
                drop(item);
                self
            }
        }
    };
}

macro_rules! impl_get_set {
    ($name:ident, $set_name:ident, $set_type:ty, $set_doc:expr, $get_name:ident, $get_type:ty, $get_doc:expr) => {
        impl ItemData {
            #[doc = $get_doc]
            pub fn $get_name(&self) -> Arc<Mutex<$get_type>> {
                self.$name.clone()
            }

            #[doc = $set_doc]
            pub fn $set_name(&mut self, $name: $set_type) -> &mut Self {
                self.$name = Arc::new(Mutex::new($name));
                self
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl Alignment {
    pub fn to_horizontal_alignment(&self) -> HorizontalAlignment {
        match self {
            Alignment::TopStart | Alignment::CenterStart | Alignment::BottomStart => {
                HorizontalAlignment::Start
            }
            Alignment::TopCenter | Alignment::Center | Alignment::BottomCenter => {
                HorizontalAlignment::Center
            }
            Alignment::TopEnd | Alignment::CenterEnd | Alignment::BottomEnd => {
                HorizontalAlignment::End
            }
        }
    }

    pub fn to_vertical_alignment(&self) -> VerticalAlignment {
        match self {
            Alignment::TopStart | Alignment::TopCenter | Alignment::TopEnd => {
                VerticalAlignment::Top
            }
            Alignment::CenterStart | Alignment::Center | Alignment::CenterEnd => {
                VerticalAlignment::Center
            }
            Alignment::BottomStart | Alignment::BottomCenter | Alignment::BottomEnd => {
                VerticalAlignment::Bottom
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HorizontalAlignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutDirection {
    LTR,
    RTL,
}

macro_rules! calculate_animation_value {
    ($name:ident, $s:ident, $display_parameter:ident) => {
        let p = {
            if let Some((start, animation)) = &$s.animations.$name {
                Some((start, animation.clone()))
            } else {
                None
            }
        };
        if let Some((start, animation)) = p {
            if !animation.is_finished() {
                $display_parameter.$name =
                    animation.interpolate_f32(*start, $display_parameter.$name);
            } else {
                $s.animations.$name = None;
            }
        }
    };
}



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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerState {
    Started,
    Moved,
    Ended,
    Cancelled,
}

impl From<TouchPhase> for PointerState {
    fn from(value: TouchPhase) -> Self {
        match value {
            TouchPhase::Started => PointerState::Started,
            TouchPhase::Moved => PointerState::Moved,
            TouchPhase::Ended => PointerState::Ended,
            TouchPhase::Cancelled => PointerState::Cancelled,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseScrollDelta {
    /// Amount in lines or rows to scroll in the horizontal
    /// and vertical directions.
    ///
    /// Positive values indicate that the content that is being scrolled should move
    /// right and down (revealing more content left and up).
    LineDelta(f32, f32),

    /// Amount in pixels to scroll in the horizontal and
    /// vertical direction.
    ///
    /// Scroll events are expressed as a `PixelDelta` if
    /// supported by the device (eg. a touchpad) and
    /// platform.
    ///
    /// Positive values indicate that the content being scrolled should
    /// move right/down.
    ///
    /// For a 'natural scrolling' touch pad (that acts like a touch screen)
    /// this means moving your fingers right and down should give positive values,
    /// and move the content right and down (to reveal more things left and up).
    LogicalDelta(f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseWheel {
    pub device_id: DeviceId,
    pub delta: MouseScrollDelta,
    pub state: PointerState
}

impl MeasureMode {
    pub fn value(self) -> f32 {
        match self {
            MeasureMode::Specified(value) => value,
            MeasureMode::Unspecified(value) => value,
        }
    }
}

pub enum CustomProperty {
    Usize(SharedUsize),
    Float(SharedF32),
    Color(SharedColor),
    Bool(SharedBool),
    Any(Box<dyn Any>),
}

#[derive(Default)]
struct Animations {
    parent_x: Option<(f32, Animation)>,
    parent_y: Option<(f32, Animation)>,
    width: Option<(f32, Animation)>,
    height: Option<(f32, Animation)>,
    relative_x: Option<(f32, Animation)>,
    relative_y: Option<(f32, Animation)>,
    offset_x: Option<(f32, Animation)>,
    offset_y: Option<(f32, Animation)>,
    opacity: Option<(f32, Animation)>,
    rotation: Option<(f32, Animation)>,
    rotation_center_x: Option<(f32, Animation)>,
    rotation_center_y: Option<(f32, Animation)>,
    scale_x: Option<(f32, Animation)>,
    scale_y: Option<(f32, Animation)>,
    scale_center_x: Option<(f32, Animation)>,
    scale_center_y: Option<(f32, Animation)>,
    skew_x: Option<(f32, Animation)>,
    skew_y: Option<(f32, Animation)>,
    skew_center_x: Option<(f32, Animation)>,
    skew_center_y: Option<(f32, Animation)>,
    float_params: HashMap<String, (f32, Animation)>,
    color_params: HashMap<String, (Color, Animation)>,
}

impl Animations {
    fn is_animating(&self) -> bool {
        self.parent_x.is_some()
            || self.parent_y.is_some()
            || self.width.is_some()
            || self.height.is_some()
            || self.relative_x.is_some()
            || self.relative_y.is_some()
            || self.offset_x.is_some()
            || self.offset_y.is_some()
            || self.opacity.is_some()
            || self.rotation.is_some()
            || self.rotation_center_x.is_some()
            || self.rotation_center_y.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
            || self.scale_center_x.is_some()
            || self.scale_center_y.is_some()
            || self.skew_x.is_some()
            || self.skew_y.is_some()
            || self.skew_center_x.is_some()
            || self.skew_center_y.is_some()
            || !self.float_params.is_empty()
            || !self.color_params.is_empty()
    }
}

/// An item is a basic building block of the UI system. It can be used to display text, images, or other content.
/// It can also be used to arrange other items in a layout.
pub struct ItemData {
    active: SharedBool,
    align_content: SharedAlignment,
    animations: Animations,
    app_context: AppContext,
    background: SharedItem,
    baseline: Option<f32>,
    children: Children,
    clip: Shared<bool>,
    clip_shape: Shared<Box<dyn Fn(DisplayParameter) -> Path>>,
    custom_properties: HashMap<String, CustomProperty>,
    display_parameter_out: Shared<DisplayParameter>,
    enable_background_blur: SharedBool,
    focused: Shared<bool>,
    foreground: SharedItem,
    height: SharedSize,
    id: usize,
    layout_direction: Shared<LayoutDirection>,
    margin_bottom: SharedF32,
    margin_end: SharedF32,
    margin_start: SharedF32,
    margin_top: SharedF32,
    max_height: SharedF32,
    max_width: SharedF32,
    measure_parameter: DisplayParameter,
    min_height: SharedF32,
    min_width: SharedF32,
    name: String,
    offset_x: SharedF32,
    offset_y: SharedF32,
    on_attach: LinkedList<Box<dyn FnMut()>>,
    on_click: Option<Box<dyn FnMut(ClickSource)>>,
    on_cursor_move: Option<Box<dyn FnMut(f32, f32)>>,
    on_detach: LinkedList<Box<dyn FnMut()>>,
    on_focus: Vec<Box<dyn FnMut(bool)>>,
    on_hover: Option<Box<dyn FnMut(bool)>>,
    on_mouse_input: Option<Box<dyn FnMut(MouseEvent)>>,
    on_pointer_input: Option<Box<dyn FnMut(PointerEvent)>>,
    on_touch_input: Option<Box<dyn FnMut(TouchEvent)>>,
    opacity: SharedF32,
    padding_bottom: SharedF32,
    padding_end: SharedF32,
    padding_start: SharedF32,
    padding_top: SharedF32,
    recorded_parameter: Option<DisplayParameter>,
    rotation: SharedF32,
    rotation_center_x: SharedInnerPosition,
    rotation_center_y: SharedInnerPosition,
    scale_center_x: SharedInnerPosition,
    scale_center_y: SharedInnerPosition,
    scale_x: SharedF32,
    scale_y: SharedF32,
    skew_center_x: SharedInnerPosition,
    skew_center_y: SharedInnerPosition,
    skew_x: SharedF32,
    skew_y: SharedF32,
    target_parameter: DisplayParameter,
    touch_start_time: Instant,
    width: SharedSize,

    apply_style: Arc<Mutex<dyn FnMut(&mut ItemData, &Style)>>,
    cursor_move: Arc<Mutex<dyn FnMut(&mut ItemData, f32, f32)>>,
    dispatch_apply_style: Arc<Mutex<dyn FnMut(&mut ItemData, &Style)>>,
    dispatch_cursor_move: Arc<Mutex<dyn FnMut(&mut ItemData, f32, f32)>>,
    dispatch_draw: Arc<Mutex<dyn FnMut(&mut ItemData, &mut Surface, f32, f32)>>,
    dispatch_focus: Arc<Mutex<dyn FnMut(&mut ItemData)>>,
    dispatch_keyboard_input: Arc<Mutex<dyn FnMut(&mut ItemData, DeviceId, KeyEvent, bool) -> bool>>,
    dispatch_layout: Arc<Mutex<dyn FnMut(&mut ItemData, f32, f32, f32, f32)>>,
    dispatch_mouse_input: Arc<Mutex<dyn FnMut(&mut ItemData, MouseEvent)>>,
    dispatch_mouse_wheel: Arc<Mutex<dyn FnMut(&mut ItemData, MouseWheel) -> bool>>,
    dispatch_timer: Arc<Mutex<dyn FnMut(&mut ItemData, usize) -> bool>>,
    dispatch_touch_input: Arc<Mutex<dyn FnMut(&mut ItemData, TouchEvent)>>,
    draw: Arc<Mutex<dyn FnMut(&mut ItemData, &Canvas)>>,
    ime_input: Arc<Mutex<dyn FnMut(&mut ItemData, ImeAction)>>,
    keyboard_input: Arc<Mutex<dyn FnMut(&mut ItemData, DeviceId, KeyEvent, bool) -> bool>>,
    layout: Arc<Mutex<dyn FnMut(&mut ItemData, f32, f32)>>,
    measure: Arc<Mutex<dyn FnMut(&mut ItemData, MeasureMode, MeasureMode)>>,
    mouse_input: Arc<Mutex<dyn FnMut(&mut ItemData, MouseEvent)>>,
    mouse_wheel: Arc<Mutex<dyn FnMut(&mut ItemData, MouseWheel) -> bool>>,
    click_event: Arc<Mutex<dyn FnMut(&mut ItemData, ClickSource)>>,
    focus_event: Arc<Mutex<dyn FnMut(&mut ItemData, bool)>>,
    hover_event: Arc<Mutex<dyn FnMut(&mut ItemData, bool)>>,
    pointer_input: Arc<Mutex<dyn FnMut(&mut ItemData, PointerEvent)>>,
    timer: Arc<Mutex<dyn FnMut(&mut ItemData, usize) -> bool>>,
    touch_input: Arc<Mutex<dyn FnMut(&mut ItemData, TouchEvent)>>,
}

impl ItemData {
    pub fn new(app_context: AppContext, children: Children) -> Self {
        let id = generate_id();

        let item = Self {
            active: layout(true.into(), id, &app_context),
            align_content: layout(Alignment::TopStart.into(), id, &app_context),
            animations: Default::default(),
            app_context: app_context.clone(),
            background: layout(SharedItem::none(), id, &app_context),
            baseline: None,
            children,
            clip: redraw(false.into(), id, &app_context),
            clip_shape: redraw(
                Shared::from_static(Box::new(|display_parameter| {
                    let rect = Rect::from_xywh(
                        display_parameter.x(),
                        display_parameter.y(),
                        display_parameter.width,
                        display_parameter.height,
                    );
                    Path::rect(rect, None)
                })),
                id,
                &app_context,
            ),
            custom_properties: HashMap::new(),
            display_parameter_out: DisplayParameter::default().into(),
            enable_background_blur: redraw(false.into(), id, &app_context),
            focused: redraw(false.into(), id, &app_context),
            foreground: redraw(SharedItem::none(), id, &app_context),
            height: redraw(Size::Compact.into(), id, &app_context),
            id,
            layout_direction: layout(LayoutDirection::LTR.into(), id, &app_context),
            margin_bottom: layout(0.0.into(), id, &app_context),
            margin_end: layout(0.0.into(), id, &app_context),
            margin_start: layout(0.0.into(), id, &app_context),
            margin_top: layout(0.0.into(), id, &app_context),
            max_height: layout(f32::INFINITY.into(), id, &app_context),
            max_width: layout(f32::INFINITY.into(), id, &app_context),
            measure_parameter: Default::default(),
            min_height: layout(0.0.into(), id, &app_context),
            min_width: layout(0.0.into(), id, &app_context),
            name: format!("Item {}", id),
            offset_x: layout(0.0.into(), id, &app_context),
            offset_y: layout(0.0.into(), id, &app_context),
            on_attach: LinkedList::new(),
            on_click: None,
            on_cursor_move: None,
            on_detach: LinkedList::new(),
            on_focus: Vec::new(),
            on_hover: None,
            on_mouse_input: None,
            on_pointer_input: None,
            on_touch_input: None,
            opacity: redraw(1.0.into(), id, &app_context),
            padding_bottom: layout(0.0.into(), id, &app_context),
            padding_end: layout(0.0.into(), id, &app_context),
            padding_start: layout(0.0.into(), id, &app_context),
            padding_top: layout(0.0.into(), id, &app_context),
            recorded_parameter: None,
            rotation: redraw(0.0.into(), id, &app_context),
            rotation_center_x: redraw(InnerPosition::default().into(), id, &app_context),
            rotation_center_y: redraw(InnerPosition::default().into(), id, &app_context),
            scale_center_x: redraw(InnerPosition::default().into(), id, &app_context),
            scale_center_y: redraw(InnerPosition::default().into(), id, &app_context),
            scale_x: redraw(1.0.into(), id, &app_context),
            scale_y: redraw(1.0.into(), id, &app_context),
            skew_center_x: redraw(InnerPosition::default().into(), id, &app_context),
            skew_center_y: redraw(InnerPosition::default().into(), id, &app_context),
            skew_x: redraw(0.0.into(), id, &app_context),
            skew_y: redraw(0.0.into(), id, &app_context),
            target_parameter: Default::default(),
            touch_start_time: Instant::now(),
            width: layout(Size::Compact.into(), id, &app_context),

            apply_style: Arc::new(Mutex::new(|_item: &mut ItemData, _style: &Style| {})),
            cursor_move: Arc::new(Mutex::new(|_item: &mut ItemData, _x: f32, _y: f32| {})),
            dispatch_apply_style: Arc::new(Mutex::new(|item: &mut ItemData, style: &Style| {
                item.get_children().items().iter_mut().for_each(|child| {
                    child.data()
                        .get_dispatch_apply_style()
                        .lock()(child.data().deref_mut(), style);
                });
                item.get_apply_style().lock()(item, style);
            })),
            dispatch_cursor_move: Arc::new(Mutex::new({
                let mut is_hovered = false;
                move |item: &mut ItemData, x: f32, y: f32| {
                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.value().as_mut() {
                        foreground.data().dispatch_cursor_move(x, y);
                    }
                    let background = item.get_background();
                    if let Some(background) = background.value().as_mut() {
                        background.data().dispatch_cursor_move(x, y);
                    }

                    if let Some(on_cursor_move) = item.get_on_cursor_move() {
                        on_cursor_move(x, y);
                    }

                    item.get_cursor_move().lock()(item, x, y);

                    if item.get_display_parameter().is_inside(x, y) {
                        if !is_hovered {
                            is_hovered = true;
                            item.get_hover_event().lock()(item, true);
                            if let Some(on_hover) = item.get_on_hover() {
                                on_hover(true);
                            }
                        }
                    } else {
                        if is_hovered {
                            is_hovered = false;
                            item.get_hover_event().lock()(item, false);
                            if let Some(on_hover) = item.get_on_hover() {
                                on_hover(false);
                            }
                        }
                    }

                    let window_size = {
                        let window_size = item.get_app_context().window_size().unwrap();
                        (window_size.width, window_size.height)
                    };
                    // item.get_children().items().iter_visible_item(window_size).for_each(|child| {
                    //     child.data().dispatch_cursor_move(x, y);
                    // });
                    rayon::iter::ParallelIterator::for_each(
                        rayon::iter::ParallelIterator::filter(
                            item.get_children().items().par_iter_mut(),
                            |item|{
                                let display_parameter = item.data().get_display_parameter();
                                let x = display_parameter.x();
                                let y = display_parameter.y();
                                let width = display_parameter.width;
                                let height = display_parameter.height;
                                let x_overlap = x < window_size.0 && x + width > 0.0;
                                let y_overlap = y < window_size.1 && y + height > 0.0;
                                x_overlap && y_overlap
                            }
                        ), 
                        |child| {
                        child.data().dispatch_cursor_move(x, y);
                    });
                }
            })),
            dispatch_draw: Arc::new(Mutex::new(
                |item: &mut ItemData, surface: &mut Surface, parent_x: f32, parent_y: f32| {
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
                        canvas.save_layer_alpha_f(None, display_parameter.opacity);
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

                    fn draw_item(item: &mut Item, surface: &mut Surface, x: f32, y: f32) {
                        let clip = item.data().get_clip().get();
                        if clip {
                            let display_parameter = item.data().get_display_parameter();
                            let clip_path = {
                                let clip_shape = item.data().get_clip_shape();
                                let path = clip_shape.value().as_ref()(display_parameter);
                                path
                            };
                            let canvas = surface.canvas();
                            canvas.save();
                            canvas.clip_path(&clip_path, None, true);
                        }
                        item.data().dispatch_draw(surface, x, y);
                        if clip {
                            let canvas = surface.canvas();
                            canvas.restore();
                        }
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
                    let window_size = {
                        let window_size = item.get_app_context().window_size().unwrap();
                        (window_size.width, window_size.height)
                    };
                    item.get_children().items().iter_visible_item(window_size).for_each(|child| {
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
            dispatch_focus: Arc::new(Mutex::new(|item: &mut ItemData| {
                let focus_changed_items = item.get_app_context().focus_changed_items;
                let focused = item.get_focused().get();
                {
                    let focus_changed_items = focus_changed_items.value();
                    if focus_changed_items.contains(&item.get_id()) {
                        item.focus(focused)
                    }
                }
                item.get_children().items().iter_mut().for_each(|child| {
                    child.data().dispatch_focus();
                });
            })),
            dispatch_keyboard_input: Arc::new(Mutex::new(
                |item: &mut ItemData, device_id: DeviceId, event: KeyEvent, is_synthetic: bool| {
                    let keyboard_input = item.get_keyboard_input();
                    if keyboard_input.lock()(item, device_id, event.clone(), is_synthetic)
                    {
                        return true;
                    }
                    item.get_children().items().iter_mut().any(|child| {
                        let dispatch_keyboard_input =
                            child.data().get_dispatch_keyboard_input();
                        let r = dispatch_keyboard_input.lock()(
                            child.data().deref_mut(),
                            device_id,
                            event.clone(),
                            is_synthetic,
                        );
                        r
                    })
                },
            )),
            dispatch_layout: Arc::new(Mutex::new(
                |item: &mut ItemData, relative_x: f32, relative_y: f32, width: f32, height: f32| {
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
                            let target_parameter = item.get_target_parameter();
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
                move |item: &mut ItemData, event: MouseEvent| {
                    let x = event.x;
                    let y = event.y;

                    // If the item captures the mouse button,
                    // the foreground and background of the item can receive all mouse input events.
                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.value().as_mut() {
                        foreground.data().dispatch_mouse_input(event);
                    }

                    let background = item.get_background();
                    if let Some(background) = background.value().as_mut() {
                        background.data().dispatch_mouse_input(event);
                    }

                    // Call the on_mouse_input event of the item.
                    if let Some(on_mouse_input) = item.get_on_mouse_input() {
                        on_mouse_input(event);
                    }

                    {
                        // Call the mouse_input event of the item_event.
                        // Why there are two mouse_input events?
                        // Because winia don't want to expose item object to the user.
                        item.get_mouse_input().lock()(item, event);
                    }

                    {
                        // Dispatch the mouse input events to the child items.
                        let children = item.get_children();
                        for child in children.items().iter_mut().rev() {
                            let display_parameter = child.data().get_display_parameter();
                            match event.pointer_state {
                                PointerState::Started => {
                                    // If the mouse pointer is inside the child item,
                                    if display_parameter.is_inside(x, y) {
                                        // The child item captures the mouse button.
                                        captured_mouse_button
                                            .insert((child.data().get_id(), event.button));
                                        child.data().dispatch_mouse_input(event);
                                        // Other child items can't receive the mouse input events.
                                        return;
                                    }
                                }
                                PointerState::Moved => {
                                    // If the child item captures the mouse button
                                    if captured_mouse_button
                                        .contains(&(child.data().get_id(), event.button))
                                    {
                                        child.data().dispatch_mouse_input(event);
                                        return;
                                    }
                                }
                                PointerState::Ended | PointerState::Cancelled => {
                                    // If the child item captures the mouse button
                                    if captured_mouse_button
                                        .contains(&(child.data().get_id(), event.button))
                                    {
                                        // The child item releases the mouse button.
                                        captured_mouse_button
                                            .remove(&(child.data().get_id(), event.button));
                                        child.data().dispatch_mouse_input(event);
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
                                    item.get_click_event().lock()(
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

                    item.get_pointer_input().lock()(item, event.into());
                    if let Some(on_pointer_input) = item.get_on_pointer_input() {
                        on_pointer_input(event.into())
                    }
                }
            })),
            dispatch_mouse_wheel: Arc::new(Mutex::new(|item: &mut ItemData, mouse_wheel: MouseWheel|{
                let children = item.get_children();
                let (cursor_x, cursor_y) = item.get_app_context().get_cursor_position();
                for child in children.items().iter_mut().rev() {
                    let display_parameter = child.data().get_display_parameter();
                    if !display_parameter.is_inside(cursor_x, cursor_y) {
                        continue;
                    }
                    let dispatch_mouse_wheel = child.data().get_dispatch_mouse_wheel();
                    let r = dispatch_mouse_wheel.lock()(child.data().deref_mut(), mouse_wheel.clone());
                    if r {
                        return true;
                    }
                }
                item.get_mouse_wheel().lock()(item, mouse_wheel)
            })),
            dispatch_timer: Arc::new(Mutex::new(|item: &mut ItemData, id: usize| {
                let timer = item.get_timer();
                if timer.lock()(item, id) {
                    return true;
                }
                item.get_children().items().iter_mut().any(|child| {
                    let dispatch_timer = child.data().get_dispatch_timer();
                    let r = dispatch_timer.lock()(child.data().deref_mut(), id);
                    r
                })
            })),
            dispatch_touch_input: Arc::new(Mutex::new({
                // item_id, touch_id
                let mut captured_touch_pointer: HashSet<(usize, u64)> = HashSet::new();
                let mut touch_start_time = Instant::now();
                move |item: &mut ItemData, event: TouchEvent| {
                    let x = event.x;
                    let y = event.y;

                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.value().as_mut() {
                        foreground.data().dispatch_touch_input(event);
                    }
                    let background = item.get_background();
                    if let Some(background) = background.value().as_mut() {
                        background.data().dispatch_touch_input(event);
                    }

                    if let Some(on_touch) = &mut item.get_on_touch_input() {
                        on_touch(event);
                    }

                    {
                        let children = item.get_children();
                        for child in children.items().iter_mut().rev() {
                            let display_parameter = child.data().get_display_parameter();
                            match event.pointer_state {
                                PointerState::Started => {
                                    if display_parameter.is_inside(x, y) {
                                        captured_touch_pointer.insert((child.data().get_id(), event.id));
                                        child.data().dispatch_touch_input(event);
                                        return;
                                    }
                                }
                                PointerState::Moved => {
                                    if captured_touch_pointer.contains(&(child.data().get_id(), event.id))
                                    {
                                        child.data().dispatch_touch_input(event);
                                        return;
                                    }
                                }
                                PointerState::Ended | PointerState::Cancelled => {
                                    if captured_touch_pointer.contains(&(child.data().get_id(), event.id))
                                    {
                                        let child_id = child.data().get_id();
                                        captured_touch_pointer
                                            .retain(|&(item_id, _touch_id)| item_id != child_id);
                                        child.data().dispatch_touch_input(event);
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
                            item.get_click_event().lock()(item, click_source);
                            if let Some(on_click) = item.get_on_click() {
                                on_click(click_source);
                            }
                        }
                        _ => {}
                    }

                    item.get_pointer_input().lock()(item, event.into());
                    if let Some(on_pointer_input) = item.get_on_pointer_input() {
                        on_pointer_input(event.into());
                    }
                }
            })),
            draw: Arc::new(Mutex::new(|_item: &mut ItemData, _canvas: &Canvas| {})),
            ime_input: Arc::new(Mutex::new(|_item: &mut ItemData, _action: ImeAction| {})),
            keyboard_input: Arc::new(Mutex::new(
                |_item: &mut ItemData, _device_id: DeviceId, _event: KeyEvent, _is_synthetic: bool| {
                    false
                },
            )),
            layout: Arc::new(Mutex::new(|_item: &mut ItemData, _width: f32, _height: f32| {})),
            measure: Arc::new(Mutex::new(|item: &mut ItemData, width_mode, height_mode| {
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
            mouse_input: Arc::new(Mutex::new(|_item: &mut ItemData, _event: MouseEvent| {})),
            mouse_wheel: Arc::new(Mutex::new(|_item: &mut ItemData, _mouse_wheel: MouseWheel| false)),
            click_event: Arc::new(Mutex::new(|_item: &mut ItemData, _source: ClickSource| {})),
            focus_event: Arc::new(Mutex::new(|_item: &mut ItemData, _focused: bool| {})),
            hover_event: Arc::new(Mutex::new(|_item: &mut ItemData, _hover: bool| {})),
            pointer_input: Arc::new(Mutex::new(|_item: &mut ItemData, _event: PointerEvent| {})),
            timer: Arc::new(Mutex::new(|_item: &mut ItemData, _id: usize| false)),
            touch_input: Arc::new(Mutex::new(|_item: &mut ItemData, _event: TouchEvent| {})),
        };
        item.focused(false)
    }
}

impl_property_layout!(
    active,
    set_active,
    get_active,
    SharedBool,
    "Whether the item is active and can receive input events."
);
impl_property_layout!(
    align_content,
    set_align_content,
    get_align_content,
    SharedAlignment,
    "The alignment of the content of the item. Not all items support this property."
);
impl_property_layout!(
    background,
    set_background,
    get_background,
    SharedItem,
    "The background of the item. It will be drawn behind the content (including children)"
);
impl_property_redraw!(
    clip,
    set_clip,
    get_clip,
    SharedBool,
    "Whether to clip the content of the item to its bounds. If this is set to true, the content will not be drawn outside the bounds of the item."
);
impl_property_redraw!(
    clip_shape,
    set_clip_shape,
    get_clip_shape,
    Shared<Box<dyn Fn(DisplayParameter) -> Path>>,
    "The shape used to clip the content of the item. If this is set, the content will be clipped to the shape."
);
impl_property_layout!(
    enable_background_blur,
    set_enable_background_blur,
    get_enable_background_blur,
    SharedBool,
    "Whether to enable background blur. This will cause the background to be blurred when it is not fully opaque."
);
impl_property_layout!(
    foreground,
    set_foreground,
    get_foreground,
    SharedItem,
    "The foreground of the item. It will be drawn in front of the content (including children)"
);
impl_property_layout!(
    height,
    set_height,
    get_height,
    SharedSize,
    "The height of the item. See [`Size`](crate::ui::item::Size) for more information."
);
impl_property_layout!(
    layout_direction,
    set_layout_direction,
    get_layout_direction,
    Shared<LayoutDirection>,
    "The layout direction of the item."
);
impl_property_layout!(
    margin_bottom,
    set_margin_bottom,
    get_margin_bottom,
    SharedF32,
    "The margin at the bottom of the item."
);
impl_property_layout!(
    margin_end,
    set_margin_end,
    get_margin_end,
    SharedF32,
    "The margin at the end of the item. The \"end\" direction depends on the layout direction."
);
impl_property_layout!(
    margin_start,
    set_margin_start,
    get_margin_start,
    SharedF32,
    "The margin at the start of the item. The \"start\" direction depends on the layout direction."
);
impl_property_layout!(
    margin_top,
    set_margin_top,
    get_margin_top,
    SharedF32,
    "The margin at the top of the item."
);
impl_property_layout!(
    max_height,
    set_max_height,
    get_max_height,
    SharedF32,
    "The maximum height of the item."
);
impl_property_layout!(
    max_width,
    set_max_width,
    get_max_width,
    SharedF32,
    "The maximum width of the item."
);
impl_property_layout!(
    min_height,
    set_min_height,
    get_min_height,
    SharedF32,
    "The minimum height of the item."
);
impl_property_layout!(
    min_width,
    set_min_width,
    get_min_width,
    SharedF32,
    "The minimum width of the item."
);
impl_property_layout!(
    offset_x,
    set_offset_x,
    get_offset_x,
    SharedF32,
    "The offset in the x direction relative to the original position."
);
impl_property_layout!(
    offset_y,
    set_offset_y,
    get_offset_y,
    SharedF32,
    "The offset in the y direction relative to the original position."
);
impl_property_layout!(
    opacity,
    set_opacity,
    get_opacity,
    SharedF32,
    "The opacity of the item. It will also affect the opacity of its children."
);
impl_property_layout!(
    padding_bottom,
    set_padding_bottom,
    get_padding_bottom,
    SharedF32,
    "The padding at the bottom of the item."
);
impl_property_layout!(
    padding_end,
    set_padding_end,
    get_padding_end,
    SharedF32,
    "The padding at the end of the item. The \"end\" direction depends on the layout direction."
);
impl_property_layout!(
    padding_start,
    set_padding_start,
    get_padding_start,
    SharedF32,
    "The padding at the start of the item. The \"start\" direction depends on the layout direction."
);
impl_property_layout!(
    padding_top,
    set_padding_top,
    get_padding_top,
    SharedF32,
    "The padding at the top of the item."
);
impl_property_layout!(
    rotation,
    set_rotation,
    get_rotation,
    SharedF32,
    "The rotation of the item in degrees."
);
impl_property_layout!(
    rotation_center_x,
    set_rotation_center_x,
    get_rotation_center_x,
    SharedInnerPosition,
    "The center of rotation in the x direction."
);
impl_property_layout!(
    rotation_center_y,
    set_rotation_center_y,
    get_rotation_center_y,
    SharedInnerPosition,
    "The center of rotation in the y direction."
);
impl_property_layout!(
    scale_center_x,
    set_scale_center_x,
    get_scale_center_x,
    SharedInnerPosition,
    "The center of scaling in the x direction."
);
impl_property_layout!(
    scale_center_y,
    set_scale_center_y,
    get_scale_center_y,
    SharedInnerPosition,
    "The center of scaling in the y direction."
);
impl_property_layout!(
    scale_x,
    set_scale_x,
    get_scale_x,
    SharedF32,
    "The scale in the x direction."
);
impl_property_layout!(
    scale_y,
    set_scale_y,
    get_scale_y,
    SharedF32,
    "The scale in the y direction."
);
impl_property_layout!(
    skew_center_x,
    set_skew_center_x,
    get_skew_center_x,
    SharedInnerPosition,
    "The center of skew in the x direction."
);
impl_property_layout!(
    skew_center_y,
    set_skew_center_y,
    get_skew_center_y,
    SharedInnerPosition,
    "The center of skew in the y direction."
);
impl_property_layout!(
    skew_x,
    set_skew_x,
    get_skew_x,
    SharedF32,
    "The skew in the x direction in degrees."
);
impl_property_layout!(
    skew_y,
    set_skew_y,
    get_skew_y,
    SharedF32,
    "The skew in the y direction in degrees."
);
impl_property_layout!(
    width,
    set_width,
    get_width,
    SharedSize,
    "The width of the item. See [`Size`](crate::ui::item::Size) for more information."
);

impl_get_set!(
    apply_style,
    set_apply_style,
    impl FnMut(&mut ItemData, &Style) + 'static,
    "item, style",
    get_apply_style,
    dyn FnMut(&mut ItemData, &Style),
    "item, style"
);
impl_get_set!(
    cursor_move,
    set_cursor_move,
    impl FnMut(&mut ItemData, f32, f32) + 'static,
    "item, x, y",
    get_cursor_move,
    dyn FnMut(&mut ItemData, f32, f32),
    "item, x, y"
);
impl_get_set!(
    dispatch_apply_style,
    set_dispatch_apply_style,
    impl FnMut(&mut ItemData, &Style) + 'static,
    "item, style",
    get_dispatch_apply_style,
    dyn FnMut(&mut ItemData, &Style),
    "item, style"
);
impl_get_set!(
    dispatch_cursor_move,
    set_dispatch_cursor_move,
    impl FnMut(&mut ItemData, f32, f32) + 'static,
    "item, x, y",
    get_dispatch_cursor_move,
    dyn FnMut(&mut ItemData, f32, f32),
    "item, x, y"
);
impl_get_set!(
    dispatch_draw,
    set_dispatch_draw,
    impl FnMut(&mut ItemData, &mut Surface, f32, f32) + 'static,
    "item, surface, x, y",
    get_dispatch_draw,
    dyn FnMut(&mut ItemData, &mut Surface, f32, f32),
    "item, surface, x, y"
);
impl_get_set!(
    dispatch_focus,
    set_dispatch_focus,
    impl FnMut(&mut ItemData) + 'static,
    "item",
    get_dispatch_focus,
    dyn FnMut(&mut ItemData),
    "item"
);
impl_get_set!(
    dispatch_keyboard_input,
    set_dispatch_keyboard_input,
    impl FnMut(&mut ItemData, DeviceId, KeyEvent, bool) -> bool + 'static,
    "item, device_id, event, is_synthetic",
    get_dispatch_keyboard_input,
    dyn FnMut(&mut ItemData, DeviceId, KeyEvent, bool) -> bool,
    "item, device_id, event, is_synthetic"
);
impl_get_set!(
    dispatch_layout,
    set_dispatch_layout,
    impl FnMut(&mut ItemData, f32, f32, f32, f32) + 'static,
    "item, relative_x, relative_y, width, height",
    get_dispatch_layout,
    dyn FnMut(&mut ItemData, f32, f32, f32, f32),
    "item, relative_x, relative_y, width, height"
);
impl_get_set!(
    dispatch_mouse_input,
    set_dispatch_mouse_input,
    impl FnMut(&mut ItemData, MouseEvent) + 'static,
    "item, event",
    get_dispatch_mouse_input,
    dyn FnMut(&mut ItemData, MouseEvent),
    "item, event"
);
impl_get_set!(
    dispatch_mouse_wheel,
    set_dispatch_mouse_wheel,
    impl FnMut(&mut ItemData, MouseWheel) -> bool + 'static,
    "item, mouse_wheel",
    get_dispatch_mouse_wheel,
    dyn FnMut(&mut ItemData, MouseWheel) -> bool,
    "item, mouse_wheel"
);
impl_get_set!(
    dispatch_timer,
    set_dispatch_timer,
    impl FnMut(&mut ItemData, usize) -> bool + 'static,
    "item, id",
    get_dispatch_timer,
    dyn FnMut(&mut ItemData, usize) -> bool,
    "item, id"
);
impl_get_set!(
    dispatch_touch_input,
    set_dispatch_touch_input,
    impl FnMut(&mut ItemData, TouchEvent) + 'static,
    "item, event",
    get_dispatch_touch_input,
    dyn FnMut(&mut ItemData, TouchEvent),
    "item, event"
);
impl_get_set!(
    draw,
    set_draw,
    impl FnMut(&mut ItemData, &Canvas) + 'static,
    "item, canvas",
    get_draw,
    dyn FnMut(&mut ItemData, &Canvas),
    "item, canvas"
);
impl_get_set!(
    ime_input,
    set_ime_input,
    impl FnMut(&mut ItemData, ImeAction) + 'static,
    "item, action",
    get_ime_input,
    dyn FnMut(&mut ItemData, ImeAction),
    "item, action"
);
impl_get_set!(
    keyboard_input,
    set_keyboard_input,
    impl FnMut(&mut ItemData, DeviceId, KeyEvent, bool) -> bool + 'static,
    "item, device_id, event, is_synthetic",
    get_keyboard_input,
    dyn FnMut(&mut ItemData, DeviceId, KeyEvent, bool) -> bool,
    "item, device_id, event, is_synthetic"
);
impl_get_set!(
    layout,
    set_layout,
    impl FnMut(&mut ItemData, f32, f32) + 'static,
    "item, width, height",
    get_layout,
    dyn FnMut(&mut ItemData, f32, f32),
    "item, width, height"
);
impl_get_set!(
    measure,
    set_measure,
    impl FnMut(&mut ItemData, MeasureMode, MeasureMode) + 'static,
    r#"item, width_mode, height_mode
Do not retain any state in this closure, except for the `measure_parameter`.
Because this closure is used to calculate the recommended size of the item,
the `layout` closure is actually responsible for setting the actual size of the item.
"#,
    get_measure,
    dyn FnMut(&mut ItemData, MeasureMode, MeasureMode) + 'static,
    "item, width_mode, height_mode"
);
impl_get_set!(
    mouse_input,
    set_mouse_input,
    impl FnMut(&mut ItemData, MouseEvent) + 'static,
    "item, event",
    get_mouse_input,
    dyn FnMut(&mut ItemData, MouseEvent),
    "item, event"
);
impl_get_set!(
    mouse_wheel,
    set_mouse_wheel,
    impl FnMut(&mut ItemData, MouseWheel) -> bool + 'static,
    "item, mouse_wheel",
    get_mouse_wheel,
    dyn FnMut(&mut ItemData, MouseWheel) -> bool,
    "item, mouse_wheel"
);
impl_get_set!(
    click_event,
    set_click_event,
    impl FnMut(&mut ItemData, ClickSource) + 'static,
    "item, source",
    get_click_event,
    dyn FnMut(&mut ItemData, ClickSource),
    "item, source"
);
impl_get_set!(
    focus_event,
    set_focus_event,
    impl FnMut(&mut ItemData, bool) + 'static,
    "item, focused",
    get_focus_event,
    dyn FnMut(&mut ItemData, bool),
    "item, focused"
);
impl_get_set!(
    hover_event,
    set_hover_event,
    impl FnMut(&mut ItemData, bool) + 'static,
    "item, hover_state",
    get_hover_event,
    dyn FnMut(&mut ItemData, bool),
    "item, hover_state"
);
impl_get_set!(
    pointer_input,
    set_pointer_input,
    impl FnMut(&mut ItemData, PointerEvent) + 'static,
    "item, event",
    get_pointer_input,
    dyn FnMut(&mut ItemData, PointerEvent),
    "item, event"
);
impl_get_set!(
    timer,
    set_timer,
    impl FnMut(&mut ItemData, usize) -> bool + 'static,
    "item, id",
    get_timer,
    dyn FnMut(&mut ItemData, usize) -> bool,
    "item, id"
);
impl_get_set!(
    touch_input,
    set_touch_input,
    impl FnMut(&mut ItemData, TouchEvent) + 'static,
    "item, event",
    get_touch_input,
    dyn FnMut(&mut ItemData, TouchEvent),
    "item, event"
);

impl ItemData {
    pub fn add_on_attach<F>(mut self, f: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_attach.push_back(Box::new(f));
        self
    }

    pub fn add_on_detach<F>(mut self, f: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_detach.push_back(Box::new(f));
        self
    }

    pub fn custom_property(&mut self, name: impl Into<String>, property: CustomProperty) {
        self.custom_properties.insert(name.into(), property);
    }

    pub fn display_parameter_out(
        mut self,
        display_parameter_out: Shared<DisplayParameter>,
    ) -> Self {
        self.display_parameter_out.remove_observer(self.id);
        self.display_parameter_out = display_parameter_out;
        self.get_display_parameter();
        self
    }

    pub(crate) fn focus(&mut self, focused: bool) {
        let on_focus = self.get_on_focus();
        on_focus.iter_mut().for_each(|f| f(focused));
        let on_focus = self.get_focus_event();
        {
            let mut on_focus = on_focus.lock();
            on_focus(self, focused)
        }
    }

    pub fn focused(mut self, focused: impl Into<Shared<bool>>) -> Self {
        let self_item_id = self.id;
        self.focused.remove_observer(self_item_id);

        let app_context = self.app_context.clone();

        self.focused = focused.into();
        let self_item_id = self.id;
        let focused_property_clone = self.focused.clone();
        self.focused
            .add_specific_observer(self_item_id, move |focused| {
                enum Action {
                    Replace,
                    Clear,
                    Nothing,
                }
                let mut focused_property_value = app_context
                    .focused_property
                    .write(|focused_property| focused_property.take());
                let action = {
                    // There is an item that is focused
                    if let Some((property, item_id)) = focused_property_value.as_mut() {
                        if *item_id == self_item_id {
                            // The item is already focused
                            if !*focused {
                                // The item is not focused anymore
                                Action::Clear
                            } else {
                                Action::Nothing
                            }
                        } else {
                            // The item is not focused
                            if *focused {
                                property.set(false);
                                Action::Replace
                            } else {
                                Action::Nothing
                            }
                        }
                    } else {
                        // There is no item that is focused
                        if *focused {
                            Action::Replace
                        } else {
                            app_context
                                .focus_changed_items
                                .write(|focus_changed_items| {
                                    focus_changed_items.insert(self_item_id)
                                });
                            Action::Nothing
                        }
                    }
                };
                match action {
                    Action::Replace => {
                        app_context
                            .focus_changed_items
                            .write(|focus_changed_items| focus_changed_items.insert(self_item_id));
                        app_context.focused_property.write(|focused_property| {
                            focused_property.replace((focused_property_clone.clone(), self_item_id))
                        });
                        app_context.send_user_event(UserEvent::RequestFocus);
                    }
                    Action::Nothing => {
                        if let Some(v) = focused_property_value {
                            app_context.focused_property.write(move |focused_property| {
                                focused_property.replace((v.0.clone(), v.1))
                            });
                        }
                    }
                    _ => {}
                }
            });
        self
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
        bind_str_to_id(&self.name, self.id);
    }

    pub fn set_on_click<F>(&mut self, f: F)
    where
        F: FnMut(ClickSource) + 'static,
    {
        self.on_click = Some(Box::new(f));
    }

    pub fn set_on_cursor_move<F>(&mut self, f: F)
    where
        F: FnMut(f32, f32) + 'static,
    {
        self.on_cursor_move = Some(Box::new(f));
    }

    pub fn set_on_focus<F>(&mut self, f: F)
    where
        F: FnMut(bool) + 'static,
    {
        self.on_focus.push(Box::new(f));
    }

    pub fn set_on_hover<F>(&mut self, f: F)
    where
        F: FnMut(bool) + 'static,
    {
        self.on_hover = Some(Box::new(f));
    }

    pub fn set_on_mouse_input<F>(&mut self, f: F)
    where
        F: FnMut(MouseEvent) + 'static,
    {
        self.on_mouse_input = Some(Box::new(f));
    }

    pub fn set_on_pointer_input<F>(&mut self, f: F)
    where
        F: FnMut(PointerEvent) + 'static,
    {
        self.on_pointer_input = Some(Box::new(f));
    }

    pub fn set_on_touch_input<F>(&mut self, f: F)
    where
        F: FnMut(TouchEvent) + 'static,
    {
        self.on_touch_input = Some(Box::new(f));
    }

    pub fn get_app_context(&self) -> AppContext {
        self.app_context.clone()
    }

    pub fn get_baseline(&self) -> Option<f32> {
        self.baseline
    }

    pub fn get_children(&self) -> &Children {
        &self.children
    }

    pub fn get_children_mut(&mut self) -> &mut Children {
        &mut self.children
    }

    pub fn get_custom_property(&self, name: &str) -> Option<&CustomProperty> {
        self.custom_properties.get(name)
    }

    pub fn get_custom_property_mut(&mut self, name: &str) -> Option<&mut CustomProperty> {
        self.custom_properties.get_mut(name)
    }

    pub fn get_display_parameter(&mut self) -> DisplayParameter {
        let mut display_parameter = self.target_parameter.clone();
        calculate_animation_value!(parent_x, self, display_parameter);
        calculate_animation_value!(parent_y, self, display_parameter);
        calculate_animation_value!(width, self, display_parameter);
        calculate_animation_value!(height, self, display_parameter);
        calculate_animation_value!(relative_x, self, display_parameter);
        calculate_animation_value!(relative_y, self, display_parameter);
        calculate_animation_value!(offset_x, self, display_parameter);
        calculate_animation_value!(offset_y, self, display_parameter);
        calculate_animation_value!(opacity, self, display_parameter);
        calculate_animation_value!(rotation, self, display_parameter);
        calculate_animation_value!(rotation_center_x, self, display_parameter);
        calculate_animation_value!(rotation_center_y, self, display_parameter);
        calculate_animation_value!(scale_x, self, display_parameter);
        calculate_animation_value!(scale_y, self, display_parameter);
        calculate_animation_value!(scale_center_x, self, display_parameter);
        calculate_animation_value!(scale_center_y, self, display_parameter);
        calculate_animation_value!(skew_x, self, display_parameter);
        calculate_animation_value!(skew_y, self, display_parameter);
        calculate_animation_value!(skew_center_x, self, display_parameter);
        calculate_animation_value!(skew_center_y, self, display_parameter);
        self.animations
            .float_params
            .retain(|_, (_, animation)| !animation.is_finished());
        self.animations
            .float_params
            .iter()
            .for_each(|(key, (start, animation))| {
                if let Some(end) = display_parameter.float_params.get(key) {
                    display_parameter
                        .float_params
                        .insert(key.clone(), animation.interpolate_f32(*start, *end));
                }
            });
        self.animations
            .color_params
            .retain(|_, (_, animation)| !animation.is_finished());
        self.animations
            .color_params
            .iter()
            .for_each(|(key, (start, animation))| {
                if let Some(end) = display_parameter.color_params.get(key) {
                    display_parameter
                        .color_params
                        .insert(key.clone(), animation.interpolate_color(*start, *end));
                }
            });
        self.display_parameter_out
            .set_static(display_parameter.clone());
        display_parameter
    }

    pub fn get_focused(&self) -> Shared<bool> {
        self.focused.clone()
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn get_max_size(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.max_width.get(),
            Orientation::Vertical => self.max_height.get(),
        }
    }

    pub fn get_min_size(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.min_width.get(),
            Orientation::Vertical => self.min_height.get(),
        }
    }

    pub fn get_margin(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.margin_start.get() + self.margin_end.get(),
            Orientation::Vertical => self.margin_top.get() + self.margin_bottom.get(),
        }
    }

    pub fn get_margin_left(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => self.margin_start.get(),
            LayoutDirection::RTL => self.margin_end.get(),
        }
    }

    pub fn get_margin_right(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => self.margin_end.get(),
            LayoutDirection::RTL => self.margin_start.get(),
        }
    }

    pub fn get_measure_parameter(&mut self) -> &mut DisplayParameter {
        &mut self.measure_parameter
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_on_attach(&mut self) -> &mut LinkedList<Box<dyn FnMut()>> {
        &mut self.on_attach
    }

    pub fn get_on_click(&mut self) -> Option<&mut Box<dyn FnMut(ClickSource)>> {
        self.on_click.as_mut()
    }

    pub fn get_on_cursor_move(&mut self) -> Option<&mut Box<dyn FnMut(f32, f32)>> {
        self.on_cursor_move.as_mut()
    }

    pub fn get_on_detach(&mut self) -> &mut LinkedList<Box<dyn FnMut()>> {
        &mut self.on_detach
    }

    pub fn get_on_focus(&mut self) -> &mut Vec<Box<dyn FnMut(bool)>> {
        &mut self.on_focus
    }

    pub fn get_on_hover(&mut self) -> Option<&mut Box<dyn FnMut(bool)>> {
        self.on_hover.as_mut()
    }

    pub fn get_on_mouse_input(&mut self) -> Option<&mut Box<dyn FnMut(MouseEvent)>> {
        self.on_mouse_input.as_mut()
    }

    pub fn get_on_pointer_input(&mut self) -> Option<&mut Box<dyn FnMut(PointerEvent)>> {
        self.on_pointer_input.as_mut()
    }

    pub fn get_on_touch_input(&mut self) -> Option<&mut Box<dyn FnMut(TouchEvent)>> {
        self.on_touch_input.as_mut()
    }

    pub fn get_padding(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.padding_start.get() + self.padding_end.get(),
            Orientation::Vertical => self.padding_top.get() + self.padding_bottom.get(),
        }
    }

    pub fn get_padding_left(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => self.padding_start.get(),
            LayoutDirection::RTL => self.padding_end.get(),
        }
    }

    pub fn get_padding_right(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => self.padding_end.get(),
            LayoutDirection::RTL => self.padding_start.get(),
        }
    }

    pub fn get_size(&self, orientation: Orientation) -> Size {
        match orientation {
            Orientation::Horizontal => self.width.get(),
            Orientation::Vertical => self.height.get(),
        }
    }

    pub fn get_target_parameter(&mut self) -> &mut DisplayParameter {
        &mut self.target_parameter
    }

    pub fn clamp_height(&self, height: f32) -> f32 {
        let min_height = self.min_height.get();
        let max_height = self.max_height.get();
        height.clamp(min_height, max_height)
    }

    pub fn clamp_width(&self, width: f32) -> f32 {
        let min_width = self.min_width.get();
        let max_width = self.max_width.get();
        width.clamp(min_width, max_width)
    }

    pub fn clone_measure_parameter(&self) -> DisplayParameter {
        self.measure_parameter.clone()
    }

    pub(crate) fn dispatch_animation(&mut self, animation: Animation) {
        if !animation.is_target(self.id) {
            return;
        }
        if let Some(recorded_parameter) = self.recorded_parameter.clone() {
            let target_parameter = self.target_parameter.clone();
            if !f32_eq(recorded_parameter.width, target_parameter.width) {
                self.animations.parent_x = Some((recorded_parameter.parent_x, animation.clone()));
            }
            if !f32_eq(recorded_parameter.height, target_parameter.height) {
                self.animations.parent_y = Some((recorded_parameter.parent_y, animation.clone()));
            }
            if !f32_eq(recorded_parameter.relative_x, target_parameter.relative_x) {
                self.animations.relative_x =
                    Some((recorded_parameter.relative_x, animation.clone()));
            }
            if !f32_eq(recorded_parameter.relative_y, target_parameter.relative_y) {
                self.animations.relative_y =
                    Some((recorded_parameter.relative_y, animation.clone()));
            }
            if !f32_eq(recorded_parameter.offset_x, target_parameter.offset_x) {
                self.animations.offset_x = Some((recorded_parameter.offset_x, animation.clone()));
            }
            if !f32_eq(recorded_parameter.offset_y, target_parameter.offset_y) {
                self.animations.offset_y = Some((recorded_parameter.offset_y, animation.clone()));
            }
            if !f32_eq(recorded_parameter.opacity, target_parameter.opacity) {
                self.animations.opacity = Some((recorded_parameter.opacity, animation.clone()));
            }
            if !f32_eq(recorded_parameter.rotation, target_parameter.rotation) {
                self.animations.rotation = Some((recorded_parameter.rotation, animation.clone()));
            }
            if !f32_eq(
                recorded_parameter.rotation_center_x,
                target_parameter.rotation_center_x,
            ) {
                self.animations.rotation_center_x =
                    Some((recorded_parameter.rotation_center_x, animation.clone()));
            }
            if !f32_eq(
                recorded_parameter.rotation_center_y,
                target_parameter.rotation_center_y,
            ) {
                self.animations.rotation_center_y =
                    Some((recorded_parameter.rotation_center_y, animation.clone()));
            }
            if !f32_eq(recorded_parameter.scale_x, target_parameter.scale_x) {
                self.animations.scale_x = Some((recorded_parameter.scale_x, animation.clone()));
            }
            if !f32_eq(recorded_parameter.scale_y, target_parameter.scale_y) {
                self.animations.scale_y = Some((recorded_parameter.scale_y, animation.clone()));
            }
            if !f32_eq(
                recorded_parameter.scale_center_x,
                target_parameter.scale_center_x,
            ) {
                self.animations.scale_center_x =
                    Some((recorded_parameter.scale_center_x, animation.clone()));
            }
            if !f32_eq(
                recorded_parameter.scale_center_y,
                target_parameter.scale_center_y,
            ) {
                self.animations.scale_center_y =
                    Some((recorded_parameter.scale_center_y, animation.clone()));
            }
            if !f32_eq(recorded_parameter.skew_x, target_parameter.skew_x) {
                self.animations.skew_x = Some((recorded_parameter.skew_x, animation.clone()));
            }
            if !f32_eq(recorded_parameter.skew_y, target_parameter.skew_y) {
                self.animations.skew_y = Some((recorded_parameter.skew_y, animation.clone()));
            }
            if !f32_eq(
                recorded_parameter.skew_center_x,
                target_parameter.skew_center_x,
            ) {
                self.animations.skew_center_x =
                    Some((recorded_parameter.skew_center_x, animation.clone()));
            }
            if !f32_eq(
                recorded_parameter.skew_center_y,
                target_parameter.skew_center_y,
            ) {
                self.animations.skew_center_y =
                    Some((recorded_parameter.skew_center_y, animation.clone()));
            }
            if !f32_eq(recorded_parameter.width, target_parameter.width) {
                self.animations.width = Some((recorded_parameter.width, animation.clone()));
            }
            if !f32_eq(recorded_parameter.height, target_parameter.height) {
                self.animations.height = Some((recorded_parameter.height, animation.clone()));
            }

            {
                recorded_parameter
                    .float_params
                    .iter()
                    .for_each(|(key, start)| {
                        if let Some(end) = target_parameter.float_params.get(key).clone() {
                            if !f32_eq(*start, *end) {
                                self.animations
                                    .float_params
                                    .insert(key.clone(), (start.clone(), animation.clone()));
                            }
                        }
                    });
            }

            {
                recorded_parameter
                    .color_params
                    .iter()
                    .for_each(|(key, start)| {
                        if let Some(end) = target_parameter.color_params.get(key).clone() {
                            if start != end {
                                self.animations
                                    .color_params
                                    .insert(key.clone(), (start.clone(), animation.clone()));
                            }
                        }
                    });
            }
        }

        self.children.items().iter_mut().for_each(|child| {
            child.data().dispatch_animation(animation.clone());
        });
    }

    pub fn dispatch_draw(&mut self, surface: &mut Surface, parent_x: f32, parent_y: f32) {
        let f = self.get_dispatch_draw();
        f.lock()(self, surface, parent_x, parent_y);
    }

    pub fn dispatch_focus(&mut self) {
        let f = self.get_dispatch_focus();
        f.lock()(self);
    }

    pub fn dispatch_keyboard_input(
        &mut self,
        device_id: DeviceId,
        event: KeyEvent,
        is_synthetic: bool,
    ) -> bool {
        let f = self.get_dispatch_keyboard_input();
        let r = f.lock()(self, device_id, event.clone(), is_synthetic);
        r
    }

    pub fn dispatch_layout(&mut self, relative_x: f32, relative_y: f32, width: f32, height: f32) {
        let f = self.get_dispatch_layout();
        f.lock()(self, relative_x, relative_y, width, height);
    }

    pub fn dispatch_mouse_input(&mut self, event: MouseEvent) {
        let f = self.get_dispatch_mouse_input();
        f.lock()(self, event);
    }

    pub fn dispatch_mouse_wheel(&mut self, event:MouseWheel) ->bool {
        let f = self.get_dispatch_mouse_wheel();
        let r =f.lock()(self, event);
        r
    }

    pub fn dispatch_cursor_move(&mut self, x: f32, y: f32) {
        let f = self.get_dispatch_cursor_move();
        f.lock()(self, x, y);
    }

    pub fn dispatch_timer(&mut self, timer_id: usize) {
        let f = self.get_dispatch_timer();
        f.lock()(self, timer_id);
    }

    pub fn dispatch_touch_input(&mut self, event: TouchEvent) {
        let f = self.get_dispatch_touch_input();
        f.lock()(self, event);
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        let f = self.get_draw();
        f.lock()(self, canvas);
    }

    pub fn find_item(&self, id: usize, f: &mut impl FnMut(&ItemData)) {
        if self.id == id {
            f(self);
        } else {
            for child in self.children.items().iter() {
                child.data().find_item(id, f);
            }
        }
    }

    pub fn find_item_mut(&mut self, id: usize, f: &mut impl FnMut(&mut ItemData)) {
        if self.id == id {
            f(self);
        } else {
            for child in self.children.items().iter_mut() {
                child.data().find_item_mut(id, f);
            }
        }
    }

    pub fn for_each_child<F>(&self, mut f: F)
    where
        F: FnMut(&Item),
    {
        for child in self.children.items().iter() {
            f(child);
        }
    }

    pub fn for_each_child_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Item),
    {
        for child in self.children.items().iter_mut() {
            f(child);
        }
    }

    pub fn is_animating(&self) -> bool {
        self.animations.is_animating()
    }

    pub fn ime_input(&mut self, event: ImeAction) {
        let f = self.get_ime_input();
        f.lock()(self, event.clone());
    }

    pub fn layout(&mut self, width: f32, height: f32) {
        let f = self.get_layout();
        f.lock()(self, width, height);
    }

    fn layout_layer(layer: SharedItem, width: f32, height: f32) {
        if let Some(item) = layer.value().as_mut() {
            item.data().measure(
                MeasureMode::Specified(width),
                MeasureMode::Specified(height),
            );
            item.data().dispatch_layout(0.0, 0.0, width, height);
        }
    }

    pub fn layout_layers(&self, width: f32, height: f32) {
        Self::layout_layer(self.get_background(), width, height);
        Self::layout_layer(self.get_foreground(), width, height);
    }

    pub fn measure(&mut self, width_mode: MeasureMode, height_mode: MeasureMode) {
        let f = self.get_measure();
        f.lock()(self, width_mode, height_mode);
    }

    pub fn measure_children(&mut self, width_mode: MeasureMode, height_mode: MeasureMode) {
        let max_width = match width_mode {
            MeasureMode::Specified(width) => width,
            MeasureMode::Unspecified(width) => width,
        };
        let max_height = match height_mode {
            MeasureMode::Specified(height) => height,
            MeasureMode::Unspecified(height) => height,
        };

        fn create_mode(size: Size, max_size: f32) -> MeasureMode {
            match size {
                Size::Compact => MeasureMode::Unspecified(max_size),
                Size::Expanded => MeasureMode::Specified(max_size),
                Size::Fixed(size) => MeasureMode::Specified(size),
                Size::Relative(ratio) => MeasureMode::Specified(max_size * ratio),
            }
        }

        self.for_each_child_mut(|child| {
            let child_width = child.data().get_width().get();
            let child_height = child.data().get_height().get();
            let max_width = child.data().clamp_width(max_width);
            let max_height = child.data().clamp_height(max_height);
            child.data().measure(
                create_mode(child_width, max_width),
                create_mode(child_height, max_height),
            );
        });
    }

    pub(crate) fn record_display_parameter(&mut self) {
        self.recorded_parameter = Some(self.get_display_parameter());
        self.children.items().iter_mut().for_each(|child| {
            child.data().record_display_parameter();
        });
    }

    pub fn set_base_line(&mut self, base_line: f32) {
        self.baseline = Some(base_line);
    }

    pub fn set_target_parameter(&mut self, parameter: DisplayParameter) {
        self.target_parameter.copy_from(&parameter)
    }
}

fn f32_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.1
}

pub struct Item {
    data: Arc<Mutex<ItemData>>,
}

impl Item {
    pub fn new(app_context: AppContext, children: Children) -> Self {
        let data = ItemData::new(app_context, children);
        Self {
            data: Arc::new(Mutex::new(data)),
        }
    }

    pub fn data(&self) -> MutexGuard<ItemData> {
        self.data.lock()
    }

    pub fn name(self, name: impl Into<String>) -> Self {
        self.data().set_name(name);
        self
    }

    pub fn on_click<F>(self, f: F) -> Self
    where
        F: FnMut(ClickSource) + 'static,
    {
        self.data().set_on_click(f);
        self
    }

    pub fn on_cursor_move<F>(self, f: F) -> Self
    where
        F: FnMut(f32, f32) + 'static,
    {
        self.data().set_on_cursor_move(f);
        self
    }

    pub fn on_focus<F>(self, f: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        self.data().set_on_focus(f);
        self
    }

    pub fn on_hover<F>(self, f: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        self.data().set_on_hover(f);
        self
    }

    pub fn on_mouse_input<F>(self, f: F) -> Self
    where
        F: FnMut(MouseEvent) + 'static,
    {
        self.data().set_on_mouse_input(f);
        self
    }

    pub fn on_pointer_input<F>(self, f: F) -> Self
    where
        F: FnMut(PointerEvent) + 'static,
    {
        self.data().set_on_pointer_input(f);
        self
    }

    pub fn on_touch_input<F>(self, f: F) -> Self
    where
        F: FnMut(TouchEvent) + 'static,
    {
        self.data().set_on_touch_input(f);
        self
    }
}

unsafe impl Send for Item {}