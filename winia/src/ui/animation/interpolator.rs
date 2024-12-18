use std::cmp::Ordering;

mod tests {
    use bezier_rs::{Bezier, TValue};
    use crate::ui::animation::interpolator::{EaseOutCirc, Interpolator};

    #[test]
    fn test_ease_out_circ() {
        let ease_out_circ = EaseOutCirc::new();
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let eased_t = ease_out_circ.interpolate(t);
            println!("({}, {}),", t, eased_t);
        }
    }

    #[test]
    fn test_generate_interpolator() {
        generate_interpolator(0.075, 0.82, 0.165, 1.0);
        // generate_interpolator(0.47, 0.0, 0.745, 0.715);
    }

    fn generate_interpolator(x1: f32, y1: f32, x2: f32, y2: f32) {
        let bezier = Bezier::from_cubic_coordinates(0.0, 0.0, x1 as f64, y1 as f64, x2 as f64, y2 as f64, 1.0, 1.0);
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let point = bezier.evaluate(TValue::Parametric(t));
            print!("({}, {}),", point.x as f32, point.y as f32);
            if i % 4 == 3 {
                println!("");
            }
        }
    }
}

pub trait Interpolator {
    fn interpolate(&self, x: f32) -> f32;
}

pub struct Linear {}
impl Default for Linear {
    fn default() -> Self {
        Self::new()
    }
}

impl Linear {
    pub fn new() -> Self {
        Self {}
    }
}

impl Interpolator for Linear {
    fn interpolate(&self, x: f32) -> f32 {
        x
    }
}


pub struct EaseOutCirc {
    points: Vec<(f32, f32)>,
}

impl Default for EaseOutCirc {
    fn default() -> Self {
        Self::new()
    }
}

impl EaseOutCirc {
    pub fn new() -> Self {
        Self {
            points: vec![
                (0.0, 0.0), (0.00225523, 0.02440846), (0.0045238403, 0.04843568), (0.00681021, 0.07208442),
                (0.00911872, 0.09535744), (0.011453751, 0.1182575), (0.013819681, 0.14078736), (0.01622089, 0.16294979),
                (0.01866176, 0.18474752), (0.02114667, 0.20618334), (0.023680001, 0.22726), (0.026266132, 0.24798025),
                (0.028909441, 0.26834688), (0.03161431, 0.28836262), (0.034385122, 0.30803025), (0.037226252, 0.3273525),
                (0.04014208, 0.34633216), (0.04313699, 0.36497197), (0.046215363, 0.3832747), (0.049381573, 0.40124315),
                (0.052640002, 0.41888), (0.055995032, 0.43618807), (0.059451044, 0.4531701), (0.06301241, 0.4698288),
                (0.06668352, 0.48616704), (0.07046875, 0.5021875), (0.074372485, 0.51789296), (0.07839909, 0.53328615),
                (0.08255296, 0.54836994), (0.08683847, 0.56314695), (0.09126, 0.57761997), (0.09582193, 0.59179187),
                (0.10052864, 0.60566527), (0.105384514, 0.619243), (0.11039393, 0.6325278), (0.115561254, 0.6455225),
                (0.120890886, 0.65822977), (0.1263872, 0.6706524), (0.13205457, 0.68279314), (0.13789737, 0.69465476),
                (0.14392, 0.70624), (0.15012683, 0.71755165), (0.15652224, 0.72859246), (0.16311061, 0.7393652),
                (0.16989632, 0.7498726), (0.17688376, 0.7601175), (0.18407728, 0.77010256), (0.19148129, 0.7798306),
                (0.19910017, 0.7893043), (0.20693827, 0.7985265), (0.215, 0.8075), (0.22328973, 0.81622744),
                (0.23181184, 0.8247117), (0.24057071, 0.8329554), (0.24957073, 0.84096146), (0.25881624, 0.8487325),
                (0.26831168, 0.8562714), (0.2780614, 0.86358076), (0.28806975, 0.8706635), (0.29834118, 0.87752235),
                (0.30888, 0.88416), (0.31969064, 0.8905793), (0.33077744, 0.8967829), (0.34214482, 0.9027736),
                (0.35379714, 0.90855426), (0.36573875, 0.91412747), (0.3779741, 0.9194962), (0.3905075, 0.924663),
                (0.40334335, 0.9296307), (0.41648608, 0.93440217), (0.42994002, 0.93898), (0.44370952, 0.94336706),
                (0.45779905, 0.9475661), (0.4722129, 0.9515798), (0.48695552, 0.955411), (0.50203127, 0.9590625),
                (0.5174445, 0.96253693), (0.5331996, 0.9658372), (0.54930097, 0.96896595), (0.565753, 0.9719259),
                (0.58256, 0.97472), (0.59972644, 0.97735083), (0.61725664, 0.97982126), (0.635155, 0.98213404),
                (0.65342593, 0.98429185), (0.6720738, 0.9862975), (0.69110286, 0.98815376), (0.7105177, 0.9898634),
                (0.73032254, 0.9914291), (0.7505219, 0.99285376), (0.77112, 0.99414), (0.79212135, 0.99529064),
                (0.81353027, 0.9963085), (0.8353511, 0.9971962), (0.85758835, 0.99795663), (0.8802462, 0.9985925),
                (0.90332925, 0.9991066), (0.9268418, 0.9995016), (0.95078814, 0.9997803), (0.97517276, 0.9999455),
                (1.0, 1.0),
            ],
        }
    }

    // 使用二分法查找x所在的区间
    fn find_interval(&self, x: f32) -> usize {
        
        let mut low = 0;
        let mut high = self.points.len() - 1;

        while high - low > 1 {
            let mid = (low + high) / 2;
            let (x_mid, _) = self.points[mid];

            if x_mid == x {
                return mid;
            } else if x_mid < x {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    }
}

impl Interpolator for EaseOutCirc {
    fn interpolate(&self, x: f32) -> f32 {
        if x < 0.0{
            return 0.0;
        }else if x > 1.0 {
            return 1.0;
        }

        // 查找x所在的区间
        let idx = self.find_interval(x);

        // 计算线性插值
        let (x1, y1) = self.points[idx];
        let (x2, y2) = self.points[idx + 1];
        ((y2 - y1) / (x2 - x1)) * (x - x1) + y1
    }
}