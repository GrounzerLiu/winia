use crate::utils::{Argb, argb_from_rgb, blue_from_int, green_from_int, red_from_int};

#[derive(Clone,Default)]
struct Box {
    r0: i32,
    r1: i32,
    g0: i32,
    g1: i32,
    b0: i32,
    b1: i32,
    vol: i32,
}

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Red,
    Green,
    Blue,
}

const INDEX_BITS: usize = 5;
const INDEX_COUNT: usize = (1 << INDEX_BITS) + 1;
const TOTAL_SIZE: usize = INDEX_COUNT * INDEX_COUNT * INDEX_COUNT;
const MAX_COLORS: usize = 256;

type IntArray = Vec<i64>;
type DoubleArray = Vec<f64>;

fn get_index(r: i32, g: i32, b: i32) -> usize {
    ((r << (INDEX_BITS * 2)) + (r << (INDEX_BITS + 1)) + (g << INDEX_BITS) +
        (r + g + b)) as usize
}

fn construct_histogram(pixels: &[Argb], weights: &mut IntArray, m_r: &mut IntArray, m_g: &mut IntArray, m_b: &mut IntArray, moments: &mut DoubleArray) {
    pixels.iter().for_each(|pixel|{
        let red = red_from_int(*pixel) as i32;
        let green = green_from_int(*pixel) as i32;
        let blue = blue_from_int(*pixel) as i32;

        let bits_to_remove = 8 - INDEX_BITS;
        let index_r = (red >> bits_to_remove) + 1;
        let index_g = (green >> bits_to_remove) + 1;
        let index_b = (blue >> bits_to_remove) + 1;
        let index = get_index(index_r, index_g, index_b);

        weights[index] += 1;
        m_r[index] += red as i64;
        m_g[index] += green as i64;
        m_b[index] += blue as i64;
        moments[index] += ((red * red) + (green * green) + (blue * blue)) as f64;
    });
}

fn compute_moments(weights: &mut IntArray, m_r: &mut IntArray, m_g: &mut IntArray, m_b: &mut IntArray, moments: &mut DoubleArray) {
    for r in 1..INDEX_COUNT {
        let mut area: [i64; INDEX_COUNT] = [0; INDEX_COUNT];
        let mut area_r: [i64; INDEX_COUNT] = [0; INDEX_COUNT];
        let mut area_g: [i64; INDEX_COUNT] = [0; INDEX_COUNT];
        let mut area_b: [i64; INDEX_COUNT] = [0; INDEX_COUNT];
        let mut area_2: [f64; INDEX_COUNT] = [0.0; INDEX_COUNT];

        for g in 1..INDEX_COUNT {
            let mut line = 0i64;
            let mut line_r = 0i64;
            let mut line_g = 0i64;
            let mut line_b = 0i64;
            let mut line_2 = 0.0;

            for b in 1..INDEX_COUNT {
                let index = get_index(r as i32, g as i32, b as i32);

                line += weights[index];
                line_r += m_r[index];
                line_g += m_g[index];
                line_b += m_b[index];
                line_2 += moments[index];

                area[b] += line;
                area_r[b] += line_r;
                area_g[b] += line_g;
                area_b[b] += line_b;
                area_2[b] += line_2;


                let previous_index = get_index((r - 1) as i32, g as i32, b as i32);
                weights[index] = weights[previous_index] + area[b];
                m_r[index] = m_r[previous_index] + area_r[b];
                m_g[index] = m_g[previous_index] + area_g[b];
                m_b[index] = m_b[previous_index] + area_b[b];
                moments[index] = moments[previous_index] + area_2[b];
            }
        }
    }
}

fn top(cube: &Box, direction: Direction, position: i32, moment: &IntArray) -> i64 {
    if direction == Direction::Red {
        moment[get_index(position, cube.g1, cube.b1)] -
            moment[get_index(position, cube.g1, cube.b0)] -
            moment[get_index(position, cube.g0, cube.b1)] +
            moment[get_index(position, cube.g0, cube.b0)]
    } else if direction == Direction::Green {
        moment[get_index(cube.r1, position, cube.b1)] -
            moment[get_index(cube.r1, position, cube.b0)] -
            moment[get_index(cube.r0, position, cube.b1)] +
            moment[get_index(cube.r0, position, cube.b0)]
    } else {
        moment[get_index(cube.r1, cube.g1, position)] -
            moment[get_index(cube.r1, cube.g0, position)] -
            moment[get_index(cube.r0, cube.g1, position)] +
            moment[get_index(cube.r0, cube.g0, position)]
    }
}

