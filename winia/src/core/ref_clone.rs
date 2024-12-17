use std::rc::Rc;
use std::sync::Arc;

pub trait RefClone: Sized {
    /// Returns a new reference to `self`.
    /// In general, all properties of the type that implements this trait should be of type Arc or Rc.
    fn ref_clone(&self) -> Self;
}

// impl<T> RefClone for Arc<T> {
//     fn ref_clone(&self) -> Self {
//         self.clone()
//     }
// }
//
// impl<T> RefClone for Rc<T> {
//     fn ref_clone(&self) -> Self {
//         self.clone()
//     }
// }