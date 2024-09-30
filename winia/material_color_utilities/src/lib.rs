pub mod utils;
#[allow(dead_code)]
pub mod hct;
pub mod quantize;
mod contrast;
pub use contrast::*;
mod dislike;
pub use dislike::*;
#[allow(dead_code)]
pub mod dynamic_color;
pub mod palettes;
pub mod scheme;
mod temperature_cache;
mod score;
mod blend;
pub use blend::*;

pub use score::*;

pub use temperature_cache::*;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[macro_export]
    macro_rules! assert_near {
        ($left:expr, $right:expr, $epsilon:expr) => {
            assert!(
                ($left - $right).abs() < $epsilon,
                "left: {}, right: {}, epsilon: {}",
                $left,
                $right,
                $epsilon
            );
        };
    }

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}


#[cfg(test)]
mod contrast_test{
    use crate::assert_near;
    use crate::contrast::*;

    #[test]
    fn test_ratio_of_tones_out_of_bounds_input() {
        assert_near!(ratio_of_tones(-10.0, 110.0), 21.0, 0.001);
    }

    #[test]
    fn test_lighter_impossible_ratio_errors() {
        assert_near!(lighter(90.0, 10.0), -1.0, 0.001);
    }

    #[test]
    fn test_lighter_out_of_bounds_input_above_errors() {
        assert_near!(lighter(110.0, 2.0), -1.0, 0.001);
    }

    #[test]
    fn test_lighter_out_of_bounds_input_below_errors() {
        assert_near!(lighter(-10.0, 2.0), -1.0, 0.001);
    }

    #[test]
    fn test_lighter_unsafe_returns_max_tone() {
        assert_near!(lighter_unsafe(100.0, 2.0), 100.0, 0.001);
    }

    #[test]
    fn test_darker_impossible_ratio_errors() {
        assert_near!(darker(10.0, 20.0), -1.0, 0.001);
    }

    #[test]
    fn test_darker_out_of_bounds_input_above_errors() {
        assert_near!(darker(110.0, 2.0), -1.0, 0.001);
    }

    #[test]
    fn test_darker_out_of_bounds_input_below_errors() {
        assert_near!(darker(-10.0, 2.0), -1.0, 0.001);
    }

    #[test]
    fn test_darker_unsafe_returns_min_tone() {
        assert_near!(darker_unsafe(0.0, 2.0), 0.0, 0.001);
    }
}


#[cfg(test)]
mod score_test {
    use crate::score::*;

