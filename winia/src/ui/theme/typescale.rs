pub mod weight {
    pub static REGULAR: &str = "typescale_weight_regular";
    pub static MEDIUM: &str = "typescale_weight_medium";
    pub static BOLD: &str = "typescale_weight_bold";
}

pub static BRAND: &str = "typescale_brand";
pub static PLAIN: &str = "typescale_plain";

macro_rules! typescale {
    ($name:ident, $font:expr, $weight:expr, $size:expr, $tracking:expr, $line_height:expr) => {
        pub mod $name {
            pub static FONT: &str = $font;
            pub static WEIGHT: &str = $weight;
            pub static SIZE: &str = $size;
            pub static TRACKING: &str = $tracking;
            pub static LINE_HEIGHT: &str = $line_height;
        }
    };
}

typescale!(
    display_large,
    "typescale_display_large_font",
    "typescale_display_large_weight",
    "typescale_display_large_size",
    "typescale_display_large_tracking",
    "typescale_display_large_line_height"
);

typescale!(
    display_medium,
    "typescale_display_medium_font",
    "typescale_display_medium_weight",
    "typescale_display_medium_size",
    "typescale_display_medium_tracking",
    "typescale_display_medium_line_height"
);

typescale!(
    display_small,
    "typescale_display_small_font",
    "typescale_display_small_weight",
    "typescale_display_small_size",
    "typescale_display_small_tracking",
    "typescale_display_small_line_height"
);

typescale!(
    headline_large,
    "typescale_headline_large_font",
    "typescale_headline_large_weight",
    "typescale_headline_large_size",
    "typescale_headline_large_tracking",
    "typescale_headline_large_line_height"
);

typescale!(
    headline_medium,
    "typescale_headline_medium_font",
    "typescale_headline_medium_weight",
    "typescale_headline_medium_size",
    "typescale_headline_medium_tracking",
    "typescale_headline_medium_line_height"
);

typescale!(
    headline_small,
    "typescale_headline_small_font",
    "typescale_headline_small_weight",
    "typescale_headline_small_size",
    "typescale_headline_small_tracking",
    "typescale_headline_small_line_height"
);

typescale!(
    title_large,
    "typescale_title_large_font",
    "typescale_title_large_weight",
    "typescale_title_large_size",
    "typescale_title_large_tracking",
    "typescale_title_large_line_height"
);

typescale!(
    title_medium,
    "typescale_title_medium_font",
    "typescale_title_medium_weight",
    "typescale_title_medium_size",
    "typescale_title_medium_tracking",
    "typescale_title_medium_line_height"
);

typescale!(
    title_small,
    "typescale_title_small_font",
    "typescale_title_small_weight",
    "typescale_title_small_size",
    "typescale_title_small_tracking",
    "typescale_title_small_line_height"
);

// typescale!(
//     label_large,
//     "typescale_label_large_font",
//     "typescale_label_large_weight",
//     "typescale_label_large_size",
//     "typescale_label_large_tracking",
//     "typescale_label_large_line_height"
// );
// 
// typescale!(
//     label_medium,
//     "typescale_label_medium_font",
//     "typescale_label_medium_weight",
//     "typescale_label_medium_size",
//     "typescale_label_medium_tracking",
//     "typescale_label_medium_line_height"
// );
// 
// typescale!(
//     label_small,
//     "typescale_label_small_font",
//     "typescale_label_small_weight",
//     "typescale_label_small_size",
//     "typescale_label_small_tracking",
//     "typescale_label_small_line_height"
// );

typescale!(
    body_large,
    "typescale_body_large_font",
    "typescale_body_large_weight",
    "typescale_body_large_size",
    "typescale_body_large_tracking",
    "typescale_body_large_line_height"
);

typescale!(
    body_medium,
    "typescale_body_medium_font",
    "typescale_body_medium_weight",
    "typescale_body_medium_size",
    "typescale_body_medium_tracking",
    "typescale_body_medium_line_height"
);

typescale!(
    body_small,
    "typescale_body_small_font",
    "typescale_body_small_weight",
    "typescale_body_small_size",
    "typescale_body_small_tracking",
    "typescale_body_small_line_height"
);

macro_rules! typescale_prominent {
    ($name:ident, $font:expr, $weight:expr, $size:expr, $tracking:expr, $line_height:expr, $weight_prominent:expr) => {
        pub mod $name {
            pub static FONT: &str = $font;
            pub static WEIGHT: &str = $weight;
            pub static SIZE: &str = $size;
            pub static TRACKING: &str = $tracking;
            pub static LINE_HEIGHT: &str = $line_height;
            pub static WEIGHT_PROMINENT: &str = $weight_prominent;
        }
    }
}

typescale_prominent!(
    label_large,
    "typescale_label_large_font",
    "typescale_label_large_weight",
    "typescale_label_large_size",
    "typescale_label_large_tracking",
    "typescale_label_large_line_height",
    "typescale_label_large_weight_prominent"
);

typescale_prominent!(
    label_medium,
    "typescale_label_medium_font",
    "typescale_label_medium_weight",
    "typescale_label_medium_size",
    "typescale_label_medium_tracking",
    "typescale_label_medium_line_height",
    "typescale_label_medium_weight_prominent"
);

typescale!(
    label_small,
    "typescale_label_small_font",
    "typescale_label_small_weight",
    "typescale_label_small_size",
    "typescale_label_small_tracking",
    "typescale_label_small_line_height"
);