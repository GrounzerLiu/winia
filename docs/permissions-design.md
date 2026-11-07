# Permission Design

> Status: first Android slice implemented on branch feat/permissions.
> Scope: Android first; iOS and desktop follow when their platform backends exist.
> This document records the decisions from the permission-system discussion and the current implementation.

## 1. Decision

Winia should provide a first-party permission layer.

It should not try to be a universal cross-platform permission system. It should be a thin capability API over platform-owned permission models:

- Applications ask for a capability, not a raw Android permission string.
- The platform decides whether a dialog, a Settings page, an entitlement, or no action at all is required.
- Native manifest files remain the source of truth for declarations.
- The OS remains the source of truth for the current status.
- Requests are always explicit; composing a widget must never open a permission dialog.

The first implementation target is Android. Winia already has an Android entry point, an Activity-backed JNI layer, and a generated GameActivity project, so Android is the only platform where a complete permission path can be implemented and tested end to end today.

Permission support is limited to GameActivity. Winia no longer exposes or supports a NativeActivity backend: without a generated Java MainActivity subclass there is no permission result bridge, and the IME path is also unavailable. All generated and supported Android projects use GameActivity.

## 1.1 Implemented first slice

The experimental branch contains the following:

- `winia::permissions` with `Capability` for camera, microphone, foreground location,
  notifications, overlay, write settings, all-files access, install packages, exact alarms, usage
  stats, and battery optimization exemptions; plus `PermissionStatus`, `PermissionError`,
  `PermissionRequest`, and a `Permissions` facade.
- `ComposeCtx::permissions()`, whose `status()` and `request()` are Unsupported on desktop.
- Android status via `Activity.checkSelfPermission`.
- Android requests through the generated adapter `com.winia.permissions.WiniaPermissions`, backed by
  XXPermissions 28.3. The adapter receives results through XXPermissions' headless Fragment, so
  MainActivity does not override `onRequestPermissionsResult`.
- The adapter stores results by request id and calls a small native notification. The notification
  bumps a reactive permission revision; a composition that read `Permissions::status()` then
  recomposes and re-queries the OS. This fixes the stale "Denied after grant" display.
- `PermissionAction` tells the UI whether a capability uses the normal system dialog or a settings
  page. The app resumes from a settings page through the same permission revision refresh.
- A probe app on an Android 36 AVD showed the system camera, foreground location, and notification
  dialogs; each grant changed the on-screen status without restarting the Activity.

Known limits of this slice:

- Location covers foreground (fine/coarse) and background; the background request includes the
  foreground permissions first. Picker/scoped access is not wired yet.
- Special permissions use XXPermissions' settings-page flow: overlay, write settings, all-files
  access, install packages, exact alarms, usage stats, and battery optimization. Overlay was granted
  on the AVD and the status refreshed after returning; the remaining special pages still need device
  coverage.
- `NotDetermined` is tracked per process: a denied permission that has never been requested maps to
  `NotDetermined`, but the history does not survive an app restart.
- `PermissionRequest::wait` blocks the calling thread; UI code should use `poll` or rely on the
  reactive status.
- `PermanentlyDenied` is reported by the adapter; `Permissions::open_settings()` opens the app's
  system details page, and `Permissions::open_settings_for(capability)` asks XXPermissions for the
  vendor-specific capability settings intent, falling back to app details.
- Declarations remain in AndroidManifest.xml; `winia.toml` still has no permission table.
- The branch does not publish a new crate version; it is for evaluation.

## 2. What Flutter Does

Flutter itself has no permission system. Dart code cannot query or request OS permissions through the Flutter framework. The de facto solution is the federated plugin permission_handler:

~~~text
Dart application
    -> permission_handler (app-facing API)
    -> permission_handler_platform_interface
    -> Android / iOS implementation
    -> OS permission APIs
~~~

The useful ideas to borrow are:

