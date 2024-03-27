pub mod utils;

pub use utils::*;

#[cfg(test)]
mod tests {
    use crate::assert_near;
    use crate::utils::*;

    const K_MATRIX: [[f64; 3]; 3] = [[1.0, 2.0, 3.0], [-4.0, 5.0, -6.0], [-7.0, -8.0, -9.0]];

    #[test]
    fn test_signum() {
        assert_eq!(signum(0.001), 1);
        assert_eq!(signum(3.0), 1);
        assert_eq!(signum(100.0), 1);
        assert_eq!(signum(-0.002), -1);
        assert_eq!(signum(-4.0), -1);
        assert_eq!(signum(-101.0), -1);
        assert_eq!(signum(0.0), 0);
    }

    #[test]
    fn test_rotation_is_positive_for_counterclockwise() {
        assert_eq!(rotation_direction(0.0, 30.0), 1.0);
        assert_eq!(rotation_direction(0.0, 60.0), 1.0);
        assert_eq!(rotation_direction(0.0, 150.0), 1.0);
        assert_eq!(rotation_direction(90.0, 240.0), 1.0);
        assert_eq!(rotation_direction(300.0, 30.0), 1.0);
        assert_eq!(rotation_direction(270.0, 60.0), 1.0);
        assert_eq!(rotation_direction(360.0 * 2.0, 15.0), 1.0);
        assert_eq!(rotation_direction(360.0 * 3.0 + 15.0, -360.0 * 4.0 + 30.0), 1.0);
    }


    #[test]
    fn test_rotation_is_negative_for_clockwise() {
        assert_eq!(rotation_direction(30.0, 0.0), -1.0);
        assert_eq!(rotation_direction(60.0, 0.0), -1.0);
        assert_eq!(rotation_direction(150.0, 0.0), -1.0);
        assert_eq!(rotation_direction(240.0, 90.0), -1.0);
        assert_eq!(rotation_direction(30.0, 300.0), -1.0);
        assert_eq!(rotation_direction(60.0, 270.0), -1.0);
        assert_eq!(rotation_direction(15.0, -360.0 * 2.0), -1.0);
        assert_eq!(rotation_direction(-360.0 * 4.0 + 270.0, 360.0 * 5.0 + 180.0), -1.0);
    }

    #[test]
    fn test_angle_difference() {
        assert_eq!(diff_degrees(0.0, 30.0), 30.0);
        assert_eq!(diff_degrees(0.0, 60.0), 60.0);
        assert_eq!(diff_degrees(0.0, 150.0), 150.0);
        assert_eq!(diff_degrees(90.0, 240.0), 150.0);
        assert_eq!(diff_degrees(300.0, 30.0), 90.0);
        assert_eq!(diff_degrees(270.0, 60.0), 150.0);

        assert_eq!(diff_degrees(30.0, 0.0), 30.0);
        assert_eq!(diff_degrees(60.0, 0.0), 60.0);
        assert_eq!(diff_degrees(150.0, 0.0), 150.0);
        assert_eq!(diff_degrees(240.0, 90.0), 150.0);
        assert_eq!(diff_degrees(30.0, 300.0), 90.0);
        assert_eq!(diff_degrees(60.0, 270.0), 150.0);
    }

    #[test]
    fn test_angle_sanitation() {
        assert_eq!(sanitize_degrees_int(30), 30);
        assert_eq!(sanitize_degrees_int(240), 240);
        assert_eq!(sanitize_degrees_int(360), 0);
        assert_eq!(sanitize_degrees_int(-30), 330);
        assert_eq!(sanitize_degrees_int(-750), 330);
        assert_eq!(sanitize_degrees_int(-54321), 39);

        assert_eq!(sanitize_degrees_double(30.0), 30.0);
        assert_eq!(sanitize_degrees_double(240.0), 240.0);
        assert_eq!(sanitize_degrees_double(360.0), 0.0);
        assert_eq!(sanitize_degrees_double(-30.0), 330.0);
        assert_eq!(sanitize_degrees_double(-750.0), 330.0);
        assert_eq!(sanitize_degrees_double(-54321.0), 39.0);
        assert_eq!(sanitize_degrees_double(360.125), 0.125);
        //assert_eq!(sanitize_degrees_double(-11111.11), 48.89);
    }

