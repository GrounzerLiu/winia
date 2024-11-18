mod hct;
pub use hct::*;
mod hct_solver;
pub use hct_solver::*;
mod cam;
pub use cam::*;
pub mod viewing_conditions;


#[cfg(test)]
mod cam_test {
    use crate::assert_near;
    use crate::hct::cam::Cam;
    use crate::utils::Argb;

    const RED: Argb = 0xffff0000;
    const GREEN: Argb = 0xff00ff00;
    const BLUE: Argb = 0xff0000ff;
    const WHITE: Argb = 0xffffffff;
    const BLACK: Argb = 0xff000000;

    #[test]
    fn test_red() {
        let cam = Cam::from_argb(RED);

        assert_near!(cam.hue, 27.408, 0.001);
        assert_near!(cam.chroma, 113.357, 0.001);
        assert_near!(cam.j, 46.445, 0.001);
        assert_near!(cam.m, 89.494, 0.001);
        assert_near!(cam.s, 91.889, 0.001);
        assert_near!(cam.q, 105.988, 0.001);
    }

    #[test]
    fn test_green() {
        let cam = Cam::from_argb(GREEN);

        assert_near!(cam.hue, 142.139, 0.001);
        assert_near!(cam.chroma, 108.410, 0.001);
        assert_near!(cam.j, 79.331, 0.001);
        assert_near!(cam.m, 85.587, 0.001);
        assert_near!(cam.s, 78.604, 0.001);
        assert_near!(cam.q, 138.520, 0.001);
    }

    #[test]
    fn test_blue() {
        let cam = Cam::from_argb(BLUE);

        assert_near!(cam.hue, 282.788, 0.001);
        assert_near!(cam.chroma, 87.230, 0.001);
        assert_near!(cam.j, 25.465, 0.001);
        assert_near!(cam.m, 68.867, 0.001);
        assert_near!(cam.s, 93.674, 0.001);
        assert_near!(cam.q, 78.481, 0.001);
    }

    #[test]
    fn test_white() {
        let cam = Cam::from_argb(WHITE);

        assert_near!(cam.hue, 209.492, 0.001);
        assert_near!(cam.chroma, 2.869, 0.001);
        assert_near!(cam.j, 100.0, 0.001);
        assert_near!(cam.m, 2.265, 0.001);
        assert_near!(cam.s, 12.068, 0.001);
        assert_near!(cam.q, 155.521, 0.001);
    }

    #[test]
    fn test_black() {
        let cam = Cam::from_argb(BLACK);

        assert_near!(cam.hue, 0.0, 0.001);
        assert_near!(cam.chroma, 0.0, 0.001);
        assert_near!(cam.j, 0.0, 0.001);
        assert_near!(cam.m, 0.0, 0.001);
        assert_near!(cam.s, 0.0, 0.001);
        assert_near!(cam.q, 0.0, 0.001);
    }

    #[test]
    fn test_red_round_trip() {
        let cam = Cam::from_argb(RED);
        let argb = cam.to_argb();
        assert_eq!(RED, argb);
    }

    #[test]
    fn test_green_round_trip() {
        let cam = Cam::from_argb(GREEN);
        let argb = cam.to_argb();
        assert_eq!(GREEN, argb);
    }

    #[test]
    fn test_blue_round_trip() {
        let cam = Cam::from_argb(BLUE);
        let argb = cam.to_argb();
        assert_eq!(BLUE, argb);
    }
}

#[cfg(test)]
mod hct_test {
    use crate::hct::cam::Cam;
    use crate::hct::hct::*;
    use crate::utils::lstar_from_argb;

    #[test]
    fn test_limited_to_srgb() {
        let hct = Hct::from_hct(120.0, 200.0, 50.0);
        let argb = hct.to_argb();

        assert_eq!(Cam::from_argb(argb).hue, hct.hue());
        assert_eq!(Cam::from_argb(argb).chroma, hct.chroma());
        assert_eq!(lstar_from_argb(argb), hct.tone());
    }

    #[test]
    fn test_truncates_colors() {
        let mut hct = Hct::from_hct(120.0, 60.0, 50.0);
        let chroma = hct.chroma();
        assert!(chroma < 60.0);

        hct.set_tone(180.0);
        assert!(hct.chroma() < chroma);
    }
}

#[cfg(test)]
mod hct_solver_test {
    use crate::hct::cam::Cam;
    use crate::hct::hct_solver::solve_to_int;
    use crate::utils::lstar_from_argb;

    #[test]
    fn test_red() {
        let color = 0xFFFE0315;
        let cam = Cam::from_argb(color);
        let tone = lstar_from_argb(color);

        let recovered = solve_to_int(cam.hue, cam.chroma, tone);
        assert_eq!(color, recovered);
    }

    #[test]
    fn test_green() {
        let color = 0xFF15FE03;
        let cam = Cam::from_argb(color);
        let tone = lstar_from_argb(color);

        let recovered = solve_to_int(cam.hue, cam.chroma, tone);
        assert_eq!(color, recovered);
    }

    #[test]
    fn test_blue() {
        let color = 0xFF0315FE;
        let cam = Cam::from_argb(color);
        let tone = lstar_from_argb(color);

        let recovered = solve_to_int(cam.hue, cam.chroma, tone);
        assert_eq!(color, recovered);
    }

    #[test]
    fn test_exhaustive() {
        for color_index in 0..=0xFFFFFF {
            let color = 0xFF000000 | color_index;
            let cam = Cam::from_argb(color);
            let tone = lstar_from_argb(color);

            let recovered = solve_to_int(cam.hue, cam.chroma, tone);
            assert_eq!(color, recovered);
        }
    }
}