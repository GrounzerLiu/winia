pub static TEXT:&str = "text";


pub mod button {
    pub static ELEVATED_BUTTON: &str = "elevated_button";
    pub static FILLED_BUTTON: &str = "filled_button";
    pub static FILLED_TONAL_BUTTON: &str = "filled_tonal_button";
    pub static OUTLINED_BUTTON: &str = "outlined_button";
    pub static TEXT_BUTTON: &str = "text_button";

    pub mod container {
        pub static HEIGHT: &str = "button_container_height";
        pub static ELEVATION: &str = "button_container_elevation";
        pub static COLOR: &str = "button_container_color";
    }
    pub mod label {
        pub static LINE_HEIGHT: &str = "button_label_line_height";
        pub static SIZE: &str = "button_label_size";
        pub static WEIGHT: &str = "button_label_weight";
        pub static COLOR: &str = "button_label_color";
    }
    pub mod icon {
        pub static SIZE: &str = "button_icon_size";
        pub static COLOR: &str = "button_icon_color";
    }
}