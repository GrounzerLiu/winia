use winia::shared::{Gettable, SharedBool, SharedText};
use winia::ui::app::WindowContext;
use winia::ui::component::{SwitchExt, TextExt};
use winia::ui::Item;
use winia::ui::layout::{AlignItems, ColumnExt, RowExt, StackExt};

pub fn switch_test(w: &WindowContext) -> Item {
    let selected_1 = SharedBool::from_static(false);
    let selected_2 = SharedBool::from_static(true);
    let selected_3 = SharedBool::from_static(false);
    
    fn switch_with_text(
        w: &WindowContext,
        selected: &SharedBool,
    ) -> Item {
        w.row(
            w.switch(selected).item().margin_end(16).margin_start(16)
            + w.text(
                SharedText::from_dynamic(
                    [selected.to_observable()].into(),
                    {
                        let selected = selected.clone();
                        move || {
                            if selected.get() {
                                "true".into()
                            } else {
                                "false".into()
                            }
                        }
                    }
                )
            ).item()
        ).align_items(AlignItems::Center).item().min_height(48)
    }
    w.column(
        switch_with_text(w, &selected_1)
        + switch_with_text(w, &selected_2)
        + switch_with_text(w, &selected_3)
    ).item()
}