use crate::utils::lerp;

/**
 * A class containing a value that changes with the contrast level.
 *
 * Usually represents the contrast requirements for a dynamic color on its
 * background. The four values correspond to values for contrast levels -1.0,
 * 0.0, 0.5, and 1.0, respectively.
 */
#[derive(Debug, Copy, Clone)]
pub struct ContrastCurve {
    low: f64,
    normal: f64,
    medium: f64,
    high: f64,
}

impl ContrastCurve {
    /**
     * Creates a `ContrastCurve` object.
     *
     * @param low Value for contrast level -1.0
     * @param normal Value for contrast level 0.0
     * @param medium Value for contrast level 0.5
     * @param high Value for contrast level 1.0
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
     * Returns the value at a given contrast level.
     *
     * @param contrastLevel The contrast level. 0.0 is the default (normal); -1.0
     *     is the lowest; 1.0 is the highest.
     * @return The value. For contrast ratios, a number between 1.0 and 21.0.
     */
    pub fn get(&self, contrast_level: f64) -> f64 {
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
