use super::*;

// ═══════════════════════════════════════════════════════════
// 完整 Material 3 色彩方案（49 色槽，对齐 material-colors Scheme）
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Copy)]
pub struct ThemeColors {
    // ── Primary ──
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub inverse_primary: Color,
    pub primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed: Color,
    pub on_primary_fixed_variant: Color,

    // ── Secondary ──
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub secondary_fixed: Color,
    pub secondary_fixed_dim: Color,
    pub on_secondary_fixed: Color,
    pub on_secondary_fixed_variant: Color,

    // ── Tertiary ──
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub tertiary_fixed: Color,
    pub tertiary_fixed_dim: Color,
    pub on_tertiary_fixed: Color,
    pub on_tertiary_fixed_variant: Color,

    // ── Error ──
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,

    // ── Surface（7 层 elevation）──
    pub surface_dim: Color,
    pub surface: Color,
    pub surface_tint: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub surface_variant: Color,

    // ── On Surface ──
    pub on_surface: Color,
    pub on_surface_variant: Color,

    // ── Outline ──
    pub outline: Color,
    pub outline_variant: Color,

    // ── Inverse ──
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,

    // ── Background／Shadow／Scrim ──
    pub background: Color,
    pub on_background: Color,
    pub shadow: Color,
    pub scrim: Color,
}

impl ThemeColors {
    /// The content colour Compose derives from a container colour — `ColorScheme.contentColorFor`
    /// (`material3/ColorScheme.kt:1064-1099`): each scheme colour that has a paired "on" colour maps to it,
    /// and anything else maps to `None`, which is why `Badge`'s default falls back to the ambient content
    /// colour next (`ColorScheme.kt:1112-1115`: `.takeOrElse { LocalContentColor.current }`).
    ///
    /// Compose builds the map with a `when` over the fourteen pairs; this is the same list, and the
    /// `surfaceContainer*`/`surfaceBright`/`surfaceDim` family all answer `onSurface` as they do there.
    pub fn content_color_for(&self, background: Color) -> Option<Color> {
        let pairs = [
            (self.primary, self.on_primary),
            (self.secondary, self.on_secondary),
            (self.tertiary, self.on_tertiary),
            (self.background, self.on_background),
            (self.error, self.on_error),
            (self.primary_container, self.on_primary_container),
            (self.secondary_container, self.on_secondary_container),
            (self.tertiary_container, self.on_tertiary_container),
            (self.error_container, self.on_error_container),
            (self.inverse_surface, self.inverse_on_surface),
            (self.surface, self.on_surface),
            (self.surface_variant, self.on_surface_variant),
            (self.surface_bright, self.on_surface),
            (self.surface_container, self.on_surface),
            (self.surface_container_high, self.on_surface),
            (self.surface_container_highest, self.on_surface),
            (self.surface_container_low, self.on_surface),
            (self.surface_container_lowest, self.on_surface),
            (self.surface_dim, self.on_surface),
            (self.primary_fixed, self.on_primary_fixed),
            (self.primary_fixed_dim, self.on_primary_fixed),
            (self.secondary_fixed, self.on_secondary_fixed),
            (self.secondary_fixed_dim, self.on_secondary_fixed),
            (self.tertiary_fixed, self.on_tertiary_fixed),
            (self.tertiary_fixed_dim, self.on_tertiary_fixed),
        ];
        pairs
            .into_iter()
            .find(|(container, _)| *container == background)
            .map(|(_, content)| content)
    }

