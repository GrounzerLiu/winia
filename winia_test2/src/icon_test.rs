use std::time::Duration;
use letclone::clone;
use winia::animation::AnimationExt;
use winia::app::WindowContext;
use winia::exclude_target;
use winia::icon::{Outlined, Rounded, Sharp};
use winia::shared::SharedF32;
use winia::ui::{button, flex, icon, image, label, rectangle, Color, ColumnPropsTrait, FlexPropsTrait, FlexWrap, IconPropsTrait, ImagePropsTrait, Item, RectanglePropsTrait, RowPropsTrait, Size};
use winia::ui::button::ButtonPropsTrait;

pub fn icon_test(w: &WindowContext) -> Item {
    let fill = SharedF32::new(1.0);
    let weight = SharedF32::new(400.0);
    let grade = SharedF32::new(0.0);
    flex(
        w.column_props(),
        vec![
            flex(
                w.row_props(),
                vec![
                    button(
                        w.button_props()
                         .on_click({
                             clone!(fill, w);
                             move |_| {
                                 let target_fill = if fill.get() < 0.1 {
                                     1.0
                                 } else {
                                     0.0
                                 };
                                 w.animate(exclude_target!())
                                  .transformation({
                                      clone!(fill);
                                      move || {
                                          fill.set(target_fill);
                                      }
                                  })
                                  .duration(Duration::from_millis(500))
                                  .start();
                             }
                         }),
                        |p, _| {
                            label(p.text("Fill"))
                        }
                    ),
                    button(
                        w.button_props()
                         .on_click({
                             clone!(weight, w);
                             move |_| {
                                 let target_weight = if weight.get() == 100.0 {
                                     700.0
                                 } else {
                                     100.0
                                 };
                                 w.animate(exclude_target!())
                                  .transformation({
                                      clone!(weight);
                                      move || {
                                          weight.set(target_weight);
                                      }
                                  })
                                  .duration(Duration::from_millis(500))
                                  .start();
                             }
                         }),
                        |p, _| {
                            label(p.text("Weight"))
                        }
                    ),
                    button(
                        w.button_props()
                         .on_click({
                             clone!(grade, w);
                             move |_| {
                                 let target_grade = if grade.get() == -50.0 {
                                     200.0
                                 } else {
                                     -50.0
                                 };
                                 w.animate(exclude_target!())
                                  .transformation({
                                      clone!(grade);
                                      move || {
                                          grade.set(target_grade);
                                      }
                                  })
                                  .duration(Duration::from_millis(500))
                                  .start();
                             }
                         }),
                        |p, _| {
                            label(p.text("Grade"))
                        }
                    ),
                    button(
                        w.button_props()
                         .on_click({
                             clone!(fill, weight, grade, w);
                             move |_| {
                                 w.animate(exclude_target!())
                                  .transformation({
                                      clone!(fill, weight, grade);
                                      move || {
                                          fill.set(0.0);
                                          grade.set(0.0);
                                          weight.set(400.0);
                                      }
                                  })
                                  .duration(Duration::from_millis(500))
                                  .start()
                             }
                         }),
                        |p, _| {
                            label(p.text("Reset"))
                        }
                    ),
                ]
            ),
            flex(
                w.flex_props()
                 .wrap(FlexWrap::Wrap)
                 .main_axis_gap(10.0)
                 .cross_axis_gap(10.0),
                vec![
                    icon(
                        w.icon_props(Outlined::CHAT)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::HOME)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::ALARM)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::SPORTS_ESPORTS)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::BOOK)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::CHECK_BOX)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::DISABLED_BY_DEFAULT)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                    icon(
                        w.icon_props(Outlined::DISABLED_BY_DEFAULT)
                         .size(48, 48)
                         .fill(&fill)
                         .weight(&weight)
                         .grade(&grade)
                         .background(
                             rectangle(w.rectangle_props(Color::GREEN).is_filled(false).border_width(2.0)),
                         )
                    ),
                ],
            )
        ],
    )
}