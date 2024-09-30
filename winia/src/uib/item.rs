use std::collections::HashMap;
use std::ops::Add;
use std::rc::Weak;
use std::sync::Mutex;

use skia_safe::Canvas;

use crate::{children, impl_item_property};
use crate::app::SharedApp;
use crate::property::{BoolProperty, FloatProperty, Gettable, GravityProperty, ItemProperty, Observable, ObservableProperty, Size, SizeProperty};
use crate::uib::{AdditionalProperty, Children, ChildrenManager, DisplayParameter, Gravity, ImeAction, ItemEvent, LayoutDirection, MeasureMode, Orientation, Pointer, PointerEvent};
use crate::uib::display_parameter::AnimationDisplayParameter;
use crate::uib::mouse_event::MouseEvent;

pub struct Item {
    parent_children_manager: Option<ChildrenManager>,
    app: SharedApp,
    name: String,
    children: Children,
    active: BoolProperty,
    width: SizeProperty,
    height: SizeProperty,
    layout_direction: ObservableProperty<LayoutDirection>,
    horizontal_gravity: GravityProperty,
    vertical_gravity: GravityProperty,
    focusable: BoolProperty,
    focused: BoolProperty,
    focusable_when_clicked: BoolProperty,
    min_width: FloatProperty,
    min_height: FloatProperty,
    max_width: FloatProperty,
    max_height: FloatProperty,
    padding_start: FloatProperty,
    padding_top: FloatProperty,
    padding_end: FloatProperty,
    padding_bottom: FloatProperty,
    margin_start: FloatProperty,
    margin_top: FloatProperty,
    margin_end: FloatProperty,
    margin_bottom: FloatProperty,
    offset_x: FloatProperty,
    offset_y: FloatProperty,
    record_display_parameter: Option<DisplayParameter>,
    animation_start_display_parameter: Option<AnimationDisplayParameter>,
    display_parameter: DisplayParameter,
    background: ItemProperty,
    foreground: ItemProperty,
    enable_clipping: BoolProperty,
    additional_properties: HashMap<String, AdditionalProperty>,
    on_click: Option<Box<dyn FnMut(Pointer)>>,
    on_focus: Option<Box<dyn Fn()>>,
    on_cursor_entered: Box<dyn Fn()>,
    on_cursor_exited: Box<dyn Fn()>,
    item_event: ItemEvent,
    pressed_pointers: Vec<Pointer>,
}


impl_item_property!(Item, active, get_active, BoolProperty);
impl_item_property!(Item, width, get_width, SizeProperty);
impl_item_property!(Item, height, get_height, SizeProperty);
impl_item_property!(Item, layout_direction, get_layout_direction, ObservableProperty<LayoutDirection>);
impl_item_property!(Item, horizontal_gravity, get_horizontal_gravity, GravityProperty);
impl_item_property!(Item, vertical_gravity, get_vertical_gravity, GravityProperty);
impl_item_property!(Item, focusable, get_focusable, BoolProperty);
impl_item_property!(Item, focusable_when_clicked, get_focusable_when_clicked, BoolProperty);
impl_item_property!(Item, min_width, get_min_width, FloatProperty);
impl_item_property!(Item, min_height, get_min_height, FloatProperty);
impl_item_property!(Item, max_width, get_max_width, FloatProperty);
impl_item_property!(Item, max_height, get_max_height, FloatProperty);
impl_item_property!(Item, padding_start, get_padding_start, FloatProperty);
impl_item_property!(Item, padding_top, get_padding_top, FloatProperty);
impl_item_property!(Item, padding_end, get_padding_end, FloatProperty);
impl_item_property!(Item, padding_bottom, get_padding_bottom, FloatProperty);
impl_item_property!(Item, margin_start, get_margin_start, FloatProperty);
impl_item_property!(Item, margin_top, get_margin_top, FloatProperty);
impl_item_property!(Item, margin_end, get_margin_end, FloatProperty);
impl_item_property!(Item, margin_bottom, get_margin_bottom, FloatProperty);
impl_item_property!(Item, offset_x, get_offset_x, FloatProperty);
impl_item_property!(Item, offset_y, get_offset_y, FloatProperty);
impl_item_property!(Item, background, get_background, ItemProperty);
impl_item_property!(Item, foreground, get_foreground, ItemProperty);
impl_item_property!(Item, enable_clipping, get_enable_clipping, BoolProperty);


