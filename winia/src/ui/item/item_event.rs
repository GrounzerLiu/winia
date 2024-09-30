use crate::property::Gettable;
use crate::ui::item::InnerPosition;
use crate::ui::Item;
use crate::OptionalInvoke;
use skia_safe::{Canvas, Point};
use std::sync::{Arc, Mutex};
use winit::event::{DeviceId, Force, MouseButton, TouchPhase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug)]
pub enum MeasureMode {
    /// Indicates that the parent has determined an exact size for the child.
    Specified(f32),
    /// Indicates that the child can determine its own size. The value of this enum is the maximum size the child can use.
    Unspecified(f32),
}

#[derive(Clone, Copy, Debug)]
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

// #[derive(Clone, Copy, Debug, PartialEq)]
// pub enum Pointer{
//     Mouse(MouseButton),
//     Touch(u64),
// }
// 
// #[derive(Clone, Copy, Debug)]
// pub struct PointerEvent {
//     pub device_id: DeviceId,
//     pub pointer: Pointer,
//     pub x: f32,
//     pub y: f32,
//     pub pointer_state: PointerState,
//     pub force: Option<Force>,
// }
// 
// impl From<TouchEvent> for PointerEvent {
//     fn from(value: TouchEvent) -> Self {
//         Self {
//             device_id: value.device_id,
//             pointer: Pointer::Touch(value.id),
//             x: value.x,
//             y: value.y,
//             pointer_state: value.pointer_state,
//             force: value.force,
//         }
//     }
// }
// 
// impl From<MouseEvent> for PointerEvent {
//     fn from(value: MouseEvent) -> Self {
//         Self {
//             device_id: value.device_id,
//             pointer: Pointer::Mouse(value.button),
//             x: value.x,
//             y: value.y,
//             pointer_state: value.pointer_state,
//             force: None,
//         }
//     }
// }

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

pub struct ItemEvent {
    pub(crate) dispatch_draw: Arc<Mutex<dyn FnMut(&mut Item, &Canvas, f32, f32)>>,
    pub(crate) draw: Arc<Mutex<dyn FnMut(&mut Item, &Canvas)>>,
    pub(crate) dispatch_layout: Arc<Mutex<dyn FnMut(&mut Item, f32, f32, f32, f32)>>,
    pub(crate) layout: Arc<Mutex<dyn FnMut(&mut Item, f32, f32, f32, f32)>>,
    pub(crate) measure: Arc<Mutex<dyn FnMut(&mut Item, Orientation, MeasureMode)>>,
    pub(crate) dispatch_mouse_input: Arc<Mutex<dyn FnMut(&mut Item, MouseEvent)>>,
    pub(crate) mouse_input: Arc<Mutex<dyn FnMut(&mut Item, MouseEvent)>>,
    pub(crate) dispatch_touch_input: Arc<Mutex<dyn FnMut(&mut Item, TouchEvent)>>,
    pub(crate) touch_input: Arc<Mutex<dyn FnMut(&mut Item, TouchEvent)>>,
}

