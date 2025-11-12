use std::ops::Range;
use clonelet::clone;
use skia_safe::{Paint, Rect};
use skia_safe::paint::Style;
use skia_safe::textlayout::{ParagraphStyle, TextAlign};
use winit::event::MouseButton;
use crate::{bind_properties, define_props};
use crate::core::next_id;
use crate::shared::{Shared, SharedBool, SharedDerived, SharedDerivedBool, SharedDerivedColor, SharedDerivedF32, SharedDerivedString, SharedDerivedText, SharedDerivedUsize, SharedSource};
use crate::text::Paragraph;
use crate::theme::color;
use crate::ui::{Color, Item, Orientation, SetColor};
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, LayoutDirection, MeasureMode, PhysicalX, PointerState};
use crate::ui::widget::label::create_text_style;

define_props!(
    InputFieldPropsTrait;
    input_feilf_props;
    TextFieldProps {
        text: SharedDerivedText,
        selectable: SharedDerivedBool,
        selection_range: SharedDerived<Range<usize>>,
        color: SharedDerivedColor,
        font_size: SharedDerivedF32,
        max_lines: SharedDerivedUsize,
        text_align: SharedDerived<Option<TextAlign>>
    }
    {
        on_selection_change: SharedSource<Option<Box<dyn Fn(Range<usize>)>>>
    }
);

impl TextFieldProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self {
            item_props,
            text: "".into(),
            selectable: true.into(),
            selection_range: (0..0).into(),
            color: Color::BLACK.into(),
            font_size: 14.0.into(),
            max_lines: usize::MAX.into(),
            text_align: None.into(),
            on_selection_change: SharedSource::new(None),
        }
    }

    pub fn on_selection_change<F: 'static + Fn(Range<usize>)>(mut self, f: F) -> Self {
        let f_boxed = Box::new(f);
        self.on_selection_change = SharedSource::new(Some(f_boxed));
        self
    }
}

pub fn text_field(props: TextFieldProps) -> Item {
    let item = Item::new(
        ItemKind::Widget,
        item_event(&props),
        props.item_props,
        Shared::new_derived(vec![]),
    );
    bind_properties!(
        item,
        props.text,
        props.selectable,
        props.selection_range,
        props.color,
        props.font_size,
        props.max_lines,
        props.text_align
    );
    item
}

