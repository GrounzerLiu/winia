use crate::core::next_id;
use crate::shared::{Shared, SharedBool, SharedDerived, SharedDerivedBool, SharedDerivedColor, SharedDerivedF32, SharedDerivedString, SharedDerivedText, SharedDerivedUsize, SharedSource};
use crate::text::Paragraph;
use crate::theme::color;
use crate::ui::item::{Children, ItemKind, ItemProps, LayoutDirection, PhysicalX};
use crate::ui::{Color, Item, Orientation, SetColor};
use proc_macro::ItemProps;
use skia_safe::paint::Style;
use skia_safe::textlayout::{ParagraphStyle, TextAlign, TextStyle};
use skia_safe::{Canvas, Paint, Rect};
use std::ops::Range;
use letclone::clone;
use winit::event::ElementState;
use crate::event::{ItemEvent, MeasureMode, PointerButton, PointerMoved};

#[derive(ItemProps)]
pub struct LabelProps {
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
    pub on_selection_change: SharedSource<Option<Box<dyn Fn(Range<usize>)>>>,
}

impl LabelProps {
    pub fn new(item_props: ItemProps, text: impl Into<SharedDerivedText>) -> Self {
        let on_surface_color = item_props
            .window_context
            .theme()
            .lock()
            .get_color(color::ON_SURFACE)
            .map_or(Color::BLACK, |c| *c);
        Self {
            item_props,
            text: text.into(),
            selectable: false.into(),
            selection_range: (0..0).into(),
            color: on_surface_color.into(),
            font_size: 14.0.into(),
            max_lines: usize::MAX.into(),
            ellipsis: "".into(),
            text_align: None.into(),
            on_selection_change: None.into(),
        }.name("Label")
    }

    pub fn on_selection_change<F>(self, callback: F) -> Self
    where
        F: Fn(Range<usize>) + 'static,
    {
        let boxed_callback: Box<dyn Fn(Range<usize>)> = Box::new(callback);
        self.on_selection_change.set(Some(boxed_callback));
        self
    }
}

pub fn label(props: LabelProps) -> Item {
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

fn item_event(props: &LabelProps) -> ItemEvent {
    let text_cache: SharedSource<TextCache> = TextCache::new().into();
    let is_text_changed = SharedBool::new(true);
    let animation_forward = SharedBool::new(true);
    bind_is_text_changed(&props.color, &is_text_changed);
    bind_is_text_changed(&props.text, &is_text_changed);
    bind_is_text_changed(&props.max_lines, &is_text_changed);
    bind_is_text_changed(&props.font_size, &is_text_changed);
    bind_is_text_changed(&props.text_align, &is_text_changed);
    bind_is_text_changed(&props.ellipsis, &is_text_changed);


    let mut start_index: SharedSource<Option<usize>> = Shared::new(None);

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
                    if is_text_changed.get() || text_cache.lock().is_empty() {
                        let new_paragraph =
                            text.create_paragraph(&paragraph_style, &text_style, max_width);
                        let text_layout = text.get_text_layout(&new_paragraph);
                        let text_width = text_layout.width();
                        let text_height = text_layout.height();
                        text_cache.lock().set_top(new_paragraph);
                        (text_width, text_height)
                    } else {
                        let mut locked_text_cache = text_cache.lock();
                        let existing_paragraph = locked_text_cache.top.as_mut().unwrap();
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
                is_text_changed,
                animation_forward,
                item_props.layout_direction
            );
            move |item, width, height| {
                if item.measure_frame.width != width || item.measure_frame.height != height {
                    let padding_h = item.get_padding(Orientation::Horizontal);
                    let max_width = item.clamp_width(width) - padding_h + 1.0;
                    if text_cache.lock().is_empty() {
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
                        text_cache.lock().set_top(new_paragraph);
                    } else {
                        let mut locked_text_cache = text_cache.lock();
                        let existing_paragraph = locked_text_cache.top.as_mut().unwrap();
                        existing_paragraph.layout(max_width);
                    }
                }
                let text_width = {
                    let locked_text_cache = text_cache.lock();
                    let existing_paragraph = locked_text_cache.top.as_ref().unwrap();
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
                if item.target_frame.get_float_param("progress").is_none() {
                    item.target_frame.set_float_param("progress", 1.0)
                }
                if is_text_changed.get() {
                    is_text_changed.set(false);
                    if animation_forward.get() {
                        item.target_frame.set_float_param("progress", 0.0);
                        animation_forward.set(false);
                    } else {
                        item.target_frame.set_float_param("progress", 1.0);
                        animation_forward.set(true);
                    }
                }
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
                animation_forward,
                props.selectable,
                props.selection_range
            );
            move |item, canvas| {
                let current_frame = item.current_frame();
                let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);

                if let Some(paragraph) = &text_cache.lock().top
                    && selectable.get()
                    && !selection_range.get().is_empty()
                {
                    let mut text = text.lock();
                    let text_length = text.len();
                    let text_layout = text.get_text_layout(paragraph);
                    let mut selection_range = selection_range.get();
                    selection_range.end = selection_range.end.min(text_length);
                    let selection_color = *item
                        .props()
                        .window_context
                        .theme()
                        .lock()
                        .get_color(color::PRIMARY)
                        .unwrap_or(&Color::BLUE);
                    selection_paint.set_any_color(selection_color.with_a_f(0.3));
                    for rect in text_layout.get_rects_for_range(selection_range) {
                        let selection_rect = Rect::from_xywh(
                            current_frame.x() + content_x + rect.rect.left,
                            current_frame.y() + content_y + rect.rect.top,
                            rect.rect.width(),
                            rect.rect.height(),
                        );
                        canvas.draw_rect(selection_rect, &selection_paint);
                    }
                }

                let progress = current_frame.get_float_param("progress").unwrap_or(1.0);
                text_cache.lock().draw(
                    canvas,
                    &text,
                    current_frame.rect(),
                    current_frame.x() + content_x,
                    current_frame.y() + content_y,
                    if animation_forward.get() {
                        progress
                    } else {
                        1.0 - progress
                    },
                );
                /*                let tl = text_cache.lock();
                if let Some(top) = &tl.top {
                    let mut text = text.lock();
                    let text_layout = text.get_text_layout(top);
                    canvas.save_layer_alpha_f(current_frame.rect(), 1.0);
                    text_layout.draw(canvas, content_x, content_y);
                    canvas.restore();
                }*/
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
                if let Some(paragraph) = &text_cache.lock().top {
                    let text_layout = text.get_text_layout(paragraph);
                    let current_frame = item.current_frame();
                    let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                    let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);
                    let local_x = pointer_button.x - current_frame.x() - content_x;
                    let local_y = pointer_button.y - current_frame.y() - content_y;
                    let index = text_layout.get_closest_grapheme_cluster_cluster_at((local_x, local_y));
                    match pointer_button.state {
                        ElementState::Pressed => {
                            start_index.lock().replace(index);
                            if let Some(callback) = on_selection_change.lock().as_mut() && selection_range.get() != (index..index) {
                                callback(index..index);
                                return true;
                            }
                            false
                        }
                        ElementState::Released => {
                            start_index.lock().take();
                            true
                        }
                        _ => false,
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
                if let Some(paragraph) = &text_cache.lock().top {
                    let text_layout = text.get_text_layout(paragraph);
                    let current_frame = item.current_frame();
                    let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                    let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);
                    let local_x = pointer_moved.x - current_frame.x() - content_x;
                    let local_y = pointer_moved.y - current_frame.y() - content_y;
                    let index = text_layout.get_closest_grapheme_cluster_cluster_at((local_x, local_y));

                    if let Some(start) = start_index.read().clone() {
                        let new_range = if index < start {
                            index..start
                        } else {
                            start..index
                        };
                        if selection_range.get() != new_range && let Some(callback) = on_selection_change.lock().as_mut() {
                            callback(new_range);
                        }
                    }
                }
            }
        })
}

