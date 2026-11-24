---
type: Research Report
title: Wayland background blur and rounded client-side corners
description: KWin 6.7.4 zoom and inset measurements explain the Wayland blur correction and the separate fix for translucent pane corners.
resource: libs/ala/crates/ala-platform/src/linux/wayland/window.rs
tags:
- gpui
- wayland
- kwin
- blur
- rounded-corners
- linux
timestamp: 2026-08-09T15:31:25Z
---

# The two visible defects

On KDE Plasma Wayland with KWin 6.7.4 (`8438567a`), application zoom exposed raw desktop at the outer
window corners. A separate blue arc remained around interior panes after the outer blur aligned. The
active HDMI-A-1 output used scale 1. The client frame used a p=4 curve, 12 logical px of shadow inset
and a 6 px shadow radius.

**Outer window.** The client shader can express fractional coverage of a curve. The compositor receives
a surface-local region made of integer, axis-aligned rectangles, attached to the existing surface on
commit. An expanded region blurred transparent tips. An exact region left the painted shadow rails over
raw desktop. Enlarging only the straight edges moved the break to their endpoints. An exact antialiased
match would require client-supplied continuous geometry or a compositor-owned clip.
[Effect protocol](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/dac6393216d2755c0302d77e739eb1bd96156852/staging/ext-background-effect/ext-background-effect-v1.xml#L83-126)
·
[KWin blur implementation at the tested revision](https://invent.kde.org/plasma/kwin/-/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/plugins/blur/blur.cpp#L453-476)

**Application zoom.** For platform width W, scale S, zoom Z and logical insets L/R, the old mask used
`(W - L - R) * S * Z`. The viewport requires `(W/Z - L - R) * S * Z`. This displaced the far edge by
`(Z - 1) * W` surface units. A discarded outer-circle experiment matched only the first scanline. At
zoom 1/1.5/2 its radius grew to about 60/92/125, with the first top blur near x=52/82/113.

**Compositor coordinates.** At 300% zoom KWin reported buffer origin (452,191) and frame origin
(464,203), a 12 px offset. Resubmitting window geometry on zoom moved the frame to (488,227), the
expected 36 px inset. The client compensated for KWin's additional content-origin translation. The
tested patch snapped the viewport mask, used the actual buffer-to-surface ratio per axis and clipped the
shadow during ext blur. The transparent margin remained available for resize input. Current code at
`1ce632dce` normalizes the mask in `libs/ala/crates/ala/src/window.rs`
(`window_corner_mask_for_viewport`), and `ala-platform/src/linux/wayland/window.rs` applies the KDE
offset in `kwin_uses_content_local_background_effect_coordinates`. Recheck that compensation against a
newer compositor with an inset surface and known diagnostic strips. [ala reference](/references/ala.md)

**Interior panes.** The 0.5 logical px expanded ring covered 1.5 device px at 300% zoom. Its
antialiasing and the translucent corner wedge left low-coverage pixels through which desktop color
showed. Removing the ring and extending the chrome under the layout border covered the gap. An opaque
backstop changed translucent content, and a paint-only border let terminal text and scrollbars enter the
chrome.

The August Wayland suite passed 29 tests. The geometry matrix covered zoom 0.8/1/1.5/2 at output scales
1/1.25/2. Live captures at 100/110/150/200/300% zoom showed clean outer corners and no top/left bands.
Pane captures at 100/300% showed the viewport inside its border without the blue arc. The measured
tradeoff was loss of the blurred window's outer drop shadow. Full patch history, geometry derivation and
sources are in `git show 3fd3fad4a:knowledge/research/2026-08-09-wayland-blur-rounded-corners.md`.