    #[test]
    fn test_matrix_multiply() {
        let vector_one = matrix_multiply(Vec3 { a: 1.0, b: 3.0, c: 5.0 }, K_MATRIX);
        assert_eq!(vector_one.a, 22.0);
        assert_eq!(vector_one.b, -19.0);
        assert_eq!(vector_one.c, -76.0);

        let vector_two = matrix_multiply(Vec3 { a: -11.1, b: 22.2, c: -33.3 }, K_MATRIX);
        assert_eq!(vector_two.a, -66.6);
        assert_eq!(vector_two.b, 355.2);
        assert_eq!(vector_two.c, 199.8);
    }

    #[test]
    fn test_alpha_from_int() {
        assert_eq!(alpha_from_int(0xff123456), 0xff);
        assert_eq!(alpha_from_int(0xffabcdef), 0xff);
    }

    #[test]
    fn test_red_from_int() {
        assert_eq!(red_from_int(0xff123456), 0x12);
        assert_eq!(red_from_int(0xffabcdef), 0xab);
    }

    #[test]
    fn test_green_from_int() {
        assert_eq!(green_from_int(0xff123456), 0x34);
        assert_eq!(green_from_int(0xffabcdef), 0xcd);
    }

    #[test]
    fn test_blue_from_int() {
        assert_eq!(blue_from_int(0xff123456), 0x56);
        assert_eq!(blue_from_int(0xffabcdef), 0xef);
    }

    #[test]
    fn test_opaqueness() {
        assert!(is_opaque(0xff123456));
        assert!(!is_opaque(0x00123456));
        assert!(!is_opaque(0x00001234));
    }

    #[test]
    fn test_linearized_components() {
        assert_near!(linearized(0), 0.0, 1e-4);
        assert_near!(linearized(1), 0.0303527, 1e-4);
        assert_near!(linearized(2), 0.0607054, 1e-4);
        assert_near!(linearized(8), 0.242822, 1e-4);
        assert_near!(linearized(9), 0.273174, 1e-4);
        assert_near!(linearized(16), 0.518152, 1e-4);
        assert_near!(linearized(32), 1.44438, 1e-4);
        assert_near!(linearized(64), 5.12695, 1e-4);
        assert_near!(linearized(128), 21.5861, 1e-4);
        assert_near!(linearized(255), 100.0, 1e-4);
    }

    #[test]
    fn test_delinearized_components() {
        assert_eq!(delinearized(0.0), 0);
        assert_eq!(delinearized(0.0303527), 1);
        assert_eq!(delinearized(0.0607054), 2);
        assert_eq!(delinearized(0.242822), 8);
        assert_eq!(delinearized(0.273174), 9);
        assert_eq!(delinearized(0.518152), 16);
        assert_eq!(delinearized(1.44438), 32);
        assert_eq!(delinearized(5.12695), 64);
        assert_eq!(delinearized(21.5861), 128);
        assert_eq!(delinearized(100.0), 255);

        assert_eq!(delinearized(25.0), 137);
        assert_eq!(delinearized(50.0), 188);
        assert_eq!(delinearized(75.0), 225);

        assert_eq!(delinearized(-1.0), 0);
        assert_eq!(delinearized(-10000.0), 0);
        assert_eq!(delinearized(101.0), 255);
        assert_eq!(delinearized(10000.0), 255);
    }

    #[test]
    fn test_delinearized_is_left_inverse_of_linearized() {
        assert_eq!(delinearized(linearized(0)), 0);
        assert_eq!(delinearized(linearized(1)), 1);
        assert_eq!(delinearized(linearized(2)), 2);
        assert_eq!(delinearized(linearized(8)), 8);
        assert_eq!(delinearized(linearized(9)), 9);
        assert_eq!(delinearized(linearized(16)), 16);
        assert_eq!(delinearized(linearized(32)), 32);
        assert_eq!(delinearized(linearized(64)), 64);
        assert_eq!(delinearized(linearized(128)), 128);
        assert_eq!(delinearized(linearized(255)), 255);
    }

    #[test]
    fn test_argb_from_linrgb() {
        assert_eq!(argb_from_linrgb(vec3(25.0, 50.0, 75.0)), 0xff89bce1);
        assert_eq!(argb_from_linrgb(vec3(0.03, 0.06, 0.12)), 0xff010204);
    }

    #[test]
    fn test_lstar_from_argb() {
        assert_near!(lstar_from_argb(0xff89bce1), 74.011, 1e-4);
        assert_near!(lstar_from_argb(0xff010204), 0.529651, 1e-4);
    }

