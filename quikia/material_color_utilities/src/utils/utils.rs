use std::f64;

pub type Argb = u32;

#[derive(Clone, Copy)]
pub struct Vec3 {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl Vec3 {
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        Self { a, b, c }
    }
}

/**
 * Value of pi.
 */
pub const K_PI: f64 = f64::consts::PI;

/**
 * Returns the standard white point; white on a sunny day.
 */
pub const WHITE_POINT_D65: [f64; 3] = [95.047, 100.0, 108.883];

pub fn vec3(a: f64, b: f64, c: f64) -> Vec3 {
    Vec3 { a, b, c }
}

/**
 * Returns the red component of a color in ARGB format.
 */
pub fn red_from_int(argb: Argb) -> u8
{ ((argb & 0x00ff0000) >> 16) as u8 }

/**
 * Returns the green component of a color in ARGB format.
 */
pub fn green_from_int(argb: Argb) -> u8
{ ((argb & 0x0000ff00) >> 8) as u8 }

/**
 * Returns the blue component of a color in ARGB format.
 */
pub fn blue_from_int(argb: Argb) -> u8
{ (argb & 0x000000ff) as u8 }

/**
 * Converts a color from RGB components to ARGB format.
 */
pub fn argb_from_rgb(red: u8, green: u8, blue: u8) -> Argb {
    0xFF000000 | (((red as u32) & 0x0ff) << 16) |
        (((green as u32) & 0x0ff) << 8) | ((blue as u32) & 0x0ff)
    // 0xFF000000 | ((red & 0xff) << 16) | ((green & 0xff) << 8) |
    //     (blue & 0xff)
}

/**
 * Converts a color from linear RGB components to ARGB format.
 */
pub fn argb_from_linrgb(linrgb: Vec3) -> Argb {
    let r = delinearized(linrgb.a);
    let g = delinearized(linrgb.b);
    let b = delinearized(linrgb.c);

    0xFF000000 | (((r as u32)& 0x0ff) << 16) | (((g as u32) & 0x0ff) << 8) | ((b as u32)& 0x0ff)
}

/**
 * Delinearizes an RGB component.
 *
 * @param rgb_component 0.0 <= rgb_component <= 100.0, represents linear
 * R/G/B channel
 *
 * @return 0 <= output <= 255, color channel converted to regular
 * RGB space
 */
pub fn delinearized(rgb_component: f64) -> u8 {
    let normalized = rgb_component / 100.0;

    let delinearized = if normalized <= 0.0031308 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    };
    clamp((delinearized * 255.0).round() as u8, 0, 255)
}

pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/**
 * Linearizes an RGB component.
 *
 * @param rgb_component 0 <= rgb_component <= 255, represents R/G/B
 * channel
 *
 * @return 0.0 <= output <= 100.0, color channel converted to
 * linear RGB space
 */
pub fn linearized(rgb_component: u32) -> f64 {
    let normalized = rgb_component as f64 / 255.0;
    if normalized <= 0.040449936 {
        normalized / 12.92 * 100.0
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4) * 100.0
    }
}

/**
 * Returns the alpha component of a color in ARGB format.
 */
pub fn alpha_from_int(argb: Argb) -> u32
{ (argb & 0xff000000) >> 24 }

/**
 * Returns whether a color in ARGB format is opaque.
 */
pub fn is_opaque(argb: Argb) -> bool
{ alpha_from_int(argb) == 255 }

/**
 * Computes the L* value of a color in ARGB representation.
 *
 * @param argb ARGB representation of a color
 *
 * @return L*, from L*a*b*, coordinate of the color
 */
pub fn lstar_from_argb(argb: Argb) -> f64 {
    let red = (argb & 0x00ff0000) >> 16;
    let green = (argb & 0x0000ff00) >> 8;
    let blue = argb & 0x000000ff;
    let red_l = linearized(red);
    let green_l = linearized(green);
    let blue_l = linearized(blue);
    let y = 0.2126 * red_l + 0.7152 * green_l + 0.0722 * blue_l;
    lstar_from_y(y)
}

/**
 * Converts an L* value to a Y value.
 *
 * L* in L*a*b* and Y in XYZ measure the same quantity, luminance.
 *
 * L* measures perceptual luminance, a linear scale. Y in XYZ
 * measures relative luminance, a logarithmic scale.
 *
 * @param lstar L* in L*a*b*. 0.0 <= L* <= 100.0
 *
 * @return Y in XYZ. 0.0 <= Y <= 100.0
 */