- A small app-facing API.
- A platform interface behind it.
- Platform-specific status values.
- Native declarations in AndroidManifest.xml and Info.plist.
- An explicit action to open the app's Settings page.

The part not to copy is a single giant enum that pretends every permission has the same request flow. Android's special app access, iOS entitlements, HealthKit, local network, and picker-based access do not fit that shape.

## 3. Goals and Non-Goals

### Goals

- Provide one capability vocabulary for common application permissions.
- Handle Android runtime permission requests, results, and Activity lifecycle correctly.
- Expose platform-specific statuses without forcing them into a boolean.
- Provide an explicit Settings fallback.
- Integrate with Winia's reactive state so UI can reflect permission changes.
- Make permission declarations visible and checkable at build time.
- Keep the door open for iOS and desktop without claiming false parity.

### Non-Goals

- Drawing a custom system permission dialog.
- Automatically requesting every permission at startup.
- Replacing AndroidManifest.xml or Info.plist with a Winia-owned permission list.
- Treating all Android/iOS permissions as runtime permissions.
- Promising that a capability has the same semantics on every platform.
- Maintaining a long-lived permission cache as the source of truth.

## 4. Permission Categories

The standard flow only covers runtime dangerous permissions. A correct design must model the other categories explicitly.

| Category | Runtime behavior | Android examples | iOS examples |
| --- | --- | --- | --- |
| Runtime | System dialog | Camera, microphone, contacts, calendar, foreground location, POST_NOTIFICATIONS (13+), READ_MEDIA_* (13+) | Camera, microphone, photos, contacts, calendar, location, Bluetooth |
| Install-time / normal | Granted at install; no request | INTERNET, VIBRATE, WAKE_LOCK, RECEIVE_BOOT_COMPLETED | Most entitlements and usage descriptions do not have a runtime request |
| Settings-only special access | Open a specific Settings page, then re-check | Overlay, write settings, all-files access, install unknown apps, usage access, exact alarms | Some privacy toggles are only in Settings |
| Service binding / component enablement | User enables a service; the OS binds it | Accessibility service, notification listener, VPN service, device admin | Network Extension, VPN, some background modes |
| Entitlement / provisioning | Build and signing concern | Some permissions are signature/system-only | Push, App Groups, Associated Domains, HealthKit, HomeKit |
| Picker / scoped access | No broad permission; user selects an item | Photo Picker, Storage Access Framework | PHPicker, document picker |
| Usage-triggered prompt | Prompt appears on first use, not on request | Some APIs have their own consent flows | Local network prompt, App Tracking Transparency |
| Policy-restricted | Runtime request may exist, but distribution policy applies | Background location, MANAGE_EXTERNAL_STORAGE, QUERY_ALL_PACKAGES, exact alarms | Sensitive entitlements and App Store review |

### 4.1 Android Details

Normal permissions:

~~~text
INTERNET
ACCESS_NETWORK_STATE
VIBRATE
WAKE_LOCK
RECEIVE_BOOT_COMPLETED
~~~

These are declared in the manifest and granted at install. request() should not show a dialog; it should report Granted or NotRequired.

Dangerous runtime permissions use the familiar path:

~~~text
checkSelfPermission
    -> requestPermissions
    -> system dialog
    -> onRequestPermissionsResult
~~~

Special app access cannot be granted with requestPermissions. Examples:

~~~text
SYSTEM_ALERT_WINDOW        Settings.canDrawOverlays()
WRITE_SETTINGS             Settings.System.canWrite()
MANAGE_EXTERNAL_STORAGE    Environment.isExternalStorageManager()
REQUEST_INSTALL_PACKAGES   PackageManager.canRequestPackageInstalls()
PACKAGE_USAGE_STATS        AppOps / usage access settings
SCHEDULE_EXACT_ALARM       AlarmManager.canScheduleExactAlarms()
~~~