impl Item {
    pub fn new(app: SharedApp, item_events: ItemEvent) -> Self {
        let layout_direction = app.layout_direction();
        Item {
            parent_children_manager: None,
            app,
            name: String::new(),
            children: children!(),
            active: true.into(),
            width: Size::Default.into(),
            height: Size::Default.into(),
            layout_direction: layout_direction.into(),
            horizontal_gravity: Gravity::Start.into(),
            vertical_gravity: Gravity::Start.into(),
            focusable: true.into(),
            focused: false.into(),
            focusable_when_clicked: true.into(),
            min_width: 0.into(),
            min_height: 0.into(),
            max_width: FloatProperty::from_value(f32::MAX),
            max_height: FloatProperty::from_value(f32::MAX),
            padding_start: 0.into(),
            padding_top: 0.into(),
            padding_end: 0.into(),
            padding_bottom: 0.into(),
            margin_start: 0.into(),
            margin_top: 0.into(),
            margin_end: 0.into(),
            margin_bottom: 0.into(),
            offset_x: 0.into(),
            offset_y: 0.into(),
            record_display_parameter: None,
            animation_start_display_parameter: None,
            display_parameter: DisplayParameter::default(),
            background: None.into(),
            foreground: None.into(),
            enable_clipping: false.into(),
            additional_properties: HashMap::new(),
            on_click: None,
            on_focus: None,
            on_cursor_entered: Box::new(|| {}),
            on_cursor_exited: Box::new(|| {}),
            item_event: item_events,
            pressed_pointers: vec![],
        }
    }

    pub fn set_parent_children_manager(&mut self, children_manager: ChildrenManager) {
        self.parent_children_manager = Some(children_manager);
    }

    pub fn attach(self) {
        if let Some(mut parent_children_manager) = self.parent_children_manager.clone() {
            parent_children_manager.add(self);
        }
    }

    pub fn get_app(&self) -> SharedApp {
        self.app.clone()
    }

    pub fn get_id(&self) -> usize {
        self as *const Item as usize
    }

    pub fn get_tag(&self) -> &str {
        &self.name
    }