    #[test]
    fn test_hex_from_argb() {
        assert_eq!(hex_from_argb(0xff89bce1), "ff89bce1");
        assert_eq!(hex_from_argb(0xff010204), "ff010204");
    }

    #[test]
    fn test_int_from_lstar() {
        // Given an L* brightness value in [0, 100], IntFromLstar returns a greyscale
        // color in ARGB format with that brightness.
        // For L* outside the domain [0, 100], returns black or white.

        assert_eq!(int_from_lstar(0.0), 0xff000000);
        assert_eq!(int_from_lstar(0.25), 0xff010101);
        assert_eq!(int_from_lstar(0.5), 0xff020202);
        assert_eq!(int_from_lstar(1.0), 0xff040404);
        assert_eq!(int_from_lstar(2.0), 0xff070707);
        assert_eq!(int_from_lstar(4.0), 0xff0e0e0e);
        assert_eq!(int_from_lstar(8.0), 0xff181818);
        assert_eq!(int_from_lstar(25.0), 0xff3b3b3b);
        assert_eq!(int_from_lstar(50.0), 0xff777777);
        assert_eq!(int_from_lstar(75.0), 0xffb9b9b9);
        assert_eq!(int_from_lstar(99.0), 0xfffcfcfc);
        assert_eq!(int_from_lstar(100.0), 0xffffffff);

        assert_eq!(int_from_lstar(-1.0), 0xff000000);
        assert_eq!(int_from_lstar(-2.0), 0xff000000);
        assert_eq!(int_from_lstar(-3.0), 0xff000000);
        assert_eq!(int_from_lstar(-9999999.0), 0xff000000);

        assert_eq!(int_from_lstar(101.0), 0xffffffff);
        assert_eq!(int_from_lstar(111.0), 0xffffffff);
        assert_eq!(int_from_lstar(9999999.0), 0xffffffff);
    }

