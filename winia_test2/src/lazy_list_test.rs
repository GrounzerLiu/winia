use letclone::clone;
use winia::app::WindowContext;
use winia::shared::{SharedDerivedWVec, SharedSource};
use winia::ui::{label, rectangle, lazy_list, LazyListState, LazyListPropsTrait};
use winia::ui::{Color, LabelPropsTrait, RectanglePropsTrait, Size, Item};

pub fn lazy_list_test(w: &WindowContext) -> Item {
    let items: SharedDerivedWVec<String> = SharedDerivedWVec::from(
        (0..10_000).map(|i| format!("Item {}", i)).collect::<Vec<_>>()
    );

    let generator = {
        clone!(w);
        move |index: usize, item: &String| {
            let colors = [Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW, Color::CYAN, Color::MAGENTA];
            rectangle(
                w.rectangle_props(colors[index % colors.len()])
                    .size(400.0, 48.0)
                    .foreground(
                        label(w.label_props(item.clone()).color(Color::WHITE)),
                    ),
            )
        }
    };

    lazy_list(
        w.lazy_list_props(items, generator)
            .list_state(SharedSource::new(LazyListState::new()))
            .size(Size::Fill, Size::Fill),
    )
}
