use proc_macro::{item, ItemProps};
use winia::app::WindowContext;
use winia::shared::{Shared, SharedDerived, SharedSource, SharedText};
use winia::ui::Item;
use winia::ui::item::{ItemEvent, ItemKind, ItemProps};
use crate::video_player::VideoPlayer;

/*#[item(uri: impl Into<SharedText>)]
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
}*/
#[derive(ItemProps)]
pub struct VideoProps {
    pub item_props: ItemProps,
    #[constructor]
    pub uri: SharedDerived<String>,
}

impl VideoProps {
    pub fn new(item_props: ItemProps, uri: impl Into<SharedDerived<String>>) -> Self {
        Self {
            item_props,
            uri: uri.into(),
        }
    }
}

pub fn video(props: VideoProps) -> Item {
    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props,
        vec![]
    )
}

fn item_event(props: &VideoProps) -> ItemEvent {
    let video_player = SharedSource::new(
        VideoPlayer::new(
            props.window_context.event_loop_proxy(),
            props.item_updater.clone(),
        ).unwrap()
    );
    video_player.lock().load_video(props.uri.get().as_str()).unwrap();
    video_player.lock().play().unwrap();
    ItemEvent::new()
        .set_draw({
            let video_player = video_player.clone();
            move |item, canvas| {
                let current_frame = item.current_frame();
                video_player.lock().draw_current_frame(canvas, current_frame.x(), current_frame.y()).unwrap();
            }
        })
}

