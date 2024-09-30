use std::collections::HashMap;
use std::f64::consts::PI;
use crate::hct::Hct;
use crate::quantize::Lab;
use crate::utils::{sanitize_degrees_double, sanitize_degrees_int};

/**
 * Design utilities using color temperature theory.
 *
 * <p>Analogous colors, complementary color, and cache to efficiently, lazily,
 * generate data for calculations when needed.
 */
pub struct TemperatureCache {
    input: Hct,
    precomputed_complement: Option<Hct>,
    precomputed_hcts_by_temp: Option<Vec<Hct>>,
    precomputed_hcts_by_hue: Option<Vec<Hct>>,
    precomputed_temps_by_hct: Option<HashMap<Hct, f64>>,
}

impl TemperatureCache {
    /**
     * Create a cache that allows calculation of ex. complementary and analogous
     * colors.
     *
     * @param input Color to find complement/analogous colors of. Any colors will
     * have the same tone, and chroma as the input color, modulo any restrictions
     * due to the other hues having lower limits on chroma.
     */
    pub fn new(input: Hct) -> Self {
        Self {
            input,
            precomputed_complement: None,
            precomputed_hcts_by_temp: None,
            precomputed_hcts_by_hue: None,
            precomputed_temps_by_hct: None,
        }
    }

    /**
     * A color that complements the input color aesthetically.
     *
     * <p>In art, this is usually described as being across the color wheel.
     * History of this shows intent as a color that is just as cool-warm as the
     * input color is warm-cool.
     */
    pub fn get_complement(&mut self) -> Hct {
        if let Some(complement) = self.precomputed_complement {
            return complement;
        }

        let coldest_hue = self.get_coldest().hue();
        let coldest_temp = *self.get_temps_by_hct().get(&self.get_coldest()).unwrap();

        let warmest_hue = self.get_warmest().hue();
        let warmest_temp = *self.get_temps_by_hct().get(&self.get_warmest()).unwrap();
        let range = warmest_temp - coldest_temp;
        let start_hue_is_coldest_to_warmest = is_between(self.input.hue(), coldest_hue, warmest_hue);
        let start_hue = if start_hue_is_coldest_to_warmest { warmest_hue } else { coldest_hue };
        let end_hue = if start_hue_is_coldest_to_warmest { coldest_hue } else { warmest_hue };
        let direction_of_rotation = 1.0;
        let mut smallest_error = 1000.0;
        let mut answer = *self.get_hcts_by_hue().get(self.input.hue().round() as usize).unwrap();

        let complement_relative_temp = 1.0 - self.get_relative_temperature(self.input);
        // Find the color in the other section, closest to the inverse percentile
        // of the input color. This is the complement.
        for hue_addend in 0..=360 {
            let hue =
                sanitize_degrees_double(start_hue + direction_of_rotation * hue_addend as f64);
            if !is_between(hue, start_hue, end_hue) {
                continue;
            }
            let possible_answer = *self.get_hcts_by_hue().get(hue.round() as usize).unwrap();
            let relative_temp =
                (self.get_temps_by_hct().get(&possible_answer).unwrap() - coldest_temp) / range;
            let error = (complement_relative_temp - relative_temp).abs();
            if error < smallest_error {
                smallest_error = error;
                answer = possible_answer;
            }
        }
        self.precomputed_complement = Some(answer);
        answer
    }

    /**
     * 5 colors that pair well with the input color.
     *
     * <p>The colors are equidistant in temperature and adjacent in hue.
     */
    pub fn get_analogous_colors(&mut self) -> Vec<Hct> {
        self.get_analogous_colors_by_divisions(5, 12)
    }