    #[test]
    fn test_lstar_argb_roundtrip_property() {
        // Confirms that L* -> ARGB -> L* preserves original value
        // (taking ARGB rounding into consideration).

        assert_near!(lstar_from_argb(int_from_lstar(0.0)), 0.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(1.0)), 1.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(2.0)), 2.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(8.0)), 8.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(25.0)), 25.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(50.0)), 50.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(75.0)), 75.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(99.0)), 99.0, 1.0);
        assert_near!(lstar_from_argb(int_from_lstar(100.0)), 100.0, 1.0);
    }

    #[test]
    fn test_argb_lstar_roundtrip_property() {
        // Confirms that ARGB -> L* -> ARGB preserves original value
        // for greyscale colors.

        assert_eq!(int_from_lstar(lstar_from_argb(0xff000000)), 0xff000000);
        assert_eq!(int_from_lstar(lstar_from_argb(0xff010101)), 0xff010101);
        assert_eq!(int_from_lstar(lstar_from_argb(0xff020202)), 0xff020202);
        assert_eq!(int_from_lstar(lstar_from_argb(0xff111111)), 0xff111111);
        assert_eq!(int_from_lstar(lstar_from_argb(0xff333333)), 0xff333333);
        assert_eq!(int_from_lstar(lstar_from_argb(0xff777777)), 0xff777777);
        assert_eq!(int_from_lstar(lstar_from_argb(0xffbbbbbb)), 0xffbbbbbb);
        assert_eq!(int_from_lstar(lstar_from_argb(0xfffefefe)), 0xfffefefe);
        assert_eq!(int_from_lstar(lstar_from_argb(0xffffffff)), 0xffffffff);
    }

    #[test]
    fn test_y_from_lstar() {
        assert_near!(y_from_lstar(0.0), 0.0, 1e-5);
        assert_near!(y_from_lstar(0.1), 0.0110705, 1e-5);
        assert_near!(y_from_lstar(0.2), 0.0221411, 1e-5);
        assert_near!(y_from_lstar(0.3), 0.0332116, 1e-5);
        assert_near!(y_from_lstar(0.4), 0.0442822, 1e-5);
        assert_near!(y_from_lstar(0.5), 0.0553528, 1e-5);
        assert_near!(y_from_lstar(1.0), 0.1107056, 1e-5);
        assert_near!(y_from_lstar(2.0), 0.2214112, 1e-5);
        assert_near!(y_from_lstar(3.0), 0.3321169, 1e-5);
        assert_near!(y_from_lstar(4.0), 0.4428225, 1e-5);
        assert_near!(y_from_lstar(5.0), 0.5535282, 1e-5);
        assert_near!(y_from_lstar(8.0), 0.8856451, 1e-5);
        assert_near!(y_from_lstar(10.0), 1.1260199, 1e-5);
        assert_near!(y_from_lstar(15.0), 1.9085832, 1e-5);
        assert_near!(y_from_lstar(20.0), 2.9890524, 1e-5);
        assert_near!(y_from_lstar(25.0), 4.4154767, 1e-5);
        assert_near!(y_from_lstar(30.0), 6.2359055, 1e-5);
        assert_near!(y_from_lstar(40.0), 11.2509737, 1e-5);
        assert_near!(y_from_lstar(50.0), 18.4186518, 1e-5);
        assert_near!(y_from_lstar(60.0), 28.1233342, 1e-5);
        assert_near!(y_from_lstar(70.0), 40.7494157, 1e-5);
        assert_near!(y_from_lstar(80.0), 56.6812907, 1e-5);
        assert_near!(y_from_lstar(90.0), 76.3033539, 1e-5);
        assert_near!(y_from_lstar(95.0), 87.6183294, 1e-5);
        assert_near!(y_from_lstar(99.0), 97.4360239, 1e-5);
        assert_near!(y_from_lstar(100.0), 100.0, 1e-5);
    }

    #[test]
    fn test_lstar_from_y() {
        assert_near!(lstar_from_y(0.0), 0.0, 1e-5);
        assert_near!(lstar_from_y(0.1), 0.9032962, 1e-5);
        assert_near!(lstar_from_y(0.2), 1.8065925, 1e-5);
        assert_near!(lstar_from_y(0.3), 2.7098888, 1e-5);
        assert_near!(lstar_from_y(0.4), 3.6131851, 1e-5);
        assert_near!(lstar_from_y(0.5), 4.5164814, 1e-5);
        assert_near!(lstar_from_y(0.8856451), 8.0, 1e-5);
        assert_near!(lstar_from_y(1.0), 8.9914424, 1e-5);
        assert_near!(lstar_from_y(2.0), 15.4872443, 1e-5);
        assert_near!(lstar_from_y(3.0), 20.0438970, 1e-5);
        assert_near!(lstar_from_y(4.0), 23.6714419, 1e-5);
        assert_near!(lstar_from_y(5.0), 26.7347653, 1e-5);
        assert_near!(lstar_from_y(10.0), 37.8424304, 1e-5);
        assert_near!(lstar_from_y(15.0), 45.6341970, 1e-5);
        assert_near!(lstar_from_y(20.0), 51.8372115, 1e-5);
        assert_near!(lstar_from_y(25.0), 57.0754208, 1e-5);
        assert_near!(lstar_from_y(30.0), 61.6542222, 1e-5);
        assert_near!(lstar_from_y(40.0), 69.4695307, 1e-5);
        assert_near!(lstar_from_y(50.0), 76.0692610, 1e-5);
        assert_near!(lstar_from_y(60.0), 81.8381891, 1e-5);
        assert_near!(lstar_from_y(70.0), 86.9968642, 1e-5);
        assert_near!(lstar_from_y(80.0), 91.6848609, 1e-5);
        assert_near!(lstar_from_y(90.0), 95.9967686, 1e-5);
        assert_near!(lstar_from_y(95.0), 98.0335184, 1e-5);
        assert_near!(lstar_from_y(99.0), 99.6120372, 1e-5);
        assert_near!(lstar_from_y(100.0), 100.0, 1e-5);
    }

    #[test]
    fn test_y_lstar_roundtrip_property(){
        let mut y = 0.0;
        while y <= 100.0 {
            let lstar = lstar_from_y(y);
            let reconstructed_y = y_from_lstar(lstar);
            assert_near!(reconstructed_y, y, 1e-8);
            y += 0.1;
        }
    }

    #[test]
    fn test_lstar_y_roundtrip_property(){
        let mut lstar = 0.0;
        while lstar <= 100.0 {
            let y = y_from_lstar(lstar);
            let reconstructed_lstar = lstar_from_y(y);
            assert_near!(reconstructed_lstar, lstar, 1e-8);
            lstar += 0.1;
        }
    }
}


