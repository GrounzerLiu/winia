use std::ops::Not;
use crate::app::Theme;
use crate::shared::Gettable;
use crate::ui::item::InnerPosition;
use crate::ui::Item;
use crate::OptionalInvoke;
// use skia_bindings::SkTileMode;
use skia_safe::image_filters::CropRect;
use skia_safe::{image_filters, Canvas, IRect, Paint, Point, Rect, Surface, TileMode, Vector};
use std::sync::{Arc, Mutex};
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

#[derive(Clone, Debug)]
pub enum ImeAction {
    Enabled,
    Enter,
    Delete,
    PreEdit(String, Option<(usize, usize)>),
    Commit(String),
    Disabled,
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

#[derive(Clone)]
pub struct ItemEvent {
    pub(crate) dispatch_draw: Arc<Mutex<dyn FnMut(&mut Item, &mut Surface, f32, f32)>>,
    pub(crate) draw: Arc<Mutex<dyn FnMut(&mut Item, &Canvas)>>,
    pub(crate) dispatch_layout: Arc<Mutex<dyn FnMut(&mut Item, f32, f32, f32, f32)>>,
    pub(crate) layout: Arc<Mutex<dyn FnMut(&mut Item, f32, f32)>>,
    pub(crate) measure: Arc<Mutex<dyn FnMut(&mut Item, MeasureMode, MeasureMode)>>,
    pub(crate) dispatch_apply_theme: Arc<Mutex<dyn FnMut(&mut Item, &Theme)>>,
    pub(crate) apply_theme: Arc<Mutex<dyn FnMut(&mut Item, &Theme)>>,
    pub(crate) dispatch_mouse_input: Arc<Mutex<dyn FnMut(&mut Item, MouseEvent)>>,
    pub(crate) mouse_input: Arc<Mutex<dyn FnMut(&mut Item, MouseEvent)>>,
    pub(crate) dispatch_touch_input: Arc<Mutex<dyn FnMut(&mut Item, TouchEvent)>>,
    pub(crate) touch_input: Arc<Mutex<dyn FnMut(&mut Item, TouchEvent)>>,
    pub(crate) on_click: Arc<Mutex<dyn FnMut(&mut Item, ClickSource)>>,
    pub(crate) ime_input: Arc<Mutex<dyn FnMut(&mut Item, ImeAction)>>,
    pub(crate) dispatch_keyboard_input: Arc<Mutex<dyn FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool>>,
    pub(crate) keyboard_input: Arc<Mutex<dyn FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool>>,
    pub(crate) dispatch_focus: Arc<Mutex<dyn FnMut(&mut Item)>>,
    pub(crate) on_focus: Arc<Mutex<dyn FnMut(&mut Item, bool)>>,
    pub(crate) dispatch_timer: Arc<Mutex<dyn FnMut(&mut Item, usize) -> bool>>,
    pub(crate) timer: Arc<Mutex<dyn FnMut(&mut Item, usize) -> bool>>,
}

impl ItemEvent {
    pub fn new() -> Self {
        Self {
            dispatch_draw: Arc::new(Mutex::new(|item: &mut Item, surface: &mut Surface, parent_x: f32, parent_y: f32| {
                {
                    let target_parameter = item.get_target_parameter();
                    target_parameter.set_parent_position(parent_x, parent_y);
                }

                let display_parameter = item.get_display_parameter().clone();

                {
                    let blur = 35.0;
                    let margin = blur * 2.0;
                    if item.get_enable_background_blur().get() {
/*                        let image = surface.image_snapshot();
                        let (width, height) = {
                            let image_info = image.image_info();
                            (image_info.width(), image_info.height())
                        };
                        let canvas = surface.canvas();
                        let mut paint = Paint::default();
                        paint.set_image_filter(image_filters::blur((35.0, 35.0), TileMode::Clamp, None, CropRect::from(
                            Rect::from_wh(
                                width as f32,
                                height as f32,
                            )
                        )));
                        canvas.save();
                        let scale_factor = 1.0 /item.get_app_context().scale_factor();

                        canvas.clip_rect(
                            Rect::from_xywh(display_parameter.x(), display_parameter.y(), display_parameter.width, display_parameter.height),
                            None,
                            None,
                        );
                        canvas.scale((scale_factor, scale_factor));
                        canvas.draw_image(image, (0.0, 0.0), Some(&paint));
                        canvas.restore();*/
                        let scale_factor = item.get_app_context().scale_factor();
                        let left = (display_parameter.x() * scale_factor - margin) as i32;
                        let top = (display_parameter.y() * scale_factor - margin) as i32;
                        let right = ((display_parameter.x() + display_parameter.width) * scale_factor + margin) as i32;
                        let bottom = ((display_parameter.y() + display_parameter.height) * scale_factor + margin) as i32;

                        let background = surface.image_snapshot_with_bounds(
                            IRect::from_ltrb(
                                left,
                                top,
                                right,
                                bottom,
                            )
                        ).unwrap();

                        let (width, height) = {
                            let image_info = background.image_info();
                            (image_info.width(), image_info.height())
                        };

                        let canvas = surface.canvas();
                        let mut paint = Paint::default();
                        paint.set_image_filter(image_filters::blur((blur, blur), TileMode::Clamp, None, CropRect::from(
                            Rect::from_wh(
                                width as f32,
                                height as f32,
                            )
                        )));

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
                    let canvas = surface.canvas();
                    canvas.save();
                    canvas.rotate(rotation, Some(
                        Point::new(
                            display_parameter.rotation_center_x, display_parameter.rotation_center_y,
                        )
                    ));

                    canvas.translate((skew_center_x, skew_center_y));
                    canvas.skew((skew_x, skew_y));
                    canvas.translate((-skew_center_x, -skew_center_y));

                    canvas.translate((scale_center_x, scale_center_y));
                    canvas.scale((display_parameter.scale_x, display_parameter.scale_y));
                    canvas.translate((-scale_center_x, -scale_center_y));
                }

                item.get_background().write(|background| {
                    if let Some(background) = background {
                        background.dispatch_draw(surface, x, y);
                    }
                });
                {
                    let canvas = surface.canvas();
                    item.draw(canvas);
                }
                item.get_children().items().iter_mut().for_each(|child| {
                    child.dispatch_draw(surface, x, y);
                });
                item.get_foreground().write(|foreground| {
                    if let Some(foreground) = foreground {
                        foreground.dispatch_draw(surface, x, y);
                    }
                });

                {
                    let canvas = surface.canvas();
                    canvas.restore();
                }
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

                    {
                        let mut target_parameter = item.get_target_parameter();
                        target_parameter.set_relative_position(relative_x, relative_y);
                        target_parameter.width = width;
                        target_parameter.height = height;
                        target_parameter.opacity = opacity;
                        target_parameter.rotation = rotation;
                        target_parameter.set_rotation_center(rotation_center_x, rotation_center_y);
                        target_parameter.set_scale(scale_x, scale_y);
                        target_parameter.set_scale_center(scale_center_x, scale_center_y);
                        target_parameter.set_offset(offset_x, offset_y);
                        target_parameter.set_skew(skew_x, skew_y);
                        target_parameter.set_skew_center(skew_center_x, skew_center_y);
                        // item.set_target_parameter(target_parameter);
                    }
                }

                item.layout(width, height);
            })),
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
            dispatch_apply_theme: Arc::new(Mutex::new(|_item: &mut Item, _theme: &Theme| {})),
            apply_theme: Arc::new(Mutex::new(|_item: &mut Item, _theme: &Theme| {})),
            dispatch_mouse_input: Arc::new(Mutex::new(|_item: &mut Item, _event: MouseEvent| {})),
            mouse_input: Arc::new(Mutex::new(|_item: &mut Item, _event: MouseEvent| {})),
            dispatch_touch_input: Arc::new(Mutex::new(|_item: &mut Item, _event: TouchEvent| {})),
            touch_input: Arc::new(Mutex::new(|_item: &mut Item, _event: TouchEvent| {})),
            on_click: Arc::new(Mutex::new(|_item: &mut Item, _source: ClickSource| {})),
            ime_input: Arc::new(Mutex::new(|_item: &mut Item, _action: ImeAction| {})),
            dispatch_keyboard_input: Arc::new(Mutex::new(|item: &mut Item, device_id: DeviceId, event: KeyEvent, is_synthetic: bool| {
                let keyboard_input = item.item_event.keyboard_input.clone();
                if keyboard_input.lock().unwrap()(item, device_id, event.clone(), is_synthetic){
                    return true;
                }
                item.get_children().items().iter_mut().any(|child| {
                    let dispatch_keyboard_input = child.item_event.dispatch_keyboard_input.clone();
                    let r = dispatch_keyboard_input.lock().unwrap()(child, device_id, event.clone(), is_synthetic);
                    r
                })
            })),
            keyboard_input: Arc::new(Mutex::new(|_item: &mut Item, _device_id: DeviceId, _event: KeyEvent, _is_synthetic: bool| false)),
            dispatch_focus: Arc::new(Mutex::new(|item: &mut Item| {
                let focus_changed_items = item.get_app_context().focus_changed_items;
                let focused = item.get_focused().get();
                {
                    let focus_changed_items = focus_changed_items.value();
                    if focus_changed_items.contains(&item.get_id()){
                        item.focus(focused)
                    }
                }
                item.get_children().items().iter_mut().for_each(|child| {
                    child.dispatch_focus();
                });
            })),
            on_focus: Arc::new(Mutex::new(|_item: &mut Item, _focused: bool| {})),
            dispatch_timer: Arc::new(Mutex::new(
                |item: &mut Item, id: usize|{
                    let timer = item.item_event.timer.clone();
                    if timer.lock().unwrap()(item, id){
                        return true;
                    }
                    item.get_children().items().iter_mut().any(|child| {
                        let dispatch_timer = child.item_event.dispatch_timer.clone();
                        let r = dispatch_timer.lock().unwrap()(child, id);
                        r
                    })
                }
            )),
            timer: Arc::new(Mutex::new(|_item: &mut Item, _id: usize| false)),
        }
    }

