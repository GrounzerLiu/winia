
pub mod core;
pub mod error;
pub mod shared;
pub mod ui;
pub mod app;
pub mod animation;
pub mod theme;
pub mod text;
mod drawable;
pub mod collection;
pub mod icon;
pub mod event;

pub use theme::Theme;

pub use skia_safe;
//pub use winit::*;
pub use parking_lot::*;
pub use skiwin::*;
pub use proc_macro::ItemProps;
pub use proc_macro::closure_use;


pub trait OptionalInvoke<T> {
    fn if_some(self, invoke: impl FnOnce(T));
    fn if_none(self, invoke: impl FnOnce());
    fn if_ref_none(&self, invoke: impl FnOnce());
    fn if_mut_none(&mut self, invoke: impl FnOnce());
    fn if_ref_some(&self, invoke: impl FnOnce(&T));
    fn if_mut_some(&mut self, invoke: impl FnOnce(&mut T));
}

impl<T> OptionalInvoke<T> for Option<T> {
    fn if_some(self, invoke: impl FnOnce(T)) {
        if let Some(value) = self {
            invoke(value);
        }
    }

    fn if_none(self, invoke: impl FnOnce()) {
        if self.is_none() {
            invoke();
        }
    }

    fn if_ref_none(&self, invoke: impl FnOnce()) {
        if self.is_none() {
            invoke();
        }
    }

    fn if_mut_none(&mut self, invoke: impl FnOnce()) {
        if self.is_none() {
            invoke();
        }
    }

    fn if_ref_some(&self, invoke: impl FnOnce(&T)) {
        if let Some(value) = self {
            invoke(value);
        }
    }

    fn if_mut_some(&mut self, invoke: impl FnOnce(&mut T)) {
        if let Some(value) = self {
            invoke(value);
        }
    }
}

pub trait Let<R> {
    fn let_ref(&self, invoke: impl FnOnce(&Self) -> R) -> R;
    fn let_mut(&mut self, invoke: impl FnOnce(&mut Self) -> R) -> R;
}

impl<T, R> Let<R> for T {
    fn let_ref(&self, invoke: impl FnOnce(&Self) -> R) -> R {
        invoke(self)
    }

    fn let_mut(&mut self, invoke: impl FnOnce(&mut Self) -> R) -> R {
        invoke(self)
    }
}

pub trait With {
    fn with_ref(self, invoke: impl FnOnce(&Self)) -> Self;
    fn with_mut(self, invoke: impl FnOnce(&mut Self)) -> Self;
}

impl<T> With for T {
    fn with_ref(self, invoke: impl FnOnce(&Self)) -> Self {
        invoke(&self);
        self
    }

    fn with_mut(mut self, invoke: impl FnOnce(&mut Self)) -> Self {
        invoke(&mut self);
        self
    }
}

pub trait ResultInvoke<T, E> {
    fn if_ok(self, invoke: impl FnOnce(T));
    fn if_err(self, invoke: impl FnOnce(E));
    fn if_ref_ok(&self, invoke: impl FnOnce(&T));
    fn if_ref_err(&self, invoke: impl FnOnce(&E));
    fn if_mut_ok(&mut self, invoke: impl FnOnce(&mut T));
    fn if_mut_err(&mut self, invoke: impl FnOnce(&mut E));
}

impl<T, E> ResultInvoke<T, E> for Result<T, E> {
    fn if_ok(self, invoke: impl FnOnce(T)) {
        if let Ok(value) = self {
            invoke(value);
        }
    }

    fn if_err(self, invoke: impl FnOnce(E)) {
        if let Err(error) = self {
            invoke(error);
        }
    }

    fn if_ref_ok(&self, invoke: impl FnOnce(&T)) {
        if let Ok(value) = self {
            invoke(value);
        }
    }

    fn if_ref_err(&self, invoke: impl FnOnce(&E)) {
        if let Err(error) = self {
            invoke(error);
        }
    }

    fn if_mut_ok(&mut self, invoke: impl FnOnce(&mut T)) {
        if let Ok(value) = self {
            invoke(value);
        }
    }

    fn if_mut_err(&mut self, invoke: impl FnOnce(&mut E)) {
        if let Err(error) = self {
            invoke(error);
        }
    }
}
