use winia::app::WindowContext;
use winia::ui::{stack, Item, Size, StackPropsTrait};
use winia_video::video::{video, VideoPropsTrait};

pub fn video_player_test(w: &WindowContext) -> Item {
    stack(
        w.stack_props()
         .size(Size::Fill, Size::Fill),
        video(
            w.video_props("/home/grounzer/Downloads/filem 副本.mp4")
             .size(Size::Fill, Size::Fill)
        ),
    )
}