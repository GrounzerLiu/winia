use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use crate::hct::{argb_from_hcl, Cam};
use crate::hct::Hct;
use crate::utils::Argb;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct TonalPalette {
    hue: f64,
    chroma: f64,
    key_color: Hct,
}

impl TonalPalette {
    pub fn new(hue: f64, chroma: f64, key_color: Hct) -> Self {
        Self {
            hue,
            chroma,
            key_color,
        }
    }

    pub fn from_argb(argb: Argb) -> Self {
        let cam = Cam::from_argb(argb);
        Self {
            hue: cam.hue,
            chroma: cam.chroma,
            key_color: KeyColor::new(cam.hue, cam.chroma).create(),
        }
    }

    pub fn from_hct(hct: Hct)->Self{
        Self{
            hue: hct.get_hue(),
            chroma: hct.get_chroma(),
            key_color: hct
        }
    }

    pub fn from_hue_and_chroma(hue: f64, chroma: f64)->Self{
        Self{
            hue,
            chroma,
            key_color: KeyColor::new(hue, chroma).create()
        }
    }
    /**
     * Returns the color for a given tone in this palette.
     *
     * @param tone 0.0 <= tone <= 100.0
     * @return a color as an integer, in ARGB format.
     */
    pub fn get(&self, tone: f64) -> Argb {
        argb_from_hcl(self.hue, self.chroma, tone)
    }

    pub fn hue(&self)->f64{
        self.hue
    }

    pub fn chroma(&self)->f64{
        self.chroma
    }

    pub fn key_color(&self)->Hct{
        self.key_color
    }
}


#[derive(Clone, Copy)]
struct F64Key(f64);
impl PartialEq for F64Key{
    fn eq(&self, other: &Self)->bool{
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for F64Key{}

impl Hash for F64Key{
    fn hash<H: Hasher>(&self, state: &mut H){
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for F64Key{
    fn partial_cmp(&self, other: &Self)->Option<Ordering>{
        self.0.partial_cmp(&other.0)
    }
}

#[derive(Clone)]
pub struct KeyColor{
    max_chroma_value: f64,
    hue: f64,
    requested_chroma: f64,
    chroma_cache: HashMap<i32, f64>
}

impl KeyColor{
    pub fn new(hue: f64, requested_chroma: f64)->Self{
        Self{
            max_chroma_value: 200.0,
            hue,
            requested_chroma,
            chroma_cache: HashMap::new()
        }
    }

    pub fn create(&mut self) ->Hct{
        // Pivot around T50 because T50 has the most chroma available, on
        // average. Thus it is most likely to have a direct answer.
        let pivot_tone = 50;
        let tone_step_size = 1;
        // Epsilon to accept values slightly higher than the requested chroma.
        let epsilon = 0.01;

        // Binary search to find the tone that can provide a chroma that is closest
        // to the requested chroma.
        let mut lower_tone = 0;
        let mut upper_tone = 100;
        while (lower_tone < upper_tone) {
            let mid_tone = (lower_tone + upper_tone) / 2;
            let is_ascending =
                self.max_chroma(mid_tone) < self.max_chroma(mid_tone + tone_step_size);
            let sufficient_chroma =
                self.max_chroma(mid_tone) >= self.requested_chroma - epsilon;
            let max = self.max_chroma(mid_tone);
            if sufficient_chroma {
                // Either range [lower_tone, mid_tone] or [mid_tone, upper_tone] has
                // the answer, so search in the range that is closer the pivot tone.
                if (lower_tone - pivot_tone).abs() < (upper_tone - pivot_tone).abs() {
                    upper_tone = mid_tone;
                } else {
                    if lower_tone == mid_tone {
                        return Hct::from_hct(self.hue, self.requested_chroma, lower_tone as f64);
                    }
                    lower_tone = mid_tone;
                }
            } else {
                // As there's no sufficient chroma in the mid_tone, follow the direction
                // to the chroma peak.
                if is_ascending {
                    lower_tone = mid_tone + tone_step_size;
                } else {
                    // Keep mid_tone for potential chroma peak.
                    upper_tone = mid_tone;
                }
            }
        }

        Hct::from_hct(self.hue, self.requested_chroma, lower_tone as f64)
    }

    pub fn max_chroma(&mut self, tone: i32) -> f64 {
        let it = self.chroma_cache.get(&(tone));
        if let Some(value) = it {
            return *value;
        }

        let chroma = Hct::from_hct(self.hue, self.max_chroma_value, tone as f64).get_chroma();
        self.chroma_cache.insert(tone, chroma);
        chroma
    }
}