use std::io::Write;
use std::fs::File;
use bezier_rs::{Bezier, TValue};

pub fn fmt_f32_min_1(v: f32) -> String {
    let s = v.to_string();

    if s.contains('.') {
        s
    } else {
        format!("{}.0", s)
    }
}


fn generate_bezier_interpolator(
    writer: &mut std::io::BufWriter<File>,
    name: &str,
    key_points: (f32, f32, f32, f32),
) {
    writeln!(writer, "interpolator!(").unwrap();
    writeln!(writer, "\t{},", name).unwrap();
    writeln!(writer, "\tvec![").unwrap();
    let (x1, y1, x2, y2) = key_points;
    let bezier = Bezier::from_cubic_coordinates(0.0, 0.0, x1 as f64, y1 as f64, x2 as f64, y2 as f64, 1.0, 1.0);
    let mut points = Vec::new();
    for i in 0..=100 {
        let t = i as f64 / 100.0;
        let point = bezier.evaluate(TValue::Parametric(t));
        points.push((point.x as f32, point.y as f32));
    }
    let max_len = points.iter()
        .map(|(x, y)| format!("({}, {})", fmt_f32_min_1(*x), fmt_f32_min_1(*y)).len())
        .max()
        .unwrap_or(0) + 2;
    for (i, (x, y)) in points.iter().enumerate() {
        let point_str = format!("({}, {}),", fmt_f32_min_1(*x), fmt_f32_min_1(*y));
        if i % 3 == 0 {
            write!(writer, "\t\t").unwrap();
        }
        write!(writer, "{:width$}", point_str, width = max_len).unwrap();
        if i % 3 == 2 {
            writeln!(writer).unwrap();
        }
    }

    writeln!(writer).unwrap();
    writeln!(writer, "\t]").unwrap();
    writeln!(writer, ");").unwrap();
}

fn generate_fun_interpolator(
    writer: &mut std::io::BufWriter<File>,
    name: &str,
    fun: fn(f32) -> f32
) {
    writeln!(writer, "interpolator!(").unwrap();
    writeln!(writer, "\t{},", name).unwrap();
    writeln!(writer, "\tvec![").unwrap();
    let mut points = Vec::new();
    for i in 0..=100 {
        let x = i as f32 / 100.0;
        let y = fun(x);
        points.push((x, y));
    }
    let max_len = points.iter()
        .map(|(x, y)| format!("({}, {})", fmt_f32_min_1(*x), fmt_f32_min_1(*y)).len())
        .max()
        .unwrap_or(0) + 2;
    for (i, (x, y)) in points.iter().enumerate() {
        let point_str = format!("({}, {}),", fmt_f32_min_1(*x), fmt_f32_min_1(*y));
        if i % 3 == 0 {
            write!(writer, "\t\t").unwrap();
        }
        write!(writer, "{:width$}", point_str, width = max_len).unwrap();
        if i % 3 == 2 {
            writeln!(writer).unwrap();
        }
    }
    writeln!(writer).unwrap();
    writeln!(writer, "\t]").unwrap();
    writeln!(writer, ");").unwrap();
}


fn main() {
    let file = File::create("interpolators.rs").unwrap();
    let mut writer = std::io::BufWriter::new(file);
    generate_bezier_interpolator(&mut writer, "EaseInSine", ease_in_sine());
    generate_bezier_interpolator(&mut writer, "EaseOutSine", ease_out_sine());
    generate_bezier_interpolator(&mut writer, "EaseInOutSine", ease_in_out_sine());

    generate_bezier_interpolator(&mut writer, "EaseInQuad", ease_in_quad());
    generate_bezier_interpolator(&mut writer, "EaseOutQuad", ease_out_quad());
    generate_bezier_interpolator(&mut writer, "EaseInOutQuad", ease_in_out_quad());

    generate_bezier_interpolator(&mut writer, "EaseInCubic", ease_in_cubic());
    generate_bezier_interpolator(&mut writer, "EaseOutCubic", ease_out_cubic());
    generate_bezier_interpolator(&mut writer, "EaseInOutCubic", ease_in_out_cubic());

    generate_bezier_interpolator(&mut writer, "EaseInQuart", ease_in_quart());
    generate_bezier_interpolator(&mut writer, "EaseOutQuart", ease_out_quart());
    generate_bezier_interpolator(&mut writer, "EaseInOutQuart", ease_in_out_quart());

    generate_bezier_interpolator(&mut writer, "EaseInQuint", ease_in_quint());
    generate_bezier_interpolator(&mut writer, "EaseOutQuint", ease_out_quint());
    generate_bezier_interpolator(&mut writer, "EaseInOutQuint", ease_in_out_quint());

    generate_bezier_interpolator(&mut writer, "EaseInExpo", ease_in_expo());
    generate_bezier_interpolator(&mut writer, "EaseOutExpo", ease_out_expo());
    generate_bezier_interpolator(&mut writer, "EaseInOutExpo", ease_in_out_expo());

    generate_bezier_interpolator(&mut writer, "EaseInCirc", ease_in_circ());
    generate_bezier_interpolator(&mut writer, "EaseOutCirc", ease_out_circ());
    generate_bezier_interpolator(&mut writer, "EaseInOutCirc", ease_in_out_circ());

    generate_bezier_interpolator(&mut writer, "EaseInBack", ease_in_back());
    generate_bezier_interpolator(&mut writer, "EaseOutBack", ease_out_back());
    generate_bezier_interpolator(&mut writer, "EaseInOutBack", ease_in_out_back());

    generate_fun_interpolator(&mut writer, "EaseInElastic", ease_in_elastic);
    generate_fun_interpolator(&mut writer, "EaseOutElastic", ease_out_elastic);
    generate_fun_interpolator(&mut writer, "EaseInOutElastic", esse_in_out_elastic);

    generate_fun_interpolator(&mut writer, "EaseInBounce", ease_in_bounce);
    generate_fun_interpolator(&mut writer, "EaseOutBounce", ease_out_bounce);
    generate_fun_interpolator(&mut writer, "EaseInOutBounce", ease_in_out_bounce);
}


