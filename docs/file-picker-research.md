# File Picker Research

> Status: research only, deliberately paused. No implementation has started and no API decision has
> been made. This document exists so the findings are not lost: it compares the platform APIs, the
> Rust crate landscape, and Flutter's design, and records the questions that have to be answered
> before any code is written.
>
> Resume from section 8. Nothing here is a commitment to a particular crate or API shape.

## 1. Why this is not one API

Every platform models "choose a file" differently, and the differences are not cosmetic:

- Windows and macOS hand back a filesystem path.
- Linux may hand back a path through GTK, or a `file://` URI through xdg-desktop-portal.
- Android hands back a `content://` URI that grants a temporary read/write permission to one
  document. There is no path, and the grant expires with the Activity unless it is persisted.

A design that returns `PathBuf` everywhere is therefore wrong on Android. The Flutter findings in
section 5 show the same conclusion reached independently.

## 2. Windows: Common Item Dialog

`IFileOpenDialog` and `IFileSaveDialog` both derive from `IFileDialog`, which derives from
`IModalWindow`.

Standard flow:

1. `CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED)` on the calling thread.
2. `CoCreateInstance(CLSID_FileOpenDialog | CLSID_FileSaveDialog, ..., IID_PPV_ARGS(&dialog))`.
3. Configure: `SetOptions`, `SetFileTypes`, `SetDefaultExtension`, `SetFileName`, `SetFolder`.
4. `Show(hwndOwner)` — modal, pumps its own message loop.
5. `GetResult` (single) or `GetResults` (multi).

| Concern | Answer |
| --- | --- |
| Blocking | Yes. `Show` runs a modal loop until the user answers. |
| Thread | The calling thread must have called `CoInitializeEx` with `COINIT_APARTMENTTHREADED`. It does not have to be the process main thread. In practice the event-loop thread is the only sensible choice because `Show` blocks it. |
| Cancel | `Show` returns `HRESULT_FROM_WIN32(ERROR_CANCELLED)` (`0x800704C7`). |
| Result | `IUnknown` → `IShellItem` / `IShellItemArray` → `GetDisplayName(SIGDN_FILESYSPATH)`. |
| Multi-select | `FOS_ALLOWMULTISELECT` + `GetResults`. |
| Folder pick | `FOS_PICKFOLDERS` on the open dialog (`FOS_FORCEFILESYSTEM` keeps it to real paths). |
| Overwrite prompt | `FOS_OVERWRITEPROMPT` on the save dialog. |

Notable: `FOS_FORCEFILESYSTEM` matters because without it a virtual shell item (a library, a cloud
folder) can be returned, which has no filesystem path.

The legacy `GetOpenFileName` (comdlg32) still works but is deprecated in favour of the COM dialogs:
it cannot return modern shell items, has a fixed filter string format, and its `OPENFILENAME` struct
changes size between Windows versions.

## 3. macOS: NSOpenPanel / NSSavePanel

Both are `@MainActor` (their `NSWindow` superclass is main-thread-only), so the call must come from
the main thread.

| Concern | Answer |
| --- | --- |
| Blocking | `runModal()` blocks. `beginSheetModalForWindow:completionHandler:` is asynchronous and attaches the panel to the window. |
| Cancel | `NSModalResponseCancel` (0), or a `nil` URL in the completion handler. |
| Result | `NSURL` (file URL) — `urls` for multi-select. |
| Multi-select | `allowsMultipleSelection`. |
| Folder pick | `canChooseDirectories = YES`, `canChooseFiles = NO`. |
| Filtering | `allowedContentTypes` (array of `UTType`, macOS 11+) replaced `allowedFileTypes` (extensions, now deprecated). |
| Default name | `nameFieldStringValue`; default directory `directoryURL`. |

Sandbox: a sandboxed app cannot keep using a URL after the panel closes. It must create a
security-scoped bookmark and later call `startAccessingSecurityScopedResource` /
`stopAccessingSecurityScopedResource` around the access. This is a real constraint for a "recent
files" feature, not an edge case.

## 4. Linux: GTK or portal

Two routes exist and they are not interchangeable:

