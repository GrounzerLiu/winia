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
    BackgroundLocation,
    Notifications,
    SystemAlertWindow,
    WriteSettings,
    ManageExternalStorage,
    RequestInstallPackages,
    ScheduleExactAlarm,
    UsageStats,
    IgnoreBatteryOptimizations,
    PictureInPicture,
    UseFullScreenIntent,
    AccessNotificationPolicy,
    ManageMedia,
    GetInstalledApps,
}

impl Capability {
    /// Every capability currently exposed by this crate.
    pub const ALL: &'static [Capability] = &[
        Capability::Camera,
        Capability::Microphone,
        Capability::Location,
        Capability::BackgroundLocation,
        Capability::Notifications,
        Capability::SystemAlertWindow,
        Capability::WriteSettings,
        Capability::ManageExternalStorage,
        Capability::RequestInstallPackages,
        Capability::ScheduleExactAlarm,
        Capability::UsageStats,
        Capability::IgnoreBatteryOptimizations,
        Capability::PictureInPicture,
        Capability::UseFullScreenIntent,
        Capability::AccessNotificationPolicy,
        Capability::ManageMedia,
        Capability::GetInstalledApps,
    ];
}

/// A permission that binds an Android Service or Receiver by class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    Accessibility,
    NotificationListener,
    Vpn,
    DeviceAdmin,
}

impl ServiceKind {
    pub(crate) fn adapter_name(self) -> &'static str {
        match self {
            ServiceKind::Accessibility => "accessibility",
            ServiceKind::NotificationListener => "notificationListener",
            ServiceKind::Vpn => "vpn",
            ServiceKind::DeviceAdmin => "deviceAdmin",
        }
    }
}

/// A service-binding permission and the concrete Java class it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServicePermission {
    kind: ServiceKind,
    class: &'static str,
}

impl ServicePermission {
    pub const fn accessibility(class: &'static str) -> Self {
        Self {
            kind: ServiceKind::Accessibility,
            class,
        }
    }

    pub const fn notification_listener(class: &'static str) -> Self {
        Self {
            kind: ServiceKind::NotificationListener,
            class,
        }
    }

    pub const fn vpn(class: &'static str) -> Self {
        Self {
            kind: ServiceKind::Vpn,
            class,
        }
    }

    pub const fn device_admin(class: &'static str) -> Self {
        Self {
            kind: ServiceKind::DeviceAdmin,
            class,
        }
    }

    pub(crate) fn kind(self) -> ServiceKind {
        self.kind
    }

    pub(crate) fn class(self) -> &'static str {
        self.class
    }
}

impl Capability {
    /// Name used by the generated Java adapter. Keep these in sync with WiniaPermissions.java.
    pub(crate) fn adapter_name(self) -> &'static str {
        match self {
            Capability::Camera => "camera",
            Capability::Microphone => "microphone",
            Capability::Location => "location",
            Capability::BackgroundLocation => "backgroundLocation",
            Capability::Notifications => "notifications",
            Capability::SystemAlertWindow => "systemAlertWindow",
            Capability::WriteSettings => "writeSettings",
            Capability::ManageExternalStorage => "manageExternalStorage",
            Capability::RequestInstallPackages => "requestInstallPackages",
            Capability::ScheduleExactAlarm => "scheduleExactAlarm",
            Capability::UsageStats => "usageStats",
            Capability::IgnoreBatteryOptimizations => "ignoreBatteryOptimizations",
        Capability::PictureInPicture => "pictureInPicture",
        Capability::UseFullScreenIntent => "useFullScreenIntent",
        Capability::AccessNotificationPolicy => "accessNotificationPolicy",
        Capability::ManageMedia => "manageMedia",
        Capability::GetInstalledApps => "getInstalledApps",
        }
    }

    /// True when Android exposes this as a special app-access page rather than a dialog.
    pub fn is_special(self) -> bool {
        matches!(
            self,
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
        )
    }
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

impl PermissionStatus {
    /// True when the capability can be used without another prompt.
    pub fn is_allowed(self) -> bool {
        matches!(
            self,
            PermissionStatus::Granted
                | PermissionStatus::Limited
                | PermissionStatus::Provisional
                | PermissionStatus::NotRequired
        )
    }

    /// True when the user explicitly denied the capability.
    pub fn is_denied(self) -> bool {
        matches!(
            self,
            PermissionStatus::Denied | PermissionStatus::PermanentlyDenied
        )
    }
}

/// The kind of user interaction a capability needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    /// The platform can show its normal permission dialog.
    SystemDialog,
    /// The user must enable the capability on a system settings page.
    OpenSettings,
    /// No permission is required on this platform or API level.
    NotRequired,
    /// The current target has no permission backend.
    Unsupported,
}

/// Why a request could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    /// The current target has no permission backend.
    Unsupported,
    /// Android is missing the generated WiniaPermissions bridge.
    AdapterMissing,
    /// The capability is a settings-only special access; use open_settings_for.
    SettingsRequired,
    /// Background location requires foreground location to be granted first.
    ForegroundLocationRequired,
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
    inner: std::sync::Arc<RequestInner>,
    id: u64,
}

#[cfg(target_os = "android")]
struct RequestInner {
    app: winit::platform::android::activity::AndroidApp,
    id: u64,
}

