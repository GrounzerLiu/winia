use winia::app::WindowContext;
use winia::ui::{shape, Item, ShapePropsTrait, Size};

pub fn shape_test(w: &WindowContext) -> Item {
    shape(w.shape_props().size(Size::Fill, Size::Fill))
}