The app opens a specific Settings intent and checks the state again when it resumes. A generic "open app settings" button is not sufficient for these.

Accessibility services, notification listeners, VPN services, and device administrators are service-binding cases. The user enables the component in system settings; there is no ordinary runtime permission dialog.

Picker-based access removes the need for broad storage permissions:

~~~text
Photo Picker
Storage Access Framework
~~~

The app receives access only to the selected URI. The permission state should be NotRequired, not Granted.

Android also has transitional semantics that a simple boolean cannot represent:

- One-time grants for location, microphone, and camera.
- Approximate vs precise location.
- Selected photos on Android 14.
- Auto-reset of unused runtime permissions.
- Notification channels disabled below Android 13, where the app permission may still be granted.

### 4.2 iOS Details

Most common permissions use a native API and a usage description from Info.plist:

~~~text
NSCameraUsageDescription
NSMicrophoneUsageDescription
NSPhotoLibraryUsageDescription
NSLocationWhenInUseUsageDescription
NSBluetoothAlwaysUsageDescription
~~~

The system prompt appears the first time the corresponding API is called. Later requests return the current status without prompting.

iOS-specific exceptions:

- Photos can be full, limited, or add-only.
- Notifications can be authorized, denied, or provisional.
- HealthKit and HomeKit use their own authorization UI, not a generic permission alert.
- App Tracking Transparency uses ATTrackingManager, its own usage description, and its own status values.
- Local network access is usage-triggered and has no ordinary "request local network permission" API.
- Push, VPN, Network Extension, App Groups, and Associated Domains are entitlements, not runtime permissions.
- There is no exact "permanently denied" status. A denied permission usually has to be changed in Settings.
- Background location is a separate upgrade from When In Use, and the user may have to change it in Settings.

### 4.3 Desktop

Desktop platforms do not share the mobile model:

- macOS: TCC prompts and System Settings privacy panes.
- Windows: privacy settings, capabilities, and per-API consent.
- Linux: portals and sandbox permissions.
- Web: the browser Permissions API with browser-specific behavior.

Desktop support should be a separate backend rather than an attempt to fit mobile statuses.

## 5. Proposed API

The exact names can change, but the shape should preserve these distinctions.

~~~rust
pub enum Capability {
    Camera,
    Microphone,
    Location(LocationAccess),
    Notifications,
    Photos(PhotoAccess),
    Contacts,
    Bluetooth(BluetoothAccess),
}

pub enum LocationAccess {
    WhenInUse,
    Always,
}

pub enum PhotoAccess {
    Full,
    Limited,
    AddOnly,
}

pub enum PermissionKind {
    Runtime,
    InstallTime,
    SettingsOnly,
    ServiceBinding,
    Entitlement,
    Picker,
    PolicyRestricted,
    Unsupported,
}

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

pub enum PermissionAction {
    SystemDialog,
    OpenSettings(SettingsPage),
    ConfigureBuild,
    NotRequired,
    Unsupported,
}
~~~

A capability state should expose:

~~~rust
pub struct PermissionState { /* backend + capability + reactive status */ }

impl PermissionState {
    pub fn capability(&self) -> Capability;
    pub fn kind(&self) -> PermissionKind;
    pub fn status(&self) -> PermissionStatus;
    pub fn action(&self) -> PermissionAction;

    /// Only valid for PermissionAction::SystemDialog.
    pub async fn request(&self) -> PermissionStatus;

    /// Explicitly open the relevant Settings page.
    pub fn open_settings(&self);
}
~~~

The action enum is important. It tells the UI what kind of interaction is possible before the user presses a button:

- SystemDialog: the normal permission request path.
- OpenSettings(page): the app must guide the user to a specific system page.
- ConfigureBuild: a manifest entry or entitlement is missing.
- NotRequired: a picker or scoped API needs no permission.
- Unsupported: this capability does not exist on the current platform.

Do not make request() silently open Settings. That hides the difference between a system permission prompt and a manual settings flow.

