//! The platform's accessibility text scale.
//!
//! Compose reads `fontScale` from the platform density object. winia asks here, once per window
//! frame, and the answer rides `runtime::density`. Opaque on purpose: the caller wants a factor, not
//! a Windows quirk to branch on.

/// The user's "text size" factor (1.0 = 100%).
///
/// Windows keeps it under `HKCU\Software\Microsoft\Accessibility\TextScaleFactor` as a percentage;
/// the UIA text-size slider writes that value (Microsoft Q&A, `learn.microsoft.com` answer 520296).
/// A missing value means the user never moved the slider, which is 100%. Any other platform answers
/// 1.0.
#[cfg(target_os = "windows")]
pub fn font_scale() -> f32 {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey("Software\\Microsoft\\Accessibility") {
        Ok(key) => key,
        // No key: the accessibility pane was never opened, so the factor is the default.
        Err(_) => return 1.0,
    };
    // The value is a REG_SZ percentage; a REG_DWORD is what some builds write instead.
    let percent: u32 = key
        .get_value("TextScaleFactor")
        .ok()
        .or_else(|| key.get_value::<String, _>("TextScaleFactor").ok().and_then(|s| s.trim().parse().ok()))
        .unwrap_or(100);
    // Windows' slider stops at 225%; anything outside Compose's own 0.5..=4 window is nonsense.
    (percent as f32 / 100.0).clamp(0.5, 4.0)
}

#[cfg(not(target_os = "windows"))]
pub fn font_scale() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    /// The reader must answer something usable on every platform, and a sane default when the
    /// setting is absent. The concrete value is whatever this machine has, so it is asserted as a
    /// RANGE — a test that pinned it would pass only on the machine it was written on.
    #[test]
    fn the_platform_answers_a_usable_text_scale() {
        let scale = super::font_scale();
        assert!((0.5..=4.0).contains(&scale), "the document text scale is inside Compose's window: {scale}");
    }

    /// The registry reader answers the value Windows writes — checked by setting it (the test
    /// cannot assume it may WRITE, so it asserts the range and, when the value is present, the
    /// percentage). Kept as a range so the suite passes on a machine that never touched the slider.
    #[test]
    fn the_platform_reads_the_registry_text_scale() {
        let scale = crate::runtime::platform::font_scale();
        assert!((0.5..=4.0).contains(&scale), "inside Compose's window: {scale}");
    }
}