fn bottom(cube: &Box, direction: Direction, moment: &IntArray) -> i64 {
    if direction == Direction::Red {
        -moment[get_index(cube.r0, cube.g1, cube.b1)] +
            moment[get_index(cube.r0, cube.g1, cube.b0)] +
            moment[get_index(cube.r0, cube.g0, cube.b1)] -
            moment[get_index(cube.r0, cube.g0, cube.b0)]
    } else if direction == Direction::Green {
        -moment[get_index(cube.r1, cube.g0, cube.b1)] +
            moment[get_index(cube.r1, cube.g0, cube.b0)] +
            moment[get_index(cube.r0, cube.g0, cube.b1)] -
            moment[get_index(cube.r0, cube.g0, cube.b0)]
    } else {
        -moment[get_index(cube.r1, cube.g1, cube.b0)] +
            moment[get_index(cube.r1, cube.g0, cube.b0)] +
            moment[get_index(cube.r0, cube.g1, cube.b0)] -
            moment[get_index(cube.r0, cube.g0, cube.b0)]
    }
}

fn vol(cube: &Box, moment: &IntArray) -> i64 {
    moment[get_index(cube.r1, cube.g1, cube.b1)] -
        moment[get_index(cube.r1, cube.g1, cube.b0)] -
        moment[get_index(cube.r1, cube.g0, cube.b1)] +
        moment[get_index(cube.r1, cube.g0, cube.b0)] -
        moment[get_index(cube.r0, cube.g1, cube.b1)] +
        moment[get_index(cube.r0, cube.g1, cube.b0)] +
        moment[get_index(cube.r0, cube.g0, cube.b1)] -
        moment[get_index(cube.r0, cube.g0, cube.b0)]
}

fn variance(cube: &Box, weights: &IntArray, m_r: &IntArray, m_g: &IntArray, m_b: &IntArray, moments: &DoubleArray) -> f64 {
    let dr = vol(cube, m_r);
    let dg = vol(cube, m_g);
    let db = vol(cube, m_b);
    let xx = moments[get_index(cube.r1, cube.g1, cube.b1)] -
        moments[get_index(cube.r1, cube.g1, cube.b0)] -
        moments[get_index(cube.r1, cube.g0, cube.b1)] +
        moments[get_index(cube.r1, cube.g0, cube.b0)] -
        moments[get_index(cube.r0, cube.g1, cube.b1)] +
        moments[get_index(cube.r0, cube.g1, cube.b0)] +
        moments[get_index(cube.r0, cube.g0, cube.b1)] -
        moments[get_index(cube.r0, cube.g0, cube.b0)];
    let hypotenuse = dr * dr + dg * dg + db * db;
    let volume = vol(cube, weights);
    xx - (hypotenuse as f64 / volume as f64)
}

#[allow(clippy::too_many_arguments)]
fn maximize(cube: &Box, direction: Direction, first: i32,
            last: i32, cut: &mut i32, whole_w: i64,
            whole_r: i64, whole_g: i64,
            whole_b: i64, weights: &IntArray,
            m_r: &IntArray, m_g: &IntArray, m_b: &IntArray) -> f64 {
    let bottom_r = bottom(cube, direction, m_r);
    let bottom_g = bottom(cube, direction, m_g);
    let bottom_b = bottom(cube, direction, m_b);
    let bottom_w = bottom(cube, direction, weights);

    let mut max = 0.0;
    *cut = -1;

    let mut half_r;
    let mut half_g;
    let mut half_b;
    let mut half_w;
    for i in first..last {
        half_r = bottom_r + top(cube, direction, i, m_r);
        half_g = bottom_g + top(cube, direction, i, m_g);
        half_b = bottom_b + top(cube, direction, i, m_b);
        half_w = bottom_w + top(cube, direction, i, weights);
        if half_w == 0 {
            continue;
        }

        let mut temp = ((half_r * half_r +
            half_g * half_g +
            half_b * half_b) /
            half_w) as f64;

        half_r = whole_r - half_r;
        half_g = whole_g - half_g;
        half_b = whole_b - half_b;
        half_w = whole_w - half_w;
        if half_w == 0 {
            continue;
        }
        temp += ((half_r * half_r +
            half_g * half_g +
            half_b * half_b) /
            half_w) as f64;

        if temp > max {
            max = temp;
            *cut = i;
        }
    }
    max
}

