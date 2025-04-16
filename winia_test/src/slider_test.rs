use winia::shared::{Gettable, Settable, SharedF32, SharedText};
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{SliderExt, TextExt};
use winia::ui::Item;
use winia::ui::layout::{ColumnExt, Flex, FlexExt, FlexGrow, FlexWrap, RowExt};

pub fn slider_test(w: &WindowContext, _attr: &WindowAttr) -> Item {
    let text = SharedText::from_static("0.0".into());
    let text_id = text.id();
    let value = SharedF32::from_static(0.0);
    let value_id = value.id();
    text.add_specific_observer(
        value_id,
        {
            let value = value.clone();
            move |text| {
                let num = text.to_string().parse::<f32>();
                if let Ok(num) = num {
                    value.try_set_static(num);
                }
            }
        }
    );
    value.add_specific_observer(
        text_id,
        {
            let text = text.clone();
            move |value| {
                text.try_set_static(format!("{:.1}", value).into());
            }
        }
    );
    w.row(
        w.text(text).item().padding_end(16).width(100)
        +w.slider(0.0, 100.0, &value)
            .item().width(400)
    ).padding(16)
}