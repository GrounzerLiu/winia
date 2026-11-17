//! The Android side of the back bridge.
//!
//! `com.winia.back.WiniaBack` owns the platform callback and forwards the four event kinds here;
//! this module only queues them (`back::inbox`) and wakes the loop. Dispatch happens on the
//! event-loop thread in `AppState::pump_back_events`, never here.
//!
//! The Java side is attached from `run_app_android`; registration stays off until the first
//! enabled handler exists, so the system keeps its own back animation while nothing can consume it.

use std::sync::atomic::{AtomicBool, Ordering};

use jni::objects::JObject;
use jni::{JValue, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

use crate::back::BackEvent;
use crate::back::BackSwipeEdge;
use crate::back::inbox::{self, Signal};
use crate::platform::android::AndroidJni;

/// Mirrors the Java-side registration state so `nativeHasHandlers` can answer without touching
/// the UI-thread table.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Attach the Java bridge to the activity that owns this run.
pub(crate) fn attach(app: &AndroidApp) {
    let result = AndroidJni::new(app).with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.back.WiniaBack"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        env.call_static_method(
            class,
            jni_str!("attach"),
            jni_sig!("(Landroid/app/Activity;)V"),
            &[JValue::Object(&activity)],
        )?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("back bridge attach failed: {error:?}");
    }
}

/// Tell Java whether a system callback should be registered.
///
/// Called from the event loop after a composition or a dispatch changed the table; the atomic
/// check keeps a per-frame call from turning into a JNI round trip.
pub(crate) fn set_enabled(app: &AndroidApp, enabled: bool) {
    if ENABLED.swap(enabled, Ordering::AcqRel) == enabled {
        return;
    }
    let result = AndroidJni::new(app).with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.back.WiniaBack"))?;
        env.call_static_method(
            class,
            jni_str!("setEnabled"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(enabled)],
        )?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("back bridge setEnabled failed: {error:?}");
    }
}

fn event(touch_x: f32, touch_y: f32, progress: f32, edge: i32, frame_millis: i64) -> BackEvent {
    BackEvent {
        touch: (touch_x, touch_y),
        progress,
        edge: BackSwipeEdge::from_android(edge),
        frame_time: std::time::Duration::from_millis(frame_millis.max(0) as u64),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_winia_back_WiniaBack_nativeBackStarted(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
    touch_x: jni::sys::jfloat,
    touch_y: jni::sys::jfloat,
    progress: jni::sys::jfloat,
    edge: jni::sys::jint,
    frame_millis: jni::sys::jlong,
) {
    inbox::push(Signal::Started(event(
        touch_x,
        touch_y,
        progress,
        edge,
        frame_millis,
    )));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_winia_back_WiniaBack_nativeBackProgressed(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
    touch_x: jni::sys::jfloat,
    touch_y: jni::sys::jfloat,
    progress: jni::sys::jfloat,
    edge: jni::sys::jint,
    frame_millis: jni::sys::jlong,
) {
    inbox::push(Signal::Progressed(event(
        touch_x,
        touch_y,
        progress,
        edge,
        frame_millis,
    )));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_winia_back_WiniaBack_nativeBackCompleted(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) {
    inbox::push(Signal::Completed);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_winia_back_WiniaBack_nativeBackCancelled(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) {
    inbox::push(Signal::Cancelled);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_winia_back_WiniaBack_nativeHasHandlers(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) -> jni::sys::jboolean {
    ENABLED.load(Ordering::Acquire)
}
