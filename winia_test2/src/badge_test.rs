use letclone::clone;
use winia::app::WindowContext;
use winia::icon::Rounded;
use winia::shared::SharedUsize;
use winia::shared_derived;
use winia::text::StyledText;
use winia::theme::{color, material_theme};
use winia::ui::button::ButtonPropsTrait;
use winia::ui::{badge, badged_box, button, flex, image, label, stack, Alignment, BadgePropsTrait, Color, ColumnPropsTrait, ImagePropsTrait, Item, RowPropsTrait, StackPropsTrait};

pub fn badge_test(w: &WindowContext) -> Item {
    let icon_color = shared_derived!(w.theme() => {
        theme.lock().get_color(color::ON_SURFACE).unwrap().clone()
    });

    flex(
        w.column_props(),
        vec![
            button(
                w.button_props()
                 .on_click({
                     clone!(w);
                     move |_| {
                         let is_dark = w.theme().read().is_dark();
                         w.theme().set(if is_dark { material_theme(Color::RED, false) } else { material_theme(Color::RED, true) });
                     }
                 }),
                |p, _| label(p.text("Change Theme"))
            ),
            badge(
                w.badge_props(),
                |_| None
            ),
            badge(
                w.badge_props(),
                |p| Some(label(p.text("1")))
            ),
            badge(
                w.badge_props(),
                |p| Some(label(p.text("999+")))
            ),
            stack(
                w.stack_props()
                 .alignment(Alignment::center())
                 .size(100, 100),
                badged_box(
                    badge(
                        w.badge_props(),
                        |_| None
                    ),
                    image(
                        w.image_props("/home/grounzer/Downloads/mail_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg")
                         .size(24, 24)
                         .color(&icon_color)
                    )
                )
            ),
            stack(
                w.stack_props()
                 .alignment(Alignment::center())
                 .size(100, 100),
                badged_box(
                    badge(
                        w.badge_props(),
                        |p| Some(label(p.text("1")))
                    ),
                    image(
                        w.image_props("/home/grounzer/Downloads/mail_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg")
                         .size(24, 24)
                         .color(&icon_color)
                    )
                )
            ),
            stack(
                w.stack_props()
                 .alignment(Alignment::center())
                 .size(100, 100),
                badged_box(
                    badge(
                        w.badge_props(),
                        |p| Some(label(p.text("999+")))
                    ),
                    image(
                        w.image_props("/home/grounzer/Downloads/mail_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg")
                         .size(24, 24)
                         .color(&icon_color)
                    )
                )
            ),
            {
                let count = SharedUsize::from(0);
                flex(
                    w.row_props(),
                    vec![
                        button(
                            w.button_props()
                             .on_click({
                                 clone!(count);
                                 move |_| {
                                     count.write(|c| *c += 1);
                                 }
                             }),
                            |p, _| label(p.text("Increment"))
                        ),
                        button(
                            w.button_props()
                             .on_click({
                                 clone!(count);
                                 move |_| {
                                     count.write(|c| *c += 10);
                                 }
                             }),
                            |p, _| label(p.text("Increment 10"))
                        ),
                        stack(
                            w.stack_props()
                             .alignment(Alignment::center())
                             .size(100, 100),
                            badged_box(
                                badge(
                                    w.badge_props(),
                                    |p| Some(label(p.text(shared_derived!(
                                        count => {
                                            if count.get() > 999 {
                                                StyledText::from("999+")
                                            } else {
                                                StyledText::from(format!("{}", count.get()))
                                            }
                                        }
                                    ))))
                                ),
                                image(
                                    w.image_props(Rounded::CHAT)
                                     .size(24, 24)
                                     .color(&icon_color)
                                )
                            )
                        )
                    ]
                )
            }
        ],
    )
}