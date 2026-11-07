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
    Capability, PermissionError, PermissionRequest, PermissionStatus, ServicePermission,
    reconcile_result,
};
use crate::platform::android::AndroidJni;

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn adapter_present(app: &AndroidApp) -> bool {
    AndroidJni::new(app)
        .with_env(|env| {
            env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
            Ok(())
        })
        .is_ok()
}

fn adapter_error(app: &AndroidApp, error: jni::errors::Error) -> PermissionError {
    if adapter_present(app) {
        PermissionError::Jni(format!("{error:?}"))
    } else {
        PermissionError::AdapterMissing
    }
}

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
    if capability.is_special() || matches!(capability, Capability::GetInstalledApps) {
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
            let state = env
                .call_method(
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

pub(crate) fn is_main_thread(app: &AndroidApp) -> bool {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        env.call_static_method(class, jni_str!("isMainThread"), jni_sig!("()Z"), &[])?
            .z()
    });
    // If the adapter cannot be queried, assume the caller may be on the main thread and refuse
    // to block; a missing adapter cannot deliver a result anyway.
    result.unwrap_or(true)
}

pub(crate) fn has_requested(app: &AndroidApp, capability: Capability) -> bool {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let name = env.new_string(capability.adapter_name())?;
        env.call_static_method(
            class,
            jni_str!("hasRequested"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)Z"),
            &[JValue::Object(&activity), JValue::Object(&name)],
        )?
        .z()
    });
    result.unwrap_or(false)
}

pub(crate) fn is_permanently_denied(app: &AndroidApp, capability: Capability) -> bool {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let name = env.new_string(capability.adapter_name())?;
        env.call_static_method(
            class,
            jni_str!("isPermanentlyDenied"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)Z"),
            &[JValue::Object(&activity), JValue::Object(&name)],
        )?
        .z()
    });
    result.unwrap_or(false)
}

pub(crate) fn service_status(app: &AndroidApp, service: ServicePermission) -> PermissionStatus {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let kind = env.new_string(service.kind().adapter_name())?;
        let class_name = env.new_string(service.class())?;
        env.call_static_method(
            class,
            jni_str!("isServiceGranted"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Z"),
            &[
                JValue::Object(&activity),
                JValue::Object(&kind),
                JValue::Object(&class_name),
            ],
        )?
        .z()
    });
    match result {
        Ok(true) => PermissionStatus::Granted,
        Ok(false) => PermissionStatus::Denied,
        Err(_) => PermissionStatus::Unsupported,
    }
}

pub(crate) fn request_service(
    app: &AndroidApp,
    service: ServicePermission,
) -> Result<PermissionRequest, PermissionError> {
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let jni = AndroidJni::new(app);
    let called = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let kind = env.new_string(service.kind().adapter_name())?;
        let class_name = env.new_string(service.class())?;
        env.call_static_method(
            class,
            jni_str!("requestService"),
            jni_sig!("(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;)V"),
            &[
                JValue::Object(&activity),
                JValue::Long(id as i64),
                JValue::Object(&kind),
                JValue::Object(&class_name),
            ],
        )?;
        Ok(())
    });
    match called {
        Ok(()) => Ok(PermissionRequest::new(app.clone(), id)),
        Err(error) => Err(adapter_error(app, error)),
    }
}

pub(crate) fn open_settings_for_service(
    app: &AndroidApp,
    service: ServicePermission,
) -> Result<(), PermissionError> {
    let jni = AndroidJni::new(app);
    let called = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let kind = env.new_string(service.kind().adapter_name())?;
        let class_name = env.new_string(service.class())?;
        env.call_static_method(
            class,
            jni_str!("openSettingsForService"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)V"),
            &[
                JValue::Object(&activity),
                JValue::Object(&kind),
                JValue::Object(&class_name),
            ],
        )?;
        Ok(())
    });
    called.map_err(|error| adapter_error(app, error))
}

pub(crate) fn request(
    app: &AndroidApp,
    capability: Capability,
) -> Result<PermissionRequest, PermissionError> {
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
        Err(error) => Err(adapter_error(app, error)),
    }
}

fn open_settings_call(
    app: &AndroidApp,
    method: &str,
    capability: Option<Capability>,
) -> Result<(), PermissionError> {
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
    called.map_err(|error| adapter_error(app, error))
}

pub(crate) fn open_settings(app: &AndroidApp) -> Result<(), PermissionError> {
    open_settings_call(app, "openSettings", None)
}

pub(crate) fn open_settings_for(
    app: &AndroidApp,
    capability: Capability,
) -> Result<(), PermissionError> {
    open_settings_call(app, "openSettingsFor", Some(capability))
}

pub(crate) fn has_declared_permission(
    app: &AndroidApp,
    capability: Capability,
) -> Result<bool, PermissionError> {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let name = env.new_string(capability.adapter_name())?;
        env.call_static_method(
            class,
            jni_str!("hasDeclaredPermission"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)Z"),
            &[JValue::Object(&activity), JValue::Object(&name)],
        )?
        .z()
    });
    result.map_err(|error| adapter_error(app, error))
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
    .map_err(|error| adapter_error(app, error))
}

pub(crate) fn cancel_all_if_destroyed(app: &AndroidApp) -> bool {
    let jni = AndroidJni::new(app);
    let result = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        let activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        env.call_static_method(
            class,
            jni_str!("cancelAllIfDestroyed"),
            jni_sig!("(Landroid/app/Activity;)Z"),
            &[JValue::Object(&activity)],
        )?
        .z()
    });
    result.unwrap_or(false)
}

pub(crate) fn clear_result(app: &AndroidApp, id: u64) {
    let jni = AndroidJni::new(app);
    let _ = jni.with_env(|env| {
        let class = env.load_class(jni_str!("com.winia.permissions.WiniaPermissions"))?;
        env.call_static_method(
            class,
            jni_str!("clearResult"),
            jni_sig!("(J)V"),
            &[JValue::Long(id as i64)],
        )?;
        Ok(())
    });
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
