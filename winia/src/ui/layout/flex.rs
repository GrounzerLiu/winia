use crate::shared::{Children, Gettable, Observable, Shared, SharedAlignment, SharedUsize};
use crate::ui::app::WindowContext;
use crate::ui::item::{CustomProperty, ItemData, LogicalX, MeasureMode, Orientation};
use crate::ui::Item;
use proc_macro::item;
use std::ops::Not;
use strum_macros::EnumString;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum FlexDirection {
    Horizontal,
    HorizontalReverse,
    Vertical,
    VerticalReverse,
}

impl FlexDirection {
    pub fn orientation(&self) -> Orientation {
        match self {
            FlexDirection::Horizontal | FlexDirection::HorizontalReverse => Orientation::Horizontal,
            FlexDirection::Vertical | FlexDirection::VerticalReverse => Orientation::Vertical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Baseline,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum AlignContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

pub trait FlexGrow {
    fn flex_grow(self, value: impl Into<SharedUsize>) -> Self;
}

pub trait GetFlexGrow {
    fn get_flex_grow(&self) -> Option<SharedUsize>;
}

impl GetFlexGrow for ItemData {
    fn get_flex_grow(&self) -> Option<SharedUsize> {
        match self.get_custom_property("flex_grow") {
            None => None,
            Some(CustomProperty::Usize(f)) => Some(f.clone()),
            _ => None,
        }
    }
}

impl FlexGrow for Item {
    /// Set the flex-grow shared of the item.
    ///
    /// It only works when the parent of the item is Flex.
    ///
    /// If the value is 0, the item will not grow.
    ///
    /// The lock with value greater than 0 will be resized proportionally to the remaining space.
    fn flex_grow(self, flex_grow: impl Into<SharedUsize>) -> Self {
        let id = self.data().get_id();
        // if let Some(CustomProperty::Any(flex_grow)) =
        //     self.data().get_custom_property_mut("flex_grow")
        // {
        //     if let Some(flex_grow) = flex_grow.downcast_mut::<SharedUsize>() {
        //         flex_grow.remove_observer(id);
        //     }
        // }
        if let Some(CustomProperty::Usize(flex_grow)) = self.data().get_custom_property("flex_grow")
        {
            flex_grow.remove_observer(id);
        }

        let event_loop_proxy = self.data().get_window_context().event_loop_proxy().clone();
        let mut flex_grow = flex_grow.into();
        flex_grow.add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_layout();
            }),
        );
        self.data()
            .custom_property("flex_grow", CustomProperty::Usize(flex_grow));
        self
    }
}

#[derive(Clone)]
struct FlexProperties {
    pub direction: Shared<FlexDirection>,
    pub wrap: Shared<FlexWrap>,
    pub justify_content: Shared<JustifyContent>,
    pub align_items: Shared<AlignItems>,
    pub align_content: Shared<AlignContent>,
    pub main_axis_gap: Shared<f32>,
    pub cross_axis_gap: Shared<f32>,
}

struct Line {
    start_index: usize,
    count: usize,
    /// The orientation of the line.
    orientation: Orientation,
    /// The alignment of the lock in the line.
    align_items: AlignItems,
    gap: f32,
    /// The maximum main axis size of the line.
    ///
    /// If None, the line will grow as much as possible.
    max_main_axis_size: Option<f32>,
    width: f32,
    height: f32,
    under_baseline: f32,
    over_baseline: f32,
}

impl Line {
    pub fn new(
        start_index: usize,
        orientation: Orientation,
        align_items: AlignItems,
        gap: f32,
        max_main_axis_size: impl Into<Option<f32>>,
    ) -> Self {
        Self {
            start_index,
            count: 0,
            orientation,
            align_items,
            gap,
            max_main_axis_size: max_main_axis_size.into(),
            width: 0.0,
            height: 0.0,
            under_baseline: 0.0,
            over_baseline: 0.0,
        }
    }

    fn main_axis_size(&self) -> f32 {
        match self.orientation {
            Orientation::Horizontal => self.width,
            Orientation::Vertical => self.height,
        }
    }

    fn cross_axis_size(&self) -> f32 {
        match self.orientation {
            Orientation::Horizontal => self.height,
            Orientation::Vertical => self.width,
        }
    }

    fn set_main_axis_size(&mut self, size: f32) {
        match self.orientation {
            Orientation::Horizontal => self.width = size,
            Orientation::Vertical => self.height = size,
        }
    }

    fn cross_axis_size_mut(&mut self) -> &mut f32 {
        match self.orientation {
            Orientation::Horizontal => &mut self.height,
            Orientation::Vertical => &mut self.width,
        }
    }

    fn set_cross_axis_size(&mut self, size: f32) {
        match self.orientation {
            Orientation::Horizontal => self.height = size,
            Orientation::Vertical => self.width = size,
        }
    }

