use proc_macro::item;
use winia::shared::{Children, Shared, SharedText};
use winia::ui::app::WindowContext;
use winia::ui::Item;
use crate::video_player::VideoPlayer;

#[item(uri: impl Into<SharedText>)]
pub struct Video {
    item: Item
}

impl Video {
    pub fn new(window_context: &WindowContext, uri: impl Into<SharedText>) -> Self {
        let item = Item::new(window_context, Children::new());
        let uri = uri.into();
        let video_player = Shared::from(VideoPlayer::new(window_context.event_loop_proxy()).unwrap());
        video_player.lock().load_video(uri.lock().as_str()).unwrap();
        video_player.lock().play().unwrap();
        item.data().set_draw({
            let video_player = video_player.clone();
            move |item, canvas| {
                let display_parameter = item.get_display_parameter();
                video_player.lock().draw_current_frame(canvas, display_parameter.x(), display_parameter.y()).unwrap();
            }
        });
        
        Self {
            item
        }
    }
}