#![allow(dead_code)]

use crate::hct::hct_solver::solve_to_int;
use crate::hct::viewing_conditions::{DEFAULT_VIEWING_CONDITIONS, ViewingConditions};
use crate::utils::{Argb, argb_from_rgb, delinearized, K_PI, linearized, sanitize_degrees_double, signum};

#[derive(Clone, Copy, Default, Debug)]
pub struct Cam {
    pub hue: f64,
    pub chroma: f64,
    pub j: f64,
    pub q: f64,
    pub m: f64,
    pub s: f64,

    pub jstar: f64,
    pub astar: f64,
    pub bstar: f64,
}

impl Cam {
    pub fn from_argb(argb: Argb) -> Self {
        Self::from_int_and_viewing_conditions(argb, &DEFAULT_VIEWING_CONDITIONS)
    }

    pub fn from_ucs_and_viewing_conditions(jstar: f64, astar: f64, bstar: f64,
                                           viewing_conditions: ViewingConditions) -> Self {
        let a = astar;
        let b = bstar;
        let m = (a * a + b * b).sqrt();
        let m_2 = ((m * 0.0228).exp() - 1.0) / 0.0228;
        let c = m_2 / viewing_conditions.fl_root;
        let mut h = b.atan2(a) * (180.0 / K_PI);
        if h < 0.0 {
            h += 360.0;
        }
        let j = jstar / (1.0 - (jstar - 100.0) * 0.007);
        Self::from_jch_and_viewing_conditions(j, c, h, viewing_conditions)
    }

    pub fn from_int_and_viewing_conditions(argb: Argb, viewing_conditions: &ViewingConditions) -> Self {
        let red = (argb as i32 & 0x00ff0000) >> 16;
        let green = (argb as i32  & 0x0000ff00) >> 8;
        let blue = argb as i32  & 0x000000ff;
        let red_l = linearized(red as u32);
        let green_l = linearized(green as u32);
        let blue_l = linearized(blue as u32);
        let x = 0.41233895 * red_l + 0.35762064 * green_l + 0.18051042 * blue_l;
        let y = 0.2126 * red_l + 0.7152 * green_l + 0.0722 * blue_l;
        let z = 0.01932141 * red_l + 0.11916382 * green_l + 0.95034478 * blue_l;

        // Convert XYZ to 'cone'/'rgb' responses
        let r_c = 0.401288 * x + 0.650173 * y - 0.051461 * z;
        let g_c = -0.250268 * x + 1.204414 * y + 0.045854 * z;
        let b_c = -0.002079 * x + 0.048952 * y + 0.953127 * z;

        // Discount illuminant.
        let r_d = viewing_conditions.rgb_d[0] * r_c;
        let g_d = viewing_conditions.rgb_d[1] * g_c;
        let b_d = viewing_conditions.rgb_d[2] * b_c;

        // Chromatic adaptation.
        let r_af = (viewing_conditions.fl * r_d.abs() / 100.0).powf(0.42);
        let g_af = (viewing_conditions.fl * g_d.abs() / 100.0).powf(0.42);
        let b_af = (viewing_conditions.fl * b_d.abs() / 100.0).powf(0.42);
        let r_a = signum(r_d) as f64 * 400.0 * r_af / (r_af + 27.13);
        let g_a = signum(g_d) as f64 * 400.0 * g_af / (g_af + 27.13);
        let b_a = signum(b_d) as f64 * 400.0 * b_af / (b_af + 27.13);

        // Redness-greenness
        let a = (11.0 * r_a + -12.0 * g_a + b_a) / 11.0;
        let b = (r_a + g_a - 2.0 * b_a) / 9.0;
        let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
        let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;

        let radians = b.atan2(a);
        let degrees = radians * 180.0 / K_PI;
        let hue = sanitize_degrees_double(degrees);
        let hue_radians = hue * K_PI / 180.0;
        let ac = p2 * viewing_conditions.nbb;

        let j = 100.0 * (ac / viewing_conditions.aw).powf(viewing_conditions.c * viewing_conditions.z);
        let q = (4.0 / viewing_conditions.c) * (j / 100.0).sqrt() *
            (viewing_conditions.aw + 4.0) * viewing_conditions.fl_root;
        let hue_prime = if hue < 20.14 { hue + 360.0 } else { hue };
        let e_hue = 0.25 * ((hue_prime * K_PI / 180.0 + 2.0).cos() + 3.8);
        let p1 =
            50000.0 / 13.0 * e_hue * viewing_conditions.n_c * viewing_conditions.ncb;
        let t = p1 * (a * a + b * b).sqrt() / (u + 0.305);
        let alpha =
            t.powf(0.9) *
                (1.64 - 0.29_f64.powf(viewing_conditions.background_y_to_white_point_y)).powf(0.73);
        let c = alpha * (j / 100.0).sqrt();
        let m = c * viewing_conditions.fl_root;
        let s = 50.0 * ((alpha * viewing_conditions.c) /
            (viewing_conditions.aw + 4.0)).sqrt();
        let jstar = (1.0 + 100.0 * 0.007) * j / (1.0 + 0.007 * j);
        let mstar = 1.0 / 0.0228 * (1.0 + 0.0228 * m).ln();
        let astar = mstar * hue_radians.cos();
        let bstar = mstar * hue_radians.sin();
        Cam { hue, chroma: c, j, q, m, s, jstar, astar, bstar }
    }