**GTK in-process.** GTK4 exposes `GtkFileDialog` (asynchronous). GTK3 used `GtkFileChooserDialog` +
`gtk_dialog_run` (blocking). Both require the GTK main thread and mean the application links GTK.

**xdg-desktop-portal.** A D-Bus interface, `org.freedesktop.portal.FileChooser`, with `OpenFile` and
`SaveFile` methods. The dialog is drawn by the portal implementation, not by the app.

- `parent_window` is `x11:<window-id>` or `wayland:<handle>`; it lets the dialog be modal to the app.
- `options` carries `handle_token`, `multiple`, `filters`, `current_name`, `current_folder`,
  `accept_label`.
- The method returns a request handle immediately; the result arrives later as a
  `Response(response, results)` signal on `org.freedesktop.portal.Request`.
- `response == 0` is success, `1` is cancel.

This route is the only correct one under Flatpak/Snap and on compositors where the compositor draws
the dialogs. The cost is that it is asynchronous and returns `file://` URIs, not paths.

## 5. Android: Storage Access Framework

Android has no path-based dialog. The mechanism is an `Intent`:

- `ACTION_OPEN_DOCUMENT` (+ `CATEGORY_OPENABLE`) — pick an existing document.
- `ACTION_OPEN_DOCUMENT_TREE` — pick a directory.
- `ACTION_CREATE_DOCUMENT` — create a new document.

The result is a `content://` URI. Key properties:

- The URI is only usable while the Activity holds the grant; `takePersistableUriPermission` makes it
  survive a restart, and must be called with the flags returned in the result Intent.
- Reading requires `ContentResolver.openInputStream`; the file has no path.
- Type filtering uses MIME types (`type` and `EXTRA_MIME_TYPES`), not extensions.
- The picker runs in another process, so the result arrives through `onActivityResult` /
  `registerForActivityResult`. The call is inherently asynchronous.

Current state in this repository: nothing implements Activity results yet. `MainActivity` is an empty
`GameActivity` subclass, and no `onActivityResult` override exists, so an Android picker needs its own
bridge — structurally the same job `WiniaPermissions.java` does for permissions.

## 6. Rust crate landscape

Verified against crates.io and the local toolchain rather than taken from memory:

| Crate | Latest | Desktop | Android | Notes |
| --- | --- | --- | --- | --- |
| `rfd` | 0.18.0 | Yes | No | `rust-version = 1.92` (see below) |
| `tinyfiledialogs` | 3.9.x | Weakest; shells out on Linux/macOS | No | Unmaintained since 2022 |
| `native-dialog` | 0.9.x | Uses `zenity`/`kdialog` subprocesses on Linux | No | Active, but Linux depends on external tools |
| `robius-file-picker` | 0.3.1 | Wraps `rfd` | Yes (SAF) | Closest thing to an Android-capable option |

**The MSRV conflict is real and was checked directly.** `cargo info rfd` reports
`rust-version: 1.92` for 0.18.0, while this workspace declares `rust-version = "1.88"` in
`winia-forge/Cargo.toml`. Adopting rfd 0.18 means either raising the workspace MSRV or pinning an
older rfd. This is a decision for the maintainer, not something to slip in.

rfd's features: `default = [xdg-portal, wayland]`. The `gtk3` feature exists but linking GTK into a
Skia application that already has its own event loop is the kind of coupling worth avoiding; the
portal path is asynchronous and does not link GTK.

winit itself has **no** dialog API, so a file picker is framework work either way.

## 7. Flutter: what to copy and what to avoid

Two competing implementations exist; the difference in publisher matters:

| | `file_selector` | `file_picker` |
| --- | --- | --- |
| Publisher | `flutter.dev` (first-party) | community (`victorcarreras.dev`) |
| Structure | federated: one package per platform | one package, all platforms |
| Returns | `XFile` (from `cross_file`) | `PlatformFile` |

Correction worth recording: Flutter never deprecated `file_picker`. `file_selector` is a first-party
reimplementation with a federated layout, and `file_picker` remains far more downloaded. Both are
legitimate references.