    pub fn width(&self) -> f32 {
        self.width
            + match self.orientation {
                Orientation::Horizontal => {
                    self.gap
                        * if self.count == 0 {
                            0.0
                        } else {
                            (self.count - 1) as f32
                        }
                }
                Orientation::Vertical => 0.0,
            }
    }

    pub fn height(&self) -> f32 {
        self.height
            + match self.orientation {
                Orientation::Horizontal => 0.0,
                Orientation::Vertical => {
                    self.gap
                        * if self.count == 0 {
                            0.0
                        } else {
                            (self.count - 1) as f32
                        }
                }
            }
    }

    /// Check if the line exceeds the maximum main axis size when adding a new item.
    fn is_exceed(&self, size: f32) -> bool {
        let max_main_axis_size = self.max_main_axis_size.unwrap_or(f32::MAX);
        let count = self.count as f32;
        self.main_axis_size() + size + self.gap * count > max_main_axis_size
    }

    pub fn add_item(&mut self, item: &Item) {
        let mut data = item.data();
        let child_param = data.get_measure_parameter();
        let child_main_axis_size = child_param.size(self.orientation);
        let child_cross_axis_size = child_param.size(self.orientation.not());
        let child_main_axis_margin = data.get_margin(self.orientation);
        let child_cross_axis_margin = data.get_margin(self.orientation.not());

        let child_main_axis_occupied_size = child_main_axis_size + child_main_axis_margin;
        let child_cross_axis_occupied_size = child_cross_axis_size + child_cross_axis_margin;

        self.set_main_axis_size(self.main_axis_size() + child_main_axis_occupied_size);

        if self.align_items == AlignItems::Baseline {
            let child_baseline = if self.orientation == Orientation::Horizontal {
                data.get_baseline()
            } else {
                None
            };

            if let Some(child_baseline) = child_baseline {
                let under_baseline = child_baseline + data.get_margin_top().get();
                let over_baseline =
                    child_cross_axis_size - child_baseline + data.get_margin_bottom().get();
                self.under_baseline = self.under_baseline.max(under_baseline);
                self.over_baseline = self.over_baseline.max(over_baseline);
            }

            let baseline_occupied_size = self.over_baseline + self.under_baseline;
            let max = child_cross_axis_occupied_size.max(baseline_occupied_size);

            self.set_cross_axis_size(self.cross_axis_size().max(max));
            self.count += 1;
        } else {
            self.set_cross_axis_size(self.cross_axis_size().max(child_cross_axis_occupied_size));
            self.count += 1;
        }
    }

    /// Try to add an item to the line. Not really adding it, just calculating the size of the line.
    pub fn try_add_item(&mut self, item: &Item) -> bool {
        let mut data = item.data();
        let child_param = data.get_measure_parameter();
        let child_main_axis_size = child_param.size(self.orientation);
        let child_main_axis_margin = data.get_margin(self.orientation);
        drop(data);

        let child_main_axis_occupied_size = child_main_axis_size + child_main_axis_margin;

        if self.is_exceed(child_main_axis_occupied_size) {
            false
        } else {
            self.add_item(item);
            true
        }
    }
}

struct Lines {
    lines: Vec<Line>,
    orientation: Orientation,
    gap: f32,
    width: f32,
    height: f32,
}

impl Lines {
    pub fn new(
        item: &ItemData,
        orientation: Orientation,
        align_items: AlignItems,
        main_axis_gap: f32,
        cross_axis_gap: f32,
        max_main_axis_size: Option<f32>,
    ) -> Self {
        let mut lines = vec![];
        let mut line = Line::new(
            0,
            orientation,
            align_items,
            main_axis_gap,
            max_main_axis_size,
        );

        // Iterate over the children of the item and add them to the line.
        for (index, child) in item.get_children().lock().iter().enumerate() {
            // Try to add the child to the current line.
            if !line.try_add_item(child) {
                // Fail to add the child, so push the current line to the lines vector and create a new line.
                if line.count == 0 {
                    // If failed to add the child and the line is empty,
                    // it means the child is too large to fit in the line.
                    // So we need to force add the child to the line.
                    line.add_item(child);
                    lines.push(line);
                    line = Line::new(
                        index,
                        orientation,
                        align_items,
                        main_axis_gap,
                        max_main_axis_size,
                    );
                } else {
                    lines.push(line);
                    line = Line::new(
                        index,
                        orientation,
                        align_items,
                        main_axis_gap,
                        max_main_axis_size,
                    );
                    // Add the child to the new line.
                    line.add_item(child);
                }
            }
        }

        if line.count > 0 {
            lines.push(line);
        }

        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        match orientation {
            Orientation::Horizontal => {
                for line in lines.iter() {
                    width = width.max(line.width());
                    height += line.height();
                }
            }
            Orientation::Vertical => {
                for line in lines.iter() {
                    height = height.max(line.height());
                    width += line.width();
                }
            }
        }

        Self {
            lines,
            orientation,
            gap: cross_axis_gap,
            width,
            height,
        }
    }

