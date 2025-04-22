use crate::dpi::{LogicalPosition, LogicalSize, Position};
use crate::shared::{
    Children, Gettable, Observable, Settable, Shared, SharedBool, SharedColor, SharedF32,
    SharedText, SharedUnSend,
};
use crate::text::StyledText;
use crate::ui::app::WindowContext;
use crate::ui::item::{
    ClickSource, DisplayParameter, HorizontalAlignment, ImeAction, LayoutDirection, LogicalX,
    MeasureMode, Orientation, PointerState, VerticalAlignment,
};
use crate::ui::theme::color;
use crate::ui::Item;
use crate::{impl_property_layout, impl_property_redraw};
use proc_macro::item;
use skia_safe::textlayout::{Paragraph, TextAlign, TextStyle};
use skia_safe::{Canvas, Color, Drawable, Paint, PictureRecorder, Rect, Vector};
use std::cmp::{Ordering, PartialEq};
use std::ops::{Not, Range};
use std::string::ToString;
use std::time::Duration;
use winit::dpi::Size;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key, NamedKey};
use crate::core::generate_id;

pub mod text_style {
    pub static FONT_SIZE: &str = "font_size";
    pub static COLOR: &str = "color";
}

static CONTEXT_X: &str = "context_x";
static CONTEXT_Y: &str = "context_y";

pub struct TextProperty {
    text: SharedText,
    editable: SharedBool,
    selectable: SharedBool,
    color: SharedColor,
    font_size: SharedF32,
}

struct DrawCache {
    drawables: Vec<(usize, Drawable)>,
}

impl DrawCache {
    pub fn new() -> Self {
        Self {
            drawables: Vec::new(),
        }
    }

    pub fn add(&mut self, mut drawable: Drawable, target_parameter: &mut DisplayParameter) {
        for (id, _drawable) in &self.drawables {
            target_parameter
                .set_float_param(id.to_string().as_str(), 0.0);
        }
        let id = generate_id();
        target_parameter
            .set_float_param(id.to_string().as_str(), 1.0);
        self.drawables.push((id, drawable));
    }

    pub fn draw(&mut self, canvas: &Canvas, display_parameter: &DisplayParameter) {
        for (id, drawable) in &mut self.drawables {
            let alpha = display_parameter
                .get_float_param(id.to_string().as_str())
                .unwrap_or(0.0).clamp(0.0, 1.0);
            let width = display_parameter.width;
            let height = display_parameter.height;
            canvas.save_layer_alpha_f(Rect::from_xywh(
                0.0,
                0.0,
                width,
                height,
            ), alpha);
            drawable.draw(canvas, None);
            canvas.restore();
        }
        let length = self.drawables.len();
        let mut index = 0;
        self.drawables.retain(|(id, drawable)| {
            let mut r = true;
            let alpha = display_parameter
                .get_float_param(id.to_string().as_str())
                .unwrap_or(0.0).clamp(0.0, 1.0);
            if index != length - 1 && alpha == 0.0 {
                r = false;
            }
            index += 1;
            r
        });
    }
}

#[derive(Clone)]
struct TextContext {
    is_text_changed: Shared<bool>,
    paragraph: SharedUnSend<Option<Paragraph>>,
    draw_cache: SharedUnSend<DrawCache>,
    cursor: Shared<Option<(f32, f32, f32)>>,
    show_cursor: Shared<bool>,
    composing: Shared<Option<(Range<usize>, Range<usize>)>>,
    selection: Shared<Range<usize>>,
}

#[item(text: impl Into<SharedText>)]
pub struct Text {
    item: Item,
    property: Shared<TextProperty>,
    text_context: TextContext,
}

