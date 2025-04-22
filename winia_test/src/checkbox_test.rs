use winia::shared::{Gettable, Shared, SharedText};
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::divider::DividerExt;
use winia::ui::component::{CheckboxExt, TextExt};
use winia::ui::item::Size;
use winia::ui::layout::ColumnExt;

pub fn checkbox_test(w: &WindowContext) -> Item {
    let checked_1 = Shared::from_static(false);
    let checked_2 = Shared::from_static(false);
    w.column(
        w.text("Checkbox 1").item()
            + w.checkbox(&checked_1).item()
            + w.text("Checkbox 2").item()
            + w.checkbox(&checked_2).item()
            + w.divider()
                .item()
                .margin_top(8)
                .margin_bottom(8)
                .width(Size::Fill)
            + w.text(SharedText::from_dynamic(
                [checked_1.to_observable()].into(),
                {
                    let checked_1 = checked_1.clone();
                    move || {
                        if checked_1.get() {
                            "Checkbox 1 is checked".into()
                        } else {
                            "Checkbox 1 is unchecked".into()
                        }
                    }
                },
            ))
            .item()
            + w.text(SharedText::from_dynamic(
                [checked_2.to_observable()].into(),
                {
                    let checked_2 = checked_2.clone();
                    move || {
                        if checked_2.get() {
                            "Checkbox 2 is checked".into()
                        } else {
                            "Checkbox 2 is unchecked".into()
                        }
                    }
                },
            ))
            .item(),
    )
    .item()
}