pub(super) fn create_text_style(font_size: &SharedDerivedF32, color: &SharedDerived<Color>) -> TextStyle {
    let font_size = font_size.get();
    let color = color.get();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    text_style.set_color(color.to_skia_color());
    text_style
}

pub(super) fn create_paragraph_style(
    layout_direction: &SharedDerived<LayoutDirection>,
    max_lines: &SharedDerived<usize>,
    ellipsis: &SharedDerivedString,
    text_align: &SharedDerived<Option<TextAlign>>,
) -> ParagraphStyle {
    let text_align = text_align.get().unwrap_or(match layout_direction.get() {
        LayoutDirection::LTR => TextAlign::Left,
        LayoutDirection::RTL => TextAlign::Right,
    });
    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_align(text_align);
    let max_lines = max_lines.get();
    paragraph_style.set_max_lines(max_lines);
    if !ellipsis.get().is_empty() && max_lines != usize::MAX {
        paragraph_style.set_ellipsis(ellipsis.get());
    }
    paragraph_style
}

struct TextCache {
    top: Option<Paragraph>,
    bottom: Option<Paragraph>,
}

impl TextCache {
    pub fn new() -> Self {
        Self {
            top: None,
            bottom: None,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.top.is_none() && self.bottom.is_none()
    }

    pub fn draw(
        &self,
        canvas: &Canvas,
        text: &SharedDerivedText,
        rect: Rect,
        x: f32,
        y: f32,
        progress: f32,
    ) {
        if let Some(bottom) = &self.bottom {
            let mut text = text.lock();
            let text_layout = text.get_text_layout(bottom);
            canvas.save_layer_alpha_f(rect, 1.0 - progress);
            text_layout.draw(canvas, x, y);
            canvas.restore();
        }
        if let Some(top) = &self.top {
            let mut text = text.lock();
            let text_layout = text.get_text_layout(top);
            canvas.save_layer_alpha_f(rect, progress);
            text_layout.draw(canvas, x, y);
            canvas.restore();
        }
    }

    pub fn set_top(&mut self, paragraph: Paragraph) {
        if let Some(top) = self.top.take() {
            self.bottom = Some(top);
        }
        self.top = Some(paragraph);
    }
}
