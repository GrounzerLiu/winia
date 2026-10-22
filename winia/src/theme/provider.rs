use super::*;

// ═══════════════════════════════════════════════════════════
// 主题入口
// ═══════════════════════════════════════════════════════════

/// How a window resolves its theme — the INTENT, captured where the window is created, as opposed to the
/// palette in force at that moment.
///
/// A window's content closure runs on every frame, so a palette captured as a value pins that window to
/// the colors it started with: measured on the demo, an `auto` window kept its dark colors while the
/// surface behind it flipped to light, because the per-frame wrapper re-provided the palette sampled when
/// the window was created. Recording the intent instead lets the wrapper resolve afresh each frame
/// (`Auto`), or re-provide a palette the application chose (`Fixed`).
#[derive(Clone, Debug, PartialEq)]
pub enum ThemeSpec {
    /// Follow the system — or the mode an application pinned through [`set_system_dark_mode`].
    Auto,
    /// Always this palette.
    Fixed(ThemeColors),
}

impl ThemeSpec {
    /// The palette this spec resolves to right now.
    pub fn colors(&self) -> ThemeColors {
        match self {
            ThemeSpec::Auto => system_theme_colors(),
            ThemeSpec::Fixed(colors) => colors.clone(),
        }
    }
}

/// A window's theme: the intent to resolve (`ThemeSpec`), the typography and direction in force where the
/// window was declared, and the palette the intent last resolved to — SHARED between the `Window` node that
/// manages the window and the window itself.
///
/// The two sides live in different composers — the node runs in the tree that DECLARES the window and
/// re-samples all of it every frame (an application may switch which theme node wraps it), while the
/// content wrapper runs in the window's own composer and must see the same, current values. A palette
/// captured as a value pinned the window to its startup colors; an intent sampled once and then frozen
/// pinned it to its startup NODE (a sub-window whose declaring tree switched from `light` to `dark` stayed
/// light forever), and the same held for typography and direction, which the wrapper re-provided as the
/// DEFAULTS every frame — so a custom type scale around a `Window` node survived exactly one frame. Hence a
/// shared cell, written by whichever side knows more.
#[derive(Clone)]
pub struct WindowTheme(std::sync::Arc<std::sync::Mutex<ThemeCell>>);

/// What a window has already drawn with. Compared with the cell's current values to decide whether the tree
/// has to run again: a component resolved colors, type and direction when it built, so a change in ANY of
/// them is only visible after a recomposition.
#[derive(Clone, PartialEq)]
pub(crate) struct AppliedTheme {
    pub colors: ThemeColors,
    pub typography: Typography,
    pub direction: LayoutDirection,
}

pub(crate) struct ThemeCell {
    pub(crate) spec: ThemeSpec,
    pub(crate) typography: Typography,
    pub(crate) direction: LayoutDirection,
    /// The palette `spec` resolved to at `resolved_epoch`.
    pub(crate) colors: ThemeColors,
    /// The system-theme epoch `colors` was resolved at.
    pub(crate) resolved_epoch: u64,
    /// The declaring tree published something different and nobody has resolved it yet.
    pub(crate) spec_dirty: bool,
}

impl WindowTheme {
    /// A theme for `spec` with the default type scale and direction — a window's first frame has no earlier
    /// resolution to reuse, and `Window::build` publishes what it sampled before that frame runs.
    pub fn new(spec: ThemeSpec) -> Self {
        let colors = spec.colors();
        Self(std::sync::Arc::new(std::sync::Mutex::new(ThemeCell {
            spec,
            typography: Typography::default(),
            direction: LayoutDirection::Ltr,
            colors,
            resolved_epoch: system_theme_epoch(),
            spec_dirty: false,
        })))
    }

    /// The cell, never panicking on a poisoned lock: a panic inside a content closure would otherwise
    /// disable the window for the rest of the process (the app loop gives up rendering after 30 consecutive
    /// panics), and the cell holds no invariant a panic could leave half-written — every field is written
    /// under this same lock.
    pub(crate) fn cell(&self) -> std::sync::MutexGuard<'_, ThemeCell> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The intent in force.
    pub fn spec(&self) -> ThemeSpec {
        self.cell().spec.clone()
    }

