use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::thread;
use skia_safe::{Color, Paint, Rect};
use skia_safe::textlayout::{paragraph, TextAlign};
use winit::dpi::Size;
use winit::event::MouseButton;
use crate::core::RefClone;
use crate::dpi::{LogicalPosition, LogicalSize, Position};
use crate::property::{BoolProperty, Children, ColorProperty, F32Property, Gettable, Observable, Property, Settable, TextProperty};
use crate::text::{EdgeBehavior, ParagraphWrapper, Style, StyledText};
use crate::ui::app::{AppContext, UserEvent};
use crate::ui::Item;
use crate::ui::item::{ClickSource, DisplayParameter, Gravity, ItemEvent, LayoutDirection, LogicalX, MeasureMode, Orientation};

/// This component has a serious problem:
///
/// The Paragraph from skia costs a lot of time when the "layout" method is first called.
/// So it may be better to create Paragraph in the sub-thread

pub struct TextBlockProperty {
    text: TextProperty,
    editable: BoolProperty,
    color: ColorProperty,
    font_size: F32Property,
    is_text_updated: Arc<Mutex<bool>>,
}

unsafe impl Send for TextBlockProperty {}

#[derive(Clone)]
struct TextContext {
    paragraph: Arc<Mutex<Option<ParagraphWrapper>>>,
    show_cursor: Arc<Mutex<bool>>,
    composing: Arc<Mutex<Option<(Range<usize>, Range<usize>)>>>,
    selection: Arc<Mutex<Range<usize>>>,
}

fn create_paragraph(property: &TextBlockProperty, text_align: TextAlign, max_width: f32) -> ParagraphWrapper {
    let mut text = property.text.get();
    let color = property.color.get();
    let size = property.font_size.get();
    text.set_style(Style::TextColor(color), 0..text.len(), EdgeBehavior::IncludeAndInclude);
    text.set_style(Style::FontSize(size), 0..text.len(), EdgeBehavior::IncludeAndInclude);
    ParagraphWrapper::new(
        &text,
        0..text.len(),
        max_width,
        text_align,
    )
}

pub struct TextBlock {
    item: Item,
    property: Arc<Mutex<TextBlockProperty>>,
}