impl Text {
    pub fn new(window_context: &WindowContext, text: impl Into<SharedText>) -> Self {
        let property = Shared::from(TextProperty {
            text: text.into(),
            editable: false.into(),
            selectable: true.into(),
            color: window_context
                .theme()
                .lock()
                .get_color(color::ON_SURFACE)
                .unwrap_or(Color::BLACK)
                .into(),
            font_size: 24.0.into(),
        });

        let context = TextContext {
            is_text_changed: true.into(),
            paragraph: None.into(),
            draw_cache: DrawCache::new().into(),
            cursor: None.into(),
            show_cursor: false.into(),
            composing: None.into(),
            selection: (0..0).into(),
        };

        let item = Item::new(window_context, Children::new());

        item.data()
            .set_measure({
                let property = property.clone();
                let context = context.clone();
                move |item, width_mode, height_mode| {
                    let property = property.lock();
                    let text_style = get_text_style(&property);

                    let padding_horizontal = item.get_padding(Orientation::Horizontal);
                    let padding_vertical = item.get_padding(Orientation::Vertical);

                    // Get text layout width and height
                    // context.check_text_changed();
                    let (text_layout_width, text_layout_height) = {
                        let mut text = property.text.lock();
                        if context.is_text_changed.get() {
                            let text_align = match item.get_layout_direction().get() {
                                LayoutDirection::LTR => TextAlign::Left,
                                LayoutDirection::RTL => TextAlign::Right,
                            };
                            let max_width = match width_mode {
                                MeasureMode::Specified(width) => {
                                    item.clamp_width(width) - padding_horizontal
                                }
                                MeasureMode::Unspecified(width) => {
                                    item.clamp_width(width) - padding_horizontal
                                }
                            };
                            let paragraph =
                                text.create_paragraph(&text_style, max_width, text_align);
                            context.paragraph.set(Some(paragraph));
                        } else {
                            let shared_paragraph = context.paragraph.clone();
                            let mut paragraph = shared_paragraph.lock();
                            let width = match width_mode {
                                MeasureMode::Specified(width) => {
                                    item.clamp_width(width) - padding_horizontal
                                }
                                MeasureMode::Unspecified(width) => {
                                    item.clamp_width(width) - padding_horizontal
                                }
                            };
                            paragraph.as_mut().unwrap().layout(width);
                        }

                        // context.is_text_changed.set(false);
                        let shared_paragraph = context.paragraph.clone();
                        let paragraph = shared_paragraph.lock();
                        let paragraph_ref = paragraph.as_ref().unwrap();
                        let text_layout = text.get_text_layout(paragraph_ref);
                        (text_layout.width(), text_layout.height())
                    };

                    let (width, height) = match width_mode {
                        MeasureMode::Specified(width) => {
                            let width = item.clamp_width(width);
                            match height_mode {
                                MeasureMode::Specified(height) => {
                                    let height = item.clamp_height(height);
                                    (width, height)
                                }
                                MeasureMode::Unspecified(_) => {
                                    let height = text_layout_height + padding_vertical;
                                    (width, height)
                                }
                            }
                        }
                        MeasureMode::Unspecified(_) => match height_mode {
                            MeasureMode::Specified(height) => {
                                let height = item.clamp_height(height);
                                (
                                    item.clamp_width(text_layout_width + padding_horizontal + 1.0),
                                    height,
                                )
                            }
                            MeasureMode::Unspecified(_) => (
                                item.clamp_width(text_layout_width + padding_horizontal + 1.0),
                                item.clamp_height(text_layout_height + padding_vertical),
                            ),
                        },
                    };

                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = width;
                    measure_parameter.height = height;
                }
            })
            .set_layout({
                let context = context.clone();
                let property = property.clone();
                let mut last_max_width = 0.0;
                move |item, width, height| {
                    // context.check_text_changed();
                    let property = property.lock();
                    let text_style = get_text_style(&property);
                    let max_width = width - item.get_padding(Orientation::Horizontal);

                    let mut text = property.text.lock();

                    let mut is_text_changed = context.is_text_changed.get();
                    if is_text_changed {
                        let paragraph =
                            text.create_paragraph(&text_style, max_width, TextAlign::Start);
                        context.is_text_changed.set(false);
                        context.paragraph.set(Some(paragraph));
                    } else if last_max_width != max_width {
                        let mut paragraph = context.paragraph.lock();
                        paragraph.as_mut().unwrap().layout(max_width);
                        is_text_changed = true;
                        last_max_width = max_width;
                    }

                    if let Some(paragraph) = context.paragraph.lock().as_ref() {
                        let text_layout = text.get_text_layout(paragraph);
                        if is_text_changed {
                            let mut recorder = PictureRecorder::new();
                            let canvas = recorder.begin_recording(
                                Rect::from_wh(text_layout.width(), text_layout.height()),
                                None,
                            );
                            text_layout.draw(
                                canvas,
                                0.0,
                                0.0,
                            );
                            let picture = recorder.finish_recording_as_drawable().unwrap();
                            let mut draw_cache = context.draw_cache.lock();
                            draw_cache.add(picture, item.get_target_parameter());
                        }

                        // let mut x = LogicalX::new(item.get_layout_direction().get(), 0.0, width);
                        // let mut y = 0.0;
                        let align_content = item.get_align_content().get();
                        let x = match align_content.to_horizontal_alignment() {
                            HorizontalAlignment::Start => item.get_padding_start().get(),
                            HorizontalAlignment::Center => (width - text_layout.width()) / 2.0,
                            HorizontalAlignment::End => width - text_layout.width() - item.get_padding_end().get(),
                        };
                        let y = match align_content.to_vertical_alignment() {
                            VerticalAlignment::Top => item.get_padding_top().get(),
                            VerticalAlignment::Center => (height - text_layout.height()) / 2.0,
                            VerticalAlignment::Bottom => height - text_layout.height() - item.get_padding_bottom().get(),
                        };
                        let x = LogicalX::new(item.get_layout_direction().get(), x, width);
                        let target_parameter = item.get_target_parameter();
                        target_parameter.set_float_param(CONTEXT_X, x.physical_value(text_layout.width()));
                        target_parameter.set_float_param(CONTEXT_Y, y);
                        
                        item.set_base_line(text_layout.base_line());
                    }
                }
            })
            .set_ime_input({
                let context = context.clone();
                let property = property.clone();
                move |item, ime_action| {
                    let property = property.lock();
                    let mut selection = context.selection.lock();
                    let mut composing = context.composing.lock();
                    // let text = property.text.clone();
                    match ime_action {
                        ImeAction::Enabled => {}
                        ImeAction::Enter => {
                            if selection.start != selection.end {
                                property.text.lock().remove(selection.clone());
                                selection.end = selection.start;
                            }
                            property.text.lock().insert_str(selection.start, "\n");
                            let new_index = selection.start + 1;
                            selection.start = new_index;
                            selection.end = new_index;

                            property.text.notify();
                        }
                        ImeAction::Delete => {
                            if selection.start != selection.end {
                                property.text.lock().remove(selection.clone());
                                selection.end = selection.start;
                                property.text.notify();
                                return;
                            }

                            if selection.start == 0 {
                                return;
                            }

                            let mut text = property.text.lock();

                            let glyph_index = text.byte_index_to_glyph_index(selection.start);
                            let prev_glyph_index = text.glyph_index_to_byte_index(glyph_index - 1);
                            text.remove(prev_glyph_index..selection.start);
                            selection.start = prev_glyph_index;
                            selection.end = prev_glyph_index;
                            drop(text);
                            property.text.notify();
                        }
                        ImeAction::PreEdit(pr_text, range) => {
                            // if selection.start != selection.end {
                            //     text.write(|text| text.remove(selection.clone()));
                            //     selection.end = selection.start;
                            // }

                            if let Some((composing_range, old_selection_range)) = composing.as_ref()
                            {
                                property.text.lock().remove(composing_range.clone());
                                selection.start = old_selection_range.start;
                                selection.end = old_selection_range.end;
                                *composing = None;
                            }

                            if let Some((start, end)) = range {
                                property.text.lock().insert_str(selection.start, pr_text);
                                *composing = Some((
                                    selection.start..(selection.start + pr_text.len()),
                                    selection.clone(),
                                ));
                                //self.composing = Some((self.selection_range.start..(self.selection_range.start + pr_text.len()), self.selection_range.clone()));
                                let new_selection_start = selection.start + start;
                                let new_selection_end = selection.start + end;
                                selection.start = new_selection_start;
                                selection.end = new_selection_start;
                            }
                            property.text.notify();
                        }
                        ImeAction::Commit(commit_text) => {
                            let commit_text_len = commit_text.len();
                            if selection.start != selection.end {
                                property.text.lock().remove(selection.clone());
                                selection.end = selection.start;
                            }
                            property.text.lock().insert_str(selection.start, &commit_text);
                            let new_index = selection.start + commit_text_len;
                            selection.start = new_index;
                            selection.end = new_index;
                            property.text.notify();
                        }
                        ImeAction::Disabled => {}
                    }
                    // context.is_text_changed.set(true);
                    // text.notify();
                    // item.get_window_context().request_layout();
                }
            })
            .set_draw({
                let property = property.clone();
                let mut context = context.clone();
                move |item, canvas| {
                    // context.check_text_changed();
                    let property = property.lock();
                    let mut text = property.text.lock();

                    if context.paragraph.lock().is_none() {
                        return;
                    }

                    let shared_paragraph = context.paragraph.clone();
                    let paragraph = shared_paragraph.lock();
                    let paragraph_ref = paragraph.as_ref().unwrap();
                    let text_layout = text.get_text_layout(paragraph_ref);

                    let layout_direction = item.get_layout_direction().get();
                    let align_content = item.get_align_content().get();
                    let padding_start = item.get_padding_start().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_bottom = item.get_padding_bottom().get();

                    let display_parameter = item.get_display_parameter();
                    let width = display_parameter.width;
                    let height = display_parameter.height;
                    let show_cursor = *context.show_cursor.lock();
                    let selection = context.selection.lock().clone();
                    let composing = context.composing.lock().clone();

                    let text_layout_width = text_layout.width();
                    let text_layout_height = text_layout.height();

                    let paragraph_x = LogicalX::new(
                        layout_direction,
                        match align_content.to_horizontal_alignment() {
                            HorizontalAlignment::Start => padding_start,
                            HorizontalAlignment::Center => (width - text_layout_width) / 2.0,
                            HorizontalAlignment::End => width - text_layout_width - padding_end,
                        },
                        width,
                    );

                    let paragraph_y = match align_content.to_vertical_alignment() {
                        VerticalAlignment::Top => padding_top,
                        VerticalAlignment::Center => (height - text_layout_height) / 2.0,
                        VerticalAlignment::Bottom => height - text_layout_height - padding_bottom,
                    };
                    if !context.is_text_changed.get() {
                        if selection.start != selection.end {
                            text_layout
                                .get_rects_for_range(selection.clone())
                                .iter()
                                .for_each(|text_box| {
                                    let rect = text_box.rect;
                                    let x = paragraph_x + rect.x();
                                    let y = paragraph_y + rect.y();
                                    let w = rect.width();
                                    let h = rect.height();
                                    let rect = Rect::from_xywh(
                                        x.logical_value() + display_parameter.x(),
                                        y + display_parameter.y(),
                                        w,
                                        h,
                                    );
                                    canvas.draw_rect(
                                        rect,
                                        Paint::default()
                                            .set_anti_alias(true)
                                            .set_color(Color::BLUE),
                                    );
                                });
                        }

                        if let Some((composing_range, _)) = composing {
                            text_layout
                                .get_rects_for_range(composing_range.clone())
                                .iter()
                                .for_each(|text_box| {
                                    let rect = text_box.rect;
                                    let x = paragraph_x + rect.x();
                                    let y = paragraph_y + rect.bottom;
                                    let w = rect.width();
                                    let h = 1.0;
                                    let rect = Rect::from_xywh(
                                        x.logical_value() + display_parameter.x(),
                                        y + display_parameter.y(),
                                        w,
                                        h,
                                    );
                                    canvas.draw_rect(
                                        rect,
                                        Paint::default().set_anti_alias(true).set_color(Color::RED),
                                    );
                                });
                        }
                    }

                    // text_layout.draw(
                    //     canvas,
                    //     paragraph_x.logical_value() + display_parameter.x(),
                    //     paragraph_y + display_parameter.y(),
                    // );
                    {
                        let x = display_parameter.x();
                        let y = display_parameter.y();
                        let context_x = display_parameter
                            .get_float_param(CONTEXT_X)
                            .unwrap_or(0.0) + x;
                        let context_y = display_parameter
                            .get_float_param(CONTEXT_Y)
                            .unwrap_or(0.0) + y;
                        canvas.save();
                        canvas.translate((context_x, context_y));
                        let mut draw_cache = context.draw_cache.lock();
                        draw_cache.draw(canvas, display_parameter.as_ref());
                        canvas.restore();
                    }

                    if !context.is_text_changed.get()
                        && property.editable.get()
                        && selection.start == selection.end
                        && show_cursor
                        && item.get_focused().get()
                    {
                        if let Some((x, y, h)) = text_layout.get_cursor_position(selection.start) {
                            let context_x = display_parameter
                                .get_float_param(CONTEXT_X)
                                .unwrap_or(0.0);
                            let context_y = display_parameter
                                .get_float_param(CONTEXT_Y)
                                .unwrap_or(0.0);
                            let mut x = x + display_parameter.x() + context_x;

                            if x < display_parameter.x() {
                                x = display_parameter.x();
                            }

                            if x >= display_parameter.x() + display_parameter.width - 2.0 {
                                x = display_parameter.x() + display_parameter.width - 2.0;
                            }
                            let y = y + display_parameter.y() + context_y;
                            let rect = Rect::from_xywh(x, y, 2.0, h);
                            canvas.draw_rect(
                                rect,
                                Paint::default().set_anti_alias(true).set_color(0xffff0000),
                            );
                            if item.get_focused().get() {
                                item.get_window_context().window.lock().set_ime_cursor_area(
                                    Position::Logical(LogicalPosition::new(x as f64, y as f64)),
                                    Size::Logical(LogicalSize::new(0.0, 0.0)),
                                )
                            }
                        }
                    }
                }
            })
            .set_keyboard_input({
                let context = context.clone();
                let property = property.clone();
                move |item, keyboard_input| {
                    if !property.lock().editable.get() || !item.get_focused().get() {
                        return false;
                    }
                    let event = &keyboard_input.key_event;
                    if event.state == ElementState::Pressed {
                        match &event.logical_key {
                            Key::Named(key) => match key {
                                NamedKey::Backspace => {
                                    item.ime_input(&ImeAction::Delete);
                                    return true;
                                }
                                NamedKey::Enter => {
                                    item.ime_input(&ImeAction::Enter);
                                    return true;
                                }
                                NamedKey::ArrowLeft => {
                                    let mut selection = context.selection.lock();
                                    if selection.start > 0 {
                                        let property = property.lock();
                                        {
                                            let mut text = property.text.lock();
                                            let glyph_index =
                                                text.byte_index_to_glyph_index(selection.start);
                                            let prev_glyph_index =
                                                text.glyph_index_to_byte_index(glyph_index - 1);
                                            selection.start = prev_glyph_index;
                                            selection.end = prev_glyph_index;
                                        }
                                        property.text.notify();
                                    }
                                    return true;
                                }
                                NamedKey::ArrowRight => {
                                    let mut selection = context.selection.lock();
                                    let property = property.lock();
                                    let mut text = property.text.lock();
                                    if selection.start < text.len() {
                                        let glyph_index =
                                            text.byte_index_to_glyph_index(selection.start);
                                        let prev_glyph_index =
                                            text.glyph_index_to_byte_index(glyph_index + 1);
                                        selection.start = prev_glyph_index;
                                        selection.end = prev_glyph_index;
                                        drop(text);
                                        property.text.notify();
                                    }
                                    return true;
                                }
                                NamedKey::Space => {
                                    item.ime_input(&ImeAction::Commit(" ".to_string()));
                                    return true;
                                }
                                NamedKey::Escape => {
                                    item.focus(false);
                                }
                                _ => {}
                            },
                            Key::Character(str) => {
                                item.ime_input(&ImeAction::Commit(str.to_string()));
                                return true;
                            }
                            Key::Unidentified(_) => {}
                            Key::Dead(_) => {}
                        }
                    }

                    false
                }
            })
            .set_mouse_input({
                let context = context.clone();
                let property = property.clone();
                let mut start_index = 0_usize;
                move |item, event| {
                    let property = property.lock();
                    if !property.editable.get() || context.paragraph.lock().is_none() {
                        return;
                    }
                    item.get_focused().set(true);
                    let mut text = property.text.lock();
                    let shared_paragraph = context.paragraph.clone();
                    let paragraph = shared_paragraph.lock();
                    let paragraph_ref = paragraph.as_ref().unwrap();
                    let text_layout = text.get_text_layout(paragraph_ref);

                    let display_parameter = item.get_display_parameter();
                    let context_x = display_parameter
                        .get_float_param(CONTEXT_X)
                        .unwrap_or(0.0);
                    let context_y = display_parameter
                        .get_float_param(CONTEXT_Y)
                        .unwrap_or(0.0);
                    let x = event.x - display_parameter.x() - context_x;
                    let y = event.y - display_parameter.y() - context_y;
                    let index = if let Some((index, _)) =
                        text_layout.get_glyph_position_at_coordinate(x, y)
                    {
                        index
                    } else {
                        return;
                    };

                    match event.pointer_state {
                        PointerState::Started => {
                            let mut selection = context.selection.lock();
                            start_index = index;
                            selection.start = index;
                            selection.end = index;
                            item.get_window_context().request_redraw();
                            *context.show_cursor.lock() = true;
                        }
                        PointerState::Moved => {
                            let mut selection = context.selection.lock();
                            // selection.end = index;
                            match index.cmp(&start_index) {
                                Ordering::Less => {
                                    selection.start = index;
                                    selection.end = start_index;
                                }
                                Ordering::Greater => {
                                    selection.start = start_index;
                                    selection.end = index;
                                }
                                _=> {}
                            }

                            item.get_window_context().request_redraw();
                            *context.show_cursor.lock() = true;
                        }
                        PointerState::Ended => {}
                        PointerState::Cancelled => {}
                    }
                }
            })
            .set_click_event(|item, click_source| {
                if click_source == ClickSource::Mouse(MouseButton::Left)
                    || click_source == ClickSource::Touch
                {
                    item.get_focused().set(true);
                }
            })
            .set_focus_event({
                let window_context = window_context.clone();
                move |item, focused| {
                    let display_parameter = item.get_display_parameter();
                    let x = display_parameter.x() as f64;
                    let y = display_parameter.y() as f64;
                    let width = display_parameter.width as f64;
                    let height = display_parameter.height as f64;
                    let padding_top = item.get_padding_top().get() as f64;
                    let padding_left = item.get_padding_start().get() as f64;
                    window_context.set_ime_allowed(item.get_id(), focused);
                    window_context.window().set_ime_cursor_area(
                        Position::Logical(LogicalPosition::new(x - width, y)),
                        Size::Logical(LogicalSize::new(0.0, 0.0)),
                    );
                    item.get_window_context()
                        .create_timer(item.get_id(), Duration::from_millis(500));
                }
            })
            .set_timer({
                let context = context.clone();
                move |item, id| {
                    if id == item.get_id() {
                        let mut show_cursor = context.show_cursor.lock();
                        *show_cursor = show_cursor.not();
                        item.get_window_context().request_redraw();
                        if item.get_focused().get() {
                            item.get_window_context()
                                .create_timer(item.get_id(), Duration::from_millis(500));
                        }
                    }
                    id == item.get_id()
                }
            });

        {
            let id = item.data().get_id();
            let property = property.lock();

            let text_context = context.clone();
            let event_loop_proxy = item.data().get_window_context().event_loop_proxy().clone();
            property
                .text
                .add_specific_observer(id, move |_text: &mut StyledText| {
                    // println!("Text changed: {}", text);
                    text_context.is_text_changed.set(true);
                    event_loop_proxy.request_layout();
                });
        }

        Self {
            item,
            property,
            text_context: context,
        }
    }