pub fn y_from_lstar(lstar: f64) -> f64 {
    let ke = 8.0;
    if lstar > ke {
        let cube_root = (lstar + 16.0) / 116.0;
        let cube = cube_root * cube_root * cube_root;
        cube * 100.0
    } else {
        lstar / (24389.0 / 27.0) * 100.0
    }
}

/**
 * Converts a Y value to an L* value.
 *
 * L* in L*a*b* and Y in XYZ measure the same quantity, luminance.
 *
 * L* measures perceptual luminance, a linear scale. Y in XYZ
 * measures relative luminance, a logarithmic scale.
 *
 * @param y Y in XYZ. 0.0 <= Y <= 100.0
 *
 * @return L* in L*a*b*. 0.0 <= L* <= 100.0
 */
pub fn lstar_from_y(y: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let y_normalized = y / 100.0;
    if y_normalized <= e {
        (24389.0 / 27.0) * y_normalized
    } else {
        y_normalized.powf(1.0 / 3.0) * 116.0 - 16.0
    }
}

/**
 * Sanitizes a degree measure as an integer.
 *
 * @return a degree measure between 0 (inclusive) and 360 (exclusive).
 */
pub fn sanitize_degrees_int(degrees: i32) -> i32 {
    if degrees < 0 {
        (degrees % 360) + 360
    } else if degrees >= 360 {
        degrees % 360
    } else {
        degrees
    }
}

/**
 * Sanitizes a degree measure as an floating-point number.
 *
 * @return a degree measure between 0.0 (inclusive) and 360.0 (exclusive).
 */
pub fn sanitize_degrees_double(degrees: f64) -> f64 {
    if degrees < 0.0 {
        (degrees % 360.0) + 360.0
    } else if degrees >= 360.0 {
        degrees % 360.0
    } else {
        degrees
    }
}

/**
 * Distance of two points on a circle, represented using degrees.
 */
pub fn diff_degrees(a: f64, b: f64) -> f64 {
    180.0 - ((a - b).abs() - 180.0).abs()
}

/**
 * Sign of direction change needed to travel from one angle to
 * another.
 *
 * For angles that are 180 degrees apart from each other, both
 * directions have the same travel distance, so either direction is
 * shortest. The value 1.0 is returned in this case.
 *
 * @param from The angle travel starts from, in degrees.
 *
 * @param to The angle travel ends at, in degrees.
 *
 * @return -1 if decreasing from leads to the shortest travel
 * distance, 1 if increasing from leads to the shortest travel
 * distance.
 */
pub fn rotation_direction(from: f64, to: f64) -> f64 {
    let increasing_difference = sanitize_degrees_double(to - from);
    if increasing_difference <= 180.0 { 1.0 } else { -1.0 }
}

/**
 * Returns the hexadecimal representation of a color.
 */
pub fn hex_from_argb(argb: Argb) -> String {
    format!("{:08x}", argb)
}

/**
 * Converts an L* value to an ARGB representation.
 *
 * @param lstar L* in L*a*b*. 0.0 <= L* <= 100.0
 *
 * @return ARGB representation of grayscale color with lightness matching L*
 */
pub fn int_from_lstar(lstar: f64) -> Argb {
    let y = y_from_lstar(lstar);
    let component = delinearized(y);
    argb_from_rgb(component, component, component)
}


/**
 * The signum function.
 *
 * @return 1 if num > 0, -1 if num < 0, and 0 if num = 0
 */
pub fn signum(num: f64) -> i32 {
    if num < 0.0 {
        -1
    } else if num == 0.0 {
        0
    } else {
        1
    }
}

/**
 * The linear interpolation function.
 *
 * @return start if amount = 0 and stop if amount = 1
 */
pub fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}

/**
 * Multiplies a 1x3 row vector with a 3x3 matrix, returning the product.
 */
pub fn matrix_multiply(input: Vec3, matrix: [[f64; 3]; 3]) -> Vec3 {
    let a =
        input.a * matrix[0][0] + input.b * matrix[0][1] + input.c * matrix[0][2];
    let b =
        input.a * matrix[1][0] + input.b * matrix[1][1] + input.c * matrix[1][2];
    let c =
        input.a * matrix[2][0] + input.b * matrix[2][1] + input.c * matrix[2][2];
    Vec3 { a, b, c }
}
