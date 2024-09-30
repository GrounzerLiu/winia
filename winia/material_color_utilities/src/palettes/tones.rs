use crate::hct::{argb_from_hcl, Cam};
use crate::hct::Hct;
use crate::utils::Argb;

#[derive(Clone, Copy, Default)]
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
            key_color: create_key_color(cam.hue, cam.chroma),
        }
    }

    pub fn from_hct(hct: Hct)->Self{
        Self{
            hue: hct.hue(),
            chroma: hct.chroma(),
            key_color: hct
        }
    }

    pub fn from_hue_and_chroma(hue: f64, chroma: f64)->Self{
        Self{
            hue,
            chroma,
            key_color: create_key_color(hue, chroma)
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

fn create_key_color(hue: f64, chroma: f64) -> Hct {
    let start_tone = 50.0;
    let mut smallest_delta_hct = Hct::from_hct(hue, chroma, start_tone);
    let mut smallest_delta = ((smallest_delta_hct.chroma() - chroma) as i32).abs() as f64;
    // Starting from T50, check T+/-delta to see if they match the requested
    // chroma.
    //
    // Starts from T50 because T50 has the most chroma available, on
    // average. Thus it is most likely to have a direct answer and minimize
    // iteration.
    for delta in 1..50 {
        // Termination condition rounding instead of minimizing delta to avoid
        // case where requested chroma is 16.51, and the closest chroma is 16.49.
        // Error is minimized, but when rounded and displayed, requested chroma
        // is 17, key color's chroma is 16.
        if chroma.round() == smallest_delta_hct.chroma().round() {
            return smallest_delta_hct;
        }
        let hct_add = Hct::from_hct(hue, chroma, start_tone + delta as f64);
        let hct_add_delta = ((hct_add.chroma() - chroma)as i32).abs() as f64;
        if hct_add_delta < smallest_delta {
            smallest_delta = hct_add_delta;
            smallest_delta_hct = hct_add;
        }
        let hct_subtract = Hct::from_hct(hue, chroma, start_tone - delta as f64);
        let hct_subtract_delta = ((hct_subtract.chroma() - chroma) as i32).abs() as f64;
        if hct_subtract_delta < smallest_delta {
            smallest_delta = hct_subtract_delta;
            smallest_delta_hct = hct_subtract;
        }
    }
    smallest_delta_hct
}