    pub fn from_jch_and_viewing_conditions(j: f64, c: f64, h: f64, viewing_conditions: ViewingConditions) -> Self {
        let q = (4.0 / viewing_conditions.c) * (j / 100.0).sqrt() *
            (viewing_conditions.aw + 4.0) * (viewing_conditions.fl_root);
        let m = c * viewing_conditions.fl_root;
        let alpha = c / (j / 100.0).sqrt();
        let s = 50.0 * ((alpha * viewing_conditions.c) /
            (viewing_conditions.aw + 4.0)).sqrt();
        let hue_radians = h * K_PI / 180.0;
        let jstar = (1.0 + 100.0 * 0.007) * j / (1.0 + 0.007 * j);
        let mstar = 1.0 / 0.0228 * (1.0 + 0.0228 * m).ln();
        let astar = mstar * hue_radians.cos();
        let bstar = mstar * hue_radians.sin();
        Cam { hue: h, chroma: c, j, q, m, s, jstar, astar, bstar }
    }


    pub fn from_xyz_and_viewing_conditions(x: f64, y: f64, z: f64, viewing_conditions: &ViewingConditions) -> Cam {
        // Convert XYZ to 'cone'/'rgb' responses
        let r_c = 0.401288 * x + 0.650173 * y - 0.051461 * z;
        let g_c = -0.250268 * x + 1.204414 * y + 0.045854 * z;
        let b_c = -0.002079 * x + 0.048952 * y + 0.953127 * z;

        // Discount illuminant.
        let r_d = viewing_conditions.rgb_d[0] * r_c;
        let g_d = viewing_conditions.rgb_d[1] * g_c;
        let b_d = viewing_conditions.rgb_d[2] * b_c;

        // Chromatic adaptation.
        let r_af = (viewing_conditions.fl * r_d.abs() / 100.0).powf(0.42);
        let g_af = (viewing_conditions.fl * g_d.abs() / 100.0).powf(0.42);
        let b_af = (viewing_conditions.fl * b_d.abs() / 100.0).powf(0.42);
        let r_a = signum(r_d) as f64 * 400.0 * r_af / (r_af + 27.13);
        let g_a = signum(g_d) as f64 * 400.0 * g_af / (g_af + 27.13);
        let b_a = signum(b_d) as f64 * 400.0 * b_af / (b_af + 27.13);

        // Redness-greenness
        let a = (11.0 * r_a + -12.0 * g_a + b_a) / 11.0;
        let b = (r_a + g_a - 2.0 * b_a) / 9.0;
        let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
        let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;

        let radians = b.powf(a);
        let degrees = radians * 180.0 / K_PI;
        let hue = sanitize_degrees_double(degrees);
        let hue_radians = hue * K_PI / 180.0;
        let ac = p2 * viewing_conditions.nbb;

        let j = 100.0 * (ac / viewing_conditions.aw).powf(
            viewing_conditions.c * viewing_conditions.z);
        let q = (4.0 / viewing_conditions.c) * (j / 100.0).sqrt() *
            (viewing_conditions.aw + 4.0) * viewing_conditions.fl_root;
        let hue_prime = if hue < 20.14
        { hue + 360.0 } else { hue };
        let e_hue = 0.25 * ((hue_prime * K_PI / 180.0 + 2.0).cos() + 3.8);
        let p1 =
            50000.0 / 13.0 * e_hue * viewing_conditions.n_c * viewing_conditions.ncb;
        let t = p1 * (a * a + b * b).sqrt() / (u + 0.305);
        let alpha =
            t.powf(0.9) *
                (1.64 - 0.29_f64.powf(viewing_conditions.background_y_to_white_point_y)).powf(
                    0.73);
        let c = alpha * (j / 100.0).sqrt();
        let m = c * viewing_conditions.fl_root;
        let s = 50.0 * ((alpha * viewing_conditions.c) /
            (viewing_conditions.aw + 4.0)).sqrt();
        let jstar = (1.0 + 100.0 * 0.007) * j / (1.0 + 0.007 * j);
        let mstar = 1.0 / 0.0228 * (1.0 + 0.0228 * m).ln();
        let astar = mstar * hue_radians.cos();
        let bstar = mstar * hue_radians.sin();
        Cam { hue, chroma: c, j, q, m, s, jstar, astar, bstar }
    }