    pub(crate) fn from_scheme(s: &material_colors::scheme::Scheme) -> Self {
        Self {
            primary:              rgb(s.primary),
            on_primary:           rgb(s.on_primary),
            primary_container:    rgb(s.primary_container),
            on_primary_container: rgb(s.on_primary_container),
            inverse_primary:      rgb(s.inverse_primary),
            primary_fixed:        rgb(s.primary_fixed),
            primary_fixed_dim:    rgb(s.primary_fixed_dim),
            on_primary_fixed:     rgb(s.on_primary_fixed),
            on_primary_fixed_variant: rgb(s.on_primary_fixed_variant),

            secondary:              rgb(s.secondary),
            on_secondary:           rgb(s.on_secondary),
            secondary_container:    rgb(s.secondary_container),
            on_secondary_container: rgb(s.on_secondary_container),
            secondary_fixed:        rgb(s.secondary_fixed),
            secondary_fixed_dim:    rgb(s.secondary_fixed_dim),
            on_secondary_fixed:     rgb(s.on_secondary_fixed),
            on_secondary_fixed_variant: rgb(s.on_secondary_fixed_variant),

            tertiary:              rgb(s.tertiary),
            on_tertiary:           rgb(s.on_tertiary),
            tertiary_container:    rgb(s.tertiary_container),
            on_tertiary_container: rgb(s.on_tertiary_container),
            tertiary_fixed:        rgb(s.tertiary_fixed),
            tertiary_fixed_dim:    rgb(s.tertiary_fixed_dim),
            on_tertiary_fixed:     rgb(s.on_tertiary_fixed),
            on_tertiary_fixed_variant: rgb(s.on_tertiary_fixed_variant),

            error:              rgb(s.error),
            on_error:           rgb(s.on_error),
            error_container:    rgb(s.error_container),
            on_error_container: rgb(s.on_error_container),

            surface_dim:               rgb(s.surface_dim),
            surface:                   rgb(s.surface),
            surface_tint:              rgb(s.surface_tint),
            surface_bright:            rgb(s.surface_bright),
            surface_container_lowest:  rgb(s.surface_container_lowest),
            surface_container_low:     rgb(s.surface_container_low),
            surface_container:         rgb(s.surface_container),
            surface_container_high:    rgb(s.surface_container_high),
            surface_container_highest: rgb(s.surface_container_highest),
            surface_variant:           rgb(s.surface_variant),

            on_surface:         rgb(s.on_surface),
            on_surface_variant: rgb(s.on_surface_variant),

            outline:         rgb(s.outline),
            outline_variant: rgb(s.outline_variant),

            inverse_surface:    rgb(s.inverse_surface),
            inverse_on_surface: rgb(s.inverse_on_surface),

            background:    rgb(s.background),
            on_background: rgb(s.on_background),
            shadow:        rgb(s.shadow),
            scrim:         rgb(s.scrim),
        }
    }

    pub fn light_from_seed(seed: u32) -> Self {
        let theme = ThemeBuilder::with_source(Rgb::from_u32(seed)).build();
        Self::from_scheme(&theme.schemes.light)
    }

    pub fn dark_from_seed(seed: u32) -> Self {
        let theme = ThemeBuilder::with_source(Rgb::from_u32(seed)).build();
        Self::from_scheme(&theme.schemes.dark)
    }

    pub fn default_light() -> Self { Self::light_from_seed(0x6750A4) }
    pub fn default_dark() -> Self  { Self::dark_from_seed(0x6750A4) }
}

/// 0.5's colour type is `Rgb`; the `Argb` it replaced also carried the alpha, and an M3 scheme colour
/// is opaque — upstream composes the same value in `Rgb::as_argb_u32` (`0xFF000000 | rgb`).
///
/// The 0.4.2 -> 0.5.0 bump also MOVES four slots of every generated LIGHT scheme, and nothing else:
/// the four `on_*_container` roles go from a very dark tone (~10) to a medium one (~30), so text and
/// icons drawn ON a container are lighter than they were. For the default seed:
/// `on_primary_container 201047 -> 4d3d75`, `on_secondary_container 1e192b -> 4a4458`,
/// `on_tertiary_container 31101d -> 633b48`, `on_error_container 410002 -> 93000a`. The other 45
/// light slots and all 49 dark ones are byte-identical (measured by dumping the 49 slots of both
/// schemes for the same seed; the method is in the commit that made this change).
///
/// Worth knowing when one of these shifts again: NOTHING in this crate pins a palette. The pixel
/// tests in `tests/visual_matrix.rs` derive their expectations from this same theme, so a palette
/// move shifts both sides and they stay green.
pub(crate) fn rgb(a: Rgb) -> Color {
    Color::from_argb(255, a.red, a.green, a.blue)
}

// ═══════════════════════════════════════════════════════════
// 系统暗色模式检测（dark-light crate）
// ═══════════════════════════════════════════════════════════

/// The stored theme mode: `0` = follow the system (the initial state), `1` = forced light, `2` = forced
/// dark. Written by [`set_system_dark_mode`] — an application that wants to pin its theme, or a test.
/// The platform's own `WindowEvent::ThemeChanged` does NOT pin anything: it OBSERVES the system, which is
/// why it is recorded in a separate slot below.
pub(crate) static SYSTEM_THEME_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// The last value the platform reported for the system theme: `-1` = never reported, `0` = light,
/// `1` = dark. Consulted only while the mode follows the system; [`is_system_dark_theme`] falls back to
/// detection, which is also the only source on platforms whose event never fires (X11, Wayland).
pub(crate) static SYSTEM_THEME_DARK: std::sync::atomic::AtomicI8 = std::sync::atomic::AtomicI8::new(-1);
/// The mode as a REACTIVE value, and the one the composition actually reads: a tracked read of it is what
/// makes a window's composer wake at all when the mode changes (each window composes in its own composer,
/// and a composer with nothing pending is never asked to redraw). The atomic above stays for the callers
/// that ask outside a composition (`is_system_dark_theme` from the app loop).
pub(crate) static SYSTEM_THEME_STATE: std::sync::LazyLock<crate::runtime::state::Reactive<u8>> =
    std::sync::LazyLock::new(|| crate::runtime::state::Reactive::new(system_theme_mode()));
