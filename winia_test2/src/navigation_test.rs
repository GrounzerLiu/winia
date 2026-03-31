use std::any::Any;

use winia::app::WindowContext;
use winia::shared::{SharedSource, SharedWVec};
use winia::ui::button::ButtonPropsTrait;
use winia::ui::item::Padding;
use winia::ui::{
    button, flex, label, nav_display, rectangle, stack, Alignment, Color, ColumnPropsTrait, Item,
    LabelPropsTrait, NavDisplayPropsTrait, NavKey, RectanglePropsTrait, Size, StackPropsTrait,
};

#[derive(Clone)]
enum DemoRoute {
    Home,
    Details { id: usize },
    About,
}

impl NavKey for DemoRoute {
    fn nav_key(&self) -> &'static str {
        match self {
            DemoRoute::Home => "home",
            DemoRoute::Details { .. } => "details",
            DemoRoute::About => "about",
        }
    }

    fn instance_key(&self) -> String {
        match self {
            DemoRoute::Home => "home".to_string(),
            DemoRoute::Details { id } => format!("details:{id}"),
            DemoRoute::About => "about".to_string(),
        }
    }
}

pub fn navigation_test(w: &WindowContext) -> Item {
    let w = w.clone();
    let back_stack: SharedWVec<Box<dyn NavKey>> = vec![Box::new(DemoRoute::Home) as Box<dyn NavKey>].into();
    let next_detail_id = SharedSource::new(1_usize);

    nav_display(
        w.nav_display_props(back_stack.clone(), {
            let w = w.clone();
            let back_stack = back_stack.clone();
            let next_detail_id = next_detail_id.clone();
            move |key| build_route_ui(&w, key, &back_stack, &next_detail_id)
        })
        .size(Size::Fill, Size::Fill)
        .alignment(Alignment::top_start()),
    )
}

fn build_route_ui(
    w: &WindowContext,
    key: &Box<dyn NavKey>,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    next_detail_id: &SharedSource<usize>,
) -> Item {
    let route = (key.as_ref() as &dyn Any)
        .downcast_ref::<DemoRoute>()
        .expect("navigation_test only accepts DemoRoute");

    match route {
        DemoRoute::Home => page_shell(
            w,
            "Home",
            "Root page. Push a details page or the about page.",
            Color::from_rgb(0x15, 0x2A, 0x38),
            vec![
                page_button(w, "Push Details", {
                    let back_stack = back_stack.clone();
                    let next_detail_id = next_detail_id.clone();
                    move || {
                        let id = next_detail_id.get();
                        next_detail_id.set(id + 1);
                        back_stack.push(Box::new(DemoRoute::Details { id }));
                    }
                }),
                page_button(w, "Open About", {
                    let back_stack = back_stack.clone();
                    move || {
                        back_stack.push(Box::new(DemoRoute::About));
                    }
                }),
            ],
        ),
        DemoRoute::Details { id } => page_shell(
            w,
            &format!("Details #{id}"),
            "Each details page has its own instance key, so pushing again creates another entry.",
            Color::from_rgb(0x33, 0x24, 0x4B),
            vec![
                page_button(w, "Push Next Details", {
                    let back_stack = back_stack.clone();
                    let next_detail_id = next_detail_id.clone();
                    move || {
                        let id = next_detail_id.get();
                        next_detail_id.set(id + 1);
                        back_stack.push(Box::new(DemoRoute::Details { id }));
                    }
                }),
                page_button(w, "Go About", {
                    let back_stack = back_stack.clone();
                    move || {
                        back_stack.push(Box::new(DemoRoute::About));
                    }
                }),
                page_button(w, "Pop", {
                    let back_stack = back_stack.clone();
                    move || {
                        if back_stack.len() > 1 {
                            back_stack.pop();
                        }
                    }
                }),
            ],
        ),
        DemoRoute::About => page_shell(
            w,
            "About",
            "This page is just another back stack entry. Pop returns to the preserved previous page.",
            Color::from_rgb(0x2B, 0x3D, 0x1F),
            vec![
                page_button(w, "Push Details", {
                    let back_stack = back_stack.clone();
                    let next_detail_id = next_detail_id.clone();
                    move || {
                        let id = next_detail_id.get();
                        next_detail_id.set(id + 1);
                        back_stack.push(Box::new(DemoRoute::Details { id }));
                    }
                }),
                page_button(w, "Pop", {
                    let back_stack = back_stack.clone();
                    move || {
                        if back_stack.len() > 1 {
                            back_stack.pop();
                        }
                    }
                }),
            ],
        ),
    }
}

fn page_shell(
    w: &WindowContext,
    title: &str,
    description: &str,
    color: Color,
    actions: Vec<Item>,
) -> Item {
    stack(
        w.stack_props()
            .size(Size::Fill, Size::Fill)
            .background(
                rectangle(
                    w.rectangle_props(color)
                        .size(Size::Fill, Size::Fill),
                ),
            )
            .padding(Padding::all(24.0)),
        flex(
            w.column_props()
                .size(Size::Fill, Size::Fill),
            {
                let mut children = vec![
                    label(
                        w.label_props(title)
                            .font_size(36.0)
                            .color(Color::WHITE),
                    ),
                    label(
                        w.label_props(description)
                            .color(Color::WHITE)
                            .padding(Padding::default().top(12.0)),
                    ),
                ];
                children.extend(actions);
                children
            },
        ),
    )
}

fn page_button(
    w: &WindowContext,
    text: &str,
    mut on_click: impl FnMut() + 'static,
) -> Item {
    button(
        w.button_props()
            .padding(Padding::default().top(16.0))
            .on_click(move |_| on_click()),
        move |props, _| label(props.text(text)),
    )
}