## 6. Request Flow

The standard runtime flow:

~~~text
Business action
    -> read PermissionState.status()
    -> decide whether a rationale is needed
    -> request()
    -> platform backend
    -> OS dialog
    -> native callback
    -> normalize status
    -> update reactive state
    -> UI reacts
~~~

### 6.1 Before the request

- Never request at startup just to "warm up" permissions.
- Check the current status first.
- Show an application-owned rationale before the system dialog when appropriate.
- Do not treat shouldShowRequestPermissionRationale as a complete state machine by itself.

On Android, shouldShowRequestPermissionRationale is false both before the first request and after a permanent denial. The permission manager must also track whether this capability has been requested, so the UI can distinguish:

~~~text
never requested
requested and denied (can ask again)
requested and permanently denied
~~~

That request history is UX metadata, not the authority. The OS status is re-checked on every fresh resolution.

### 6.2 Android bridge

The preferred Android backend is XXPermissions (Apache-2.0), behind a Winia-owned adapter.

XXPermissions creates a headless Fragment and receives the permission result through the Fragment
lifecycle. For a plain Activity it uses android.app.Fragment; for a FragmentActivity it uses the
AndroidX fragment manager. GameActivity is a plain Activity, so the generated MainActivity does NOT
need to override onRequestPermissionsResult.

Preferred flow:

~~~text
Rust Capability
    -> PermissionManager (request id + pending future)
    -> JNI
    -> WiniaPermissions.java (thin XXPermissions adapter)
    -> XXPermissions.with(activity).permission(...).request(...)
    -> XXPermissions Fragment result
    -> nativeOnPermissionResult(requestId, granted, denied, doNotAskAgain)
    -> pending Rust request
~~~

XXPermissions is an implementation detail, not part of Winia's public API. Its types and callback
objects must not leak into application code, so the backend can be replaced without changing the
Capability or PermissionStatus model.

Requirements:

- Requests run on the Android UI thread.
- Requests are serialized; do not show several permission dialogs at once.
- Pending requests have stable IDs and are removed on completion.
- Activity recreation clears or re-queues pending requests; a result cannot be assumed to survive process death.
- The callback must not mutate composition from an arbitrary thread; it should resolve a future or post an event to the runtime.
- The generated project must include the Java adapter and the pinned XXPermissions dependency; do not
  rely on the library's default rationale or its unchecked() mode.
- NativeActivity is not supported anywhere in Winia. It has no generated Java MainActivity subclass and therefore no supported result bridge; Winia Forge and the framework expose only GameActivity.

### 6.3 iOS bridge

When iOS support exists, the flow is:

~~~text
Info.plist declaration
    -> native authorization API
    -> system prompt or framework sheet
    -> delegate / completion handler
    -> normalized status
~~~

iOS has no permanent-denied status. A denied capability should be shown as denied with an "Open Settings" action, not as a second request.

### 6.4 After the result

Normalize the native result:

~~~text
granted                              -> Granted
denied, can ask again                -> Denied
denied, cannot ask again             -> PermanentlyDenied
policy / parental restriction        -> Restricted
partial photo access                 -> Limited
provisional notification authorization -> Provisional
picker / scoped access               -> NotRequired
~~~

Then update the reactive state and let the UI choose the next step:

- Execute the feature.
- Explain the denial and offer the action again if allowed.
- Offer the specific Settings page.
- Degrade gracefully, for example use the photo picker instead of photo library access.

### 6.5 Re-check on resume

Permissions can change outside the app:

- The user revokes a permission in Settings.
- Android auto-resets an unused permission.
- iOS changes location authorization from Always to When In Use.
- A notification channel is disabled.

Re-query the capability on resume and notify the UI if the status changed. Do not assume the startup status is still valid.

## 7. winia.toml Boundary

Permission declarations do not belong in winia.toml.