    pub fn width(&self) -> f32 {
        self.width
            + match self.orientation {
                Orientation::Horizontal => {
                    self.gap
                        * if self.lines.is_empty() {
                            0.0
                        } else {
                            (self.lines.len() - 1) as f32
                        }
                }
                Orientation::Vertical => 0.0,
            }
    }

    pub fn height(&self) -> f32 {
        self.height
            + match self.orientation {
                Orientation::Horizontal => 0.0,
                Orientation::Vertical => {
                    self.gap
                        * if self.lines.is_empty() {
                            0.0
                        } else {
                            (self.lines.len() - 1) as f32
                        }
                }
            }
    }

    pub fn lines(&self) -> &Vec<Line> {
        &self.lines
    }
}

/// Calculate the size of the flex item.
fn calculate_size(
    item: &ItemData,
    orientation: Orientation,
    align_items: AlignItems,
    main_axis_gap: f32,
    cross_axis_gap: f32,
    max_main_axis_size: Option<f32>,
) -> (f32, f32) {
    let l = Lines::new(
        item,
        orientation,
        align_items,
        main_axis_gap,
        cross_axis_gap,
        max_main_axis_size,
    );

    let lines = l.lines();

    let max_main_axis_size = max_main_axis_size.unwrap_or(f32::MAX);

    /*    match orientation {
        Orientation::Horizontal => {
            let mut width = 0.0_f32;
            let mut height = 0.0_f32;
            for line in lines.iter() {
                width = width.max(line.width());
                height += line.height();
            }
            (
                if lines.len() == 1 {
                    width
                } else {
                    max_main_axis_size
                },
                height
                    + cross_axis_gap
                        * if lines.is_empty() {
                            0.0
                        } else {
                            (lines.len() - 1) as f32
                        },
            )
        }
        Orientation::Vertical => {
            let mut width = 0.0_f32;
            let mut height = 0.0_f32;
            for line in lines.iter() {
                height = height.max(line.height());
                width += line.width();
            }
            (
                width
                    + cross_axis_gap
                        * if lines.is_empty() {
                            0.0
                        } else {
                            (lines.len() - 1) as f32
                        },
                if lines.len() == 1 {
                    height
                } else {
                    max_main_axis_size
                },
            )
        }
    }*/

    (l.width(), l.height())
}

#[item(children: impl Into<Children>)]
pub struct Flex {
    item: Item,
    property: Shared<FlexProperties>,
}