#[cfg(test)]
mod temperature_cache_test{
    use crate::{assert_near, raw_temperature};
    use crate::hct::Hct;
    use crate::TemperatureCache;

    #[test]
    fn test_raw_temperature(){
        let blue_hct = Hct::from_argb(0xff0000ff);
        let blue_temp = raw_temperature(blue_hct);
        assert_near!(blue_temp, -1.393, 0.001);

        let red_hct = Hct::from_argb(0xffff0000);
        let red_temp = raw_temperature(red_hct);
        assert_near!(red_temp, 2.351, 0.001);

        let green_hct = Hct::from_argb(0xff00ff00);
        let green_temp = raw_temperature(green_hct);
        assert_near!(green_temp, -0.267, 0.001);

        let white_hct = Hct::from_argb(0xffffffff);
        let white_temp = raw_temperature(white_hct);
        assert_near!(white_temp, -0.5, 0.001);

        let black_hct = Hct::from_argb(0xff000000);
        let black_temp = raw_temperature(black_hct);
        assert_near!(black_temp, -0.5, 0.001);
    }

    #[test]
    fn test_complement(){
        let blue_complement = TemperatureCache::new(Hct::from_argb(0xff0000ff)).get_complement().to_argb();
        assert_eq!(0xff9d0002, blue_complement);

        let red_complement = TemperatureCache::new(Hct::from_argb(0xffff0000)).get_complement().to_argb();
        assert_eq!(0xff007bfc, red_complement);

        let green_complement = TemperatureCache::new(Hct::from_argb(0xff00ff00)).get_complement().to_argb();
        assert_eq!(0xffffd2c9, green_complement);

        let white_complement = TemperatureCache::new(Hct::from_argb(0xffffffff)).get_complement().to_argb();
        assert_eq!(0xffffffff, white_complement);

        let black_complement = TemperatureCache::new(Hct::from_argb(0xff000000)).get_complement().to_argb();
        assert_eq!(0xff000000, black_complement);
    }

    #[test]
    fn test_analogous(){
        let blue_analogous = TemperatureCache::new(Hct::from_argb(0xff0000ff)).get_analogous_colors();
        assert_eq!(0xff00590c, blue_analogous[0].to_argb());
        assert_eq!(0xff00564e, blue_analogous[1].to_argb());
        assert_eq!(0xff0000ff, blue_analogous[2].to_argb());
        assert_eq!(0xff6700cc, blue_analogous[3].to_argb());
        assert_eq!(0xff81009f, blue_analogous[4].to_argb());

        let red_analogous = TemperatureCache::new(Hct::from_argb(0xffff0000)).get_analogous_colors();
        assert_eq!(0xfff60082, red_analogous[0].to_argb());
        assert_eq!(0xfffc004c, red_analogous[1].to_argb());
        assert_eq!(0xffff0000, red_analogous[2].to_argb());
        assert_eq!(0xffd95500, red_analogous[3].to_argb());
        assert_eq!(0xffaf7200, red_analogous[4].to_argb());

        let green_analogous = TemperatureCache::new(Hct::from_argb(0xff00ff00)).get_analogous_colors();
        assert_eq!(0xffcee900, green_analogous[0].to_argb());
        assert_eq!(0xff92f500, green_analogous[1].to_argb());
        assert_eq!(0xff00ff00, green_analogous[2].to_argb());
        assert_eq!(0xff00fd6f, green_analogous[3].to_argb());
        assert_eq!(0xff00fab3, green_analogous[4].to_argb());

        let black_analogous = TemperatureCache::new(Hct::from_argb(0xff000000)).get_analogous_colors();
        assert_eq!(0xff000000, black_analogous[0].to_argb());
        assert_eq!(0xff000000, black_analogous[1].to_argb());
        assert_eq!(0xff000000, black_analogous[2].to_argb());
        assert_eq!(0xff000000, black_analogous[3].to_argb());
        assert_eq!(0xff000000, black_analogous[4].to_argb());

        let white_analogous = TemperatureCache::new(Hct::from_argb(0xffffffff)).get_analogous_colors();
        assert_eq!(0xffffffff, white_analogous[0].to_argb());
        assert_eq!(0xffffffff, white_analogous[1].to_argb());
        assert_eq!(0xffffffff, white_analogous[2].to_argb());
        assert_eq!(0xffffffff, white_analogous[3].to_argb());
        assert_eq!(0xffffffff, white_analogous[4].to_argb());
    }
}