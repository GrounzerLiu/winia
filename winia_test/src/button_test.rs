use std::time::Duration;
use winia::ui::animation::Target;
use winia::exclude_target;
use winia::shared::{Gettable, Settable, Shared, SharedText};
use winia::ui::animation::AnimationExt;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, ButtonType, TextExt};
use winia::ui::Item;
use winia::ui::layout::{ColumnExt, RowExt};

pub fn button_test(w: &WindowContext) -> Item {
    let text = SharedText::from("Button");
    let enabled = Shared::from(true);
    w.column(
        w.text(&text)
            .editable(false)
            .item()
        + w.button(&text)
            .item()
            .on_click({
                let mut click_count = 0;
                move |_|{
                    click_count += 1;
                    text.set(format!("Button clicked {} times", click_count));
                }
            })
        + w.button("Enable")
            .icon("/home/grounzer/RustroverProjects/winia/example/add.svg")
            .item()
            .margin_top(16)
            .on_click({
                let enabled = enabled.clone();
                let w = w.clone();
                move |_| {
                    // enabled.set(!enabled.get());
                    let enabled = enabled.clone();
                    w.animate(exclude_target!()).transformation(
                        move || {
                            enabled.set(!enabled.get());
                        }
                    ).duration(Duration::from_millis(300)).start()
                }
            })
        
        + w.row(
            w.button("Elevated button")
                .button_type(ButtonType::Elevated)
                .item()
                .enabled(&enabled)
            + w.button("Elevated button")
                .button_type(ButtonType::Elevated)
                .item()
                .enabled(false)
        ).item().margin_top(8)
        + w.row(
            w.button("Filled button")
                .button_type(ButtonType::Filled)
                .item()
                .enabled(&enabled)
            + w.button("Filled button")
                .button_type(ButtonType::Filled)
                .item()
                .enabled(false)
        ).item().margin_top(8)
        + w.row(
            w.button("Filled tonal button")
                .button_type(ButtonType::FilledTonal)
                .item()
                .enabled(&enabled)
            + w.button("Filled tonal button")
                .button_type(ButtonType::FilledTonal)
                .item()
                .enabled(false)
        ).item().margin_top(8)
        + w.row(
            w.button("Outlined button")
                .button_type(ButtonType::Outlined)
                .item()
                .enabled(&enabled)
            + w.button("Outlined button")
                .button_type(ButtonType::Outlined)
                .item()
                .enabled(false)
        ).item().margin_top(8)
        + w.row(
            w.button("Text button")
                .button_type(ButtonType::Text)
                .item()
                .enabled(&enabled)
            + w.button("Text button")
                .button_type(ButtonType::Text)
                .item()
                .enabled(false)
        ).item().margin_top(8)
    ).item().padding(16)
}