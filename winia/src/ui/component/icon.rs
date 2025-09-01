use crate::icon::{IconDrawable, MaterialSymbol};
use crate::shared::{Children, Gettable, Shared};
use crate::ui::app::WindowContext;
use crate::ui::component::Drawable;
use crate::ui::item::MeasureMode;
use crate::ui::Item;
use clonelet::clone;
use proc_macro::item;
use skia_safe::Color;

struct IconProperty {
    symbol: Shared<MaterialSymbol>,
    color: Shared<Color>,
    fill: Shared<f32>,
    weight: Shared<f32>,
    grade: Shared<f32>,
    optical_size: Shared<f32>,
}

#[item(
    symbol: impl Into<Shared<MaterialSymbol>>,
    color: impl Into<Shared<Color>>
)]
pub struct Icon {
    item: Item,
    property: Shared<IconProperty>,
}

impl Icon {
    pub fn new(
        window_context: &WindowContext,
        symbol: impl Into<Shared<MaterialSymbol>>,
        color: impl Into<Shared<Color>>,
    ) -> Self {
        let w = window_context;
        let e = w.event_loop_proxy().clone();
        let item = Item::new(w, Children::new());
        let id = item.data().get_id();
        let symbol = symbol.into().redraw_when_changed(&e, id);
        let color = color.into().redraw_when_changed(&e, id);
        let fill = Shared::from(0.0).redraw_when_changed(&e, id);
        let weight = Shared::from(400.0).redraw_when_changed(&e, id);
        let grade = Shared::from(0.0).redraw_when_changed(&e, id);
        let optical_size = Shared::from(24.0).redraw_when_changed(&e, id);
        let property = Shared::from(IconProperty {
            symbol: symbol.clone(),
            color: color.clone(),
            fill: fill.clone(),
            weight: weight.clone(),
            grade: grade.clone(),
            optical_size: optical_size.clone(),
        });
        let icon_drawable = match symbol.get() {
            #[cfg(feature = "material-symbols-outlined")]
            MaterialSymbol::Outlined(_) => IconDrawable::outlined(symbol.get(), 24.0, color.get()),
            #[cfg(feature = "material-symbols-rounded")]
            MaterialSymbol::Rounded(_) => {
                IconDrawable::rounded(symbol.get(), 24.0, color.get())
            }
            #[cfg(feature = "material-symbols-sharp")]
            MaterialSymbol::Sharp(_) => IconDrawable::sharp(symbol.get(), 24.0, color.get()),
        };
        let icon_drawable = Shared::from_static(icon_drawable);
        item.data()
            .set_measure({
                move |item, width_mode, height_mode| {
                    let (w, h) = match (width_mode, height_mode) {
                        (MeasureMode::Specified(w), MeasureMode::Specified(h)) => (w, h),
                        (MeasureMode::Unspecified(_), MeasureMode::Specified(h)) => (24.0, h),
                        (MeasureMode::Specified(w), MeasureMode::Unspecified(_)) => (w, 24.0),
                        (MeasureMode::Unspecified(_), MeasureMode::Unspecified(_)) => (24.0, 24.0),
                    };
                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = w;
                    measure_parameter.height = h;
                }
            })
            .set_layout({
                clone!(icon_drawable, property);
                move |item, width, height| {
                    let size = width.min(height);
                    icon_drawable.lock().set_width(size);
                    icon_drawable.lock().set_height(size);
                    let target_parameter = item.get_target_parameter();
                    target_parameter.width = size;
                    target_parameter.height = size;
                    let property = property.lock();
                    let color = property.color.get();
                    let fill = property.fill.get();
                    let weight = property.weight.get();
                    let grade = property.grade.get();
                    let optical_size = property.optical_size.get();
                    target_parameter.set_color_param("color", color);
                    target_parameter.set_float_param("fill", fill);
                    target_parameter.set_float_param("weight", weight);
                    target_parameter.set_float_param("grade", grade);
                    target_parameter.set_float_param("optical_size", optical_size);
                }
            })
            .set_draw({
                clone!(icon_drawable, property);
                move |item, canvas| {
                    let property = property.lock();
                    let symbol = property.symbol.get();
                    let display_parameter = item.get_display_parameter();
                    let color = display_parameter.get_color_param("color").unwrap_or(Color::BLACK);
                    let fill = display_parameter.get_float_param("fill").unwrap_or(0.0);
                    let weight = display_parameter.get_float_param("weight").unwrap_or(400.0);
                    let grade = display_parameter.get_float_param("grade").unwrap_or(0.0);
                    let optical_size = display_parameter.get_float_param("optical_size").unwrap_or(24.0);
                    let mut icon_drawable = icon_drawable.lock();
                    // icon_drawable.set_symbol(symbol);
                    icon_drawable.set_color(Some(color));
                    icon_drawable.set_fill(fill);
                    icon_drawable.set_weight(weight);
                    icon_drawable.set_grade(grade);
                    icon_drawable.set_optical_size(optical_size);
                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let width = display_parameter.width;
                    let height = display_parameter.height;

                    let icon_x = x + (width - icon_drawable.width()) / 2.0;
                    let icon_y = y + (height - icon_drawable.height()) / 2.0;

                    icon_drawable.draw(canvas, icon_x, icon_y);
                }
            });
        Icon { item, property }
    }

    pub fn fill(self, fill: impl Into<Shared<f32>>) -> Self {
        self.property.lock().fill.set_shared(fill);
        self
    }

    pub fn weight(self, weight: impl Into<Shared<f32>>) -> Self {
        self.property.lock().weight.set_shared(weight);
        self
    }

    pub fn grade(self, grade: impl Into<Shared<f32>>) -> Self {
        self.property.lock().grade.set_shared(grade);
        self
    }

    pub fn optical_size(self, optical_size: impl Into<Shared<f32>>) -> Self {
        self.property.lock().optical_size.set_shared(optical_size);
        self
    }
}