    /// item, canvas, parent_x, parent_y
    pub fn dispatch_draw(mut self, dispatch_draw: impl FnMut(&mut Item, &mut Surface, f32, f32) + 'static) -> Self {
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

    /// item, width, height
    pub fn layout(mut self, layout: impl FnMut(&mut Item, f32, f32) + 'static) -> Self {
        self.layout = Arc::new(Mutex::new(layout));
        self
    }

    /// Closure parameters: item, width_mode, height_mode
    ///
    /// Do not retain any state in this closure, except for the `measure_parameter`.
    /// Because this closure is used to calculate the recommended size of the item,
    /// the `layout` closure is actually responsible for setting the actual size of the item.
    pub fn measure(mut self, measure: impl FnMut(&mut Item, MeasureMode, MeasureMode) + 'static) -> Self {
        self.measure = Arc::new(Mutex::new(measure));
        self
    }

    /// item, theme
    pub fn apply_theme(mut self, apply_theme: impl FnMut(&mut Item, &Theme) + 'static) -> Self {
        self.apply_theme = Arc::new(Mutex::new(apply_theme));
        self
    }

    pub fn on_mouse_input(mut self, on_mouse_input: impl FnMut(&mut Item, MouseEvent) + 'static) -> Self {
        self.mouse_input = Arc::new(Mutex::new(on_mouse_input));
        self
    }

    pub fn on_click(mut self, on_click: impl FnMut(&mut Item, ClickSource) + 'static) -> Self {
        self.on_click = Arc::new(Mutex::new(on_click));
        self
    }

    pub fn ime_input(mut self, ime_input: impl FnMut(&mut Item, ImeAction) + 'static) -> Self {
        self.ime_input = Arc::new(Mutex::new(ime_input));
        self
    }

    pub fn keyboard_input(mut self, keyboard_input: impl FnMut(&mut Item, DeviceId, KeyEvent, bool) -> bool + 'static) -> Self {
        self.keyboard_input = Arc::new(Mutex::new(keyboard_input));
        self
    }

    pub fn on_focus(mut self, on_focus: impl FnMut(&mut Item, bool) + 'static) -> Self {
        self.on_focus = Arc::new(Mutex::new(on_focus));
        self
    }

    pub fn timer(mut self, timer: impl FnMut(&mut Item, usize) -> bool + 'static) -> Self {
        self.timer = Arc::new(Mutex::new(timer));
        self
    }
}

impl Default for ItemEvent {
    fn default() -> Self {
        Self::new()
    }
}