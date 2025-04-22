use winia::shared::{Gettable, Settable, Shared, SharedText};
use winia::text::StyledText;
use winia::ui::app::WindowContext;
use winia::ui::component::{RadioExt, RadioGroupExt, TextExt};
use winia::ui::component::divider::DividerExt;
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::{AlignItems, ColumnExt, RowExt, StackExt};

pub fn radio_test(w: &WindowContext) -> Item {
    let selected_value = SharedText::from("A");
    let selected_value_number = Shared::from_static(0);
    w.column(
        w.row(
            w.radio(StyledText::from("A"), &selected_value, None).item()
            + w.text("A").item().margin_start(16)
        ).align_items(AlignItems::Center).item().padding_start(16)
        + w.row(
            w.radio(StyledText::from("B"), &selected_value, None).item()
            + w.text("B").item().margin_start(16)
        ).align_items(AlignItems::Center).item().padding_start(16)
        + w.row(
            w.radio(StyledText::from("C"), &selected_value, None).item()
            + w.text("C").item().margin_start(16)
        ).align_items(AlignItems::Center).item().padding_start(16)
        + w.text(&selected_value).item().margin_start(16)
        + w.divider().item().width(Size::Fill).margin_top(8).margin_bottom(8)
        + w.column(
            w.radio_group(
                &selected_value_number,
                &[
                    (0, "Apple".into()),
                    (1, "Banana".into()),
                    (2, "Cherry".into())
                ],
                |s,v|{
                    s.set(v);
                }
            )
        ).item()
        + w.text(
            Shared::from_dynamic(
                [selected_value_number.to_observable()].into(),
                {
                    let selected_value_number = selected_value_number.clone();
                    move || {
                        let selected_value_number = selected_value_number.get();
                        match selected_value_number {
                            0 => "Apple".into(),
                            1 => "Banana".into(),
                            2 => "Cherry".into(),
                            _ => "Unknown".into(),
                        }
                    }
                }
            )
        ).item()
    ).item().padding(16)
}