use crate::contrast::{darker, darker_unsafe, lighter, lighter_unsafe, ratio_of_tones};
use crate::dynamic_color::{ContrastCurve, ToneDeltaPair, TonePolarity};
use crate::palettes::TonalPalette;
use crate::scheme::DynamicScheme;

type DoubleFunction = Box<dyn Fn(&DynamicScheme) -> f64>;

fn safe_call<T, U>(f: Option<Box<dyn Fn(&T) -> Option<U>>>, x: &T) -> Option<U> {
    if let Some(f) = f {
        f(x)
    } else {
        None
    }
}

fn safe_call_clean_result<T, U>(f: Option<Box<dyn Fn(T) -> U>>, x: T) -> Option<U> {
    if let Some(f) = f {
        Some(f(x))
    } else {
        None
    }
}

/**
 * Given a background tone, find a foreground tone, while ensuring they reach
 * a contrast ratio that is as close to [ratio] as possible.
 *
 * [bgTone] Tone in HCT. Range is 0 to 100, undefined behavior when it falls
 * outside that range.
 * [ratio] The contrast ratio desired between [bgTone] and the return value.
 */
pub fn foreground_tone(bg_tone: f64, ratio: f64) -> f64 {
    let lighter_tone = lighter_unsafe(bg_tone, ratio);
    let darker_tone = darker_unsafe(bg_tone, ratio);
    let lighter_ratio = ratio_of_tones(lighter_tone, bg_tone);
    let darker_ratio = ratio_of_tones(darker_tone, bg_tone);
    let prefer_lighter = tone_prefers_light_foreground(bg_tone);

    if prefer_lighter {
        let negligible_difference = (lighter_ratio - darker_ratio).abs() < 0.1 && lighter_ratio < ratio && darker_ratio < ratio;
        if lighter_ratio >= ratio || lighter_ratio >= darker_ratio || negligible_difference {
            lighter_tone
        } else {
            darker_tone
        }
    } else if darker_ratio >= ratio || darker_ratio >= lighter_ratio {
        darker_tone
    } else {
        lighter_tone
    }
}

/**
 * Adjust a tone such that white has 4.5 contrast, if the tone is
 * reasonably close to supporting it.
 */
pub fn enable_light_foreground(tone: f64) -> f64 {
    if tone_prefers_light_foreground(tone) && !tone_allows_light_foreground(tone) {
        49.0
    } else {
        tone
    }
}

/**
 * Returns whether [tone] prefers a light foreground.
 *
 * People prefer white foregrounds on ~T60-70. Observed over time, and also
 * by Andrew Somers during research for APCA.
 *
 * T60 used as to create the smallest discontinuity possible when skipping
 * down to T49 in order to ensure light foregrounds.
 *
 * Since `tertiaryContainer` in dark monochrome scheme requires a tone of
 * 60, it should not be adjusted. Therefore, 60 is excluded here.
 */
pub fn tone_prefers_light_foreground(tone: f64) -> bool {
    tone.round() < 60.0
}

/**
 * Returns whether [tone] can reach a contrast ratio of 4.5 with a lighter
 * color.
 */
pub fn tone_allows_light_foreground(tone: f64) -> bool {
    tone.round() <= 49.0
}

/**
 * @param name_ The name of the dynamic color.
 * @param palette_ Function that provides a TonalPalette given
 * DynamicScheme. A TonalPalette is defined by a hue and chroma, so this
 * replaces the need to specify hue/chroma. By providing a tonal palette, when
 * contrast adjustments are made, intended chroma can be preserved.
 * @param tone_ Function that provides a tone given DynamicScheme.
 * @param is_background_ Whether this dynamic color is a background, with
 * some other color as the foreground.
 * @param background_ The background of the dynamic color (as a function of a
 *     `DynamicScheme`), if it exists.
 * @param second_background_ A second background of the dynamic color (as a
 *     function of a `DynamicScheme`), if it
 * exists.
 * @param contrast_curve_ A `ContrastCurve` object specifying how its contrast
 * against its background should behave in various contrast levels options.
 * @param tone_delta_pair_ A `ToneDeltaPair` object specifying a tone delta
 * constraint between two colors. One of them must be the color being
 * constructed.
 */