**API shape of `file_selector`, which is the more interesting one:**

```dart
Future<XFile?> openFile({
  List<XTypeGroup> acceptedTypeGroups = const <XTypeGroup>[],
  String? initialDirectory,
  String? confirmButtonText,
});

Future<List<XFile>> openFiles({...});

Future<FileSaveLocation?> getSaveLocation({
  List<XTypeGroup> acceptedTypeGroups = const <XTypeGroup>[],
  String? initialDirectory,
  String? suggestedName,
  String? confirmButtonText,
});
```

`XTypeGroup` carries `label`, `extensions`, `mimeTypes`, `uniformTypeIdentifiers`, `webWildCards` —
one group, four representations, and each platform reads the one it understands. That is the cleanest
answer to "extension filter on Windows, MIME on Android, UTType on macOS".

`XFile` is the abstraction over "maybe a path, maybe a stream": it exposes `path`, `name`, `length()`,
`openRead()`, `readAsBytes()`, and `saveTo()`. On Android `path` may be a `content://` URI or a cache
copy path, depending on the implementation.

**Answers to the design questions, as implemented:**

- Async, not callbacks. Everything returns a `Future`.
- Cancel is `null` (`Future<XFile?>`), not an exception. `openFiles` returns an empty list.
- Save returns a location, not a file: `FileSaveLocation` has `path` and `suggestedName`, so the
  platform can hand back a directory plus a name rather than an opened handle.
- Multi-select and directory selection are separate functions, not flags.
- Android `content://` is not unified away at the API level: `XFile` keeps it, and reading goes
  through a stream.

`file_picker` differs mainly by inlining options as flags: `FileType.custom`, `allowedExtensions`,
`allowMultiple`, `withData`, `withReadStream`. Putting `withData`/`withReadStream` on the call means
the caller chooses eagerly how the bytes arrive. That is convenient but pushes a policy decision into
every call site.

## 8. Open questions to settle before designing the API

1. **Result type.** Path-only, or a handle that hides path vs. URI vs. stream? The Android case argues
   for a handle; a desktop-only design would argue for `PathBuf`.
2. **Threading and blocking.** Which of the three desktop platforms block the event loop, and is a
   blocking call acceptable on the UI thread? On Windows `Show` blocks the caller, so an async API
   still needs a worker thread plus a way to feed the result back into the event loop.
3. **MSRV.** rfd 0.18 requires 1.92 against a 1.88 workspace. Pin an older rfd, raise the MSRV, or
   implement the platform calls directly.
4. **Linux route.** Portal (async, no GTK linkage, correct under Flatpak) versus GTK (blocking,
   links GTK, but available without a portal). Portal looks like the default, with GTK as a fallback.
5. **Android bridge.** Nothing exists yet for Activity results. This needs the same kind of Java
   adapter, result storage and revision bump that the permission layer already has.
6. **Filters.** Adopting an `XTypeGroup`-like grouping (label + extensions + MIME + UTType) keeps one
   declaration usable on every platform; a single extension list does not survive macOS and Android.
7. **Persistence.** Android needs `takePersistableUriPermission` and macOS needs security-scoped
   bookmarks for anything beyond a one-shot read. That is a separate capability from "pick a file".
8. **Cancellation vs. error.** Flutter returns `null`. Winia's permission layer returns a typed
   error. The two should not disagree without a reason.

## 9. Sources

- Common Item Dialog: <https://learn.microsoft.com/en-us/windows/win32/shell/common-file-dialog>,
  `IFileDialog` reference pages under `/windows/win32/api/shobjidl_core/`.
- `NSOpenPanel` / `NSSavePanel`: Apple Developer documentation, AppKit.
- `GtkFileDialog`: <https://docs.gtk.org/gtk4/class.FileDialog.html>.
- `org.freedesktop.portal.FileChooser`:
  <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html>.
- Android SAF: <https://developer.android.com/guide/topics/providers/document-provider>.
- rfd: <https://crates.io/crates/rfd>, <https://docs.rs/rfd>.
- `file_selector`: <https://pub.dev/packages/file_selector>, source in `flutter/packages`.
