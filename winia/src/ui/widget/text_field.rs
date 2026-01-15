use crate::core::next_id;
use crate::shared::{Shared, SharedBool, SharedDerived, SharedDerivedBool, SharedDerivedColor, SharedDerivedF32, SharedDerivedString, SharedDerivedText, SharedDerivedUsize, SharedSource, SharedText};
use crate::text::Paragraph;
use crate::theme::color;
use crate::ui::item::{Children, ImeAction, ItemEvent, ItemKind, ItemProps, MeasureMode, PhysicalX, PointerButton, PointerMoved};
use crate::ui::widget::label::{create_paragraph_style, create_text_style};
use crate::ui::{Color, Item, Orientation, SetColor};
use clonelet::clone;
use proc_macro::ItemProps;
use skia_safe::paint::Style;
use skia_safe::textlayout::TextAlign;
use skia_safe::{Paint, Rect};
use std::ops::{Deref, Range};
use std::time::{Duration, Instant};
use arboard::Clipboard;
use winit::dpi::{LogicalPosition, LogicalSize, Position, Size};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key, NamedKey};
use winit::window::ImeRequest;
use crate::keyboard::ModifiersKeyState;
use crate::window::ImeRequestData;

pub enum TextChange {
    Inserted { index: usize, text: String },
    Deleted { range: Range<usize> },
}
impl TextChange {
    pub fn apply_to(&self, text: &SharedText) {
        text.write(|text| {
            match self {
                TextChange::Inserted { index, text: new_text } => {
                    text.insert_str(*index, new_text);
                }
                TextChange::Deleted { range } => {
                    text.remove(range.clone());
                }
            }
        });
    }

    pub fn try_apply_to(&self, text: &SharedText) -> String {
        let mut text_clone = text.read().to_string();
        match self {
            TextChange::Inserted { index, text: new_text } => {
                text_clone.insert_str(*index, new_text);
            }
            TextChange::Deleted { range } => {
                text_clone.drain(range.clone());
            }
        }
        text_clone
    }
}

#[derive(ItemProps)]
pub struct TextFieldProps {
    pub item_props: ItemProps,
    #[constructor]
    pub text: SharedDerivedText,
    pub selectable: SharedDerivedBool,
    pub selection_range: SharedDerived<Range<usize>>,
    pub color: SharedDerivedColor,
    pub font_size: SharedDerivedF32,
    pub max_lines: SharedDerivedUsize,
    pub ellipsis: SharedDerivedString,
    pub text_align: SharedDerived<Option<TextAlign>>,
    #[not_shared]
    pub on_selection_change: SharedSource<Box<dyn FnMut(Range<usize>)>>,
    #[not_shared]
    #[constructor(type = impl FnMut(TextChange) + 'static)]
    pub on_text_change: SharedSource<Box<dyn FnMut(TextChange)>>,
}

impl TextFieldProps {
    pub fn new(item_props: ItemProps, text: impl Into<SharedDerivedText>, on_text_change: impl FnMut(TextChange) + 'static) -> Self {
        let on_surface_color = item_props
            .window_context
            .theme()
            .lock()
            .get_color(color::ON_SURFACE)
            .map_or(Color::BLACK, |c| *c);
        let on_text_change_boxed: Box<dyn FnMut(TextChange)> = Box::new(on_text_change);
        let on_text_change: SharedSource<Box<dyn FnMut(TextChange)>> =
            SharedSource::new(on_text_change_boxed);
        let selection_range: SharedSource<Range<usize>> = (0..0).into();
        let on_selection_change: Box<dyn FnMut(Range<usize>)> = Box::new({
            let selection_range = selection_range.clone();
            move |new_range: Range<usize>| {
                selection_range.set(new_range);
            }
        });
        let on_selection_change: SharedSource<Box<dyn FnMut(Range<usize>)>> =
            SharedSource::new(on_selection_change);

        Self {
            item_props,
            text: text.into(),
            selectable: true.into(),
            selection_range: selection_range.into(),
            color: on_surface_color.into(),
            font_size: 14.0.into(),
            max_lines: usize::MAX.into(),
            ellipsis: "".into(),
            text_align: None.into(),
            on_selection_change,
            on_text_change,
        }
    }

