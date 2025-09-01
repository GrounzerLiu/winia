use crate::Children;
use clonelet::clone;
use winia::children;
use winia::shared::{Settable, Shared};
use winia::text::StyledText;
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::{ButtonExt, CheckboxExt, FilledTextFieldExt, TextExt};
use winia::ui::layout::{ColumnExt, JustifyContent, RowExt};

pub fn login_test(w: &WindowContext) -> Item {
    let username = Shared::from_static(StyledText::from(""));
    let password = Shared::from_static(StyledText::from(""));
    w.row(children!(
        w.column(children!(
            w.filled_text_field(&username).item(),
            w.filled_text_field(&password).item(),
            w.button("Login").item().on_click({ move |source| {} }),
            {
                let checked = Shared::from_static(false);
                w.checkbox(&checked).item()
            }
        ))
        .item()
    ))
    .justify_content(JustifyContent::Center)
    .item()
}
