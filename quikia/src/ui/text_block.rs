use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use skia_safe::{Color, Paint, Point, Rect};
use skia_safe::textlayout::TextAlign;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::keyboard::{Key, NamedKey};
use crate::app::{SharedApp, ThemeColor};
use crate::ui::{Gravity, ImeAction, Item, ItemEvent, LayoutDirection, MeasureMode, PointerAction};
use crate::ui::additional_property::BaseLine;
use crate::property::{BoolProperty, ColorProperty, FloatProperty, Gettable, Observable, Observer, SharedProperty, TextProperty};
use crate::text::{EdgeBehavior, ParagraphWrapper, Style, StyledText};

pub struct TextBlockProperties {
    text: TextProperty,
    editable: BoolProperty,
    color: ColorProperty,
    size: FloatProperty,
}

pub struct TextBlock {
    item: Item,
    properties: Arc<Mutex<TextBlockProperties>>,
}

impl TextBlock {
    pub fn new(app: SharedApp) -> Self {
        let properties = Arc::new(Mutex::new(TextBlockProperties {
            text: TextProperty::from_value(StyledText::from_str("")),
            editable: BoolProperty::from_value(true),
            color: Color::BLACK.into(),
            size: 14.0.into(),
        }));

        let paragraph: SharedProperty<Option<ParagraphWrapper>> = SharedProperty::from_value(None);
        let show_cursor: SharedProperty<bool> = SharedProperty::from_value(true);
        let composing: SharedProperty<Option<(Range<usize>, Range<usize>)>> = SharedProperty::from_value(None);
        let selection: SharedProperty<Range<usize>> = SharedProperty::from_value(0..0);

        let item = Item::new(
            app,
            ItemEvent::default()
                .set_on_draw(
                    {
                        let paragraph = paragraph.clone();
                        let show_cursor = show_cursor.clone();
                        let composing = composing.clone();
                        let selection = selection.clone();

                        let properties = properties.clone();

                        move |item, canvas| {
                            let layout_params = item.get_layout_params();

                            if let Some(background) = item.get_background().lock().as_mut() {
                                background.draw(canvas);
                            }

                            let paragraph_guard = paragraph.lock();
                            if let Some(paragraph) = paragraph_guard.as_ref() {

                                // Draw selection
                                if selection.get().start != selection.get().end {
                                    paragraph.get_rects_for_range(selection.get().clone()).iter().for_each(|text_box| {
                                        let rect = text_box.rect;
                                        let rect = Rect::from_xywh(rect.left + layout_params.x, rect.top + layout_params.y, rect.width(), rect.height());
                                        canvas.draw_rect(&rect, Paint::default().set_anti_alias(true).set_color(0x7f0000ff));
                                    });
                                }

                                let horizontal_gravity = item.get_horizontal_gravity().get();
                                let vertical_gravity = item.get_vertical_gravity().get();

                                let paragraph_x = match item.get_layout_direction().get() {
                                    LayoutDirection::LeftToRight => {
                                        match horizontal_gravity {
                                            Gravity::Start => {
                                                layout_params.x + layout_params.padding_start
                                            }
                                            Gravity::Center => {
                                                layout_params.x + (layout_params.width - paragraph.layout_width()) / 2.0
                                            }
                                            Gravity::End => {
                                                layout_params.x + layout_params.width - layout_params.padding_end - paragraph.layout_width()
                                            }
                                        }
                                    }
                                    LayoutDirection::RightToLeft => {
                                        match horizontal_gravity {
                                            Gravity::Start => {
                                                layout_params.x - layout_params.padding_start
                                            }
                                            Gravity::Center => {
                                                layout_params.x - (layout_params.width - paragraph.layout_width()) / 2.0
                                            }
                                            Gravity::End => {
                                                layout_params.x - layout_params.width + layout_params.padding_end + paragraph.layout_width()
                                            }
                                        }
                                    }
                                };

                                let paragraph_y = match vertical_gravity {
                                    Gravity::Start => {
                                        layout_params.y + layout_params.padding_top
                                    }
                                    Gravity::Center => {
                                        layout_params.y + (layout_params.height - paragraph.layout_height()) / 2.0
                                    }
                                    Gravity::End => {
                                        layout_params.y + layout_params.height - layout_params.padding_bottom - paragraph.layout_height()
                                    }
                                };

                                // Draw text
                                //paragraph.draw(canvas, layout_params.x+layout_params.padding_start, layout_params.y+layout_params.padding_top);
                                paragraph.draw(canvas, paragraph_x, paragraph_y);

                                // Draw the underline for composing text
                                if let Some((composing_range, _)) = composing.get() {
                                    let color = properties.lock().unwrap().color.get();
                                    for text_box in paragraph.get_rects_for_range(composing_range.clone()).iter()
                                    {
                                        let rect = text_box.rect;
                                        let rect = Rect::from_xywh(rect.left + layout_params.x, rect.bottom + layout_params.y, rect.width(), 1.0);
                                        canvas.draw_rect(&rect, Paint::default().set_anti_alias(true).set_color(color));
                                    };
                                }

                                if properties.lock().unwrap().editable.get() && selection.get().start == selection.get().end{
                                    if show_cursor.get() {
                                        let (x, y, h) = paragraph.get_cursor_position(selection.get().start);
                                        let mut x = x + layout_params.x;
                                        if x < layout_params.x {
                                            x = layout_params.x;
                                        }

                                        if x >= layout_params.x + layout_params.width - 2.0 {
                                            x = layout_params.x + layout_params.width - 2.0;
                                        }
                                        let y = y + layout_params.y;
                                        let rect = Rect::from_xywh(x, y, 2.0, h);
                                        canvas.draw_rect(&rect, Paint::default().set_anti_alias(true).set_color(0xffff0000));
                                        item.get_app().lock().unwrap().window().set_ime_cursor_area(LogicalPosition::new(x, y + h), LogicalSize::new(0, 0));
                                    }
                                }
                            }

                            if let Some(foreground) = item.get_foreground().lock().as_mut() {
                                foreground.draw(canvas);
                            }
                        }
                    }
                )

                .set_on_measure(
                    {
                        let mut paragraph = paragraph.clone();
                        let properties = properties.clone();
                        move |item, width_measure_mode, height_measure_mode| {
                            let mut layout_params = item.get_layout_params().clone();
                            layout_params.init_from_item(item);

                            let max_width = item.get_max_width().get();
                            let min_width = item.get_min_width().get();
                            let max_height = item.get_max_height().get();
                            let min_height = item.get_min_height().get();

                            let mut new_paragraph = None;

                            let properties_guard = properties.lock().unwrap();

                            let mut text = &properties_guard.text;
                            let mut text_guard = text.lock();
                            let text_ref = text_guard.as_mut();

                            let text_color = properties_guard.color.get();
                            let text_size = properties_guard.size.get();
                            text_ref.set_style(Style::TextColor(text_color), 0..text_ref.len(), EdgeBehavior::IncludeAndInclude);
                            text_ref.set_style(Style::FontSize(text_size), 0..text_ref.len(), EdgeBehavior::IncludeAndInclude);

                            //text_ref.clear_styles();
                            //text_ref.set_style(Style::TextColor(properties.lock().unwrap().color.get()), 0..text_ref.len(),EdgeBehavior::IncludeAndInclude);

                            match width_measure_mode {
                                MeasureMode::Specified(width) => {
                                    layout_params.width = width.max(item.get_min_width().get());
                                    new_paragraph = Some(ParagraphWrapper::new(text_ref, 0..text_ref.len(), width,
                                                                               match item.get_layout_direction().get() {
                                                                                   LayoutDirection::LeftToRight => {
                                                                                       TextAlign::Left
                                                                                   }
                                                                                   LayoutDirection::RightToLeft => {
                                                                                       TextAlign::Right
                                                                                   }
                                                                               }));
                                }
                                MeasureMode::Unspecified(width) => {
                                    new_paragraph = Some(ParagraphWrapper::new(text_ref, 0..text_ref.len(), width,
                                                                               match item.get_layout_direction().get() {
                                                                                   LayoutDirection::LeftToRight => {
                                                                                       TextAlign::Left
                                                                                   }
                                                                                   LayoutDirection::RightToLeft => {
                                                                                       TextAlign::Right
                                                                                   }
                                                                               }));
                                    let expected_width = new_paragraph.as_ref().unwrap().layout_width() + 1.0 + layout_params.padding_start + layout_params.padding_end;
                                    layout_params.width = expected_width.min(max_width).max(min_width);
                                    new_paragraph = Some(ParagraphWrapper::new(text_ref, 0..text_ref.len(), layout_params.width,
                                                                               match item.get_layout_direction().get() {
                                                                                   LayoutDirection::LeftToRight => {
                                                                                       TextAlign::Left
                                                                                   }
                                                                                   LayoutDirection::RightToLeft => {
                                                                                       TextAlign::Right
                                                                                   }
                                                                               }));
                                }
                            }

                            match height_measure_mode {
                                MeasureMode::Specified(height) => {
                                    layout_params.height = height.min(max_height).max(min_height);
                                }
                                MeasureMode::Unspecified(height) => {
                                    if let Some(paragraph) = &new_paragraph {
                                        layout_params.height = paragraph.layout_height().min(max_height).max(min_height);
                                    } else {
                                        layout_params.height = min_height
                                    }
                                }
                            }

                            item.set_layout_params(&layout_params);
                            if let Some(paragraph) = &new_paragraph {
                                item.set_baseline(paragraph.base_line())
                            }
                            paragraph.set_value(new_paragraph);

                            if let Some(background) = item.get_background().lock().as_mut() {
                                background.measure(MeasureMode::Specified(layout_params.width), MeasureMode::Specified(layout_params.height));
                            }

                            if let Some(foreground) = item.get_foreground().lock().as_mut() {
                                foreground.measure(MeasureMode::Specified(layout_params.width), MeasureMode::Specified(layout_params.height));
                            }
                        }
                    }
                )

                .set_on_pointer_input(
                    {
                        let properties = properties.clone();
                        let paragraph = paragraph.clone();
                        let composing = composing.clone();
                        let selection = selection.clone();
                        let show_cursor = show_cursor.clone();
                        move |item, pointer_action| {
                            if !properties.lock().unwrap().editable.get() {
                                return false;
                            }
                            if let Some(paragraph) = paragraph.lock().as_ref() {
                                let layout_params = item.get_layout_params();
                                match pointer_action {
                                    PointerAction::Down { x, y, pointer_type } => {
                                        if item.get_focusable().get() && item.get_focusable_when_clicked().get() {
                                            item.get_app().request_focus(item.get_id());
                                            item.get_app().activate_ime();
                                        }

                                        let x = x - layout_params.x - layout_params.padding_start;
                                        let y = y - layout_params.y - layout_params.padding_top;
                                        let index = paragraph.get_closest_glyph_cluster_at(Point::new(x, y));
                                        selection.set_value(index..index);

                                        item.get_app().request_redraw();


                                        //self.selection_start_when_drag = Some(self.selection_range.start);
                                    }
                                    PointerAction::Move { x, y, .. } => {
                                        // if let Some(start) = self.selection_start_when_drag {
                                        //     let x = x - self.layout_params.x - self.layout_params.padding_start;
                                        //     let y = y - self.layout_params.y - self.layout_params.padding_top;
                                        //     let index = paragraph.get_closest_glyph_cluster_at(Point::new(x, y));
                                        //     if index > start {
                                        //         self.selection_range.start = start;
                                        //         self.selection_range.end = index;
                                        //     } else if index < start {
                                        //         self.selection_range.start = index;
                                        //         self.selection_range.end = start;
                                        //     }
                                        // }
                                        // self.app.request_redraw();
                                    }
                                    _ => {
                                        //self.selection_start_when_drag = None;
                                    }
                                }
                                return true;
                            }

                            true
                        }
                    }
                )

                .set_on_ime_input(
                    {
                        let properties = properties.clone();
                        let paragraph = paragraph.clone();
                        let composing = composing.clone();
                        let selection = selection.clone();
                        let show_cursor = show_cursor.clone();
                        move |item, ime_action| {
                            if !item.get_focusable().get() {
                                return false;
                            }
                            if paragraph.lock().is_none() {
                                return true;
                            }
                            if !properties.lock().unwrap().editable.get() {
                                return true;
                            }
                            let paragraph_guard = paragraph.lock();
                            let paragraph_ref = paragraph_guard.as_ref();
                            let paragraph = paragraph_ref.as_ref().unwrap();
                            let mut text_clone = properties.lock().unwrap().text.clone();
                            let mut text = text_clone.lock();
                            match ime_action {
                                ImeAction::Enabled => {}
                                ImeAction::Enter => {
                                    if selection.get().start != selection.get().end {
                                        text.as_mut().remove(selection.get().clone());
                                        selection.set_value(selection.get().start..selection.get().start);
                                    }
                                    text.as_mut().insert(selection.get().start, "\n");
                                    let new_index = selection.get().start + 1;
                                    selection.set_value(new_index..new_index);
                                }
                                ImeAction::Delete => {
                                    let text_mut = text.as_mut();

                                    let selection_range = selection.get().clone();
                                    if selection_range.start != selection_range.end {
                                        text_mut.remove(selection_range.clone());
                                        selection.set_value(selection_range.start..selection_range.start);
                                        return true;
                                    }

                                    if selection_range.start == 0 {
                                        return true;
                                    }

                                    let glyph_index = paragraph.byte_index_to_glyph_index(selection_range.start);
                                    let prev_glyph_index = paragraph.glyph_index_to_byte_index(glyph_index - 1);
                                    text_mut.remove(prev_glyph_index..selection_range.start);
                                    selection.set_value(prev_glyph_index..prev_glyph_index);
                                }
                                ImeAction::Preedit(pr_text, range) => {
                                    let selection_range = selection.get().clone();
                                    if selection_range.start != selection_range.end {
                                        text.as_mut().remove(selection_range.clone());
                                        selection.set_value(selection_range.start..selection_range.start);
                                    }

                                    if let Some((composing_range, old_selection_range)) = composing.get() {
                                        text.as_mut().remove(composing_range.clone());
                                        selection.set_value(old_selection_range.clone());
                                        composing.set_value(None);
                                    }

                                    if let Some((start, end)) = range {
                                        text.as_mut().insert(selection.get().start, &pr_text);
                                        composing.set_value(Some((selection.get().start..(selection.get().start + pr_text.len()), selection.get().clone())));
                                        //self.composing = Some((self.selection_range.start..(self.selection_range.start + pr_text.len()), self.selection_range.clone()));
                                        let new_selection_start = selection.get().start + start;
                                        let new_selection_end = selection.get().start + end;
                                        selection.set_value(new_selection_start..new_selection_end);
                                    }
                                }
                                ImeAction::Commit(commit_text) => {
                                    let commit_text_len = commit_text.len();
                                    if selection.get().start != selection.get().end {
                                        text.as_mut().remove(selection.get().clone());
                                        selection.set_value(selection.get().start..selection.get().start);
                                    }
                                    text.as_mut().insert(selection.get().start, &commit_text);
                                    let new_index = selection.get().start + commit_text_len;
                                    selection.set_value(new_index..new_index);
                                }
                                ImeAction::Disabled => {}
                            }
                            item.get_app().request_layout();
                            true
                        }
                    }
                )

                .set_on_keyboard_input(
                    {
                        let properties = properties.clone();
                        let paragraph = paragraph.clone();
                        let composing = composing.clone();
                        let selection = selection.clone();
                        let show_cursor = show_cursor.clone();
                        move |item, device_id, key_event, is_synthetic| {
                            if !item.get_focusable().get() {
                                return false;
                            }
                            if paragraph.lock().is_none() {
                                return true;
                            }
                            if let Some(paragraph) = paragraph.lock().as_ref() {
                                let layout_params = item.get_layout_params();
                                let mut text_clone = properties.lock().unwrap().text.clone();
                                let mut text = text_clone.lock();
                                if key_event.state == winit::event::ElementState::Pressed {
                                    match key_event.logical_key {
                                        Key::Named(named_key) => {
                                            match named_key {
                                                NamedKey::ArrowLeft => {
                                                    if selection.get().start > 0 {
                                                        let glyph_index = paragraph.byte_index_to_glyph_index(selection.get().start);
                                                        let prev_glyph_index = paragraph.glyph_index_to_byte_index(glyph_index - 1);
                                                        selection.set_value(prev_glyph_index..prev_glyph_index);
                                                    }
                                                }
                                                NamedKey::ArrowRight => {
                                                    if selection.get().start < text.len() {
                                                        let glyph_index = paragraph.byte_index_to_glyph_index(selection.get().start);
                                                        let next_glyph_index = paragraph.glyph_index_to_byte_index(glyph_index + 1);
                                                        selection.set_value(next_glyph_index..next_glyph_index);
                                                    }
                                                }
                                                NamedKey::Backspace => {
                                                    let text_mut = text.as_mut();

                                                    let selection_range = selection.get().clone();
                                                    if selection_range.start != selection_range.end {
                                                        text_mut.remove(selection_range.clone());
                                                        selection.set_value(selection_range.start..selection_range.start);
                                                        return true;
                                                    }

                                                    if selection_range.start == 0 {
                                                        return true;
                                                    }

                                                    let glyph_index = paragraph.byte_index_to_glyph_index(selection_range.start);
                                                    let prev_glyph_index = paragraph.glyph_index_to_byte_index(glyph_index - 1);
                                                    text_mut.remove(prev_glyph_index..selection_range.start);
                                                    selection.set_value(prev_glyph_index..prev_glyph_index);
                                                }
                                                NamedKey::Enter => {
                                                    if selection.get().start != selection.get().end {
                                                        text.as_mut().remove(selection.get().clone());
                                                        selection.set_value(selection.get().start..selection.get().start);
                                                    }
                                                    text.as_mut().insert(selection.get().start, "\n");
                                                    let new_index = selection.get().start + 1;
                                                    selection.set_value(new_index..new_index);
                                                }
                                                _ => {}
                                            }
                                        }
                                        Key::Character(smol_str) => {
                                            let string = smol_str.to_string();
                                            if selection.get().start != selection.get().end {
                                                text.as_mut().remove(selection.get().clone());
                                                selection.set_value(selection.get().start..selection.get().start);
                                            }
                                            text.as_mut().insert(selection.get().start, &string);
                                            let new_index = selection.get().start + string.len();
                                            selection.set_value(new_index..new_index);
                                        }
                                        Key::Unidentified(_) => {}
                                        Key::Dead(_) => {}
                                    }
                                }
                            }
                            true
                        }
                    })
        );
        {
            let app = item.get_app();
            let text = properties.lock().unwrap().text.clone();
            let mut text_guard = text.lock();
            let len = text_guard.len();
            text_guard.set_style(Style::TextColor(app.lock().unwrap().theme().get_color(ThemeColor::OnSurfaceVariant)), 0..len, EdgeBehavior::IncludeAndInclude);
        }
        TextBlock {
            item,
            properties,
        }
    }

    pub fn text(mut self, text: impl Into<TextProperty>) -> Self {
        let text = text.into();
        let app = self.item.get_app();
        text.add_observer(
            Observer::new_without_id(
                move || {
                    app.lock().unwrap().request_layout();
                }
            )
        );
        self.properties.lock().unwrap().text = text;
        self
    }

    pub fn color(mut self, color: impl Into<ColorProperty>) -> Self {
        let color = color.into();
        let app = self.item.get_app();
        color.add_observer(
            Observer::new_without_id(
                move || {
                    app.lock().unwrap().request_layout();
                }
            )
        );
        self.properties.lock().unwrap().color = color;
        self
    }

    pub fn editable(mut self, editable: impl Into<BoolProperty>) -> Self {
        let editable = editable.into();
        let app = self.item.get_app();
        editable.add_observer(
            Observer::new_without_id(
                move || {
                    app.lock().unwrap().request_layout();
                }
            )
        );
        self.properties.lock().unwrap().editable = editable;
        self
    }

    pub fn get_app(&self) -> SharedApp {
        self.item.get_app()
    }

    pub fn unwrap(self) -> Item {
        self.item
    }
}