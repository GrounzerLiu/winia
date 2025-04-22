use crate::shared::{Children, Settable, Shared};
use crate::text::StyledText;
use crate::ui::app::WindowContext;
use crate::ui::component::{RadioExt, TextExt};
use crate::ui::layout::{AlignItems, RowExt};

pub trait RadioGroupExt<T> {
    fn radio_group(
        &self,
        selected_value: &Shared<T>,
        values: &[(T, StyledText)],
        on_selected: impl FnMut(&Shared<T>, T) + Clone + 'static,
    ) -> Children;
}

impl<T: Clone + PartialEq + Send + 'static> RadioGroupExt<T> for WindowContext {
    fn radio_group(
        &self,
        selected_value: &Shared<T>,
        values: &[(T, StyledText)],
        on_selected: impl FnMut(&Shared<T>, T) + Clone + 'static,
    ) -> Children {
        let mut children = Children::new();
        for (value, label) in values {
            let radio = self.radio(value.clone(), selected_value, Some(Box::new(on_selected.clone())))
                .item()
                .margin_end(4);
            let mut on_click = radio.data().get_on_click().cloned();
            children.add(
                self.row(
                    radio
                        + self.text(label).font_size(16).item().on_click({
                            let selected_value = selected_value.clone();
                            let value = value.clone();
                            move |s| {
                                if let Some(on_click) = &mut on_click {
                                    on_click.lock()(s);
                                }
                            }
                        }),
                )
                .align_items(AlignItems::Center)
                .item(),
            );
        }
        children
    }
}
