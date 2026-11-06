//! JNI access for Android.
//!
//! The activity pointer is a global reference owned by `AndroidApp`: do not wrap it in `Global`
//! or `Auto`, and do not use it after the activity has been destroyed.

use winit::platform::android::activity::AndroidApp;

/// JNI access for an Android activity.
pub struct AndroidJni<'a> {
    app: &'a AndroidApp,
}

impl<'a> AndroidJni<'a> {
    pub(crate) fn new(app: &'a AndroidApp) -> Self {
        Self { app }
    }

    /// The Java VM that owns this activity.
    pub fn java_vm(&self) -> jni::JavaVM {
        // SAFETY: AndroidApp owns the VM handle for as long as this AndroidJni is valid.
        unsafe { jni::JavaVM::from_raw(self.app.vm_as_ptr().cast()) }
    }

    /// Attach the current thread and run a JNI closure with an Env.
    pub fn with_env<F, T>(&self, f: F) -> jni::errors::Result<T>
    where
        F: FnOnce(&mut jni::Env<'_>) -> jni::errors::Result<T>,
    {
        self.java_vm().attach_current_thread(f)
    }

    /// The Activity's JNI global reference.
    ///
    /// The reference is owned by AndroidApp; do not delete it.
    pub fn activity_ptr(&self) -> jni::sys::jobject {
        self.app.activity_as_ptr() as jni::sys::jobject
    }
}
