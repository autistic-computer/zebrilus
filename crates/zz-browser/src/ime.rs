use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImeCharacterBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ImeCharacterBounds {
    #[must_use]
    pub fn from_view(x: i32, y: i32, width: i32, height: i32, raster_scale: f32) -> Self {
        let scale = if raster_scale.is_finite() && raster_scale > 0.0 {
            raster_scale
        } else {
            1.0
        };
        Self {
            x: x as f32 / scale,
            y: y as f32 / scale,
            width: width.max(0) as f32 / scale,
            height: height.max(0) as f32 / scale,
        }
    }

    fn right(self) -> f32 {
        self.x + self.width
    }

    fn bottom(self) -> f32 {
        self.y + self.height
    }

    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: self.right().max(other.right()) - x,
            height: self.bottom().max(other.bottom()) - y,
        }
    }
}

#[must_use]
pub fn ime_range_bounds(
    bounds: &[ImeCharacterBounds],
    range: Range<usize>,
) -> Option<ImeCharacterBounds> {
    let last = *bounds.last()?;
    if range.start >= bounds.len() {
        return Some(ImeCharacterBounds {
            x: last.right(),
            width: 0.0,
            ..last
        });
    }
    let end = range.end.clamp(range.start + 1, bounds.len());
    bounds[range.start..end]
        .iter()
        .copied()
        .reduce(ImeCharacterBounds::union)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> ImeCharacterBounds {
        ImeCharacterBounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn view_rects_unscale_into_pane_pixels() {
        assert_eq!(
            ImeCharacterBounds::from_view(40, 20, 16, 32, 2.0),
            bounds(20.0, 10.0, 8.0, 16.0)
        );
        assert_eq!(
            ImeCharacterBounds::from_view(30, 45, 15, 30, 1.5 * 1.25),
            bounds(16.0, 24.0, 8.0, 16.0)
        );
        assert_eq!(
            ImeCharacterBounds::from_view(7, 9, 3, 12, 1.0),
            bounds(7.0, 9.0, 3.0, 12.0)
        );
        assert_eq!(
            ImeCharacterBounds::from_view(7, 9, -3, 12, 0.0),
            bounds(7.0, 9.0, 0.0, 12.0)
        );
    }

    #[test]
    fn range_bounds_cover_characters_or_sit_after_the_last() {
        let characters = [
            bounds(10.0, 5.0, 8.0, 16.0),
            bounds(18.0, 5.0, 8.0, 16.0),
            bounds(26.0, 6.0, 9.0, 16.0),
        ];
        assert_eq!(
            ime_range_bounds(&characters, 0..2),
            Some(bounds(10.0, 5.0, 16.0, 16.0))
        );
        assert_eq!(
            ime_range_bounds(&characters, 1..1),
            Some(bounds(18.0, 5.0, 8.0, 16.0))
        );
        assert_eq!(
            ime_range_bounds(&characters, 3..3),
            Some(bounds(35.0, 6.0, 0.0, 16.0))
        );
        assert_eq!(
            ime_range_bounds(&characters, 1..9),
            Some(bounds(18.0, 5.0, 17.0, 17.0))
        );
        assert_eq!(ime_range_bounds(&[], 0..1), None);
    }
}
