mod animations;
mod children;
mod frame;
mod item_event;
mod item_props;
mod physical_x;
mod item_state;
mod focus_requester;
mod item_updater;

pub use crate::ui::alignment::*;
pub use crate::ui::size::*;
use crate::{calculate_animation_value, depend, override_animation};
pub use children::*;
pub use focus_requester::*;
pub use frame::*;
pub use item_event::*;
pub use item_props::*;
pub use item_state::*;
pub use physical_x::*;
pub use item_updater::*;

use std::fmt::Debug;
use std::ops::{Add, Deref, DerefMut};

use crate::animation::LayoutAnimation;
use crate::app::{EventLoopProxy, WindowContext};
use crate::core::next_id;
use crate::lock_api::MutexGuard;
use crate::shared::{Derived, Shared, SharedDerived, SharedDerivedBool, SharedSource};
use crate::ui::item::animations::Animations;
use crate::ui::{Color, Orientation};
use crate::override_animations;
use parking_lot::{Mutex, RawMutex};
use skia_safe::Picture;
use std::rc::Rc;
use std::sync::Arc;
use getset::Getters;
use winit::event::ButtonSource;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutDirection {
    LTR,
    RTL,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ItemKind {
    Container,
    #[default]
    Widget,
}

impl ItemKind {
    pub fn is_container(&self) -> bool {
        *self == ItemKind::Container
    }

    pub fn is_widget(&self) -> bool {
        *self == ItemKind::Widget
    }
}


#[derive(Copy, Clone, Debug, Default)]
pub struct FocusState {
    pub is_focused: bool,
    pub has_focus: bool,
    pub has_parent_focus: bool,
    pub is_captured: bool,
}

pub trait ItemPropsTrait {
    fn bind(&self, id: u32);
    fn to_item_props(self) -> ItemProps;
}

#[derive(Getters)]
pub struct ItemData {
    animations: Animations,
    children: SharedDerived<Vec<Item>>,
    draw_cache: Option<Picture>,
    event: ItemEvent,
    id: u32,
    pub focus_state: FocusState,
    kind: ItemKind,
    pub measure_frame: Frame,
    pub last_measure_width_mode: Option<MeasureMode>,
    pub last_measure_height_mode: Option<MeasureMode>,
    // pub needs_draw: Arc<Mutex<bool>>,
    props: ItemProps,
    recorded_frame: Option<Frame>,
    pub target_frame: Frame,
}

impl ItemData {
    pub fn new(
        kind: ItemKind,
        props: impl ItemPropsTrait,
        event: ItemEvent,
        children: impl Into<SharedDerived<Vec<Item>>>,
    ) -> Self {
        let id = next_id();
        props.bind(id);
        let children = children.into();
        let props = props.to_item_props();

        let width = props.width.clone();
        let height = props.height.clone();
        props.item_updater.lock().is_fixed_size = SharedDerivedBool::from_fn(
            depend!(width, height),
            move || {
                let width = width.get();
                let height = height.get();
                matches!(
                    (width, height),
                    (Size::Fixed(_), Size::Fixed(_))
                    | (Size::Fill, Size::Fill)
                    | (Size::Fixed(_), Size::Fill)
                    | (Size::Fill, Size::Fixed(_))
                )
            },
        );
        {
            let children = children.lock();
            for child in children.iter() {
                let child_data = child.data();
                child_data.props.item_updater.lock().parent = Some(Arc::downgrade(&props.item_updater));
            }
        }

        props.focus_requester.lock().set_event_loop_proxy(props.window_context.event_loop_proxy().clone());
        props.focus_requester.lock().set_focusable(&props.focusable);
        props.focus_requester.lock().set_item_id(id);

        // props.bind(id, &props.need_redraw, &props.window_context);
        Self {
            animations: Default::default(),
            children,
            draw_cache: None,
            event,
            id,
            focus_state: Default::default(),
            kind,
            measure_frame: Frame::default(),
            // needs_draw,
            last_measure_width_mode: None,
            last_measure_height_mode: None,
            props,
            recorded_frame: None,
            target_frame: Frame::default(),
        }
    }

    pub fn children(&self) -> &Shared<Vec<Item>, Derived> {
        &self.children
    }

    pub fn clamp_height(&self, value: f32) -> f32 {
        let max = self.props.max_height.get();
        let min = self.props.min_height.get();
        value.clamp(min, max)
    }

    pub fn clamp_width(&self, value: f32) -> f32 {
        let max = self.props.max_width.get();
        let min = self.props.min_width.get();
        value.clamp(min, max)
    }

    pub fn current_frame(&mut self) -> Frame {
        let mut frame = self.target_frame.clone();
        calculate_animation_value!(width, self, frame);
        calculate_animation_value!(height, self, frame);
        calculate_animation_value!(relative_x, self, frame);
        calculate_animation_value!(relative_y, self, frame);
        calculate_animation_value!(offset_x, self, frame);
        calculate_animation_value!(offset_y, self, frame);
        calculate_animation_value!(opacity, self, frame);
        calculate_animation_value!(rotation, self, frame);
        calculate_animation_value!(rotation_center_x, self, frame);
        calculate_animation_value!(rotation_center_y, self, frame);
        calculate_animation_value!(scale_x, self, frame);
        calculate_animation_value!(scale_y, self, frame);
        calculate_animation_value!(scale_center_x, self, frame);
        calculate_animation_value!(scale_center_y, self, frame);
        calculate_animation_value!(skew_x, self, frame);
        calculate_animation_value!(skew_y, self, frame);
        calculate_animation_value!(skew_center_x, self, frame);
        calculate_animation_value!(skew_center_y, self, frame);
        self.animations
            .float_params
            .retain(|_, (_, _, animation)| !animation.is_finished());
        self.animations
            .float_params
            .iter()
            .for_each(|(key, (start, _, animation))| {
                if let Some(end) = frame.float_params.get(key) {
                    frame
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
                if let Some(end) = frame.color_params.get(key) {
                    frame
                        .color_params
                        .insert(key.clone(), animation.interpolate_color(start, end));
                }
            });
        frame
    }

    pub fn dispatch_animation(&mut self, animation: &LayoutAnimation, forced: bool) {
        let (animatable, children_force) = animation.animatable(self.id, forced);

        if animatable && let Some(recorded_frame) = self.recorded_frame.clone() {
            let target_frame = self.target_frame.clone();
            override_animations!(
                animation,
                recorded_frame,
                target_frame,
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
                target_frame.float_params.iter().for_each(|(key, end)| {
                    let target_changed =
                        if let Some((_, end, _)) = self.animations.float_params.get(key) {
                            if let Some(target) = target_frame.float_params.get(key) {
                                target != end
                            } else {
                                true
                            }
                        } else {
                            true
                        };

                    if let Some(start) = recorded_frame.float_params.get(key) {
                        if (*start - *end).abs() > 0.1 && target_changed {
                            self.animations
                                .float_params
                                .insert(key.clone(), (*start, *end, animation.clone()));
                        }
                    } else if target_changed {
                        self.animations
                            .float_params
                            .insert(key.clone(), (0.0, *end, animation.clone()));
                    }
                });
            }

            {
                target_frame.color_params.iter().for_each(|(key, end)| {
                    let target_changed =
                        if let Some((_, end, _)) = self.animations.color_params.get(key) {
                            if let Some(target) = target_frame.color_params.get(key) {
                                target != end
                            } else {
                                true
                            }
                        } else {
                            true
                        };

                    if let Some(start) = recorded_frame.color_params.get(key) {
                        if start != end && target_changed {
                            self.animations
                                .color_params
                                .insert(key.clone(), (*start, *end, animation.clone()));
                        }
                    } else if target_changed {
                        self.animations
                            .color_params
                            .insert(key.clone(), (Color::TRANSPARENT, *end, animation.clone()));
                    }
                });
            }
        }

        self.children.lock().iter_mut().for_each(|child| {
            child.data().dispatch_animation(animation, children_force);
        });

        {
            let mut background = self.props.background.lock();
            if let Some(background) = background.deref_mut() {
                background.data().dispatch_animation(animation, forced);
            }
        }
        {
            let mut foreground = self.props.foreground.lock();
            if let Some(foreground) = foreground.deref_mut() {
                foreground.data().dispatch_animation(animation, forced);
            }
        }
    }

    pub fn for_each_child<F>(&self, mut f: F)
    where
        F: FnMut(&Item),
    {
        let children = self.children.lock();
        for child in children.iter() {
            f(child);
        }
    }

    pub fn get_padding(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => {
                self.props.padding.start.get() + self.props.padding.end.get()
            }
            Orientation::Vertical => self.props.padding.top.get() + self.props.padding.bottom.get(),
        }
    }

    pub fn get_padding_left(&self) -> f32 {
        match self.props.layout_direction.get() {
            LayoutDirection::LTR => self.props.padding.start.get(),
            LayoutDirection::RTL => self.props.padding.end.get()
        }
    }

    pub fn get_padding_right(&self) -> f32 {
        match self.props.layout_direction.get() {
            LayoutDirection::LTR => self.props.padding.end.get(),
            LayoutDirection::RTL => self.props.padding.start.get()
        }
    }


    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn item_event(&self) -> &ItemEvent {
        &self.event
    }


    pub fn measure_children(&mut self, width_mode: MeasureMode, height_mode: MeasureMode) {
        let padding_h = self.get_padding(Orientation::Horizontal);
        let padding_v = self.get_padding(Orientation::Vertical);
        let max_width = width_mode.value() - padding_h;
        let max_height = height_mode.value() - padding_v;
        self.for_each_child(|child| {
            let mut child_data = child.data();
            let child_width = child_data.props.width.get();
            let child_height = child_data.props.height.get();
            child_data.dispatch_measure(
                child_width.create_measure_mode(max_width),
                child_height.create_measure_mode(max_height),
            );
        });
    }

    pub fn on_click(&mut self, button_source: &ButtonSource) {
        if let Some(on_click) = &mut self.props.on_click {
            on_click(button_source);
        }
    }

    pub fn on_focus_changed(&mut self, focus_state: &FocusState) {
        if let Some(on_focus_changed) = &mut self.props.on_focus_changed {
            on_focus_changed(focus_state);
        }
    }

    pub fn on_hover_changed(&mut self, is_hovered: bool) {
        if let Some(on_hover_changed) = &mut self.props.on_hover_changed {
            on_hover_changed(is_hovered);
        }
    }

    pub fn on_pointer_button(&mut self, input: &PointerButton) -> bool {
        if let Some(on_pointer_button) = &mut self.props.on_pointer_button {
            on_pointer_button(input)
        } else {
            false
        }
    }
    pub fn on_pointer_moved(&mut self, input: &PointerMoved) -> bool {
        if let Some(on_pointer_moved) = &mut self.props.on_pointer_moved {
            on_pointer_moved(input)
        } else {
            false
        }
    }

    pub fn on_state_changed(&mut self, state: ItemState) {
        let item_state = self.props.item_state.clone();
        (self.props.on_state_changed)(item_state, state);
    }


    pub fn props(&self) -> &ItemProps {
        &self.props
    }
    pub(crate) fn record_frame(&mut self) {
        self.recorded_frame = Some(self.current_frame());
        self.children.lock().iter_mut().for_each(|child| {
            child.data().record_frame();
        });
        if let Some(background) = self.props.background.lock().deref_mut() {
            background.data().record_frame();
        }
        if let Some(foreground) = self.props.foreground.lock().deref_mut() {
            foreground.data().record_frame();
        }
    }

    pub fn re_layout(&mut self) {
        let target_frame = &self.target_frame;
        self.dispatch_layout(
            target_frame.relative_x,
            target_frame.relative_y,
            target_frame.width,
            target_frame.height,
        );
    }
    pub fn re_measure(&mut self, max_width: f32, max_height: f32) {
        let width = self.props.width.get();
        let height = self.props.height.get();
        let width_mode = self.last_measure_width_mode
            .unwrap_or(width.create_measure_mode(max_width));
        let height_mode = self.last_measure_height_mode
            .unwrap_or(height.create_measure_mode(max_height));
        self.dispatch_measure(width_mode, height_mode);
    }

    pub fn window_context(&self) -> &WindowContext {
        &self.props.window_context
    }
}

impl Deref for ItemData {
    type Target = ItemProps;

    fn deref(&self) -> &Self::Target {
        &self.props
    }
}

pub struct Item {
    data: Rc<Mutex<ItemData>>,
}

impl Item {
    pub fn new(
        kind: ItemKind,
        event: ItemEvent,
        props: impl ItemPropsTrait,
        children: impl Into<SharedDerived<Vec<Item>>>,
    ) -> Self {
        let data = ItemData::new(kind, props, event, children);
        Self {
            data: Rc::new(Mutex::new(data)),
        }
    }

    pub fn data(&self) -> MutexGuard<'_, RawMutex, ItemData> {
        self.data.lock()
    }

    pub fn id(&self) -> u32 {
        self.data().id
    }
}

impl Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = self.data();
        f.debug_struct("Item")
         .field("id", &data.id)
         .field("kind", &data.kind)
         .finish()
    }
}

impl Add<Item> for Item {
    type Output = Children;

    fn add(self, rhs: Item) -> Self::Output {
        let mut children = Children::empty();
        children.add_item(self);
        children.add_item(rhs);
        children
    }
}
