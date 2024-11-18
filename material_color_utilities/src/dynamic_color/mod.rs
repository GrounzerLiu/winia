mod contrast_curve;
pub use contrast_curve::*;
mod dynamic_color;
pub use dynamic_color::*;
mod tone_delta_pair;
pub use tone_delta_pair::*;
pub mod material_dynamic_colors;


#[cfg(test)]
mod dynamic_color_test {
    use crate::hct::Hct;
    use crate::scheme::{DynamicScheme, DynamicSchemeOptions, scheme_vibrant};
    use crate::dynamic_color::material_dynamic_colors;

    #[test]
    fn test() {
        let s = DynamicScheme::new(
            DynamicSchemeOptions {
                contrast_level: 0.5,
                ..scheme_vibrant(Hct::from_argb(0xFFFF0000), false)
            }
        );
        assert_eq!(material_dynamic_colors::primary_palette_key_color().get_argb(&s), 0xfffe0000);
        assert_eq!(material_dynamic_colors::secondary_palette_key_color().get_argb(&s), 0xff9c6c54);
        assert_eq!(material_dynamic_colors::tertiary_palette_key_color().get_argb(&s), 0xff9f6c39);
        assert_eq!(material_dynamic_colors::neutral_palette_key_color().get_argb(&s), 0xff89726e);
        assert_eq!(material_dynamic_colors::neutral_variant_palette_key_color().get_argb(&s), 0xff8c716d);
        assert_eq!(material_dynamic_colors::background().get_argb(&s), 0xfffff8f6);
        assert_eq!(material_dynamic_colors::on_background().get_argb(&s), 0xff271815);
        assert_eq!(material_dynamic_colors::surface().get_argb(&s), 0xfffff8f6);
        assert_eq!(material_dynamic_colors::surface_dim().get_argb(&s), 0xfff0d4cf);
        assert_eq!(material_dynamic_colors::surface_bright().get_argb(&s), 0xfffff8f6);
        assert_eq!(material_dynamic_colors::surface_container_lowest().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::surface_container_low().get_argb(&s), 0xfffff0ee);
        assert_eq!(material_dynamic_colors::surface_container().get_argb(&s), 0xffffe9e6);
        assert_eq!(material_dynamic_colors::surface_container_high().get_argb(&s), 0xffffe2dd);
        assert_eq!(material_dynamic_colors::surface_container_highest().get_argb(&s), 0xfff9dcd8);
        assert_eq!(material_dynamic_colors::on_surface().get_argb(&s), 0xff271815);
        assert_eq!(material_dynamic_colors::surface_variant().get_argb(&s), 0xfffddbd5);
        assert_eq!(material_dynamic_colors::on_surface_variant().get_argb(&s), 0xff543d3a);
        assert_eq!(material_dynamic_colors::inverse_surface().get_argb(&s), 0xff3d2c29);
        assert_eq!(material_dynamic_colors::inverse_on_surface().get_argb(&s), 0xffffedea);
        assert_eq!(material_dynamic_colors::outline().get_argb(&s), 0xff725955);
        assert_eq!(material_dynamic_colors::outline_variant().get_argb(&s), 0xff907470);
        assert_eq!(material_dynamic_colors::shadow().get_argb(&s), 0xff000000);
        assert_eq!(material_dynamic_colors::scrim().get_argb(&s), 0xff000000);
        assert_eq!(material_dynamic_colors::surface_tint().get_argb(&s), 0xffc00100);
        assert_eq!(material_dynamic_colors::primary().get_argb(&s), 0xff8c0100);
        assert_eq!(material_dynamic_colors::on_primary().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::primary_container().get_argb(&s), 0xffeb0000);
        assert_eq!(material_dynamic_colors::on_primary_container().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::inverse_primary().get_argb(&s), 0xffffb4a8);
        assert_eq!(material_dynamic_colors::secondary().get_argb(&s), 0xff603924);
        assert_eq!(material_dynamic_colors::on_secondary().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::secondary_container().get_argb(&s), 0xff996952);
        assert_eq!(material_dynamic_colors::on_secondary_container().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::tertiary().get_argb(&s), 0xff633909);
        assert_eq!(material_dynamic_colors::on_tertiary().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::tertiary_container().get_argb(&s), 0xff9d6937);
        assert_eq!(material_dynamic_colors::on_tertiary_container().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::error().get_argb(&s), 0xff8c0009);
        assert_eq!(material_dynamic_colors::on_error().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::error_container().get_argb(&s), 0xffda342e);
        assert_eq!(material_dynamic_colors::on_error_container().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::primary_fixed().get_argb(&s), 0xffeb0000);
        assert_eq!(material_dynamic_colors::primary_fixed_dim().get_argb(&s), 0xffbc0100);
        assert_eq!(material_dynamic_colors::on_primary_fixed().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::on_primary_fixed_variant().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::secondary_fixed().get_argb(&s), 0xff996952);
        assert_eq!(material_dynamic_colors::secondary_fixed_dim().get_argb(&s), 0xff7d513b);
        assert_eq!(material_dynamic_colors::on_secondary_fixed().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::on_secondary_fixed_variant().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::tertiary_fixed().get_argb(&s), 0xff9d6937);
        assert_eq!(material_dynamic_colors::tertiary_fixed_dim().get_argb(&s), 0xff805121);
        assert_eq!(material_dynamic_colors::on_tertiary_fixed().get_argb(&s), 0xffffffff);
        assert_eq!(material_dynamic_colors::on_tertiary_fixed_variant().get_argb(&s), 0xffffffff);
    }
}