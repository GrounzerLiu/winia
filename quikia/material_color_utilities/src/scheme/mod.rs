mod variant;
pub use variant::*;
mod dynamic_scheme;
pub use dynamic_scheme::*;
mod scheme_vibrant;
pub use scheme_vibrant::*;
mod scheme;
pub use scheme::*;
mod scheme_content;
pub use scheme_content::*;
mod scheme_expressive;
pub use scheme_expressive::*;
mod scheme_fidelity;
pub use scheme_fidelity::*;
mod scheme_fruit_salad;
pub use scheme_fruit_salad::*;
mod scheme_monochrome;
pub use scheme_monochrome::*;
mod scheme_neutral;
pub use scheme_neutral::*;
mod scheme_rainbow;
pub use scheme_rainbow::*;
mod scheme_tonal_spot;
pub use scheme_tonal_spot::*;


#[cfg(test)]
mod scheme_test {
    use crate::assert_near;
    use crate::scheme::{material_dark_color_scheme, material_dark_content_color_scheme, material_light_color_scheme, material_light_content_color_scheme};
    use crate::utils::{hex_from_argb, lstar_from_argb};

    #[test]
    fn test_surface_tones() {
        let color = 0xff0000ff;
        let dark = material_dark_color_scheme(color);
        assert_near!(lstar_from_argb(dark.surface), 10.0, 0.1);

        let light = material_light_color_scheme(color);
        assert_near!(lstar_from_argb(light.surface), 99.0, 0.1);
    }

    #[test]
    fn test_blue_light_scheme() {
        let scheme = material_light_color_scheme(0xff0000ff);
        assert_eq!(hex_from_argb(scheme.primary), "ff343dff");
    }

    #[test]
    fn test_blue_dark_scheme() {
        let scheme = material_dark_color_scheme(0xff0000ff);
        assert_eq!(hex_from_argb(scheme.primary), "ffbec2ff");
    }

    #[test]
    fn test_third_party_light_scheme() {
        let scheme = material_light_color_scheme(0xff6750a4);
        assert_eq!(hex_from_argb(scheme.primary), "ff6750a4");
        assert_eq!(hex_from_argb(scheme.secondary), "ff625b71");
        assert_eq!(hex_from_argb(scheme.tertiary), "ff7e5260");
        assert_eq!(hex_from_argb(scheme.surface), "fffffbff");
        assert_eq!(hex_from_argb(scheme.on_surface), "ff1c1b1e");
    }

    #[test]
    fn test_third_party_dark_scheme() {
        let scheme = material_dark_color_scheme(0xff6750a4);
        assert_eq!(hex_from_argb(scheme.primary), "ffcfbcff");
        assert_eq!(hex_from_argb(scheme.secondary), "ffcbc2db");
        assert_eq!(hex_from_argb(scheme.tertiary), "ffefb8c8");
        assert_eq!(hex_from_argb(scheme.surface), "ff1c1b1e");
        assert_eq!(hex_from_argb(scheme.on_surface), "ffe6e1e6");
    }

