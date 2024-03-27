use crate::hct::Hct;

/**
 * Checks and/or fixes universally disliked colors.
 *
 * Color science studies of color preference indicate universal distaste for
 * dark yellow-greens, and also show this is correlated to distate for
 * biological waste and rotting food.
 *
 * See Palmer and Schloss, 2010 or Schloss and Palmer's Chapter 21 in Handbook
 * of Color Psychology (2015).
 */

/**
 * @return whether the color is disliked.
 *
 * Disliked is defined as a dark yellow-green that is not neutral.
 * @param hct The color to be tested.
 */
pub fn is_disliked(hct: Hct) -> bool {
    let rounded_hue = hct.hue().round();
    let hue_passes = rounded_hue >= 90.0 && rounded_hue <= 111.0;
    let chroma_passes = hct.chroma().round() > 16.0;
    let tone_passes = hct.tone().round() < 65.0;
    hue_passes && chroma_passes && tone_passes
}

/**
 * If a color is disliked, lightens it to make it likable.
 *
 * The original color is not modified.
 *
 * @param hct The color to be tested (and fixed, if needed).
 * @return The original color if it is not disliked; otherwise, the fixed
 *     color.
 */
// Hct FixIfDisliked(Hct hct);
pub fn fix_if_disliked(hct: Hct) -> Hct {
    if is_disliked(hct) {
        return Hct::from_hct(hct.hue(), hct.chroma(), 70.0);
    }
    hct
}