use std::collections::{HashMap, LinkedList};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::core::{bind_str_to_id, generate_id, RefClone};
use crate::property::{Action, BoolProperty, Children, F32Property, Gettable, InnerPositionProperty, ItemProperty, Observable, Property, Settable, SizeProperty};
use skia_safe::{Canvas, Color};
use winit::event::{DeviceId, MouseButton};
use crate::OptionalInvoke;
use crate::ui::Animation;
use crate::ui::animation::Value;
use crate::ui::app::AppContext;
use crate::ui::item::{ClickSource, DisplayParameter, InnerPosition, ItemEvent, MeasureMode, MouseEvent, Orientation, PointerState, Size, TouchEvent};

macro_rules! impl_property {
    ($property_name:ident, $get_property_name:ident, $property_type:ty, $doc:expr) => {
        impl Item {
            #[doc=$doc]
            pub fn $property_name(mut self, $property_name: impl Into<$property_type>) -> Self {
                self.$property_name.remove_observer(self.id);
                let app_context = self.app_context.ref_clone();
                self.$property_name = $property_name.into();
                self.$property_name.add_observer(self.id, Box::new(move || {
                    app_context.request_re_layout();
                })).drop();
                self
            }
            
            pub fn $get_property_name(&self) -> $property_type {
                self.$property_name.ref_clone()
            }

        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gravity {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutDirection {
    LTR,
    RTL,
}

/*#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Padding {
    pub start: f32,
    pub top: f32,
    pub end: f32,
    pub bottom: f32,
}

impl Padding {
    pub fn new(start: f32, top: f32, end: f32, bottom: f32) -> Self {
        Self { start, top, end, bottom }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Margin {
    pub start: f32,
    pub top: f32,
    pub end: f32,
    pub bottom: f32,
}

impl Margin {
    pub fn new(start: f32, top: f32, end: f32, bottom: f32) -> Self {
        Self { start, top, end, bottom }
    }
}*/


#[derive(Default)]
struct AnimationItem{
    pub parent_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub parent_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub width: Option<((Value<f32>, Value<f32>), Animation)>,
    pub height: Option<((Value<f32>, Value<f32>), Animation)>,
    pub relative_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub relative_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub offset_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub offset_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub opacity: Option<((Value<f32>, Value<f32>), Animation)>,
    pub rotation: Option<((Value<f32>, Value<f32>), Animation)>,
    pub rotation_center_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub rotation_center_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub scale_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub scale_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub scale_center_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub scale_center_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub skew_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub skew_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub skew_center_x: Option<((Value<f32>, Value<f32>), Animation)>,
    pub skew_center_y: Option<((Value<f32>, Value<f32>), Animation)>,
    pub float_params: HashMap<String, Option<((Value<f32>, Value<f32>), Animation)>>,
    pub color_params: HashMap<String, Option<((Value<Color>, Value<Color>), Animation)>>,
}

/// An item is a basic building block of the UI system. It can be used to display text, images, or other content.
/// It can also be used to arrange other items in a layout.
pub struct Item {
    id: usize,
    name: String,
    app_context: AppContext,
    children: Children,
    item_event: ItemEvent,
    animation_item: AnimationItem,
    pub(crate) captured_mouse_button: Vec<MouseButton>,
    pub(crate) captured_touch_id: Vec<(DeviceId, u64)>,
    pub(crate) on_attach: LinkedList<Box<dyn FnMut()>>,
    pub(crate) on_detach: LinkedList<Box<dyn FnMut()>>,
    click_source: Option<ClickSource>,
    on_click: Option<Box<dyn FnMut(ClickSource)>>,
    on_mouse_input: Option<Box<dyn FnMut(MouseEvent)>>,
    on_touch: Option<Box<dyn FnMut(TouchEvent)>>,
    touch_start_time: Instant,
    recorded_parameter: Option<DisplayParameter>,
    target_parameter: DisplayParameter,
    display_parameter: DisplayParameter,
    display_parameter_out: Property<DisplayParameter>,
    measure_parameter: DisplayParameter,
    active: BoolProperty,
    layout_direction: Property<LayoutDirection>,
    width: SizeProperty,
    min_width: F32Property,
    max_width: F32Property,
    height: SizeProperty,
    min_height: F32Property,
    max_height: F32Property,
    padding_start: F32Property,
    padding_top: F32Property,
    padding_end: F32Property,
    padding_bottom: F32Property,
    margin_start: F32Property,
    margin_top: F32Property,
    margin_end: F32Property,
    margin_bottom: F32Property,
    scale_x: F32Property,
    scale_y: F32Property,
    scale_center_x: InnerPositionProperty,
    scale_center_y: InnerPositionProperty,
    offset_x: F32Property,
    offset_y: F32Property,
    opacity: F32Property,
    rotation: F32Property,
    rotation_center_x: InnerPositionProperty,
    rotation_center_y: InnerPositionProperty,
    skew_x: F32Property,
    skew_y: F32Property,
    skew_center_x: InnerPositionProperty,
    skew_center_y: InnerPositionProperty,
    background: ItemProperty,
    foreground: ItemProperty,
    horizontal_gravity: Property<Gravity>,
    vertical_gravity: Property<Gravity>,
}


impl_property!(active, get_active, BoolProperty,
    "Whether the item is active and can receive input events.");
impl_property!(layout_direction, get_layout_direction, Property<LayoutDirection>,
    "The layout direction of the item.");
impl_property!(width, get_width, SizeProperty,
    "The width of the item. See [`Size`](crate::ui::item::Size) for more information.");
impl_property!(min_width, get_min_width, F32Property,
    "The minimum width of the item.");
impl_property!(max_width, get_max_width, F32Property,
    "The maximum width of the item.");
impl_property!(height, get_height, SizeProperty,
    "The height of the item. See [`Size`](crate::ui::item::Size) for more information.");
impl_property!(min_height, get_min_height, F32Property,
    "The minimum height of the item.");
impl_property!(max_height, get_max_height, F32Property,
    "The maximum height of the item.");
impl_property!(padding_start, get_padding_start, F32Property,
    "The padding at the start of the item. The \"start\" direction depends on the layout direction.");
impl_property!(padding_top, get_padding_top, F32Property,
    "The padding at the top of the item.");
impl_property!(padding_end, get_padding_end, F32Property,
    "The padding at the end of the item. The \"end\" direction depends on the layout direction.");
impl_property!(padding_bottom, get_padding_bottom, F32Property,
    "The padding at the bottom of the item.");
impl_property!(margin_start, get_margin_start, F32Property,
    "The margin at the start of the item. The \"start\" direction depends on the layout direction.");
impl_property!(margin_top, get_margin_top, F32Property,
    "The margin at the top of the item.");
impl_property!(margin_end, get_margin_end, F32Property,
    "The margin at the end of the item. The \"end\" direction depends on the layout direction.");
impl_property!(margin_bottom, get_margin_bottom, F32Property,
    "The margin at the bottom of the item.");
impl_property!(scale_x, get_scale_x, F32Property,
    "The scale in the x direction.");
impl_property!(scale_y, get_scale_y, F32Property,
    "The scale in the y direction.");
impl_property!(scale_center_x, get_scale_center_x, InnerPositionProperty,
    "The center of scaling in the x direction.");
impl_property!(scale_center_y, get_scale_center_y, InnerPositionProperty,
    "The center of scaling in the y direction.");
impl_property!(offset_x, get_offset_x, F32Property,
    "The offset in the x direction relative to the original position.");
impl_property!(offset_y, get_offset_y, F32Property,
    "The offset in the y direction relative to the original position.");
impl_property!(opacity, get_opacity, F32Property,
    "The opacity of the item. It will also affect the opacity of its children.");
impl_property!(rotation, get_rotation, F32Property,
    "The rotation of the item in degrees.");
impl_property!(rotation_center_x, get_rotation_center_x, InnerPositionProperty,
    "The center of rotation in the x direction.");
impl_property!(rotation_center_y, get_rotation_center_y, InnerPositionProperty,
    "The center of rotation in the y direction.");
impl_property!(skew_x, get_skew_x, F32Property,
    "The skew in the x direction in degrees.");
impl_property!(skew_y, get_skew_y, F32Property,
    "The skew in the y direction in degrees.");
impl_property!(skew_center_x, get_skew_center_x, InnerPositionProperty,
    "The center of skew in the x direction.");
impl_property!(skew_center_y, get_skew_center_y, InnerPositionProperty,
    "The center of skew in the y direction.");
impl_property!(background, get_background, ItemProperty,
    "The background of the item. It will be drawn behind the content (including children)");
impl_property!(foreground, get_foreground, ItemProperty,
    "The foreground of the item. It will be drawn in front of the content (including children)");
impl_property!(horizontal_gravity, get_horizontal_gravity, Property<Gravity>,
    "The horizontal gravity of the item. It determines how the item is positioned horizontally within its parent.");
impl_property!(vertical_gravity, get_vertical_gravity, Property<Gravity>,
    "The vertical gravity of the item. It determines how the item is positioned vertically within its parent.");
/*
pub struct DisplayParameter {
    parent_x: f32,
    parent_y: f32,
    width: f32,
    height: f32,
    relative_x: f32,
    relative_y: f32,
    offset_x: f32,
    offset_y: f32,
    opacity: f32,
    rotation: f32,
    rotation_center_x: f32,
    rotation_center_y: f32,
    scale_x: f32,
    scale_y: f32,
    scale_center_x: f32,
    scale_center_y: f32,
    skew_x: f32,
    skew_y: f32,
    skew_center_x: f32,
    skew_center_y: f32,
    float_params: HashMap<String, f32>,
    color_params: HashMap<String, Color>,
}
*/

impl Item {
    pub fn new(app_context: AppContext, children: Children, item_event: ItemEvent) -> Self {
        let id = generate_id();

        Self {
            id,
            name: format!("Item {}", id),
            app_context,
            children,
            item_event,
            animation_item: AnimationItem::default(),
            captured_mouse_button: vec![],
            captured_touch_id: vec![],
            on_attach: LinkedList::new(),
            on_detach: LinkedList::new(),
            click_source: None,
            on_click: None,
            on_mouse_input: None,
            on_touch: None,
            touch_start_time: Instant::now(),
            recorded_parameter: None,
            target_parameter: DisplayParameter::default(),
            display_parameter: Default::default(),
            display_parameter_out: Property::from_static(Default::default()),
            measure_parameter: Default::default(),
            active: true.into(),
            layout_direction: LayoutDirection::LTR.into(),
            width: Size::Compact.into(),
            min_width: 0.0.into(),
            max_width: f32::INFINITY.into(),
            height: Size::Compact.into(),
            min_height: 0.0.into(),
            max_height: f32::INFINITY.into(),
            padding_start: 0.0.into(),
            padding_top: 0.0.into(),
            padding_end: 0.0.into(),
            padding_bottom: 0.0.into(),
            margin_start: 0.0.into(),
            margin_top: 0.0.into(),
            margin_end: 0.0.into(),
            margin_bottom: 0.0.into(),
            scale_x: 1.0.into(),
            scale_y: 1.0.into(),
            scale_center_x: InnerPosition::default().into(),
            scale_center_y: InnerPosition::default().into(),
            offset_x: 0.0.into(),
            offset_y: 0.0.into(),
            opacity: 1.0.into(),
            rotation: 0.0.into(),
            rotation_center_x: InnerPosition::default().into(),
            rotation_center_y: InnerPosition::default().into(),
            skew_x: 0.0.into(),
            skew_y: 0.0.into(),
            skew_center_x: InnerPosition::default().into(),
            skew_center_y: InnerPosition::default().into(),
            background: ItemProperty::none(),
            foreground: ItemProperty::none(),
            horizontal_gravity: Gravity::Start.into(),
            vertical_gravity: Gravity::Start.into(),
        }
    }

    pub fn get_app_context(&self) -> AppContext {
        self.app_context.ref_clone()
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        bind_str_to_id(&self.name, self.id);
        self
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

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

    pub fn on_click<F>(mut self, f: F) -> Self
    where
        F: FnMut(ClickSource) + 'static,
    {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn on_mouse_input<F>(mut self, f: F) -> Self
    where
        F: FnMut(MouseEvent) + 'static,
    {
        self.on_mouse_input = Some(Box::new(f));
        self
    }

    pub fn display_parameter_out(mut self, display_parameter_out: Property<DisplayParameter>) -> Self {
        self.display_parameter_out.remove_observer(self.id);
        self.display_parameter_out = display_parameter_out;
        self.display_parameter_out.set_static(self.display_parameter.clone());
        self
    }
    
    pub fn get_display_parameter(&mut self) -> &mut DisplayParameter {
        &mut self.display_parameter
    }

    pub fn set_target_parameter(&mut self, mut parameter: DisplayParameter) {
        parameter.set_parent_position(self.display_parameter.parent_x(), self.display_parameter.parent_y());
        self.display_parameter = parameter;
    }

    pub fn get_measure_parameter(&mut self) -> &mut DisplayParameter {
        &mut self.measure_parameter
    }

    pub fn set_measure_parameter(&mut self, parameter: DisplayParameter) {
        self.measure_parameter = parameter;
    }

    pub fn get_size(&self, orientation: Orientation) -> Size {
        match orientation {
            Orientation::Horizontal => self.width.get(),
            Orientation::Vertical => self.height.get(),
        }
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

    pub fn get_padding(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.padding_start.get() + self.padding_end.get(),
            Orientation::Vertical => self.padding_top.get() + self.padding_bottom.get(),
        }
    }

    pub fn get_padding_left(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => {
                self.padding_start.get()
            }
            LayoutDirection::RTL => {
                self.padding_end.get()
            }
        }
    }
    
    pub fn get_padding_right(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => {
                self.padding_end.get()
            }
            LayoutDirection::RTL => {
                self.padding_start.get()
            }
        }
    }
    
    pub fn get_margin_left(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => {
                self.margin_start.get()
            }
            LayoutDirection::RTL => {
                self.margin_end.get()
            }
        }
    }
    
    pub fn get_margin_right(&self) -> f32 {
        match self.layout_direction.get() {
            LayoutDirection::LTR => {
                self.margin_end.get()
            }
            LayoutDirection::RTL => {
                self.margin_start.get()
            }
        }
    }

    pub fn get_margin(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.margin_start.get() + self.margin_end.get(),
            Orientation::Vertical => self.margin_top.get() + self.margin_bottom.get(),
        }
    }

    pub fn get_children(&self) -> &Children {
        &self.children
    }

    pub fn get_children_mut(&mut self) -> &mut Children {
        &mut self.children
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

    fn layout_layer(layer: ItemProperty, width: f32, height: f32) {
        layer.value().as_mut().if_some(|item| {
            item.measure(Orientation::Horizontal, MeasureMode::Specified(width));
            item.measure(Orientation::Vertical, MeasureMode::Specified(height));
            item.dispatch_layout(0.0, 0.0, width, height);
        });
    }

    pub fn layout_layers(&self, width: f32, height: f32) {
        Self::layout_layer(self.get_background(), width, height);
        Self::layout_layer(self.get_foreground(), width, height);
    }

    pub(crate) fn record_display_parameter(&mut self) {
        self.recorded_parameter = Some(self.display_parameter.clone());
        self.children.items().iter_mut().for_each(|child| {
            child.record_display_parameter();
        });
    }

    pub(crate) fn dispatch_animation(&mut self, animation: Animation) {
        
    }

    pub fn dispatch_draw(&mut self, canvas: &Canvas, parent_x: f32, parent_y: f32) {
        let f = self.item_event.dispatch_draw.clone();
        f.lock().unwrap()(self, canvas, parent_x, parent_y);
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        let f = self.item_event.draw.clone();
        f.lock().unwrap()(self, canvas);
    }

    pub fn measure(&mut self, orientation: Orientation, measure_mode: MeasureMode) {
        let f = self.item_event.measure.clone();
        f.lock().unwrap()(self, orientation, measure_mode);
    }

    pub fn measure_children(&mut self, orientation: Orientation, measure_mode: MeasureMode) {
        let max = measure_mode.value();
        self.for_each_child_mut(|child| {
            match child.get_size(orientation) {
                Size::Compact => child.measure(orientation, MeasureMode::Unspecified(max)),
                Size::Expanded => child.measure(orientation, MeasureMode::Specified(max)),
                Size::Fixed(size) => child.measure(orientation, MeasureMode::Specified(size)),
                _ => {}
            }
        });
    }
    
    pub fn dispatch_layout(&mut self, relative_x: f32, relative_y: f32, width: f32, height: f32) {
        let f = self.item_event.dispatch_layout.clone();
        f.lock().unwrap()(self, relative_x, relative_y, width, height);
    }

    pub fn layout(&mut self, relative_x: f32, relative_y: f32, width: f32, height: f32) {
        let f = self.item_event.layout.clone();
        f.lock().unwrap()(self, relative_x, relative_y, width, height);
    }

    pub fn mouse_input(&mut self, event: MouseEvent) {
        let x = event.x;
        let y = event.y;

        {
            let foreground = self.get_foreground();
            foreground.value().as_mut().if_some(|foreground| {
                foreground.mouse_input(event);
            });

            let background = self.get_background();
            background.value().as_mut().if_some(|background| {
                background.mouse_input(event);
            });
        }

        if let Some(on_mouse_input) = &mut self.on_mouse_input {
            on_mouse_input(event);
        }

        {
            let children = self.get_children();
            for child in children.items().iter_mut().rev() {
                let display_parameter = child.get_display_parameter();
                match event.pointer_state {
                    PointerState::Started => {
                        if display_parameter.is_inside(x, y) {
                            child.captured_mouse_button.push(event.button);
                            child.mouse_input(event);
                            return;
                        }
                    }
                    PointerState::Moved => {
                        if child.captured_mouse_button.contains(&event.button) {
                            child.mouse_input(event);
                            return;
                        }
                    }
                    PointerState::Ended | PointerState::Canceled => {
                        if child.captured_mouse_button.contains(&event.button) {
                            child.captured_mouse_button.retain(|&button| button != event.button);
                            child.mouse_input(event);
                            return;
                        }
                    }
                }
            }
        }

        match event.pointer_state {
            PointerState::Started => {
                self.click_source = Some(ClickSource::Mouse(event.button));
            }
            PointerState::Ended => {
                if self.get_display_parameter().is_inside(x, y) {
                    if let Some(click_source) = self.click_source {
                        if click_source == ClickSource::Mouse(event.button) {
                            if let Some(on_click) = &mut self.on_click {
                                on_click(click_source);
                            }
                        }
                    }
                    self.click_source = None;
                }
            }
            _ => {}
        }
    }

    pub fn touch_input(&mut self, event: TouchEvent) {
        let x = event.x;
        let y = event.y;

        {
            let foreground = self.get_foreground();
            foreground.value().as_mut().if_some(|foreground| {
                foreground.touch_input(event);
            });

            let background = self.get_background();
            background.value().as_mut().if_some(|background| {
                background.touch_input(event);
            });
        }

        if let Some(on_touch) = &mut self.on_touch {
            on_touch(event);
        }

        {
            let children = self.get_children();
            for child in children.items().iter_mut().rev() {
                let display_parameter = child.get_display_parameter();
                match event.pointer_state {
                    PointerState::Started => {
                        if display_parameter.is_inside(x, y) {
                            child.captured_touch_id.push((event.device_id, event.id));
                            child.touch_input(event);
                            return;
                        }
                    }
                    PointerState::Moved => {
                        if child.captured_touch_id.contains(&(event.device_id, event.id)) {
                            child.touch_input(event);
                            return;
                        }
                    }
                    PointerState::Ended | PointerState::Canceled => {
                        if child.captured_touch_id.contains(&(event.device_id, event.id)) {
                            child.captured_touch_id.retain(|&(device_id, id)| {
                                device_id != event.device_id || id != event.id
                            });
                            child.touch_input(event);
                            return;
                        }
                    }
                }
            }
        }

        match event.pointer_state {
            PointerState::Started => {
                self.click_source = Some(ClickSource::Touch);
                self.touch_start_time = Instant::now();
            }
            PointerState::Ended => {
                if let Some(click_source) = self.click_source {
                    if click_source == ClickSource::Touch {
                        if let Some(on_click) = &mut self.on_click {
                            if self.touch_start_time.elapsed().as_millis() < 300 {
                                on_click(click_source);
                            } else {
                                on_click(ClickSource::LongTouch);
                            }
                        }
                    }
                }
                self.click_source = None;
            }
            _ => {}
        }
    }
}