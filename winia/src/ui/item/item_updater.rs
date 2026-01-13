use parking_lot::Mutex;
use crate::shared::SharedDerivedBool;

#[derive(Clone)]
pub struct ItemUpdater {
    pub is_fixed_size: SharedDerivedBool,
    pub need_redraw: bool,
    pub parent: Option<std::sync::Weak<Mutex<ItemUpdater>>>,
}

impl ItemUpdater {
    pub fn request_update(&mut self) {
        self.need_redraw = true;
        let mut parent = self.parent.clone();
        while let Some(p) = &mut parent {
            if let Some(p) = p.upgrade() {
                let mut p = p.lock();
                p.need_redraw = true;
                if p.is_fixed_size.get() {
                    break;
                }
                parent = p.parent.clone();
            } else {
                break;
            }
        }
    }
}