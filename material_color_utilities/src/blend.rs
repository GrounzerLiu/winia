use crate::hct::Cam;
use crate::hct::Hct;
use crate::hct::viewing_conditions::DEFAULT_VIEWING_CONDITIONS;
use crate::utils::{Argb, diff_degrees, rotation_direction, sanitize_degrees_double};

pub fn blend_harmonize(design_color: Argb, key_color: Argb) -> Argb {
    let mut from_hct = Hct::from_argb(design_color);
    let to_hct = Hct::from_argb(key_color);
    let difference_degrees = diff_degrees(from_hct.hue(), to_hct.hue());
    let rotation_degrees = (difference_degrees * 0.5).min(15.0);
    let output_hue = sanitize_degrees_double(
        from_hct.hue() +
            rotation_degrees *
                rotation_direction(from_hct.hue(), to_hct.hue()));
    from_hct.set_hue(output_hue);
    return from_hct.to_argb();
}

pub fn blend_hct_hue(from: Argb, to: Argb, amount: f64)->Argb {
    let ucs = blend_cam16ucs(from, to, amount);
    let ucs_hct=Hct::from_argb(ucs);
    let mut from_hct =Hct::from_argb(from);
    from_hct.set_hue(ucs_hct.hue());
    return from_hct.to_argb();
}

pub fn blend_cam16ucs(from: Argb, to: Argb, amount: f64) -> Argb {
    let from_cam = Cam::from_argb(from);
    let to_cam = Cam::from_argb(to);

    let a_j = from_cam.jstar;
    let a_a = from_cam.astar;
    let a_b = from_cam.bstar;

    let b_j = to_cam.jstar;
    let b_a = to_cam.astar;
    let b_b = to_cam.bstar;

    let jstar = a_j + (b_j - a_j) * amount;
    let astar = a_a + (b_a - a_a) * amount;
    let bstar = a_b + (b_b - a_b) * amount;

    Cam::from_ucs_and_viewing_conditions(jstar, astar, bstar,
                                         DEFAULT_VIEWING_CONDITIONS)
        .to_argb()
}