    #[test]
    fn test_prioritizes_chroma() {
        let argb_to_population = [(0xff000000, 1), (0xffffffff, 1), (0xff0000ff, 1)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 4,
            ..Default::default()
        });

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0], 0xff0000ff);
    }

    #[test]
    fn test_generates_g_blue_when_no_colors_available() {
        let argb_to_population = [(0xff000000, 1)];
        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 4,
            ..Default::default()
        });

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0], 0xff4285f4);
    }

    #[test]
    fn test_dedupes_nearby_hues() {
        let argb_to_population = [(0xff008772, 1), (0xff318477, 1)];
        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 4,
            ..Default::default()
        });

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0], 0xff008772);
    }

    #[test]
    fn test_maximizes_hue_distance() {
        let argb_to_population = [(0xff008772, 1), (0xff008587, 1), (0xff007ebc, 1)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 2,
            ..Default::default()
        });

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0], 0xff007ebc);
        assert_eq!(ranked[1], 0xff008772);
    }

    #[test]
    fn test_generated_scenario_one() {
        let argb_to_population = [(0xff7ea16d, 67), (0xffd8ccae, 67), (0xff835c0d, 49)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 3,
            fallback_color_argb: 0xff8d3819,
            filter: false,
        });

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 0xff7ea16d);
        assert_eq!(ranked[1], 0xffd8ccae);
        assert_eq!(ranked[2], 0xff835c0d);
    }

    #[test]
    fn test_generated_scenario_two() {
        let argb_to_population = [(0xffd33881, 14), (0xff3205cc, 77), (0xff0b48cf, 36), (0xffa08f5d, 81)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 4,
            fallback_color_argb: 0xff7d772b,
            filter: true,
        });

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 0xff3205cc);
        assert_eq!(ranked[1], 0xffa08f5d);
        assert_eq!(ranked[2], 0xffd33881);
    }

    #[test]
    fn test_generated_scenario_three() {
        let argb_to_population = [(0xffbe94a6, 23), (0xffc33fd7, 42), (0xff899f36, 90), (0xff94c574, 82)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 3,
            fallback_color_argb: 0xffaa79a4,
            filter: true,
        });

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 0xff94c574);
        assert_eq!(ranked[1], 0xffc33fd7);
        assert_eq!(ranked[2], 0xffbe94a6);
    }

    #[test]
    fn test_generated_scenario_four() {
        let argb_to_population = [(0xffdf241c, 85), (0xff685859, 44), (0xffd06d5f, 34), (0xff561c54, 27), (0xff713090, 88)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 5,
            fallback_color_argb: 0xff58c19c,
            filter: false,
        });

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0], 0xffdf241c);
        assert_eq!(ranked[1], 0xff561c54);
    }

    #[test]
    fn test_generated_scenario_five() {
        let argb_to_population = [(0xffbe66f8, 41), (0xff4bbda9, 88), (0xff80f6f9, 44), (0xffab8017, 43), (0xffe89307, 65)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 3,
            fallback_color_argb: 0xff916691,
            filter: false,
        });

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 0xffab8017);
        assert_eq!(ranked[1], 0xff4bbda9);
        assert_eq!(ranked[2], 0xffbe66f8);
    }

    #[test]
    fn test_generated_scenario_six() {
        let argb_to_population = [(0xff18ea8f, 93), (0xff327593, 18), (0xff066a18, 53), (0xfffa8a23, 74), (0xff04ca1f, 62)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 2,
            fallback_color_argb: 0xff4c377a,
            filter: false,
        });

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0], 0xff18ea8f);
        assert_eq!(ranked[1], 0xfffa8a23);
    }

    #[test]
    fn test_generated_scenario_seven(){
        let argb_to_population = [(0xff2e05ed, 23), (0xff153e55, 90), (0xff9ab220, 23), (0xff153379, 66), (0xff68bcc3, 81)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 2,
            fallback_color_argb: 0xfff588dc,
            filter: true,
        });

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0], 0xff2e05ed);
        assert_eq!(ranked[1], 0xff9ab220);
    }

    #[test]
    fn test_generated_scenario_eight(){
        let argb_to_population = [(0xff816ec5, 24), (0xff6dcb94, 19), (0xff3cae91, 98), (0xff5b542f, 25)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 1,
            fallback_color_argb: 0xff84b0fd,
            filter: false,
        });

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0], 0xff3cae91);
    }

    #[test]
    fn test_generated_scenario_nine(){
        let argb_to_population = [(0xff206f86, 52), (0xff4a620d, 96), (0xfff51401, 85), (0xff2b8ebf, 3), (0xff277766, 59)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 3,
            fallback_color_argb: 0xff02b415,
            filter: true,
        });

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 0xfff51401);
        assert_eq!(ranked[1], 0xff4a620d);
        assert_eq!(ranked[2], 0xff2b8ebf);
    }

    #[test]
    fn test_generated_scenario_ten(){
        let argb_to_population = [(0xff8b1d99, 54), (0xff27effe, 43), (0xff6f558d, 2), (0xff77fdf2, 78)];

        let ranked = ranked_suggestions(argb_to_population.iter().copied(), &ScoreOptions {
            desired: 4,
            fallback_color_argb: 0xff5e7a10,
            filter: true,
        });

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0], 0xff27effe);
        assert_eq!(ranked[1], 0xff8b1d99);
        assert_eq!(ranked[2], 0xff6f558d);
    }
}