    pub fn name(mut self, tag: impl Into<String>) -> Self {
        self.name = tag.into();
        self
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn set_children(&mut self, children: Children) {
        self.children = children;
    }

    pub fn get_children(&self) -> Children {
        self.children.clone()
    }

    pub fn add_child(mut self, child: Item) -> Self {
        self.children.add_child(child);
        self
    }

    pub fn find_item(&self, id: usize, f: Box<impl FnOnce(&Item)>) -> Option<Box<impl FnOnce(&Item)>> {
        let mut f = f;
        if self.get_id() == id {
            f(self);
            return None;
        }
        for child in self.get_children().lock().iter() {
            if let Some(ff) = child.find_item(id, f) {
                f = ff;
            } else {
                return None;
            }
        }
        Some(f)
    }

    pub fn set_additional_property(&mut self, key: impl Into<String>, value: impl Into<AdditionalProperty>) {
        self.additional_properties.insert(key.into(), value.into());
    }

    pub fn get_additional_property(&self, key: impl Into<String>) -> Option<&AdditionalProperty> {
        self.additional_properties.get(&key.into())
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        let layout_params = self.get_display_parameter();
        let content_width = self.app.content_width();
        let content_height = self.app.content_height();
        if layout_params.x() + layout_params.width < 0.0 || layout_params.x() > content_width || layout_params.y() + layout_params.height < 0.0 || layout_params.y() > content_height {
            return;
        }
        self.item_event.draw_event.clone().lock().unwrap()(self, canvas);
    }

    pub fn on_draw(&mut self, canvas: &Canvas) {
        self.item_event.on_draw.clone().lock().unwrap()(self, canvas);
    }

    pub fn measure(&mut self, width_measure_mode: MeasureMode, height_measure_mode: MeasureMode) {
        self.item_event.measure_event.clone().lock().unwrap()(self, width_measure_mode, height_measure_mode);
    }

    pub fn layout(&mut self, x: f32, y: f32) {
        self.item_event.layout_event.clone().lock().unwrap()(self, x, y);
    }

    pub fn focused(mut self, focused: impl Into<BoolProperty>) -> Self {
        self.focused = focused.into();
        let mut app = self.app.clone();
        let mut on_focus = self.item_event.on_focused.clone();
        let weak_focused = self.focused.property_weak();
        self.focused.add_observer(move || {
            on_focus.get_events().iter_mut().for_each(|event| {
                if let Some(focused) = weak_focused.upgrade() {
                    event(*focused.lock().unwrap().value());
                }
            });
            app.request_layout();
        },self.get_id());
        self
    }

    pub fn focus(&mut self, focused: bool) {
        self.focused.set_value(focused);
    }

    pub fn orientation_changed(&mut self, orientation: Orientation) {
        self.item_event.orientation_changed_event.clone().lock().unwrap()(self, orientation);
    }
    pub fn on_orientation_changed(mut self, on_orientation_changed: impl Fn(Orientation) + 'static) -> Self {
        self.item_event.on_orientation_changed.clone().add_event(Box::new(on_orientation_changed));
        self
    }

    pub fn invoke_on_orientation_changed(&mut self, orientation: Orientation) {
        for event in self.item_event.on_orientation_changed.clone().get_events().iter_mut() {
            event(orientation);
        }
    }

    pub fn mouse_input(&mut self, mouse_event: MouseEvent) -> bool {
        self.item_event.mouse_input_event.clone().lock().unwrap()(self, mouse_event)
    }

    pub fn on_mouse_input(self, mouse_input_event: impl FnMut(&mut Item, MouseEvent) -> bool + 'static) -> Self {
        self.item_event.on_mouse_input.clone().add_event(Box::new(mouse_input_event));
        self
    }

    pub fn on_pointer_input(self, pointer_input_event: impl FnMut(&mut Item, PointerEvent) -> bool + 'static) -> Self {
        self.item_event.on_pointer_input.clone().add_event(Box::new(pointer_input_event));
        self
    }

    pub fn invoke_on_mouse_input(&mut self, mouse_event: MouseEvent) -> bool {
        for event in self.item_event.on_mouse_input.clone().get_events().iter_mut() {
            if event(self, mouse_event) {
                return true;
            }
        }
        false
    }


    pub fn invoke_on_pointer_input(&mut self, pointer_event: PointerEvent) -> bool {
        for event in self.item_event.on_pointer_input.clone().get_events().iter_mut() {
            if event(self, pointer_event) {
                return true;
            }
        }
        false
    }

    pub fn ime_input(&mut self, ime_action: ImeAction) {
        self.item_event.ime_input_event.clone().lock().unwrap()(self, ime_action);
    }

    pub fn invoke_on_ime_input(&mut self, ime_action:ImeAction){
        self.item_event.on_ime_input.clone().lock().unwrap()(self, ime_action);
    }
    pub fn get_pressed_pointers(&self) -> &Vec<Pointer> {
        &self.pressed_pointers
    }

    pub fn get_pressed_pointers_mut(&mut self) -> &mut Vec<Pointer> {
        &mut self.pressed_pointers
    }
    

    pub fn get_display_parameter(&self) -> DisplayParameter {
        self.display_parameter.clone()
    }

    pub fn get_display_parameter_mut(&mut self) -> &mut DisplayParameter {
        &mut self.display_parameter
    }
                                                
    pub fn set_display_parameter(&mut self, layout_params: impl Into<DisplayParameter>) {
        self.display_parameter = layout_params.into();
    }
    

    pub fn init_display_parameter(&mut self) {
        self.display_parameter.padding_start = self.padding_start.get();
        self.display_parameter.padding_top = self.padding_top.get();
        self.display_parameter.padding_end = self.padding_end.get();
        self.display_parameter.padding_bottom = self.padding_bottom.get();
        self.display_parameter.margin_start = self.margin_start.get();
        self.display_parameter.margin_top = self.margin_top.get();
        self.display_parameter.margin_end = self.margin_end.get();
        self.display_parameter.margin_bottom = self.margin_bottom.get();
        self.display_parameter.offset_x = self.offset_x.get();
        self.display_parameter.offset_y = self.offset_y.get();
        self.display_parameter.max_width = self.max_width.get();
        self.display_parameter.max_height = self.max_height.get();
        self.display_parameter.min_width = self.min_width.get();
        self.display_parameter.min_height = self.min_height.get();
    }

    pub fn on_click<F>(mut self, on_click: F) -> Self
        where F: FnMut(Pointer) + 'static
    {
        self.on_click = Some(Box::new(on_click));
        self
    }

    pub fn get_on_click(&mut self) -> Option<&mut Box<dyn FnMut(Pointer)>> {
        self.on_click.as_mut()
    }

    pub fn on_cursor_entered(mut self, on_cursor_entered: impl Fn() + 'static) -> Self {
        self.on_cursor_entered = Box::new(on_cursor_entered);
        self
    }

    pub fn on_cursor_exited(mut self, on_cursor_exited: impl Fn() + 'static) -> Self {
        self.on_cursor_exited = Box::new(on_cursor_exited);
        self
    }

    pub fn gravity(mut self, gravity: impl Into<(GravityProperty, GravityProperty)>) -> Self {
        let (horizontal_gravity, vertical_gravity) = gravity.into();
        self.horizontal_gravity = horizontal_gravity;
        self.vertical_gravity = vertical_gravity;
        {
            let app = self.app.clone();
            self.horizontal_gravity.add_observer(
                move || {
                    app.request_layout();
                },
                self.get_id(),
            );
        }

        {
            let app = self.app.clone();
            self.vertical_gravity.add_observer(
                move || {
                    app.request_layout();
                },
                self.get_id(),
            );
        }

        self
    }

    pub fn get_focused(&self) -> BoolProperty {
        self.focused.clone()
    }

    pub fn children(self) -> Box<dyn FnOnce(Children) -> Item> {
        Box::new(move |children| {
            let mut item = self;
            item.set_children(children);
            item
        })
    }
}

impl Into<(GravityProperty, GravityProperty)> for &ObservableProperty<Gravity> {
    fn into(self) -> (GravityProperty, GravityProperty) {
        let horizontal_gravity = self.clone();
        let vertical_gravity = self.clone();
        (horizontal_gravity.into(), vertical_gravity.into())
    }
}

impl Add<Item> for Children {
    type Output = Children;

    fn add(mut self, rhs: Item) -> Self::Output {
        self.add_child(rhs);
        self
    }
}


macro_rules! impl_into_additional_property {
    ($t:ty, $variant:ident) => {
        impl From<$t> for AdditionalProperty {
            fn from(value: $t) -> Self {
                AdditionalProperty::$variant(value)
            }
        }
    };
}

impl_into_additional_property!(i8, I8);
impl_into_additional_property!(i16, I16);
impl_into_additional_property!(i32, I32);
impl_into_additional_property!(i64, I64);
impl_into_additional_property!(u8, U8);
impl_into_additional_property!(u16, U16);
impl_into_additional_property!(f32, F32);
impl_into_additional_property!(f64, F64);
impl_into_additional_property!(bool, Bool);
impl_into_additional_property!(String, String);