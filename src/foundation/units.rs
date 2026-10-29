use ala::{App, Global, Pixels, Rems, rems};

pub const BASE_UI_FONT_SIZE: f32 = 16.0;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct UiZoom(pub f32);

impl Default for UiZoom {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Global for UiZoom {}

impl UiZoom {
    #[must_use]
    pub fn get(cx: &App) -> f32 {
        cx.try_global::<Self>().map_or(1.0, |zoom| zoom.0)
    }

    #[must_use]
    pub fn unzoomed(value: Pixels, cx: &App) -> Pixels {
        value / Self::get(cx)
    }
}

pub const fn rems_from_px(value: f32) -> Rems {
    rems(value / BASE_UI_FONT_SIZE)
}

#[cfg(test)]
mod tests {
    use ala::px;

    use super::*;

    #[test]
    fn larger_root_rem_scales_the_design_measurement() {
        assert_eq!(rems_from_px(36.0).to_pixels(px(20.0)), px(45.0));
    }
}