    #[test]
    fn test_light_scheme_from_high_chroma_color() {
        let scheme = material_light_color_scheme(0xfffa2bec);
        assert_eq!(hex_from_argb(scheme.primary), "ffab00a2");
        assert_eq!(hex_from_argb(scheme.on_primary), "ffffffff");
        assert_eq!(hex_from_argb(scheme.primary_container), "ffffd7f3");
        assert_eq!(hex_from_argb(scheme.on_primary_container), "ff390035");
        assert_eq!(hex_from_argb(scheme.secondary), "ff6e5868");
        assert_eq!(hex_from_argb(scheme.on_secondary), "ffffffff");
        assert_eq!(hex_from_argb(scheme.secondary_container), "fff8daee");
        assert_eq!(hex_from_argb(scheme.on_secondary_container), "ff271624");
        assert_eq!(hex_from_argb(scheme.tertiary), "ff815343");
        assert_eq!(hex_from_argb(scheme.on_tertiary), "ffffffff");
        assert_eq!(hex_from_argb(scheme.tertiary_container), "ffffdbd0");
        assert_eq!(hex_from_argb(scheme.on_tertiary_container), "ff321207");
        assert_eq!(hex_from_argb(scheme.error), "ffba1a1a");
        assert_eq!(hex_from_argb(scheme.on_error), "ffffffff");
        assert_eq!(hex_from_argb(scheme.error_container), "ffffdad6");
        assert_eq!(hex_from_argb(scheme.on_error_container), "ff410002");
        assert_eq!(hex_from_argb(scheme.background), "fffffbff");
        assert_eq!(hex_from_argb(scheme.on_background), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.surface), "fffffbff");
        assert_eq!(hex_from_argb(scheme.on_surface), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.surface_variant), "ffeedee7");
        assert_eq!(hex_from_argb(scheme.on_surface_variant), "ff4e444b");
        assert_eq!(hex_from_argb(scheme.outline), "ff80747b");
        assert_eq!(hex_from_argb(scheme.outline_variant), "ffd2c2cb");
        assert_eq!(hex_from_argb(scheme.shadow), "ff000000");
        assert_eq!(hex_from_argb(scheme.scrim), "ff000000");
        assert_eq!(hex_from_argb(scheme.inverse_surface), "ff342f32");
        assert_eq!(hex_from_argb(scheme.inverse_on_surface), "fff8eef2");
        assert_eq!(hex_from_argb(scheme.inverse_primary), "ffffabee");
    }

    #[test]
    fn test_dark_scheme_from_high_chroma_color() {
        let scheme = material_dark_color_scheme(0xfffa2bec);
        assert_eq!(hex_from_argb(scheme.primary), "ffffabee");
        assert_eq!(hex_from_argb(scheme.on_primary), "ff5c0057");
        assert_eq!(hex_from_argb(scheme.primary_container), "ff83007b");
        assert_eq!(hex_from_argb(scheme.on_primary_container), "ffffd7f3");
        assert_eq!(hex_from_argb(scheme.secondary), "ffdbbed1");
        assert_eq!(hex_from_argb(scheme.on_secondary), "ff3e2a39");
        assert_eq!(hex_from_argb(scheme.secondary_container), "ff554050");
        assert_eq!(hex_from_argb(scheme.on_secondary_container), "fff8daee");
        assert_eq!(hex_from_argb(scheme.tertiary), "fff5b9a5");
        assert_eq!(hex_from_argb(scheme.on_tertiary), "ff4c2619");
        assert_eq!(hex_from_argb(scheme.tertiary_container), "ff663c2d");
        assert_eq!(hex_from_argb(scheme.on_tertiary_container), "ffffdbd0");
        assert_eq!(hex_from_argb(scheme.error), "ffffb4ab");
        assert_eq!(hex_from_argb(scheme.on_error), "ff690005");
        assert_eq!(hex_from_argb(scheme.error_container), "ff93000a");
        assert_eq!(hex_from_argb(scheme.on_error_container), "ffffb4ab");
        assert_eq!(hex_from_argb(scheme.background), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.on_background), "ffeae0e4");
        assert_eq!(hex_from_argb(scheme.surface), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.on_surface), "ffeae0e4");
        assert_eq!(hex_from_argb(scheme.surface_variant), "ff4e444b");
        assert_eq!(hex_from_argb(scheme.on_surface_variant), "ffd2c2cb");
        assert_eq!(hex_from_argb(scheme.outline), "ff9a8d95");
        assert_eq!(hex_from_argb(scheme.outline_variant), "ff4e444b");
        assert_eq!(hex_from_argb(scheme.shadow), "ff000000");
        assert_eq!(hex_from_argb(scheme.scrim), "ff000000");
        assert_eq!(hex_from_argb(scheme.inverse_surface), "ffeae0e4");
        assert_eq!(hex_from_argb(scheme.inverse_on_surface), "ff342f32");
        assert_eq!(hex_from_argb(scheme.inverse_primary), "ffab00a2");
    }

    #[test]
    fn test_light_content_scheme_from_high_chroma_color() {
        let scheme = material_light_content_color_scheme(0xfffa2bec);
        assert_eq!(hex_from_argb(scheme.primary), "ffab00a2");
        assert_eq!(hex_from_argb(scheme.on_primary), "ffffffff");
        assert_eq!(hex_from_argb(scheme.primary_container), "ffffd7f3");
        assert_eq!(hex_from_argb(scheme.on_primary_container), "ff390035");
        assert_eq!(hex_from_argb(scheme.secondary), "ff7f4e75");
        assert_eq!(hex_from_argb(scheme.on_secondary), "ffffffff");
        assert_eq!(hex_from_argb(scheme.secondary_container), "ffffd7f3");
        assert_eq!(hex_from_argb(scheme.on_secondary_container), "ff330b2f");
        assert_eq!(hex_from_argb(scheme.tertiary), "ff9c4323");
        assert_eq!(hex_from_argb(scheme.on_tertiary), "ffffffff");
        assert_eq!(hex_from_argb(scheme.tertiary_container), "ffffdbd0");
        assert_eq!(hex_from_argb(scheme.on_tertiary_container), "ff390c00");
        assert_eq!(hex_from_argb(scheme.error), "ffba1a1a");
        assert_eq!(hex_from_argb(scheme.on_error), "ffffffff");
        assert_eq!(hex_from_argb(scheme.error_container), "ffffdad6");
        assert_eq!(hex_from_argb(scheme.on_error_container), "ff410002");
        assert_eq!(hex_from_argb(scheme.background), "fffffbff");
        assert_eq!(hex_from_argb(scheme.on_background), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.surface), "fffffbff");
        assert_eq!(hex_from_argb(scheme.on_surface), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.surface_variant), "ffeedee7");
        assert_eq!(hex_from_argb(scheme.on_surface_variant), "ff4e444b");
        assert_eq!(hex_from_argb(scheme.outline), "ff80747b");
        assert_eq!(hex_from_argb(scheme.outline_variant), "ffd2c2cb");
        assert_eq!(hex_from_argb(scheme.shadow), "ff000000");
        assert_eq!(hex_from_argb(scheme.scrim), "ff000000");
        assert_eq!(hex_from_argb(scheme.inverse_surface), "ff342f32");
        assert_eq!(hex_from_argb(scheme.inverse_on_surface), "fff8eef2");
        assert_eq!(hex_from_argb(scheme.inverse_primary), "ffffabee");
    }

    #[test]
    fn test_dark_content_scheme_from_high_chroma_color() {
        let scheme = material_dark_content_color_scheme(0xfffa2bec);
        assert_eq!(hex_from_argb(scheme.primary), "ffffabee");
        assert_eq!(hex_from_argb(scheme.on_primary), "ff5c0057");
        assert_eq!(hex_from_argb(scheme.primary_container), "ff83007b");
        assert_eq!(hex_from_argb(scheme.on_primary_container), "ffffd7f3");
        assert_eq!(hex_from_argb(scheme.secondary), "fff0b4e1");
        assert_eq!(hex_from_argb(scheme.on_secondary), "ff4b2145");
        assert_eq!(hex_from_argb(scheme.secondary_container), "ff64375c");
        assert_eq!(hex_from_argb(scheme.on_secondary_container), "ffffd7f3");
        assert_eq!(hex_from_argb(scheme.tertiary), "ffffb59c");
        assert_eq!(hex_from_argb(scheme.on_tertiary), "ff5c1900");
        assert_eq!(hex_from_argb(scheme.tertiary_container), "ff7d2c0d");
        assert_eq!(hex_from_argb(scheme.on_tertiary_container), "ffffdbd0");
        assert_eq!(hex_from_argb(scheme.error), "ffffb4ab");
        assert_eq!(hex_from_argb(scheme.on_error), "ff690005");
        assert_eq!(hex_from_argb(scheme.error_container), "ff93000a");
        assert_eq!(hex_from_argb(scheme.on_error_container), "ffffb4ab");
        assert_eq!(hex_from_argb(scheme.background), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.on_background), "ffeae0e4");
        assert_eq!(hex_from_argb(scheme.surface), "ff1f1a1d");
        assert_eq!(hex_from_argb(scheme.on_surface), "ffeae0e4");
        assert_eq!(hex_from_argb(scheme.surface_variant), "ff4e444b");
        assert_eq!(hex_from_argb(scheme.on_surface_variant), "ffd2c2cb");
        assert_eq!(hex_from_argb(scheme.outline), "ff9a8d95");
        assert_eq!(hex_from_argb(scheme.outline_variant), "ff4e444b");
        assert_eq!(hex_from_argb(scheme.shadow), "ff000000");
        assert_eq!(hex_from_argb(scheme.scrim), "ff000000");
        assert_eq!(hex_from_argb(scheme.inverse_surface), "ffeae0e4");
        assert_eq!(hex_from_argb(scheme.inverse_on_surface), "ff342f32");
        assert_eq!(hex_from_argb(scheme.inverse_primary), "ffab00a2");
    }
}


