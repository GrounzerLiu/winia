use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::app::SharedApp;
use crate::property::{FloatProperty, Gettable, ObservableProperty, Size};
use crate::uib::{Item, ItemEvent, DisplayParameter, LogicalX, measure_child, MeasureMode, Children};
use crate::uib::additional_property::BaseLine;

#[macro_export]
macro_rules! flex_layout {
    ($($child:expr)+) => {
        {
            let children = vec![$($child),*];
            let app = children.first().unwrap().get_app().clone();
            $crate::layout::FlexLayout::new(app, children)
        }
    }
}

/// When the layout direction is left to right, the Start is the left position of the layout, and the End is the right position of the layout.
/// When the layout direction is right to left, the Start is the right position of the layout, and the End is the left position of the layout.
#[derive(Clone, Copy, PartialEq)]
pub enum MainAxis {
    StartToEnd,
    EndToStart,
    TopToBottom,
    BottomToTop,
}

/// The Forward is start to end, or top to bottom, and the Reverse is end to start, or bottom to top.
#[derive(Clone, Copy, PartialEq)]
pub enum CrossAxis {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
}

/// The Start is the start position of the main axis, and the End is the end position of the main axis.
#[derive(Clone, Copy, PartialEq)]
pub enum FlexAlign {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// The Start is the start position of the cross axis, and the End is the end position of the cross axis.
#[derive(Clone, Copy, PartialEq)]
pub enum ItemAlign {
    Start,
    End,
    Center,
    Baseline,
    Stretch,
}

#[derive(Clone, Debug)]
struct Row {
    range: Range<usize>,
    row_min_width: f32,
    row_max_height: f32,
    baseline: f32,
}

struct FlexLayoutProperties {
    main_axis: ObservableProperty<MainAxis>,
    cross_axis: ObservableProperty<CrossAxis>,
    /// Whether the layout is wrapped when the child elements exceed the width or height of the layout.
    flex_wrap: ObservableProperty<FlexWrap>,
    /// The alignment of the child elements along the main axis.
    justify_content: ObservableProperty<FlexAlign>,
    /// The alignment of the child elements along the cross axis in the
    align_items: ObservableProperty<ItemAlign>,
    /// When a flex container has multiple flex lines, this property defines how flex items are aligned on the cross axis.
    align_content: ObservableProperty<FlexAlign>,
    /// The minimum space between items on the main axis.
    /// Note: This space refers to the distance between items and does not include the margins of the items.
    main_axis_minimum_space: FloatProperty,
    /// The minimum space between items on the cross axis.
    /// Note: This space refers to the distance between items and does not include the margins of the items.
    cross_axis_minimum_space: FloatProperty,
    rows: Vec<Row>,
}

impl Default for FlexLayoutProperties {
    fn default() -> Self {
        FlexLayoutProperties {
            main_axis: ObservableProperty::from_value(MainAxis::StartToEnd),
            cross_axis: ObservableProperty::from_value(CrossAxis::Forward),
            flex_wrap: ObservableProperty::from_value(FlexWrap::Wrap),
            justify_content: ObservableProperty::from_value(FlexAlign::Start),
            align_items: ObservableProperty::from_value(ItemAlign::Start),
            align_content: ObservableProperty::from_value(FlexAlign::Start),
            main_axis_minimum_space: 0.0.into(),
            cross_axis_minimum_space: 0.0.into(),
            rows: Vec::new(),
        }
    }
}

pub struct FlexLayout {
    item: Item,
    properties: Rc<Mutex<FlexLayoutProperties>>,
}

impl FlexLayout {
    pub fn new(app: SharedApp, children: Children) -> Self {
        let properties = Rc::new(Mutex::new(FlexLayoutProperties::default()));

        let mut item = Item::new(
            app,
            ItemEvent::default()
                .set_measure_event({// Measure the layout, get the expected width and height of the layout and its children
                    let properties = properties.clone();
                    move |item, width_measure_mode, height_measure_mode| {
                        let mut properties = properties.lock().unwrap();

                        let mut display_parameter = item.get_display_parameter();
                        display_parameter.init_from_item(item);

                        let max_width = display_parameter.max_width;
                        let max_height = display_parameter.max_height;
                        let min_width = display_parameter.min_width;
                        let min_height = display_parameter.min_height;

                        let mut measure_width = 0.0_f32;
                        let mut measure_height = 0.0_f32;

                        let flex_wrap = properties.flex_wrap.get();
                        let main_axis = properties.main_axis.get();
                        let cross_axis = properties.cross_axis.get();
                        let justify_content = properties.justify_content.get();
                        let align_items = properties.align_items.get();
                        let align_content = properties.align_content.get();

                        match width_measure_mode {
                            MeasureMode::Specified(width) => {
                                match height_measure_mode {
                                    MeasureMode::Specified(height) => {
                                        measure_width = width.clamp(min_width, max_width);
                                        measure_height = height.clamp(min_height, max_height);

                                        properties.rows.clear();

                                        if item.get_children().len() > 0 {
                                            let row = Row {
                                                range: 0..0,
                                                row_min_width: 0.0,
                                                row_max_height: 0.0,
                                                baseline: 0.0,
                                            };
                                            properties.rows.push(row);
                                        }

                                        item.get_children().items_mut().lock().unwrap().iter_mut().for_each(|mut child| {
                                            match main_axis {
                                                // The main axis is the horizontal direction
                                                MainAxis::StartToEnd | MainAxis::EndToStart => {
                                                    let (child_width_measure_mode, child_height_measure_mode) =
                                                        measure_child(child, &display_parameter, width_measure_mode, height_measure_mode);
                                                    child.measure(child_width_measure_mode, child_height_measure_mode);
                                                    
                                                    let child_display_parameter = child.get_display_parameter();
                                                    
                                                    let mut new_row = None;
                                                    if let Some(row) = properties.rows.last_mut() {
                                                        let new_width = row.row_min_width + child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end;
                                                        if new_width > (measure_width - display_parameter.padding_start - display_parameter.padding_end) && flex_wrap == FlexWrap::Wrap {
                                                            new_row = Some(Row {
                                                                range: row.range.end..row.range.end + 1,
                                                                row_min_width: child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end,
                                                                row_max_height: child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom,
                                                                baseline: if align_items == ItemAlign::Baseline && child.get_baseline() != None {
                                                                    child.get_baseline().unwrap() + child_display_parameter.margin_top
                                                                } else {
                                                                    0.0
                                                                },
                                                            });
                                                        } else {
                                                            row.range.end += 1;
                                                            row.row_min_width = new_width;
                                                            if align_items == ItemAlign::Baseline && child.get_baseline() != None {
                                                                let original_baseline = row.baseline;
                                                                let original_above_baseline = row.row_max_height - original_baseline;
                                                                let item_baseline = child.get_baseline().unwrap() + child_display_parameter.margin_top;
                                                                let item_above_baseline = child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom - item_baseline;
                                                                let new_baseline = original_baseline.max(item_baseline);
                                                                let new_above_baseline = original_above_baseline.max(item_above_baseline);
                                                                row.baseline = new_baseline;
                                                                row.row_max_height = row.row_max_height.max(new_above_baseline + new_baseline);
                                                            } else {
                                                                row.row_max_height = row.row_max_height.max(child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom);
                                                            }
                                                        }
                                                    }
                                                    if let Some(new_row) = new_row {
                                                        properties.rows.push(new_row);
                                                    }
                                                }
                                                // The main axis is the vertical direction
                                                MainAxis::TopToBottom | MainAxis::BottomToTop => {}
                                            }
                                        });

                                        if flex_wrap == FlexWrap::NoWrap && properties.rows.len() == 1 {
                                            properties.rows.first_mut().unwrap().
                                                row_max_height = measure_height;
                                        }

                                        if align_items == ItemAlign::Stretch {
                                            let average_height = (measure_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                            properties.rows.iter_mut().for_each(|row| {
                                                row.row_max_height = average_height;
                                                for i in row.range.clone() {
                                                    if let Some(child) = item.get_children().items_mut().lock().unwrap().get_mut(i) {
                                                        let mut child_display_parameter = child.get_display_parameter();
                                                        child_display_parameter.height = average_height - child_display_parameter.margin_top - child_display_parameter.margin_bottom;
                                                        child.set_display_parameter(&child_display_parameter);
                                                    }
                                                }
                                            });
                                        }
                                    }

                                    MeasureMode::Unspecified(height) => {}
                                }
                            }
                            MeasureMode::Unspecified(width) => {
                                match height_measure_mode {
                                    MeasureMode::Specified(height) => {}
                                    MeasureMode::Unspecified(height) => {}
                                }
                            }
                        }

                        display_parameter.width = measure_width;
                        display_parameter.height = measure_height;

                        if let Some(background) = item.get_background().lock().unwrap().as_mut() {
                            background.measure(MeasureMode::Specified(display_parameter.width), MeasureMode::Specified(display_parameter.height));
                        }

                        if let Some(foreground) = item.get_foreground().lock().unwrap().as_mut() {
                            foreground.measure(MeasureMode::Specified(display_parameter.width), MeasureMode::Specified(display_parameter.height));
                        }

                        item.set_display_parameter(&display_parameter);
                    }
                })
                .set_layout_event({
                    let properties = properties.clone();
                    move |item, relative_x, relative_y| {
                        let properties = properties.lock().unwrap();
                        let flex_wrap = properties.flex_wrap.get();
                        let main_axis = properties.main_axis.get();
                        let cross_axis = properties.cross_axis.get();
                        let justify_content = properties.justify_content.get();
                        let align_items = properties.align_items.get();
                        let align_content = properties.align_content.get();

                        let mut display_parameter = item.get_display_parameter();
                        let mut width = display_parameter.width;
                        let mut height = display_parameter.height;
                        display_parameter.relative_x = relative_x;
                        display_parameter.relative_y = relative_y;

                        item.set_display_parameter(&display_parameter);
                        
                        if let Some(background) = item.get_background().lock().unwrap().as_mut() {
                            background.layout(relative_x, relative_y);
                        }

                        if let Some(foreground) = item.get_foreground().lock().unwrap().as_mut() {
                            foreground.layout(relative_x, relative_y);
                        }


                        let mut x = LogicalX::new(item.get_layout_direction().get(), 0.0, display_parameter.width);
                        let mut y = 0.0;

                        let children_len = item.get_children().len();

                        let total_row_height = properties.rows.iter().fold(0.0, |sum, row| sum + row.row_max_height);
                        match main_axis {
                            MainAxis::StartToEnd => {
                                let mut row_y = match cross_axis {
                                    CrossAxis::Forward => {
                                        match align_content {
                                            FlexAlign::Start => {
                                                y + display_parameter.padding_top
                                            }
                                            FlexAlign::End => {
                                                y + height - total_row_height - display_parameter.padding_bottom
                                            }
                                            FlexAlign::Center => {
                                                y + (height - total_row_height) / 2.0
                                            }
                                            FlexAlign::SpaceBetween => {
                                                if properties.rows.len() == 1 {
                                                    y + (height - total_row_height) / 2.0
                                                } else {
                                                    y + display_parameter.padding_top
                                                }
                                            }
                                            FlexAlign::SpaceAround => {
                                                if properties.rows.len() == 1 {
                                                    y + (height - total_row_height) / 2.0
                                                } else {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    y + space / 2.0
                                                }
                                            }
                                            FlexAlign::SpaceEvenly => {
                                                if properties.rows.len() == 1 {
                                                    y + (height - total_row_height) / 2.0
                                                } else {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() + 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    y + space
                                                }
                                            }
                                        }
                                    }
                                    CrossAxis::Reverse => {
                                        match align_content {
                                            FlexAlign::Start => {
                                                y + height - display_parameter.padding_bottom
                                            }
                                            FlexAlign::End => {
                                                y + display_parameter.padding_top + total_row_height
                                            }
                                            FlexAlign::Center => {
                                                y + height - (height - total_row_height) / 2.0
                                            }
                                            FlexAlign::SpaceBetween => {
                                                if properties.rows.len() == 1 {
                                                    y + (height - total_row_height) / 2.0
                                                } else {
                                                    y + display_parameter.padding_top
                                                }
                                            }
                                            FlexAlign::SpaceAround => {
                                                if properties.rows.len() == 1 {
                                                    y + (height - total_row_height) / 2.0
                                                } else {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    y + space / 2.0
                                                }
                                            }
                                            FlexAlign::SpaceEvenly => {
                                                if properties.rows.len() == 1 {
                                                    y + (height - total_row_height) / 2.0
                                                } else {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() + 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    y + space
                                                }
                                            }
                                        }
                                    }
                                };
                                properties.rows.iter().for_each(|row| {
                                    let mut item_x = match justify_content {
                                        FlexAlign::Start => {
                                            x + display_parameter.padding_start
                                        }
                                        FlexAlign::End => {
                                            x + width - row.row_min_width - display_parameter.padding_end
                                        }
                                        FlexAlign::Center => {
                                            x + (width - row.row_min_width) / 2.0
                                        }
                                        FlexAlign::SpaceBetween => {
                                            if row.range.len() == 1 {
                                                x + (width - row.row_min_width) / 2.0
                                            } else {
                                                x + display_parameter.padding_start
                                            }
                                        }
                                        FlexAlign::SpaceAround => {
                                            if row.range.len() == 1 {
                                                x + (width - row.row_min_width) / 2.0
                                            } else {
                                                let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / row.range.len() as f32;
                                                if space < 0.0 {
                                                    space = 0.0;
                                                }
                                                x + space / 2.0
                                            }
                                        }
                                        FlexAlign::SpaceEvenly => {
                                            if row.range.len() == 1 {
                                                x + (width - row.row_min_width) / 2.0
                                            } else {
                                                let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / (row.range.len() + 1) as f32;
                                                if space < 0.0 {
                                                    space = 0.0;
                                                }
                                                x + space
                                            }
                                        }
                                    };
                                    for i in row.range.clone() {
                                        if let Some(child_item) = item.get_children().items_mut().lock().unwrap().get_mut(i) {
                                            let child_display_parameter = child_item.get_display_parameter();
                                            let child_y = match cross_axis {
                                                CrossAxis::Forward => {
                                                    match align_items {
                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                            row_y + child_display_parameter.margin_top
                                                        }
                                                        ItemAlign::End => {
                                                            row_y + row.row_max_height - child_display_parameter.margin_bottom - child_display_parameter.height
                                                        }
                                                        ItemAlign::Center => {
                                                            row_y + (row.row_max_height - child_display_parameter.height) / 2.0
                                                        }
                                                        ItemAlign::Baseline => {
                                                            if let Some(baseline) = child_item.get_baseline() {
                                                                row_y + row.baseline - baseline
                                                            } else {
                                                                row_y + child_display_parameter.margin_top
                                                            }
                                                        }
                                                    }
                                                }
                                                CrossAxis::Reverse => {
                                                    match align_items {
                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                            row_y - child_display_parameter.margin_bottom
                                                        }
                                                        ItemAlign::End => {
                                                            row_y - row.row_max_height + child_display_parameter.margin_top + child_display_parameter.height
                                                        }
                                                        ItemAlign::Center => {
                                                            row_y - (row.row_max_height - child_display_parameter.height) / 2.0
                                                        }
                                                        ItemAlign::Baseline => {
                                                            if let Some(baseline) = child_item.get_baseline() {
                                                                row_y + row.baseline - baseline
                                                            } else {
                                                                row_y + child_display_parameter.margin_top
                                                            }
                                                        }
                                                    }
                                                }
                                            };

                                            child_item.layout(
                                                item_x.physical_value(child_display_parameter.width),
                                                match cross_axis {
                                                    CrossAxis::Forward => {
                                                        child_y
                                                    }
                                                    CrossAxis::Reverse => {
                                                        child_y - child_display_parameter.height
                                                    }
                                                },
                                            );

                                            match justify_content {
                                                FlexAlign::Start | FlexAlign::End | FlexAlign::Center => {
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end;
                                                }
                                                FlexAlign::SpaceBetween => {
                                                    let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / (row.range.len() - 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end + space;
                                                }
                                                FlexAlign::SpaceAround => {
                                                    let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / row.range.len() as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end + space;
                                                }
                                                FlexAlign::SpaceEvenly => {
                                                    let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / (row.range.len() + 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end + space;
                                                }
                                            }
                                        }
                                    }

                                    match cross_axis {
                                        CrossAxis::Forward => {
                                            match align_content {
                                                FlexAlign::Start | FlexAlign::Center | FlexAlign::End => {
                                                    row_y += row.row_max_height;
                                                }
                                                FlexAlign::SpaceBetween => {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() - 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    row_y += row.row_max_height + space;
                                                }
                                                FlexAlign::SpaceAround => {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    row_y += row.row_max_height + space;
                                                }
                                                FlexAlign::SpaceEvenly => {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() + 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    row_y += row.row_max_height + space;
                                                }
                                            }
                                        }
                                        CrossAxis::Reverse => {
                                            match align_content {
                                                FlexAlign::Start | FlexAlign::Center | FlexAlign::End => {
                                                    row_y -= row.row_max_height;
                                                }
                                                FlexAlign::SpaceBetween => {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() - 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    row_y += row.row_max_height + space;
                                                }
                                                FlexAlign::SpaceAround => {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    row_y += row.row_max_height + space;
                                                }
                                                FlexAlign::SpaceEvenly => {
                                                    let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() + 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    row_y += row.row_max_height + space;
                                                }
                                            }
                                        }
                                    };
                                });
                            }
                            MainAxis::EndToStart => {
                                let mut row_y = match align_content {
                                    FlexAlign::Start => {
                                        y + display_parameter.padding_top
                                    }
                                    FlexAlign::End => {
                                        y + height - total_row_height - display_parameter.padding_bottom
                                    }
                                    FlexAlign::Center => {
                                        y + (height - total_row_height) / 2.0
                                    }
                                    FlexAlign::SpaceBetween => {
                                        if properties.rows.len() == 1 {
                                            y + (height - total_row_height) / 2.0
                                        } else {
                                            y + display_parameter.padding_top
                                        }
                                    }
                                    FlexAlign::SpaceAround => {
                                        if properties.rows.len() == 1 {
                                            y + (height - total_row_height) / 2.0
                                        } else {
                                            let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                            if space < 0.0 {
                                                space = 0.0;
                                            }
                                            y + space / 2.0
                                        }
                                    }
                                    FlexAlign::SpaceEvenly => {
                                        if properties.rows.len() == 1 {
                                            y + (height - total_row_height) / 2.0
                                        } else {
                                            let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() + 1) as f32;
                                            if space < 0.0 {
                                                space = 0.0;
                                            }
                                            y + space
                                        }
                                    }
                                };
                                properties.rows.iter().for_each(|row| {
                                    let mut item_x = match justify_content {
                                        FlexAlign::Start => {
                                            x + width - display_parameter.padding_end
                                        }
                                        FlexAlign::End => {
                                            x + display_parameter.padding_start + width
                                        }
                                        FlexAlign::Center => {
                                            x + width - (width - row.row_min_width) / 2.0
                                        }
                                        FlexAlign::SpaceBetween => {
                                            if row.range.len() == 1 {
                                                x + (width - row.row_min_width) / 2.0
                                            } else {
                                                x + display_parameter.padding_start
                                            }
                                        }
                                        FlexAlign::SpaceAround => {
                                            if row.range.len() == 1 {
                                                x + (width - row.row_min_width) / 2.0
                                            } else {
                                                let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / row.range.len() as f32;
                                                if space < 0.0 {
                                                    space = 0.0;
                                                }
                                                x + space / 2.0
                                            }
                                        }
                                        FlexAlign::SpaceEvenly => {
                                            if row.range.len() == 1 {
                                                x + (width - row.row_min_width) / 2.0
                                            } else {
                                                let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / (row.range.len() + 1) as f32;
                                                if space < 0.0 {
                                                    space = 0.0;
                                                }
                                                x + space
                                            }
                                        }
                                    };
                                    for i in row.range.clone() {
                                        if let Some(child_item) = item.get_children().items_mut().lock().unwrap().get_mut(i) {
                                            let child_display_parameter = child_item.get_display_parameter();
                                            let child_y = match cross_axis {
                                                CrossAxis::Forward => {
                                                    match align_items {
                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                            row_y + child_display_parameter.margin_top
                                                        }
                                                        ItemAlign::End => {
                                                            row_y + row.row_max_height - child_display_parameter.margin_bottom - child_display_parameter.height
                                                        }
                                                        ItemAlign::Center => {
                                                            row_y + (row.row_max_height - child_display_parameter.height) / 2.0
                                                        }
                                                        ItemAlign::Baseline => {
                                                            if let Some(baseline) = child_item.get_baseline() {
                                                                row_y + row.baseline - baseline
                                                            } else {
                                                                row_y + child_display_parameter.margin_top
                                                            }
                                                        }
                                                    }
                                                }
                                                CrossAxis::Reverse => {
                                                    0.0
                                                }
                                            };

                                            child_item.layout((item_x - child_display_parameter.width).physical_value(child_display_parameter.width), child_y);

                                            match justify_content {
                                                FlexAlign::Start | FlexAlign::End | FlexAlign::Center => {
                                                    item_x -= child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end;
                                                }
                                                FlexAlign::SpaceBetween => {
                                                    let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / (row.range.len() - 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end + space;
                                                }
                                                FlexAlign::SpaceAround => {
                                                    let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / row.range.len() as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end + space;
                                                }
                                                FlexAlign::SpaceEvenly => {
                                                    let mut space = (width - row.row_min_width - display_parameter.padding_start - display_parameter.padding_end) / (row.range.len() + 1) as f32;
                                                    if space < 0.0 {
                                                        space = 0.0;
                                                    }
                                                    item_x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end + space;
                                                }
                                            }
                                        }
                                    }
                                    match align_content {
                                        FlexAlign::Start | FlexAlign::Center | FlexAlign::End => {
                                            row_y += row.row_max_height;
                                        }
                                        FlexAlign::SpaceBetween => {
                                            let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() - 1) as f32;
                                            if space < 0.0 {
                                                space = 0.0;
                                            }
                                            row_y += row.row_max_height + space;
                                        }
                                        FlexAlign::SpaceAround => {
                                            let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / properties.rows.len() as f32;
                                            if space < 0.0 {
                                                space = 0.0;
                                            }
                                            row_y += row.row_max_height + space;
                                        }
                                        FlexAlign::SpaceEvenly => {
                                            let mut space = (height - total_row_height - display_parameter.padding_top - display_parameter.padding_bottom) / (properties.rows.len() + 1) as f32;
                                            if space < 0.0 {
                                                space = 0.0;
                                            }
                                            row_y += row.row_max_height + space;
                                        }
                                    }
                                });
                            }
                            MainAxis::TopToBottom => {}
                            MainAxis::BottomToTop => {}
                        }


                        /*                        match flex_wrap {
                                                    FlexWrap::NoWrap => {
                                                        fn calculate_y(y: f32, axis_start: AxisStart, align_items: ItemAlign, layout_params: &LayoutParams, child_layout_params: &LayoutParams) -> f32 {
                                                            let height = layout_params.height;
                                                            match axis_start {
                                                                AxisStart::StartTop | AxisStart::EndTop => {
                                                                    match align_items {
                                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                                            y + layout_params.padding_top + child_layout_params.margin_top
                                                                        }
                                                                        ItemAlign::End => {
                                                                            y + height - layout_params.padding_bottom - child_layout_params.margin_bottom - child_layout_params.height
                                                                        }
                                                                        ItemAlign::Center => {
                                                                            y + (height - layout_params.padding_top - layout_params.padding_bottom - child_layout_params.height) / 2.0
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                }
                                                                AxisStart::StartBottom | AxisStart::EndBottom => {
                                                                    match align_items {
                                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                                            y + height - layout_params.padding_bottom - child_layout_params.margin_bottom - child_layout_params.height
                                                                        }
                                                                        ItemAlign::End => {
                                                                            y + layout_params.padding_top + child_layout_params.margin_top
                                                                        }
                                                                        ItemAlign::Center => {
                                                                            y + (height - layout_params.padding_top - layout_params.padding_bottom - child_layout_params.height) / 2.0
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                }
                                                                _ => { 0.0 }
                                                            }
                                                        }

                                                        fn calculate_x(x: LogicalX, axis_start: AxisStart, align_items: ItemAlign, layout_params: &LayoutParams, child_layout_params: &LayoutParams) -> LogicalX {
                                                            let width = layout_params.width;
                                                            match axis_start {
                                                                AxisStart::TopStart | AxisStart::BottomStart => {
                                                                    match align_items {
                                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                                            x + layout_params.padding_start + child_layout_params.margin_start
                                                                        }
                                                                        ItemAlign::End => {
                                                                            x + width - layout_params.padding_end - child_layout_params.margin_end - child_layout_params.width
                                                                        }
                                                                        ItemAlign::Center => {
                                                                            x + (width - layout_params.padding_start - layout_params.padding_end - child_layout_params.width) / 2.0
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                }
                                                                AxisStart::TopEnd | AxisStart::BottomEnd => {
                                                                    match align_items {
                                                                        ItemAlign::Start | ItemAlign::Stretch => {
                                                                            x + width - layout_params.padding_end - child_layout_params.margin_end - child_layout_params.width
                                                                        }
                                                                        ItemAlign::End => {
                                                                            x + layout_params.padding_start + child_layout_params.margin_start
                                                                        }
                                                                        ItemAlign::Center => {
                                                                            x + (width - layout_params.padding_start - layout_params.padding_end - child_layout_params.width) / 2.0
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                }
                                                                _ => { LogicalX::new(LayoutDirection::LeftToRight, 0.0, 0.0, 0.0) }
                                                            }
                                                        }

                                                        match axis_start {
                                                            AxisStart::StartTop | AxisStart::StartBottom => {
                                                                match justify_content {
                                                                    FlexAlign::Start => {
                                                                        let mut child_x = x + layout_params.padding_start;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x + child_layout_params.margin_start;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x += child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                                                                        });
                                                                    }
                                                                    FlexAlign::End => {
                                                                        let mut child_x = x + width - properties.children_occupied_space.width - layout_params.padding_end;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x + child_layout_params.margin_start;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x += child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                                                                        });
                                                                    }
                                                                    FlexAlign::Center => {
                                                                        let mut child_x = x + (width - properties.children_occupied_space.width) / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x + child_layout_params.margin_start;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x += child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceBetween => {
                                                                        let mut space = (width - properties.children_occupied_space.width - layout_params.padding_start - layout_params.padding_end) / (children_len - 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_x = x + layout_params.padding_start;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x + child_layout_params.margin_start;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x += child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceAround => {
                                                                        let mut space = (width - properties.children_occupied_space.width - layout_params.padding_start - layout_params.padding_end) / children_len as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_x = x + layout_params.padding_start + space / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x + child_layout_params.margin_start;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x += child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceEvenly => {
                                                                        let mut space = (width - properties.children_occupied_space.width - layout_params.padding_start - layout_params.padding_end) / (children_len + 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_x = x + layout_params.padding_start + space;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x + child_layout_params.margin_start;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x += child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end + space;
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                            AxisStart::EndTop | AxisStart::EndBottom => {
                                                                match justify_content {
                                                                    FlexAlign::Start => {
                                                                        let mut child_x = x + width - layout_params.padding_end;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x - child_layout_params.width - child_layout_params.margin_end;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x -= child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                                                                        });
                                                                    }
                                                                    FlexAlign::End => {
                                                                        let mut child_x = x + layout_params.padding_start + properties.children_occupied_space.width;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x - child_layout_params.width - child_layout_params.margin_end;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x -= child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                                                                        });
                                                                    }
                                                                    FlexAlign::Center => {
                                                                        let mut child_x = x + width - (width - properties.children_occupied_space.width) / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x - child_layout_params.width - child_layout_params.margin_end;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x -= child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceBetween => {
                                                                        let mut space = (width - properties.children_occupied_space.width - layout_params.padding_start - layout_params.padding_end) / (children_len - 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_x = x + width - layout_params.padding_end;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x - child_layout_params.width - child_layout_params.margin_end;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x -= child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceAround => {
                                                                        let mut space = (width - properties.children_occupied_space.width - layout_params.padding_start - layout_params.padding_end) / children_len as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_x = x + width - layout_params.padding_end - space / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x - child_layout_params.width - child_layout_params.margin_end;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x -= child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceEvenly => {
                                                                        let mut space = (width - properties.children_occupied_space.width - layout_params.padding_start - layout_params.padding_end) / (children_len + 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_x = x + width - layout_params.padding_end - space;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let x = child_x - child_layout_params.width - child_layout_params.margin_end;
                                                                            let y = calculate_y(y, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_x -= child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end + space;
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                            AxisStart::TopStart | AxisStart::TopEnd => {
                                                                match justify_content {
                                                                    FlexAlign::Start => {
                                                                        let mut child_y = y + layout_params.padding_top;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y + child_layout_params.margin_top;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y += child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom;
                                                                        });
                                                                    }
                                                                    FlexAlign::End => {
                                                                        let mut child_y = y + height - properties.children_occupied_space.height - layout_params.padding_bottom;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y + child_layout_params.margin_top;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y += child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom;
                                                                        });
                                                                    }
                                                                    FlexAlign::Center => {
                                                                        let mut child_y = y + (height - properties.children_occupied_space.height) / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y + child_layout_params.margin_top;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y += child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceBetween => {
                                                                        let mut space = (height - properties.children_occupied_space.height - layout_params.padding_top - layout_params.padding_bottom) / (children_len - 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_y = y + layout_params.padding_top;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y + child_layout_params.margin_top;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y += child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceAround => {
                                                                        let mut space = (height - properties.children_occupied_space.height - layout_params.padding_top - layout_params.padding_bottom) / children_len as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_y = y + layout_params.padding_top + space / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y + child_layout_params.margin_top;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y += child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceEvenly => {
                                                                        let mut space = (height - properties.children_occupied_space.height - layout_params.padding_top - layout_params.padding_bottom) / (children_len + 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_y = y + layout_params.padding_top + space;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y + child_layout_params.margin_top;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y += child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom + space;
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                            AxisStart::BottomStart | AxisStart::BottomEnd => {
                                                                match justify_content {
                                                                    FlexAlign::Start => {
                                                                        let mut child_y = y + height - layout_params.padding_bottom;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y - child_layout_params.height - child_layout_params.margin_bottom;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y -= child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom;
                                                                        });
                                                                    }
                                                                    FlexAlign::End => {
                                                                        let mut child_y = y + layout_params.padding_top + properties.children_occupied_space.height;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y - child_layout_params.height - child_layout_params.margin_bottom;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y -= child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom;
                                                                        });
                                                                    }
                                                                    FlexAlign::Center => {
                                                                        let mut child_y = y + height - (height - properties.children_occupied_space.height) / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y - child_layout_params.height - child_layout_params.margin_bottom;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y -= child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceBetween => {
                                                                        let mut space = (height - properties.children_occupied_space.height - layout_params.padding_top - layout_params.padding_bottom) / (children_len - 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_y = y + height - layout_params.padding_bottom;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y - child_layout_params.height - child_layout_params.margin_bottom;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y -= child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceAround => {
                                                                        let mut space = (height - properties.children_occupied_space.height - layout_params.padding_top - layout_params.padding_bottom) / children_len as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_y = y + height - layout_params.padding_bottom - space / 2.0;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y - child_layout_params.height - child_layout_params.margin_bottom;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y -= child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom + space;
                                                                        });
                                                                    }
                                                                    FlexAlign::SpaceEvenly => {
                                                                        let mut space = (height - properties.children_occupied_space.height - layout_params.padding_top - layout_params.padding_bottom) / (children_len + 1) as f32;
                                                                        if space < 0.0 {
                                                                            space = 0.0;
                                                                        }
                                                                        let mut child_y = y + height - layout_params.padding_bottom - space;
                                                                        item.get_children_mut().iter_mut().for_each(|child| {
                                                                            let mut child_layout_params = child.get_layout_params().clone();
                                                                            let y = child_y - child_layout_params.height - child_layout_params.margin_bottom;
                                                                            let x = calculate_x(x, axis_start, align_items, &layout_params, &child_layout_params);
                                                                            child.layout(x.physical_value(child_layout_params.width), y);
                                                                            child_y -= child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom + space;
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    FlexWrap::Wrap => {
                                                        match axis_start {
                                                            AxisStart::StartTop | AxisStart::StartBottom => {
                                                                match justify_content {
                                                                    FlexAlign::Start => {

                                                                    }
                                                                    _ => {}
                                                                }
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                }*/
                    }
                })
        );
        item.set_children(children);