/// How many times the SYSTEM theme may have changed. A window records the epoch it last resolved at and
/// re-resolves when it moves — per WINDOW state, deliberately: a single process-wide "pending" flag is
/// consumed by whichever window renders first, which left every other window on its old palette.
pub(crate) static SYSTEM_THEME_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// Set when every window should be asked for a frame because the mode moved.
///
/// Windows that follow the system are woken by their own dependency on the mode, but not every window has
/// one: a window whose content an application composed itself (through `app::open_window_with_title`) has
/// nothing to wake it, and a mode change can come from a background thread that cannot reach the windows.
/// The app loop drains this — it is the one place that knows them.
pub(crate) static THEME_REDRAW_ALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether every window should be asked for a frame (the app loop's check).
pub(crate) fn take_theme_redraw_all() -> bool {
    THEME_REDRAW_ALL.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// The pinned mode (`0` = follow the system, `1` = light, `2` = dark).
pub(crate) fn system_theme_mode() -> u8 {
    SYSTEM_THEME_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Bumped whenever the system theme may have changed ([`set_system_dark_mode`], [`note_platform_theme`]).
/// Callers compare it against the epoch they last resolved at: equal means "nothing to look at".
pub(crate) fn system_theme_epoch() -> u64 {
    SYSTEM_THEME_EPOCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// Pin the theme, or hand it back to the system with `None`.
///
/// The change lands on the next frame: this only stores the value and bumps [`system_theme_epoch`]
/// (running a full recompose from arbitrary code would be a re-entrancy hazard, and the loop is where the
/// composers live). Each window resolves the stored mode when its next frame refreshes the theme.
pub fn set_system_dark_mode(mode: Option<bool>) {
    use std::sync::atomic::Ordering;
    let value = match mode {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    if SYSTEM_THEME_MODE.swap(value, Ordering::Relaxed) != value {
        // Wake every window that follows the system (the tracked read in the content wrapper) and tell
        // them there is something to re-resolve; ask the loop for a frame for the ones that cannot be woken
        // that way.
        SYSTEM_THEME_STATE.set(value);
        SYSTEM_THEME_EPOCH.fetch_add(1, Ordering::Relaxed);
        THEME_REDRAW_ALL.store(true, Ordering::Relaxed);
        crate::runtime::state::wake_loop();
    }
}

/// Record what the platform reported for the system theme — `WindowEvent::ThemeChanged`.
///
/// An observation, not a pin: the platform value is what "follow the system" resolves to, and pinning it
/// would freeze the app on the first report (a later system change would then be ignored, because an
/// explicit pin outranks detection by design). While an application has pinned a mode this only records
/// the value — nothing the user sees can change.
pub(crate) fn note_platform_theme(dark: bool) {
    use std::sync::atomic::Ordering;
    let value = if dark { 1i8 } else { 0i8 };
    if SYSTEM_THEME_DARK.swap(value, Ordering::Relaxed) != value && system_theme_mode() == 0 {
        SYSTEM_THEME_EPOCH.fetch_add(1, Ordering::Relaxed);
    }
}

/// Detect whether the system is in dark mode — or answer with the mode an application pinned through
/// [`set_system_dark_mode`].
///
/// Order: a pinned mode wins; then the value the platform reported; then a fresh detection
/// (cross-platform: the registry on Windows, NSUserDefaults on macOS, freedesktop on Linux). Note that
/// the platform's live theme event exists on Windows and macOS only; elsewhere a pinned mode is the only
/// way to follow a change without the app asking again.
pub fn is_system_dark_theme() -> bool {
    use std::sync::atomic::Ordering;
    match SYSTEM_THEME_MODE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => match SYSTEM_THEME_DARK.load(Ordering::Relaxed) {
            1 => true,
            0 => false,
            _ => matches!(dark_light::detect().unwrap_or(dark_light::Mode::Unspecified), dark_light::Mode::Dark),
        },
    }
}

/// The palette the system theme resolves to right now (the default M3 palettes; a pinned mode outranks
/// the system).
pub(crate) fn system_theme_colors() -> ThemeColors {
    if is_system_dark_theme() { ThemeColors::default_dark() } else { ThemeColors::default_light() }
}

