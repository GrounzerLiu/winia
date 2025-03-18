use crate::dpi::{LogicalPosition, LogicalSize, Position};
use crate::shared::{Children, Gettable, Observable, Settable, Shared, SharedBool, SharedColor, SharedF32, SharedText, SharedUnSend};
use crate::text::StyledText;
use crate::ui::app::AppContext;
use crate::ui::item::{
    ClickSource, HorizontalAlignment, ImeAction, LayoutDirection, LogicalX, MeasureMode,
    Orientation, PointerState, VerticalAlignment,
};
use crate::ui::Item;
use crate::{impl_property_layout, impl_property_redraw};
use proc_macro::item;
use skia_safe::textlayout::{Paragraph, TextAlign, TextStyle};
use skia_safe::{Color, Drawable, Paint, PictureRecorder, Rect, Vector};
use std::ops::{Not, Range};
use std::string::ToString;
use std::time::Duration;
use winit::dpi::Size;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key, NamedKey};

pub mod text_style {
    pub static FONT_SIZE: &str = "font_size";
    pub static COLOR: &str = "color";
}

pub struct TextProperty {
    text: SharedText,
    editable: SharedBool,
    selectable: SharedBool,
    color: SharedColor,
    font_size: SharedF32,
}

struct DrawCache {
    pub bottom: (String, Option<Drawable>),
    pub top: (String, Option<Drawable>),
}

impl DrawCache {
    pub fn new() -> Self {
        Self {
            bottom: ("alpha_0".to_string(), None),
            top: ("alpha_1".to_string(), None),
        }
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.bottom, &mut self.top);
    }
}

#[derive(Clone)]
struct TextContext {
    is_text_changed: Shared<bool>,
    paragraph: SharedUnSend<Option<Paragraph>>,
    cursor: Shared<Option<(f32, f32, f32)>>,
    show_cursor: Shared<bool>,
    composing: Shared<Option<(Range<usize>, Range<usize>)>>,
    selection: Shared<Range<usize>>,
}

impl TextContext {
    pub fn has_paragraph(&self) -> bool {
        self.paragraph.value().is_some()
    }

    pub fn check_text_changed(&mut self) {
        if self.is_text_changed.get() {
            self.paragraph.set(None);
            self.is_text_changed.set(false);
        }
    }
}

#[item(text: impl Into<SharedText>)]
pub struct Text {
    item: Item,
    property: Shared<TextProperty>,
    text_context: TextContext,
}