        FlexLayout {
            item,
            properties,
        }
    }

    pub fn item(self) -> Item {
        self.item
    }
}

fn measure_child_stretch(is_height: bool, child: &Item, parent_layout_params: &DisplayParameter, width_measure_mode: MeasureMode, height_measure_mode: MeasureMode) -> (MeasureMode, MeasureMode) {
    let display_parameter = child.get_display_parameter();
    let max_width = match width_measure_mode {
        MeasureMode::Specified(width) => width,
        MeasureMode::Unspecified(width) => width,
    } - display_parameter.margin_start - display_parameter.margin_end - parent_layout_params.padding_start - parent_layout_params.margin_end;
    let max_height = match height_measure_mode {
        MeasureMode::Specified(height) => height,
        MeasureMode::Unspecified(height) => height,
    } - display_parameter.margin_top - display_parameter.margin_bottom - parent_layout_params.padding_top - parent_layout_params.margin_bottom;

    let child_width = child.get_width().get();
    let child_height = child.get_height().get();

    let child_width_measure_mode =
        if is_height {
            match child_width {
                Size::Default => MeasureMode::Unspecified(max_width),
                Size::Fill => MeasureMode::Specified(max_width),
                Size::Fixed(width) => MeasureMode::Specified(width),
                Size::Relative(scale) => MeasureMode::Specified(max_width * scale),
            }
        } else {
            MeasureMode::Specified(max_width)
        };

    let child_height_measure_mode =
        if is_height {
            MeasureMode::Specified(max_height)
        } else {
            match child_height {
                Size::Default => MeasureMode::Unspecified(max_height),
                Size::Fill => MeasureMode::Specified(max_height),
                Size::Fixed(height) => MeasureMode::Specified(height),
                Size::Relative(scale) => MeasureMode::Specified(max_height * scale),
            }
        };

    (child_width_measure_mode, child_height_measure_mode)
}

pub trait FlexLayoutExt {
    fn flex_layout(&self, children: Children) -> FlexLayout;
}

impl FlexLayoutExt for SharedApp {
    fn flex_layout(&self, children: Children) -> FlexLayout {
        FlexLayout::new(self.clone(), children)
    }
}