use skia_safe::Color;
use crate::uib::{AdditionalProperty, Item};
use crate::property::{Observable, Observer, ObservableProperty};

pub trait ShadowColor {
    fn get_shadow_color(&self) -> Option<ObservableProperty<Color>>;
    fn shadow_color(self, color: impl Into<ObservableProperty<Color>>) -> Self;
}

impl ShadowColor for Item {
    fn get_shadow_color(&self) -> Option<ObservableProperty<Color>> {
        if let Some(value) = self.get_additional_property("shadow_color") {
            if let AdditionalProperty::SharedColor(value) = value {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn shadow_color(mut self, color: impl Into<ObservableProperty<Color>>) -> Self {
        if let Some(color) = self.get_shadow_color() {
            color.clear_observers();
        }
        let color = color.into();
        let mut app = self.get_app().clone();
        color.add_observer(
            move || {
                app.request_redraw();
            },
            self.get_id(),
        );
        self.set_additional_property("shadow_color", AdditionalProperty::SharedColor(color));
        self
    }
}

pub trait ShadowOffsetX {
    fn get_shadow_offset_x(&self) -> Option<ObservableProperty<f32>>;
    fn shadow_offset_x(self, offset: impl Into<ObservableProperty<f32>>) -> Self;
}

impl ShadowOffsetX for Item {
    fn get_shadow_offset_x(&self) -> Option<ObservableProperty<f32>> {
        if let Some(value) = self.get_additional_property("shadow_offset_x") {
            if let AdditionalProperty::SharedF32(value) = value {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn shadow_offset_x(mut self, offset: impl Into<ObservableProperty<f32>>) -> Self {
        if let Some(offset) = self.get_shadow_offset_x() {
            offset.clear_observers();
        }
        let offset = offset.into();
        let mut app = self.get_app().clone();
        offset.add_observer(
            move || {
                app.request_redraw();
            },
            self.get_id(),
        );
        self.set_additional_property("shadow_offset_x", AdditionalProperty::SharedF32(offset));
        self
    }
}

pub trait ShadowOffsetY {
    fn get_shadow_offset_y(&self) -> Option<ObservableProperty<f32>>;
    fn shadow_offset_y(self, offset: impl Into<ObservableProperty<f32>>) -> Self;
}

impl ShadowOffsetY for Item {
    fn get_shadow_offset_y(&self) -> Option<ObservableProperty<f32>> {
        if let Some(value) = self.get_additional_property("shadow_offset_y") {
            if let AdditionalProperty::SharedF32(value) = value {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn shadow_offset_y(mut self, offset: impl Into<ObservableProperty<f32>>) -> Self {
        if let Some(offset) = self.get_shadow_offset_y() {
            offset.clear_observers();
        }
        let offset = offset.into();
        let mut app = self.get_app().clone();
        offset.add_observer(
            move || {
                app.request_redraw();
            },
            self.get_id(),
        );
        self.set_additional_property("shadow_offset_y", AdditionalProperty::SharedF32(offset));
        self
    }
}

pub trait ShadowBlur {
    fn get_shadow_blur(&self) -> Option<ObservableProperty<f32>>;
    fn shadow_blur(self, blur: impl Into<ObservableProperty<f32>>) -> Self;
}

impl ShadowBlur for Item {
    fn get_shadow_blur(&self) -> Option<ObservableProperty<f32>> {
        if let Some(value) = self.get_additional_property("shadow_blur") {
            if let AdditionalProperty::SharedF32(value) = value {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn shadow_blur(mut self, blur: impl Into<ObservableProperty<f32>>) -> Self {
        if let Some(blur) = self.get_shadow_blur() {
            blur.clear_observers();
        }
        let blur = blur.into();
        let mut app = self.get_app().clone();
        blur.add_observer(
            move || {
                app.request_redraw();
            },
            self.get_id(),
        );
        self.set_additional_property("shadow_blur", AdditionalProperty::SharedF32(blur));
        self
    }
}