impl Text {
    pub fn new(app_context: AppContext, text: impl Into<SharedText>) -> Self {
        let property = Shared::new(TextProperty {
            text: text.into(),
            editable: true.into(),
            selectable: true.into(),
            color: Color::BLACK.into(),
            font_size: 24.0.into(),
        });

        let context = TextContext {
            is_text_changed: true.into(),
            paragraph: None.into(),
            cursor: None.into(),
            show_cursor: false.into(),
            composing: None.into(),
            selection: (0..0).into(),
        };

        let item = Item::new(app_context.clone(), Children::new());

        item.data()
            .set_measure({
                let property = property.clone();
                let mut context = context.clone();
                move |item, width_mode, height_mode| {
                    let property = property.value();
                    let text_style = get_text_style(&property);

                    let padding_horizontal = item.get_padding(Orientation::Horizontal);
                    let padding_vertical = item.get_padding(Orientation::Vertical);

                    // Get text layout width and height
                    context.check_text_changed();
                    let (text_layout_width, text_layout_height) = {
                        let mut text = property.text.value();
                        if !context.has_paragraph() {
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
                            let paragraph = text.create_paragraph(text_style.clone(), max_width, text_align);
                            context.paragraph.set(Some(paragraph));
                        } else {
                            let shared_paragraph = context.paragraph.clone();
                            let mut paragraph = shared_paragraph.value();
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

                        let shared_paragraph = context.paragraph.clone();
                        let paragraph = shared_paragraph.value();
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
                let mut context = context.clone();
                let property = property.clone();
                move |item, width, _height| {
                    context.check_text_changed();
                    let property = property.value();
                    let text_style = get_text_style(&property);
                    let max_width = width - item.get_padding(Orientation::Horizontal);

                    let mut text = property.text.value();

                    if !context.has_paragraph() {
                        let paragraph = text.create_paragraph(text_style.clone(), max_width, TextAlign::Justify);
                        context.paragraph.set(Some(paragraph));
                    } else {
                        let mut paragraph = context.paragraph.value();
                        paragraph.as_mut().unwrap().layout(max_width);
                    }
                }
            })
            .set_ime_input({
                let mut context = context.clone();
                let property = property.clone();
                move |item, ime_action| {
                    let mut property = property.value();
                    let mut selection = context.selection.value();
                    let mut composing = context.composing.value();
                    let text = &mut property.text;
                    match ime_action {
                        ImeAction::Enabled => {}
                        ImeAction::Enter => {
                            if selection.start != selection.end {
                                text.value().remove(selection.clone());
                                selection.end = selection.start;
                            }
                            text.value().insert(selection.start, "\n");
                            let new_index = selection.start + 1;
                            selection.start = new_index;
                            selection.end = new_index;
                        }
                        ImeAction::Delete => {
                            if selection.start != selection.end {
                                text.value().remove(selection.clone());
                                selection.end = selection.start;
                                return;
                            }

                            if selection.start == 0 {
                                return;
                            }

                            let mut text = property.text.value();

                            let glyph_index = text.byte_index_to_glyph_index(selection.start);
                            let prev_glyph_index =
                                text.glyph_index_to_byte_index(glyph_index - 1);
                            text.remove(prev_glyph_index..selection.start);
                            selection.start = prev_glyph_index;
                            selection.end = prev_glyph_index;
                        }
                        ImeAction::PreEdit(pr_text, range) => {
                            // if selection.start != selection.end {
                            //     text.write(|text| text.remove(selection.clone()));
                            //     selection.end = selection.start;
                            // }

                            if let Some((composing_range, old_selection_range)) = composing.as_ref()
                            {
                                text.value().remove(composing_range.clone());
                                selection.start = old_selection_range.start;
                                selection.end = old_selection_range.end;
                                *composing = None;
                            }

                            if let Some((start, end)) = range {
                                text.value().insert(selection.start, &pr_text);
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
                        }
                        ImeAction::Commit(commit_text) => {
                            let commit_text_len = commit_text.len();
                            if selection.start != selection.end {
                                text.value().remove(selection.clone());
                                selection.end = selection.start;
                            }
                            text.value().insert(selection.start, &commit_text);
                            let new_index = selection.start + commit_text_len;
                            selection.start = new_index;
                            selection.end = new_index;
                        }
                        ImeAction::Disabled => {}
                    }
                    context.is_text_changed.set(true);
                    item.get_app_context().request_layout();
                }
            })
            .set_draw({
                let property = property.clone();
                let mut context = context.clone();
                move |item, canvas| {
                    context.check_text_changed();
                    let property = property.value();
                    let text = property.text.value();

                    if !context.has_paragraph() {
                        return;
                    }
                    let shared_paragraph = context.paragraph.clone();                        let paragraph = shared_paragraph.value();
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
                    let show_cursor = *context.show_cursor.value();
                    let selection = context.selection.value().clone();
                    let composing = context.composing.value().clone();

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
                                    Paint::default().set_anti_alias(true).set_color(Color::BLUE),
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

                    text_layout.draw(
                        canvas,
                        paragraph_x.logical_value() + display_parameter.x(),
                        paragraph_y + display_parameter.y(),
                    );

                    if property.editable.get() && selection.start == selection.end {
                        if show_cursor {
                            if let Some((x, y, h)) =
                                text_layout.get_cursor_position(selection.start)
                            {
                                let mut x = x + display_parameter.x();
                                if x < display_parameter.x() {
                                    x = display_parameter.x();
                                }

                                if x >= display_parameter.x() + display_parameter.width - 2.0 {
                                    x = display_parameter.x() + display_parameter.width - 2.0;
                                }
                                let y = y + display_parameter.y();
                                let rect = Rect::from_xywh(x, y, 2.0, h);
                                canvas.draw_rect(
                                    &rect,
                                    Paint::default().set_anti_alias(true).set_color(0xffff0000),
                                );
                                if item.get_focused().get() {
                                    item.get_app_context().window(|window| {
                                        window.set_ime_cursor_area(
                                            Position::Logical(LogicalPosition::new(
                                                x as f64, y as f64,
                                            )),
                                            Size::Logical(LogicalSize::new(0.0, 0.0)),
                                        )
                                    });
                                }
                            }
                        }
                    }
                }
            })
            .set_keyboard_input({
                let context = context.clone();
                let property = property.clone();
                move |item, device_id, event, is_synthetic| {
                    if !property.value().editable.get() || !item.get_focused().get() {
                        return false;
                    }

                    if event.state == ElementState::Pressed {
                        match event.logical_key {
                            Key::Named(key) => match key {
                                NamedKey::Backspace => {
                                    item.ime_input(ImeAction::Delete);
                                    return true;
                                }
                                NamedKey::Enter => {
                                    item.ime_input(ImeAction::Enter);
                                    return true;
                                }
                                NamedKey::ArrowLeft => {
                                    let mut selection = context.selection.value();
                                    if selection.start > 0 {
                                        let property = property.value();
                                        property.text.read(move |text| {
                                            let glyph_index =
                                                text.byte_index_to_glyph_index(selection.start);
                                            let prev_glyph_index =
                                                text.glyph_index_to_byte_index(glyph_index - 1);
                                            selection.start = prev_glyph_index;
                                            selection.end = prev_glyph_index;
                                        });
                                    }
                                    item.get_app_context().request_layout();
                                    return true;
                                }
                                NamedKey::ArrowRight => {
                                    let mut selection = context.selection.value();
                                    let property = property.value();
                                    property.text.read(move |text| {
                                        if selection.start < text.len() {
                                            let glyph_index =
                                                text.byte_index_to_glyph_index(selection.start);
                                            let prev_glyph_index =
                                                text.glyph_index_to_byte_index(glyph_index + 1);
                                            selection.start = prev_glyph_index;
                                            selection.end = prev_glyph_index;
                                        }
                                    });
                                    return true;
                                }
                                NamedKey::Space => {
                                    item.ime_input(ImeAction::Commit(" ".to_string()));
                                    return true;
                                }
                                NamedKey::Escape => {
                                    item.focus(false);
                                }
                                _ => {}
                            },
                            Key::Character(str) => {
                                item.ime_input(ImeAction::Commit(str.to_string()));
                                return true;
                            }
                            Key::Unidentified(_) => {}
                            Key::Dead(_) => {}
                        }
                    }

                    return false;
                }
            })
            .set_mouse_input({
                let context = context.clone();
                let property = property.clone();
                move |item, event| {
                    let property = property.value();
                    if !property.editable.get() || !context.has_paragraph() {
                        return;
                    }
                    let text = property.text.value();
                    let shared_paragraph = context.paragraph.clone();                        let paragraph = shared_paragraph.value();
                    let paragraph_ref = paragraph.as_ref().unwrap();
                    let text_layout = text.get_text_layout(paragraph_ref);

                    let display_parameter = item.get_display_parameter();
                    let padding_top = item.get_padding_top().get();
                    let padding_left = item.get_padding_start().get();
                    let x = event.x - display_parameter.x() - padding_left;
                    let y = event.y - display_parameter.y() - padding_top;
                    let index = if let Some((index, _)) =
                        text_layout.get_glyph_position_at_coordinate(x, y)
                    {
                        index
                    } else {
                        return;
                    };

                    match event.pointer_state {
                        PointerState::Started => {
                            let mut selection = context.selection.value();
                            selection.start = index;
                            selection.end = index;
                            item.get_app_context().request_redraw();
                            *context.show_cursor.value() = true;
                        }
                        PointerState::Moved => {
                            let mut selection = context.selection.value();
                            selection.end = index;
                            item.get_app_context().request_redraw();
                            *context.show_cursor.value() = true;
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
                let app_context = app_context.clone();
                move |item, focused| {
                    let display_parameter = item.get_display_parameter();
                    let x = display_parameter.x() as f64;
                    let y = display_parameter.y() as f64;
                    let width = display_parameter.width as f64;
                    let height = display_parameter.height as f64;
                    let padding_top = item.get_padding_top().get() as f64;
                    let padding_left = item.get_padding_start().get() as f64;
                    app_context.window(|window| {
                        window.set_ime_allowed(focused);
                        window.set_ime_cursor_area(
                            Position::Logical(LogicalPosition::new(x - width, y)),
                            Size::Logical(LogicalSize::new(0.0, 0.0)),
                        );
                    });
                    item.get_app_context()
                        .create_timer(item.get_id(), Duration::from_millis(500));
                }
            })
            .set_timer({
                let context = context.clone();
                move |item, id| {
                    if id == item.get_id() {
                        let mut show_cursor = context
                            .show_cursor
                            .value();
                        *show_cursor = show_cursor.not();
                        item.get_app_context().request_redraw();
                        if item.get_focused().get() {
                            item.get_app_context()
                                .create_timer(item.get_id(), Duration::from_millis(500));
                        }
                    }
                    id == item.get_id()
                }
            });

        Self {
            item,
            property,
            text_context: context,
        }
    }

    pub fn text(self, text: impl Into<SharedText>) -> Self {
        {
            let id = self.item.data().get_id();
            let mut property = self.property.value();
            property.text.remove_observer(id);

            let mut text_context = self.text_context.clone();
            let event_loop_proxy = self.item.data().get_app_context().event_loop_proxy();
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
}

impl_property_redraw!(Text, color, SharedColor);
impl_property_layout!(Text, font_size, SharedF32);

fn get_text_style(property: &TextProperty) -> TextStyle {
    let color = property.color.get();
    let font_size = property.font_size.get();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    text_style.set_color(color);
    text_style
}
