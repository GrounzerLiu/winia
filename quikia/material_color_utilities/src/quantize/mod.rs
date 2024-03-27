mod wu;
pub use wu::*;
mod lab;
pub use lab::*;
mod wsmeans;
pub use wsmeans::*;
mod celebi;
pub use celebi::*;



#[cfg(test)]
mod wu_test {
    use crate::quantize::quantize_wu;
    use crate::utils::Argb;

    #[test]
    fn test_full_image() {
        let mut pixels: Vec<Argb> = vec![0; 12544];
        for i in 0..pixels.len() {
            pixels[i] = i as Argb % 8000;
        }
        let max_colors = 128;
        quantize_wu(&pixels, max_colors);
    }

    #[test]
    fn test_two_red_three_green() {
        let mut pixels: Vec<Argb> = vec![0; 5];
        pixels[0] = 0xffff0000;
        pixels[1] = 0xffff0000;
        pixels[2] = 0xffff0000;
        pixels[3] = 0xff00ff00;
        pixels[4] = 0xff00ff00;
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_one_red() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xffff0000;
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0xffff0000);
    }

    #[test]
    fn test_one_green() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff00ff00;
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0xff00ff00);
    }

    #[test]
    fn test_one_blue() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff0000ff;
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0xff0000ff);
    }

    #[test]
    fn test_five_blue() {
        let mut pixels: Vec<Argb> = vec![0; 5];
        for i in 0..pixels.len() {
            pixels[i] = 0xff0000ff;
        }
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0xff0000ff);
    }

    #[test]
    fn test_one_red_and_o() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff141216;
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0xff141216);
    }

    #[test]
    fn test_red_green_blue() {
        let mut pixels: Vec<Argb> = vec![0; 3];
        pixels[0] = 0xffff0000;
        pixels[1] = 0xff00ff00;
        pixels[2] = 0xff0000ff;
        let max_colors = 256;
        let result = quantize_wu(&pixels, max_colors);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0xff0000ff);
        assert_eq!(result[1], 0xffff0000);
        assert_eq!(result[2], 0xff00ff00);
    }

    #[test]
    fn test_testonly() {
        let mut pixels: Vec<Argb> = vec![0; 5];
        pixels[0] = 0xff010203;
        pixels[1] = 0xff665544;
        pixels[2] = 0xff708090;
        pixels[3] = 0xffc0ffee;
        pixels[4] = 0xfffedcba;
        let max_colors = 256;
        let _result = quantize_wu(&pixels, max_colors);
    }
}

#[cfg(test)]
mod wsmeans_test {

    use crate::quantize::wsmeans::quantize_wsmeans;
    use crate::utils::Argb;

    #[test]
    fn test_full_image() {
        let mut pixels: Vec<Argb> = vec![0; 12544];
        for i in 0..pixels.len() {
            pixels[i] = i as Argb % 8000;
        }
        let starting_clusters: Vec<Argb> = vec![];

        let iterations = 1;
        let max_colors = 128;

        let mut _sum = 0.0;

        for _i in 0..iterations {
            let begin = std::time::Instant::now();
            quantize_wsmeans(&pixels, &starting_clusters, max_colors);
            let end = std::time::Instant::now();
            let time_spent = end.duration_since(begin).as_secs_f64();
            _sum += time_spent;
        }
    }

    #[test]
    fn test_one_red_and_o() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff141216;
        let starting_clusters: Vec<Argb> = vec![];
        let max_colors = 256;
        let result = quantize_wsmeans(&pixels, &starting_clusters, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff141216], 1);
    }

    #[test]
    fn test_one_red() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xffff0000;
        let starting_clusters: Vec<Argb> = vec![];
        let max_colors = 256;
        let result = quantize_wsmeans(&pixels, &starting_clusters, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xffff0000], 1);
    }

    #[test]
    fn test_one_green() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff00ff00;
        let starting_clusters: Vec<Argb> = vec![];
        let max_colors = 256;
        let result = quantize_wsmeans(&pixels, &starting_clusters, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff00ff00], 1);
    }

    #[test]
    fn test_one_blue() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff0000ff;
        let starting_clusters: Vec<Argb> = vec![];
        let max_colors = 256;
        let result = quantize_wsmeans(&pixels, &starting_clusters, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff0000ff], 1);
    }

    #[test]
    fn test_five_blue() {
        let mut pixels: Vec<Argb> = vec![0; 5];
        for i in 0..pixels.len() {
            pixels[i] = 0xff0000ff;
        }
        let starting_clusters: Vec<Argb> = vec![];
        let max_colors = 256;
        let result = quantize_wsmeans(&pixels, &starting_clusters, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff0000ff], 5);
    }
}

