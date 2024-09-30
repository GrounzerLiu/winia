use crate::hct::Cam;
use crate::hct::Hct;
use crate::hct::viewing_conditions::{DEFAULT_VIEWING_CONDITIONS, ViewingConditions};
use crate::dislike::fix_if_disliked;
use crate::dynamic_color::{ContrastCurve, DynamicColor, foreground_tone, ToneDeltaPair, TonePolarity};
use crate::scheme::{DynamicScheme, Variant};
use crate::utils::{lstar_from_y, signum, Vec3};

fn is_fidelity(scheme: &DynamicScheme) -> bool {
    scheme.variant() == Variant::Fidelity ||
        scheme.variant() == Variant::Content
}

fn is_monochrome(scheme: &DynamicScheme) -> bool {
    scheme.variant() == Variant::Monochrome
}


fn xyz_in_viewing_conditions(cam: Cam, viewing_conditions: ViewingConditions) -> Vec3 {
    let alpha = if cam.chroma == 0.0 || cam.j == 0.0 {
        0.0
    } else {
        cam.chroma / (cam.j / 100.0).sqrt()
    };

    let t = (alpha / (1.64 - 0.29f64.powf(viewing_conditions.background_y_to_white_point_y).powf(0.73))).powf(1.0 / 0.9);
    let h_rad = cam.hue * std::f64::consts::PI / 180.0;

    let e_hue = 0.25 * (h_rad + 2.0).cos() + 3.8;
    let ac = viewing_conditions.aw * (cam.j / 100.0).powf(1.0 / viewing_conditions.c / viewing_conditions.z);
    let p1 = e_hue * (50000.0 / 13.0) * viewing_conditions.n_c * viewing_conditions.ncb;

    let p2 = ac / viewing_conditions.nbb;

    let h_sin = h_rad.sin();
    let h_cos = h_rad.cos();

    let gama = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
    let a = gama * h_cos;
    let b = gama * h_sin;
    let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
    let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
    let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;

    let r_c_base = 0.0_f64.max((27.12 * r_a.abs()) / (400.0 - r_a.abs()));
    let r_c = signum(r_a) as f64 * (100.0 / viewing_conditions.fl) * r_c_base.powf(1.0 / 0.42);
    let g_c_base = 0.0_f64.max((27.12 * g_a.abs()) / (400.0 - g_a.abs()));
    let g_c = signum(g_a) as f64 * (100.0 / viewing_conditions.fl) * g_c_base.powf(1.0 / 0.42);
    let b_c_base = 0.0_f64.max((27.12 * b_a.abs()) / (400.0 - b_a.abs()));
    let b_c = signum(b_a) as f64 * (100.0 / viewing_conditions.fl) * b_c_base.powf(1.0 / 0.42);

    let r_f = r_c / viewing_conditions.rgb_d[0];
    let g_f = g_c / viewing_conditions.rgb_d[1];
    let b_f = b_c / viewing_conditions.rgb_d[2];

    let x = 1.86206786 * r_f - 1.01125463 * g_f + 0.14918677 * b_f;
    let y = 0.38752654 * r_f + 0.62144744 * g_f - 0.00897398 * b_f;
    let z = -0.01584150 * r_f - 0.03412294 * g_f + 1.04996444 * b_f;

    Vec3::new(x, y, z)
}

fn in_viewing_conditions(hct: Hct, vc: ViewingConditions) -> Hct {
    // 1. Use CAM16 to find XYZ coordinates of color in specified VC.
    let cam16 = Cam::from_argb(hct.to_argb());
    let viewed_in_vc = xyz_in_viewing_conditions(cam16, vc);

    // 2. Create CAM16 of those XYZ coordinates in default VC.
    let recast_in_vc =
        Cam::from_xyz_and_viewing_conditions(viewed_in_vc.a, viewed_in_vc.b,
                                             viewed_in_vc.c, &DEFAULT_VIEWING_CONDITIONS);

    // 3. Create HCT from:
    // - CAM16 using default VC with XYZ coordinates in specified VC.
    // - L* converted from Y in XYZ coordinates in specified VC.
    Hct::from_hct(recast_in_vc.hue, recast_in_vc.chroma, lstar_from_y(viewed_in_vc.b))
}