pub struct DynamicColor {
    name: String,
    pub palette: Box<dyn Fn(&DynamicScheme) -> TonalPalette>,
    pub tone: Box<dyn Fn(&DynamicScheme) -> f64>,
    is_background: bool,
    background: Option<Box<dyn Fn(&DynamicScheme) -> DynamicColor>>,
    second_background: Option<Box<dyn Fn(&DynamicScheme) -> DynamicColor>>,
    contrast_curve: Option<ContrastCurve>,
    tone_delta_pair: Option<Box<dyn Fn(&DynamicScheme) -> ToneDeltaPair>>,
}

impl DynamicColor {
    /** The default constructor. */
    pub fn new(
        name: impl Into<String>,
        palette: impl Fn(&DynamicScheme) -> TonalPalette + 'static,
        tone: impl Fn(&DynamicScheme) -> f64 + 'static,
        is_background: bool,
        background: Option<Box<dyn Fn(&DynamicScheme) -> DynamicColor>>,
        second_background: Option<Box<dyn Fn(&DynamicScheme) -> DynamicColor>>,
        contrast_curve: Option<ContrastCurve>,
        tone_delta_pair: Option<Box<dyn Fn(&DynamicScheme) -> ToneDeltaPair>>,
    ) -> Self {
        Self {
            name:name.into(),
            palette: Box::new(palette),
            tone: Box::new(tone),
            is_background,
            background,
            second_background,
            contrast_curve,
            tone_delta_pair,
        }
    }

    pub fn from_palette(
        name: impl Into<String>,
        palette: impl  Fn(&DynamicScheme) -> TonalPalette + 'static,
        tone: impl Fn(&DynamicScheme) -> f64 + 'static,
    ) -> Self {
        Self::new(
            name,
            palette,
            tone,
            false,
            None,
            None,
            None,
            None,
        )
    }

    pub fn get_argb(&self, scheme: &DynamicScheme) -> u32 {
        self.palette.as_ref()(scheme).get(self.get_tone(scheme))
    }

    pub fn get_hct(&self, scheme: &DynamicScheme) -> u32 {
        self.get_argb(scheme)
    }

