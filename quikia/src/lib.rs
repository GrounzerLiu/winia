pub mod app;
pub mod property;
pub mod text;
pub mod animation;
pub mod ui;
pub mod theme;
pub mod component;
pub mod layout;
mod test;
pub mod widget;

pub use winit::*;
pub use skia_safe::*;

pub trait OptionalInvoke<T>{
    fn if_some(self, invoke: impl FnOnce(T) -> ());
    fn if_none(self, invoke: impl FnOnce() -> ());
    fn if_ref_none(&self, invoke: impl FnOnce() -> ());
    fn if_mut_none(&mut self, invoke: impl FnOnce() -> ());
    fn if_ref_some(&self, invoke: impl FnOnce(&T) -> ());
    fn if_mut_some(&mut self, invoke: impl FnOnce(&mut T) -> ());
}

impl<T> OptionalInvoke<T> for Option<T>{
    fn if_some(self, invoke: impl FnOnce(T) -> ()) {
        if let Some(value) = self{
            invoke(value);
        }
    }

    fn if_none(self, invoke: impl FnOnce() -> ()) {
        if self.is_none(){
            invoke();
        }
    }

    fn if_ref_none(&self, invoke: impl FnOnce() -> ()) {
        if self.is_none(){
            invoke();
        }
    }

    fn if_mut_none(&mut self, invoke: impl FnOnce() -> ()) {
        if self.is_none(){
            invoke();
        }
    }

    fn if_ref_some(&self, invoke: impl FnOnce(&T) -> ()) {
        if let Some(value) = self{
            invoke(value);
        }
    }

    fn if_mut_some(&mut self, invoke: impl FnOnce(&mut T) -> ()) {
        if let Some(value) = self{
            invoke(value);
        }
    }
}