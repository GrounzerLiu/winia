use winia::app::WindowContext;
use winia::ui::{flex, label, rectangle, scroll_area, Color, ColumnPropsTrait, DefaultScrollAreaProps, Item, LabelPropsTrait, Radius, RectanglePropsTrait, ScrollState};

pub fn scroll_area_test(w: &WindowContext) -> Item {
    let h_scroll_state = ScrollState::new_shared();
    let v_scroll_state = ScrollState::new_shared();
    let colors = [Color::RED,
        Color::GREEN,
        Color::BLUE,
        Color::YELLOW,
        Color::CYAN,
        Color::MAGENTA];
    scroll_area(
        w.vertical_scroll_props(),
        flex(
            w.column_props(),
            (0..50)
                .map(|i| {
                    rectangle(
                        w.rectangle_props(colors[i % colors.len()])
                         .size(100.0, 100.0)
                         .radius(Radius::new().all(12.0))
                         .foreground(label(w.label_props(format!("Item {}", i)).color(Color::WHITE))),
                    )
                })
                .collect::<Vec<Item>>(),
        ),
    )
}