impl TextBlock {
    pub fn new(app_context: AppContext) -> Self {
        let property = Arc::new(Mutex::new(TextBlockProperty {
            text: TextProperty::from_static(StyledText::from("")),
            editable: BoolProperty::from_static(false),
            color: ColorProperty::from_static(Color::BLACK),
            font_size: F32Property::from_static(14.0),
            is_text_updated: Arc::new(Mutex::new(true)),
        }));

        let context = TextContext {
            paragraph: Arc::new(Mutex::new(None)),
            show_cursor: Arc::new(Mutex::new(false)),
            composing: Arc::new(Mutex::new(None)),
            selection: Arc::new(Mutex::new(0..0)),
        };

        let item_event = ItemEvent::new()
            .measure({
                let property = property.clone();
                let context = context.clone();
                move |item, width_mode, height_mode| {
                    let property = property.lock().unwrap();

                    if *property.is_text_updated.lock().unwrap() || context.paragraph.lock().unwrap().is_none() {
                        *property.is_text_updated.lock().unwrap() = false;
                        let paragraph = context.paragraph.clone();
                        paragraph.lock().unwrap().replace({
                            let text_align = match item.get_layout_direction().get() {
                                LayoutDirection::LTR => TextAlign::Left,
                                LayoutDirection::RTL => TextAlign::Right,
                            };
                            let max_width = match width_mode {
                                MeasureMode::Specified(width) => item.clamp_width(width),
                                MeasureMode::Unspecified(width) => item.clamp_width(width),
                            };
                            create_paragraph(&property, text_align, max_width)
                        });
                    }
                    let mut paragraph_lock = context.paragraph.lock().unwrap();
                    let paragraph = paragraph_lock.as_mut().unwrap();

                    let padding_horizontal = item.get_padding(Orientation::Horizontal);
                    let padding_vertical = item.get_padding(Orientation::Vertical);

                    let (width, height) = match width_mode {
                        MeasureMode::Specified(width) => {
                            let width = item.clamp_width(width);
                            paragraph.re_layout(width - padding_horizontal);
                            match height_mode {
                                MeasureMode::Specified(height) => {
                                    let height = item.clamp_height(height);
                                    (width, height)
                                }
                                MeasureMode::Unspecified(_) => {
                                    let height = paragraph.layout_height() + padding_vertical;
                                    (width, height)
                                }
                            }
                        }
                        MeasureMode::Unspecified(_) => {
                            paragraph.re_layout(item.get_max_width().get() - padding_horizontal);
                            let paragraph_width = paragraph.layout_width();
                            let paragraph_height = paragraph.layout_height();
                            match height_mode {
                                MeasureMode::Specified(height) => {
                                    let height = item.clamp_height(height);
                                    (item.clamp_width(paragraph_width + padding_horizontal + 1.0), height)
                                }
                                MeasureMode::Unspecified(_) => {
                                    (item.clamp_width(paragraph_width + padding_horizontal + 1.0), item.clamp_height(paragraph_height + padding_vertical))
                                }
                            }
                        }
                    };
                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = width;
                    measure_parameter.height = height;
                }
            })
            .layout({
                let property = property.clone();
                let context = context.clone();
                move |item, width, _height| {
                    let mut paragraph = context.paragraph.lock().unwrap();
                    if let Some(paragraph) = paragraph.as_mut() {
                        paragraph.re_layout(width - item.get_padding(Orientation::Horizontal));
                    }
                }
            })
            .ime_input({
                move |item, ime_action| {
                    println!("IME input: {:?}", ime_action);
                }
            })
            .draw({
                let property = property.clone();
                let context = context.clone();
                move |item, canvas| {
                    let layout_direction = item.get_layout_direction().get();
                    let horizontal_gravity = item.get_horizontal_gravity().get();
                    let vertical_gravity = item.get_vertical_gravity().get();
                    let padding_start = item.get_padding_start().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_bottom = item.get_padding_bottom().get();

                    let display_parameter = item.get_display_parameter();
                    let width = display_parameter.width;
                    let height = display_parameter.height;
                    let show_cursor = *context.show_cursor.lock().unwrap();
                    let selection = context.selection.lock().unwrap().clone();
                    let composing = context.composing.lock().unwrap().clone();

                    // let property = property.lock().unwrap();

                    let paragraph = context.paragraph.lock().unwrap();

                    if let Some(paragraph) = paragraph.as_ref() {
                        let paragraph_width = paragraph.layout_width();
                        let paragraph_height = paragraph.layout_height();

                        let paragraph_x = LogicalX::new(
                            layout_direction,
                            match horizontal_gravity {
                                Gravity::Start => padding_start,
                                Gravity::Center => (width - paragraph_width) / 2.0,
                                Gravity::End => width - paragraph_width - padding_end,
                            },
                            width,
                        );

                        let paragraph_y = match vertical_gravity {
                            Gravity::Start => padding_top,
                            Gravity::Center => (height - paragraph_height) / 2.0,
                            Gravity::End => height - paragraph_height - padding_bottom,
                        };

                        if selection.start != selection.end {
                            paragraph.get_rects_for_range(selection.clone())
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
                                    canvas.draw_rect(rect, Paint::default().set_anti_alias(true).set_color(Color::BLUE));
                                });
                        }

                        if let Some((composing_range, _)) = composing {
                            paragraph.get_rects_for_range(composing_range.clone())
                                .iter()
                                .for_each(|text_box| {
                                    let rect = text_box.rect;
                                    let x = paragraph_x + rect.x();
                                    let y = paragraph_y + rect.y();
                                    let w = rect.width();
                                    let h = 1.0;
                                    let rect = Rect::from_xywh(
                                        x.logical_value() + display_parameter.x(),
                                        y + display_parameter.y(),
                                        w,
                                        h,
                                    );
                                    canvas.draw_rect(rect, Paint::default().set_anti_alias(true).set_color(Color::RED));
                                });
                        }

                        paragraph.draw(canvas, paragraph_x.logical_value() + display_parameter.x(), paragraph_y + display_parameter.y());
                    }
                }
            })
            .on_click(|item, click_source| {
                if click_source == ClickSource::Mouse(MouseButton::Left) || click_source == ClickSource::Touch {
                    item.get_focused().set(true);
                }
            })
            .on_focus({
                let app_context = app_context.ref_clone();
                move |item, focused| {
                    app_context.window(|window| {
                        window.set_ime_allowed(focused);
                        window.set_ime_cursor_area(
                            Position::Logical(LogicalPosition::new(0.0, 0.0)),
                            Size::Logical(LogicalSize::new(1.0, 1.0)),
                        );
                    });
                }
            });

        Self {
            item: Item::new(app_context, Children::new(), item_event),
            property,
        }
    }

    pub fn item(self) -> Item {
        self.item
    }

    pub fn text(self, text: impl Into<TextProperty>) -> Self {
        {
            let id = self.item.get_id();
            let mut property = self.property.lock().unwrap();
            property.text.remove_observer(id);

            let app_context = self.item.get_app_context();
            let is_text_updated = property.is_text_updated.clone();
            property.text = text.into();
            property.text.add_observer(
                id,
                Box::new(move || {
                    *is_text_updated.lock().unwrap() = true;
                    app_context.request_re_layout();
                }),
            );
        }
        self
    }

    pub fn color(self, color: impl Into<ColorProperty>) -> Self {
        {
            let id = self.item.get_id();
            let mut property = self.property.lock().unwrap();
            property.color.remove_observer(id);

            let app_context = self.item.get_app_context();
            let is_text_updated = property.is_text_updated.clone();
            property.color = color.into();
            property.color.add_observer(
                id,
                Box::new(move || {
                    *is_text_updated.lock().unwrap() = true;
                    app_context.request_re_layout();
                }),
            );
        }
        self
    }

    pub fn font_size(self, font_size: impl Into<F32Property>) -> Self {
        {
            let id = self.item.get_id();
            let mut property = self.property.lock().unwrap();
            property.font_size.remove_observer(id);

            let app_context = self.item.get_app_context();
            let is_text_updated = property.is_text_updated.clone();
            property.font_size = font_size.into();
            property.font_size.add_observer(
                id,
                Box::new(move || {
                    *is_text_updated.lock().unwrap() = true;
                    app_context.request_re_layout();
                }),
            );
        }
        self
    }
}

impl Into<Item> for TextBlock {
    fn into(self) -> Item {
        self.item
    }
}

pub trait TextBlockExt {
    fn text_block(&self, text: impl Into<TextProperty>) -> TextBlock;
}

impl TextBlockExt for AppContext {
    fn text_block(&self, text: impl Into<TextProperty>) -> TextBlock {
        TextBlock::new(self.ref_clone()).text(text)
    }
}