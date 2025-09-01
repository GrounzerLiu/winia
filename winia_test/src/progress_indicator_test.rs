use crate::Children;
use winia::children;
use winia::shared::{Settable, SharedF32};
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::divider::DividerExt;
use winia::ui::component::{ProgressIndicatorExt, ProgressIndicatorType, SliderExt};
use winia::ui::item::Size;
use winia::ui::layout::ColumnExt;

pub fn progress_indicator_test(w: &WindowContext) -> Item {
    let progress = SharedF32::from(0.0);
    w.column(children!(
        w.progress_indicator(ProgressIndicatorType::Circular, &progress, true)
            .item()
            .margin_bottom(8),
        w.progress_indicator(ProgressIndicatorType::Circular, &progress, false)
            .item()
            .margin_bottom(8),
        w.progress_indicator(ProgressIndicatorType::Linear, &progress, true)
            .item()
            .margin_bottom(8),
        w.progress_indicator(ProgressIndicatorType::Linear, &progress, false)
            .item()
            .margin_bottom(8),
        w.divider().item().width(Size::Fill).margin(8),
        w.slider(0.0, 1.0, &progress,{
            let progress = progress.clone();
            move |v| {
                progress.set(v);
            }
        }).item(),
    ))
    .item()
    .padding(16)
    .padding_top(36)
}
