use crate::quantize::quantize_wu;
use crate::quantize::wsmeans::{quantize_wsmeans, QuantizerResult};
use crate::utils::{Argb, is_opaque};

pub fn quantize_celebi(pixels: &[Argb], max_colors: u16) -> QuantizerResult {
    let mut max_colors = max_colors;
    if max_colors == 0 || pixels.is_empty() {
        return QuantizerResult::default();
    }

    if max_colors > 256 {
        max_colors = 256;
    }

    let pixel_count = pixels.len();

    let mut opaque_pixels: Vec<Argb> = Vec::with_capacity(pixel_count);

    pixels.iter().for_each(|pixel|{
        if is_opaque(*pixel){
            opaque_pixels.push(*pixel);
        }
    });

    let wu_result = quantize_wu(&opaque_pixels, max_colors);

    let result =
        quantize_wsmeans(&opaque_pixels, &wu_result, max_colors);

    println!("pixels: {:#?}", pixels);
    println!("opaque: {:#?}", opaque_pixels);
    println!("wu_result: {:#?}", wu_result);
    println!("celebi: {:#?}", result);
    //println!("celebi: {} -> {}", opaque_pixels.len(), result.color_to_count.len());
    result
}