    /// What this window would draw with right now. The honest starting point for a window whose first frame
    /// has NOT run yet (and whose theme was never compared): defaulting the typography and direction here
    /// would (a) re-run that first frame for nothing and (b) miss a later publish that changed a custom type
    /// scale back to the default, leaving the window on the old one until something else re-ran it.
    pub(crate) fn applied(&self) -> AppliedTheme {
        let cell = self.cell();
        AppliedTheme { colors: cell.colors, typography: cell.typography.clone(), direction: cell.direction }
    }

    /// The palette to compose with — the last resolved one, deliberately not a fresh resolve: a frame
    /// that composes and still skips every group should not rebuild a Material color scheme.
    pub(crate) fn colors(&self) -> ThemeColors {
        self.cell().colors
    }

    /// Publish what the declaring tree sampled this frame. Returns whether anything moved — the window's
    /// own frame has to be scheduled in that case, because the change happened in ANOTHER composer (which
    /// leaves the window's composer with nothing pending).
    pub(crate) fn publish(&self, spec: ThemeSpec, typography: Typography, direction: LayoutDirection) -> bool {
        let mut cell = self.cell();
        if cell.spec == spec && cell.typography == typography && cell.direction == direction {
            return false;
        }
        cell.spec = spec;
        cell.typography = typography;
        cell.direction = direction;
        cell.spec_dirty = true;
        true
    }

    /// Bring the window's theme up to date — the system epoch moved, or the declaring tree published
    /// something else — and compare it with what the window has already drawn. Returns whether what was
    /// drawn is now WRONG; a `Fixed` window under a system change resolves the same palette and keeps the
    /// same type, so it answers `false` and costs nothing.
    pub(crate) fn refresh(&self, drawn: &mut AppliedTheme) -> bool {
        let epoch = system_theme_epoch();
        let mut cell = self.cell();
        if !cell.spec_dirty && cell.resolved_epoch == epoch {
            return false;
        }
        cell.spec_dirty = false;
        cell.resolved_epoch = epoch;
        let colors = cell.spec.colors();
        cell.colors = colors;
        if colors == drawn.colors && cell.typography == drawn.typography && cell.direction == drawn.direction {
            return false;
        }
        drawn.colors = colors;
        drawn.typography = cell.typography.clone();
        drawn.direction = cell.direction;
        true
    }

    /// Provide this window's theme for `content` — the wrapper its content composes under every frame.
    pub(crate) fn provide(&self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let (spec, colors, typography, direction) = {
            let cell = self.cell();
            (cell.spec.clone(), cell.colors, cell.typography.clone(), cell.direction)
        };
        WiniaTheme::provide_resolved(spec, colors, typography, direction, ctx, content);
    }
}

thread_local! {
    /// The spec of the theme the running composition sits inside. A `Window` node samples it so the
    /// window's own content can keep resolving the same way on later frames.
    static CURRENT_THEME_SPEC: std::cell::RefCell<Option<ThemeSpec>> = std::cell::RefCell::new(None);
}

/// The theme spec of the enclosing composition, for a caller that has to carry it into a composition of
/// its own (a window). Falls back to the palette in force, so content created outside any theme node
/// behaves as it always did.
pub(crate) fn current_theme_spec() -> ThemeSpec {
    CURRENT_THEME_SPEC.with(|s| s.borrow().clone()).unwrap_or_else(|| ThemeSpec::Fixed(WiniaTheme::colors()))
}

/// Restores the enclosing spec when a theme provide ends (also on the panic path).
pub(crate) struct SpecGuard(Option<ThemeSpec>);

impl Drop for SpecGuard {
    fn drop(&mut self) {
        CURRENT_THEME_SPEC.with(|s| *s.borrow_mut() = self.0.take());
    }
}

pub struct WiniaTheme;