    /**
     * A set of colors with differing hues, equidistant in temperature.
     *
     * <p>In art, this is usually described as a set of 5 colors on a color wheel
     * divided into 12 sections. This method allows provision of either of those
     * values.
     *
     * <p>Behavior is undefined when count or divisions is 0. When divisions <
     * count, colors repeat.
     *
     * [count] The number of colors to return, includes the input color.
     * [divisions] The number of divisions on the color wheel.
     */
    pub fn get_analogous_colors_by_divisions(&mut self, count: usize, divisions: usize) -> Vec<Hct> {
        // The starting hue is the hue of the input color.
        let start_hue = self.input.hue().round() as usize;
        let start_hct = self.get_hcts_by_hue()[start_hue];
        let mut last_temp = self.get_relative_temperature(start_hct);

        let mut all_colors = Vec::new();
        all_colors.push(start_hct);

        let mut absolute_total_temp_delta = 0.0;
        for i in 0..360{
            let hue = sanitize_degrees_int((start_hue + i) as i32);
            let hct = self.get_hcts_by_hue()[hue as usize];
            let temp = self.get_relative_temperature(hct);
            let temp_delta = (temp - last_temp).abs();
            last_temp = temp;
            absolute_total_temp_delta += temp_delta;
        }

        let mut hue_addend = 1;
        let temp_step = absolute_total_temp_delta / divisions as f64;
        let mut total_temp_delta = 0.0;
        last_temp = self.get_relative_temperature(start_hct);
        while all_colors.len() < divisions {
            let hue = sanitize_degrees_int((start_hue + hue_addend) as i32);
            let hct = self.get_hcts_by_hue()[hue as usize];
            let temp = self.get_relative_temperature(hct);
            let temp_delta = (temp - last_temp).abs();
            total_temp_delta += temp_delta;

            let mut desired_total_temp_delta_for_index = all_colors.len() as f64 * temp_step;
            let mut index_satisfied =
                total_temp_delta >= desired_total_temp_delta_for_index;
            let mut index_addend = 1;
            // Keep adding this hue to the answers until its temperature is
            // insufficient. This ensures consistent behavior when there aren't
            // `divisions` discrete steps between 0 and 360 in hue with `temp_step`
            // delta in temperature between them.
            //
            // For example, white and black have no analogues: there are no other
            // colors at T100/T0. Therefore, they should just be added to the array
            // as answers.
            while index_satisfied &&
                all_colors.len() < divisions {
                all_colors.push(hct);
                desired_total_temp_delta_for_index =
                    (all_colors.len() + index_addend) as f64 * temp_step;
                index_satisfied = total_temp_delta >= desired_total_temp_delta_for_index;
                index_addend += 1;
            }
            last_temp = temp;
            hue_addend += 1;

            if hue_addend > 360 {
                while all_colors.len() < divisions {
                    all_colors.push(hct);
                }
                break;
            }
        }

        let mut answers = Vec::new();
        answers.push(self.input);

        let ccw_count = ((count as f64 - 1.0) / 2.0).floor() as usize;
        for i in 1..(ccw_count + 1) {
            let mut index:i32 = 0 - i as i32;
            while index < 0 {
                index += all_colors.len() as i32;
            }
            if index >= all_colors.len() as i32 {
                index %= all_colors.len() as i32;
            }
            answers.insert(0, all_colors[index as usize]);
        }

        let cw_count = count - ccw_count - 1;
        for i in 1..(cw_count + 1) {
            let mut index:i32 = i as i32;
            while index < 0 {
                index += all_colors.len() as i32;
            }
            if index >= all_colors.len() as i32 {
                index %= all_colors.len() as i32;
            }
            answers.push(all_colors[index as usize]);
        }

        answers
    }

    /**
     * Temperature relative to all colors with the same chroma and tone.
     *
     * @param hct HCT to find the relative temperature of.
     * @return Value on a scale from 0 to 1.
     */
    pub fn get_relative_temperature(&mut self, hct: Hct) -> f64 {
        let range = self.get_temps_by_hct().get(&self.get_warmest()).unwrap() -
            self.get_temps_by_hct().get(&self.get_coldest()).unwrap();
        let difference_from_coldest = self.get_temps_by_hct().get(&hct).unwrap() -
            self.get_temps_by_hct().get(&self.get_coldest()).unwrap();
        // Handle when there's no difference in temperature between warmest and
        // coldest: for example, at T100, only one color is available, white.
        if range == 0. {
            return 0.5;
        }
        difference_from_coldest / range
    }