    pub fn on_selection_change<F>(self, callback: F) -> Self
    where
        F: FnMut(Range<usize>) + 'static,
    {
        let boxed_callback: Box<dyn FnMut(Range<usize>)> = Box::new(callback);
        self.on_selection_change.set(boxed_callback);
        self
    }
}

pub fn text_field(props: TextFieldProps) -> Item {
    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props,
        Children::new(),
    )
}

fn bind_is_text_changed<T>(
    source: &SharedDerived<T>,
    is_text_changed: &SharedBool,
) where
    T: Send + 'static,
{
    source.subscribe(next_id(), {
        let is_text_changed = is_text_changed.clone();
        move || {
            is_text_changed.set(true);
        }
    });
}

fn item_event(props: &TextFieldProps) -> ItemEvent {
    let text_cache: SharedSource<Option<Paragraph>> = None.into();
    let is_text_changed = SharedBool::new(true);
    is_text_changed.subscribe(
        next_id(),
        {
            let e = props.window_context.event_loop_proxy().clone();
            let updater = props.item_updater.clone();
            move || {
                updater.lock().request_update();
                e.request_update_layout();
            }
        },
    );
    bind_is_text_changed(&props.color, &is_text_changed);
    bind_is_text_changed(&props.text, &is_text_changed);
    bind_is_text_changed(&props.max_lines, &is_text_changed);
    bind_is_text_changed(&props.font_size, &is_text_changed);
    bind_is_text_changed(&props.text_align, &is_text_changed);
    bind_is_text_changed(&props.ellipsis, &is_text_changed);

    let start_index: SharedSource<Option<usize>> = None.into();
    let composing = SharedSource::new(None::<(Range<usize>, Range<usize>)>);
    let last_cursor_blink = SharedSource::new(Instant::now());
    let show_cursor = SharedBool::new(false);
    show_cursor.subscribe(
        next_id(),
        {
            let e = props.window_context.event_loop_proxy().clone();
            let updater = props.item_updater.clone();
            move || {
                updater.lock().request_update();
                e.request_update_layout();
            }
        },
    );
    props.item_props.spawn_task({
        clone!(show_cursor, last_cursor_blink);
        async move {
            loop {
                let elapsed = last_cursor_blink.get().elapsed();
                if elapsed >= Duration::from_millis(500) {
                    show_cursor.set(!show_cursor.get());
                }
                let sleep_duration = if elapsed >= Duration::from_millis(500) {
                    Duration::from_millis(500)
                } else {
                    Duration::from_millis(500) - elapsed
                };
                tokio::time::sleep(sleep_duration).await;
            }
        }
    });

    let item_props = &props.item_props;
    ItemEvent::new()
        .set_measure({
            clone!(
                props.font_size,
                props.color,
                props.text,
                props.text_align,
                props.max_lines,
                props.ellipsis,
                text_cache,
                is_text_changed,
                item_props.layout_direction
            );
            move |item, width_mode, height_mode| {
                let mut text = text.lock();
                let text_style = create_text_style(&font_size, &color);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let padding_v = item.get_padding(Orientation::Vertical);
                let paragraph_style =
                    create_paragraph_style(&layout_direction, &max_lines, &ellipsis, &text_align);
                let max_width = item.clamp_width(width_mode.value()) - padding_h + 1.0;

                let (text_width, text_height) =
                    if is_text_changed.get() || text_cache.lock().is_none() {
                        let new_paragraph =
                            text.create_paragraph(&paragraph_style, &text_style, max_width);
                        let text_layout = text.get_text_layout(&new_paragraph);
                        let text_width = text_layout.width();
                        let text_height = text_layout.height();
                        text_cache.lock().replace(new_paragraph);
                        (text_width, text_height)
                    } else {
                        let mut locked_text_cache = text_cache.lock();
                        let existing_paragraph = locked_text_cache.as_mut().unwrap();
                        existing_paragraph.layout(max_width);
                        let text_layout = text.get_text_layout(existing_paragraph);
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
                            MeasureMode::Unspecified(_height) => {
                                let height = text_height + padding_v;
                                (width, height)
                            }
                        }
                    }
                    MeasureMode::Unspecified(_) => match height_mode {
                        MeasureMode::Specified(height) => {
                            let height = item.clamp_height(height);
                            (item.clamp_width(text_width + padding_h + 1.0), height)
                        }
                        MeasureMode::Unspecified(_) => (
                            item.clamp_width(text_width + padding_h + 1.0),
                            item.clamp_height(text_height + padding_v),
                        ),
                    },
                };
                item.measure_frame.width = width;
                item.measure_frame.height = height;
            }
        })
        .set_layout({
            let item_props = &props.item_props;
            clone!(
                props.font_size,
                props.color,
                props.text,
                props.text_align,
                props.max_lines,
                props.ellipsis,
                text_cache,
                item_props.layout_direction
            );
            move |item, width, height| {
                if item.measure_frame.width != width || item.measure_frame.height != height {
                    let padding_h = item.get_padding(Orientation::Horizontal);
                    let max_width = item.clamp_width(width) - padding_h + 1.0;
                    if text_cache.lock().is_none() {
                        let mut text = text.lock();
                        let text_style = create_text_style(&font_size, &color);
                        let paragraph_style = create_paragraph_style(
                            &layout_direction,
                            &max_lines,
                            &ellipsis,
                            &text_align,
                        );
                        let new_paragraph =
                            text.create_paragraph(&paragraph_style, &text_style, max_width);
                        text_cache.lock().replace(new_paragraph);
                    } else {
                        let mut locked_text_cache = text_cache.lock();
                        let existing_paragraph = locked_text_cache.as_mut().unwrap();
                        existing_paragraph.layout(max_width);
                    }
                }
                let text_width = {
                    let locked_text_cache = text_cache.lock();
                    let existing_paragraph = locked_text_cache.as_ref().unwrap();
                    let mut text = text.lock();
                    let text_layout = text.get_text_layout(existing_paragraph);
                    text_layout.width()
                };
                let padding_top = item.props().padding.top.get();
                let padding_start = item.props().padding.start.get();
                item.target_frame.set_float_param(
                    "content_x",
                    padding_start.physical_x(layout_direction.get(), width, text_width),
                );
                item.target_frame.set_float_param("content_y", padding_top);
            }
        })
        .set_draw({
            // let layout_direction = props.item_props.layout_direction.clone();
            let mut selection_paint = Paint::default();
            selection_paint.set_anti_alias(true);
            selection_paint.set_style(Style::Fill);
            clone!(
                props.text,
                text_cache,
                text_cache,
                composing,
                props.selectable,
                props.selection_range,
                show_cursor,
                is_text_changed
            );
            move |item, canvas| {
                let current_frame = item.current_frame();
                let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);

                let paragraph_x = current_frame.x() + content_x;
                let paragraph_y = current_frame.y() + content_y;

                let mut selection_range = selection_range.get();
                if let Some(paragraph) = text_cache.lock().as_ref()
                    && selectable.get()
                {
                    let mut text = text.lock();
                    let text_length = text.len();
                    let text_layout = text.get_text_layout(paragraph);
                    if !selection_range.is_empty() {
                        selection_range.end = selection_range.end.min(text_length);
                        let selection_color = *item
                            .props()
                            .window_context
                            .theme()
                            .lock()
                            .get_color(color::PRIMARY)
                            .unwrap_or(&Color::BLUE);
                        selection_paint.set_any_color(selection_color.with_a_f(0.3));
                        for rect in text_layout.get_rects_for_range(selection_range.clone()) {
                            let selection_rect = Rect::from_xywh(
                                current_frame.x() + content_x + rect.rect.left,
                                current_frame.y() + content_y + rect.rect.top,
                                rect.rect.width(),
                                rect.rect.height(),
                            );
                            canvas.draw_rect(selection_rect, &selection_paint);
                        }
                    }
                }
                if let Some(paragraph) = text_cache.lock().as_ref() {
                    let mut text = text.lock();
                    let text_layout = text.get_text_layout(paragraph);
                    if let Some((composing_range, _)) = composing.lock().as_ref() {
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
                                    x,
                                    y,
                                    w,
                                    h,
                                );
                                canvas.draw_rect(
                                    rect,
                                    Paint::default().set_anti_alias(true).set_any_color(Color::RED),
                                );
                            });
                    }
                    text_layout.draw(canvas, paragraph_x, paragraph_y);

                    if show_cursor.get() && selection_range.is_empty() {
                        if let Some((x, y, h)) = text_layout.get_cursor_position(selection_range.start) {
                            let mut x = paragraph_x + x;

                            if x < current_frame.x() {
                                x = current_frame.x();
                            }

                            if x >= current_frame.x() + current_frame.width - 2.0 {
                                x = current_frame.x() + current_frame.width - 2.0;
                            }
                            let y = paragraph_y + y;
                            let rect = Rect::from_xywh(x, y, 2.0, h);
                            canvas.draw_rect(
                                rect,
                                Paint::default().set_anti_alias(true).set_color(0xffff0000),
                            );
                            if item.focus_state.is_focused {
                                // item.get_window_context().window.lock().set_ime_cursor_area(
                                //     Position::Logical(LogicalPosition::new(x as f64, y as f64)),
                                //     Size::Logical(LogicalSize::new(0.0, 0.0)),
                                // )
                                item.window_context().window().request_ime_update(
                                    ImeRequest::Update(ImeRequestData::default().with_cursor_area(
                                        Position::Logical(LogicalPosition::new(x as f64, y as f64)),
                                        Size::Logical(LogicalSize::new(0.0, 0.0)),
                                    ))
                                ).map_err(|e| {
                                    ;
                                    println!("Failed to request ime update: {:?}", e);
                                }).ok();
                            }
                        }
                    }
                }
            }
        })
        .set_pointer_button({
            clone!(
                props.selectable,
                props.selection_range,
                props.text,
                props.on_selection_change,
                text_cache,
                start_index
            );
            move |item, pointer_button: &PointerButton| {
                if !pointer_button.primary || !selectable.get() {
                    return false;
                }
                let mut text = text.lock();
                if let Some(paragraph) = text_cache.lock().as_ref() {
                    let text_layout = text.get_text_layout(paragraph);
                    let current_frame = item.current_frame();
                    let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                    let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);
                    let local_x = pointer_button.position.x - current_frame.x() - content_x;
                    let local_y = pointer_button.position.y - current_frame.y() - content_y;
                    let index = text_layout.get_closest_grapheme_cluster_cluster_at((local_x, local_y));
                    match pointer_button.state {
                        ElementState::Pressed => {
                            start_index.lock().replace(index);
                            // if let Some(callback) = on_selection_change.lock().as_mut() && selection_range.get() != (index..index) {
                            //     callback(index..index);
                            //     return true;
                            // }
                            // false
                            let mut on_selection_change = on_selection_change.lock();
                            on_selection_change(index..index);
                            item.props().focus_requester.lock().request_focus();
                            true
                        }
                        ElementState::Released => {
                            start_index.lock().take();
                            true
                        }
                    }
                } else {
                    false
                }
            }
        })
        .set_pointer_moved({
            clone!(
                props.selectable,
                props.selection_range,
                props.text,
                props.on_selection_change,
                text_cache,
                start_index
            );
            move |item, pointer_moved: &PointerMoved| {
                if !pointer_moved.primary || !selectable.get() || start_index.lock().is_none() {
                    return;
                }
                let mut text = text.lock();
                if let Some(paragraph) = text_cache.lock().as_ref() {
                    let text_layout = text.get_text_layout(paragraph);
                    let current_frame = item.current_frame();
                    let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                    let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);
                    let local_x = pointer_moved.position.x - current_frame.x() - content_x;
                    let local_y = pointer_moved.position.y - current_frame.y() - content_y;
                    let index = text_layout.get_closest_grapheme_cluster_cluster_at((local_x, local_y));

                    if let Some(start) = start_index.read().clone() {
                        let new_range = if index < start {
                            index..start
                        } else {
                            start..index
                        };
                        // if selection_range.get() != new_range && let Some(callback) = on_selection_change.lock().as_mut() {
                        //     callback(new_range);
                        // }
                        let mut on_selection_change = on_selection_change.lock();
                        (on_selection_change)(new_range);
                    }
                }
            }
        })
        .set_ime_input({
            clone!(
                props.text,
                props.on_text_change,
                props.selection_range,
                props.on_selection_change,
                show_cursor,
                last_cursor_blink,
                text_cache,
            );
            move |item, ime_input| {
                let mut on_selection_change = on_selection_change.lock();
                let mut on_text_change = on_text_change.lock();
                {
                    let text = text.lock();
                    let text_len = text.len();
                    let mut selection_range = selection_range.get();
                    on_selection_change(selection_range.start.clamp(0, text_len)..selection_range.end.clamp(0, text_len));
                }
                let mut composing = composing.lock();
                match ime_input {
                    ImeAction::Enter | ImeAction::Delete | ImeAction::Commit(_) => {
                        if !selection_range.lock().is_empty() {
                            on_text_change(TextChange::Deleted {
                                range: selection_range.get(),
                            });
                            on_selection_change(selection_range.get().start..selection_range.get().start);
                        }
                    }
                    _ => {}
                }
                match ime_input {
                    ImeAction::Enabled => {}
                    ImeAction::Enter => {
                        on_text_change(TextChange::Inserted {
                            index: selection_range.get().start,
                            text: "\n".to_string(),
                        });
                        let new_index = selection_range.get().start + 1;
                        on_selection_change(new_index..new_index);
                    }
                    ImeAction::Delete => {
                        if selection_range.lock().start == 0 {
                            return;
                        }

                        if let Some(paragraph) = text_cache.lock().as_ref()
                            && let Some(prev_glyph_index) = paragraph.prev_glyph_byte_index(selection_range.get().start) {
                            on_text_change(TextChange::Deleted {
                                range: prev_glyph_index..selection_range.get().start,
                            });
                            on_selection_change(prev_glyph_index..prev_glyph_index);
                        }
                    }
                    ImeAction::PreEdit(pr_text, range) => {
                        if let Some((composing_range, old_selection_range)) = composing.as_ref()
                        {
                            on_text_change(TextChange::Deleted {
                                range: composing_range.clone(),
                            });
                            on_selection_change(old_selection_range.start..old_selection_range.end);
                            *composing = None;
                        }

                        if let Some((start, end)) = range {
                            on_text_change(TextChange::Inserted {
                                index: selection_range.get().start,
                                text: pr_text.clone(),
                            });
                            *composing = Some((
                                selection_range.get().start..(selection_range.get().start + pr_text.len()),
                                selection_range.get().clone(),
                            ));
                            let new_selection_start = selection_range.get().start + start;
                            let new_selection_end = selection_range.get().start + end;
                            on_selection_change(new_selection_start..new_selection_end);
                        }
                    }
                    ImeAction::Commit(commit_text) => {
                        let commit_text_len = commit_text.len();
                        on_text_change(TextChange::Inserted {
                            index: selection_range.get().start,
                            text: commit_text.clone(),
                        });
                        let new_index = selection_range.get().start + commit_text_len;
                        on_selection_change(new_index..new_index);
                    }
                    ImeAction::Disabled => {}
                    _ => {}
                }
                show_cursor.set(true);
                last_cursor_blink.set(Instant::now());
            }
        })
        .set_keyboard_input({
            clone!(
                props.text,
                text_cache,
                props.on_text_change,
                props.on_selection_change,
                props.selection_range,
                show_cursor,
                last_cursor_blink
            );
            move |item, keyboard_input| {
                let event = &keyboard_input.key_event;
                if event.state == ElementState::Pressed {
                    let modifiers = item.window_context.modifiers.get();
                    if let Some(modifiers) = modifiers
                        && modifiers.state().control_key()
                    {
                        if let Key::Character(c) = &event.logical_key {
                            if c.as_str() == "c"|| c.as_str() == "x" {
                                let mut clipboard = Clipboard::new().unwrap();

                                let selection_range = selection_range.get();
                                if selection_range.start != selection_range.end {
                                    let text = text.lock();
                                    let selected_text = text.substring(selection_range.clone());
                                    clipboard.set_text(selected_text.to_string()).unwrap();
                                    drop(text);
                                    if c.as_str() == "x" {
                                        let mut on_text_change =  on_text_change.lock();
                                        on_text_change(TextChange::Deleted {
                                            range: selection_range.clone()
                                        });
                                        let mut on_selection_change = on_selection_change.lock();
                                        on_selection_change(selection_range.start..selection_range.start);
                                    }
                                }
                            } else if c.as_str() == "v" {
                                let mut clipboard = Clipboard::new().unwrap();
                                if let Ok(text) = clipboard.get_text() {
                                    item.ime_input(&ImeAction::Commit(text));
                                }
                            }
                        }
                    } else {
                        match &event.logical_key {
                            Key::Named(key) => match key {
                                NamedKey::Backspace => {
                                    item.ime_input(&ImeAction::Delete);
                                }
                                NamedKey::Enter => {
                                    item.ime_input(&ImeAction::Enter);
                                }
                                NamedKey::ArrowLeft => {
                                    let mut selection_range = selection_range.get();
                                    if selection_range.start > 0 {
                                        if let Some(paragraph) = text_cache.lock().as_ref()
                                            && let Some(prev_glyph_index) = paragraph.prev_glyph_byte_index(selection_range.start) {
                                            let mut on_selection_change = on_selection_change.lock();
                                            on_selection_change(prev_glyph_index..prev_glyph_index);
                                        }
                                    }
                                    show_cursor.set(true);
                                    last_cursor_blink.set(Instant::now());
                                }
                                NamedKey::ArrowRight => {
                                    let mut selection_range = selection_range.get();
                                    let text_len = text.lock().len();
                                    if selection_range.start < text_len {
                                        if let Some(paragraph) = text_cache.lock().as_ref()
                                            && let Some(next_glyph_index) = paragraph.next_glyph_byte_index(selection_range.start) {
                                            let mut on_selection_change = on_selection_change.lock();
                                            on_selection_change(next_glyph_index..next_glyph_index);
                                        }
                                    }
                                    show_cursor.set(true);
                                    last_cursor_blink.set(Instant::now());
                                }
                                NamedKey::Escape => {
                                    item.props().focus_requester.lock().clear_focus()
                                }
                                _ => {}
                            },
                            Key::Character(str) => {
                                item.ime_input(&ImeAction::Commit(str.to_string()));
                            }
                            Key::Unidentified(_) => {}
                            Key::Dead(_) => {}
                        }
                    }
                }
            }
        })
        .set_click_input({
            move |item, click_input| {
                item.props().focus_requester.lock().request_focus();
            }
        })
        .set_focus_changed({
            move |item, focus_state| {
                if focus_state.is_focused {
                    item.window_context().set_ime_allowed(item.id(), true);
                }
            }
        })
}