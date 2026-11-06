//! Runtime permission state and requests.
//!
//! Android is the only backend in this first slice. Project declarations stay in
//! AndroidManifest.xml; this module only exposes capability queries and requests.
//! A project without the generated Java adapter (for example an older project) reports
//! AdapterMissing instead of pretending the request succeeded.

use std::time::Duration;

/// A permission capability the application can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Camera,
    Microphone,
    Location,
    Notifications,
}

/// The platform's current answer for a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    NotDetermined,
    Granted,
    Denied,
    PermanentlyDenied,
    Restricted,
    Limited,
    Provisional,
    NotRequired,
    Unsupported,
}

/// Why a request could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    /// The current target has no permission backend.
    Unsupported,
    /// Android is missing the generated WiniaPermissions bridge.
    AdapterMissing,
    /// A JNI call failed.
    Jni(String),
}

/// A handle for one in-flight Android permission request.
///
/// The Java adapter records the result and the UI calls poll (or wait) to consume it. This keeps
/// the result bridge independent of Activity callbacks and makes the request lifetime explicit.
#[derive(Clone)]
pub struct PermissionRequest {
    #[cfg(target_os = "android")]
    app: winit::platform::android::activity::AndroidApp,
    id: u64,
}

impl std::fmt::Debug for PermissionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionRequest").field("id", &self.id).finish()
    }
}

impl PartialEq for PermissionRequest {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for PermissionRequest {}

impl PermissionRequest {
    #[cfg(target_os = "android")]
    pub(crate) fn new(app: winit::platform::android::activity::AndroidApp, id: u64) -> Self {
        Self { app, id }
    }

    /// Return the result once the platform has delivered it, without blocking.
    pub fn poll(&self) -> Option<PermissionStatus> {
        #[cfg(target_os = "android")]
        {
            return crate::platform::android::permissions::poll_result(&self.app, self.id);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = self.id;
            None
        }
    }

    /// Poll until the result arrives or the timeout expires.
    ///
    /// A timeout returns Denied because the request was not granted; callers that need to
    /// distinguish a timeout should use poll and their own deadline.
    pub fn wait(&self, timeout: Duration) -> PermissionStatus {
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.poll() {
                return status;
            }
            if start.elapsed() >= timeout {
                return PermissionStatus::Denied;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Permission facade tied to the current composition.
pub struct Permissions<'a> {
    #[cfg(target_os = "android")]
    app: Option<&'a winit::platform::android::activity::AndroidApp>,
    #[cfg(not(target_os = "android"))]
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Permissions<'a> {
    #[cfg(target_os = "android")]
    pub(crate) fn new(app: Option<&'a winit::platform::android::activity::AndroidApp>) -> Self {
        Self { app }
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// The current status of a capability.
    pub fn status(&self, capability: Capability) -> PermissionStatus {
        let _ = revision();
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return PermissionStatus::Unsupported;
            };
            let status = crate::platform::android::permissions::status(app, capability);
            return if status == PermissionStatus::Denied && !was_requested(capability) {
                PermissionStatus::NotDetermined
            } else {
                status
            };
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = capability;
            PermissionStatus::Unsupported
        }
    }

    /// Open the platform page where a denied permission can be changed.
    pub fn open_settings(&self) -> Result<(), PermissionError> {
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            return crate::platform::android::permissions::open_settings(app);
        }
        #[cfg(not(target_os = "android"))]
        {
            Err(PermissionError::Unsupported)
        }
    }

    /// Start a permission request on the Android UI thread.
    pub fn request(&self, capability: Capability) -> Result<PermissionRequest, PermissionError> {
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            let request = crate::platform::android::permissions::request(app, capability);
            if request.is_ok() {
                mark_requested(capability);
            }
            return request;
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = capability;
            Err(PermissionError::Unsupported)
        }
    }
}

static REQUESTED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn requested_bit(capability: Capability) -> u32 {
    match capability {
        Capability::Camera => 1,
        Capability::Microphone => 2,
        Capability::Location => 4,
        Capability::Notifications => 8,
    }
}

fn mark_requested(capability: Capability) {
    REQUESTED.fetch_or(requested_bit(capability), std::sync::atomic::Ordering::Relaxed);
}

fn was_requested(capability: Capability) -> bool {
    REQUESTED.load(std::sync::atomic::Ordering::Relaxed) & requested_bit(capability) != 0
}

static REVISION: std::sync::LazyLock<crate::runtime::state::Reactive<u64>> =
    std::sync::LazyLock::new(|| crate::runtime::state::Reactive::new(0));

/// Read the permission generation. A UI query through Permissions::status depends on this
/// reactive value, so the native result callback can wake the composition that displays it.
fn revision() -> u64 {
    REVISION.get()
}

pub(crate) fn note_android_result() {
    let next = revision().wrapping_add(1);
    REVISION.set(next);
}

pub(crate) fn reconcile_result(value: i32) -> Option<PermissionStatus> {
    match value {
        0 => Some(PermissionStatus::Denied),
        1 => Some(PermissionStatus::Granted),
        2 => Some(PermissionStatus::PermanentlyDenied),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_codes_map_to_permission_states() {
        assert_eq!(reconcile_result(-1), None);
        assert_eq!(reconcile_result(0), Some(PermissionStatus::Denied));
        assert_eq!(reconcile_result(1), Some(PermissionStatus::Granted));
        assert_eq!(
            reconcile_result(2),
            Some(PermissionStatus::PermanentlyDenied)
        );
    }

    #[test]
    fn desktop_permissions_are_unsupported() {
        let permissions = Permissions::new();
        assert_eq!(
            permissions.status(Capability::Camera),
            PermissionStatus::Unsupported
        );
        assert_eq!(
            permissions.request(Capability::Camera).unwrap_err(),
            PermissionError::Unsupported
        );
        assert_eq!(
            permissions.open_settings().unwrap_err(),
            PermissionError::Unsupported
        );
    }
}
