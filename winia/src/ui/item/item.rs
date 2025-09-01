use crate::core::{bind_str_to_id, next_id, unbind_id};
use crate::shared::{Children, Gettable, LocalObservable, Observable, Settable, Shared, SharedAlignment, SharedBool, SharedColor, SharedF32, SharedInnerPosition, SharedItem, SharedSize, SharedUsize};
use crate::ui::animation::{Target, Animation};
use crate::ui::app::WindowContext;
use crate::ui::item::{DisplayParameter, InnerPosition, Size};
use crate::ui::theme::color;
use crate::ui::{LayoutAnimation, Theme};
use parking_lot::{Mutex, MutexGuard};
use proc_macro::AsRef;
use skia_safe::image_filters::CropRect;
use skia_safe::{
    image_filters, surfaces, BlendMode, Canvas, Color, IRect, Paint, Path, Point, Rect, Surface,
    TileMode, Vector,
};
use std::any::Any;
use std::collections::{HashMap, HashSet, LinkedList};
use std::ops::{Add, DerefMut, Not};
use std::sync::{Arc, Weak};
use std::time::Instant;
use winit::event::{DeviceId, Force, KeyEvent, Modifiers, MouseButton, TouchPhase};

pub fn layout<T: Send>(
    mut property: Shared<T>,
    id: usize,
    window_context: &WindowContext,
) -> Shared<T> {
    let event_loop_proxy = window_context.event_loop_proxy().clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_layout();
            }),
        )
        .drop();
    property
}

pub fn init_property_layout<T: Send>(
    property: &mut Shared<T>,
    id: usize,
    window_context: &WindowContext,
) {
    let event_loop_proxy = window_context.event_loop_proxy().clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_layout();
            }),
        )
        .drop();
}

pub fn redraw<T: Send>(
    mut property: Shared<T>,
    id: usize,
    window_context: &WindowContext,
) -> Shared<T> {
    let event_loop_proxy = window_context.event_loop_proxy().clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_redraw();
            }),
        )
        .drop();
    property
}

pub fn init_property_redraw<T: Send>(
    property: &mut Shared<T>,
    id: usize,
    window_context: &WindowContext,
) {
    let event_loop_proxy = window_context.event_loop_proxy().clone();
    property
        .add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_redraw();
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
                init_property_layout(&mut self.$property_name, self.id, &self.window_context);
            }

            pub fn $get_property_name(&self) -> &$property_type {
                &self.$property_name
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
                init_property_redraw(&mut self.$property_name, self.id, &self.window_context);
            }

            pub fn $get_property_name(&self) -> &$property_type {
                &self.$property_name
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

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
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

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
pub enum HorizontalAlignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
pub enum LayoutDirection {
    LTR,
    RTL,
}

macro_rules! calculate_animation_value {
    ($name:ident, $s:ident, $display_parameter:ident) => {
        let p = {
            if let Some((start, end, animation)) = &$s.animations.$name {
                Some((start, end, animation.clone()))
            } else {
                None
            }
        };
        if let Some((start, _, animation)) = p {
            if !animation.is_finished() {
                $display_parameter.$name =
                    animation.interpolate_f32(*start, $display_parameter.$name);
            } else {
                $s.animations.$name = None;
            }
        }
    };
}

macro_rules! override_animation {
    ($animation:ident, $recorded_parameter:ident, $target_parameter:ident, $self_:ident, $name:ident) => {{
        let recorded = $recorded_parameter.$name;
        let target = $target_parameter.$name;
        if !f32_eq(recorded, target)
            && $self_
                .animations
                .$name
                .as_ref()
                .map_or(true, |(_, end, _)| *end != target)
        {
            $self_.animations.$name = Some((recorded, target, $animation.clone_boxed()));
        }
    }};
}

macro_rules! override_animations {
    ($animation:ident, $recorded_parameter:ident, $target_parameter:ident, $self_:ident, $($name:ident),+) => {
        $(
            override_animation!($animation, $recorded_parameter, $target_parameter, $self_, $name);
        )+
    }
}

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Self::Horizontal)
    }

    pub fn is_vertical(&self) -> bool {
        matches!(self, Self::Vertical)
    }
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

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
pub enum MeasureMode {
    /// Indicates that the parent has determined an exact size for the child.
    Specified(f32),
    /// Indicates that the child can determine its own size. The value of this enum is the maximum size the child can use.
    Unspecified(f32),
}

impl MeasureMode {
    pub fn from_size(size:Size, max: f32) -> Self {
        match size {
            Size::Auto => MeasureMode::Unspecified(max),
            Size::Fill => MeasureMode::Specified(max),
            Size::Fixed(size) => MeasureMode::Specified(size),
            Size::Relative(ratio) => MeasureMode::Specified(max * ratio.clamp(0.0, f32::MAX))
        }
    }
}

impl Into<f32> for MeasureMode {
    fn into(self) -> f32 {
        match self {
            MeasureMode::Specified(v) => v,
            MeasureMode::Unspecified(v) => v
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, AsRef)]
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

#[derive(Clone, Copy, Debug, AsRef)]
pub struct MouseInput {
    pub device_id: DeviceId,
    pub x: f32,
    pub y: f32,
    pub button: MouseButton,
    pub pointer_state: PointerState,
}

#[derive(Clone, Copy, Debug, AsRef)]
pub struct TouchInput {
    pub device_id: DeviceId,
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub pointer_state: PointerState,
    pub force: Option<Force>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, AsRef)]
pub enum Pointer {
    Touch { id: u64 },
    Mouse { button: MouseButton },
}

#[derive(Clone, Copy, Debug, AsRef)]
pub struct PointerInput {
    pub device_id: DeviceId,
    pub pointer: Pointer,
    pub x: f32,
    pub y: f32,
    pub pointer_state: PointerState,
    pub force: Option<Force>,
}

impl From<&TouchInput> for PointerInput {
    fn from(value: &TouchInput) -> Self {
        PointerInput {
            device_id: value.device_id,
            pointer: Pointer::Touch { id: value.id },
            x: value.x,
            y: value.y,
            pointer_state: value.pointer_state,
            force: value.force,
        }
    }
}

impl From<&MouseInput> for PointerInput {
    fn from(value: &MouseInput) -> Self {
        PointerInput {
            device_id: value.device_id,
            pointer: Pointer::Mouse {
                button: value.button,
            },
            x: value.x,
            y: value.y,
            pointer_state: value.pointer_state,
            force: None,
        }
    }
}

#[derive(Clone, Debug, AsRef)]
pub enum ImeAction {
    Enabled,
    Enter,
    Delete,
    PreEdit(String, Option<(usize, usize)>),
    Commit(String),
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, AsRef)]
pub enum ClickSource {
    Mouse(MouseButton),
    Touch,
    LongTouch,
}

#[derive(Debug, Clone, Copy, PartialEq, AsRef)]
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

#[derive(Debug, Clone, Copy, PartialEq, AsRef)]
pub struct MouseWheel {
    pub device_id: DeviceId,
    pub delta: MouseScrollDelta,
    pub state: PointerState,
}

#[derive(Debug, Clone, Copy, PartialEq, AsRef)]
pub struct CursorMove {
    pub device_id: DeviceId,
    pub x: f32,
    pub y: f32,
    pub is_left_window: bool,
}

