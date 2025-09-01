use winia::ui::app::WindowContext;
use winia::ui::Item;
use winia_video::video::VideoExt;

pub fn video_test(w: &WindowContext) -> Item {
    w.video("/home/grounzer/Downloads/30372990671-1-192.mp4").item()
}