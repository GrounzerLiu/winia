use crate::Children;
use clonelet::clone;
use winia::shared::{Gettable, Settable, Shared, SharedText};
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::divider::DividerExt;
use winia::ui::component::{CheckboxExt, TextExt};
use winia::ui::item::Size;
use winia::ui::layout::ColumnExt;
use winia::{children, observables};

pub fn checkbox_test(w: &WindowContext) -> Item {
    let checked_1 = Shared::from_static(false);
    let checked_2 = Shared::from_static(true);
    // w.column(
    //     w.text("Checkbox 1").item()
    //         + w.checkbox(&checked_1)
    //             .on_checked_changed({
    //                 clone!(checked_1);
    //                 move |it| {
    //                     checked_1.set(it);
    //                 }
    //             })
    //             .item()
    //         + w.text("Checkbox 2").item()
    //         + w.checkbox(&checked_2)
    //             .on_checked_changed({
    //                 clone!(checked_2);
    //                 move |it| {
    //                     checked_2.set(it);
    //                 }
    //             })
    //             .item()
    //         + w.divider()
    //             .item()
    //             .margin_top(8)
    //             .margin_bottom(8)
    //             .width(Size::Fill)
    //         + w.text(SharedText::from_dynamic(
    //             [checked_1.to_observable()].into(),
    //             {
    //                 let checked_1 = checked_1.clone();
    //                 move || {
    //                     if checked_1.get() {
    //                         "Checkbox 1 is checked".into()
    //                     } else {
    //                         "Checkbox 1 is unchecked".into()
    //                     }
    //                 }
    //             },
    //         ))
    //         .item()
    //         + w.text(SharedText::from_dynamic(
    //             [checked_2.to_observable()].into(),
    //             {
    //                 let checked_2 = checked_2.clone();
    //                 move || {
    //                     if checked_2.get() {
    //                         "Checkbox 2 is checked".into()
    //                     } else {
    //                         "Checkbox 2 is unchecked".into()
    //                     }
    //                 }
    //             },
    //         ))
    //         .item(),
    // )
    // .item()

    w.column(children!(
        w.text("Checkbox 1").item(),
        w.checkbox(&checked_1)
            .on_checked_changed({
                clone!(checked_1);
                move |it| {
                    checked_1.set(it);
                }
            })
            .item(),
        w.text("Checkbox 2").item(),
        w.checkbox(&checked_2)
            .on_checked_changed({
                clone!(checked_2);
                move |it| {
                    checked_2.set(it);
                }
            })
            .item(),
        w.divider()
            .item()
            .margin_top(8)
            .margin_bottom(8)
            .width(Size::Fill),
        w.column(children!(
            w.text(SharedText::from_dynamic(observables!(checked_1), {
                let checked_1 = checked_1.clone();
                move || {
                    if checked_1.get() {
                        "Checkbox 1 is checked".into()
                    } else {
                        "Checkbox 1 is unchecked".into()
                    }
                }
            },))
                .item(),
            w.text(SharedText::from_dynamic(observables!(checked_2), {
                let checked_2 = checked_2.clone();
                move || {
                    if checked_2.get() {
                        "Checkbox 2 is checked".into()
                    } else {
                        "Checkbox 2 is unchecked".into()
                    }
                }
            },))
                .item()
        ))
        .item()
    ))
    .item()
}