    pub fn get_tone(&self, scheme: &DynamicScheme) -> f64 {
        let decreasing_contrast = scheme.contrast_level() < 0.0;

        // Case 1: dual foreground, pair of colors with delta constraint.
        return if let Some(tone_delta_pair) = &self.tone_delta_pair {
            let tone_delta_pair = tone_delta_pair(scheme);
            let role_a = tone_delta_pair.role_a;
            let role_b = tone_delta_pair.role_b;
            let delta = tone_delta_pair.delta;
            let polarity = tone_delta_pair.polarity;
            let stay_together = tone_delta_pair.stay_together;

            let bg = self.background.as_ref().unwrap()(scheme);
            let bg_tone = bg.get_tone(scheme);

            let a_is_nearer =
                polarity == TonePolarity::Nearer ||
                    (polarity == TonePolarity::Lighter && !scheme.is_dark()) ||
                    (polarity == TonePolarity::Darker && scheme.is_dark());
            let nearer = if a_is_nearer { &role_a } else { &role_b };
            let farther = if a_is_nearer { &role_b } else { &role_a };
            let am_nearer = self.name == nearer.name;
            let expansion_dir = if scheme.is_dark() { 1.0 } else { -1.0 };

            // 1st round: solve to min, each
            let n_contrast = nearer.contrast_curve.unwrap().get_contrast(scheme.contrast_level());
            let f_contrast = farther.contrast_curve.unwrap().get_contrast(scheme.contrast_level());

            // If a color is good enough, it is not adjusted.
            // Initial and adjusted tones for `nearer`
            let n_initial_tone = nearer.tone.as_ref()(scheme);
            let mut n_tone = if ratio_of_tones(bg_tone, n_initial_tone) >= n_contrast { n_initial_tone } else { foreground_tone(bg_tone, n_contrast) };

            // Initial and adjusted tones for `farther`
            let f_initial_tone = farther.tone.as_ref()(scheme);
            let mut f_tone = if ratio_of_tones(bg_tone, f_initial_tone) >= f_contrast { f_initial_tone } else { foreground_tone(bg_tone, f_contrast) };

            if decreasing_contrast {
                // If decreasing contrast, adjust color to the "bare minimum"
                // that satisfies contrast.
                n_tone = foreground_tone(bg_tone, n_contrast);
                f_tone = foreground_tone(bg_tone, f_contrast);
            }

            if (f_tone - n_tone) * expansion_dir >= delta {
                // Good! Tones satisfy the constraint; no change needed.
            } else {
                // 2nd round: expand farther to match delta.
                f_tone = (n_tone + delta * expansion_dir).clamp(0.0, 100.0);
                if (f_tone - n_tone) * expansion_dir >= delta {
                    // Good! Tones now satisfy the constraint; no change needed.
                } else {
                    // 3rd round: contract nearer to match delta.
                    n_tone = (f_tone - delta * expansion_dir).clamp(0.0, 100.0);
                }
            }

            // Avoids the 50-59 awkward zone.
            if (50.0..60.0).contains(&n_tone) {
                // If `nearer` is in the awkward zone, move it away, together with
                // `farther`.
                if expansion_dir > 0.0 {
                    n_tone = 60.0;
                    f_tone = f_tone.max(n_tone + delta * expansion_dir);
                } else {
                    n_tone = 49.0;
                    f_tone = f_tone.min(n_tone + delta * expansion_dir);
                }
            } else if (50.0..60.0).contains(&f_tone) {
                if stay_together {
                    // Fixes both, to avoid two colors on opposite sides of the "awkward
                    // zone".
                    if expansion_dir > 0.0 {
                        n_tone = 60.0;
                        f_tone = f_tone.max(n_tone + delta * expansion_dir);
                    } else {
                        n_tone = 49.0;
                        f_tone = f_tone.min(n_tone + delta * expansion_dir);
                    }
                } else {
                    // Not required to stay together; fixes just one.
                    if expansion_dir > 0.0 {
                        f_tone = 60.0;
                    } else {
                        f_tone = 49.0;
                    }
                }
            }

            // Returns `n_tone` if this color is `nearer`, otherwise `f_tone`.
            if am_nearer { n_tone } else { f_tone }
        } else {
            // Case 2: No contrast pair; just solve for itself.
            let mut answer = self.tone.as_ref()(scheme);

            if self.background.is_none() {
                return answer;  // No adjustment for colors with no background.
            }

            let bg_tone = self.background.as_ref().unwrap()(scheme).get_tone(scheme);

            let desired_ratio = self.contrast_curve.as_ref().unwrap().get_contrast(scheme.contrast_level());

            if ratio_of_tones(bg_tone, answer) >= desired_ratio {
                // Don't "improve" what's good enough.
            } else {
                // Rough improvement.
                answer = foreground_tone(bg_tone, desired_ratio);
            }

            if decreasing_contrast {
                answer = foreground_tone(bg_tone, desired_ratio);
            }

            if self.is_background && 50.0 <= answer && answer < 60.0 {
                // Must adjust
                if ratio_of_tones(49.0, bg_tone) >= desired_ratio {
                    answer = 49.0;
                } else {
                    answer = 60.0;
                }
            }

            if let Some(second_background) = &self.second_background {
                // Case 3: Adjust for dual backgrounds.

                let bg_tone_1 = self.background.as_ref().unwrap()(scheme).get_tone(scheme);
                let bg_tone_2 = second_background(scheme).get_tone(scheme);

                let upper = bg_tone_1.max(bg_tone_2);
                let lower = bg_tone_1.min(bg_tone_2);

                if ratio_of_tones(upper, answer) >= desired_ratio &&
                    ratio_of_tones(lower, answer) >= desired_ratio {
                    return answer;
                }

                // The darkest light tone that satisfies the desired ratio,
                // or -1 if such ratio cannot be reached.
                let light_option = lighter(upper, desired_ratio);

                // The lightest dark tone that satisfies the desired ratio,
                // or -1 if such ratio cannot be reached.
                let dark_option = darker(lower, desired_ratio);

                // Tones suitable for the foreground.
                let mut availables = vec![];
                if light_option != -1.0 {
                    availables.push(light_option);
                }
                if dark_option != -1.0 {
                    availables.push(dark_option);
                }

                let prefers_light = tone_prefers_light_foreground(bg_tone_1) ||
                    tone_prefers_light_foreground(bg_tone_2);
                if prefers_light {
                    return if availables.is_empty() { 100.0 } else { light_option };
                }
                if availables.len() == 1 {
                    return availables[0];
                }
                return if availables.is_empty() { 0.0 } else { dark_option };
            }

            answer
        };
    }
}