use std::ops::Not;
use letclone::clone;
use strum_macros::EnumString;
use proc_macro::ItemProps;
use crate::{bind_properties, define_props};
use crate::app::WindowContext;
use crate::event::{ItemEvent, MeasureMode};
use crate::shared::{SharedDerived, SharedDerivedUsize};
use crate::ui::item::{Children, ItemData, ItemKind, ItemProps, PhysicalX, SetCustomProp};
use crate::ui::{Item, Orientation};

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

pub trait SetFlexGrow {
    fn flex_grow(self, align: impl Into<SharedDerivedUsize>) -> Self;
}

pub trait GetFlexGrow {
    fn get_flex_grow(&self) -> Option<SharedDerivedUsize>;
}

impl<T: SetCustomProp> SetFlexGrow for T {
    fn flex_grow(mut self, align: impl Into<SharedDerivedUsize>) -> Self {
        self.set_custom_prop("flex_grow", align);
        self
    }
}

impl GetFlexGrow for ItemProps {
    fn get_flex_grow(&self) -> Option<SharedDerivedUsize> {
        self.get_custom::<SharedDerivedUsize>("flex_grow").cloned()
    }
}

/*define_props!(
    FlexPropsTrait;
    flex_props;
    FlexProps {
        direction: SharedDerived<FlexDirection>,
        wrap: SharedDerived<FlexWrap>,
        justify_content: SharedDerived<JustifyContent>,
        align_items: SharedDerived<AlignItems>,
        align_content: SharedDerived<AlignContent>,
        main_axis_gap: SharedDerived<f32>,
        cross_axis_gap: SharedDerived<f32>,
    }
);
*/

#[derive(ItemProps)]
pub struct FlexProps {
    pub item_props: ItemProps,
    pub direction: SharedDerived<FlexDirection>,
    pub wrap: SharedDerived<FlexWrap>,
    pub justify_content: SharedDerived<JustifyContent>,
    pub align_items: SharedDerived<AlignItems>,
    pub align_content: SharedDerived<AlignContent>,
    pub main_axis_gap: SharedDerived<f32>,
    pub cross_axis_gap: SharedDerived<f32>,
}

impl FlexProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self {
            item_props,
            direction: FlexDirection::Horizontal.into(),
            wrap: FlexWrap::NoWrap.into(),
            justify_content: JustifyContent::Start.into(),
            align_items: AlignItems::Start.into(),
            align_content: AlignContent::Start.into(),
            main_axis_gap: 0.0.into(),
            cross_axis_gap: 0.0.into(),
        }.name("Flex")
    }
}

pub fn flex(props: FlexProps, children: impl Into<Children>) -> Item {
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        children.into()
    )
}

pub trait ColumnPropsTrait {
    fn column_props(&self) -> FlexProps;
}

impl ColumnPropsTrait for WindowContext {
    fn column_props(&self) -> FlexProps {
        self.flex_props().direction(FlexDirection::Vertical)
    }
}

pub trait RowPropsTrait {
    fn row_props(&self) -> FlexProps;
}
impl RowPropsTrait for WindowContext {
    fn row_props(&self) -> FlexProps {
        self.flex_props().direction(FlexDirection::Horizontal)
    }
}

