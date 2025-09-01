use std::sync::Arc;
use gstreamer as gst;
use gstreamer::prelude::{Cast, DeviceExt, ElementExt, ElementExtManual, GstBinExt, GstBinExtManual, ObjectExt, PadExt};
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use skia_safe::{AlphaType, Canvas, ColorType, Data, ImageInfo};
use winia::ui::app::EventLoopProxy;

pub struct VideoPlayer {
    event_loop_proxy: EventLoopProxy,
    pipeline: gst::Pipeline,
    app_sink: gst_app::AppSink,
    current_frame: Arc<Mutex<Option<Vec<u8>>>>,
    video_info: Arc<Mutex<Option<gst_video::VideoInfo>>>,
    is_playing: Arc<Mutex<bool>>,
}

impl VideoPlayer {
    pub fn new(event_loop_proxy: &EventLoopProxy) -> Result<Self, Box<dyn std::error::Error>> {
        gst::init()?;

        let pipeline = gst::Pipeline::with_name("video-player");
        
        let app_sink = gst_app::AppSink::builder()
            .name("video-sink")
            .caps(
                &gst_video::VideoCapsBuilder::new()
                    .format(gst_video::VideoFormat::Rgba)
                    .build(),
            )
            .build();

        app_sink.set_drop(true);
        app_sink.set_max_buffers(1);

        let current_frame = Arc::new(Mutex::new(None));
        let video_info = Arc::new(Mutex::new(None));
        let is_playing = Arc::new(Mutex::new(false));

        Ok(VideoPlayer {
            event_loop_proxy: event_loop_proxy.clone(),
            pipeline,
            app_sink,
            current_frame,
            video_info,
            is_playing,
        })
    }

    pub fn load_video(&mut self, uri: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.pipeline.set_state(gst::State::Null)?;
        
        let source = if uri.starts_with("http://") || uri.starts_with("https://") {
            gst::ElementFactory::make("souphttpsrc")
                .name("source")
                .property("location", uri)
                .build()?
        } else {
            gst::ElementFactory::make("filesrc")
                .name("source")
                .property("location", uri)
                .build()?
        };

        let decodebin = gst::ElementFactory::make("decodebin")
            .name("decoder")
            .build()?;

        let audio_convert = gst::ElementFactory::make("audioconvert")
            .name("audio-convert")
            .build()?;

        let audio_resample = gst::ElementFactory::make("audioresample")
            .name("audio-resample")
            .build()?;

        let audio_sink = gst::ElementFactory::make("autoaudiosink")
            .name("audio-sink")
            .build()?;

        let video_convert = gst::ElementFactory::make("videoconvert")
            .name("video-convert")
            .build()?;

        let video_scale = gst::ElementFactory::make("videoscale")
            .name("video-scale")
            .build()?;
        
        self.pipeline.add_many([
            &source,
            &decodebin,
            &audio_convert,
            &audio_resample,
            &audio_sink,
            &video_convert,
            &video_scale,
            self.app_sink.upcast_ref(),
        ])?;

        source.link(&decodebin)?;
        audio_convert.link(&audio_resample)?;
        audio_resample.link(&audio_sink)?;
        video_convert.link(&video_scale)?;
        video_scale.link(&self.app_sink)?;
        
        let audio_convert_weak = audio_convert.downgrade();
        let video_convert_weak = video_convert.downgrade();

        decodebin.connect_pad_added(move |_, pad| {
            let caps = pad.current_caps().unwrap();
            let structure = caps.structure(0).unwrap();
            let media_type = structure.name();

            if media_type.starts_with("audio/") {
                if let Some(audio_convert) = audio_convert_weak.upgrade() {
                    let sink_pad = audio_convert.static_pad("sink").unwrap();
                    if !sink_pad.is_linked() {
                        pad.link(&sink_pad).unwrap();
                    }
                }
            } else if media_type.starts_with("video/") {
                if let Some(video_convert) = video_convert_weak.upgrade() {
                    let sink_pad = video_convert.static_pad("sink").unwrap();
                    if !sink_pad.is_linked() {
                        pad.link(&sink_pad).unwrap();
                    }
                }
            }
        });

        let current_frame = Arc::clone(&self.current_frame);
        let video_info = Arc::clone(&self.video_info);

        let event_loop_proxy = self.event_loop_proxy.clone();
        self.app_sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;

                    let video_info_obj = gst_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;

                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let data = map.as_slice().to_vec();

                    *current_frame.lock() = Some(data);
                    *video_info.lock() = Some(video_info_obj);
                    
                    event_loop_proxy.request_redraw();

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        Ok(())
    }

    pub fn play(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.pipeline.set_state(gst::State::Playing)?;
        *self.is_playing.lock() = true;
        Ok(())
    }

    pub fn pause(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.pipeline.set_state(gst::State::Paused)?;
        *self.is_playing.lock() = false;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.pipeline.set_state(gst::State::Null)?;
        *self.is_playing.lock() = false;
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        *self.is_playing.lock()
    }

    pub fn draw_current_frame(&self, canvas: &Canvas, x: f32, y: f32) -> Result<(), Box<dyn std::error::Error>> {
        let frame_data = self.current_frame.lock();
        let video_info = self.video_info.lock();

        if let (Some(data), Some(info)) = (frame_data.as_ref(), video_info.as_ref()) {
            let width = info.width() as i32;
            let height = info.height() as i32;

            let image_info = ImageInfo::new(
                (width, height),
                ColorType::RGBA8888,
                AlphaType::Premul,
                None,
            );

            let skia_data = Data::new_copy(data.as_slice());

            if let Some(image) = skia_safe::images::raster_from_data(&image_info, skia_data, (width * 4) as usize) {
                canvas.draw_image(&image, (0, 0), None);
            }
        }

        Ok(())
    }

    pub fn seek(&self, position: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_nseconds(position),
        )?;
        Ok(())
    }

    pub fn get_position(&self) -> Option<u64> {
        self.pipeline.query_position::<gst::ClockTime>()
            .map(|pos| pos.nseconds())
    }

    pub fn get_duration(&self) -> Option<u64> {
        self.pipeline.query_duration::<gst::ClockTime>()
            .map(|dur| dur.nseconds())
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(audio_sink) = self.pipeline.by_name("audio-sink") {
            audio_sink.set_property("volume", volume);
        }
        Ok(())
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;
    use super::*;

    #[test]
    fn test_player() {
        // let mut player = VideoPlayer::new().unwrap();
        // player.load_video("/home/grounzer/Downloads/30372990671-1-192.mp4").unwrap();
        // player.play().unwrap();
        // thread::sleep(Duration::from_secs(30)); // Let it play for a while
        // 
    }
}
