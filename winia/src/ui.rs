pub mod item;
mod orientation;
mod layout;
mod size;
mod alignment;
pub mod widget;
mod color;
mod inner_position;
mod lazy_list_state;

pub use item::Item;
pub use orientation::*;
pub use size::*;
pub use alignment::*;
pub use layout::*;
pub use widget::*;
pub use color::*;
pub use inner_position::*;
pub use lazy_list_state::*;

// fn bind_property() {
//
// }
#[macro_export]
macro_rules! bind_property {
    ($property:expr_2021, $item:ident) => {
        {
            let id = $item.id();
            let need_redraw = $item.data().props().need_redraw.clone();
            let e = $item.data().window_context().event_loop_proxy.clone();
            $property.subscribe(id, move ||{
                need_redraw.lock().request();
                e.request_layout();
            });
        }
    };
}

#[macro_export]
macro_rules! bind_properties {
    ($item:ident, $( $property:expr_2021 ),* ) => {
        $(
            $crate::bind_property!($property, $item);
        )*
    };
}