    pub fn distance_to(&self, other: Cam) -> f64 {
        let d_j = self.jstar - other.jstar;
        let d_a = self.astar - other.astar;
        let d_b = self.bstar - other.bstar;
        let d_e_prime = (d_j * d_j + d_a * d_a + d_b * d_b).sqrt();
        1.41 * d_e_prime.powf(0.63)
    }

    pub fn to_argb(&self) -> Argb {
        argb_from_cam(*self)
    }
}


fn argb_from_cam_and_viewing_conditions(cam: Cam,
                                        viewing_conditions: ViewingConditions) -> Argb {
    let alpha = if cam.chroma == 0.0 || cam.j == 0.0
    { 0.0 } else { cam.chroma / (cam.j / 100.0).sqrt() };
    let t = (alpha / (1.64 - 0.29_f64.powf(viewing_conditions.background_y_to_white_point_y)).powf(0.73)).powf(1.0 / 0.9);
    let h_rad = cam.hue * K_PI / 180.0;
    let e_hue = 0.25 * ((h_rad + 2.0).cos() + 3.8);
    let ac =
        viewing_conditions.aw *
            (cam.j / 100.0).powf(1.0 / viewing_conditions.c / viewing_conditions.z);
    let p1 = e_hue * (50000.0 / 13.0) * viewing_conditions.n_c *
        viewing_conditions.ncb;
    let p2 = ac / viewing_conditions.nbb;
    let h_sin = h_rad.sin();
    let h_cos = h_rad.cos();
    let gamma = 23.0 * (p2 + 0.305) * t /
        (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
    let a = gamma * h_cos;
    let b = gamma * h_sin;
    let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
    let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
    let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;

    let r_c_base = 0_f64.max((27.13 * r_a.abs()) / (400.0 - r_a.abs()));
    let r_c = signum(r_a) as f64 * (100.0 / viewing_conditions.fl) * r_c_base.powf(1.0 / 0.42);
    let g_c_base = 0_f64.max((27.13 * g_a.abs()) / (400.0 - g_a.abs()));
    let g_c = signum(g_a) as f64 * (100.0 / viewing_conditions.fl) * g_c_base.powf(1.0 / 0.42);
    let b_c_base = 0_f64.max((27.13 * b_a.abs()) / (400.0 - b_a.abs()));
    let b_c = signum(b_a) as f64 * (100.0 / viewing_conditions.fl) * b_c_base.powf(1.0 / 0.42);
    let r_x = r_c / viewing_conditions.rgb_d[0];
    let g_x = g_c / viewing_conditions.rgb_d[1];
    let b_x = b_c / viewing_conditions.rgb_d[2];
    let x = 1.86206786 * r_x - 1.01125463 * g_x + 0.14918677 * b_x;
    let y = 0.38752654 * r_x + 0.62144744 * g_x - 0.00897398 * b_x;
    let z = -0.01584150 * r_x - 0.03412294 * g_x + 1.04996444 * b_x;

// intFromXyz
    let r_l = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g_l = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let b_l = 0.0557 * x - 0.2040 * y + 1.0570 * z;

    let red = delinearized(r_l);
    let green = delinearized(g_l);
    let blue = delinearized(b_l);

    argb_from_rgb(red, green, blue)
}

fn argb_from_cam(cam: Cam) -> Argb {
    argb_from_cam_and_viewing_conditions(cam, DEFAULT_VIEWING_CONDITIONS)
}

impl From<Argb> for Cam {
    fn from(argb: Argb) -> Self {
        Self::from_argb(argb)
    }
}

impl From<Cam> for Argb {
    fn from(cam: Cam) -> Self {
        argb_from_cam(cam)
    }
}

// fn cam_distance(a: Cam, b: Cam) -> f64 {
//     let d_j = a.jstar - b.jstar;
//     let d_a = a.astar - b.astar;
//     let d_b = a.bstar - b.bstar;
//     let d_e_prime = (d_j * d_j + d_a * d_a + d_b * d_b).sqrt();
//     1.41 * d_e_prime.powf(0.63)
// }


pub fn argb_from_hcl(hue: f64, chroma: f64, lstar: f64) -> Argb {
    solve_to_int(hue, chroma, lstar)
}

