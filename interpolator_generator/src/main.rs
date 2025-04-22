use bezier_rs::{Bezier, TValue};

fn generate_interpolator(x1: f32, y1: f32, x2: f32, y2: f32) {
    let bezier = Bezier::from_cubic_coordinates(0.0, 0.0, x1 as f64, y1 as f64, x2 as f64, y2 as f64, 1.0, 1.0);
    for i in 0..=100 {
        let t = i as f64 / 100.0;
        let point = bezier.evaluate(TValue::Parametric(t));
        print!("({}, {}),", point.x as f32, point.y as f32);
        if i % 2 == 1 {
            println!();
        }
    }
}

fn main() {
    generate_interpolator(0.6, 0.04, 0.98, 0.335)
}
