use crate::app::EventLoopProxy;
use crate::shared::{Derived, SharedDerivedBool, SharedSource, SharedWeak};

#[derive(Default)]
pub struct FocusRequester {
    item_id: u32,
    focusable: Option<SharedWeak<bool, Derived>>,
    event_loop_proxy: Option<EventLoopProxy>,
}
impl FocusRequester {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_focus(&self) {
        if let (Some(event_loop_proxy), Some(focusable)) =
            (&self.event_loop_proxy, &self.focusable)
        {
            if let Some(focusable) = focusable.upgrade() {
                if focusable.get() {
                    event_loop_proxy.request_focus(self.item_id);
                }
            }
        }
    }

    pub fn clear_focus(&self) {
        if let (Some(event_loop_proxy), Some(_focusable)) =
            (&self.event_loop_proxy, &self.focusable)
        {
            event_loop_proxy.request_focus(0);
        }
    }

    pub fn set_focusable(&mut self, focusable: &SharedDerivedBool) {
        self.focusable = Some(focusable.weak());
    }

    pub(crate) fn set_item_id(&mut self, item_id: u32) {
        self.item_id = item_id;
    }
    pub(crate) fn set_event_loop_proxy(&mut self, event_loop_proxy: EventLoopProxy) {
        self.event_loop_proxy = Some(event_loop_proxy);
    }
}

pub trait FocusRequesterTrait {
    fn new_focus_requester() -> SharedSource<FocusRequester>;
    fn request_focus(&self);
    fn clear_focus(&self);
}

impl FocusRequesterTrait for SharedSource<FocusRequester> {
    fn new_focus_requester() -> SharedSource<FocusRequester> {
        SharedSource::new(FocusRequester::new())
    }

    fn request_focus(&self) {
        self.read().request_focus();
    }

    fn clear_focus(&self) {
        self.read().clear_focus();
    }
}