pub fn quantize_wu(pixels: &Vec<Argb>, max_colors: u16) -> Vec<Argb> {
    let mut max_colors = max_colors;
    if max_colors == 0 || max_colors > 256 || pixels.is_empty() {
        return Vec::new();
    }
    let mut weights = vec![0i64; TOTAL_SIZE];
    let mut moments_red = vec![0i64; TOTAL_SIZE];
    let mut moments_green = vec![0i64; TOTAL_SIZE];
    let mut moments_blue = vec![0i64; TOTAL_SIZE];
    let mut moments = vec![0.0; TOTAL_SIZE];

    construct_histogram(pixels, &mut weights, &mut moments_red, &mut moments_green, &mut moments_blue,
                        &mut moments);
    compute_moments(&mut weights, &mut moments_red, &mut moments_green, &mut moments_blue, &mut moments);

    let mut cubes = vec![Box::default(); MAX_COLORS];
    cubes[0].r1 = (INDEX_COUNT - 1) as i32;
    cubes[0].g1 = (INDEX_COUNT - 1) as i32;
    cubes[0].b1 = (INDEX_COUNT - 1) as i32;

    let mut volume_variance = vec![0.0; MAX_COLORS];
    let mut next = 0;
    for mut i in 1usize..max_colors as usize {
        let res = {
            let mut result = true;

            let box1 = &cubes[next];
            let whole_r = vol(box1, &moments_red);
            let whole_g = vol(box1, &moments_green);
            let whole_b = vol(box1, &moments_blue);
            let whole_w = vol(box1, &weights);

            let mut cut_r: i32 = 0;
            let mut cut_g: i32 = 0;
            let mut cut_b: i32 = 0;

            let max_r =
                maximize(box1, Direction::Red, box1.r0 + 1, box1.r1, &mut cut_r, whole_w,
                         whole_r, whole_g, whole_b, &weights, &moments_red, &moments_green, &moments_blue);
            let max_g =
                maximize(box1, Direction::Green, box1.g0 + 1, box1.g1, &mut cut_g, whole_w,
                         whole_r, whole_g, whole_b, &weights, &moments_red, &moments_green, &moments_blue);
            let max_b =
                maximize(box1, Direction::Blue, box1.b0 + 1, box1.b1, &mut cut_b, whole_w,
                         whole_r, whole_g, whole_b, &weights, &moments_red, &moments_green, &moments_blue);

            let direction;
            if max_r >= max_g && max_r >= max_b {
                direction = Direction::Red;
                if cut_r < 0 {
                    result = false;
                }
            } else if max_g >= max_r && max_g >= max_b {
                direction = Direction::Green;
            } else {
                direction = Direction::Blue;
            }


            if result {
                let box1 = box1.clone();
                {
                    let box2 = &mut cubes[i];
                    box2.r1 = box1.r1;
                    box2.g1 = box1.g1;
                    box2.b1 = box1.b1;
                }

                if direction == Direction::Red {
                    {
                        let box2 = &mut cubes[i];
                        box2.g0 = box1.g0;
                        box2.b0 = box1.b0;
                        box2.r0 = cut_r;
                    }
                    cubes[next].r1=cut_r;
                } else if direction == Direction::Green {
                    {
                        let box2 = &mut cubes[i];
                        box2.r0 = box1.r0;
                        box2.b0 = box1.b0;
                        box2.g0 = cut_g;
                    }
                    cubes[next].g1=cut_g;
                } else {
                    {
                        let box2 = &mut cubes[i];
                        box2.r0 = box1.r0;
                        box2.g0 = box1.g0;
                        box2.b0 = cut_b;
                    }
                    cubes[next].b1=cut_b;
                }
                {
                    let box2 = &mut cubes[i];
                    box2.vol = (box2.r1 - box2.r0) * (box2.g1 - box2.g0) * (box2.b1 - box2.b0);
                }
                {
                    let box1 = &mut cubes[next];
                    box1.vol = (box1.r1 - box1.r0) * (box1.g1 - box1.g0) * (box1.b1 - box1.b0);
                }
                true
            }
            else {
                false
            }

        };
        if res
        {
            volume_variance[next] =
                if cubes[next].vol > 1 {
                    variance(&cubes[next], &weights, &moments_red,
                             &moments_green, &moments_blue, &moments)
                } else { 0.0 };
            volume_variance[i] = if cubes[i].vol > 1
            {
                variance(&cubes[i], &weights, &moments_red,
                         &moments_green, &moments_blue, &moments)
            } else { 0.0 };
        } else {
            volume_variance[next] = 0.0;
            i -= 1;
        }

        next = 0;
        let mut temp = volume_variance[0];
        for j in 1..=i {
            if volume_variance[j] > temp {
                temp = volume_variance[j];
                next = j;
            }
        }
        if temp <= 0.0 {
            max_colors = (i + 1) as u16;
            break;
        }
    }

    let mut out_colors = Vec::new();

    for i in 0usize..(max_colors as usize) {
        let weight = vol(&cubes[i], &weights);
        if weight > 0 {
            let red = vol(&cubes[i], &moments_red) / weight;
            let green = vol(&cubes[i], &moments_green) / weight;
            let blue = vol(&cubes[i], &moments_blue) / weight;
            let argb = argb_from_rgb(red as u8, green as u8, blue as u8);
            out_colors.push(argb);
        }
    }

    out_colors
}