    pub fn text(self, text: impl Into<SharedText>) -> Self {
        {
            let id = self.item.data().get_id();
            let mut property = self.property.lock();
            property.text.remove_observer(id);

            let text_context = self.text_context.clone();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            property.text = text.into();
            property.text.add_specific_observer(
                id,
                Box::new(move |_text: &mut StyledText| {
                    text_context.is_text_changed.set(true);
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn color(self, color: impl Into<SharedColor>) -> Self {
        {
            let id = self.item.data().get_id();
            let mut property = self.property.lock();
            property.color.remove_observer(id);

            let text_context = self.text_context.clone();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            property.color = color.into();
            property.color.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_redraw();
                    text_context.is_text_changed.set(true);
                }),
            );
        }
        self
    }

    pub fn font_size(self, font_size: impl Into<SharedF32>) -> Self {
        {
            let id = self.item.data().get_id();
            let mut property = self.property.lock();
            property.font_size.remove_observer(id);

            let text_context = self.text_context.clone();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            property.font_size = font_size.into();
            property.font_size.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_redraw();
                    text_context.is_text_changed.set(true);
                }),
            );
        }
        self
    }
}

// impl_property_layout!(Text, color, SharedColor);
// impl_property_layout!(Text, font_size, SharedF32);
impl_property_redraw!(Text, editable, SharedBool);

fn get_text_style(property: &TextProperty) -> TextStyle {
    let color = property.color.get();
    let font_size = property.font_size.get();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    text_style.set_color(color);
    text_style
}
