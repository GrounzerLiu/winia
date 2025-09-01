use crate::Children;
use winia::shared::{Settable, SharedF32, SharedText};
use winia::text::StyledText;
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::{SliderExt, TextExt};
use winia::ui::layout::RowExt;
use winia::{bind, children};

pub fn slider_test(w: &WindowContext) -> Item {
    let text = SharedText::from_static("0.0".into());
    let value = SharedF32::from_static(0.0);
    bind!(
        text,
        value,
        |text: &mut StyledText| text.to_string().parse::<f32>().ok(),
        |value| Some(format!("{:.1}", value).into())
    );
    w.row(children!(
        w.text(text)
            .editable(true)
            .item()
            .padding_end(16)
            .width(100),
        w.slider(0.0, 100.0, &value, {
            let value = value.clone();
            move |v| {
                value.set(v);
            }
        }).item().width(400)
    ))
    .item()
    .padding(16)
}