Current platform sections such as [android] and [package.windows] are build parameters. Permissions are security declarations owned by the platform, and they have platform-specific details:

- Android manifest entries can include maxSdkVersion, uses-feature, and manifest-merger contributions from dependencies.
- iOS usage descriptions are localized human-readable strings in Info.plist.
- Entitlements are tied to signing and provisioning.
- Users and native tooling may edit the platform project files directly.

Keeping a second permission list in winia.toml would create two sources of truth and would eventually drift from the packaged manifest.

The resulting boundary:

~~~text
winia.toml             build/app configuration; no permission table
AndroidManifest.xml    Android permission declarations
Info.plist             iOS declarations and usage descriptions
Entitlements           signing/provisioning capabilities
winia::permissions     runtime query/request API
winia doctor           read-only declaration checks and warnings
~~~

winia doctor can report which permissions are declared, warn about missing usage descriptions, and list capability requirements. It should not claim to statically infer every permission a program will request.

If Winia later takes full ownership of generating platform projects, a declarative permission source becomes possible, but that is a larger architectural change. It would require:

- Generated manifests that users do not edit.
- A migration and merge strategy.
- A rule that android init --force cannot erase user permissions.
- An explicit generated-file contract.

Until then, native declarations are the source of truth.

## 8. Implementation Plan

### Phase 1: Android runtime permissions

- Add the capability/status/action API.
- Support Camera, Microphone, Location (When In Use), and Notifications.
- Add a Java permission adapter backed by XXPermissions; no MainActivity override.
- Add a serialized request queue.
- Add manifest declaration checks to winia doctor.
- Test on a real device with both grant and deny paths.

### Phase 2: Reactive state and UX

- Add PermissionState to the application/runtime context.
- Notify the UI after requests and on resume.
- Add a rationale component or a documented rationale pattern.
- Handle permanent denial and specific Settings pages.
- Cover approximate location and one-time grants.

### Phase 3: Special and scoped access

- Add Settings-only permissions.
- Add picker/scoped access as NotRequired.
- Add selected-photo and limited-photo states.
- Add background location as a separate, policy-aware capability.

### Phase 4: iOS and desktop

- Add an iOS backend with Info.plist integration.
- Add HealthKit, ATT, local network, and other non-standard iOS cases only where there is a clear API.
- Add desktop backends separately; do not reuse mobile status semantics blindly.

## 9. Acceptance Criteria

The first phase is complete when:

- A real Android app can query camera status.
- Granting camera permission returns Granted and starts the feature.
- Denying once returns Denied and can be requested again.
- Permanent denial returns PermanentlyDenied and the UI can open the correct Settings page.
- A permission revoked in Settings is reflected after resume.
- Two concurrent requests do not produce two overlapping system dialogs.
- Activity recreation does not hang a pending request.
- The manifest check reports a missing declaration instead of failing at request time.
- The behavior is covered by device tests and fake-backend unit tests.

## 10. Open Questions

- The exact first capability list; Camera, Microphone, Location, and Notifications are the proposed minimum.
- Whether permission state belongs on ComposeCtx, an application service, or a window-level context. The request API should not depend on composition for correctness.
- Whether request() returns a status or a richer result that includes the chosen location precision or photo scope.
- How long to retain "has requested before" UX metadata.
- NativeActivity has been removed from both winia-forge and the winia framework; GameActivity is the only supported Android backend.
- How much of the iOS model can be exposed before Winia has an iOS platform backend.
- Whether winia doctor should only check declarations or also generate a recommended manifest snippet.

## 11. Summary

~~~text
Declarations belong to the platform.
Queries and requests belong to Winia's runtime API.
The system owns the result.
The application owns the rationale and the fallback UX.
~~~

The first version should be Android-only, runtime-permission-only, explicit, and small. Special permissions, iOS entitlements, and desktop privacy models should be added as separate platform capabilities, not compressed into one universal "request permission" button.
