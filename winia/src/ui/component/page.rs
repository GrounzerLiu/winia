use std::collections::LinkedList;
use std::ops::Add;
use clonelet::clone;
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};
use proc_macro::item;
use crate::shared::{Children, Shared};
use crate::ui::app::{EventLoopProxy, WindowContext};
use crate::ui::component::RectangleExt;
use crate::ui::Item;
use crate::ui::item::Size;
use crate::ui::layout::StackExt;
use crate::ui::theme::color;

enum PageAction {
    Push(Box<dyn Fn(&WindowContext, PageManager) -> Item + Send>),
    Pop,
}

struct PageStackProperty {
    actions: Shared<LinkedList<PageAction>>,
}

#[derive(Clone)]
pub struct PageManager {
    actions: Shared<LinkedList<PageAction>>,
    event_loop_proxy: EventLoopProxy
}

impl PageManager {
    pub fn push<F>(&self, page_fn: F)
    where
        F: Fn(&WindowContext, PageManager) -> Item + Send + 'static,
    {
        self.actions.lock().push_back(PageAction::Push(Box::new(page_fn)));
        self.event_loop_proxy.request_layout();
    }

    pub fn pop(&self) {
        self.actions.lock().push_back(PageAction::Pop);
        self.event_loop_proxy.request_layout();
    }
}

#[item(
    first_page: impl Fn(&WindowContext, PageManager) -> Item + Send
)]
pub struct PageStack {
    item: Item,
    property: Shared<PageStackProperty>,
}

impl PageStack {
    pub fn new(window_context: &WindowContext, first_page: impl Fn(&WindowContext, PageManager) -> Item + Send) -> Self {
        let w = window_context;
        let e = w.event_loop_proxy().clone();
        let children = Children::new();
        let item = w.stack(children.clone()).item();
        let id = item.data().get_id();
        let actions = Shared::from(LinkedList::new()).redraw_when_changed(&e, id);
        let page_manager = PageManager {
            actions: actions.clone(),
            event_loop_proxy: e,
        };
        let first_page_item = first_page(w, page_manager.clone());
        children.add_item(first_page_item);

        let measure = item.data().get_measure();
        item.data().set_measure({
            clone!(actions, page_manager, children);
            move |item, width_mode, height_mode| {
                if let Some(action) = actions.lock().pop_front() {
                    match action {
                        PageAction::Push(page_fn) => {
                            let window_context = item.get_window_context();
                            let new_item = page_fn(item.get_window_context(), page_manager.clone());
                            let background = window_context.stack(
                                new_item
                            ).item().size(Size::Fill, Size::Fill).background(window_context.rectangle(
                                *window_context.theme().lock().get_color(color::BACKGROUND).unwrap()
                            ).item()).on_click(|_| {});
                            children.insert_with_animation(
                                children.len(),
                                background
                            );
                        }
                        PageAction::Pop => {
                            if children.len() > 1 {
                                let last_item_id = children.lock().last().map(|item| item.data().get_id());
                                if let Some(id) = last_item_id {
                                    children.remove_with_animation(id);
                                }
                            }
                        }
                    }
                }
                measure.lock()(item, width_mode, height_mode);
            }
        }).set_keyboard_input({
            clone!(page_manager);
            move |item, input| {
                if input.key_event.state == ElementState::Pressed {
                    match input.key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            page_manager.pop();
                            true
                        }
                        _=> {false}
                    }
                } else {
                    false
                }
            }
        });

        let property = Shared::from(PageStackProperty { actions });
        PageStack {
            item,
            property,
        }
    }
}