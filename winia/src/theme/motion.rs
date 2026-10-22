use super::*;

// ═══════════════════════════════════════════════════════════
// CompositionLocal
// ═══════════════════════════════════════════════════════════

pub(crate) static LOCAL_COLORS: LazyLock<CompositionLocal<ThemeColors>> = LazyLock::new(|| {
    CompositionLocal::new(|| ThemeColors::default_light())
});

pub(crate) static LOCAL_TYPOGRAPHY: LazyLock<CompositionLocal<Typography>> = LazyLock::new(|| {
    CompositionLocal::new(Typography::default)
});

/// 内容色（对标 Compose `LocalContentColor`）——Icon 等内容组件默认取
/// 当前子树内容色；`with_theme*` 子树内默认 on_surface，容器组件
/// （如 IconButton）可覆盖。未包裹主题时回退纯黑（与 Compose 默认一致）。
pub(crate) static LOCAL_CONTENT_COLOR: LazyLock<CompositionLocal<Color>> =
    LazyLock::new(|| CompositionLocal::new(|| Color::BLACK));

/// tonal overlay 总开关（对标 Compose `LocalTonalElevationEnabled`，`ColorScheme.kt:1556`）——
/// 默认开；`Surface` 据此决定 `tonal_elevation` 是否真的上色。
pub(crate) static LOCAL_TONAL_ELEVATION_ENABLED: LazyLock<CompositionLocal<bool>> =
    LazyLock::new(|| CompositionLocal::new(|| true));

/// 绝对 tonal elevation（对标 Compose `LocalAbsoluteTonalElevation`，`material3/Surface.kt:487`）——每层
/// `Surface` 把自己的 `tonal_elevation` 加到父层提供的值上再下传，tint 按这个**和**取值。Compose 四个
/// 重载都是 `LocalAbsoluteTonalElevation.current + tonalElevation`（`Surface.kt:106`、`:211`、`:317`、
/// `:424`），理由见 `Surface.kt:146-150`：a Surface never appears to have a lower elevation overlay than
/// its ancestors。默认 0。
///
/// ⚠ 与 Compose 的差别不在算术而在**失效语义**：Compose 的 `LocalAbsoluteTonalElevation` 是
/// `compositionLocalOf`（动态，读者会被失效重算）；winia 的 `CompositionLocal` 是线程局部槽栈，
/// `provides` 不通知读者（`core/composition_local.rs:150-163`），和既有的 `LOCAL_CONTENT_COLOR` 一样。
/// 所以一个被跳过（`GroupStatus::Skip`）的内层 `Surface` 会保留它上次组合时的绝对 elevation，而 Compose
/// 会重算。抬升祖先的 elevation 时才会碰到。
pub(crate) static LOCAL_ABSOLUTE_TONAL_ELEVATION: LazyLock<CompositionLocal<f32>> =
    LazyLock::new(|| CompositionLocal::new(|| 0.0));

/// The motion scheme in force (Compose's `LocalMotionScheme`, `material3/MotionScheme.kt`) — the six springs
/// a component asks its theme for instead of naming its own. A component never reads this directly: it asks
/// `SpringSpec::spatial_fast()` and friends, which resolve through here.
pub(crate) static LOCAL_MOTION_SCHEME: LazyLock<CompositionLocal<MotionScheme>> =
    LazyLock::new(|| CompositionLocal::new(MotionScheme::standard));

/// Compose's `MotionScheme` (`material3/MotionScheme.kt:96-162`, tokens in
/// `material3/tokens/StandardMotionTokens.kt:20-31` and `ExpressiveMotionTokens.kt:22-33`).
///
/// The point of the object is that a component asks for a ROLE — default/fast/slow, spatial/effects — rather
/// than for a spring, so an application switches `standard()` to `expressive()` in one place and every
/// animated component follows. winia had the same numbers as constants `SpringSpec::spatial_*`/
/// `effects_*` read directly; those now resolve through the scheme in force.
///
/// The two schemes differ only in their spatial tokens: expressive motion is softer and bouncier (damping
/// 0.8/0.6 against 0.9, stiffness 380/800/200 against 700/1400/300). The EFFECT springs are identical in
/// both — which is why a colour animation does not change when the scheme does.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionScheme {
    /// `MotionSchemeKeyTokens.DefaultSpatial` — position and size changes.
    pub default_spatial: crate::animation::SpringSpec,
    /// `FastSpatial` — a small, immediate move (a thumb, a slot).
    pub fast_spatial: crate::animation::SpringSpec,
    /// `SlowSpatial` — a large surface appearing or leaving.
    pub slow_spatial: crate::animation::SpringSpec,
    /// `DefaultEffects` — a colour, an alpha.
    pub default_effects: crate::animation::SpringSpec,
    /// `FastEffects`.
    pub fast_effects: crate::animation::SpringSpec,
    /// `SlowEffects`.
    pub slow_effects: crate::animation::SpringSpec,
}

impl MotionScheme {
    /// `StandardMotionTokens` (`tokens/StandardMotionTokens.kt:20-31`) — M3's default motion.
    pub fn standard() -> Self {
        use crate::animation::SpringSpec;
        let spatial = |damping_ratio: f32, stiffness: f32| SpringSpec {
            damping_ratio,
            stiffness,
            mass: 1.0,
            threshold: 0.01,
        };
        Self {
            default_spatial: spatial(0.9, 700.0),
            fast_spatial: spatial(0.9, 1400.0),
            slow_spatial: spatial(0.9, 300.0),
            default_effects: spatial(1.0, 1600.0),
            fast_effects: spatial(1.0, 3800.0),
            slow_effects: spatial(1.0, 800.0),
        }
    }

    /// `ExpressiveMotionTokens` (`tokens/ExpressiveMotionTokens.kt:22-33`).
    pub fn expressive() -> Self {
        use crate::animation::SpringSpec;
        let spatial = |damping_ratio: f32, stiffness: f32| SpringSpec {
            damping_ratio,
            stiffness,
            mass: 1.0,
            threshold: 0.01,
        };
        Self {
            default_spatial: spatial(0.8, 380.0),
            fast_spatial: spatial(0.6, 800.0),
            slow_spatial: spatial(0.8, 200.0),
            default_effects: spatial(1.0, 1600.0),
            fast_effects: spatial(1.0, 3800.0),
            slow_effects: spatial(1.0, 800.0),
        }
    }
}

impl Default for MotionScheme {
    fn default() -> Self {
        Self::standard()
    }
}

