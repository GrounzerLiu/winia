// use std::collections::{HashMap, LinkedList};
// 
// 
// use skia_safe::Color;
// use winit::dpi::LogicalPosition;
// use winit::event::{ElementState, Force, MouseButton, TouchPhase};
// 
// pub use item::*;
// pub use item_event::*;
// pub use logical_x::*;
// 
// use crate::property::{Gettable, ObservableProperty, Size};
// 
// mod item;
// // mod rectangle;
// mod logical_x;
// mod item_event;
// // mod text_block;
// // mod image;
// // mod ripple;
// pub mod additional_property;
// mod display_parameter;
// pub use display_parameter::*;
// mod mouse_event;
// pub use mouse_event::*;
// mod pointer_event;
// pub use pointer_event::*;
// mod children;
// pub use children::*;
// mod multicast_event;
// pub use multicast_event::*;
// mod edgs;
// mod drawing_item;
// mod app;
// pub use app::*;
// 
// pub use edgs::*;
// 
// 
// 
// 
// 
// 
// // pub use rectangle::*;
// // pub use text_block::*;
// // pub use image::*;
// // pub use ripple::*;
// 
// pub fn measure_child(child: &Item, parent_layout_params: &DisplayParameter, width_measure_mode: MeasureMode, height_measure_mode: MeasureMode) -> (MeasureMode, MeasureMode) {
//     let layout_params = child.get_display_parameter();
//     let max_width = match width_measure_mode {
//         MeasureMode::Specified(width) => width,
//         MeasureMode::Unspecified(width) => width,
//     } - layout_params.margin_start - layout_params.margin_end - parent_layout_params.padding_start - parent_layout_params.margin_end;
//     let max_height = match height_measure_mode {
//         MeasureMode::Specified(height) => height,
//         MeasureMode::Unspecified(height) => height,
//     } - layout_params.margin_top - layout_params.margin_bottom - parent_layout_params.padding_top - parent_layout_params.margin_bottom;
// 
//     let child_width = child.get_width().get();
//     let child_height = child.get_height().get();
// 
//     let child_width_measure_mode = match child_width {
//         Size::Default => MeasureMode::Unspecified(max_width),
//         Size::Fill => MeasureMode::Specified(max_width),
//         Size::Fixed(width) => MeasureMode::Specified(width),
//         Size::Relative(scale) => MeasureMode::Specified(max_width * scale),
//     };
// 
//     let child_height_measure_mode = match child_height {
//         Size::Default => MeasureMode::Unspecified(max_height),
//         Size::Fill => MeasureMode::Specified(max_height),
//         Size::Fixed(height) => MeasureMode::Specified(height),
//         Size::Relative(percent) => MeasureMode::Specified(max_height * percent),
//     };
// 
//     (child_width_measure_mode, child_height_measure_mode)
// }
// 
// #[derive(Clone, Copy, Debug, PartialEq)]
// pub enum Gravity {
//     Start,
//     Center,
//     End,
// }
// 
// #[derive(Clone, Copy, Debug)]
// pub enum MeasureMode {
//     /// Indicates that the parent has determined an exact size for the child.
//     Specified(f32),
//     /// Indicates that the child can determine its own size. The value of this enum is the maximum size the child can use.
//     Unspecified(f32),
// }
// 
// 
// #[derive(Clone, Copy, Debug, PartialEq)]
// pub enum LayoutDirection {
//     LeftToRight,
//     RightToLeft,
// }
// 
// 
// #[derive(Clone, Debug)]
// pub enum ImeAction {
//     Enabled,
//     Enter,
//     Delete,
//     Preedit(String, Option<(usize, usize)>),
//     Commit(String),
//     Disabled,
// }
// 
// pub enum AdditionalProperty{
//     I8(i8),
//     I16(i16),
//     I32(i32),
//     I64(i64),
//     I128(i128),
//     Isize(isize),
//     U8(u8),
//     U16(u16),
//     U32(u32),
//     U64(u64),
//     U128(u128),
//     Usize(usize),
//     F32(f32),
//     F64(f64),
//     Bool(bool),
//     String(String),
//     Color(Color),
//     Item(Item),
//     SharedI8(ObservableProperty<i8>),
//     SharedI16(ObservableProperty<i16>),
//     SharedI32(ObservableProperty<i32>),
//     SharedI64(ObservableProperty<i64>),
//     SharedI128(ObservableProperty<i128>),
//     SharedIsize(ObservableProperty<isize>),
//     SharedU8(ObservableProperty<u8>),
//     SharedU16(ObservableProperty<u16>),
//     SharedU32(ObservableProperty<u32>),
//     SharedU64(ObservableProperty<u64>),
//     SharedU128(ObservableProperty<u128>),
//     SharedUsize(ObservableProperty<usize>),
//     SharedF32(ObservableProperty<f32>),
//     SharedF64(ObservableProperty<f64>),
//     SharedBool(ObservableProperty<bool>),
//     SharedString(ObservableProperty<String>),
//     SharedColor(ObservableProperty<Color>),
//     SharedItem(ObservableProperty<Item>),
// }
// 
// #[macro_export]
// macro_rules! impl_item_property {
//     ($struct_name:ident, $property_name:ident,$get:ident, $t:ty) => {
//         impl $struct_name{
//             pub fn $property_name(mut self, $property_name: impl Into<$t>) -> Self{
//                 self.$property_name=$property_name.into();
//                 let mut app = self.get_app();
//                 self.width.add_observer(
//                     move ||{
//                         app.request_layout();
//                     },
//                     self.get_id()
//                 );
//                 self
//             }
// 
//             pub fn $get(&self) -> $t{
//                 self.$property_name.clone()
//             }
//         }
//     };
// }