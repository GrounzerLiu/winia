use crate::utils::{positive_modulus, require, DISTANCE_EPSILON};

/// Checks if the given progress is in the given progress range, since progress is in the [0..1)
/// interval, and wraps, there is a special case when `progress_to` < `progress_From`. For example, if the
/// progress range is 0.7 to 0.2, both 0.8 and 0.1 are inside and 0.5 is outside.
pub fn progress_in_range(progress: f32, progress_from: f32, progress_to: f32) -> bool {
    if progress_to >= progress_from {
        progress >= progress_from && progress <= progress_to
    } else {
        progress >= progress_from || progress <= progress_to
    }
}

/// Maps from one set of progress values to another. This is used by [`DoubleMapper`] to retrieve the
/// value on one shape that maps to the appropriate value on the other.
pub fn linear_map(
    x_value: &[f32],
    y_value: &[f32],
    x: f32,
) -> f32 {
    require(x >= 0.0 && x <= 1.0, format!("Invalid progress: {}", x));
    let segment_start_index = (0..x_value.len()).find(|&i| progress_in_range(x, x_value[i], x_value[(i + 1) % x_value.len()])).unwrap();
    let segment_end_index = (segment_start_index + 1) % x_value.len();
    let segment_size_x = positive_modulus(x_value[segment_end_index] - x_value[segment_start_index], 1.0);
    let segment_size_y = positive_modulus(y_value[segment_end_index] - y_value[segment_start_index], 1.0);
    let position_in_segment = if segment_size_x < 0.001 {
        0.5
    } else {
        positive_modulus(x - x_value[segment_start_index], 1.0) / segment_size_x
    };
    positive_modulus(y_value[segment_start_index] + segment_size_y * position_in_segment, 1.0)
}

/// [`DoubleMapper`] creates mappings from values in the `[0..1)` source space to values in the
/// `[0..1)` target space, and back. This mapping is created given a finite list of representative
/// mappings, and is extended to the whole interval by linear interpolation and wrapping around.
///
/// For example, if we have mappings `0.2` → `0.5` and `0.4` → `0.6`, then `0.3` (which is in the
/// middle of the source interval) will be mapped to `0.55` (the middle of the targets for the
/// interval), `0.21` will map to `0.505`, and so on.
///
/// As a more complete example, if we use `x` to represent a value in the source space and `y` for
/// the target space, and given as input the mappings `0 → 0`, `0.5 → 0.25`, this will create a
/// mapping that:
///
/// ```text
/// if x in [0..0.5]       y = x / 2
/// if x in [0.5..1]       y = 0.25 + (x - 0.5) * 1.5 = x * 1.5 - 0.5
/// ```
///
/// The mapping can also be used the other way around (using the [`map_back`](DoubleMapper::map_back) function), resulting in:
///
/// ```text
/// if y in [0..0.25]      x = y * 2
/// if y in [0.25..1]      x = (y + 0.5) / 1.5
/// ```
///
/// This is used to create mappings of progress values between the start and end shape, which is
/// then used to insert new curves and match curves overall.
pub struct DoubleMapper {
    pub mappings: Vec<(f32, f32)>,
    source_values: Vec<f32>,
    target_values: Vec<f32>,
}

fn validate_progress(p: &Vec<f32>) {
    let mut prev = *p.last().unwrap();
    let mut wraps = 0;
    for i in 0..p.len() {
        let curr = &p[i];
        require(
            *curr >= 0.0 && *curr < 1.0,
            format!("FloatMapping - Progress outside of range: {:?}", p)
        );
        require(
            progress_distance(*curr, prev) > DISTANCE_EPSILON,
            format!("FloatMapping - Progress repeats a value: {:?}", p)
        );
        if curr < &prev {
            wraps += 1;
            require(
                wraps <= 1,
                format!("FloatMapping - Progress wraps more than once: {:?}", p)
            );
        }
        prev = *curr;
    }
}

fn progress_distance(p1: f32, p2: f32) -> f32 {
    let d = (p1 - p2).abs();
    d.min(1.0 - d)
}

impl DoubleMapper {
    pub fn new(mappings: &[(f32, f32)]) -> Self {
        let mut source_values = Vec::with_capacity(mappings.len());
        let mut target_values = Vec::with_capacity(mappings.len());
        for (source, target) in mappings.iter() {
            source_values.push(*source);
            target_values.push(*target);
        }
        validate_progress(&source_values);
        validate_progress(&target_values);
        DoubleMapper {
            mappings: mappings.to_vec(),
            source_values,
            target_values,
        }
    }

    pub fn map(&self, x: f32) -> f32 {
        linear_map(&self.source_values, &self.target_values, x)
    }

    pub fn map_back(&self, y: f32) -> f32 {
        linear_map(&self.target_values, &self.source_values, y)
    }

    pub fn identity() -> Self {
        DoubleMapper::new(&[(0.0, 0.0), (0.5, 0.5)])
    }
}

#[cfg(test)]
mod float_mapping_tests {
    use crate::{assert_equalish, assert_panic};
    use crate::float_mapping::DoubleMapper;

    #[test]
    fn identity_mapping_test() {
        let mapper = DoubleMapper::identity();
        validate_mapping(&mapper, &|x| x);
    }
    #[test]
    fn simple_mapping_test() {
        let mapper = DoubleMapper::new(&[(0.0, 0.0), (0.5, 0.25)]);
        validate_mapping(&mapper, &|x| {
            if x < 0.5 {
                x / 2.0
            } else {
                (3.0 * x - 1.0) / 2.0
            }
        });
    }
    #[test]
    fn target_wrap_test() {
        let mapper = DoubleMapper::new(&[(0.0, 0.5), (0.1, 0.6)]);
        validate_mapping(&mapper, &|x| (x + 0.5) % 1.0);
    }
    #[test]
    fn source_wrap_test() {
        let mapper = DoubleMapper::new(&[(0.5, 0.0), (0.1, 0.6)]);
        validate_mapping(&mapper, &|x| (x + 0.5) % 1.0);
    }
    #[test]
    fn both_wrap_test() {
        let mapper = DoubleMapper::new(&[(0.5, 0.5), (0.75, 0.75), (0.1, 0.1), (0.49, 0.49)]);
        validate_mapping(&mapper, &|x| x);
    }
    #[test]
    fn multiple_point_test() {
        let mapper = DoubleMapper::new(&[(0.4, 0.2), (0.5, 0.22), (0.0, 0.8)]);
        validate_mapping(&mapper, &|x| {
            if x < 0.4 {
                (0.8 + x) % 1.0
            } else if x < 0.5 {
                0.2 + (x - 0.4) / 5.0
            } else {
                0.22 + (x - 0.5) * 1.16
            }
        });
    }
    #[test]
    fn target_double_wrap_throws() {
        assert_panic!({
            DoubleMapper::new(&[(0.0, 0.0), (0.3, 0.6), (0.6, 0.3), (0.9, 0.9)])
        });
    }

    #[test]
    fn source_double_wrap_throws() {
        assert_panic!({
            DoubleMapper::new(&[(0.0, 0.0), (0.6, 0.3), (0.3, 0.6), (0.9, 0.9)])
        });
    }

    fn validate_mapping(mapper: &DoubleMapper, expected_function: &dyn Fn(f32) -> f32) {
        for i in 0..=9999 {
            let source = i as f32 / 10000.0;
            let target = expected_function(source);

            assert_equalish!(target, mapper.map(source));
            assert_equalish!(source, mapper.map_back(target));
        }
    }
}