fn find_desired_chroma_by_tone(hue: f64, chroma: f64, tone: f64,
                               by_decreasing_tone: bool) -> f64 {
    let mut answer = tone;

    let mut closest_to_chroma = Hct::from_hct(hue, chroma, tone);
    if closest_to_chroma.chroma() < chroma {
        let mut chroma_peak = closest_to_chroma.chroma();
        while closest_to_chroma.chroma() < chroma {
            answer += if by_decreasing_tone { -1.0 } else { 1.0 };
            let potential_solution = Hct::from_hct(hue, chroma, answer);
            if chroma_peak > potential_solution.chroma() {
                break;
            }
            if (potential_solution.chroma() - chroma).abs() < 0.4 {
                break;
            }

            let potential_delta = (potential_solution.chroma() - chroma).abs();
            let current_delta = (closest_to_chroma.chroma() - chroma).abs();
            if potential_delta < current_delta {
                closest_to_chroma = potential_solution;
            }
            chroma_peak = chroma_peak.max(potential_solution.chroma());
        }
    }

    answer
}

const CONTENT_ACCENT_TONE_DELTA: f64 = 15.0;

fn highest_surface(scheme: &DynamicScheme) -> DynamicColor {
    if scheme.is_dark() {
        surface_bright()
    } else {
        surface_dim()
    }
}

// Compatibility Keys Colors for Android
pub fn primary_palette_key_color() -> DynamicColor {
    DynamicColor::from_palette(
        "primary_palette_key_color",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| s.primary_palette().key_color().tone(),
    )
}

pub fn secondary_palette_key_color() -> DynamicColor {
    DynamicColor::from_palette(
        "secondary_palette_key_color",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme| s.secondary_palette().key_color().tone(),
    )
}

pub fn tertiary_palette_key_color() -> DynamicColor {
    DynamicColor::from_palette(
        "tertiary_palette_key_color",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme| s.tertiary_palette().key_color().tone(),
    )
}

pub fn neutral_palette_key_color() -> DynamicColor {
    DynamicColor::from_palette(
        "neutral_palette_key_color",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| s.neutral_palette().key_color().tone(),
    )
}

pub fn neutral_variant_palette_key_color() -> DynamicColor {
    DynamicColor::from_palette(
        "neutral_variant_palette_key_color",
        |s: &DynamicScheme| s.neutral_variant_palette(),
        |s: &DynamicScheme| s.neutral_variant_palette().key_color().tone(),
    )
}