impl Flex {
    pub fn new(app_context: &WindowContext, children: impl Into<Children>) -> Self {
        let property = Shared::from(FlexProperties {
            direction: FlexDirection::Horizontal.into(),
            wrap: FlexWrap::NoWrap.into(),
            justify_content: JustifyContent::Start.into(),
            align_items: AlignItems::Start.into(),
            align_content: AlignContent::Start.into(),
            main_axis_gap: 0.0.into(),
            cross_axis_gap: 0.0.into(),
        });

        let item = Item::new(app_context, children.into());
        item.data()
            .set_measure({
                let property = property.clone();
                move |item, width_mode, height_mode| {
                    let property = property.lock();
                    item.measure_children(width_mode, height_mode);
                    let direction = property.direction.get();
                    let wrap = property.wrap.get();
                    let align_items = property.align_items.get();

                    let max_width = match width_mode {
                        MeasureMode::Specified(width) => item.clamp_width(width),
                        MeasureMode::Unspecified(_) => item.get_max_width().get(),
                    };

                    let max_height = match height_mode {
                        MeasureMode::Specified(height) => item.clamp_height(height),
                        MeasureMode::Unspecified(_) => item.get_max_height().get(),
                    };

                    let orientation = direction.orientation();
                    let (children_width, children_height) = calculate_size(
                        item,
                        orientation,
                        align_items,
                        property.main_axis_gap.get(),
                        property.cross_axis_gap.get(),
                        if wrap == FlexWrap::NoWrap {
                            None
                        } else if orientation == Orientation::Horizontal {
                            Some(max_width)
                        } else {
                            Some(max_height)
                        },
                    );

                    let min_width = item.get_min_width().get();
                    let min_height = item.get_min_height().get();
                    let max_width = max_width.max(min_width);
                    let max_height = max_height.max(min_height);
                    let padding_horizontal = item.get_padding(Orientation::Horizontal);
                    let padding_vertical = item.get_padding(Orientation::Vertical);
                    let measure_parameter = item.get_measure_parameter();
                    match width_mode {
                        MeasureMode::Specified(width) => {
                            measure_parameter.width = width.clamp(min_width, max_width);
                            match height_mode {
                                MeasureMode::Specified(height) => {
                                    measure_parameter.height = height.clamp(min_height, max_height);
                                }
                                MeasureMode::Unspecified(_) => {
                                    measure_parameter.height = (children_height + padding_vertical)
                                        .clamp(min_height, max_height);
                                }
                            }
                        }
                        MeasureMode::Unspecified(_) => {
                            measure_parameter.width =
                                (children_width + padding_horizontal).clamp(min_width, max_width);
                            match height_mode {
                                MeasureMode::Specified(height) => {
                                    measure_parameter.height = height.clamp(min_height, max_height);
                                }
                                MeasureMode::Unspecified(_) => {
                                    measure_parameter.height = (children_height + padding_vertical)
                                        .clamp(min_height, max_height);
                                }
                            }
                        }
                    }
                }
            })
            .set_layout({
                let property = property.clone();
                move |item, width, height| {
                    if item.get_children().lock().is_empty() {
                        return;
                    }

                    let padding_start = item.get_padding_start().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_bottom = item.get_padding_bottom().get();

                    let property = property.lock();
                    let direction = property.direction.get();
                    let wrap = property.wrap.get();
                    let justify_content = property.justify_content.get();
                    let align_items = property.align_items.get();
                    let align_content = property.align_content.get();
                    let main_axis_gap = property.main_axis_gap.get();
                    let cross_axis_gap = property.cross_axis_gap.get();

                    let lines = Lines::new(
                        item,
                        direction.orientation(),
                        align_items,
                        main_axis_gap,
                        cross_axis_gap,
                        if wrap == FlexWrap::NoWrap {
                            None
                        } else if direction.orientation() == Orientation::Horizontal {
                            Some(width - padding_start - padding_end)
                        } else {
                            Some(height - padding_top - padding_bottom)
                        },
                    );

                    let line_count = lines.lines().len();

                    let lines_width = lines.width();
                    let lines_height = lines.height();

                    match direction {
                        FlexDirection::Horizontal | FlexDirection::HorizontalReverse => {
                            let lines_height = if wrap == FlexWrap::NoWrap {
                                height - padding_top - padding_bottom
                            } else {
                                lines_height
                            };
                            let remaining_space_between_lines =
                                height - lines_height - padding_top - padding_bottom;
                            let mut line_stretch = None;
                            let mut space_between_lines = 0.0;

                            let mut y = if wrap != FlexWrap::WrapReverse {
                                match align_content {
                                    AlignContent::Start => padding_top,
                                    AlignContent::End => height - lines_height - padding_bottom,
                                    AlignContent::Center => (height - lines_height) / 2.0,
                                    AlignContent::SpaceBetween => {
                                        if remaining_space_between_lines > 0.0 && line_count > 1 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count - 1) as f32;
                                        }
                                        padding_top
                                    }
                                    AlignContent::SpaceAround => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines =
                                                remaining_space_between_lines / line_count as f32;
                                        }
                                        padding_top + space_between_lines / 2.0
                                    }
                                    AlignContent::SpaceEvenly => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count + 1) as f32;
                                        }
                                        padding_top + space_between_lines
                                    }
                                    AlignContent::Stretch => {
                                        if wrap != FlexWrap::NoWrap {
                                            line_stretch = Some(
                                                (height - padding_top - padding_bottom)
                                                    / line_count as f32,
                                            );
                                        }
                                        padding_top
                                    }
                                }
                            } else {
                                let default_y = height - padding_bottom;
                                match align_content {
                                    AlignContent::Start => default_y,
                                    AlignContent::End => lines_height + padding_top,
                                    AlignContent::Center => default_y - (height - lines_height) / 2.0,
                                    AlignContent::SpaceBetween => {
                                        if remaining_space_between_lines > 0.0 && line_count > 1 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count - 1) as f32;
                                        }
                                        default_y
                                    }
                                    AlignContent::SpaceAround => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines =
                                                remaining_space_between_lines / line_count as f32;
                                        }
                                        default_y - space_between_lines / 2.0
                                    }
                                    AlignContent::SpaceEvenly => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count + 1) as f32;
                                        }
                                        default_y - space_between_lines
                                    }
                                    AlignContent::Stretch => {
                                        line_stretch = Some(
                                            (height - padding_top - padding_bottom)
                                                / line_count as f32,
                                        );
                                        default_y
                                    }
                                }
                            };

                            for line in lines.lines().iter() {
                                let start_index = line.start_index;
                                let count = line.count;
                                let line_width = line.width();

                                let total_grow = {
                                    let mut total_grow = 0_usize;
                                    for index in start_index..start_index + count {
                                        let children = item.get_children().lock();
                                        let child = children.get(index).unwrap();
                                        {
                                            let child_data = child.data();
                                            if let Some(grow) = child_data.get_flex_grow() {
                                                total_grow += grow.get();
                                            }
                                        }
                                    }
                                    total_grow
                                };

                                let remaining_space_between_items =
                                    width - line_width - padding_start - padding_end;
                                let mut space_between_items = 0.0_f32;
                                let raw_x = if direction == FlexDirection::Horizontal {
                                    match justify_content {
                                        JustifyContent::Start => padding_start,
                                        JustifyContent::End => width - padding_end - line_width,
                                        JustifyContent::Center => (width - line_width) / 2.0,
                                        JustifyContent::SpaceBetween => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 1.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count - 1.0);
                                                }
                                            }
                                            padding_start
                                        }
                                        JustifyContent::SpaceAround => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items / count;
                                                }
                                                space_between_items / 2.0
                                            } else {
                                                padding_start
                                            }
                                        }
                                        JustifyContent::SpaceEvenly => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count + 1.0);
                                                }
                                                space_between_items
                                            } else {
                                                padding_start
                                            }
                                        }
                                    }
                                } else {
                                    let default_x = width - padding_end;
                                    match justify_content {
                                        JustifyContent::Start => default_x,
                                        JustifyContent::End => padding_start + line_width,
                                        JustifyContent::Center => default_x - (width - line_width) / 2.0,
                                        JustifyContent::SpaceBetween => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 1.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count - 1.0);
                                                }
                                            }
                                            default_x
                                        }
                                        JustifyContent::SpaceAround => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items / count;
                                                }
                                                default_x - space_between_items / 2.0
                                            } else {
                                                default_x
                                            }
                                        }
                                        JustifyContent::SpaceEvenly => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count + 1.0);
                                                }
                                                default_x - space_between_items
                                            } else {
                                                default_x
                                            }
                                        }
                                    }
                                };

                                let mut x =
                                    LogicalX::new(item.get_layout_direction().get(), raw_x, width);

                                for index in start_index..start_index + count {
                                    let mut children = item.get_children().lock();
                                    let child = children.get_mut(index).unwrap();
                                    let mut child_data = child.data();
                                    let child_param = child_data.get_measure_parameter();

                                    let mut child_width = child_param.width;
                                    let mut child_height = if let Some(line_stretch) = line_stretch
                                    {
                                        line_stretch
                                    } else {
                                        child_param.height
                                    };

                                    let child_margin_start = child_data.get_margin_start().get();
                                    let child_margin_end = child_data.get_margin_end().get();
                                    let child_margin_top = child_data.get_margin_top().get();
                                    let child_margin_bottom = child_data.get_margin_bottom().get();

                                    if remaining_space_between_items > 0.0 && total_grow > 0 {
                                        let flex_grow = child_data
                                            .get_flex_grow()
                                            .map(|v| v.get())
                                            .unwrap_or(0);
                                        if flex_grow > 0 {
                                            child_width += remaining_space_between_items
                                                * (flex_grow as f32 / total_grow as f32);
                                        }
                                    }

                                    let line_height = if wrap == FlexWrap::NoWrap {
                                        height - padding_top - padding_bottom
                                    } else {
                                        line.height()
                                    };

                                    let child_y = if wrap != FlexWrap::WrapReverse {
                                        y + if line_stretch.is_none() {
                                            match align_items {
                                                AlignItems::Start => child_margin_top,
                                                AlignItems::End => {
                                                    line_height - child_margin_bottom - child_height
                                                }
                                                AlignItems::Center => {
                                                    (line_height - child_height) / 2.0
                                                }
                                                AlignItems::Baseline => {
                                                    let child_baseline = child_data.get_baseline();
                                                    match child_baseline {
                                                        None => child_margin_top,
                                                        Some(baseline) => {
                                                            line.under_baseline - baseline
                                                        }
                                                    }
                                                }
                                                AlignItems::Stretch => {
                                                    child_height = line_height
                                                        - child_margin_top
                                                        - child_margin_bottom;
                                                    child_margin_top
                                                }
                                            }
                                        } else {
                                            0.0
                                        }
                                    } else {
                                        y - if line_stretch.is_none() {
                                            match align_items {
                                                AlignItems::Start => child_margin_bottom,
                                                AlignItems::End => {
                                                    line_height - child_margin_top - child_height
                                                }
                                                AlignItems::Center => {
                                                    (line_height - child_height) / 2.0
                                                }
                                                AlignItems::Baseline => {
                                                    let child_baseline = child_data.get_baseline();
                                                    match child_baseline {
                                                        None => child_margin_bottom,
                                                        Some(baseline) => {
                                                            line.over_baseline - (child_height - baseline)
                                                        }
                                                    }
                                                }
                                                AlignItems::Stretch => {
                                                    child_height = line_height
                                                        - child_margin_top
                                                        - child_margin_bottom;
                                                    child_margin_bottom
                                                }
                                            }
                                        } else {
                                            0.0
                                        }
                                    };

                                    let x_factor = if direction == FlexDirection::Horizontal {
                                        1.0
                                    } else {
                                        -1.0
                                    };
                                    x += child_margin_start * x_factor;
                                    child_data.dispatch_layout(
                                        if direction == FlexDirection::Horizontal {
                                            x.logical_value()
                                        } else {
                                            (x - child_width).logical_value()
                                        },
                                        if wrap != FlexWrap::WrapReverse {
                                            child_y
                                        } else {
                                            child_y - child_height
                                        },
                                        child_width,
                                        child_height,
                                    );
                                    x += (child_width
                                        + child_margin_end
                                        + main_axis_gap
                                        + space_between_items)
                                        * x_factor;
                                }
                                y += (if let Some(line_stretch) = line_stretch {
                                    line_stretch
                                } else {
                                    line.height()
                                } + space_between_lines
                                    + cross_axis_gap)
                                    * if wrap == FlexWrap::WrapReverse {
                                        -1.0
                                    } else {
                                        1.0
                                    };
                            }
                        }
                        FlexDirection::Vertical | FlexDirection::VerticalReverse => {
                            let lines_width = if wrap == FlexWrap::NoWrap {
                                width - padding_start - padding_end
                            } else {
                                lines_width
                            };
                            let remaining_space_between_lines =
                                width - lines_width - padding_start - padding_end;
                            let mut line_stretch = None;
                            let mut space_between_lines = 0.0;

                            let x = if wrap != FlexWrap::WrapReverse {
                                match align_content {
                                    // AlignContent::Start => padding_top,
                                    AlignContent::Start => padding_start,
                                    AlignContent::End => width - lines_width - padding_end,
                                    AlignContent::Center => (width - lines_width) / 2.0,
                                    // AlignContent::End => height - lines_height - padding_bottom,
                                    // AlignContent::Center => (height - lines_height) / 2.0,
                                    AlignContent::SpaceBetween => {
                                        if remaining_space_between_lines > 0.0 && line_count > 1 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count - 1) as f32;
                                        }
                                        // padding_top
                                        padding_start
                                    }
                                    AlignContent::SpaceAround => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines =
                                                remaining_space_between_lines / line_count as f32;
                                        }
                                        // padding_top + space_between_lines / 2.0
                                        padding_start + space_between_lines / 2.0
                                    }
                                    AlignContent::SpaceEvenly => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count + 1) as f32;
                                        }
                                        // padding_top + space_between_lines
                                        padding_start + space_between_lines
                                    }
                                    AlignContent::Stretch => {
                                        // line_stretch = Some(
                                        //     (height - padding_top - padding_bottom) / line_count as f32
                                        // );
                                        // padding_top
                                        if wrap != FlexWrap::NoWrap {
                                            line_stretch = Some(
                                                (width - padding_start - padding_end)
                                                    / line_count as f32,
                                            );
                                        }
                                        padding_start
                                    }
                                }
                            } else {
                                // let default_y = height - padding_bottom;
                                let default_x = width - padding_end;
                                match align_content {
                                    // AlignContent::Start => default_y,
                                    // AlignContent::End => lines_height + padding_top,
                                    // AlignContent::Center => (height - lines_height) / 2.0,
                                    AlignContent::Start => default_x,
                                    AlignContent::End => lines_width + padding_start,
                                    AlignContent::Center => default_x - (width - lines_width) / 2.0,
                                    AlignContent::SpaceBetween => {
                                        if remaining_space_between_lines > 0.0 && line_count > 1 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count - 1) as f32;
                                        }
                                        // default_y
                                        default_x
                                    }
                                    AlignContent::SpaceAround => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines =
                                                remaining_space_between_lines / line_count as f32;
                                        }
                                        // default_y - space_between_lines / 2.0
                                        default_x - space_between_lines / 2.0
                                    }
                                    AlignContent::SpaceEvenly => {
                                        if remaining_space_between_lines > 0.0 && line_count > 0 {
                                            space_between_lines = remaining_space_between_lines
                                                / (line_count + 1) as f32;
                                        }
                                        // default_y - space_between_lines
                                        default_x - space_between_lines
                                    }
                                    AlignContent::Stretch => {
                                        // line_stretch = Some(
                                        //     (height - padding_top - padding_bottom) / line_count as f32
                                        // );
                                        // default_y
                                        line_stretch = Some(
                                            (width - padding_start - padding_end)
                                                / line_count as f32,
                                        );
                                        default_x
                                    }
                                }
                            };

                            let mut x = LogicalX::new(item.get_layout_direction().get(), x, width);

                            for line in lines.lines().iter() {
                                let start_index = line.start_index;
                                let count = line.count;
                                // let line_width = line.width();
                                let line_height = line.height();

                                let total_grow = {
                                    let mut total_grow = 0_usize;
                                    for child in start_index..start_index + count {
                                        let children = item.get_children().lock();
                                        let child = children.get(child).unwrap();
                                        {
                                            let child_data = child.data();
                                            if let Some(grow) = child_data.get_flex_grow() {
                                                total_grow += grow.get();
                                            }
                                        }
                                    }
                                    total_grow
                                };

                                // let remaining_space_between_lock = width - line_width - padding_start - padding_end;
                                let remaining_space_between_items =
                                    height - line_height - padding_top - padding_bottom;
                                let mut space_between_items = 0.0_f32;
                                let raw_y = if direction == FlexDirection::Vertical {
                                    match justify_content {
                                        // JustifyContent::Start => padding_start,
                                        // JustifyContent::End => width - padding_end - line_width,
                                        // JustifyContent::Center => (width - line_width) / 2.0,
                                        JustifyContent::Start => padding_top,
                                        JustifyContent::End => {
                                            height - padding_bottom - line_height
                                        }
                                        JustifyContent::Center => (height - line_height) / 2.0,
                                        JustifyContent::SpaceBetween => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 1.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count - 1.0);
                                                }
                                            }
                                            // padding_start
                                            padding_top
                                        }
                                        JustifyContent::SpaceAround => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items / count;
                                                }
                                                space_between_items / 2.0
                                            } else {
                                                // padding_start
                                                padding_top
                                            }
                                        }
                                        JustifyContent::SpaceEvenly => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count + 1.0);
                                                }
                                                space_between_items
                                            } else {
                                                // padding_start
                                                padding_top
                                            }
                                        }
                                    }
                                } else {
                                    // let default_x = width - padding_end;
                                    let default_y = height - padding_bottom;
                                    match justify_content {
                                        // JustifyContent::Start => default_x,
                                        // JustifyContent::End => padding_start + line_width,
                                        // JustifyContent::Center => (width - line_width) / 2.0,
                                        JustifyContent::Start => default_y,
                                        JustifyContent::End => padding_top + line_height,
                                        JustifyContent::Center => default_y - (height - line_height) / 2.0,
                                        JustifyContent::SpaceBetween => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 1.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count - 1.0);
                                                }
                                            }
                                            // default_x
                                            default_y
                                        }
                                        JustifyContent::SpaceAround => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items / count;
                                                }
                                                // default_x - space_between_lock / 2.0
                                                default_y - space_between_items / 2.0
                                            } else {
                                                // default_x
                                                default_y
                                            }
                                        }
                                        JustifyContent::SpaceEvenly => {
                                            if total_grow == 0 {
                                                let count = line.count as f32;
                                                if count > 0.0
                                                    && remaining_space_between_items > 0.0
                                                {
                                                    space_between_items =
                                                        remaining_space_between_items
                                                            / (count + 1.0);
                                                }
                                                // default_x - space_between_lock
                                                default_y - space_between_items
                                            } else {
                                                // default_x
                                                default_y
                                            }
                                        }
                                    }
                                };

                                // let mut x = LogicalX::new(item.get_layout_direction().get(), raw_x, width);
                                let mut y = raw_y;

                                for index in start_index..start_index + count {
                                    let mut children = item.get_children().lock();
                                    let child = children.get_mut(index).unwrap();
                                    let mut child_data = child.data();
                                    let child_param = child_data.get_measure_parameter();

                                    // let mut child_width = child_param.width;
                                    // let mut child_height = if let Some(line_stretch) = line_stretch {
                                    //     line_stretch
                                    // } else {
                                    //     child_param.height
                                    // };
                                    let mut child_width = if let Some(line_stretch) = line_stretch {
                                        line_stretch
                                    } else {
                                        child_param.width
                                    };
                                    let mut child_height = child_param.height;

                                    let child_margin_start = child_data.get_margin_start().get();
                                    let child_margin_end = child_data.get_margin_end().get();
                                    let child_margin_top = child_data.get_margin_top().get();
                                    let child_margin_bottom = child_data.get_margin_bottom().get();

                                    if remaining_space_between_items > 0.0 && total_grow > 0 {
                                        let flex_grow = child
                                            .data()
                                            .get_flex_grow()
                                            .map(|v| v.get())
                                            .unwrap_or(0);
                                        if flex_grow > 0 {
                                            // child_width += remaining_space_between_lock * (flex_grow as f32 / total_grow as f32);
                                            child_height += remaining_space_between_items
                                                * (flex_grow as f32 / total_grow as f32);
                                        }
                                    }
                                    
                                    let line_width = if wrap == FlexWrap::NoWrap {
                                        width - padding_start - padding_end
                                    } else {
                                        line.width()
                                    };

                                    let child_x = if wrap != FlexWrap::WrapReverse {
                                        x + if line_stretch.is_none() {
                                            match align_items {
                                                // AlignItems::Start => child_margin_top,
                                                // AlignItems::End => line_height - child_margin_bottom - child_height,
                                                // AlignItems::Center => (line_height - child_height) / 2.0,
                                                AlignItems::Start => child_margin_start,
                                                AlignItems::End => {
                                                    line_width - child_margin_end - child_width
                                                }
                                                AlignItems::Center => {
                                                    (line_width - child_width) / 2.0
                                                }
                                                AlignItems::Baseline => {
                                                    // let child_baseline = child.get_baseline();
                                                    // match child_baseline {
                                                    //     None => child_margin_top,
                                                    //     Some(baseline) => line.under_baseline - baseline
                                                    // }
                                                    child_margin_start
                                                }
                                                AlignItems::Stretch => {
                                                    // child_height = line_height - child_margin_top - child_margin_bottom;
                                                    // child_margin_top
                                                    child_width = line_width
                                                        - child_margin_start
                                                        - child_margin_end;
                                                    child_margin_start
                                                }
                                            }
                                        } else {
                                            0.0
                                        }
                                    } else {
                                        x - if line_stretch.is_none() {
                                            match align_items {
                                                // AlignItems::Start => child_margin_bottom,
                                                // AlignItems::End => line_height - child_margin_top - child_height,
                                                // AlignItems::Center => (line_height - child_height) / 2.0,
                                                AlignItems::Start => child_margin_end,
                                                AlignItems::End => {
                                                    line_width - child_margin_start - child_width
                                                }
                                                AlignItems::Center => {
                                                    (line_width - child_width) / 2.0
                                                }
                                                AlignItems::Baseline => {
                                                    // let child_baseline = child.get_baseline();
                                                    // match child_baseline {
                                                    //     None => child_margin_bottom,
                                                    //     Some(baseline) => line.over_baseline - baseline
                                                    // }
                                                    child_margin_bottom
                                                }
                                                AlignItems::Stretch => {
                                                    // child_height = line_height - child_margin_top - child_margin_bottom;
                                                    // child_margin_bottom
                                                    child_width = line_width
                                                        - child_margin_start
                                                        - child_margin_end;
                                                    child_margin_end
                                                }
                                            }
                                        } else {
                                            0.0
                                        }
                                    };

                                    let y_factor = if direction == FlexDirection::Vertical {
                                        1.0
                                    } else {
                                        -1.0
                                    };
                                    // x += child_margin_start * x_factor;
                                    y += child_margin_top * y_factor;
                                    child_data.dispatch_layout(
                                        // if direction == FlexDirection::Vertical { x.logical_value() } else { (x - child_width).logical_value() },
                                        // if wrap != FlexWrap::WrapReverse { child_y } else { child_y - child_height },
                                        if wrap != FlexWrap::WrapReverse {
                                            child_x.logical_value()
                                        } else {
                                            (child_x - child_width).logical_value()
                                        },
                                        if direction == FlexDirection::Vertical {
                                            y
                                        } else {
                                            y - child_height
                                        },
                                        child_width,
                                        child_height,
                                    );
                                    // x += (child_width + child_margin_end + main_axis_gap + space_between_lock) * x_factor;
                                    y += (child_height
                                        + child_margin_bottom
                                        + main_axis_gap
                                        + space_between_items)
                                        * y_factor;
                                }
                                // y += (
                                //     if let Some(line_stretch) = line_stretch {
                                //         line_stretch
                                //     } else {
                                //         line.height()
                                //     } + space_between_lines + cross_axis_gap
                                // ) * if wrap == FlexWrap::WrapReverse { -1.0 } else { 1.0 };
                                x += (if let Some(line_stretch) = line_stretch {
                                    line_stretch
                                } else {
                                    line.width()
                                } + space_between_lines
                                    + cross_axis_gap)
                                    * if wrap == FlexWrap::WrapReverse {
                                        -1.0
                                    } else {
                                        1.0
                                    };
                            }
                        }
                    }
                }
            });

        Self {
            item,
            property: property,
        }
    }

    pub fn flex_direction(self, direction: impl Into<Shared<FlexDirection>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.direction.remove_observer(id);
            property.direction = direction.into();
            property.direction.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn wrap(self, wrap: impl Into<Shared<FlexWrap>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.wrap.remove_observer(id);
            property.wrap = wrap.into();
            property.wrap.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn justify_content(self, justify_content: impl Into<Shared<JustifyContent>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.justify_content.remove_observer(id);
            property.justify_content = justify_content.into();
            property.justify_content.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn align_items(self, align_items: impl Into<Shared<AlignItems>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.align_items.remove_observer(id);
            property.align_items = align_items.into();
            property.align_items.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn align_content(self, align_content: impl Into<Shared<AlignContent>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.align_content.remove_observer(id);
            property.align_content = align_content.into();
            property.align_content.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn main_axis_gap(self, main_axis_gap: impl Into<Shared<f32>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.main_axis_gap.remove_observer(id);
            property.main_axis_gap = main_axis_gap.into();
            property.main_axis_gap.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }

    pub fn cross_axis_gap(self, cross_axis_gap: impl Into<Shared<f32>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self
                .item
                .data()
                .get_window_context()
                .event_loop_proxy()
                .clone();
            let mut property = self.property.lock();
            property.cross_axis_gap.remove_observer(id);
            property.cross_axis_gap = cross_axis_gap.into();
            property.cross_axis_gap.add_observer(
                id,
                Box::new(move || {
                    event_loop_proxy.request_layout();
                }),
            );
        }
        self
    }
}
