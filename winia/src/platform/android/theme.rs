//! Android system theme detection.
//!
//! android-activity keeps its ConfigurationRef current when the activity receives a
//! ConfigChanged event, so reading uiMode from it is both cheaper and less fragile than
//! rebuilding the same value through Java calls. The API exposes the RESOLVED night mode, not
//! the user's auto/yes/no setting: the system has already decided what the app should use.

use ndk::configuration::UiModeNight;
use winit::platform::android::activity::AndroidApp;

/// Whether Android currently resolves this app to night mode.
pub(crate) fn is_dark(app: &AndroidApp) -> bool {
    matches!(app.config().ui_mode_night(), UiModeNight::Yes)
}