fn ease_in_sine() -> (f32, f32, f32, f32) {
    (0.12, 0.0, 0.39, 0.0)
}
fn ease_out_sine() -> (f32, f32, f32, f32) {
    (0.61, 1.0, 0.88, 1.0)
}
fn ease_in_out_sine() -> (f32, f32, f32, f32) {
    (0.37, 0.0, 0.63, 1.0)
}
fn ease_in_quad() -> (f32, f32, f32, f32) {
    (0.11, 0.0, 0.5, 0.0)
}
fn ease_out_quad() -> (f32, f32, f32, f32) {
    (0.5, 1.0, 0.89, 1.0)
}
fn ease_in_out_quad() -> (f32, f32, f32, f32) {
    (0.45, 0.0, 0.55, 1.0)
}

fn ease_in_cubic() -> (f32, f32, f32, f32) {
    (0.32, 0.0, 0.67, 0.0)
}
fn ease_out_cubic() -> (f32, f32, f32, f32) {
    (0.33, 1.0, 0.68, 1.0)
}
fn ease_in_out_cubic() -> (f32, f32, f32, f32) {
    (0.65, 0.0, 0.35, 1.0)
}
fn ease_in_quart() -> (f32, f32, f32, f32) {
    (0.5, 0.0, 0.75, 0.0)
}
fn ease_out_quart() -> (f32, f32, f32, f32) {
    (0.25, 1.0, 0.5, 1.0)
}
fn ease_in_out_quart() -> (f32, f32, f32, f32) {
    (0.76, 0.0, 0.24, 1.0)
}

fn ease_in_quint() -> (f32, f32, f32, f32) {
    (0.64, 0.0, 0.78, 0.0)
}
fn ease_out_quint() -> (f32, f32, f32, f32) {
    (0.22, 1.0, 0.36, 1.0)
}
fn ease_in_out_quint() -> (f32, f32, f32, f32) {
    (0.83, 0.0, 0.17, 1.0)
}
fn ease_in_expo() -> (f32, f32, f32, f32) {
    (0.7, 0.0, 0.84, 0.0)
}
fn ease_out_expo() -> (f32, f32, f32, f32) {
    (0.16, 1.0, 0.3, 1.0)
}
fn ease_in_out_expo() -> (f32, f32, f32, f32) {
    (0.87, 0.0, 0.13, 1.0)
}
fn ease_in_circ() -> (f32, f32, f32, f32) {
    (0.55, 0.0, 1.0, 0.45)
}
fn ease_out_circ() -> (f32, f32, f32, f32) {
    (0.0, 0.55, 0.45, 1.0)
}
fn ease_in_out_circ() -> (f32, f32, f32, f32) {
    (0.85, 0.0, 0.15, 1.0)
}
fn ease_in_back() -> (f32, f32, f32, f32) {
    (0.36, 0.0, 0.66, -0.56)
}
fn ease_out_back() -> (f32, f32, f32, f32) {
    (0.34, 1.56, 0.64, 1.0)
}
fn ease_in_out_back() -> (f32, f32, f32, f32) {
    (0.68, -0.6, 0.32, 1.6)
}

fn ease_in_elastic(x: f32) -> f32 {
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;

    if x == 0.0 {
        0.0
    } else if x == 1.0 {
        1.0
    } else {
        -((2.0f32).powf(10.0 * x - 10.0) * ((x * 10.0 - 10.75) * c4).sin())
    }
}
fn ease_out_elastic(x: f32) -> f32 {
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;

    if x == 0.0 {
        0.0
    } else if x == 1.0 {
        1.0
    } else {
        (2.0f32).powf(-10.0 * x) * ((x * 10.0 - 0.75) * c4).sin() + 1.0
    }
}
fn esse_in_out_elastic(x: f32) -> f32 {
    let c5 = (2.0 * std::f32::consts::PI) / 4.5;

    if x == 0.0 {
        0.0
    } else if x == 1.0 {
        1.0
    } else if x < 0.5 {
        -( (2.0f32).powf(20.0 * x - 10.0) * ((20.0 * x - 11.125) * c5).sin() ) / 2.0
    } else {
        ( (2.0f32).powf(-20.0 * x + 10.0) * ((20.0 * x - 11.125) * c5).sin() ) / 2.0 + 1.0
    }
}
fn ease_in_bounce(x: f32) -> f32 {
    1.0 - ease_out_bounce(1.0 - x)
}
fn ease_out_bounce(x: f32) -> f32 {
    let n1 = 7.5625;
    let d1 = 2.75;

    if x < 1.0 / d1 {
        n1 * x * x
    } else if x < 2.0 / d1 {
        let x = x - 1.5 / d1;
        n1 * x * x + 0.75
    } else if x < 2.5 / d1 {
        let x = x - 2.25 / d1;
        n1 * x * x + 0.9375
    } else {
        let x = x - 2.625 / d1;
        n1 * x * x + 0.984375
    }
}
fn ease_in_out_bounce(x: f32) -> f32 {
    if x < 0.5 {
        (1.0 - ease_out_bounce(1.0 - 2.0 * x)) / 2.0
    } else {
        (1.0 + ease_out_bounce(2.0 * x - 1.0)) / 2.0
    }
}