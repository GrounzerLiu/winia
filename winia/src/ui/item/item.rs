use crate::core::{bind_str_to_id, generate_id, RefClone};
use crate::shared::{
    Children, Gettable, Observable, Settable, Shared, SharedBool, SharedColor, SharedF32,
    SharedInnerPosition, SharedItem, SharedSize, SharedUsize,
};
use crate::ui::app::{AppContext, UserEvent};
use crate::ui::item::{
    ClickSource, DisplayParameter, ImeAction, InnerPosition, ItemEvent, MeasureMode, MouseEvent,
    Orientation, PointerState, Size, TouchEvent,
};
use crate::ui::Animation;
use crate::OptionalInvoke;
use skia_safe::{Canvas, Color, Surface};
use std::any::Any;
use std::collections::{HashMap, LinkedList};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::event::{DeviceId, KeyEvent, MouseButton};

pub fn init_property_re_layout<T>(app_context: AppContext, property: &mut Shared<T>, id: usize) {
    property
        .add_observer(
            id,
            Box::new(move || {
                app_context.request_re_layout();
            }),
        )
        .drop();
}

pub fn init_property_redraw<T>(app_context: AppContext, property: &mut Shared<T>, id: usize) {
    property
        .add_observer(
            id,
            Box::new(move || {
                app_context.request_redraw();
            }),
        )
        .drop();
}

macro_rules! impl_property_re_layout {
    ($property_name:ident, $get_property_name:ident, $property_type:ty, $doc:expr) => {
        impl Item {
            #[doc=$doc]
            pub fn $property_name(mut self, $property_name: impl Into<$property_type>) -> Self {
                self.$property_name.remove_observer(self.id);
                // let app_context = self.app_context.ref_clone();
                self.$property_name = $property_name.into();
                // self.$property_name.add_observer(self.id, Box::new(move || {
                //     app_context.request_re_layout();
                // })).drop();
                init_property_re_layout(
                    self.app_context.ref_clone(),
                    &mut self.$property_name,
                    self.id,
                );
                self
            }

            pub fn $get_property_name(&self) -> $property_type {
                self.$property_name.ref_clone()
            }
        }
    };
}