pub fn background() -> DynamicColor {
    DynamicColor::new(
        "background",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 6.0 } else { 98.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn on_background() -> DynamicColor {
    DynamicColor::new(
        "on_background",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 90.0 } else { 10.0 },
        false,
        Some(Box::new(|_| background())),
        None,
        Some(ContrastCurve::new(3.0, 3.0, 4.5, 7.0)),
        None,
    )
}

pub fn surface() -> DynamicColor {
    DynamicColor::new(
        "surface",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 6.0 } else { 98.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_dim() -> DynamicColor {
    DynamicColor::new(
        "surface_dim",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 6.0 } else { 87.0 },
        true,
        None,
        None,
        None,
        None,
    )
}


pub fn surface_bright() -> DynamicColor {
    DynamicColor::new(
        "surface_bright",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 24.0 } else { 98.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_container_lowest() -> DynamicColor {
    DynamicColor::new(
        "surface_container_lowest",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 4.0 } else { 100.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_container_low() -> DynamicColor {
    DynamicColor::new(
        "surface_container_low",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 10.0 } else { 96.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_container() -> DynamicColor {
    DynamicColor::new(
        "surface_container",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 12.0 } else { 94.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_container_high() -> DynamicColor {
    DynamicColor::new(
        "surface_container_high",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 17.0 } else { 92.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_container_highest() -> DynamicColor {
    DynamicColor::new(
        "surface_container_highest",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 22.0 } else { 90.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn on_surface() -> DynamicColor {
    DynamicColor::new(
        "on_surface",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 90.0 } else { 10.0 },
        false,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn surface_variant() -> DynamicColor {
    DynamicColor::new(
        "surface_variant",
        |s: &DynamicScheme| s.neutral_variant_palette(),
        |s: &DynamicScheme| if s.is_dark() { 30.0 } else { 90.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn on_surface_variant() -> DynamicColor {
    DynamicColor::new(
        "on_surface_variant",
        |s: &DynamicScheme| s.neutral_variant_palette(),
        |s: &DynamicScheme| if s.is_dark() { 80.0 } else { 30.0 },
        false,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        None,
    )
}

pub fn inverse_surface() -> DynamicColor {
    DynamicColor::new(
        "inverse_surface",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 90.0 } else { 20.0 },
        false,
        None,
        None,
        None,
        None,
    )
}

pub fn inverse_on_surface() -> DynamicColor {
    DynamicColor::new(
        "inverse_on_surface",
        |s: &DynamicScheme| s.neutral_palette(),
        |s: &DynamicScheme| if s.is_dark() { 20.0 } else { 95.0 },
        false,
        Some(Box::new(|_| inverse_surface())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn outline() -> DynamicColor {
    DynamicColor::new(
        "outline",
        |s: &DynamicScheme| s.neutral_variant_palette(),
        |s: &DynamicScheme| if s.is_dark() { 60.0 } else { 50.0 },
        false,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.5, 3.0, 4.5, 7.0)),
        None,
    )
}

pub fn outline_variant() -> DynamicColor {
    DynamicColor::new(
        "outline_variant",
        |s: &DynamicScheme| s.neutral_variant_palette(),
        |s: &DynamicScheme| if s.is_dark() { 30.0 } else { 80.0 },
        false,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        None,
    )
}


pub fn shadow() -> DynamicColor {
    DynamicColor::new(
        "shadow",
        |s: &DynamicScheme| s.neutral_palette(),
        |_| 0.0,
        false,
        None,
        None,
        None,
        None,
    )
}

pub fn scrim() -> DynamicColor {
    DynamicColor::new(
        "scrim",
        |s: &DynamicScheme| s.neutral_palette(),
        |_| 0.0,
        false,
        None,
        None,
        None,
        None,
    )
}

pub fn surface_tint() -> DynamicColor {
    DynamicColor::new(
        "surface_tint",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if s.is_dark() { 80.0 } else { 40.0 },
        true,
        None,
        None,
        None,
        None,
    )
}

pub fn primary() -> DynamicColor {
    DynamicColor::new(
        "primary",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) {
            if s.is_dark() { 100.0 } else { 0.0 }
        } else if s.is_dark() { 80.0 } else { 40.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                primary_container(),
                primary(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_primary() -> DynamicColor {
    DynamicColor::new(
        "on_primary",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) {
            if s.is_dark() { 10.0 } else { 90.0 }
        } else if s.is_dark() { 20.0 } else { 100.0 },
        false,
        Some(Box::new(|_| primary())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn primary_container() -> DynamicColor {
    DynamicColor::new(
        "primary_container",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme|
            if is_fidelity(s) {
                s.source_color_hct().tone()
            }else if is_monochrome(s) {
                if s.is_dark() { 85.0 } else { 25.0 }
            }else if s.is_dark() { 30.0 } else { 90.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                primary_container(),
                primary(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_primary_container() -> DynamicColor {
    DynamicColor::new(
        "on_primary_container",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) {
            if s.is_dark() { 0.0 } else { 100.0 }
        } else if s.is_dark() { 90.0 } else { 10.0 },
        false,
        Some(Box::new(|_| primary_container())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn inverse_primary() -> DynamicColor {
    DynamicColor::new(
        "inverse_primary",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if s.is_dark() { 40.0 } else { 80.0 },
        false,
        Some(Box::new(|_| inverse_surface())),
        None,
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        None,
    )
}

pub fn secondary() -> DynamicColor {
    DynamicColor::new(
        "secondary",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme| if s.is_dark() { 80.0 } else { 40.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                secondary_container(),
                secondary(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_secondary() -> DynamicColor {
    DynamicColor::new(
        "on_secondary",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme|
            if is_monochrome(s) {
                if s.is_dark() { 10.0 } else { 100.0 }
            } else if s.is_dark() { 20.0 } else { 100.0 },
        false,
        Some(Box::new(|_| secondary())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn secondary_container() -> DynamicColor {
    DynamicColor::new(
        "secondary_container",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme| {
            let initial_tone = if s.is_dark() { 30.0 } else { 90.0 };
            if is_monochrome(s) {
                if s.is_dark() { 30.0 } else { 85.0 }
            } else if !is_fidelity(s) {
                initial_tone
            } else {
                find_desired_chroma_by_tone(s.secondary_palette().hue(),
                                            s.secondary_palette().chroma(),
                                            initial_tone, !s.is_dark())
            }
        },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                secondary_container(),
                secondary(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_secondary_container() -> DynamicColor {
    DynamicColor::new(
        "on_secondary_container",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme|
            if !is_fidelity(s) {
                if s.is_dark() { 90.0 } else { 10.0 }
            } else {
                foreground_tone(secondary_container().tone.as_ref()(s), 4.5)
            },
        false,
        Some(Box::new(|_| secondary_container())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn tertiary() -> DynamicColor {
    DynamicColor::new(
        "tertiary",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme|
            if is_monochrome(s) {
                if s.is_dark() { 90.0 } else { 25.0 }
            } else if s.is_dark() { 80.0 } else { 40.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                tertiary_container(),
                tertiary(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_tertiary() -> DynamicColor {
    DynamicColor::new(
        "on_tertiary",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) {
            if s.is_dark() { 10.0 } else { 100.0 }
        } else if s.is_dark() { 20.0 } else { 100.0 },
        false,
        Some(Box::new(|_| tertiary())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn tertiary_container() -> DynamicColor {
    DynamicColor::new(
        "tertiary_container",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme|
            if is_monochrome(s) {
                if s.is_dark() { 60.0 } else { 49.0 }
            } else if !is_fidelity(s) {
                if s.is_dark() { 30.0 } else { 90.0 }
            } else {
                let proposed_hct =
                    Hct::from_argb(s.tertiary_palette().get(s.source_color_hct().tone()));
                fix_if_disliked(proposed_hct).tone()
            },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                tertiary_container(),
                tertiary(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_tertiary_container() -> DynamicColor {
    DynamicColor::new(
        "on_tertiary_container",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme|
            if is_monochrome(s) {
                if s.is_dark() { 0.0 } else { 100.0 }
            } else if !is_fidelity(s) {
                if s.is_dark() { 90.0 } else { 10.0 }
            } else {
                foreground_tone(tertiary_container().tone.as_ref()(s), 4.5)
            },
        false,
        Some(Box::new(|_| tertiary_container())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn error() -> DynamicColor {
    DynamicColor::new(
        "error",
        |s: &DynamicScheme| s.error_palette(),
        |s: &DynamicScheme| if s.is_dark() { 80.0 } else { 40.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                error_container(),
                error(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_error() -> DynamicColor {
    DynamicColor::new(
        "on_error",
        |s: &DynamicScheme| s.error_palette(),
        |s: &DynamicScheme| if s.is_dark() { 20.0 } else { 100.0 },
        false,
        Some(Box::new(|_| error())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn error_container() -> DynamicColor {
    DynamicColor::new(
        "error_container",
        |s: &DynamicScheme| s.error_palette(),
        |s: &DynamicScheme| if s.is_dark() { 30.0 } else { 90.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                error_container(),
                error(),
                15.0,
                TonePolarity::Nearer,
                false,
            )
        })),
    )
}

pub fn on_error_container() -> DynamicColor {
    DynamicColor::new(
        "on_error_container",
        |s: &DynamicScheme| s.error_palette(),
        |s: &DynamicScheme| if s.is_dark() { 90.0 } else { 10.0 },
        false,
        Some(Box::new(|_| error_container())),
        None,
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn primary_fixed() -> DynamicColor {
    DynamicColor::new(
        "primary_fixed",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 40.0 } else { 90.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                primary_fixed(),
                primary_fixed_dim(),
                10.0,
                TonePolarity::Lighter,
                true,
            )
        })),
    )
}

pub fn primary_fixed_dim() -> DynamicColor {
    DynamicColor::new(
        "primary_fixed_dim",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 30.0 } else { 80.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                primary_fixed(),
                primary_fixed_dim(),
                10.0,
                TonePolarity::Lighter,
                true,
            )
        })),
    )
}

pub fn on_primary_fixed() -> DynamicColor {
    DynamicColor::new(
        "on_primary_fixed",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 100.0 } else { 10.0 },
        false,
        Some(Box::new(|_| primary_fixed_dim())),
        Some(Box::new(|_| primary_fixed())),
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn on_primary_fixed_variant() -> DynamicColor {
    DynamicColor::new(
        "on_primary_fixed_variant",
        |s: &DynamicScheme| s.primary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 90.0 } else { 30.0 },
        false,
        Some(Box::new(|_| primary_fixed_dim())),
        Some(Box::new(|_| primary_fixed())),
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        None,
    )
}

pub fn secondary_fixed() -> DynamicColor {
    DynamicColor::new(
        "secondary_fixed",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 80.0 } else { 90.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                secondary_fixed(),
                secondary_fixed_dim(),
                10.0,
                TonePolarity::Lighter,
                true,
            )
        })),
    )
}

pub fn secondary_fixed_dim() -> DynamicColor {
    DynamicColor::new(
        "secondary_fixed_dim",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 70.0 } else { 80.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                secondary_fixed(),
                secondary_fixed_dim(),
                10.0,
                TonePolarity::Lighter,
                true,
            )
        })),
    )
}

pub fn on_secondary_fixed() -> DynamicColor {
    DynamicColor::new(
        "on_secondary_fixed",
        |s: &DynamicScheme| s.secondary_palette(),
        |_| 10.0,
        false,
        Some(Box::new(|_| secondary_fixed_dim())),
        Some(Box::new(|_| secondary_fixed())),
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn on_secondary_fixed_variant() -> DynamicColor {
    DynamicColor::new(
        "on_secondary_fixed_variant",
        |s: &DynamicScheme| s.secondary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 25.0 } else { 30.0 },
        false,
        Some(Box::new(|_| secondary_fixed_dim())),
        Some(Box::new(|_| secondary_fixed())),
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        None,
    )
}

pub fn tertiary_fixed() -> DynamicColor {
    DynamicColor::new(
        "tertiary_fixed",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 40.0 } else { 90.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                tertiary_fixed(),
                tertiary_fixed_dim(),
                10.0,
                TonePolarity::Lighter,
                true,
            )
        })),
    )
}

pub fn tertiary_fixed_dim() -> DynamicColor {
    DynamicColor::new(
        "tertiary_fixed_dim",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 30.0 } else { 80.0 },
        true,
        Some(Box::new(|s: &DynamicScheme| highest_surface(s))),
        None,
        Some(ContrastCurve::new(1.0, 1.0, 3.0, 7.0)),
        Some(Box::new(|_| {
            ToneDeltaPair::new(
                tertiary_fixed(),
                tertiary_fixed_dim(),
                10.0,
                TonePolarity::Lighter,
                true,
            )
        })),
    )
}

pub fn on_tertiary_fixed() -> DynamicColor {
    DynamicColor::new(
        "on_tertiary_fixed",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 100.0 } else { 10.0 },
        false,
        Some(Box::new(|_| tertiary_fixed_dim())),
        Some(Box::new(|_| tertiary_fixed())),
        Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0)),
        None,
    )
}

pub fn on_tertiary_fixed_variant() -> DynamicColor {
    DynamicColor::new(
        "on_tertiary_fixed_variant",
        |s: &DynamicScheme| s.tertiary_palette(),
        |s: &DynamicScheme| if is_monochrome(s) { 90.0 } else { 30.0 },
        false,
        Some(Box::new(|_| tertiary_fixed_dim())),
        Some(Box::new(|_| tertiary_fixed())),
        Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0)),
        None,
    )
}
