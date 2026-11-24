---
type: Research Report
title: NVIDIA Linux CEF accelerated OSR failure
description: CEF 151 produced no frames on a tested NVIDIA Wayland host. Source and controls point to the buffer allocation, with a matched libcef build still needed for confirmation.
resource: crates/zz/src/browser/controller.rs
tags:
- browser
- cef
- nvidia
- linux
- osr
- dmabuf
- gbm
timestamp: 2026-08-07T10:24:50Z
---

# A loaded page with no paint callbacks

On 2026-08-06 both the fixture and full application loaded the page, updated the title and fetched the
favicon. The GPU logged `Unable to initialize SkSurface`. Accelerated paints, readback paints and zz
import attempts all stayed at zero. `ZZ_BROWSER_SHARED_TEXTURE=0` restored animated output. With no
callback or handle, a failed wgpu import could not account for the blank pane.

| Component | Observed value |
| --- | --- |
| Distribution and session | CachyOS, KDE Wayland |
| Kernel | `7.1.6-1-cachyos-deckify` |
| GPU | NVIDIA GeForce RTX 3070 Laptop GPU, PCI `10de:24dd` |
| NVIDIA kernel and userspace | `610.57.04` |
| CEF | `151.3.14`, commit `5d67476` |
| Chromium embedded by CEF | `151.0.7922.72` |
| Mesa / libgbm | `26.1.6` |
| EGL GBM platform | `egl-gbm 1.1.3` |
| EGL Wayland platform | `egl-wayland 1.1.21` |
| libdrm | `2.4.134` |

The live GPU child ran native Wayland, mapped NVIDIA EGL/GBM libraries and opened `/dev/dri/renderD128`.
Removing MangoHud or forcing the NVIDIA vendor/backend still produced zero callbacks. KMS enumeration
worked. EGL exposed DMA-BUF import/modifiers, export and fence sync, the compositor advertised
linux-dmabuf v5 and linear/block-linear formats, and Vulkan advertised DMA-BUF external memory and DRM
modifiers. These probes establish the host's transport capabilities.

## Where the source points

At the tested CEF revision, `CefVideoConsumerOSR::SetActive` requested `kPreferMappableSharedImage`.
Chromium 151 mapped CPU access to `SCANOUT_CPU_READ_WRITE`, then to linear GBM storage with
scanout/texturing flags but without `GBM_BO_USE_RENDERING`. Skia failed creating its render target
before capture produced a handle. The source and runtime observations were consistent with this
allocation failing on the tested NVIDIA configuration. Mesa AMD could satisfy the request. No matched
libcef A/B isolated the allocation as the cause.
[CEF at the tested commit](https://github.com/chromiumembedded/cef/blob/5d67476b12f718c8388918d1740aeec27f6b2b80/libcef/browser/osr/video_consumer_osr.cc)
·
[Chromium 151 GBM mapping](https://chromium.googlesource.com/chromium/src/+/refs/tags/151.0.7922.76/ui/gfx/linux/gbm_util.cc)

Chromium's `kPreferSharedImageWithNativeHandle` permits GPU-oriented scanout storage. CEF's public
shared-texture switch did not expose that preference. ANGLE/Ozone setup affects availability of a GL
representation, while this capture reached Skia write access. Forcing the Vulkan feature on native
Wayland produced accelerated callbacks but failed full-app resize stress and logged an unsupported
combination. The later [Linux pacing study](/research/2026-10-28-linux-browser-pacing.md) used that
temporary switch to measure imports on driver 615.78.08.
[Chromium native-handle change](https://chromium.googlesource.com/chromium/src/+/a531c83a9bbb552fa13ceeaf73d0a19d1203cc12)

## Recovery and proof still needed

At `1ce632dce`, `crates/zz/src/browser/controller.rs` arms `SHARED_TEXTURE_FIRST_FRAME_TIMEOUT` for
eligible visible Linux sessions. After two seconds without a frame, the controller can recreate the pane
using readback, leaving Chromium GPU rendering enabled. Readback transfers the composed BGRA frame via
CPU memory for upload. [CEF runtime](/browser/cef-runtime.md) ·
[OSR rendering](/browser/osr-rendering.md)

To establish the proposed fix, compare matched libcef builds differing only in the Linux shared-image
preference. Verify accelerated callbacks, successful imports, changing pixels and no first-frame
fallback. Check resize, video/WebGL, popups, multiple panes, hide/show and shutdown on NVIDIA and Mesa,
with X11 and native Wayland recorded separately. The August evidence does not cover newer drivers or CEF
revisions. [Accelerated compositing design](/designs/accelerated-osr-compositing.md) ·
[Updating CEF](/playbooks/updating-cef.md) · [CEF artifact lock](/references/cef-artifacts.md) ·
[Running zz](/playbooks/running-zz.md)

The [original investigation](https://github.com/autistic-computer/zebrilus/blob/5a81862d4/knowledge/research/2026-08-07-nvidia-cef-accelerated-osr.md)
records the package checks, CEF/Chromium source chain, control runs and citations.