#[derive(Debug, Clone, PartialEq, AsRef)]
pub struct KeyboardInput {
    pub device_id: DeviceId,
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

pub enum CustomProperty {
    Usize(SharedUsize),
    Float(SharedF32),
    Color(SharedColor),
    Bool(SharedBool),
    Any(Box<dyn Any>),
}
type AnimationOption = Option<(f32, f32, Box<dyn Animation>)>;
#[derive(Default)]
struct Animations {
    // (start, end, animation)
    width: AnimationOption,
    height: AnimationOption,
    relative_x: AnimationOption,
    relative_y: AnimationOption,
    offset_x: AnimationOption,
    offset_y: AnimationOption,
    opacity: AnimationOption,
    rotation: AnimationOption,
    rotation_center_x: AnimationOption,
    rotation_center_y: AnimationOption,
    scale_x: AnimationOption,
    scale_y: AnimationOption,
    scale_center_x: AnimationOption,
    scale_center_y: AnimationOption,
    skew_x: AnimationOption,
    skew_y: AnimationOption,
    skew_center_x: AnimationOption,
    skew_center_y: AnimationOption,
    float_params: HashMap<String, (f32, f32, Box<dyn Animation>)>,
    color_params: HashMap<String, (Color, Color, Box<dyn Animation>)>,
}

impl Animations {
    fn is_animating(&self) -> bool {
        self.width.is_some()
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

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ItemState {
    #[default]
    Enabled,
    Disabled,
    Focused,
    Hovered,
    Pressed,
}

/// An item is a basic building block of the UI system. It can be used to display text, images, or other content.
/// It can also be used to arrange other lock in a layout.
pub struct ItemData {
    active: SharedBool,
    align_content: SharedAlignment,
    animations: Animations,
    window_context: WindowContext,
    background: SharedItem,
    baseline: Option<f32>,
    blur: Shared<f32>,
    children: Children,
    clip: Shared<bool>,
    clip_shape: Shared<Box<dyn Fn(&mut ItemData) -> Path + Send>>,
    custom_properties: HashMap<String, CustomProperty>,
    display_parameter_out: Shared<DisplayParameter>,
    elevation: SharedF32,
    enabled: SharedBool,
    enable_background_blur: SharedBool,
    focusable: Shared<bool>,
    focused: Shared<bool>,
    focused_when_clicked: Shared<bool>,
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
    on_click: Option<Arc<Mutex<dyn FnMut(ClickSource)>>>,
    on_cursor_move: Option<Box<dyn FnMut(f32, f32)>>,
    on_detach: LinkedList<Box<dyn FnMut()>>,
    on_focus: Vec<Box<dyn FnMut(bool)>>,
    on_hover: Option<Box<dyn FnMut(bool)>>,
    on_keyboard_input: Option<Box<dyn FnMut(&KeyboardInput) -> bool>>,
    on_mouse_input: Option<Box<dyn FnMut(&MouseInput)>>,
    on_pointer_input: Option<Box<dyn FnMut(&PointerInput)>>,
    on_touch_input: Option<Box<dyn FnMut(&TouchInput)>>,
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
    state: Shared<ItemState>,
    target_parameter: DisplayParameter,
    visible: SharedBool,
    width: SharedSize,

    apply_theme: Arc<Mutex<dyn FnMut(&mut ItemData, &Theme)>>,
    cursor_move: Arc<Mutex<dyn FnMut(&mut ItemData, &CursorMove)>>,
    dispatch_apply_theme: Arc<Mutex<dyn FnMut(&mut ItemData, &Theme)>>,
    dispatch_cursor_move: Arc<Mutex<dyn FnMut(&mut ItemData, &CursorMove)>>,
    dispatch_draw: Arc<Mutex<dyn FnMut(&mut ItemData, &mut Surface, f32, f32)>>,
    dispatch_focus: Arc<Mutex<dyn FnMut(&mut ItemData)>>,
    dispatch_keyboard_input: Arc<Mutex<dyn FnMut(&mut ItemData, &KeyboardInput) -> bool>>,
    dispatch_layout: Arc<Mutex<dyn FnMut(&mut ItemData, f32, f32, f32, f32)>>,
    dispatch_modifiers_changed: Arc<Mutex<dyn FnMut(&mut ItemData, &Modifiers)>>,
    dispatch_mouse_input: Arc<Mutex<dyn FnMut(&mut ItemData, &MouseInput)>>,
    dispatch_mouse_wheel_x: Arc<Mutex<dyn FnMut(&mut ItemData, &MouseWheel) -> bool>>,
    dispatch_mouse_wheel_y: Arc<Mutex<dyn FnMut(&mut ItemData, &MouseWheel) -> bool>>,
    dispatch_timer: Arc<Mutex<dyn FnMut(&mut ItemData, usize) -> bool>>,
    dispatch_touch_input: Arc<Mutex<dyn FnMut(&mut ItemData, &TouchInput)>>,
    draw: Arc<Mutex<dyn FnMut(&mut ItemData, &Canvas)>>,
    focus_next: Arc<Mutex<dyn FnMut(&mut ItemData) -> bool>>,
    ime_input: Arc<Mutex<dyn FnMut(&mut ItemData, &ImeAction)>>,
    keyboard_input: Arc<Mutex<dyn FnMut(&mut ItemData, &KeyboardInput) -> bool>>,
    layout: Arc<Mutex<dyn FnMut(&mut ItemData, f32, f32)>>,
    measure: Arc<Mutex<dyn FnMut(&mut ItemData, MeasureMode, MeasureMode)>>,
    modifiers_changed: Arc<Mutex<dyn FnMut(&mut ItemData, &Modifiers)>>,
    mouse_input: Arc<Mutex<dyn FnMut(&mut ItemData, &MouseInput)>>,
    mouse_wheel_x: Arc<Mutex<dyn FnMut(&mut ItemData, &MouseWheel) -> bool>>,
    mouse_wheel_y: Arc<Mutex<dyn FnMut(&mut ItemData, &MouseWheel) -> bool>>,
    click_event: Arc<Mutex<dyn FnMut(&mut ItemData, ClickSource)>>,
    focus_event: Arc<Mutex<dyn FnMut(&mut ItemData, bool)>>,
    hover_event: Arc<Mutex<dyn FnMut(&mut ItemData, bool)>>,
    pointer_input: Arc<Mutex<dyn FnMut(&mut ItemData, &PointerInput)>>,
    timer: Arc<Mutex<dyn FnMut(&mut ItemData, usize) -> bool>>,
    touch_input: Arc<Mutex<dyn FnMut(&mut ItemData, &TouchInput)>>,
}

// unsafe impl Send for ItemData {}

impl ItemData {
    pub fn new(window_context: &WindowContext, children: Children) -> Self {
        let id = next_id();

        let state: Shared<ItemState> = ItemState::Enabled.into();
        let mut item = Self {
            active: layout(true.into(), id, window_context),
            align_content: layout(Alignment::TopStart.into(), id, window_context),
            animations: Default::default(),
            window_context: window_context.clone(),
            background: {
                let mut item = SharedItem::none();
                let event_loop_proxy = window_context.event_loop_proxy().clone();
                item.add_observer(
                    id,
                    Box::new(move || {
                        event_loop_proxy.request_layout();
                    }),
                )
                .drop();
                item
            },
            baseline: None,
            blur: redraw(35.0.into(), id, window_context),
            children: children.layout_when_changed(window_context.event_loop_proxy(), id),
            clip: redraw(false.into(), id, window_context),
            clip_shape: redraw(
                Shared::from_static(Box::new(|item| {
                    let display_parameter = item.get_display_parameter();
                    let rect = Rect::from_xywh(
                        display_parameter.x(),
                        display_parameter.y(),
                        display_parameter.width,
                        display_parameter.height,
                    );
                    Path::rect(rect, None)
                })),
                id,
                window_context,
            ),
            custom_properties: HashMap::new(),
            display_parameter_out: DisplayParameter::default().into(),
            elevation: redraw(0.0.into(), id, window_context),
            enabled: {
                let enabled: SharedBool = true.into();
                let event_loop_proxy = window_context.event_loop_proxy().clone();
                let state = state.clone();
                enabled.add_specific_observer(id, move |enabled| {
                    event_loop_proxy.request_layout();
                    let state_value = state.get();
                    if *enabled {
                        if state_value == ItemState::Disabled {
                            state.set(ItemState::Enabled);
                        }
                    } else {
                        match state_value {
                            ItemState::Enabled
                            | ItemState::Hovered
                            | ItemState::Pressed
                            | ItemState::Focused => {
                                state.set(ItemState::Disabled);
                            }
                            _ => {}
                        }
                    }
                });
                enabled
            },
            enable_background_blur: redraw(false.into(), id, window_context),
            focusable: layout(false.into(), id, window_context),
            focused: layout(false.into(), id, window_context),
            focused_when_clicked: layout(false.into(), id, window_context),
            foreground: {
                let mut item = SharedItem::none();
                let event_loop_proxy = window_context.event_loop_proxy().clone();
                item.add_observer(
                    id,
                    Box::new(move || {
                        event_loop_proxy.request_layout();
                    }),
                )
                .drop();
                item
            },
            height: redraw(Size::Auto.into(), id, window_context),
            id,
            layout_direction: layout(LayoutDirection::LTR.into(), id, window_context),
            margin_bottom: layout(0.0.into(), id, window_context),
            margin_end: layout(0.0.into(), id, window_context),
            margin_start: layout(0.0.into(), id, window_context),
            margin_top: layout(0.0.into(), id, window_context),
            max_height: layout(f32::INFINITY.into(), id, window_context),
            max_width: layout(f32::INFINITY.into(), id, window_context),
            measure_parameter: Default::default(),
            min_height: layout(0.0.into(), id, window_context),
            min_width: layout(0.0.into(), id, window_context),
            name: format!("Item {}", id),
            offset_x: layout(0.0.into(), id, window_context),
            offset_y: layout(0.0.into(), id, window_context),
            on_attach: LinkedList::new(),
            on_click: None,
            on_cursor_move: None,
            on_detach: LinkedList::new(),
            on_focus: Vec::new(),
            on_hover: None,
            on_keyboard_input: None,
            on_mouse_input: None,
            on_pointer_input: None,
            on_touch_input: None,
            opacity: redraw(1.0.into(), id, window_context),
            padding_bottom: layout(0.0.into(), id, window_context),
            padding_end: layout(0.0.into(), id, window_context),
            padding_start: layout(0.0.into(), id, window_context),
            padding_top: layout(0.0.into(), id, window_context),
            recorded_parameter: None,
            rotation: redraw(0.0.into(), id, window_context),
            rotation_center_x: redraw(InnerPosition::default().into(), id, window_context),
            rotation_center_y: redraw(InnerPosition::default().into(), id, window_context),
            scale_center_x: redraw(InnerPosition::default().into(), id, window_context),
            scale_center_y: redraw(InnerPosition::default().into(), id, window_context),
            scale_x: redraw(1.0.into(), id, window_context),
            scale_y: redraw(1.0.into(), id, window_context),
            skew_center_x: redraw(InnerPosition::default().into(), id, window_context),
            skew_center_y: redraw(InnerPosition::default().into(), id, window_context),
            skew_x: redraw(0.0.into(), id, window_context),
            skew_y: redraw(0.0.into(), id, window_context),
            state,
            target_parameter: Default::default(),
            visible: layout(true.into(), id, window_context),
            width: layout(Size::Auto.into(), id, window_context),

            apply_theme: Arc::new(Mutex::new(|_item: &mut ItemData, _theme: &Theme| {})),
            cursor_move: Arc::new(Mutex::new(
                |_item: &mut ItemData, _cursor_move: &CursorMove| {},
            )),
            dispatch_apply_theme: Arc::new(Mutex::new(|item: &mut ItemData, theme: &Theme| {
                let background = item.get_background();
                if let Some(background) = background.lock().as_mut() {
                    background.data().dispatch_apply_theme(theme);
                }

                let foreground = item.get_foreground();
                if let Some(foreground) = foreground.lock().as_mut() {
                    foreground.data().dispatch_apply_theme(theme);
                }

                // item.get_children().lock().iter_mut().for_each(|child| {
                //     child.data().dispatch_apply_theme(theme);
                // });
                {
                    let children = item.get_children().lock();
                    for child in children.iter() {
                        child.data().dispatch_apply_theme(theme);
                    }
                }

                let apply_theme = item.get_apply_theme();
                apply_theme.lock()(item, theme);
            })),
            dispatch_cursor_move: Arc::new(Mutex::new({
                let mut is_hovered = false;
                move |item: &mut ItemData, cursor_move: &CursorMove| {
                    if !item.get_enabled().get() {
                        return;
                    }
                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.lock().as_mut() {
                        foreground.data().dispatch_cursor_move(cursor_move);
                    }
                    let background = item.get_background();
                    if let Some(background) = background.lock().as_mut() {
                        background.data().dispatch_cursor_move(cursor_move);
                    }

                    if let Some(on_cursor_move) = item.get_on_cursor_move() {
                        on_cursor_move(cursor_move.x, cursor_move.y);
                    }

                    item.get_cursor_move().lock()(item, cursor_move);

                    if item
                        .get_display_parameter()
                        .is_inside(cursor_move.x, cursor_move.y)
                        && !cursor_move.is_left_window
                    {
                        if !is_hovered {
                            is_hovered = true;
                            item.get_hover_event().lock()(item, true);
                            if let Some(on_hover) = item.get_on_hover() {
                                on_hover(true);
                            }
                        }
                    } else if is_hovered {
                        is_hovered = false;
                        item.get_hover_event().lock()(item, false);
                        if let Some(on_hover) = item.get_on_hover() {
                            on_hover(false);
                        }
                    }

                    let (window_width, window_height) = item.get_window_context().window_size();
                    // rayon::iter::ParallelIterator::for_each(
                    //     rayon::iter::ParallelIterator::filter(
                    //         item.get_children().lock().par_iter_mut(),
                    //         |item| {
                    //             let display_parameter = item.data().get_display_parameter();
                    //             let x = display_parameter.x();
                    //             let y = display_parameter.y();
                    //             let width = display_parameter.width;
                    //             let height = display_parameter.height;
                    //             let x_overlap = x < window_width && x + width > 0.0;
                    //             let y_overlap = y < window_height && y + height > 0.0;
                    //             x_overlap && y_overlap
                    //         },
                    //     ),
                    //     |child| {
                    //         child.data().dispatch_cursor_move(x, y);
                    //     },
                    // );
                    // use rayon::iter::ParallelIterator;
                    item.get_children()
                        .lock()
                        .iter_mut() /*.par_iter_mut()*/
                        .for_each(|child| {
                            let display_parameter = child.data().get_display_parameter();
                            let item_x = display_parameter.x();
                            let item_y = display_parameter.y();
                            let item_width = display_parameter.width;
                            let item_height = display_parameter.height;
                            let x_overlap = item_x < window_width && item_x + item_width > 0.0;
                            let y_overlap = item_y < window_height && item_y + item_height > 0.0;
                            if x_overlap && y_overlap {
                                child.data().dispatch_cursor_move(cursor_move);
                            }
                        });
                }
            })),
            dispatch_draw: Arc::new(Mutex::new({
                let mut image_filter_paint = Paint::default();
                let mut shadow_paint = Paint::default();
                shadow_paint
                    .set_anti_alias(true)
                    .set_blend_mode(BlendMode::SrcIn);
                move |item: &mut ItemData, surface: &mut Surface, parent_x: f32, parent_y: f32| {
                    {
                        // Set the parent position of the target parameter of the item.
                        // It's child lock can use the parent position to calculate their own position.
                        // Why not update the parent position of the target parameter of the item in the layout event?
                        // Because animation can change the position of the item without notifying the layout event.
                        let target_parameter = item.get_target_parameter();
                        target_parameter.set_parent_position(parent_x, parent_y);
                    }

                    // if !item.get_visible().get() {
                    //     return;
                    // }

                    let display_parameter = item.get_display_parameter();
                    {
                        let (window_width, window_height) = item.get_window_context().window_size();
                        let x = display_parameter.x();
                        let y = display_parameter.y();
                        let width = display_parameter.width;
                        let height = display_parameter.height;
                        let x_overlap = x < window_width && x + width > 0.0;
                        let y_overlap = y < window_height && y + height > 0.0;
                        if !x_overlap || !y_overlap {
                            return;
                        }
                    }

                    {
                        // Draw the background blur effect.
                        let blur = /*35.0*/item.get_blur().get();
                        let margin = blur * 2.0;
                        if item.get_enable_background_blur().get() && !display_parameter.is_empty()
                        {
                            let scale_factor = item.get_window_context().scale_factor();
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
                            image_filter_paint.set_image_filter(image_filters::blur(
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
                            canvas.draw_image(
                                background,
                                Point::new(0.0, 0.0),
                                Some(&image_filter_paint),
                            );
                            canvas.restore();
                        }
                    }

                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let rotation = display_parameter.rotation;
                    let rotation_center_x = display_parameter.rotation_center_x + x;
                    let rotation_center_y = display_parameter.rotation_center_y + y;

                    let skew_x = display_parameter.skew_x;
                    let skew_y = display_parameter.skew_y;
                    let skew_center_x = display_parameter.skew_center_x + x;
                    let skew_center_y = display_parameter.skew_center_y + y;
                    let scale_x = display_parameter.scale_x;
                    let scale_y = display_parameter.scale_y;
                    let scale_center_x = display_parameter.scale_center_x + x;
                    let scale_center_y = display_parameter.scale_center_y + y;
                    // if item.get_name() == "blue" {
                    //     println!("")
                    // }

                    {
                        // Apply the transformation matrix to the canvas.
                        let canvas = surface.canvas();
                        if display_parameter.opacity < 1.0 {
                            // canvas.save();
                            canvas.save_layer_alpha_f(
                                Rect::from_xywh(
                                    display_parameter.x(),
                                    display_parameter.y(),
                                    display_parameter.width,
                                    display_parameter.height,
                                ),
                                display_parameter.opacity,
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

                    {
                        // Draw the shadow
                        let elevation = item.get_elevation().get() / 2.0;
                        if elevation > 0.0
                            && (display_parameter.width > 0.0 && display_parameter.height > 0.0)
                        {
                            let theme = item.window_context.theme.lock();
                            let shadow_color = theme
                                .get_color(color::SHADOW)
                                .unwrap()
                                .with_a((0.5 * 255.0) as u8);
                            shadow_paint.set_color(shadow_color);
                            drop(theme);
                            let blur_sigma = elevation * 1.5;
                            let shadow_offset = elevation;

                            let mut shadow_surface = surfaces::raster_n32_premul((
                                display_parameter.width.ceil() as i32,
                                display_parameter.height.ceil() as i32,
                            ))
                            .unwrap();

                            let shadow_canvas = shadow_surface.canvas();
                            shadow_canvas
                                .translate((-display_parameter.x(), -display_parameter.y()));
                            item.draw(shadow_canvas);

                            shadow_canvas.draw_paint(&shadow_paint);

                            let shadow_image = shadow_surface.image_snapshot();

                            let rect = Rect::from_xywh(
                                display_parameter.x() - elevation * 6.0,
                                display_parameter.y() - elevation * 6.0,
                                display_parameter.width + elevation * 12.0,
                                display_parameter.height + elevation * 12.0,
                            );
                            image_filter_paint.set_image_filter(image_filters::blur(
                                (blur_sigma, blur_sigma),
                                TileMode::Clamp,
                                None,
                                CropRect::from(rect),
                            ));

                            let canvas = surface.canvas();
                            canvas.draw_image(
                                shadow_image,
                                Point::new(
                                    display_parameter.x(),
                                    display_parameter.y() + shadow_offset,
                                ),
                                Some(&image_filter_paint),
                            );
                        }
                    }

                    let clip = item.get_clip().get();
                    if clip {
                        let canvas = surface.canvas();
                        canvas.save();
                        let clip_shape = item.get_clip_shape().clone();
                        let path = clip_shape.lock().as_ref()(item);
                        canvas.clip_path(&path, None, true);
                    }

                    {
                        // Draw the background
                        if let Some(background) = item.get_background().lock().as_mut() {
                            background.data().dispatch_draw(surface, x, y);
                        }
                    }
                    {
                        // Draw the item itself.
                        let canvas = surface.canvas();
                        item.draw(canvas);
                    }

                    // Draw the children of the item.
                    //let (window_width, window_height) = item.get_window_context().window_size();
                    item.get_children()
                        .lock()
                        .iter_mut()
                        // .iter_visible_item(window_size)
                        .for_each(|child| {
                            child.data().dispatch_draw(surface, x, y);
                        });

                    {
                        // Draw the foreground
                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.data().dispatch_draw(surface, x, y);
                        }
                    }

                    if clip {
                        // Restore the transformation matrix of the canvas.
                        let canvas = surface.canvas();
                        canvas.restore();
                    }

                    {
                        // Restore the transformation matrix of the canvas.
                        let canvas = surface.canvas();
                        canvas.restore();
                    }
                }
            })),
            dispatch_focus: Arc::new(Mutex::new(|item: &mut ItemData| {
                let (last, new) = item.get_window_context().item_focused.get();
                // let focused = item.get_focused().get();
                // {
                //     let focus_changed_items = focus_changed_items.lock();
                //     if focus_changed_items.contains(&item.get_id()) {
                //         item.focus(focused)
                //     }
                // }
                let mut focus_changed = false;
                if let Some((_new, id)) = new {
                    if id == item.get_id() {
                        focus_changed = true;
                    }
                }
                if let Some((last, id)) = last {
                    if id == item.get_id() && !last.get() {
                        focus_changed = true;
                    }
                }
                
                if focus_changed {
                    let focused = item.get_focused().get();
                    item.focus(focused);
                    item.focus_event.clone().lock()(item, focused);
                }
                
                item.get_children().lock().iter_mut().for_each(|child| {
                    child.data().dispatch_focus();
                });
            })),
            dispatch_keyboard_input: Arc::new(Mutex::new(
                |item: &mut ItemData, keyboard_input: &KeyboardInput| {
                    if !item.get_enabled().get() {
                        return false;
                    }
                    if let Some(on_keyboard_input) = item.get_on_keyboard_input() {
                        if on_keyboard_input(keyboard_input) {
                            return true;
                        }
                    }
                    if /*item.get_focused().get()
                        && */item.get_keyboard_input().lock()(item, keyboard_input)
                    {
                        return true;
                    }
                    item.get_children().lock().iter_mut().any(|child| {
                        let dispatch_keyboard_input = child.data().get_dispatch_keyboard_input();
                        let r = dispatch_keyboard_input.lock()(
                            child.data().deref_mut(),
                            keyboard_input,
                        );
                        r
                    })
                },
            )),
            dispatch_layout: Arc::new(Mutex::new(
                |item: &mut ItemData, relative_x: f32, relative_y: f32, width: f32, height: f32| {
                    {
                        let measure_parameter = item.get_measure_parameter();
                        if width != measure_parameter.width
                            || height != measure_parameter.height
                        {
                            item.measure(
                                MeasureMode::Specified(width),
                                MeasureMode::Specified(height),
                            );
                        }
                    }
                    let visible = item.get_visible().get();
                    let offset_x = item.get_offset_x().get();
                    let offset_y = item.get_offset_y().get();
                    // let opacity = item.get_opacity().get();
                    let opacity = if visible {
                        item.get_opacity().get()
                    } else {
                        0.0
                    };
                    let rotation = item.get_rotation().get();
                    // let scale_x = item.get_scale_x().get();
                    // let scale_y = item.get_scale_y().get();
                    let scale_x = if visible {
                        item.get_scale_x().get()
                    } else {
                        0.0
                    };
                    let scale_y = if visible {
                        item.get_scale_y().get()
                    } else {
                        0.0
                    };
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

                    item.layout_layers(width, height);

                    item.layout(width, height);
                },
            )),
            dispatch_modifiers_changed: Arc::new(Mutex::new(
                |item: &mut ItemData, modifiers: &Modifiers| {
                    if !item.get_enabled().get() {
                        return;
                    }
                    item.get_modifiers_changed().lock()(item, modifiers);
                    item.get_children().lock().iter_mut().for_each(|child| {
                        let dispatch_modifiers_changed = child.data().get_dispatch_modifiers_changed();
                        dispatch_modifiers_changed.lock()(child.data().deref_mut(), modifiers);
                    });
                },
            )),
            dispatch_mouse_input: Arc::new(Mutex::new({
                // The mouse button that the item has captured.
                // When the item captures a mouse button, the item can receive mouse input
                // events even if the mouse pointer is outside the item.
                let mut captured_mouse_button: HashSet<(usize, MouseButton)> = HashSet::new();
                // The source of the click event.
                let mut click_source: Option<ClickSource> = None;
                move |item: &mut ItemData, mouse_input: &MouseInput| {
                    if !item.get_enabled().get() {
                        return;
                    }
                    let x = mouse_input.x;
                    let y = mouse_input.y;

                    // If the item captures the mouse button,
                    // the foreground and background of the item can receive all mouse input events.
                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.lock().as_mut() {
                        foreground.data().dispatch_mouse_input(mouse_input);
                    }

                    let background = item.get_background();
                    if let Some(background) = background.lock().as_mut() {
                        background.data().dispatch_mouse_input(mouse_input);
                    }

                    match mouse_input.pointer_state {
                        PointerState::Started => {
                            let item_state = item.get_state();
                            item_state.set(ItemState::Pressed);
                        }
                        PointerState::Ended | PointerState::Cancelled => {
                            let item_state = item.get_state();
                            if item_state.get() == ItemState::Pressed && item.get_enabled().get() {
                                item_state.set(ItemState::Enabled);
                            }
                        }
                        _=> {}
                    }

                    // Call the on_mouse_input event of the item.
                    if let Some(on_mouse_input) = item.get_on_mouse_input() {
                        on_mouse_input(mouse_input);
                    }

                    {
                        // Call the mouse_input event of the item_event.
                        // Why there are two mouse_input events?
                        // Because winia don't want to expose item object to the user.
                        item.get_mouse_input().lock()(item, mouse_input);

                        let pointer_input = PointerInput::from(mouse_input);
                        item.get_pointer_input().lock()(item, &pointer_input);
                        if let Some(on_pointer_input) = item.get_on_pointer_input() {
                            on_pointer_input(&pointer_input)
                        }
                    }

                    let mut click_consumed = false;
                    {
                        // Dispatch the mouse input events to the child lock.
                        let children = item.get_children();
                        for child in children.lock().iter_mut().rev() {
                            let display_parameter = child.data().get_display_parameter();
                            match mouse_input.pointer_state {
                                PointerState::Started => {
                                    // If the mouse pointer is inside the child item,
                                    if display_parameter.is_inside(x, y) {
                                        // The child item captures the mouse button.
                                        captured_mouse_button
                                            .insert((child.data().get_id(), mouse_input.button));
                                        child.data().dispatch_mouse_input(mouse_input);
                                        // Other child lock can't receive the mouse input events.
                                        click_consumed = child.data().get_on_click().is_some();
                                        break;
                                    }
                                }
                                PointerState::Moved => {
                                    // If the child item captures the mouse button
                                    if captured_mouse_button
                                        .contains(&(child.data().get_id(), mouse_input.button))
                                    {
                                        child.data().dispatch_mouse_input(mouse_input);
                                        click_consumed = child.data().get_on_click().is_some();
                                        break;
                                    }
                                }
                                PointerState::Ended | PointerState::Cancelled => {
                                    // If the child item captures the mouse button
                                    if captured_mouse_button
                                        .contains(&(child.data().get_id(), mouse_input.button))
                                    {
                                        // The child item releases the mouse button.
                                        captured_mouse_button
                                            .remove(&(child.data().get_id(), mouse_input.button));
                                        child.data().dispatch_mouse_input(mouse_input);
                                        click_consumed = child.data().get_on_click().is_some();
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // Handle the click event.
                    if !click_consumed {
                        match mouse_input.pointer_state {
                            PointerState::Started => {
                                click_source.replace(ClickSource::Mouse(mouse_input.button));
                            }
                            PointerState::Ended => {
                                if item.get_display_parameter().is_inside(x, y) {
                                    let is_clicked = {
                                        click_source == Some(ClickSource::Mouse(mouse_input.button))
                                    };
                                    if is_clicked {
                                        if item.get_focusable().get() && item.get_focused_when_clicked().get() {
                                            item.get_focused().set(true)
                                        }
                                        item.get_click_event().lock()(item, click_source.unwrap());
                                        if let Some(on_click) = item.get_on_click() {
                                            on_click.lock()(click_source.unwrap());
                                        }
                                    }
                                    click_source.take();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            })),
            dispatch_mouse_wheel_x: Arc::new(Mutex::new(
                |item: &mut ItemData, mouse_wheel: &MouseWheel| {
                    if !item.get_enabled().get() {
                        return false;
                    }
                    let children = item.get_children();
                    let (cursor_x, cursor_y) = item.get_window_context().get_cursor_position();
                    for child in children.lock().iter_mut().rev() {
                        let display_parameter = child.data().get_display_parameter();
                        if display_parameter.is_inside(cursor_x, cursor_y) {
                            let dispatch_mouse_wheel = child.data().get_dispatch_mouse_wheel_x();
                            let r = dispatch_mouse_wheel.lock()(child.data().deref_mut(), mouse_wheel);
                            if r {
                                return true;
                            }
                        }
                    }
                    if item.get_mouse_wheel_x().lock()(item, mouse_wheel) {
                        return true;
                    }
                    false
                },
            )),
            dispatch_mouse_wheel_y: Arc::new(Mutex::new(
                |item: &mut ItemData, mouse_wheel: &MouseWheel| {
                    if !item.get_enabled().get() {
                        return false;
                    }
                    let children = item.get_children();
                    let (cursor_x, cursor_y) = item.get_window_context().get_cursor_position();
                    for child in children.lock().iter_mut().rev() {
                        let display_parameter = child.data().get_display_parameter();
                        if display_parameter.is_inside(cursor_x, cursor_y) {
                            let dispatch_mouse_wheel = child.data().get_dispatch_mouse_wheel_y();
                            let r = dispatch_mouse_wheel.lock()(child.data().deref_mut(), mouse_wheel);
                            if r {
                                return true;
                            }
                        }
                    }
                    if item.get_mouse_wheel_y().lock()(item, mouse_wheel) {
                        return true;
                    }
                    false
                },
            )),
            dispatch_timer: Arc::new(Mutex::new(|item: &mut ItemData, id: usize| {
                let timer = item.get_timer();
                if timer.lock()(item, id) {
                    return true;
                }
                item.get_children().lock().iter_mut().any(|child| {
                    let dispatch_timer = child.data().get_dispatch_timer();
                    let r = dispatch_timer.lock()(child.data().deref_mut(), id);
                    r
                })
            })),
            dispatch_touch_input: Arc::new(Mutex::new({
                // item_id, touch_id
                let mut captured_touch_pointer: HashSet<(usize, u64)> = HashSet::new();
                let mut touch_start_time = Instant::now();
                move |item: &mut ItemData, touch_input: &TouchInput| {
                    if !item.get_enabled().get() {
                        return;
                    }
                    let x = touch_input.x;
                    let y = touch_input.y;

                    let foreground = item.get_foreground();
                    if let Some(foreground) = foreground.lock().as_mut() {
                        foreground.data().dispatch_touch_input(touch_input);
                    }
                    let background = item.get_background();
                    if let Some(background) = background.lock().as_mut() {
                        background.data().dispatch_touch_input(touch_input);
                    }

                    if let Some(on_touch) = &mut item.get_on_touch_input() {
                        on_touch(touch_input);
                    }

                    let pointer_input = PointerInput::from(touch_input);
                    item.get_pointer_input().lock()(item, &pointer_input);
                    if let Some(on_pointer_input) = item.get_on_pointer_input() {
                        on_pointer_input(&pointer_input)
                    }

                    let mut click_consumed = false;

                    {
                        let children = item.get_children();
                        for child in children.lock().iter_mut().rev() {
                            let display_parameter = child.data().get_display_parameter();
                            match touch_input.pointer_state {
                                PointerState::Started => {
                                    if display_parameter.is_inside(x, y) {
                                        captured_touch_pointer
                                            .insert((child.data().get_id(), touch_input.id));
                                        child.data().dispatch_touch_input(touch_input);
                                        click_consumed = child.data().get_on_click().is_some();
                                        break;
                                    }
                                }
                                PointerState::Moved => {
                                    if captured_touch_pointer
                                        .contains(&(child.data().get_id(), touch_input.id))
                                    {
                                        child.data().dispatch_touch_input(touch_input);
                                        click_consumed = child.data().get_on_click().is_some();
                                        break;
                                    }
                                }
                                PointerState::Ended | PointerState::Cancelled => {
                                    if captured_touch_pointer
                                        .contains(&(child.data().get_id(), touch_input.id))
                                    {
                                        let child_id = child.data().get_id();
                                        captured_touch_pointer
                                            .retain(|&(item_id, _touch_id)| item_id != child_id);
                                        child.data().dispatch_touch_input(touch_input);
                                        click_consumed = child.data().get_on_click().is_some();
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if !click_consumed {
                        match touch_input.pointer_state {
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
                                    on_click.lock()(click_source);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            })),
            draw: Arc::new(Mutex::new(|_item: &mut ItemData, _canvas: &Canvas| {})),
            focus_next: Arc::new(Mutex::new({
                let mut last_focused_index = 0_usize;
                move |item: &mut ItemData| {
                    let children = item.get_children().lock();
                    if children.is_empty() {
                        return true
                    }
                    loop {
                        if last_focused_index >= children.len() {
                            last_focused_index = 0;
                            return true;
                        }
                        if let Some(child) = children.get(last_focused_index) {
                            if child.data().focus_next() {
                                last_focused_index += 1;
                            } else { 
                                return false;
                            }
                        } else {
                            last_focused_index = 0;
                            return true;
                        }
                    }
                }
            })),
            ime_input: Arc::new(Mutex::new(|_item: &mut ItemData, _action: &ImeAction| {})),
            keyboard_input: Arc::new(Mutex::new(
                |_item: &mut ItemData, _keyboard_input: &KeyboardInput| false,
            )),
            layout: Arc::new(Mutex::new(
                |_item: &mut ItemData, _width: f32, _height: f32| {},
            )),
            measure: Arc::new(Mutex::new(
                |item: &mut ItemData, width_mode, height_mode| {
                    item.measure_children(width_mode, height_mode);
                    fn get_size(measure_mode: MeasureMode) -> f32 {
                        match measure_mode {
                            MeasureMode::Specified(value) => value,
                            MeasureMode::Unspecified(_) => 0.0,
                        }
                    }

                    let width = item.clamp_width(get_size(width_mode));
                    let height = item.clamp_height(get_size(height_mode));

                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = width;
                    measure_parameter.height = height;
                },
            )),
            modifiers_changed: Arc::new(Mutex::new(|_item: &mut ItemData, _modifiers: &Modifiers| {})),
            mouse_input: Arc::new(Mutex::new(|_item: &mut ItemData, _event: &MouseInput| {})),
            mouse_wheel_x: Arc::new(Mutex::new(
                |_item: &mut ItemData, _mouse_wheel: &MouseWheel| false,
            )),
            mouse_wheel_y: Arc::new(Mutex::new(
                |_item: &mut ItemData, _mouse_wheel: &MouseWheel| false,
            )),
            click_event: Arc::new(Mutex::new(|_item: &mut ItemData, _source: ClickSource| {})),
            focus_event: Arc::new(Mutex::new(|item: &mut ItemData, focused: bool| {
                let state = item.get_state();
                let state_value = state.get();
                if focused {
                    if state_value == ItemState::Enabled {
                        state.set(ItemState::Focused);
                    }
                } else if state_value == ItemState::Focused {
                    state.set(ItemState::Enabled);
                }
            })),
            hover_event: Arc::new(Mutex::new(|item: &mut ItemData, hover: bool| {
                let state = item.get_state();
                let state_value = state.get();
                if hover {
                    if state_value == ItemState::Enabled {
                        state.set(ItemState::Hovered);
                    }
                } else if state_value == ItemState::Hovered {
                    state.set(ItemState::Enabled);
                }
            })),
            pointer_input: Arc::new(Mutex::new(|_item: &mut ItemData, _event: &PointerInput| {})),
            timer: Arc::new(Mutex::new(|_item: &mut ItemData, _id: usize| false)),
            touch_input: Arc::new(Mutex::new(|_item: &mut ItemData, _event: &TouchInput| {})),
        };
        item.set_focused(false);
        item
    }
}

impl Drop for ItemData {
    fn drop(&mut self) {
        unbind_id(self.id);
        self.window_context.set_ime_allowed(self.id, false);
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
    "The alignment of the content of the item. Not all lock support this property."
);
// impl_property_layout!(
//     background,
//     set_background,
//     get_background,
//     SharedItem,
//     "The background of the item. It will be drawn behind the content (including children)"
// );
impl ItemData {
    pub fn set_background(&mut self, background: impl Into<SharedItem>) {
        self.background.remove_observer(self.id);
        self.background = background.into();
        let event_loop_proxy = self.window_context.event_loop_proxy().clone();
        self.background
            .add_observer(
                self.id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            )
            .drop();
    }

    pub fn get_background(&self) -> &SharedItem {
        &self.background
    }

    pub fn get_state(&self) -> &Shared<ItemState> {
        &self.state
    }
}
impl Item {
    pub fn background(self, background: impl Into<SharedItem>) -> Self {
        self.data().set_background(background);
        self
    }
}
impl_property_redraw!(
    blur,
    set_blur,
    get_blur,
    SharedF32,
    "The blur radius of the item. This will cause the item to be blurred when it is drawn."
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
    Shared<Box<dyn Fn(&mut ItemData) -> Path + Send>>,
    "The shape used to clip the content of the item. If this is set, the content will be clipped to the shape."
);
// impl_property_redraw!(
//     enabled,
//     set_enabled,
//     get_enabled,
//     SharedBool,
//     "Whether the item is enabled and can receive input events. If this is set to false, the item will not receive input events."
// );
impl ItemData {
    pub fn set_enabled(&mut self, enabled: impl Into<SharedBool>) {
        self.enabled.remove_observer(self.id);
        self.enabled = enabled.into();
        let event_loop_proxy = self.window_context.event_loop_proxy().clone();
        let state = self.state.clone();
        let id = self.id;
        self.enabled.add_specific_observer(id, move |enabled| {
            event_loop_proxy.request_layout();
            let state_value = state.get();
            if *enabled {
                if state_value == ItemState::Disabled {
                    state.set(ItemState::Enabled);
                }
            } else {
                match state_value {
                    ItemState::Enabled
                    | ItemState::Hovered
                    | ItemState::Pressed
                    | ItemState::Focused => {
                        state.set(ItemState::Disabled);
                    }
                    _ => {}
                }
            }
        });
        self.enabled.notify();
    }

    pub fn get_enabled(&self) -> &SharedBool {
        &self.enabled
    }
}

impl Item {
    pub fn enabled(self, enabled: impl Into<SharedBool>) -> Self {
        self.data().set_enabled(enabled);
        self
    }
}

impl_property_layout!(
    enable_background_blur,
    set_enable_background_blur,
    get_enable_background_blur,
    SharedBool,
    "Whether to enable background blur. This will cause the background to be blurred when it is not fully opaque."
);
impl_property_layout!(
    elevation,
    set_elevation,
    get_elevation,
    SharedF32,
    "The elevation of the item. It will affect the shadow of the item."
);
impl_property_layout!(
    focusable,
    set_focusable,
    get_focusable,
    SharedBool,
    "Whether the item can be focused. If this is set to false, the item will not receive focus events."
);
impl_property_layout!(
    focused_when_clicked,
    set_focused_when_clicked,
    get_focused_when_clicked,
    SharedBool,
    "Whether the item will be focused when clicked. If this is set to false, the item will not receive focus events when clicked."
);

// impl_property_layout!(
//     foreground,
//     set_foreground,
//     get_foreground,
//     SharedItem,
//     "The foreground of the item. It will be drawn in front of the content (including children)"
// );
impl ItemData {
    pub fn set_foreground(&mut self, foreground: impl Into<SharedItem>) {
        self.foreground.remove_observer(self.id);
        self.foreground = foreground.into();
        let event_loop_proxy = self.window_context.event_loop_proxy().clone();
        self.foreground
            .add_observer(
                self.id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            )
            .drop();
    }

    pub fn get_foreground(&self) -> &SharedItem {
        &self.foreground
    }
}
impl Item {
    pub fn foreground(self, foreground: impl Into<SharedItem>) -> Self {
        self.data().set_foreground(foreground);
        self
    }
}
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
    visible,
    set_visible,
    get_visible,
    SharedBool,
    "Whether the item is visible. If this is set to false, the item will not be drawn."
);
impl_property_layout!(
    width,
    set_width,
    get_width,
    SharedSize,
    "The width of the item. See [`Size`](crate::ui::item::Size) for more information."
);

impl_get_set!(
    apply_theme,
    set_apply_theme,
    impl FnMut(&mut ItemData, &Theme) + 'static,
    "item, theme",
    get_apply_theme,
    dyn FnMut(&mut ItemData, &Theme),
    "item, theme"
);
impl_get_set!(
    cursor_move,
    set_cursor_move,
    impl FnMut(&mut ItemData, &CursorMove) + 'static,
    "item, cursor_move",
    get_cursor_move,
    dyn FnMut(&mut ItemData, &CursorMove),
    "item, cursor_move"
);
impl_get_set!(
    dispatch_apply_theme,
    set_dispatch_apply_theme,
    impl FnMut(&mut ItemData, &Theme) + 'static,
    "item, theme",
    get_dispatch_apply_theme,
    dyn FnMut(&mut ItemData, &Theme),
    "item, theme"
);
impl_get_set!(
    dispatch_cursor_move,
    set_dispatch_cursor_move,
    impl FnMut(&mut ItemData, &CursorMove) + 'static,
    "item, cursor_move",
    get_dispatch_cursor_move,
    dyn FnMut(&mut ItemData, &CursorMove),
    "item, cursor_move"
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
    impl FnMut(&mut ItemData, &KeyboardInput) -> bool + 'static,
    "item, keyboard_input",
    get_dispatch_keyboard_input,
    dyn FnMut(&mut ItemData, &KeyboardInput) -> bool,
    "item, keyboard_input"
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
    dispatch_modifiers_changed,
    set_dispatch_modifiers_changed,
    impl FnMut(&mut ItemData, &Modifiers) + 'static,
    "item, modifiers",
    get_dispatch_modifiers_changed,
    dyn FnMut(&mut ItemData, &Modifiers),
    "item, modifiers"
);
impl_get_set!(
    dispatch_mouse_input,
    set_dispatch_mouse_input,
    impl FnMut(&mut ItemData, &MouseInput) + 'static,
    "item, event",
    get_dispatch_mouse_input,
    dyn FnMut(&mut ItemData, &MouseInput),
    "item, event"
);
impl_get_set!(
    dispatch_mouse_wheel_x,
    set_dispatch_mouse_wheel_x,
    impl FnMut(&mut ItemData, &MouseWheel) -> bool + 'static,
    "item, mouse_wheel",
    get_dispatch_mouse_wheel_x,
    dyn FnMut(&mut ItemData, &MouseWheel) -> bool,
    "item, mouse_wheel"
);
impl_get_set!(
    dispatch_mouse_wheel_y,
    set_dispatch_mouse_wheel_y,
    impl FnMut(&mut ItemData, &MouseWheel) -> bool + 'static,
    "item, mouse_wheel",
    get_dispatch_mouse_wheel_y,
    dyn FnMut(&mut ItemData, &MouseWheel) -> bool,
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
    impl FnMut(&mut ItemData, &TouchInput) + 'static,
    "item, event",
    get_dispatch_touch_input,
    dyn FnMut(&mut ItemData, &TouchInput),
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
    focus_next,
    set_focus_next,
    impl FnMut(&mut ItemData) -> bool + 'static,
    "item",
    get_focus_next,
    dyn FnMut(&mut ItemData) -> bool,
    "item"
);
impl_get_set!(
    ime_input,
    set_ime_input,
    impl FnMut(&mut ItemData, &ImeAction) + 'static,
    "item, action",
    get_ime_input,
    dyn FnMut(&mut ItemData, &ImeAction),
    "item, action"
);
impl_get_set!(
    keyboard_input,
    set_keyboard_input,
    impl FnMut(&mut ItemData, &KeyboardInput) -> bool + 'static,
    "item, keyboard_input",
    get_keyboard_input,
    dyn FnMut(&mut ItemData, &KeyboardInput) -> bool,
    "item, keyboard_input"
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
    modifiers_changed,
    set_modifiers_changed,
    impl FnMut(&mut ItemData, &Modifiers) + 'static,
    "item, modifiers",
    get_modifiers_changed,
    dyn FnMut(&mut ItemData, &Modifiers),
    "item, modifiers"
);
impl_get_set!(
    mouse_input,
    set_mouse_input,
    impl FnMut(&mut ItemData, &MouseInput) + 'static,
    "item, event",
    get_mouse_input,
    dyn FnMut(&mut ItemData, &MouseInput),
    "item, event"
);
impl_get_set!(
    mouse_wheel_x,
    set_mouse_wheel_x,
    impl FnMut(&mut ItemData, &MouseWheel) -> bool + 'static,
    "item, mouse_wheel",
    get_mouse_wheel_x,
    dyn FnMut(&mut ItemData, &MouseWheel) -> bool,
    "item, mouse_wheel"
);
impl_get_set!(
    mouse_wheel_y,
    set_mouse_wheel_y,
    impl FnMut(&mut ItemData, &MouseWheel) -> bool + 'static,
    "item, mouse_wheel",
    get_mouse_wheel_y,
    dyn FnMut(&mut ItemData, &MouseWheel) -> bool,
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
    impl FnMut(&mut ItemData, &PointerInput) + 'static,
    "item, event",
    get_pointer_input,
    dyn FnMut(&mut ItemData, &PointerInput),
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
    impl FnMut(&mut ItemData, &TouchInput) + 'static,
    "item, event",
    get_touch_input,
    dyn FnMut(&mut ItemData, &TouchInput),
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

    pub fn set_focused(&mut self, focused: impl Into<Shared<bool>>) {
        let self_item_id = self.id;
        self.focused.remove_observer(self_item_id);

        let event_loop_proxy = self.window_context.event_loop_proxy.clone();
        let item_focused = self.get_window_context().item_focused.clone();

        self.focused = focused.into();
        let item_id = self.id;
        let my_focused = self.focused.clone();
        self.focused.add_specific_observer(item_id, move |focused| {
            event_loop_proxy.request_layout();
            if *focused {
                if let Some(mut item_focused) = item_focused.try_lock() {
                    let (last, new) = item_focused.deref_mut();
                    if let Some((new_, _id)) = new {
                        if new_.id() != my_focused.id() {
                            new_.try_set_static(false);
                        }
                        *new = None;
                    }
                    if let Some((last, _id)) = last {
                        if last.id() != my_focused.id() {
                            last.try_set_static(false);
                            //println!("last {}", last.get());
                            *new = Some((my_focused.clone(), item_id));
                        }
                    }
                    if new.is_none() && last.is_none() {
                        *new = Some((my_focused.clone(), item_id));
                    }
                }
            } else if let Some(mut item_focused) = item_focused.try_lock() {
                let (_last, new) = item_focused.deref_mut();
                if let Some((new_v, _)) = new {
                    if new_v.id() == my_focused.id() {
                        *new = None;
                    }
                }
            }
        });
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
        bind_str_to_id(&self.name, self.id);
    }

    pub fn set_on_click<F>(&mut self, f: F)
    where
        F: FnMut(ClickSource) + 'static,
    {
        self.on_click = Some(Arc::new(Mutex::new(f)));
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
    
    pub fn set_on_keyboard_input<F>(&mut self, f: F)
    where
        F: FnMut(&KeyboardInput) -> bool + 'static,
    {
        self.on_keyboard_input = Some(Box::new(f));
    }

    pub fn set_on_mouse_input<F>(&mut self, f: F)
    where
        F: FnMut(&MouseInput) + 'static,
    {
        self.on_mouse_input = Some(Box::new(f));
    }

    pub fn set_on_pointer_input<F>(&mut self, f: F)
    where
        F: FnMut(&PointerInput) + 'static,
    {
        self.on_pointer_input = Some(Box::new(f));
    }

    pub fn set_on_touch_input<F>(&mut self, f: F)
    where
        F: FnMut(&TouchInput) + 'static,
    {
        self.on_touch_input = Some(Box::new(f));
    }

    pub fn get_window_context(&self) -> &WindowContext {
        &self.window_context
    }

    pub fn get_baseline(&self) -> Option<f32> {
        if self.visible.get() {
            self.baseline
        } else {
            None
        }
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
        // calculate_animation_value!(parent_x, self, display_parameter);
        // calculate_animation_value!(parent_y, self, display_parameter);
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
            .retain(|_, (_, _, animation)| !animation.is_finished());
        self.animations
            .float_params
            .iter()
            .for_each(|(key, (start, _, animation))| {
                if let Some(end) = display_parameter.float_params.get(key) {
                    display_parameter
                        .float_params
                        .insert(key.clone(), animation.interpolate_f32(*start, *end));
                }
            });
        self.animations
            .color_params
            .retain(|_, (_, _, animation)| !animation.is_finished());
        self.animations
            .color_params
            .iter()
            .for_each(|(key, (start, _, animation))| {
                if let Some(end) = display_parameter.color_params.get(key) {
                    display_parameter
                        .color_params
                        .insert(key.clone(), animation.interpolate_color(start, end));
                }
            });
        self.display_parameter_out
            .set_static(display_parameter.clone());
        display_parameter
    }

    pub fn get_focused(&self) -> &Shared<bool> {
        &self.focused
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
        let visible = self.visible.get();
        let margin_start = self.margin_start.get();
        let margin_end = self.margin_end.get();
        let margin_top = self.margin_top.get();
        let margin_bottom = self.margin_bottom.get();
        let padding_start = self.padding_start.get();
        let padding_end = self.padding_end.get();
        let padding_top = self.padding_top.get();
        let padding_bottom = self.padding_bottom.get();
        self.measure_parameter.visible = visible;
        self.measure_parameter.margin_start = margin_start;
        self.measure_parameter.margin_end = margin_end;
        self.measure_parameter.margin_top = margin_top;
        self.measure_parameter.margin_bottom = margin_bottom;
        self.measure_parameter.padding_start = padding_start;
        self.measure_parameter.padding_end = padding_end;
        self.measure_parameter.padding_top = padding_top;
        self.measure_parameter.padding_bottom = padding_bottom;
        &mut self.measure_parameter
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_on_attach(&mut self) -> &mut LinkedList<Box<dyn FnMut()>> {
        &mut self.on_attach
    }

    pub fn get_on_click(&mut self) -> Option<&mut Arc<Mutex<dyn FnMut(ClickSource)>>> {
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
    
    pub fn get_on_keyboard_input(&mut self) -> Option<&mut Box<dyn FnMut(&KeyboardInput) -> bool>> {
        self.on_keyboard_input.as_mut()
    }

    pub fn get_on_mouse_input(&mut self) -> Option<&mut Box<dyn FnMut(&MouseInput)>> {
        self.on_mouse_input.as_mut()
    }

    pub fn get_on_pointer_input(&mut self) -> Option<&mut Box<dyn FnMut(&PointerInput)>> {
        self.on_pointer_input.as_mut()
    }

    pub fn get_on_touch_input(&mut self) -> Option<&mut Box<dyn FnMut(&TouchInput)>> {
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
        let visible = self.visible.get();
        let margin_start = self.margin_start.get();
        let margin_end = self.margin_end.get();
        let margin_top = self.margin_top.get();
        let margin_bottom = self.margin_bottom.get();
        let padding_start = self.padding_start.get();
        let padding_end = self.padding_end.get();
        let padding_top = self.padding_top.get();
        let padding_bottom = self.padding_bottom.get();
        self.target_parameter.visible = visible;
        self.target_parameter.margin_start = margin_start;
        self.target_parameter.margin_end = margin_end;
        self.target_parameter.margin_top = margin_top;
        self.target_parameter.margin_bottom = margin_bottom;
        self.target_parameter.padding_start = padding_start;
        self.target_parameter.padding_end = padding_end;
        self.target_parameter.padding_top = padding_top;
        self.target_parameter.padding_bottom = padding_bottom;
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

    // pub fn clone_measure_parameter(&self) -> DisplayParameter {
    //     self.measure_parameter.clone()
    // }

    pub(crate) fn dispatch_animation(&mut self, animation: &dyn Animation, forced: bool) {

        let (animatable, children_force) = animation.animatable(self.id, forced);

        if animatable {
            if let Some(recorded_parameter) = self.recorded_parameter.clone() {
                let target_parameter = self.target_parameter.clone();
                override_animations!(
                    animation,
                    recorded_parameter,
                    target_parameter,
                    self,
                    relative_x,
                    relative_y,
                    width,
                    height,
                    offset_x,
                    offset_y,
                    opacity,
                    rotation,
                    rotation_center_x,
                    rotation_center_y,
                    scale_x,
                    scale_y,
                    scale_center_x,
                    scale_center_y,
                    skew_x,
                    skew_y,
                    skew_center_x,
                    skew_center_y
                );

                {
                    target_parameter
                        .float_params
                        .iter()
                        .for_each(|(key, end)| {
                            let target_changed =
                                if let Some((_, end, _)) = self.animations.float_params.get(key) {
                                    if let Some(target) = target_parameter.float_params.get(key) {
                                        target != end
                                    } else {
                                        true
                                    }
                                } else {
                                    true
                                };

                            if let Some(start) = recorded_parameter.float_params.get(key) {
                                if !f32_eq(*start, *end) && target_changed {
                                    self.animations.float_params.insert(
                                        key.clone(),
                                        (*start, *end, animation.clone_boxed()),
                                    );
                                }
                            } else if target_changed {
                                self.animations.float_params.insert(
                                    key.clone(),
                                    (0.0, *end, animation.clone_boxed()),
                                );
                            }
                        });
                }

                {
                    target_parameter
                        .color_params
                        .iter()
                        .for_each(|(key, end)| {
                            let target_changed =
                                if let Some((_, end, _)) = self.animations.color_params.get(key) {
                                    if let Some(target) = target_parameter.color_params.get(key) {
                                        target != end
                                    } else {
                                        true
                                    }
                                } else {
                                    true
                                };

                            if let Some(start) = recorded_parameter.color_params.get(key) {
                                if start!=end && target_changed {
                                    self.animations.color_params.insert(
                                        key.clone(),
                                        (*start, *end, animation.clone_boxed()),
                                    );
                                }
                            } else if target_changed {
                                self.animations.color_params.insert(
                                    key.clone(),
                                    (Color::TRANSPARENT, *end, animation.clone_boxed()),
                                );
                            }
                        });
                }
            }
        }

        self.children.lock().iter_mut().for_each(|child| {
            child.data().dispatch_animation(animation, children_force);
        });

        let background = self.get_background();
        let mut background_value = background.lock();
        if let Some(background) = background_value.as_mut() {
            background
                .data()
                .dispatch_animation(animation, children_force);
        }

        let foreground = self.get_foreground();
        let mut foreground_value = foreground.lock();
        if let Some(foreground) = foreground_value.as_mut() {
            foreground
                .data()
                .dispatch_animation(animation, children_force);
        }
    }

    pub fn dispatch_apply_theme(&mut self, theme: &Theme) {
        let f = self.get_dispatch_apply_theme();
        f.lock()(self, theme);
    }

    pub fn dispatch_draw(&mut self, surface: &mut Surface, parent_x: f32, parent_y: f32) {
        let f = self.get_dispatch_draw();
        f.lock()(self, surface, parent_x, parent_y);
    }

    pub fn dispatch_focus(&mut self) {
        let f = self.get_dispatch_focus();
        f.lock()(self);
    }

    pub fn dispatch_keyboard_input(&mut self, keyboard_input: &KeyboardInput) -> bool {
        let f = self.get_dispatch_keyboard_input();
        let r = f.lock()(self, keyboard_input);
        r
    }

    pub fn dispatch_layout(&mut self, relative_x: f32, relative_y: f32, width: f32, height: f32) {
        let f = self.get_dispatch_layout();
        f.lock()(self, relative_x, relative_y, width, height);
    }
    
    pub fn dispatch_modifiers(&mut self, modifiers: &Modifiers) {
        let f = self.get_dispatch_modifiers_changed();
        f.lock()(self, modifiers);
    }

    pub fn dispatch_mouse_input(&mut self, event: &MouseInput) {
        let f = self.get_dispatch_mouse_input();
        f.lock()(self, event);
    }

    pub fn dispatch_mouse_wheel_x(&mut self, event: &MouseWheel) -> bool {
        let f = self.get_dispatch_mouse_wheel_x();
        let r = f.lock()(self, event);
        r
    }

    pub fn dispatch_mouse_wheel_y(&mut self, event: &MouseWheel) -> bool {
        let f = self.get_dispatch_mouse_wheel_y();
        let r = f.lock()(self, event);
        r
    }


    pub fn dispatch_cursor_move(&mut self, cursor_move: &CursorMove) {
        let f = self.get_dispatch_cursor_move();
        f.lock()(self, cursor_move);
    }

    pub fn dispatch_measure(&mut self, max_width: f32, max_height: f32) {
        let width_mode = MeasureMode::from_size(self.get_width().get(), max_width);
        let height_mode = MeasureMode::from_size(self.get_height().get(), max_height);
        let f = self.get_measure();
        f.lock()(self, width_mode, height_mode);
    }

    pub fn dispatch_timer(&mut self, timer_id: usize) {
        let f = self.get_dispatch_timer();
        f.lock()(self, timer_id);
    }

    pub fn dispatch_touch_input(&mut self, event: &TouchInput) {
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
            for child in self.children.lock().iter() {
                child.data().find_item(id, f);
            }
        }
    }

    pub fn find_item_mut(&mut self, id: usize, f: &mut impl FnMut(&mut ItemData)) {
        if self.id == id {
            f(self);
        } else {
            for child in self.children.lock().iter_mut() {
                child.data().find_item_mut(id, f);
            }
        }
    }

    /// Returns true if it can focus next item.
    pub fn focus_next(&mut self) -> bool {
        let focus_next = self.focus_next.clone();
        let r = focus_next.lock()(self);
        r
    }

    pub fn for_each_child<F>(&self, mut f: F)
    where
        F: FnMut(&Item),
    {
        for child in self.children.lock().iter() {
            f(child);
        }
    }

    pub fn for_each_child_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Item),
    {
        for child in self.children.lock().iter_mut() {
            f(child);
        }
    }

    pub fn is_animating(&self) -> bool {
        self.animations.is_animating()
    }

    pub fn ime_input(&mut self, event: &ImeAction) {
        let f = self.get_ime_input();
        f.lock()(self, event);
    }

    pub fn layout(&mut self, width: f32, height: f32) {
        let f = self.get_layout();
        f.lock()(self, width, height);
    }

    fn layout_layer(layer: &SharedItem, width: f32, height: f32) {
        if let Some(item) = layer.lock().as_mut() {
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

    fn create_mode(size: Size, max_size: f32) -> MeasureMode {
        match size {
            Size::Auto => MeasureMode::Unspecified(max_size),
            Size::Fill => MeasureMode::Specified(max_size),
            Size::Fixed(size) => MeasureMode::Specified(size),
            Size::Relative(ratio) => MeasureMode::Specified(max_size * ratio),
        }
    }

    pub fn measure_children(&mut self, width_mode: MeasureMode, height_mode: MeasureMode) {
        let padding_horizontal = self.get_padding(Orientation::Horizontal);
        let padding_vertical = self.get_padding(Orientation::Vertical);
        let max_width = match width_mode {
            MeasureMode::Specified(width) => width - padding_horizontal,
            MeasureMode::Unspecified(width) => width - padding_horizontal,
        };
        let max_height = match height_mode {
            MeasureMode::Specified(height) => height - padding_vertical,
            MeasureMode::Unspecified(height) => height - padding_vertical,
        };

        self.for_each_child_mut(|child| {
            let child_width = child.data().get_width().get();
            let child_height = child.data().get_height().get();
            child.data().measure(
                Self::create_mode(child_width, max_width),
                Self::create_mode(child_height, max_height),
            );
        });
    }

    pub fn measure_child(
        &mut self,
        child: &Item,
        width_mode: MeasureMode,
        height_mode: MeasureMode,
    ) {
        let padding_horizontal = self.get_padding(Orientation::Horizontal);
        let padding_vertical = self.get_padding(Orientation::Vertical);
        let max_width = match width_mode {
            MeasureMode::Specified(width) => width - padding_horizontal,
            MeasureMode::Unspecified(width) => width - padding_horizontal,
        };
        let max_height = match height_mode {
            MeasureMode::Specified(height) => height - padding_vertical,
            MeasureMode::Unspecified(height) => height - padding_vertical,
        };

        let child_width = child.data().get_width().get();
        let child_height = child.data().get_height().get();
        let max_width = child.data().clamp_width(max_width);
        let max_height = child.data().clamp_height(max_height);
        child.data().measure(
            Self::create_mode(child_width, max_width),
            Self::create_mode(child_height, max_height),
        );
    }

    pub fn measure_child_by_specified(&mut self, child: &Item, width: f32, height: f32) {
        let padding_horizontal = self.get_padding(Orientation::Horizontal);
        let padding_vertical = self.get_padding(Orientation::Vertical);
        let max_width = width - padding_horizontal;
        let max_height = height - padding_vertical;

        let child_width = child.data().get_width().get();
        let child_height = child.data().get_height().get();
        let max_width = child.data().clamp_width(max_width);
        let max_height = child.data().clamp_height(max_height);
        child.data().measure(
            Self::create_mode(child_width, max_width),
            Self::create_mode(child_height, max_height),
        );
    }

    pub(crate) fn record_display_parameter(&mut self) {
        self.recorded_parameter = Some(self.get_display_parameter());
        self.children.lock().iter_mut().for_each(|child| {
            child.data().record_display_parameter();
        });

        let background = self.get_background();
        let mut background_value = background.lock();
        if let Some(item) = background_value.as_mut() {
            item.data().record_display_parameter();
        }

        let foreground = self.get_foreground();
        let mut foreground_value = foreground.lock();
        if let Some(item) = foreground_value.as_mut() {
            item.data().record_display_parameter()
        }
    }

    pub fn set_size<W, H>(&mut self, width: W, height: H) -> &mut Self
    where
        W: Into<SharedSize>,
        H: Into<SharedSize>,
    {
        self.set_width(width);
        self.set_height(height);
        self
    }

    pub fn set_base_line(&mut self, base_line: f32) {
        self.baseline = Some(base_line);
    }
}

fn f32_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.1
}

pub struct Item {
    data: Arc<Mutex<ItemData>>,
}

impl Item {
    pub fn new(window_context: &WindowContext, children: Children) -> Self {
        let data = ItemData::new(window_context, children);
        Self {
            data: Arc::new(Mutex::new(data)),
        }
    }

    pub fn data(&self) -> MutexGuard<ItemData> {
        self.data.lock()
    }
    
    pub fn data_clone(&self) -> Weak<Mutex<ItemData>> {
        Arc::downgrade(&self.data)
    }

    pub fn focused(self, focused: impl Into<Shared<bool>>) -> Self {
        self.data().set_focused(focused);
        self
    }

    pub fn margin(self, margin: impl Into<SharedF32>) -> Self {
        let margin = margin.into();
        self.data().set_margin_start(margin.clone());
        self.data().set_margin_end(margin.clone());
        self.data().set_margin_top(margin.clone());
        self.data().set_margin_bottom(margin);
        self
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
    
    pub fn on_keyboard_input<F>(self, f: F) -> Self
    where
        F: FnMut(&KeyboardInput) -> bool + 'static,
    {
        self.data().set_on_keyboard_input(f);
        self
    }

    pub fn on_mouse_input<F>(self, f: F) -> Self
    where
        F: FnMut(&MouseInput) + 'static,
    {
        self.data().set_on_mouse_input(f);
        self
    }

    pub fn on_pointer_input<F>(self, f: F) -> Self
    where
        F: FnMut(&PointerInput) + 'static,
    {
        self.data().set_on_pointer_input(f);
        self
    }

    pub fn on_touch_input<F>(self, f: F) -> Self
    where
        F: FnMut(&TouchInput) + 'static,
    {
        self.data().set_on_touch_input(f);
        self
    }

    pub fn padding(self, padding: impl Into<SharedF32>) -> Self {
        let padding = padding.into();
        self.data().set_padding_start(padding.clone());
        self.data().set_padding_end(padding.clone());
        self.data().set_padding_top(padding.clone());
        self.data().set_padding_bottom(padding);
        self
    }

    pub fn rotation_center(
        self,
        center_x: impl Into<SharedInnerPosition>,
        center_y: impl Into<SharedInnerPosition>,
    ) -> Self {
        self.data().set_rotation_center_x(center_x);
        self.data().set_rotation_center_y(center_y);
        self
    }

    pub fn scale(self, scale_x: impl Into<SharedF32>, scale_y: impl Into<SharedF32>) -> Self {
        self.data().set_scale_x(scale_x);
        self.data().set_scale_y(scale_y);
        self
    }

    pub fn size<W, H>(self, width: W, height: H) -> Self
    where
        W: Into<SharedSize>,
        H: Into<SharedSize>,
    {
        self.data().set_size(width, height);
        self
    }
}

impl Add<Item> for Item {
    type Output = Children;

    fn add(self, rhs: Item) -> Self::Output {
        Children::new() + self + rhs
    }
}