#[cfg(test)]
mod celebi_test{
    use crate::quantize::celebi::quantize_celebi;
    use crate::utils::Argb;

    #[test]
    fn test_full_image() {
        let mut pixels: Vec<Argb> = vec![0; 12544];
        for i in 0..pixels.len() {
            pixels[i] = i as Argb % 8000;
        }

        let iterations = 1;
        let max_colors = 128;
        let mut _sum = 0.0;

        for _i in 0..iterations {
            let begin = std::time::Instant::now();
            quantize_celebi(&pixels, max_colors);
            let end = std::time::Instant::now();
            let time_spent = end.duration_since(begin).as_secs_f64();
            _sum += time_spent;
        }
    }

    #[test]
    fn test_one_red() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xffff0000;
        let max_colors = 256;
        let result = quantize_celebi(&pixels, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xffff0000], 1);
    }

    #[test]
    fn test_one_green() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff00ff00;
        let max_colors = 256;
        let result = quantize_celebi(&pixels, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff00ff00], 1);
    }

    #[test]
    fn test_one_blue() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xff0000ff;
        let max_colors = 256;
        let result = quantize_celebi(&pixels, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff0000ff], 1);
    }

    #[test]
    fn test_five_blue() {
        let mut pixels: Vec<Argb> = vec![0; 5];
        for i in 0..pixels.len() {
            pixels[i] = 0xff0000ff;
        }
        let max_colors = 256;
        let result = quantize_celebi(&pixels, max_colors);
        assert_eq!(result.color_to_count.len(), 1);
        assert_eq!(result.color_to_count[&0xff0000ff], 5);
    }

    #[test]
    fn test_one_red_one_green_one_blue() {
        let mut pixels: Vec<Argb> = vec![0; 3];
        pixels[0] = 0xffff0000;
        pixels[1] = 0xff00ff00;
        pixels[2] = 0xff0000ff;
        let max_colors = 256;
        let result = quantize_celebi(&pixels, max_colors);
        assert_eq!(result.color_to_count.len(), 3);
        assert_eq!(result.color_to_count[&0xffff0000], 1);
        assert_eq!(result.color_to_count[&0xff00ff00], 1);
        assert_eq!(result.color_to_count[&0xff0000ff], 1);
    }

    #[test]
    fn test_two_red_three_green() {
        let mut pixels: Vec<Argb> = vec![0; 5];
        pixels[0] = 0xffff0000;
        pixels[1] = 0xffff0000;
        pixels[2] = 0xff00ff00;
        pixels[3] = 0xff00ff00;
        pixels[4] = 0xff00ff00;
        let max_colors = 256;
        let result = quantize_celebi(&pixels, max_colors);
        assert_eq!(result.color_to_count.len(), 2);
        assert_eq!(result.color_to_count[&0xffff0000], 2);
        assert_eq!(result.color_to_count[&0xff00ff00], 3);
    }

    #[test]
    fn test_no_colors() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0xFFFFFFFF;
        let max_colors = 0;
        let result = quantize_celebi(&pixels, max_colors);
        assert!(result.color_to_count.is_empty());
    }

    #[test]
    fn test_single_transparent() {
        let mut pixels: Vec<Argb> = vec![0; 1];
        pixels[0] = 0x20F93013;
        let max_colors = 1;
        let result = quantize_celebi(&pixels, max_colors);
        assert!(result.color_to_count.is_empty());
    }

    #[test]
    fn test_too_many_colors() {
        let pixels: Vec<Argb> = vec![0; 1];
        let max_colors = 32767;
        let result = quantize_celebi(&pixels, max_colors);
        assert!(result.color_to_count.is_empty());
    }
}