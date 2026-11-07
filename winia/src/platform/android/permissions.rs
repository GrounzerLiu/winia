//! Android permission backend.
//!
//! The Java adapter (com.winia.permissions.WiniaPermissions) owns XXPermissions and stores the
//! result by request id. Rust polls the result through JNI; no Activity callback override is
//! needed. The adapter also calls a small native notification so a composition reading
//! Permissions::status wakes up when the dialog closes.

use jni::objects::JObject;
use jni::{JValue, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

use crate::permissions::{
    Capability, PermissionError, PermissionRequest, PermissionStatus, reconcile_result,
};
use crate::platform::android::AndroidJni;

fn permission_names(capability: Capability) -> &'static [&'static str] {
    match capability {
        Capability::Camera => &["android.permission.CAMERA"],
        Capability::Microphone => &["android.permission.RECORD_AUDIO"],
        Capability::Location => &[
            "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.ACCESS_COARSE_LOCATION",
        ],
        Capability::BackgroundLocation => &["android.permission.ACCESS_BACKGROUND_LOCATION"],
        Capability::Notifications => &["android.permission.POST_NOTIFICATIONS"],
        Capability::SystemAlertWindow
        | Capability::WriteSettings
        | Capability::ManageExternalStorage
        | Capability::RequestInstallPackages
        | Capability::ScheduleExactAlarm
        | Capability::UsageStats
        | Capability::IgnoreBatteryOptimizations
        | Capability::PictureInPicture
        | Capability::UseFullScreenIntent
        | Capability::AccessNotificationPolicy
        | Capability::ManageMedia
        | Capability::GetInstalledApps => &[],
    }
}

pub(crate) fn status(app: &AndroidApp, capability: Capability) -> PermissionStatus {
    if matches!(capability, Capability::Notifications) && AndroidApp::sdk_version() < 33 {
        return PermissionStatus::NotRequired;
    }
    if capability.is_special() {
        return match is_granted(app, capability) {
            Ok(true) => PermissionStatus::Granted,
            Ok(false) => PermissionStatus::Denied,
            Err(_) => PermissionStatus::Unsupported,
        };
    }
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        for permission in permission_names(capability) {
            let name = env.new_string(permission)?;
            let state = env.call_method(
                &activity,
                jni_str!("checkSelfPermission"),
                jni_sig!("(Ljava/lang/String;)I"),
                &[JValue::Object(&name)],
            )?
            .i()?;
            if state == 0 {
                return Ok(true);
            }
        }
        Ok(false)
    });
    match result {
        Ok(true) => PermissionStatus::Granted,
        Ok(false) => PermissionStatus::Denied,
        Err(_) => PermissionStatus::Unsupported,
    }
}

pub(crate) fn request(
    app: &AndroidApp,
    capability: Capability,
) -> Result<PermissionRequest, PermissionError> {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let jni = AndroidJni::new(app);
    let called = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let name = env.new_string(capability.adapter_name())?;
        env.call_static_method(
            class,
            jni_str!("requestCapability"),
            jni_sig!("(Landroid/app/Activity;JLjava/lang/String;)V"),
            &[
                JValue::Object(&activity),
                JValue::Long(id as i64),
                JValue::Object(&name),
            ],
        )?;
        Ok(())
    });
    match called {
        Ok(()) => Ok(PermissionRequest::new(app.clone(), id)),
        Err(_) => Err(PermissionError::AdapterMissing),
    }
}

fn open_settings_call(app: &AndroidApp, method: &str, capability: Option<Capability>) -> Result<(), PermissionError> {
    let jni = AndroidJni::new(app);
    let called = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        match method {
            "openSettings" => {
                env.call_static_method(
                    class,
                    jni_str!("openSettings"),
                    jni_sig!("(Landroid/app/Activity;)V"),
                    &[JValue::Object(&activity)],
                )?;
            }
            "openSettingsFor" => {
                let capability = capability.expect("capability required for openSettingsFor");
                let name = env.new_string(capability.adapter_name())?;
                env.call_static_method(
                    class,
                    jni_str!("openSettingsFor"),
                    jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)V"),
                    &[JValue::Object(&activity), JValue::Object(&name)],
                )?;
            }
            _ => unreachable!(),
        }
        Ok(())
    });
    called.map_err(|_| PermissionError::AdapterMissing)
}

pub(crate) fn open_settings(app: &AndroidApp) -> Result<(), PermissionError> {
    open_settings_call(app, "openSettings", None)
}

pub(crate) fn open_settings_for(app: &AndroidApp, capability: Capability) -> Result<(), PermissionError> {
    open_settings_call(app, "openSettingsFor", Some(capability))
}

fn is_granted(app: &AndroidApp, capability: Capability) -> Result<bool, PermissionError> {
    let jni = AndroidJni::new(app);
    jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let name = env.new_string(capability.adapter_name())?;
        env.call_static_method(
            class,
            jni_str!("isGranted"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)Z"),
            &[JValue::Object(&activity), JValue::Object(&name)],
        )?
        .z()
    })
    .map_err(|_| PermissionError::AdapterMissing)
}

pub(crate) fn poll_result(app: &AndroidApp, id: u64) -> Option<PermissionStatus> {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        env.call_static_method(
            class,
            jni_str!("takeResult"),
            jni_sig!("(J)I"),
            &[JValue::Long(id as i64)],
        )?
        .i()
    });
    result.ok().and_then(reconcile_result)
}

/// Java -> Rust result notification.
///
/// The Java adapter calls this after XXPermissions stores the result. Its only job is to bump the
/// permission generation; UI that read Permissions::status then recomposes and re-queries the OS.
#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_winia_permissions_WiniaPermissions_nativeOnResult(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
    _request_id: jni::sys::jlong,
    _granted: jni::sys::jboolean,
    _do_not_ask_again: jni::sys::jboolean,
) {
    crate::permissions::note_android_result();
}
