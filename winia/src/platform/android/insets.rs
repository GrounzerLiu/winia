//! Read Android system insets through JNI.
//!
//! This is the platform input side of WindowInsets: it converts Android's pixel values into
//! logical pixels and records visibility separately from size.

use jni::objects::JObject;
use jni::{JValue, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

use crate::window::insets::{Insets, InsetsKind, WindowInsets};

#[derive(Debug, Default, Clone, Copy)]
struct InsetsPx {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    visible: bool,
}

/// Read the activity's current system insets.
///
/// Returns a default (zero) value when the window has no root insets yet. Android API 30+ uses
/// WindowInsets.getInsets; API 26-29 falls back to the deprecated system-window inset getters.
pub(crate) fn read(app: &AndroidApp) -> jni::errors::Result<WindowInsets> {
    let sdk = AndroidApp::sdk_version();
    let density = app.config().density().unwrap_or(160) as f32;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr() as jni::sys::jobject;

    vm.attach_current_thread(|env| {
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let window = env
            .call_method(
                &*activity,
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )?
            .l()?;
        let decor = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )?
            .l()?;
        let root = env
            .call_method(
                &decor,
                jni_str!("getRootWindowInsets"),
                jni_sig!("()Landroid/view/WindowInsets;"),
                &[],
            )?
            .l()?;

        if root.as_raw().is_null() {
            return Ok(WindowInsets::default());
        }

        if sdk >= 30 {
            read_modern(env, &root, density)
        } else {
            read_legacy(env, &root, density)
        }
    })
}

fn read_modern(
    env: &mut jni::Env<'_>,
    root: &JObject<'_>,
    density: f32,
) -> jni::errors::Result<WindowInsets> {
    let types = env.find_class(jni_str!("android/view/WindowInsets$Type"))?;
    let status = type_id(env, &types, jni_str!("statusBars"))?;
    let navigation = type_id(env, &types, jni_str!("navigationBars"))?;
    let ime = type_id(env, &types, jni_str!("ime"))?;
    let cutout = type_id(env, &types, jni_str!("displayCutout"))?;
    let gestures = type_id(env, &types, jni_str!("systemGestures"))?;
    let tappable = type_id(env, &types, jni_str!("tappableElement"))?;

    let status = read_inset(env, root, status)?;
    let navigation = read_inset(env, root, navigation)?;
    let ime = read_inset(env, root, ime)?;
    let cutout = read_inset(env, root, cutout)?;
    let gestures = read_inset(env, root, gestures)?;
    let tappable = read_inset(env, root, tappable)?;

    Ok(build(
        status, navigation, ime, cutout, gestures, tappable, density,
    ))
}

fn read_legacy(
    env: &mut jni::Env<'_>,
    root: &JObject<'_>,
    density: f32,
) -> jni::errors::Result<WindowInsets> {
    let left = int_method(env, root, jni_str!("getSystemWindowInsetLeft"))?;
    let top = int_method(env, root, jni_str!("getSystemWindowInsetTop"))?;
    let right = int_method(env, root, jni_str!("getSystemWindowInsetRight"))?;
    let bottom = int_method(env, root, jni_str!("getSystemWindowInsetBottom"))?;
    let stable_left = int_method(env, root, jni_str!("getStableInsetLeft"))?;
    let stable_right = int_method(env, root, jni_str!("getStableInsetRight"))?;
    let stable_bottom = int_method(env, root, jni_str!("getStableInsetBottom"))?;

    let status = InsetsPx {
        left,
        top,
        right,
        visible: top > 0 || left > 0 || right > 0,
        ..Default::default()
    };
    let navigation = InsetsPx {
        left: stable_left,
        right: stable_right,
        bottom: stable_bottom,
        visible: stable_bottom > 0 || stable_left > 0 || stable_right > 0,
        ..Default::default()
    };
    let ime = InsetsPx {
        bottom: (bottom - stable_bottom).max(0),
        visible: bottom > stable_bottom,
        ..Default::default()
    };

    Ok(build(
        status,
        navigation,
        ime,
        InsetsPx::default(),
        InsetsPx::default(),
        InsetsPx::default(),
        density,
    ))
}

fn type_id(
    env: &mut jni::Env<'_>,
    types: &jni::objects::JClass<'_>,
    name: &jni::strings::JNIStr,
) -> jni::errors::Result<i32> {
    env.call_static_method(types, name, jni_sig!("()I"), &[])?
        .i()
}

fn read_inset(
    env: &mut jni::Env<'_>,
    root: &JObject<'_>,
    kind: i32,
) -> jni::errors::Result<InsetsPx> {
    let insets = env
        .call_method(
            root,
            jni_str!("getInsets"),
            jni_sig!("(I)Landroid/graphics/Insets;"),
            &[JValue::Int(kind)],
        )?
        .l()?;
    let visible = env
        .call_method(
            root,
            jni_str!("isVisible"),
            jni_sig!("(I)Z"),
            &[JValue::Int(kind)],
        )?
        .z()?;

    Ok(InsetsPx {
        left: field_i32(env, &insets, jni_str!("left"))?,
        top: field_i32(env, &insets, jni_str!("top"))?,
        right: field_i32(env, &insets, jni_str!("right"))?,
        bottom: field_i32(env, &insets, jni_str!("bottom"))?,
        visible,
    })
}

fn field_i32(
    env: &mut jni::Env<'_>,
    object: &JObject<'_>,
    name: &jni::strings::JNIStr,
) -> jni::errors::Result<i32> {
    env.get_field(object, name, jni_sig!("I"))?.i()
}

fn int_method(
    env: &mut jni::Env<'_>,
    object: &JObject<'_>,
    name: &jni::strings::JNIStr,
) -> jni::errors::Result<i32> {
    env.call_method(object, name, jni_sig!("()I"), &[])?.i()
}

fn build(
    status: InsetsPx,
    navigation: InsetsPx,
    ime: InsetsPx,
    cutout: InsetsPx,
    gestures: InsetsPx,
    tappable: InsetsPx,
    density: f32,
) -> WindowInsets {
    let scale = 160.0 / density;
    let convert = |x: InsetsPx| {
        Insets::new(
            x.left as f32 * scale,
            x.top as f32 * scale,
            x.right as f32 * scale,
            x.bottom as f32 * scale,
        )
    };

    WindowInsets {
        status_bars: convert(status),
        navigation_bars: convert(navigation),
        ime: convert(ime),
        display_cutout: convert(cutout),
        system_gestures: convert(gestures),
        tappable_elements: convert(tappable),
        visibility: Default::default(),
    }
    .with_visibility(InsetsKind::StatusBars, status.visible)
    .with_visibility(InsetsKind::NavigationBars, navigation.visible)
    .with_visibility(InsetsKind::Ime, ime.visible)
    .with_visibility(InsetsKind::DisplayCutout, cutout.visible)
    .with_visibility(InsetsKind::SystemGestures, gestures.visible)
    .with_visibility(InsetsKind::TappableElements, tappable.visible)
}
