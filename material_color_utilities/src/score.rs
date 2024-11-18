use crate::utils::{diff_degrees, sanitize_degrees_int, Argb};
use std::cmp::Ordering;
use crate::hct::Hct;

const K_TARGET_CHROMA: f64 = 48.0;
// A1 Chroma
const K_WEIGHT_PROPORTION: f64 = 0.7;
const K_WEIGHT_CHROMA_ABOVE: f64 = 0.3;
const K_WEIGHT_CHROMA_BELOW: f64 = 0.1;
const K_CUTOFF_CHROMA: f64 = 5.0;
const K_CUTOFF_EXCITED_PROPORTION: f64 = 0.01;

/**
 * Default options for ranking colors based on usage counts.
 * `desired`: is the max count of the colors returned.
 * `fallback_color_argb`: Is the default color that should be used if no
 *                        other colors are suitable.
 * `filter`: controls if the resulting colors should be filtered to not include
 *         hues that are not used often enough, and colors that are effectively
 *         grayscale.
 */
pub struct ScoreOptions {
    pub desired: usize,
    pub fallback_color_argb: Argb,
    pub filter: bool,
}

impl Default for ScoreOptions {
    fn default() -> Self {
        ScoreOptions {
            desired: 4,                      // 4 colors matches the Android wallpaper picker.
            fallback_color_argb: 0xff4285f4, // Google Blue.
            filter: true,                    // Avoid unsuitable colors.
        }
    }
}

fn compare_scored_hct(a: &(Hct, f64), b: &(Hct, f64)) -> Ordering {
    if a.1 < b.1 {
        Ordering::Greater
    } else if a.1 > b.1 {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

/**
 * Given a map with keys of colors and values of how often the color appears,
 * rank the colors based on suitability for being used for a UI theme.
 *
 * The list returned is of length <= [desired]. The recommended color is the
 * first item, the least suitable is the last. There will always be at least
 * one color returned. If all the input colors were not suitable for a theme,
 * a default fallback color will be provided, Google Blue, or supplied fallback
 * color. The default number of colors returned is 4, simply because that's the
 * # of colors display in Android 12's wallpaper picker.
 */
pub fn ranked_suggestions<I: Iterator<Item = (Argb, u32)>>(
    argb_to_population: I,
    options: &ScoreOptions,
) -> Vec<Argb> {
    let mut color_hct:Vec<Hct> =Vec::new();
    let mut hue_population:[i32;360]=[0;360];
    let mut population_sum:f64=0.0;

    for (argb, population) in argb_to_population {
        let hct = Hct::from_argb(argb);
        color_hct.push(hct);
        let hue = hct.hue().floor() as usize;
        hue_population[hue] += population as i32;
        population_sum += population as f64;
    }

    let mut hue_excited_proportions:[f64;360]=[0.0;360];
    for hue in 0_i32..360_i32 {
        let proportion = hue_population[hue as  usize] as f64 / population_sum;
        for i in hue-14..hue+16 {
            let neighbor_hue =sanitize_degrees_int(i) as usize;
            hue_excited_proportions[neighbor_hue]+=proportion;
        }
    }

    let mut scored_hct:Vec<(Hct,f64)>=Vec::new();
    for hct in color_hct{
        let hue=sanitize_degrees_int(hct.hue().round() as i32);
        let proportion=hue_excited_proportions[hue as usize];
        if options.filter&&(hct.chroma()<K_CUTOFF_CHROMA||proportion<K_CUTOFF_EXCITED_PROPORTION) {
            continue;
        }

        let proportion_score = proportion * 100.0 * K_WEIGHT_PROPORTION;
        let chroma_weight = if hct.chroma() < K_TARGET_CHROMA{
            K_WEIGHT_CHROMA_BELOW
        }
        else{
            K_WEIGHT_CHROMA_ABOVE
        };

        let chroma_score = (hct.chroma()- K_TARGET_CHROMA) * chroma_weight;
        let score = proportion_score + chroma_score;
        scored_hct.push((hct,score));
    }

    scored_hct.sort_by(compare_scored_hct);

    let mut chosen_colors:Vec<Hct> =Vec::new();
    for difference_degrees in (15..=90).rev() {
        chosen_colors.clear();
        for (hct,_) in &scored_hct{

            if !chosen_colors.iter().any(|chosen_hct| diff_degrees(hct.hue(), chosen_hct.hue()) < difference_degrees as f64) {
                chosen_colors.push(*hct);
            }

            if chosen_colors.len()>=options.desired {
                break;
            }
        }

        if chosen_colors.len()>=options.desired {
            break;
        }
    }

    let mut colors:Vec<Argb>=Vec::new();
    if chosen_colors.is_empty(){
        colors.push(options.fallback_color_argb);
    }

    colors.extend(chosen_colors.iter().map(|hct| hct.to_argb()));

    colors
}
