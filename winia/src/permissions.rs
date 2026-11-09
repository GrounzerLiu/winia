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
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
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

    /// VPN permission is not tied to a concrete service class in XXPermissions.
    pub const fn vpn() -> Self {
        Self {
            kind: ServiceKind::Vpn,
            class: "",
        }
    }

    pub const fn device_admin(class: &'static str) -> Self {
        Self {
            kind: ServiceKind::DeviceAdmin,
            class,
        }
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn kind(self) -> ServiceKind {
        self.kind
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn class(self) -> &'static str {
        self.class
    }
}

impl Capability {
    /// Name used by the generated Java adapter. Keep these in sync with WiniaPermissions.java.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
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

    /// True when the capability needs a declaration in the application manifest.
    ///
    /// Most special app-access permissions still require a manifest entry. Picture-in-picture and
    /// media management are intentionally excluded because their manifest requirements vary by API
    /// level and should not produce false ConfigureBuild results.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn needs_manifest_declaration(self) -> bool {
        !self.is_special()
            || matches!(
                self,
                Capability::SystemAlertWindow
                    | Capability::WriteSettings
                    | Capability::ManageExternalStorage
                    | Capability::RequestInstallPackages
                    | Capability::ScheduleExactAlarm
                    | Capability::UsageStats
                    | Capability::IgnoreBatteryOptimizations
                    | Capability::UseFullScreenIntent
                    | Capability::AccessNotificationPolicy
            )
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
    Cancelled,
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
    /// Another capability has to be granted first; requesting this one now would fail.
    ///
    /// Android only reports background location after foreground location is granted, so
    /// `action(BackgroundLocation)` returns this until `status(Location)` is allowed. Request
    /// `Capability::Location` first and re-check this action afterwards.
    Prerequisite,
    /// The manifest or build configuration is missing the required declaration.
    ConfigureBuild,
    /// No permission is required on this platform or API level.
    NotRequired,
    /// The current target has no permission backend.
    Unsupported,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn normalize_status(
    status: PermissionStatus,
    requested: bool,
    permanently_denied: bool,
) -> PermissionStatus {
    if status != PermissionStatus::Denied {
        return status;
    }
    if permanently_denied {
        PermissionStatus::PermanentlyDenied
    } else if requested {
        PermissionStatus::Denied
    } else {
        PermissionStatus::NotDetermined
    }
}

fn action_for(
    status: PermissionStatus,
    special: bool,
    missing_declaration: bool,
    prerequisite_missing: bool,
) -> PermissionAction {
    if missing_declaration {
        return PermissionAction::ConfigureBuild;
    }
    match status {
        PermissionStatus::Unsupported => PermissionAction::Unsupported,
        PermissionStatus::NotRequired => PermissionAction::NotRequired,
        // Checked before PermanentlyDenied on purpose: request() refuses this capability while
        // the prerequisite is missing, so pointing at the settings page would not help yet.
        _ if prerequisite_missing => PermissionAction::Prerequisite,
        PermissionStatus::PermanentlyDenied => PermissionAction::OpenSettings,
        PermissionStatus::Cancelled => PermissionAction::SystemDialog,
        _ if special => PermissionAction::OpenSettings,
        _ => PermissionAction::SystemDialog,
    }
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
    /// The AndroidManifest does not declare the permission for this capability.
    MissingDeclaration,
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

/// What a poll already knows about one request.
///
/// The Java adapter hands a result out once (`takeResult` removes it), so the status has to be
/// remembered: without that, a second poll reports "no result yet" and a granted permission looks
/// like it went back to NotDetermined on the next frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
enum PollState {
    /// The adapter has not delivered a result yet.
    Pending,
    /// Delivered by the adapter; every later poll reports it again.
    Ready(PermissionStatus),
    /// `cancel()` discarded the request, so polls have nothing left to report.
    Discarded,
}

/// Apply one poll to the remembered state.
///
/// Split from the handle so the caching rule stays testable without an Activity.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn advance_poll(
    state: PollState,
    generation_matches: bool,
    take_latest: impl FnOnce() -> Option<PermissionStatus>,
) -> (PollState, Option<PermissionStatus>) {
    match state {
        PollState::Ready(status) => (state, Some(status)),
        PollState::Discarded => (state, None),
        // A destroyed Activity invalidates the handle before a result can arrive.
        PollState::Pending if !generation_matches => (
            PollState::Ready(PermissionStatus::Cancelled),
            Some(PermissionStatus::Cancelled),
        ),
        PollState::Pending => match take_latest() {
            Some(status) => (PollState::Ready(status), Some(status)),
            None => (PollState::Pending, None),
        },
    }
}

#[cfg(target_os = "android")]
struct RequestInner {
    app: winit::platform::android::activity::AndroidApp,
    id: u64,
    generation: u64,
    state: std::sync::Mutex<PollState>,
}

#[cfg(target_os = "android")]
impl RequestInner {
    fn state(&self) -> PollState {
        *self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn set_state(&self, next: PollState) {
        *self.state.lock().unwrap_or_else(|error| error.into_inner()) = next;
    }
}

#[cfg(target_os = "android")]
impl Drop for RequestInner {
    fn drop(&mut self) {
        crate::platform::android::permissions::clear_result(&self.app, self.id);
    }
}

impl std::fmt::Debug for PermissionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionRequest")
            .field("id", &self.id)
            .finish()
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
            inner: std::sync::Arc::new(RequestInner {
                app,
                id,
                generation: request_generation(),
                state: std::sync::Mutex::new(PollState::Pending),
            }),
            id,
        }
    }

    /// Return the result once the platform has delivered it, without blocking.
    ///
    /// The first poll that sees a result remembers it, so polling again (for example on every
    /// frame) keeps reporting the same status instead of falling back to None.
    pub fn poll(&self) -> Option<PermissionStatus> {
        #[cfg(target_os = "android")]
        {
            let generation_matches = self.inner.generation == request_generation();
            let (next, status) = advance_poll(self.inner.state(), generation_matches, || {
                crate::platform::android::permissions::poll_result(&self.inner.app, self.inner.id)
            });
            self.inner.set_state(next);
            return status;
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = self.id;
            None
        }
    }

    /// Cancel this request and discard its result.
    ///
    /// A queued request is removed immediately. An active request is marked cancelled without
    /// dismissing the system dialog; the queue advances only when XXPermissions returns. The handle
    /// stays usable but polling returns None because the result was discarded.
    pub fn cancel(&self) {
        #[cfg(target_os = "android")]
        {
            self.inner.set_state(PollState::Discarded);
            crate::platform::android::permissions::clear_result(&self.inner.app, self.inner.id);
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
            let requested = crate::platform::android::permissions::has_requested(app, capability);
            let permanent =
                crate::platform::android::permissions::is_permanently_denied(app, capability);
            return normalize_status(status, requested, permanent);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = capability;
            PermissionStatus::Unsupported
        }
    }

    /// What kind of interaction this capability needs on the current platform.
    pub fn action(&self, capability: Capability) -> PermissionAction {
        let status = self.status(capability);
        #[cfg(target_os = "android")]
        let mut missing_declaration = false;
        #[cfg(not(target_os = "android"))]
        let missing_declaration = false;
        #[cfg(target_os = "android")]
        let mut prerequisite_missing = false;
        #[cfg(not(target_os = "android"))]
        let prerequisite_missing = false;
        #[cfg(target_os = "android")]
        if matches!(capability, Capability::BackgroundLocation)
            && !matches!(
                status,
                PermissionStatus::Unsupported | PermissionStatus::NotRequired
            )
            // Same rule request() enforces: background location is refused unless foreground
            // location is granted, so the action has to report that instead of a dialog.
            && self.status(Capability::Location) != PermissionStatus::Granted
        {
            prerequisite_missing = true;
        }
        #[cfg(target_os = "android")]
        if capability.needs_manifest_declaration()
            && !matches!(
                status,
                PermissionStatus::Unsupported | PermissionStatus::NotRequired
            )
            && let Some(app) = self.app
        {
            match crate::platform::android::permissions::has_declared_permission(app, capability) {
                Ok(true) => {}
                Ok(false) => missing_declaration = true,
                Err(PermissionError::AdapterMissing) => return PermissionAction::ConfigureBuild,
                Err(_) => return PermissionAction::Unsupported,
            }
        }
        action_for(
            status,
            capability.is_special(),
            missing_declaration,
            prerequisite_missing,
        )
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

    /// What kind of interaction a service-binding permission needs.
    pub fn service_action(&self, service: ServicePermission) -> PermissionAction {
        #[cfg(target_os = "android")]
        {
            if let Some(app) = self.app {
                match crate::platform::android::permissions::has_declared_service(app, service) {
                    Ok(true) => {}
                    // Without the component in the manifest the settings page itself cannot show
                    // the service, so the fix is a build change, not navigation.
                    Ok(false) => return PermissionAction::ConfigureBuild,
                    Err(PermissionError::AdapterMissing) => {
                        return PermissionAction::ConfigureBuild;
                    }
                    Err(_) => return PermissionAction::Unsupported,
                }
            }
        }
        match self.service_status(service) {
            PermissionStatus::Unsupported => PermissionAction::Unsupported,
            PermissionStatus::NotRequired => PermissionAction::NotRequired,
            _ => PermissionAction::OpenSettings,
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

    /// Start the settings-page flow for a service-binding permission.
    ///
    /// Service permissions do not have a normal runtime dialog; `service_action` reports
    /// `OpenSettings` for them. `open_settings_for_service` is the explicit alternative when the
    /// caller only wants to navigate, not observe a request result.
    ///
    /// The manifest is checked first: a missing component would otherwise surface as an exception
    /// on the main looper instead of an error the caller can act on.
    pub fn request_service(
        &self,
        service: ServicePermission,
    ) -> Result<PermissionRequest, PermissionError> {
        #[cfg(target_os = "android")]
        {
            let Some(app) = self.app else {
                return Err(PermissionError::Unsupported);
            };
            match crate::platform::android::permissions::has_declared_service(app, service) {
                Ok(true) => {}
                Ok(false) => return Err(PermissionError::MissingDeclaration),
                Err(error) => return Err(error),
            }
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
        #[cfg(target_os = "android")]
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
            match crate::platform::android::permissions::has_declared_permission(app, capability) {
                Ok(true) => {}
                Ok(false) => return Err(PermissionError::MissingDeclaration),
                Err(error) => return Err(error),
            }
            return crate::platform::android::permissions::request(app, capability);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = capability;
            Err(PermissionError::Unsupported)
        }
    }
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
static REQUEST_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn request_generation() -> u64 {
    REQUEST_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_os = "android")]
pub(crate) fn invalidate_pending_requests(app: &winit::platform::android::activity::AndroidApp) {
    if crate::platform::android::permissions::cancel_all_if_destroyed(app) {
        REQUEST_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        note_android_result();
    }
}

static REVISION: std::sync::LazyLock<crate::runtime::state::Reactive<u64>> =
    std::sync::LazyLock::new(|| crate::runtime::state::Reactive::new(0));

/// Read the permission generation. A UI query through Permissions::status depends on this
/// reactive value, so the native result callback can wake the composition that displays it.
fn revision() -> u64 {
    REVISION.get()
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn note_android_result() {
    let next = revision().wrapping_add(1);
    REVISION.set(next);
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
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
        assert!(Capability::SystemAlertWindow.needs_manifest_declaration());
        assert!(!Capability::PictureInPicture.needs_manifest_declaration());
        assert!(Capability::ManageExternalStorage.is_special());
        assert!(Capability::ManageExternalStorage.needs_manifest_declaration());
        assert!(!Capability::Location.is_special());
        assert!(!Capability::BackgroundLocation.is_special());
        assert!(!Capability::GetInstalledApps.is_special());
        assert_eq!(Capability::ALL.len(), 17);
        let names: std::collections::HashSet<&str> = Capability::ALL
            .iter()
            .map(|cap| cap.adapter_name())
            .collect();
        assert_eq!(names.len(), Capability::ALL.len());
        assert!(PermissionStatus::Granted.is_allowed());
        assert!(PermissionStatus::NotRequired.is_allowed());
        assert!(PermissionStatus::Denied.is_denied());
        assert!(!PermissionStatus::Denied.is_allowed());
    }

    #[test]
    fn actions_follow_permission_status() {
        assert_eq!(
            action_for(PermissionStatus::PermanentlyDenied, false, false, false),
            PermissionAction::OpenSettings
        );
        assert_eq!(
            action_for(PermissionStatus::NotRequired, false, false, false),
            PermissionAction::NotRequired
        );
        assert_eq!(
            action_for(PermissionStatus::Denied, true, false, false),
            PermissionAction::OpenSettings
        );
        assert_eq!(
            action_for(PermissionStatus::Denied, false, false, false),
            PermissionAction::SystemDialog
        );
        assert_eq!(
            action_for(PermissionStatus::Cancelled, false, false, false),
            PermissionAction::SystemDialog
        );
        assert_eq!(
            action_for(PermissionStatus::Unsupported, false, false, false),
            PermissionAction::Unsupported
        );
        assert_eq!(
            action_for(PermissionStatus::Granted, false, true, false),
            PermissionAction::ConfigureBuild
        );
    }

    #[test]
    fn a_missing_prerequisite_wins_over_the_remaining_actions() {
        // request(BackgroundLocation) refuses while foreground location is not allowed, so the
        // action must not point at a dialog or the settings page.
        assert_eq!(
            action_for(PermissionStatus::Denied, false, false, true),
            PermissionAction::Prerequisite
        );
        assert_eq!(
            action_for(PermissionStatus::PermanentlyDenied, false, false, true),
            PermissionAction::Prerequisite
        );
        // A missing manifest declaration is still the more actionable answer.
        assert_eq!(
            action_for(PermissionStatus::Denied, false, true, true),
            PermissionAction::ConfigureBuild
        );
        // Capabilities without a backend or without a need stay as they are.
        assert_eq!(
            action_for(PermissionStatus::Unsupported, false, false, true),
            PermissionAction::Unsupported
        );
        assert_eq!(
            action_for(PermissionStatus::NotRequired, false, false, true),
            PermissionAction::NotRequired
        );
    }

    #[test]
    fn status_normalization_distinguishes_permission_history() {
        assert_eq!(
            normalize_status(PermissionStatus::Denied, false, false),
            PermissionStatus::NotDetermined
        );
        assert_eq!(
            normalize_status(PermissionStatus::Denied, true, false),
            PermissionStatus::Denied
        );
        assert_eq!(
            normalize_status(PermissionStatus::Denied, true, true),
            PermissionStatus::PermanentlyDenied
        );
        assert_eq!(
            normalize_status(PermissionStatus::Granted, false, false),
            PermissionStatus::Granted
        );
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
    fn polls_remember_a_delivered_result() {
        // The first poll takes the result out of the adapter.
        let (state, status) =
            advance_poll(PollState::Pending, true, || Some(PermissionStatus::Granted));
        assert_eq!(status, Some(PermissionStatus::Granted));
        assert_eq!(state, PollState::Ready(PermissionStatus::Granted));

        // Later polls replay it instead of asking the adapter again, which would report None.
        let (state, status) = advance_poll(state, true, || panic!("adapter polled twice"));
        assert_eq!(status, Some(PermissionStatus::Granted));
        assert_eq!(state, PollState::Ready(PermissionStatus::Granted));
    }

    #[test]
    fn polls_report_nothing_until_the_adapter_delivers() {
        let (state, status) = advance_poll(PollState::Pending, true, || None);
        assert_eq!(status, None);
        assert_eq!(state, PollState::Pending);
    }

    #[test]
    fn a_new_generation_reports_cancelled_once_and_then_repeats_it() {
        let (state, status) = advance_poll(PollState::Pending, false, || {
            panic!("adapter polled after teardown")
        });
        assert_eq!(status, Some(PermissionStatus::Cancelled));
        assert_eq!(state, PollState::Ready(PermissionStatus::Cancelled));

        let (state, status) = advance_poll(state, false, || panic!("adapter polled twice"));
        assert_eq!(status, Some(PermissionStatus::Cancelled));
        assert_eq!(state, PollState::Ready(PermissionStatus::Cancelled));
    }

    #[test]
    fn polls_stay_silent_after_cancel() {
        let (state, status) = advance_poll(PollState::Discarded, true, || {
            panic!("adapter polled after cancel")
        });
        assert_eq!(status, None);
        assert_eq!(state, PollState::Discarded);
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
            PermissionError::Unsupported
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
        assert_eq!(
            permissions.service_action(service),
            PermissionAction::Unsupported
        );
    }

    #[test]
    fn a_service_without_a_component_is_only_checked_on_android() {
        // VPN has no service class of its own, so it never trips the declaration check.
        let vpn = ServicePermission::vpn();
        assert_eq!(vpn.kind(), ServiceKind::Vpn);
        assert_eq!(vpn.class(), "");
        // Desktop has no backend at all: the action stays Unsupported instead of ConfigureBuild.
        let permissions = Permissions::new();
        assert_eq!(
            permissions.service_action(vpn),
            PermissionAction::Unsupported
        );
    }
}