fn item_event(props: &TextFieldProps) -> ItemEvent {
    let paragraph: SharedSource<Option<Paragraph>> = SharedSource::new(None);
    let is_text_changed = SharedBool::new(true);
    props.color.subscribe(next_id(), {
        let is_text_changed = is_text_changed.clone();
        move || {
            is_text_changed.set(true);
        }
    });
    props.text.subscribe(next_id(), {
        let is_text_changed = is_text_changed.clone();
        move || {
            is_text_changed.set(true);
        }
    });
    props.max_lines.subscribe(next_id(), {
        let is_text_changed = is_text_changed.clone();
        move || {
            is_text_changed.set(true);
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
                paragraph,
                is_text_changed,
                item_props.layout_direction
            );
            move |item, width_mode, height_mode| {
                let mut text = text.lock();
                let text_style = create_text_style(&font_size, &color);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let padding_v = item.get_padding(Orientation::Vertical);
                let paragraph_style =
                    create_paragraph_style(&layout_direction, &max_lines, &text_align);
                let max_width = item.clamp_width(width_mode.value()) - padding_h + 1.0;

                let (text_width, text_height) =
                    if is_text_changed.get() || paragraph.lock().is_none() {
                        let new_paragraph =
                            text.create_paragraph(&paragraph_style, &text_style, max_width);
                        let text_layout = text.get_text_layout(&new_paragraph);
                        let text_width = text_layout.width();
                        let text_height = text_layout.height();
                        paragraph.lock().replace(new_paragraph);
                        (text_width, text_height)
                    } else {
                        let mut paragraph = paragraph.lock();
                        paragraph.as_mut().unwrap().layout(max_width);
                        let text_layout = text.get_text_layout(paragraph.as_mut().unwrap());
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
                paragraph,
                is_text_changed,
                item_props.layout_direction
            );
            move |item, width, height| {
                if item.measure_frame.width != width || item.measure_frame.height != height {
                    let padding_h = item.get_padding(Orientation::Horizontal);
                    let max_width = item.clamp_width(width) - padding_h + 1.0;
                    if paragraph.lock().is_none() {
                        let mut text = text.lock();
                        let text_style = create_text_style(&font_size, &color);
                        let paragraph_style = create_paragraph_style(
                            &layout_direction,
                            &max_lines,
                            &text_align,
                        );
                        let new_paragraph =
                            text.create_paragraph(&paragraph_style, &text_style, max_width);
                        paragraph.lock().replace(new_paragraph);
                    } else {
                        let mut paragraph = paragraph.lock();
                        paragraph.as_mut().unwrap().layout(max_width);
                    }
                }
                let text_width = {
                    let mut paragraph = paragraph.lock();
                    let mut text = text.lock();
                    let text_layout = text.get_text_layout(paragraph.as_mut().unwrap());
                    text_layout.width()
                };
                let padding_top = item.props().padding.top.get();
                let padding_start = item.props().padding.start.get();
                item.target_frame.set_float_param(
                    "content_x",
                    padding_start.physical_x(layout_direction.get(), width, text_width),
                );
                item.target_frame.set_float_param("content_y", padding_top);
                if is_text_changed.get() {
                    is_text_changed.set(false);
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
                paragraph,
                props.selectable,
                props.selection_range
            );
            move |item, canvas| {
                let current_frame = item.current_frame();
                let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);

                if let Some(paragraph) = paragraph.lock().as_mut()
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
                // text_cache.lock().draw(
                //     canvas,
                //     &text,
                //     current_frame.rect(),
                //     current_frame.x() + content_x,
                //     current_frame.y() + content_y,
                //     if animation_forward.get() {
                //         progress
                //     } else {
                //         1.0 - progress
                //     },
                // );
                if let Some(paragraph) = paragraph.lock().as_mut() {
                    let mut text = text.lock();
                    let text_layout = text.get_text_layout(paragraph);
                    text_layout.draw(canvas, current_frame.x() + content_x, current_frame.y() + content_y);
                }
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
        .set_mouse_input({
            clone!(
                props.selectable,
                props.selection_range,
                props.text,
                props.on_selection_change,
                paragraph
            );
            let mut start_index: Option<usize> = None;
            move |item, mouse_input| {
                if mouse_input.button != MouseButton::Left || !selectable.get() {
                    return false;
                }
                let mut text = text.lock();
                if let Some(paragraph) = paragraph.lock().as_mut() {
                    let text_layout = text.get_text_layout(paragraph);
                    let current_frame = item.current_frame();
                    let content_x = current_frame.get_float_param("content_x").unwrap_or(0.0);
                    let content_y = current_frame.get_float_param("content_y").unwrap_or(0.0);
                    let local_x = mouse_input.x - current_frame.x() - content_x;
                    let local_y = mouse_input.y - current_frame.y() - content_y;
                    let index = text_layout.get_closest_glyph_cluster_at((local_x, local_y));
                    match mouse_input.pointer_state {
                        PointerState::Started => {
                            start_index = Some(index);
                            if let Some(callback) = on_selection_change.lock().as_mut() && selection_range.get() != (index..index) {
                                callback(index..index);
                                return true;
                            }
                            false
                        }
                        PointerState::Moved => {
                            if let Some(start) = start_index {
                                let new_range = if index < start {
                                    index..start
                                } else {
                                    start..index
                                };
                                if selection_range.get() != new_range && let Some(callback) = on_selection_change.lock().as_mut() {
                                    callback(new_range);
                                }
                            }
                            true
                        }
                        PointerState::Ended => {
                            start_index = None;
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            }
        })
}

pub(super) fn create_paragraph_style(
    layout_direction: &SharedDerived<LayoutDirection>,
    max_lines: &SharedDerived<usize>,
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
    paragraph_style
}