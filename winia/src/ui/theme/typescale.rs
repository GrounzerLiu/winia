pub mod weight {
    pub static REGULAR: &str = "typescale_weight_regular";
    pub static MEDIUM: &str = "typescale_weight_medium";
    pub static BOLD: &str = "typescale_weight_bold";
}

pub static BRAND: &str = "typescale_brand";
pub static PLAIN: &str = "typescale_plain";

pub static DISPLAY_LARGE: &str = "typescale_display_large";
pub static DISPLAY_MEDIUM: &str = "typescale_display_medium";
pub static DISPLAY_SMALL: &str = "typescale_display_small";
pub static HEADLINE_LARGE: &str = "typescale_headline_large";
pub static HEADLINE_MEDIUM: &str = "typescale_headline_medium";
pub static HEADLINE_SMALL: &str = "typescale_headline_small";
pub static TITLE_LARGE: &str = "typescale_title_large";
pub static TITLE_MEDIUM: &str = "typescale_title_medium";
pub static TITLE_SMALL: &str = "typescale_title_small";
pub static BODY_LARGE: &str = "typescale_body_large";
pub static BODY_MEDIUM: &str = "typescale_body_medium";
pub static BODY_SMALL: &str = "typescale_body_small";
pub static  LABEL_LARGE: &str = "typescale_label_large";
pub static  LABEL_MEDIUM: &str = "typescale_label_medium";
pub static  LABEL_SMALL: &str = "typescale_label_small";

#[derive(Debug, Clone, PartialEq)]
pub struct TypeScale {
    pub font_name: String,
    pub font_weight: f32,
    pub font_size: f32,
    pub font_tracking: f32,
    pub line_height: f32,
}

impl TypeScale {
    pub fn new(
        font_name: impl Into<String>,
        font_weight: f32,
        font_size: f32,
        font_tracking: f32,
        line_height: f32,
    ) -> Self {
        TypeScale {
            font_name: font_name.into(),
            font_weight,
            font_size,
            font_tracking,
            line_height,
        }
    }
}