fn item_event(props: &FlexProps) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            clone!(props.direction,
                   props.wrap,
                   props.align_items,
                   props.main_axis_gap,
                   props.cross_axis_gap);
            move |item, width_mode, height_mode| {
                let direction = direction.get();
                let wrap = wrap.get();
                let align_items = align_items.get();

                let max_width = match width_mode {
                    MeasureMode::Specified(width) => item.clamp_width(width),
                    MeasureMode::Unspecified(_) => item.props().max_width.get(),
                };

                let max_height = match height_mode {
                    MeasureMode::Specified(height) => item.clamp_height(height),
                    MeasureMode::Unspecified(_) => item.props().max_height.get(),
                };

                let orientation = direction.orientation();
                let (children_width, children_height) = calculate_size(
                    item,
                    orientation,
                    align_items,
                    main_axis_gap.get(),
                    cross_axis_gap.get(),
                    if wrap == FlexWrap::NoWrap {
                        None
                    } else if orientation == Orientation::Horizontal {
                        Some(max_width)
                    } else {
                        Some(max_height)
                    },
                    false,
                    Some(width_mode),
                    Some(height_mode),
                );

                let min_width = item.props().min_width.get();
                let min_height = item.props().min_height.get();
                let max_width = max_width.max(min_width);
                let max_height = max_height.max(min_height);
                let padding_horizontal = item.get_padding(Orientation::Horizontal);
                let padding_vertical = item.get_padding(Orientation::Vertical);
                let measure_frame = &mut item.measure_frame;
                match width_mode {
                    MeasureMode::Specified(width) => {
                        measure_frame.width = width.clamp(min_width, max_width);
                        match height_mode {
                            MeasureMode::Specified(height) => {
                                measure_frame.height = height.clamp(min_height, max_height);
                            }
                            MeasureMode::Unspecified(_) => {
                                measure_frame.height = (children_height + padding_vertical)
                                    .clamp(min_height, max_height);
                            }
                        }
                    }
                    MeasureMode::Unspecified(_) => {
                        measure_frame.width =
                            (children_width + padding_horizontal).clamp(min_width, max_width);
                        match height_mode {
                            MeasureMode::Specified(height) => {
                                measure_frame.height = height.clamp(min_height, max_height);
                            }
                            MeasureMode::Unspecified(_) => {
                                measure_frame.height = (children_height + padding_vertical)
                                    .clamp(min_height, max_height);
                            }
                        }
                    }
                }
            }
        })
        .set_layout({
            clone!(props.direction,
                   props.wrap,
                   props.justify_content,
                   props.align_items,
                   props.align_content,
                   props.main_axis_gap,
                   props.cross_axis_gap);
            move |item, width, height| {
                if item.children().lock().is_empty() {
                    return;
                }

                let padding_start = item.props().padding.start.get();
                let padding_end = item.props().padding.end.get();
                let padding_top = item.props().padding.top.get();
                let padding_bottom = item.props().padding.bottom.get();

                let direction = direction.get();
                let wrap = wrap.get();
                let justify_content = justify_content.get();
                let align_items = align_items.get();
                let align_content = align_content.get();
                let main_axis_gap = main_axis_gap.get();
                let cross_axis_gap = cross_axis_gap.get();

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
                    true,
                    None,
                    None,
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
                                AlignContent::Center => {
                                    default_y - (height - lines_height) / 2.0
                                }
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
                                    let children = item.children().lock();
                                    let child = children.get(index).unwrap();
                                    {
                                        let child_data = child.data();
                                        if let Some(grow) = child_data.props().get_flex_grow() {
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
                                    JustifyContent::Center => {
                                        default_x - (width - line_width) / 2.0
                                    }
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

                            let mut x = raw_x;

                            for index in start_index..start_index + count {
                                let mut children = item.children().lock();
                                let child = children.get_mut(index).unwrap();
                                let mut child_data = child.data();
                                let child_param = &child_data.measure_frame;

                                let mut child_width = child_param.width;
                                let mut child_height = if let Some(line_stretch) = line_stretch
                                {
                                    line_stretch
                                } else {
                                    child_param.height
                                };

                                if remaining_space_between_items > 0.0 && total_grow > 0 {
                                    let flex_grow = child_data
                                        .props()
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
                                            AlignItems::Start => 0.0,
                                            AlignItems::End => {
                                                line_height - child_height
                                            }
                                            AlignItems::Center => {
                                                (line_height - child_height) / 2.0
                                            }
                                            AlignItems::Baseline => {
                                                let child_baseline = child_data.props().baseline.get();
                                                match child_baseline {
                                                    None => 0.0,
                                                    Some(baseline) => {
                                                        line.under_baseline - baseline
                                                    }
                                                }
                                            }
                                            AlignItems::Stretch => {
                                                child_height = line_height;
                                                0.0
                                            }
                                        }
                                    } else {
                                        0.0
                                    }
                                } else {
                                    y - if line_stretch.is_none() {
                                        match align_items {
                                            AlignItems::Start => 0.0,
                                            AlignItems::End => {
                                                line_height - child_height
                                            }
                                            AlignItems::Center => {
                                                (line_height - child_height) / 2.0
                                            }
                                            AlignItems::Baseline => {
                                                let child_baseline = child_data.props().baseline.get();
                                                match child_baseline {
                                                    None => 0.0,
                                                    Some(baseline) => {
                                                        line.over_baseline
                                                            - (child_height - baseline)
                                                    }
                                                }
                                            }
                                            AlignItems::Stretch => {
                                                child_height = line_height;
                                                0.0
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
                                {
                                    let x = x;
                                    child_data.dispatch_layout(
                                        if direction == FlexDirection::Horizontal {
                                            x.physical_x(item.props().layout_direction.get(), width, child_width)
                                        } else {
                                            (x - child_width).physical_x(item.props().layout_direction.get(), width, child_width)
                                        },
                                        if wrap != FlexWrap::WrapReverse {
                                            child_y
                                        } else {
                                            child_y - child_height
                                        },
                                        child_width,
                                        child_height,
                                    );
                                }
                                if child_data.props().enable.get() {
                                    x += (
                                        child_width
                                            + main_axis_gap
                                            + space_between_items
                                    )
                                        * x_factor;
                                }
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
                                AlignContent::Start => padding_start,
                                AlignContent::End => width - lines_width - padding_end,
                                AlignContent::Center => (width - lines_width) / 2.0,
                                AlignContent::SpaceBetween => {
                                    if remaining_space_between_lines > 0.0 && line_count > 1 {
                                        space_between_lines = remaining_space_between_lines
                                            / (line_count - 1) as f32;
                                    }
                                    padding_start
                                }
                                AlignContent::SpaceAround => {
                                    if remaining_space_between_lines > 0.0 && line_count > 0 {
                                        space_between_lines =
                                            remaining_space_between_lines / line_count as f32;
                                    }
                                    padding_start + space_between_lines / 2.0
                                }
                                AlignContent::SpaceEvenly => {
                                    if remaining_space_between_lines > 0.0 && line_count > 0 {
                                        space_between_lines = remaining_space_between_lines
                                            / (line_count + 1) as f32;
                                    }
                                    padding_start + space_between_lines
                                }
                                AlignContent::Stretch => {
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
                            let default_x = width - padding_end;
                            match align_content {
                                AlignContent::Start => default_x,
                                AlignContent::End => lines_width + padding_start,
                                AlignContent::Center => default_x - (width - lines_width) / 2.0,
                                AlignContent::SpaceBetween => {
                                    if remaining_space_between_lines > 0.0 && line_count > 1 {
                                        space_between_lines = remaining_space_between_lines
                                            / (line_count - 1) as f32;
                                    }
                                    default_x
                                }
                                AlignContent::SpaceAround => {
                                    if remaining_space_between_lines > 0.0 && line_count > 0 {
                                        space_between_lines =
                                            remaining_space_between_lines / line_count as f32;
                                    }
                                    default_x - space_between_lines / 2.0
                                }
                                AlignContent::SpaceEvenly => {
                                    if remaining_space_between_lines > 0.0 && line_count > 0 {
                                        space_between_lines = remaining_space_between_lines
                                            / (line_count + 1) as f32;
                                    }
                                    default_x - space_between_lines
                                }
                                AlignContent::Stretch => {
                                    line_stretch = Some(
                                        (width - padding_start - padding_end)
                                            / line_count as f32,
                                    );
                                    default_x
                                }
                            }
                        };

                        let mut x = x;

                        for line in lines.lines().iter() {
                            let start_index = line.start_index;
                            let count = line.count;
                            // let line_width = line.width();
                            let line_height = line.height();

                            let total_grow = {
                                let mut total_grow = 0_usize;
                                for child in start_index..start_index + count {
                                    let children = item.children().lock();
                                    let child = children.get(child).unwrap();
                                    {
                                        let child_data = child.data();
                                        if let Some(grow) = child_data.props().get_flex_grow() {
                                            total_grow += grow.get();
                                        }
                                    }
                                }
                                total_grow
                            };

                            let remaining_space_between_items =
                                height - line_height - padding_top - padding_bottom;
                            let mut space_between_items = 0.0_f32;
                            let raw_y = if direction == FlexDirection::Vertical {
                                match justify_content {
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
                                            padding_top
                                        }
                                    }
                                }
                            } else {
                                let default_y = height - padding_bottom;
                                match justify_content {
                                    JustifyContent::Start => default_y,
                                    JustifyContent::End => padding_top + line_height,
                                    JustifyContent::Center => {
                                        default_y - (height - line_height) / 2.0
                                    }
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
                                            default_y - space_between_items / 2.0
                                        } else {
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
                                            default_y - space_between_items
                                        } else {
                                            default_y
                                        }
                                    }
                                }
                            };

                            let mut y = raw_y;

                            for index in start_index..start_index + count {
                                let mut children = item.children().lock();
                                let child = children.get_mut(index).unwrap();
                                let mut child_data = child.data();
                                let child_param = &child_data.measure_frame;

                                let mut child_width = if let Some(line_stretch) = line_stretch {
                                    line_stretch
                                } else {
                                    child_param.width
                                };
                                let mut child_height = child_param.height;

                                if remaining_space_between_items > 0.0 && total_grow > 0 {
                                    let flex_grow = child
                                        .data()
                                        .props()
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
                                            AlignItems::Start => 0.0,
                                            AlignItems::End => {
                                                line_width - child_width
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
                                                0.0
                                            }
                                            AlignItems::Stretch => {
                                                // child_height = line_height - child_margin_top - child_margin_bottom;
                                                // child_margin_top
                                                child_width = line_width;
                                                0.0
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
                                            AlignItems::Start => 0.0,
                                            AlignItems::End => {
                                                line_width - child_width
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
                                                0.0
                                            }
                                            AlignItems::Stretch => {
                                                // child_height = line_height - child_margin_top - child_margin_bottom;
                                                // child_margin_bottom
                                                child_width = line_width;
                                                0.0
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
                                // y += child_margin_top * y_factor;
                                {
                                    let y = y;
                                    child_data.dispatch_layout(
                                        if wrap != FlexWrap::WrapReverse {
                                            child_x.physical_x(item.props().layout_direction.get(), width, child_width)
                                        } else {
                                            (child_x - child_width).physical_x(item.props().layout_direction.get(), width, child_width)
                                        },
                                        if direction == FlexDirection::Vertical {
                                            y
                                        } else {
                                            y - child_height
                                        },
                                        child_width,
                                        child_height,
                                    );
                                }
                                // x += (child_width + child_margin_end + main_axis_gap + space_between_lock) * x_factor;
                                if child_data.props().enable.get() {
                                    y += (
                                        child_height
                                            + main_axis_gap
                                            + space_between_items)
                                        * y_factor;
                                }
                            }
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
        })
}

/// Calculate the size of the flex item.
fn calculate_size(
    item: &ItemData,
    orientation: Orientation,
    align_items: AlignItems,
    main_axis_gap: f32,
    cross_axis_gap: f32,
    max_main_axis_size: Option<f32>,
    measured: bool,
    width_measure_mode: Option<MeasureMode>,
    height_measure_mode: Option<MeasureMode>,
) -> (f32, f32) {
    let l = Lines::new(
        item,
        orientation,
        align_items,
        main_axis_gap,
        cross_axis_gap,
        max_main_axis_size,
        measured,
        width_measure_mode,
        height_measure_mode,
    );
    (l.width(), l.height())
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
        let child_current_frame = &data.measure_frame;
        let child_main_axis_size = child_current_frame.size(self.orientation);
        let child_cross_axis_size = child_current_frame.size(self.orientation.not());

        self.set_main_axis_size(self.main_axis_size() + child_main_axis_size);

        if self.align_items == AlignItems::Baseline {
            let child_baseline = if self.orientation == Orientation::Horizontal {
                data.props().baseline.get()
            } else {
                None
            };

            if let Some(child_baseline) = child_baseline {
                let under_baseline = child_baseline;
                let over_baseline =
                    child_cross_axis_size - child_baseline;
                self.under_baseline = self.under_baseline.max(under_baseline);
                self.over_baseline = self.over_baseline.max(over_baseline);
            }

            let baseline_occupied_size = self.over_baseline + self.under_baseline;
            let max = child_cross_axis_size.max(baseline_occupied_size);

            self.set_cross_axis_size(self.cross_axis_size().max(max));
            self.count += 1;
        } else {
            self.set_cross_axis_size(self.cross_axis_size().max(child_cross_axis_size));
            self.count += 1;
        }
    }

    /// Try to add an item to the line. Not really adding it, just calculating the size of the line.
    pub fn try_add_item(&mut self, item: &Item) -> bool {
        let mut data = item.data();
        let child_current_frame = data.current_frame();
        let child_main_axis_size = child_current_frame.size(self.orientation);
        drop(data);

        if self.is_exceed(child_main_axis_size) {
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
        measured: bool,
        width_measure_mode: Option<MeasureMode>,
        height_measure_mode: Option<MeasureMode>,
    ) -> Self {
        let mut lines = vec![];
        let mut line = Line::new(
            0,
            orientation,
            align_items,
            main_axis_gap,
            max_main_axis_size,
        );

        let padding_horizontal = item.get_padding(Orientation::Horizontal);
        let padding_vertical = item.get_padding(Orientation::Vertical);

        // Iterate over the children of the item and add them to the line.
        for (index, child) in item.children().lock().iter().enumerate() {
            if !measured {
                let width: f32 = width_measure_mode.unwrap().value();
                let height: f32 = height_measure_mode.unwrap().value();
                let w_mode = MeasureMode::from_size(child.data().props().width.get(), width - padding_horizontal);
                let h_mode = MeasureMode::from_size(child.data().props().height.get(), height - padding_vertical);
                child.data().dispatch_measure(w_mode, h_mode);
            }

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