#[cfg(target_os = "android")]
impl Drop for RequestInner {
    fn drop(&mut self) {
        crate::platform::android::permissions::clear_result(&self.app, self.id);
    }
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
        Self {
            inner: std::sync::Arc::new(RequestInner { app, id }),
            id,
        }
    }

    /// Return the result once the platform has delivered it, without blocking.
    pub fn poll(&self) -> Option<PermissionStatus> {
        #[cfg(target_os = "android")]
        {
            return crate::platform::android::permissions::poll_result(&self.inner.app, self.inner.id);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = self.id;
            None
        }
    }

    /// Poll until the result arrives or the timeout expires.
    ///
    /// A timeout returns None; it is not the same as a denied permission. Do not call this from
    /// the UI thread: Android delivers the result on the main looper.
    pub fn wait(&self, timeout: Duration) -> Option<PermissionStatus> {
        #[cfg(target_os = "android")]
        if crate::platform::android::permissions::is_main_thread(&self.inner.app) {
            return None;
        }
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.poll() {
                return Some(status);
            }
            if start.elapsed() >= timeout {
                return None;
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
            if status == PermissionStatus::Denied
                && crate::platform::android::permissions::is_permanently_denied(app, capability)
            {
                return PermissionStatus::PermanentlyDenied;
            }
            return if status == PermissionStatus::Denied
                && !crate::platform::android::permissions::has_requested(app, capability)
            {
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

    /// What kind of interaction this capability needs on the current platform.
    pub fn action(&self, capability: Capability) -> PermissionAction {
        match self.status(capability) {
            PermissionStatus::Unsupported => PermissionAction::Unsupported,
            PermissionStatus::PermanentlyDenied => PermissionAction::OpenSettings,
            PermissionStatus::NotRequired => PermissionAction::NotRequired,
            _ if capability.is_special() => PermissionAction::OpenSettings,
            _ => PermissionAction::SystemDialog,
        }
    }

    /// Open the platform page for one capability, when the app knows which permission failed.
    pub fn open_settings_for(&self, capability: Capability) -> Result<(), PermissionError> {
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            return crate::platform::android::permissions::open_settings_for(app, capability);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = capability;
            Err(PermissionError::Unsupported)
        }
    }

    /// Status of a service-binding permission.
    pub fn service_status(&self, service: ServicePermission) -> PermissionStatus {
        let _ = revision();
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return PermissionStatus::Unsupported;
            };
            return crate::platform::android::permissions::service_status(app, service);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = service;
            PermissionStatus::Unsupported
        }
    }

    /// Start a service-binding permission request.
    pub fn request_service(
        &self,
        service: ServicePermission,
    ) -> Result<PermissionRequest, PermissionError> {
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            return crate::platform::android::permissions::request_service(app, service);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = service;
            Err(PermissionError::Unsupported)
        }
    }

    /// Open the settings page for a service-binding permission.
    pub fn open_settings_for_service(
        &self,
        service: ServicePermission,
    ) -> Result<(), PermissionError> {
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            return crate::platform::android::permissions::open_settings_for_service(app, service);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = service;
            Err(PermissionError::Unsupported)
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
        if capability.is_special() {
            return Err(PermissionError::SettingsRequired);
        }
        #[cfg(target_os = "android")]
        if matches!(capability, Capability::BackgroundLocation) {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            if crate::platform::android::permissions::status(app, Capability::Location)
                != PermissionStatus::Granted
            {
                return Err(PermissionError::ForegroundLocationRequired);
            }
        }
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            return crate::platform::android::permissions::request(app, capability);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = capability;
            Err(PermissionError::Unsupported)
        }
    }
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
    fn capabilities_are_classified_for_the_adapter() {
        assert_eq!(Capability::Camera.adapter_name(), "camera");
        assert!(!Capability::Camera.is_special());
        assert!(Capability::SystemAlertWindow.is_special());
        assert!(Capability::ManageExternalStorage.is_special());
        assert!(!Capability::Location.is_special());
        assert!(!Capability::BackgroundLocation.is_special());
        assert!(!Capability::GetInstalledApps.is_special());
        assert_eq!(Capability::ALL.len(), 17);
        assert!(PermissionStatus::Granted.is_allowed());
        assert!(PermissionStatus::NotRequired.is_allowed());
        assert!(PermissionStatus::Denied.is_denied());
        assert!(!PermissionStatus::Denied.is_allowed());
    }

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
            permissions
                .request(Capability::SystemAlertWindow)
                .unwrap_err(),
            PermissionError::SettingsRequired
        );
        assert_eq!(
            permissions.open_settings().unwrap_err(),
            PermissionError::Unsupported
        );
        assert_eq!(
            permissions.action(Capability::Camera),
            PermissionAction::Unsupported
        );
        let service = ServicePermission::accessibility("com.example.Accessibility");
        assert_eq!(service.kind(), ServiceKind::Accessibility);
        assert_eq!(service.class(), "com.example.Accessibility");
        assert_eq!(
            permissions.service_status(service),
            PermissionStatus::Unsupported
        );
        assert_eq!(
            permissions.request_service(service).unwrap_err(),
            PermissionError::Unsupported
        );
    }
}
