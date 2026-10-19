use crate::frame::FrameDamage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PopupRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PopupPlacement {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PopupPlacement {
    pub(crate) fn damage(self) -> FrameDamage {
        FrameDamage {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

fn scaled_origin(view: i32, view_extent: i32, pixel_extent: u32) -> i64 {
    let scale = if view_extent > 0 {
        f64::from(pixel_extent) / f64::from(view_extent)
    } else {
        1.0
    };
    (f64::from(view) * scale).round() as i64
}

#[must_use]
pub(crate) fn popup_placement(
    rect: PopupRect,
    popup_width: u32,
    popup_height: u32,
    frame_width: u32,
    frame_height: u32,
) -> Option<PopupPlacement> {
    let axis = |view: i32, view_extent: i32, popup: u32, frame: u32| {
        let origin = scaled_origin(view, view_extent, popup)
            .min(i64::from(frame) - i64::from(popup))
            .max(0);
        let origin = u32::try_from(origin).ok()?;
        let extent = popup.min(frame.saturating_sub(origin));
        (extent > 0).then_some((origin, extent))
    };
    let (x, width) = axis(rect.x, rect.width, popup_width, frame_width)?;
    let (y, height) = axis(rect.y, rect.height, popup_height, frame_height)?;
    Some(PopupPlacement {
        x,
        y,
        width,
        height,
    })
}

pub(crate) fn composite_bgra(
    frame: &mut [u8],
    frame_width: u32,
    popup: &[u8],
    popup_width: u32,
    placement: PopupPlacement,
) {
    let row_bytes = placement.width as usize * 4;
    for row in 0..placement.height as usize {
        let target =
            ((placement.y as usize + row) * frame_width as usize + placement.x as usize) * 4;
        let source = row * popup_width as usize * 4;
        let (Some(target), Some(source)) = (
            frame.get_mut(target..target + row_bytes),
            popup.get(source..source + row_bytes),
        ) else {
            return;
        };
        target.copy_from_slice(source);
    }
}

#[derive(Default)]
pub(crate) struct PopupState {
    visible: bool,
    rect: PopupRect,
    pixels: Vec<u8>,
    pixel_width: u32,
    pixel_height: u32,
}

impl PopupState {
    pub(crate) fn set_visible(&mut self, visible: bool) {
        if !visible {
            *self = Self::default();
        }
        self.visible = visible;
    }

    pub(crate) fn set_rect(&mut self, rect: PopupRect) {
        self.rect = rect;
    }

    pub(crate) fn rect(&self) -> Option<PopupRect> {
        self.visible.then_some(self.rect)
    }

    pub(crate) fn store_pixels(&mut self, pixels: &[u8], width: u32, height: u32) -> bool {
        if !self.visible {
            return false;
        }
        self.pixels.clear();
        self.pixels.extend_from_slice(pixels);
        self.pixel_width = width;
        self.pixel_height = height;
        true
    }

    pub(crate) fn composite(
        &self,
        frame: &mut [u8],
        frame_width: u32,
        frame_height: u32,
    ) -> Option<FrameDamage> {
        if !self.visible || self.pixels.is_empty() {
            return None;
        }
        let placement = popup_placement(
            self.rect,
            self.pixel_width,
            self.pixel_height,
            frame_width,
            frame_height,
        )?;
        composite_bgra(
            frame,
            frame_width,
            &self.pixels,
            self.pixel_width,
            placement,
        );
        Some(placement.damage())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> PopupRect {
        PopupRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn placement_scales_view_coordinates_to_device_pixels() {
        assert_eq!(
            popup_placement(rect(10, 20, 50, 40), 100, 80, 800, 600),
            Some(PopupPlacement {
                x: 20,
                y: 40,
                width: 100,
                height: 80,
            })
        );
    }

    #[test]
    fn placement_keeps_the_popup_inside_the_frame() {
        assert_eq!(
            popup_placement(rect(780, 590, 50, 40), 50, 40, 800, 600),
            Some(PopupPlacement {
                x: 750,
                y: 560,
                width: 50,
                height: 40,
            })
        );
        assert_eq!(
            popup_placement(rect(-5, -9, 50, 40), 50, 40, 800, 600),
            Some(PopupPlacement {
                x: 0,
                y: 0,
                width: 50,
                height: 40,
            })
        );
    }

    #[test]
    fn placement_clips_a_popup_larger_than_the_frame() {
        assert_eq!(
            popup_placement(rect(0, 0, 900, 40), 900, 40, 800, 600),
            Some(PopupPlacement {
                x: 0,
                y: 0,
                width: 800,
                height: 40,
            })
        );
        assert_eq!(popup_placement(rect(0, 0, 10, 10), 10, 10, 0, 600), None);
    }

    #[test]
    fn composite_copies_popup_rows_at_the_placement() {
        let mut frame = vec![0_u8; 4 * 4 * 4];
        let popup: Vec<u8> = (0..2 * 2 * 4).map(|value| value as u8 + 1).collect();
        composite_bgra(
            &mut frame,
            4,
            &popup,
            2,
            PopupPlacement {
                x: 1,
                y: 2,
                width: 2,
                height: 2,
            },
        );
        assert_eq!(&frame[(2 * 4 + 1) * 4..(2 * 4 + 3) * 4], &popup[0..8]);
        assert_eq!(&frame[(3 * 4 + 1) * 4..(3 * 4 + 3) * 4], &popup[8..16]);
        assert!(frame[..2 * 4 * 4].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn hidden_popup_state_drops_pixels_and_skips_compositing() {
        let mut state = PopupState::default();
        assert!(!state.store_pixels(&[1; 16], 2, 2));
        state.set_visible(true);
        state.set_rect(rect(0, 0, 2, 2));
        assert!(state.store_pixels(&[7; 16], 2, 2));
        let mut frame = vec![0_u8; 4 * 4 * 4];
        assert_eq!(
            state.composite(&mut frame, 4, 4),
            Some(FrameDamage {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            })
        );
        assert_eq!(frame[0], 7);
        state.set_visible(false);
        assert_eq!(state.rect(), None);
        assert_eq!(state.composite(&mut frame, 4, 4), None);
    }
}