macro_rules! impl_property_redraw {
    ($property_name:ident, $get_property_name:ident, $property_type:ty, $doc:expr) => {
        impl Item {
            #[doc=$doc]
            pub fn $property_name(mut self, $property_name: impl Into<$property_type>) -> Self {
                self.$property_name.remove_observer(self.id);
                // let app_context = self.app_context.ref_clone();
                self.$property_name = $property_name.into();
                // self.$property_name.add_observer(self.id, Box::new(move || {
                //     app_context.request_redraw();
                // })).drop();
                init_property_redraw(
                    self.app_context.ref_clone(),
                    &mut self.$property_name,
                    self.id,
                );
                self
            }

            pub fn $get_property_name(&self) -> $property_type {
                self.$property_name.ref_clone()
            }
        }
    };
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

macro_rules! calculate_animation_value {
    ($name:ident, $s:ident, $display_parameter:ident) => {
        let p = {
            if let Some((start, animation)) = &$s.animations.$name {
                Some((start, animation.ref_clone()))
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
pub struct Item {
    id: usize,
    name: String,
    app_context: AppContext,
    children: Children,
    pub(crate) item_event: ItemEvent,
    animations: Animations,
    pub(crate) captured_mouse_button: Vec<MouseButton>,
    pub(crate) captured_touch_id: Vec<(DeviceId, u64)>,
    pub(crate) on_attach: LinkedList<Box<dyn FnMut()>>,
    pub(crate) on_detach: LinkedList<Box<dyn FnMut()>>,
    click_source: Option<ClickSource>,
    on_click: Option<Box<dyn FnMut(ClickSource)>>,
    focused: Shared<bool>,
    pub(crate) on_focus: Arc<Mutex<Vec<Box<dyn FnMut(bool) + 'static>>>>,
    on_mouse_input: Option<Box<dyn FnMut(MouseEvent)>>,
    on_touch: Option<Box<dyn FnMut(TouchEvent)>>,
    touch_start_time: Instant,
    recorded_parameter: Option<DisplayParameter>,
    target_parameter: DisplayParameter,
    display_parameter_out: Shared<DisplayParameter>,
    measure_parameter: DisplayParameter,
    custom_properties: HashMap<String, CustomProperty>,
    baseline: Option<f32>,
    active: SharedBool,
    layout_direction: Shared<LayoutDirection>,
    width: SharedSize,
    min_width: SharedF32,
    max_width: SharedF32,
    height: SharedSize,
    min_height: SharedF32,
    max_height: SharedF32,
    padding_start: SharedF32,
    padding_top: SharedF32,
    padding_end: SharedF32,
    padding_bottom: SharedF32,
    margin_start: SharedF32,
    margin_top: SharedF32,
    margin_end: SharedF32,
    margin_bottom: SharedF32,
    scale_x: SharedF32,
    scale_y: SharedF32,
    scale_center_x: SharedInnerPosition,
    scale_center_y: SharedInnerPosition,
    offset_x: SharedF32,
    offset_y: SharedF32,
    opacity: SharedF32,
    rotation: SharedF32,
    rotation_center_x: SharedInnerPosition,
    rotation_center_y: SharedInnerPosition,
    skew_x: SharedF32,
    skew_y: SharedF32,
    skew_center_x: SharedInnerPosition,
    skew_center_y: SharedInnerPosition,
    background: SharedItem,
    foreground: SharedItem,
    enable_background_blur: SharedBool,
    horizontal_gravity: Shared<Gravity>,
    vertical_gravity: Shared<Gravity>,
}

impl_property_re_layout!(
    active,
    get_active,
    SharedBool,
    "Whether the item is active and can receive input events."
);
impl_property_re_layout!(
    layout_direction,
    get_layout_direction,
    Shared<LayoutDirection>,
    "The layout direction of the item."
);
impl_property_re_layout!(
    width,
    get_width,
    SharedSize,
    "The width of the item. See [`Size`](crate::ui::item::Size) for more information."
);
impl_property_re_layout!(
    min_width,
    get_min_width,
    SharedF32,
    "The minimum width of the item."
);
impl_property_re_layout!(
    max_width,
    get_max_width,
    SharedF32,
    "The maximum width of the item."
);
impl_property_re_layout!(
    height,
    get_height,
    SharedSize,
    "The height of the item. See [`Size`](crate::ui::item::Size) for more information."
);
impl_property_re_layout!(
    min_height,
    get_min_height,
    SharedF32,
    "The minimum height of the item."
);
impl_property_re_layout!(
    max_height,
    get_max_height,
    SharedF32,
    "The maximum height of the item."
);
impl_property_re_layout!(padding_start, get_padding_start, SharedF32,
    "The padding at the start of the item. The \"start\" direction depends on the layout direction.");
impl_property_re_layout!(
    padding_top,
    get_padding_top,
    SharedF32,
    "The padding at the top of the item."
);
impl_property_re_layout!(
    padding_end,
    get_padding_end,
    SharedF32,
    "The padding at the end of the item. The \"end\" direction depends on the layout direction."
);
impl_property_re_layout!(
    padding_bottom,
    get_padding_bottom,
    SharedF32,
    "The padding at the bottom of the item."
);
impl_property_re_layout!(
    margin_start,
    get_margin_start,
    SharedF32,
    "The margin at the start of the item. The \"start\" direction depends on the layout direction."
);
impl_property_re_layout!(
    margin_top,
    get_margin_top,
    SharedF32,
    "The margin at the top of the item."
);
impl_property_re_layout!(
    margin_end,
    get_margin_end,
    SharedF32,
    "The margin at the end of the item. The \"end\" direction depends on the layout direction."
);
impl_property_re_layout!(
    margin_bottom,
    get_margin_bottom,
    SharedF32,
    "The margin at the bottom of the item."
);
impl_property_re_layout!(
    scale_x,
    get_scale_x,
    SharedF32,
    "The scale in the x direction."
);
impl_property_re_layout!(
    scale_y,
    get_scale_y,
    SharedF32,
    "The scale in the y direction."
);
impl_property_re_layout!(
    scale_center_x,
    get_scale_center_x,
    SharedInnerPosition,
    "The center of scaling in the x direction."
);
impl_property_re_layout!(
    scale_center_y,
    get_scale_center_y,
    SharedInnerPosition,
    "The center of scaling in the y direction."
);
impl_property_re_layout!(
    offset_x,
    get_offset_x,
    SharedF32,
    "The offset in the x direction relative to the original position."
);
impl_property_re_layout!(
    offset_y,
    get_offset_y,
    SharedF32,
    "The offset in the y direction relative to the original position."
);
impl_property_re_layout!(
    opacity,
    get_opacity,
    SharedF32,
    "The opacity of the item. It will also affect the opacity of its children."
);
impl_property_re_layout!(
    rotation,
    get_rotation,
    SharedF32,
    "The rotation of the item in degrees."
);
impl_property_re_layout!(
    rotation_center_x,
    get_rotation_center_x,
    SharedInnerPosition,
    "The center of rotation in the x direction."
);
impl_property_re_layout!(
    rotation_center_y,
    get_rotation_center_y,
    SharedInnerPosition,
    "The center of rotation in the y direction."
);
impl_property_re_layout!(
    skew_x,
    get_skew_x,
    SharedF32,
    "The skew in the x direction in degrees."
);
impl_property_re_layout!(
    skew_y,
    get_skew_y,
    SharedF32,
    "The skew in the y direction in degrees."
);
impl_property_re_layout!(
    skew_center_x,
    get_skew_center_x,
    SharedInnerPosition,
    "The center of skew in the x direction."
);
impl_property_re_layout!(
    skew_center_y,
    get_skew_center_y,
    SharedInnerPosition,
    "The center of skew in the y direction."
);
impl_property_re_layout!(
    background,
    get_background,
    SharedItem,
    "The background of the item. It will be drawn behind the content (including children)"
);
impl_property_re_layout!(
    foreground,
    get_foreground,
    SharedItem,
    "The foreground of the item. It will be drawn in front of the content (including children)"
);
impl_property_re_layout!(enable_background_blur, get_enable_background_blur, SharedBool,
    "Whether to enable background blur. This will cause the background to be blurred when it is not fully opaque.");
impl_property_re_layout!(horizontal_gravity, get_horizontal_gravity, Shared<Gravity>,
    "The horizontal gravity of the item. It determines how the item is positioned horizontally within its parent.");
impl_property_re_layout!(vertical_gravity, get_vertical_gravity, Shared<Gravity>,
    "The vertical gravity of the item. It determines how the item is positioned vertically within its parent.");

impl Item {
    pub fn new(app_context: AppContext, children: Children, item_event: ItemEvent) -> Self {
        let id = generate_id();

        let mut item = Self {
            id,
            name: format!("Item {}", id),
            app_context,
            children,
            item_event,
            animations: Default::default(),
            captured_mouse_button: vec![],
            captured_touch_id: vec![],
            on_attach: LinkedList::new(),
            on_detach: LinkedList::new(),
            click_source: None,
            on_click: None,
            focused: false.into(),
            on_focus: Arc::new(Mutex::new(vec![])),
            on_mouse_input: None,
            on_touch: None,
            touch_start_time: Instant::now(),
            recorded_parameter: None,
            target_parameter: Default::default(),
            display_parameter_out: Shared::from_static(Default::default()),
            measure_parameter: Default::default(),
            custom_properties: HashMap::new(),
            baseline: None,
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
            background: SharedItem::none(),
            foreground: SharedItem::none(),
            enable_background_blur: false.into(),
            horizontal_gravity: Gravity::Start.into(),
            vertical_gravity: Gravity::Start.into(),
        };
        init_property_re_layout(item.app_context.ref_clone(), &mut item.active, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.layout_direction,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.width, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.min_width, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.max_width, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.height, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.min_height, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.max_height, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.padding_start,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.padding_top, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.padding_end, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.padding_bottom,
            item.id,
        );
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.margin_start,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.margin_top, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.margin_end, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.margin_bottom,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.scale_x, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.scale_y, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.scale_center_x,
            item.id,
        );
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.scale_center_y,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.offset_x, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.offset_y, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.opacity, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.rotation, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.rotation_center_x,
            item.id,
        );
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.rotation_center_y,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.skew_x, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.skew_y, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.skew_center_x,
            item.id,
        );
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.skew_center_y,
            item.id,
        );
        init_property_re_layout(item.app_context.ref_clone(), &mut item.background, item.id);
        init_property_re_layout(item.app_context.ref_clone(), &mut item.foreground, item.id);
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.enable_background_blur,
            item.id,
        );
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.horizontal_gravity,
            item.id,
        );
        init_property_re_layout(
            item.app_context.ref_clone(),
            &mut item.vertical_gravity,
            item.id,
        );
        item.focused(false)
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

    pub fn custom_property(mut self, name: impl Into<String>, property: CustomProperty) -> Self {
        self.custom_properties.insert(name.into(), property);
        self
    }

    pub fn get_custom_property(&self, name: &str) -> Option<&CustomProperty> {
        self.custom_properties.get(name)
    }

    pub fn set_base_line(&mut self, base_line: f32) {
        self.baseline = Some(base_line);
    }

    pub fn get_baseline(&self) -> Option<f32> {
        self.baseline
    }

    pub fn focused(mut self, focused: impl Into<Shared<bool>>) -> Self {
        let self_item_id = self.id;
        self.focused.remove_observer(self_item_id);

        let mut app_context = self.app_context.ref_clone();

        self.focused = focused.into();
        let self_item_id = self.id;
        let focused_property_clone = self.focused.ref_clone();
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
                            focused_property
                                .replace((focused_property_clone.ref_clone(), self_item_id))
                        });
                        app_context.send_user_event(UserEvent::RequestFocus);
                    }
                    Action::Nothing => {
                        if let Some(v) = focused_property_value {
                            app_context.focused_property.write(move |focused_property| {
                                focused_property.replace((v.0.ref_clone(),v.1))
                            });
                        }
                    }
                    _ => {}
                }
            });
        self
    }

    pub(crate) fn focus(&mut self, focused: bool) {
        let on_focus = self.on_focus.clone();
        on_focus.lock().unwrap().iter_mut().for_each(|f| f(focused));
        let on_focus = self.item_event.on_focus.clone();
        {
            let mut on_focus = on_focus.lock().unwrap();
            on_focus(self, focused)
        }
    }

    pub fn get_focused(&self) -> Shared<bool> {
        self.focused.ref_clone()
    }

    pub fn is_animating(&self) -> bool {
        self.animations.is_animating()
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

    pub fn on_focus<F>(self, f: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        self.on_focus.lock().unwrap().push(Box::new(f));
        self
    }

    pub fn on_mouse_input<F>(mut self, f: F) -> Self
    where
        F: FnMut(MouseEvent) + 'static,
    {
        self.on_mouse_input = Some(Box::new(f));
        self
    }

    pub fn find_item(&self, id: usize, f: &mut impl FnMut(&Item)) {
        if self.id == id {
            f(self);
        } else {
            for child in self.children.items().iter() {
                child.find_item(id, f);
            }
        }
    }

    pub fn find_item_mut(&mut self, id: usize, f: &mut impl FnMut(&mut Item)) {
        if self.id == id {
            f(self);
        } else {
            for child in self.children.items().iter_mut() {
                child.find_item_mut(id, f);
            }
        }
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

    pub fn set_target_parameter(&mut self, parameter: DisplayParameter) {
        self.target_parameter.copy_from(&parameter)
    }

    pub fn get_target_parameter(&mut self) -> &mut DisplayParameter {
        &mut self.target_parameter
    }

    pub fn get_measure_parameter(&mut self) -> &mut DisplayParameter {
        &mut self.measure_parameter
    }

    pub fn clone_measure_parameter(&self) -> DisplayParameter {
        self.measure_parameter.clone()
    }

    // pub fn set_measure_parameter(&mut self, parameter: DisplayParameter) {
    //     self.measure_parameter = parameter;
    // }

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

    pub fn get_margin(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.margin_start.get() + self.margin_end.get(),
            Orientation::Vertical => self.margin_top.get() + self.margin_bottom.get(),
        }
    }

    pub fn clamp_width(&self, width: f32) -> f32 {
        let min_width = self.min_width.get();
        let max_width = self.max_width.get();
        width.clamp(min_width, max_width)
    }

    pub fn clamp_height(&self, height: f32) -> f32 {
        let min_height = self.min_height.get();
        let max_height = self.max_height.get();
        height.clamp(min_height, max_height)
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

    fn layout_layer(mut layer: SharedItem, width: f32, height: f32) {
        if let Some(item) = layer.value().as_mut(){
            item.measure(
                MeasureMode::Specified(width),
                MeasureMode::Specified(height),
            );
            item.dispatch_layout(0.0, 0.0, width, height);
        }
    }

    pub fn layout_layers(&self, width: f32, height: f32) {
        Self::layout_layer(self.get_background(), width, height);
        Self::layout_layer(self.get_foreground(), width, height);
    }

    pub(crate) fn record_display_parameter(&mut self) {
        self.recorded_parameter = Some(self.get_display_parameter());
        self.children.items().iter_mut().for_each(|child| {
            child.record_display_parameter();
        });
    }

    pub(crate) fn dispatch_animation(&mut self, animation: Animation) {
        if !animation.is_target(self.id) {
            return;
        }
        if let Some(recorded_parameter) = self.recorded_parameter.clone() {
            let target_parameter = self.target_parameter.clone();
            if !f32_eq(recorded_parameter.width, target_parameter.width) {
                self.animations.parent_x =
                    Some((recorded_parameter.parent_x, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.height, target_parameter.height) {
                self.animations.parent_y =
                    Some((recorded_parameter.parent_y, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.relative_x, target_parameter.relative_x) {
                self.animations.relative_x =
                    Some((recorded_parameter.relative_x, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.relative_y, target_parameter.relative_y) {
                self.animations.relative_y =
                    Some((recorded_parameter.relative_y, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.offset_x, target_parameter.offset_x) {
                self.animations.offset_x =
                    Some((recorded_parameter.offset_x, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.offset_y, target_parameter.offset_y) {
                self.animations.offset_y =
                    Some((recorded_parameter.offset_y, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.opacity, target_parameter.opacity) {
                self.animations.opacity = Some((recorded_parameter.opacity, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.rotation, target_parameter.rotation) {
                self.animations.rotation =
                    Some((recorded_parameter.rotation, animation.ref_clone()));
            }
            if !f32_eq(
                recorded_parameter.rotation_center_x,
                target_parameter.rotation_center_x,
            ) {
                self.animations.rotation_center_x =
                    Some((recorded_parameter.rotation_center_x, animation.ref_clone()));
            }
            if !f32_eq(
                recorded_parameter.rotation_center_y,
                target_parameter.rotation_center_y,
            ) {
                self.animations.rotation_center_y =
                    Some((recorded_parameter.rotation_center_y, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.scale_x, target_parameter.scale_x) {
                self.animations.scale_x = Some((recorded_parameter.scale_x, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.scale_y, target_parameter.scale_y) {
                self.animations.scale_y = Some((recorded_parameter.scale_y, animation.ref_clone()));
            }
            if !f32_eq(
                recorded_parameter.scale_center_x,
                target_parameter.scale_center_x,
            ) {
                self.animations.scale_center_x =
                    Some((recorded_parameter.scale_center_x, animation.ref_clone()));
            }
            if !f32_eq(
                recorded_parameter.scale_center_y,
                target_parameter.scale_center_y,
            ) {
                self.animations.scale_center_y =
                    Some((recorded_parameter.scale_center_y, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.skew_x, target_parameter.skew_x) {
                self.animations.skew_x = Some((recorded_parameter.skew_x, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.skew_y, target_parameter.skew_y) {
                self.animations.skew_y = Some((recorded_parameter.skew_y, animation.ref_clone()));
            }
            if !f32_eq(
                recorded_parameter.skew_center_x,
                target_parameter.skew_center_x,
            ) {
                self.animations.skew_center_x =
                    Some((recorded_parameter.skew_center_x, animation.ref_clone()));
            }
            if !f32_eq(
                recorded_parameter.skew_center_y,
                target_parameter.skew_center_y,
            ) {
                self.animations.skew_center_y =
                    Some((recorded_parameter.skew_center_y, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.width, target_parameter.width) {
                self.animations.width = Some((recorded_parameter.width, animation.ref_clone()));
            }
            if !f32_eq(recorded_parameter.height, target_parameter.height) {
                self.animations.height = Some((recorded_parameter.height, animation.ref_clone()));
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
                                    .insert(key.clone(), (start.clone(), animation.ref_clone()));
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
                                    .insert(key.clone(), (start.clone(), animation.ref_clone()));
                            }
                        }
                    });
            }
        }

        self.children.items().iter_mut().for_each(|child| {
            child.dispatch_animation(animation.ref_clone());
        });
    }

    pub fn dispatch_draw(&mut self, surface: &mut Surface, parent_x: f32, parent_y: f32) {
        let f = self.item_event.dispatch_draw.clone();
        f.lock().unwrap()(self, surface, parent_x, parent_y);
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        let f = self.item_event.draw.clone();
        f.lock().unwrap()(self, canvas);
    }

    pub fn measure(&mut self, width_mode: MeasureMode, height_mode: MeasureMode) {
        let f = self.item_event.measure.clone();
        f.lock().unwrap()(self, width_mode, height_mode);
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
            let child_width = child.get_width().get();
            let child_height = child.get_height().get();
            child.measure(
                create_mode(child_width, max_width),
                create_mode(child_height, max_height),
            );
        });
    }

    pub fn dispatch_layout(&mut self, relative_x: f32, relative_y: f32, width: f32, height: f32) {
        let f = self.item_event.dispatch_layout.clone();
        f.lock().unwrap()(self, relative_x, relative_y, width, height);
    }

    pub fn layout(&mut self, width: f32, height: f32) {
        let f = self.item_event.layout.clone();
        f.lock().unwrap()(self, width, height);
    }

    pub fn mouse_input(&mut self, event: MouseEvent) {
        let x = event.x;
        let y = event.y;

        let foreground = self.get_foreground();
        if let Some(foreground) = foreground.value().as_mut() {
            foreground.mouse_input(event);
        }

        let background = self.get_background();
        if let Some(background) = background.value().as_mut() {
            background.mouse_input(event);
        }

        if let Some(on_mouse_input) = &mut self.on_mouse_input {
            on_mouse_input(event);
        }

        {
            let f = self.item_event.mouse_input.clone();
            let mut mouse_input = f.lock().unwrap();
            mouse_input(self, event);
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
                            child
                                .captured_mouse_button
                                .retain(|&button| button != event.button);
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
                            {
                                let f = self.item_event.on_click.clone();
                                let mut on_click = f.lock().unwrap();
                                on_click(self, click_source);
                            }
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

        let foreground = self.get_foreground();
        if let Some(foreground) = foreground.value().as_mut() {
            foreground.touch_input(event);
        }
        let background = self.get_background();
        if let Some(background) = background.value().as_mut() {
            background.touch_input(event);
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
                        if child
                            .captured_touch_id
                            .contains(&(event.device_id, event.id))
                        {
                            child.touch_input(event);
                            return;
                        }
                    }
                    PointerState::Ended | PointerState::Canceled => {
                        if child
                            .captured_touch_id
                            .contains(&(event.device_id, event.id))
                        {
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
                        let elapsed_time = self.touch_start_time.elapsed().as_millis();
                        {
                            let on_click = self.item_event.on_click.clone();
                            let mut on_click = on_click.lock().unwrap();
                            if elapsed_time < 300 {
                                on_click(self, click_source);
                            } else {
                                on_click(self, ClickSource::LongTouch);
                            }
                        }
                        if let Some(on_click) = &mut self.on_click {
                            if elapsed_time < 300 {
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

    pub fn ime_input(&mut self, event: ImeAction) {
        let f = self.item_event.ime_input.clone();
        let mut ime_input = f.lock().unwrap();
        ime_input(self, event.clone());
    }

    pub fn dispatch_keyboard_input(
        &mut self,
        device_id: DeviceId,
        event: KeyEvent,
        is_synthetic: bool,
    ) -> bool {
        let f = self.item_event.dispatch_keyboard_input.clone();
        let mut keyboard_input = f.lock().unwrap();
        keyboard_input(self, device_id, event.clone(), is_synthetic)
    }

    pub fn dispatch_focus(&mut self) {
        let f = self.item_event.dispatch_focus.clone();
        f.lock().unwrap()(self);
    }

    pub fn dispatch_timer(&mut self, timer_id: usize) {
        let f = self.item_event.dispatch_timer.clone();
        f.lock().unwrap()(self, timer_id);
    }
}

fn f32_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.1
}
