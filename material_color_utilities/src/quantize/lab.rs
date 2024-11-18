use std::fmt::{Display, Formatter};
use crate::utils::{Argb, argb_from_rgb, delinearized, linearized, WHITE_POINT_D65};

#[derive(Copy, Clone, Default)]
pub struct Lab {
    pub(crate) l: f64,
    pub(crate) a: f64,
    pub(crate) b: f64,
}

impl Lab {
    pub fn new(l: f64, a: f64, b: f64) -> Self {
        Self { l, a, b }
    }
    pub fn from_argb(argb: Argb) -> Self {
        let red = (argb & 0x00ff0000) >> 16;
        let green = (argb & 0x0000ff00) >> 8;
        let blue = argb & 0x000000ff;
        let red_l = linearized(red);
        let green_l = linearized(green);
        let blue_l = linearized(blue);
        let x = 0.41233895 * red_l + 0.35762064 * green_l + 0.18051042 * blue_l;
        let y = 0.2126 * red_l + 0.7152 * green_l + 0.0722 * blue_l;
        let z = 0.01932141 * red_l + 0.11916382 * green_l + 0.95034478 * blue_l;
        let y_normalized = y / WHITE_POINT_D65[1];
        let e = 216.0 / 24389.0;
        let kappa = 24389.0 / 27.0;
        let fy=if y_normalized > e {
            y_normalized.powf(1.0 / 3.0)
        } else {
            (kappa * y_normalized + 16.0) / 116.0
        };

        let x_normalized = x / WHITE_POINT_D65[0];
        let fx=if x_normalized > e {
            x_normalized.powf(1.0 / 3.0)
        } else {
            (kappa * x_normalized + 16.0) / 116.0
        };

        let z_normalized = z / WHITE_POINT_D65[2];
        let fz=if z_normalized > e {
            z_normalized.powf(1.0 / 3.0)
        } else {
            (kappa * z_normalized + 16.0) / 116.0
        };

        let l = 116.0 * fy - 16.0;
        let a = 500.0 * (fx - fy);
        let b = 200.0 * (fy - fz);
        Self { l, a, b }
    }

    pub fn to_argb(&self) -> Argb {
        let e = 216.0 / 24389.0;
        let kappa = 24389.0 / 27.0;
        let ke = 8.0;

        let fy = (self.l + 16.0) / 116.0;
        let fx = (self.a / 500.0) + fy;
        let fz = fy - (self.b / 200.0);
        let fx3 = fx * fx * fx;
        let x_normalized = if fx3 > e { fx3 } else { (116.0 * fx - 16.0) / kappa };
        let y_normalized = if self.l > ke { fy * fy * fy } else { self.l / kappa };
        let fz3 = fz * fz * fz;
        let z_normalized = if fz3 > e { fz3 } else { (116.0 * fz - 16.0) / kappa };
        let x = x_normalized * WHITE_POINT_D65[0];
        let y = y_normalized * WHITE_POINT_D65[1];
        let z = z_normalized * WHITE_POINT_D65[2];

        // letFromXyz
        let r_l = 3.2406 * x - 1.5372 * y - 0.4986 * z;
        let g_l = -0.9689 * x + 1.8758 * y + 0.0415 * z;
        let b_l = 0.0557 * x - 0.2040 * y + 1.0570 * z;

        let red = delinearized(r_l);
        let green = delinearized(g_l);
        let blue = delinearized(b_l);

        argb_from_rgb(red, green, blue)
    }

    pub fn delta_e(&self, lab: Lab) -> f64 {
        let d_l = self.l - lab.l;
        let d_a = self.a - lab.a;
        let d_b = self.b - lab.b;
        (d_l * d_l) + (d_a * d_a) + (d_b * d_b)
    }


}

impl Display for Lab{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f,"Lab: L* {} a* {} b* {}",self.l,self.a,self.b)
    }
}

impl From<Argb> for Lab {
    fn from(argb: Argb) -> Self {
        Self::from_argb(argb)
    }
}

impl From<Lab> for Argb {
    fn from(lab: Lab) -> Self {
        lab.to_argb()
    }
}
