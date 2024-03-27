use crate::utils::{lstar_from_y, y_from_lstar};

/**Given a color and a contrast ratio to reach, the luminance of a color that
reaches that ratio with the color can be calculated. However, that luminance
may not contrast as desired, i.e. the contrast ratio of the input color
and the returned luminance may not reach the contrast ratio  asked for.

When the desired contrast ratio and the result contrast ratio differ by
more than this amount,  an error value should be returned, or the method
should be documented as 'unsafe', meaning, it will return a valid luminance
but that luminance may not meet the requested contrast ratio.

0.04 selected because it ensures the resulting ratio rounds to the
same tenth.*/

const  CONTRAST_RATIO_EPSILON:f64 = 0.04;

/**Color spaces that measure luminance, such as Y in XYZ, L* in L*a*b*,
or T in HCT, are known as  perceptual accurate color spaces.

To be displayed, they must gamut map to a "display space", one that has
a defined limit on the number of colors. Display spaces include sRGB,
more commonly understood as RGB/HSL/HSV/HSB.

Gamut mapping is undefined and not defined by the color space. Any
gamut mapping algorithm must choose how to sacrifice accuracy in hue,
saturation, and/or lightness.

A principled solution is to maintain lightness, thus maintaining
contrast/a11y, maintain hue, thus maintaining aesthetic intent, and reduce
chroma until the color is in gamut.

HCT chooses this solution, but, that doesn't mean it will _exactly_ matched
desired lightness, if only because RGB is quantized: RGB is expressed as
a set of integers: there may be an RGB color with, for example,
47.892 lightness, but not 47.891.

To allow for this inherent incompatibility between perceptually accurate
color spaces and display color spaces, methods that take a contrast ratio
and luminance, and return a luminance that reaches that contrast ratio for
the input luminance, purposefully darken/lighten their result such that
the desired contrast ratio will be reached even if inaccuracy is introduced.

0.4 is generous, ex. HCT requires much less delta. It was chosen because
it provides a rough guarantee that as long as a percetual color space
gamut maps lightness such that the resulting lightness rounds to the same
as the requested, the desired contrast ratio will be reached.*/
const LUMINANCE_GAMUT_MAP_TOLERANCE:f64 = 0.4;

fn ratio_of_ys(y1: f64, y2: f64) -> f64 {
    let lighter = if y1 > y2 { y1 } else { y2 };
    let darker = if lighter == y2 { y1 } else { y2 };
    (lighter + 5.0) / (darker + 5.0)
}

/**
 * @return a contrast ratio, which ranges from 1 to 21.
 * @param tone_a Tone between 0 and 100. Values outside will be clamped.
 * @param tone_b Tone between 0 and 100. Values outside will be clamped.
 */
pub fn ratio_of_tones(tone_a: f64, tone_b: f64) -> f64 {
    let tone_a = tone_a.max(0.0).min(100.0);
    let tone_b = tone_b.max(0.0).min(100.0);
    ratio_of_ys(y_from_lstar(tone_a), y_from_lstar(tone_b))
}

/**
 * @return a tone >= [tone] that ensures [ratio].
 * Return value is between 0 and 100.
 * Returns -1 if [ratio] cannot be achieved with [tone].
 *
 * @param tone Tone return value must contrast with.
 * Range is 0 to 100. Invalid values will result in -1 being returned.
 * @param ratio Contrast ratio of return value and [tone].
 * Range is 1 to 21, invalid values have undefined behavior.
 */
//double Lighter(double tone, double ratio);
pub fn lighter(tone: f64, ratio: f64) -> f64{
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }

    let dark_y = y_from_lstar(tone);
    let light_y = ratio * (dark_y + 5.0) - 5.0;
    let real_contrast = ratio_of_ys(light_y, dark_y);
    let delta = (real_contrast - ratio).abs();
    if real_contrast < ratio && delta > CONTRAST_RATIO_EPSILON {
        return -1.0;
    }

    // ensure gamut mapping, which requires a 'range' on tone, will still result
    // the correct ratio by darkening slightly.
    let value = lstar_from_y(light_y) + LUMINANCE_GAMUT_MAP_TOLERANCE;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

/**
 * @return a tone <= [tone] that ensures [ratio].
 * Return value is between 0 and 100.
 * Returns -1 if [ratio] cannot be achieved with [tone].
 *
 * @param tone Tone return value must contrast with.
 * Range is 0 to 100. Invalid values will result in -1 being returned.
 * @param ratio Contrast ratio of return value and [tone].
 * Range is 1 to 21, invalid values have undefined behavior.
 */
//double Darker(double tone, double ratio);
pub fn darker(tone:f64,ratio:f64)->f64{
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }

    let light_y = y_from_lstar(tone);
    let dark_y = ((light_y + 5.0) / ratio) - 5.0;
    let real_contrast = ratio_of_ys(light_y, dark_y);

    let delta = (real_contrast - ratio).abs();
    if real_contrast < ratio && delta > CONTRAST_RATIO_EPSILON {
        return -1.0;
    }

    // ensure gamut mapping, which requires a 'range' on tone, will still result
    // the correct ratio by darkening slightly.
    let value = lstar_from_y(dark_y) - LUMINANCE_GAMUT_MAP_TOLERANCE;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

/**
 * @return a tone >= [tone] that ensures [ratio].
 * Return value is between 0 and 100.
 * Returns 100 if [ratio] cannot be achieved with [tone].
 *
 * This method is unsafe because the returned value is guaranteed to be in
 * bounds for tone, i.e. between 0 and 100. However, that value may not reach
 * the [ratio] with [tone]. For example, there is no color lighter than T100.
 *
 * @param tone Tone return value must contrast with.
 * Range is 0 to 100. Invalid values will result in 100 being returned.
 * @param ratio Desired contrast ratio of return value and tone parameter.
 * Range is 1 to 21, invalid values have undefined behavior.
 */
// double LighterUnsafe(double tone, double ratio);
pub fn lighter_unsafe(tone:f64,ratio:f64)->f64{
    let lighter_safe = lighter(tone, ratio);
    if lighter_safe<0.0{100.0}else{lighter_safe}
}

/**
 * @return a tone <= [tone] that ensures [ratio].
 * Return value is between 0 and 100.
 * Returns 0 if [ratio] cannot be achieved with [tone].
 *
 * This method is unsafe because the returned value is guaranteed to be in
 * bounds for tone, i.e. between 0 and 100. However, that value may not reach
 * the [ratio] with [tone]. For example, there is no color darker than T0.
 *
 * @param tone Tone return value must contrast with.
 * Range is 0 to 100. Invalid values will result in 0 being returned.
 * @param ratio Desired contrast ratio of return value and tone parameter.
 * Range is 1 to 21, invalid values have undefined behavior.
 */
// double DarkerUnsafe(double tone, double ratio);
pub fn darker_unsafe(tone:f64,ratio:f64)->f64{
    let darker_safe = darker(tone, ratio);
    if darker_safe<0.0{0.0}else{darker_safe}
}