    /** Coldest color with same chroma and tone as input. */
    fn get_coldest(&mut self) -> Hct {
        *self.get_hcts_by_temp().get(0).unwrap()
    }

    /** Warmest color with same chroma and tone as input. */
    fn get_warmest(&mut self) -> Hct {
        *self.get_hcts_by_temp().get(self.get_hcts_by_temp().len() - 1).unwrap()
    }


    /**
     * HCTs for all colors with the same chroma/tone as the input.
     *
     * <p>Sorted by hue, ex. index 0 is hue 0.
     */
    fn get_hcts_by_hue(&mut self) -> Vec<Hct> {
        if let Some(hcts) = &self.precomputed_hcts_by_hue {
            return hcts.clone();
        }

        let mut hcts = Vec::new();
        for hue in 0..=360 {
            let color_at_hue = Hct::from_hct(hue as f64, self.input.chroma(), self.input.tone());
            hcts.push(color_at_hue);
        }
        self.precomputed_hcts_by_hue = Some(hcts.clone());
        hcts
    }

    /**
     * HCTs for all colors with the same chroma/tone as the input.
     *
     * <p>Sorted from coldest first to warmest last.
     */
    fn get_hcts_by_temp(&mut self) -> Vec<Hct> {
        if let Some(hcts) = &self.precomputed_hcts_by_temp {
            return hcts.clone();
        }

        let mut hcts = self.get_hcts_by_hue();
        hcts.push(self.input);
        let temps_by_hct = self.get_temps_by_hct();
        hcts.sort_by(|a, b| temps_by_hct.get(a).unwrap().partial_cmp(temps_by_hct.get(b).unwrap()).unwrap());

        self.precomputed_hcts_by_temp = Some(hcts.clone());
        hcts
    }

    /** Keys of HCTs in GetHctsByTemp, values of raw temperature. */
    fn get_temps_by_hct(&mut self) -> HashMap<Hct, f64> {
        if let Some(temps_by_hct) = &self.precomputed_temps_by_hct {
            return temps_by_hct.clone();
        }

        let mut  all_hcts = self.get_hcts_by_hue();
        all_hcts.push(self.input);

        let mut temperatures_by_hct = HashMap::new();
        for hct in &all_hcts {
            temperatures_by_hct.insert(*hct, raw_temperature(*hct));
        }

        self.precomputed_temps_by_hct = Some(temperatures_by_hct.clone());
        temperatures_by_hct
    }
}

/** Determines if an angle is between two other angles, rotating clockwise. */
fn is_between(angle: f64, a: f64, b: f64) -> bool {
    if a < b {
        return a <= angle && angle <= b;
    }
    a <= angle || angle <= b
}

/**
 * Value representing cool-warm factor of a color. Values below 0 are
 * considered cool, above, warm.
 *
 * <p>Color science has researched emotion and harmony, which art uses to
 * select colors. Warm-cool is the foundation of analogous and complementary
 * colors. See: - Li-Chen Ou's Chapter 19 in Handbook of Color Psychology
 * (2015). - Josef Albers' Interaction of Color chapters 19 and 21.
 *
 * <p>Implementation of Ou, Woodcock and Wright's algorithm, which uses
 * Lab/LCH color space. Return value has these properties:<br>
 * - Values below 0 are cool, above 0 are warm.<br>
 * - Lower bound: -9.66. Chroma is infinite. Assuming max of Lab chroma
 * 130.<br>
 * - Upper bound: 8.61. Chroma is infinite. Assuming max of Lab chroma 130.
 */
pub fn raw_temperature(color: Hct) -> f64 {
    let lab = Lab::from_argb(color.to_argb());
    let hue = sanitize_degrees_double(lab.b.atan2(lab.a) * 180.0 / PI);
    let chroma = lab.a.hypot(lab.b);
    -0.5 + 0.02 * chroma.powf(1.07) * (sanitize_degrees_double(hue - 50.0) * PI / 180.0).cos()
}