impl WiniaTheme {
    /// 自动检测系统暗色模式并在子树中提供对应主题。
    pub fn auto(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // A TRACKED read: this scope becomes a dependent of the mode, so `set_system_dark_mode` (or the
        // platform's ThemeChanged) invalidates exactly the compositions that resolved a theme.
        let _ = SYSTEM_THEME_STATE.get();
        Self::provide_spec(ThemeSpec::Auto, Typography::default(), LayoutDirection::Ltr, ctx, content);
    }

    /// 在子树中提供亮色主题。
    pub fn light(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme(ThemeColors::default_light(), ctx, content);
    }

    /// 在子树中提供暗色主题。
    pub fn dark(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme(ThemeColors::default_dark(), ctx, content);
    }

    /// 在子树中提供自定义颜色方案 + LTR 方向。
    pub fn with_theme(colors: ThemeColors, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme_typography_and_direction(colors, Typography::default(), LayoutDirection::Ltr, ctx, content);
    }

    /// 在子树中提供自定义颜色方案、Typography 和 LTR 方向。
    pub fn with_theme_and_typography(
        colors: ThemeColors,
        typography: Typography,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        Self::with_theme_typography_and_direction(colors, typography, LayoutDirection::Ltr, ctx, content);
    }

    /// 在子树中提供自定义颜色方案和布局方向。
    pub fn with_theme_and_direction(colors: ThemeColors, direction: LayoutDirection, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme_typography_and_direction(colors, Typography::default(), direction, ctx, content);
    }

    /// 在子树中提供完整的颜色、Typography 与布局方向主题。
    pub fn with_theme_typography_and_direction(
        colors: ThemeColors,
        typography: Typography,
        direction: LayoutDirection,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        Self::provide_spec(ThemeSpec::Fixed(colors), typography, direction, ctx, content);
    }

    /// The one place a theme reaches the locals: resolve the spec, provide the four locals, and record
    /// the spec for whoever has to carry this theme into a composition of its own (a window's content,
    /// which runs outside this closure on later frames).
    pub(crate) fn provide_spec(
        spec: ThemeSpec,
        typography: Typography,
        direction: LayoutDirection,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        let colors = spec.colors();
        Self::provide_resolved(spec, colors, typography, direction, ctx, content);
    }

    /// Provide a palette that has ALREADY been resolved, keeping the intent it came from — the per-frame
    /// wrapper a window's content composes under.
    ///
    /// Why not resolve here: a window recomposes for every interaction, and a Material scheme is not free
    /// to build, so an idle frame must not rebuild one just to provide the same colors again. The intent
    /// still travels (a `Window` node further down samples it), so a sub-window of a window that follows
    /// the system follows the system too.
    pub(crate) fn provide_resolved(
        spec: ThemeSpec,
        colors: ThemeColors,
        typography: Typography,
        direction: LayoutDirection,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        // A TRACKED read while the theme follows the system: it is what makes THIS composer a dependent of
        // the mode, and therefore what wakes the window at all when the mode changes — a composer with
        // nothing pending is never asked to redraw, and a frame never runs for it.
        if matches!(spec, ThemeSpec::Auto) {
            let _ = SYSTEM_THEME_STATE.get();
        }
        let on_surface = colors.on_surface;
        // A `CompositionLocal` does not invalidate its readers, so the provider has to: when the resolved
        // theme differs from the previous frame's, everything inside composed with the old value —
        // padding, alignment, colors — and has to run again. Without this, a theme or direction switch
        // only reached components that happened to read a state directly (measured: a param-less group
        // between the provider and the reader stayed on the previous palette after the system mode moved).
        //
        // The previous values live in a backchannel: it has the same slot stability as `remember` but
        // never notifies, and this frame is already composing — waking the composer for a value it is
        // reading right now would only schedule an empty pass.
        // The tuple is cheap to clone and every field is `PartialEq`; the backchannel keeps a COPY, so
        // the values below still move into the locals.
        // Keyed by the POSITION rather than by a remembered call site: this function also runs from a
        // window's per-frame wrapper, where there is no `#[composable]` call site to key off, and two
        // providers inside one composer must not share the memory of what they last provided.
        const NAMESPACE: u64 = 0x7769_6E69_6174_6865; // "winia the(me)"
        let prev = ctx.remember_backchannel_at_key(ctx.position_key(NAMESPACE), || {
            crate::runtime::state::Backchannel::new((colors, typography.clone(), direction))
        });
        let resolved = (colors, typography.clone(), direction);
        if prev.get() != resolved {
            prev.set(resolved);
            ctx.mark_subtree_dirty();
        }
        let _restore = SpecGuard(CURRENT_THEME_SPEC.with(|s| s.borrow_mut().replace(spec)));
        crate::layout::direction::provides(direction, || {
            LOCAL_COLORS.provides(colors, || {
                LOCAL_TYPOGRAPHY.provides(typography, || {
                    LOCAL_CONTENT_COLOR.provides(on_surface, || {
                        // The motion scheme travels with the theme, as Compose's `LocalMotionScheme` does.
                        // The theme provides the standard one; a subtree switches to expressive motion with
                        // `WiniaTheme::with_motion_scheme`.
                        LOCAL_MOTION_SCHEME.provides(MotionScheme::standard(), || {
                            content(ctx);
                        });
                    });
                });
            });
        });
    }

