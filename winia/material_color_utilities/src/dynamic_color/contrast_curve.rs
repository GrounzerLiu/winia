use crate::utils::lerp;

/**
 * Documents a constraint between two DynamicColors, in which their tones must
 * have a certain distance from each other.
 */
#[derive(Debug, Copy, Clone)]
pub struct ContrastCurve {
    low: f64,
    normal: f64,
    medium: f64,
    high: f64,
}

impl ContrastCurve{
    /**
     * Creates a `ContrastCurve` object.
     *
     * @param low Contrast requirement for contrast level -1.0
     * @param normal Contrast requirement for contrast level 0.0
     * @param medium Contrast requirement for contrast level 0.5
     * @param high Contrast requirement for contrast level 1.0
     */
    pub fn new(low: f64, normal: f64, medium: f64, high: f64) -> Self {
        Self {
            low,
            normal,
            medium,
            high,
        }
    }
    /**
     * Returns the contrast ratio at a given contrast level.
     *
     * @param contrastLevel The contrast level. 0.0 is the default (normal);
     * -1.0 is the lowest; 1.0 is the highest.
     * @return The contrast ratio, a number between 1.0 and 21.0.
     */
    pub fn get_contrast(&self, contrast_level: f64) -> f64 {
        if contrast_level <= -1.0 {
            self.low
        } else if contrast_level < 0.0 {
            lerp(self.low, self.normal, (contrast_level - (-1.0)) / 1.0)
        } else if contrast_level < 0.5 {
            lerp(self.normal, self.medium, (contrast_level - 0.0) / 0.5)
        } else if contrast_level < 1.0 {
            lerp(self.medium, self.high, (contrast_level - 0.5) / 0.5)
        } else {
            self.high
        }
    }
}