impl ItemEvent {
    pub fn new() -> Self {
        Self {
            dispatch_draw: Arc::new(Mutex::new(|item: &mut Item, canvas: &Canvas, parent_x: f32, parent_y: f32| {
                item.get_display_parameter().set_parent_position(parent_x, parent_y);

                let display_parameter = item.get_display_parameter().clone();

                let x = display_parameter.x();
                let y = display_parameter.y();
                let rotation = display_parameter.rotation();

                canvas.save();
                canvas.rotate(rotation, Some(
                    Point::new(
                        display_parameter.rotation_center_x(), display_parameter.rotation_center_y(),
                    )
                ));
                
                let skew_x = display_parameter.skew_x();
                let skew_y = display_parameter.skew_y();
                let skew_center_x = display_parameter.skew_center_x();
                let skew_center_y = display_parameter.skew_center_y();
                canvas.translate((skew_center_x, skew_center_y));
                canvas.skew((skew_x, skew_y));
                canvas.translate((-skew_center_x, -skew_center_y));
                
                let scale_center_x = display_parameter.scale_center_x();
                let scale_center_y = display_parameter.scale_center_y();
                canvas.translate((scale_center_x, scale_center_y));
                canvas.scale((display_parameter.scale_x(), display_parameter.scale_y()));
                canvas.translate((-scale_center_x, -scale_center_y));
                
                
                item.get_background().value().if_mut_some(|background| {
                    background.dispatch_draw(canvas, x, y);
                });
                item.draw(canvas);
                item.get_children().items().iter_mut().for_each(|child| {
                    child.dispatch_draw(canvas, x, y);
                });
                item.get_foreground().value().if_mut_some(|foreground| {
                    foreground.dispatch_draw(canvas, x, y);
                });
                
                
                canvas.restore();
            })),
            draw: Arc::new(Mutex::new(|_item: &mut Item, _canvas: &Canvas| {})),
            dispatch_layout: Arc::new(Mutex::new(|item: &mut Item, relative_x: f32, relative_y: f32, width: f32, height: f32| {
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


                    let display_parameter = item.get_display_parameter();
                    display_parameter.set_relative_position(relative_x, relative_y);
                    display_parameter.set_width(width);
                    display_parameter.set_height(height);
                    display_parameter.set_opacity(opacity);
                    display_parameter.set_rotation(rotation);
                    display_parameter.set_rotation_center_x(rotation_center_x);
                    display_parameter.set_rotation_center_y(rotation_center_y);
                    display_parameter.set_scale_x(scale_x);
                    display_parameter.set_scale_y(scale_y);
                    display_parameter.set_scale_center_x(scale_center_x);
                    display_parameter.set_scale_center_y(scale_center_y);
                    display_parameter.set_offset_x(offset_x);
                    display_parameter.set_offset_y(offset_y);
                    display_parameter.set_skew_x(skew_x);
                    display_parameter.set_skew_y(skew_y);
                    display_parameter.set_skew_center_x(skew_center_x);
                    display_parameter.set_skew_center_y(skew_center_y);
                }

                item.layout(relative_x, relative_y, width, height);
            })),
            layout: Arc::new(Mutex::new(|_item: &mut Item, _relative_x: f32, _relative_y: f32, _width: f32, _height: f32| {})),
            measure: Arc::new(Mutex::new(|_item: &mut Item, _orientation: Orientation, _mode: MeasureMode| {})),
            dispatch_mouse_input: Arc::new(Mutex::new(|_item: &mut Item, _event: MouseEvent| {})),
            mouse_input: Arc::new(Mutex::new(|_item: &mut Item, _event: MouseEvent| {})),
            dispatch_touch_input: Arc::new(Mutex::new(|_item: &mut Item, _event: TouchEvent| {})),
            touch_input: Arc::new(Mutex::new(|_item: &mut Item, _event: TouchEvent| {})),
        }
    }

    /// item, canvas, parent_x, parent_y
    pub fn dispatch_draw(mut self, dispatch_draw: impl FnMut(&mut Item, &Canvas, f32, f32) + 'static) -> Self {
        self.dispatch_draw = Arc::new(Mutex::new(dispatch_draw));
        self
    }

    /// item, canvas
    pub fn draw(mut self, draw: impl FnMut(&mut Item, &Canvas) + 'static) -> Self {
        self.draw = Arc::new(Mutex::new(draw));
        self
    }

    /// item, relative_x, relative_y, width, height
    pub fn dispatch_layout(mut self, dispatch_layout: impl FnMut(&mut Item, f32, f32, f32, f32) + 'static) -> Self {
        self.dispatch_layout = Arc::new(Mutex::new(dispatch_layout));
        self
    }

    /// item, relative_x, relative_y, width, height
    pub fn layout(mut self, layout: impl FnMut(&mut Item, f32, f32, f32, f32) + 'static) -> Self {
        self.layout = Arc::new(Mutex::new(layout));
        self
    }

    /// item, orientation, mode
    pub fn measure(mut self, measure: impl FnMut(&mut Item, Orientation, MeasureMode) + 'static) -> Self {
        self.measure = Arc::new(Mutex::new(measure));
        self
    }
}

impl Default for ItemEvent {
    fn default() -> Self {
        Self::new()
    }
}