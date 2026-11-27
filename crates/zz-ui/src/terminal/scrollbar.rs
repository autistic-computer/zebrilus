use super::*;

pub fn scrollbar_strip_hit(
    surface: Bounds<Pixels>,
    scrollbar: ScrollbarState,
    position: Point<Pixels>,
    gutter: Pixels,
) -> bool {
    zz_client::terminal_view::scrollbar_strip_hit(
        f32::from(surface.right()),
        f32::from(surface.origin.y),
        f32::from(surface.bottom()),
        f32::from(gutter),
        scrollbar,
        f32::from(position.x),
        f32::from(position.y),
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollbarFade {
    shown_at: Option<web_time::Instant>,
    seen: Option<ScrollbarState>,
}

impl ScrollbarFade {
    pub fn show(&mut self, now: web_time::Instant) {
        self.shown_at = Some(now);
    }

    pub fn frame(
        &mut self,
        scrollbar: ScrollbarState,
        now: web_time::Instant,
        reduce_motion: bool,
    ) -> Option<(f32, Duration)> {
        let at_bottom = |bar: ScrollbarState| bar.offset >= bar.total.saturating_sub(bar.len);
        if self.seen.replace(scrollbar).is_some_and(|seen| {
            seen.offset != scrollbar.offset && !(at_bottom(seen) && at_bottom(scrollbar))
        }) {
            self.shown_at = Some(now);
        }
        let frame = self
            .shown_at
            .filter(|_| scrollbar.total > scrollbar.len)
            .and_then(|shown_at| {
                fade_frame(
                    now.saturating_duration_since(shown_at).as_secs_f32(),
                    reduce_motion,
                )
            });
        if frame.is_none() {
            self.shown_at = None;
        }
        frame
    }
}

pub fn scroll_fraction(surface: Bounds<Pixels>, position: Point<Pixels>) -> u32 {
    zz_client::terminal_view::scroll_fraction(
        f32::from(surface.origin.y),
        f32::from(surface.size.height),
        f32::from(position.y),
    )
}

pub(super) fn scrollbar_quad(
    scrollbar: ScrollbarState,
    bounds: Bounds<Pixels>,
    overscroll: Pixels,
    opacity: f32,
    rem: Pixels,
    cx: &App,
) -> Option<PaintQuad> {
    if opacity <= 0.0 || scrollbar.total <= scrollbar.len || scrollbar.total == 0 {
        return None;
    }
    let inset = THUMB_INSET.to_pixels(rem);
    let width = THUMB_WIDTH.to_pixels(rem);
    let shortest = MIN_THUMB_SIZE.to_pixels(rem);
    let track_height = bounds.size.height - inset * 2.0;
    let ratio = scrollbar.len as f32 / scrollbar.total as f32;
    let thumb_height = (track_height * ratio).max(shortest);
    let travel = (track_height - thumb_height).max(px(0.0));
    let denominator = scrollbar.total.saturating_sub(scrollbar.len).max(1);
    let progress = scrollbar.offset as f32 / denominator as f32;
    let squeezed = (thumb_height - overscroll.abs()).max(shortest);
    let top = bounds.origin.y
        + inset
        + travel * progress
        + if overscroll < px(0.0) {
            thumb_height - squeezed
        } else {
            px(0.0)
        };
    let thumb_height = squeezed;
    let origin = point(bounds.right() - width - inset, top);
    let mut color = thumb_color(cx);
    color.a *= opacity;
    Some(
        fill(Bounds::new(origin, size(width, thumb_height)), color)
            .corner_radii(thumb_radius(width, cx)),
    )
}