    /// The motion scheme in force — Compose's `LocalMotionScheme.current`.
    pub fn motion_scheme() -> MotionScheme {
        LOCAL_MOTION_SCHEME.current()
    }

    /// Override the motion scheme for a subtree — Compose's
    /// `CompositionLocalProvider(LocalMotionScheme provides MotionScheme.expressive())`, which is how an
    /// application switches every animated component at once. The six springs a component asks for
    /// (`SpringSpec::spatial_*`/`effects_*`) resolve through here, so no component names its own numbers.
    pub fn with_motion_scheme(
        scheme: MotionScheme,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        LOCAL_MOTION_SCHEME.provides(scheme, || content(ctx));
    }

    /// 在子树中覆盖 Material Typography token，不改变颜色或布局方向。
    pub fn with_typography(typography: Typography, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        LOCAL_TYPOGRAPHY.provides(typography, || content(ctx));
    }

    /// 在子树中覆盖内容色（对标 Compose `CompositionLocalProvider(LocalContentColor)`）
    pub fn with_content_color(color: Color, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        LOCAL_CONTENT_COLOR.provides(color, || content(ctx));
    }

    /// 在子树中开关 tonal overlay（对标 Compose `LocalTonalElevationEnabled`）。关闭后子树内所有
    /// `Surface` 的 `tonal_elevation` 都不再上色（Compose `ColorScheme.kt:1556`）。
    pub fn with_tonal_elevation_enabled(
        enabled: bool,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        LOCAL_TONAL_ELEVATION_ENABLED.provides(enabled, || content(ctx));
    }

    /// 当前子树是否启用 tonal overlay（默认开——对标 Compose `LocalTonalElevationEnabled`）。
    pub(crate) fn tonal_elevation_enabled() -> bool {
        LOCAL_TONAL_ELEVATION_ENABLED.current()
    }

    /// 在子树中提供绝对 tonal elevation（对标 Compose `LocalAbsoluteTonalElevation provides
    /// absoluteElevation`，`Surface.kt:109`）。只有 `Surface` 自己会调——它提供的是"自己那份加上
    /// 父层已经提供的和"。
    pub(crate) fn with_absolute_tonal_elevation(
        elevation: f32,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        LOCAL_ABSOLUTE_TONAL_ELEVATION.provides(elevation, || content(ctx));
    }

    /// 父层累积下来的 tonal elevation（对标 Compose `LocalAbsoluteTonalElevation.current`）。
    pub(crate) fn absolute_tonal_elevation() -> f32 {
        LOCAL_ABSOLUTE_TONAL_ELEVATION.current()
    }

    /// 读取当前子树主题色。
    pub fn colors() -> ThemeColors {
        LOCAL_COLORS.current()
    }
    /// 读取当前 Material Typography token。
    pub fn typography() -> Typography {
        LOCAL_TYPOGRAPHY.current()
    }

    /// 读取当前子树内容色（默认主题 on_surface）
    pub fn content_color() -> Color {
        LOCAL_CONTENT_COLOR.current()
    }
}

