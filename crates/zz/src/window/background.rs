use ala::{App, Global, Pixels, Window, WindowBackgroundAppearance};
use penna::foundation::interface_theme::Frost;

#[cfg(any(target_os = "macos", test))]
const BACKDROP_OPACITY: f32 = 0.65;
#[cfg(any(target_os = "macos", test))]
const BACKDROP_OPACITY_SPAN: f32 = 0.70;
#[cfg(any(target_os = "macos", test))]
const BACKDROP_CAP: f32 = 0.10;
#[cfg(any(target_os = "macos", test))]
const BACKDROP_CAP_SPAN: f32 = 0.30;
#[cfg(any(target_os = "macos", test))]
const BACKDROP_CAP_FLOOR: f32 = 0.04;

struct CompositorBlurSupport(bool);

impl Global for CompositorBlurSupport {}

pub(crate) fn compositor_supports_blur(cx: &App) -> bool {
    cx.try_global::<CompositorBlurSupport>()
        .is_none_or(|support| support.0)
}

pub fn detect_compositor_support(cx: &mut App) {
    let supported = probe_compositor();
    if !supported {
        log::info!(
            target: "zz::window_background",
            "compositor does not support background blur; glass will keep the window opaque"
        );
    }
    cx.set_global(CompositorBlurSupport(supported));
}

#[cfg(target_os = "macos")]
struct WallpaperHue(f32);

#[cfg(target_os = "macos")]
impl Global for WallpaperHue {}

pub(crate) fn wallpaper_hue(cx: &App) -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        cx.try_global::<WallpaperHue>().map(|hue| hue.0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cx;
        None
    }
}

#[cfg(target_os = "macos")]
pub fn sample_wallpaper(cx: &mut App) {
    let Some(path) = macos::desktop_image_path() else {
        return;
    };
    cx.spawn(async move |cx| {
        let hue = cx
            .background_executor()
            .spawn(async move { macos::thumbnail_rgba(&path).as_deref().and_then(mean_hue) })
            .await;
        if let Some(hue) = hue {
            cx.update(|cx| {
                cx.set_global(WallpaperHue(hue));
                crate::theme::refresh_current_theme(cx);
            });
        }
    })
    .detach();
}

#[cfg(any(target_os = "macos", test))]
const GRAY_CHROMA: f32 = 0.01;

#[cfg(any(target_os = "macos", test))]
fn mean_hue(rgba: &[u8]) -> Option<f32> {
    let (mut a, mut b, mut count) = (0.0_f32, 0.0_f32, 0.0_f32);
    let channel = |value: u8| f32::from(value) / 255.0;
    for &[red, green, blue, _] in rgba.as_chunks::<4>().0 {
        let color = ala::Rgba {
            r: channel(red),
            g: channel(green),
            b: channel(blue),
            a: 1.0,
        };
        let (_, chroma, hue) = penna::to_oklch(color.into());
        let (sin, cos) = hue.to_radians().sin_cos();
        a += chroma * cos;
        b += chroma * sin;
        count += 1.0;
    }
    if count == 0.0 {
        return None;
    }
    let (a, b) = (a / count, b / count);
    (a.hypot(b) >= GRAY_CHROMA).then(|| b.atan2(a).to_degrees().rem_euclid(360.0))
}

#[cfg(any(target_os = "macos", test))]
fn backdrop_brightness_cap(frost: Frost) -> f32 {
    (BACKDROP_CAP - (frost.get() - Frost::DEFAULT.get()) * BACKDROP_CAP_SPAN)
        .max(BACKDROP_CAP_FLOOR)
}

#[cfg(any(target_os = "macos", test))]
fn backdrop_chrome_opacity(frost: Frost) -> f32 {
    if frost.get() >= 1.0 {
        return 1.0;
    }
    (BACKDROP_OPACITY + (frost.get() - Frost::DEFAULT.get()) * BACKDROP_OPACITY_SPAN).min(1.0)
}

pub(crate) fn chrome_opacity(frost: Frost) -> f32 {
    #[cfg(target_os = "macos")]
    if macos::backdrop_available() {
        return backdrop_chrome_opacity(frost);
    }
    frost.window_opacity()
}

pub(crate) fn backdrop_ceiling(frost: Frost) -> f32 {
    #[cfg(target_os = "macos")]
    if macos::backdrop_available() {
        return backdrop_brightness_cap(frost);
    }
    let _ = frost;
    1.0
}

#[cfg(test)]
pub(crate) fn set_compositor_support_for_tests(supported: bool, cx: &mut App) {
    cx.set_global(CompositorBlurSupport(supported));
}

pub(crate) fn system_reduces_transparency() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::reduces_transparency()
    }
    #[cfg(target_os = "windows")]
    {
        windows_registry::CURRENT_USER
            .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .and_then(|key| key.get_u32("EnableTransparency"))
            .is_ok_and(|enabled| enabled == 0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

fn probe_compositor() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::probe_compositor()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

pub(crate) const fn native_appearance(
    requested: WindowBackgroundAppearance,
) -> WindowBackgroundAppearance {
    #[cfg(target_os = "macos")]
    if matches!(requested, WindowBackgroundAppearance::Blurred) {
        return WindowBackgroundAppearance::Transparent;
    }

    requested
}

pub(crate) fn apply(
    window: &Window,
    requested: WindowBackgroundAppearance,
    corner_radius: Pixels,
    frost: Frost,
) {
    let enabled = requested == WindowBackgroundAppearance::Blurred;

    #[cfg(target_os = "macos")]
    {
        let backdrop = enabled && macos::backdrop_available();
        if let Err(error) =
            macos::set_backdrop(window, backdrop.then(|| backdrop_brightness_cap(frost)))
        {
            log::warn!(target: "zz::window_background", "could not update macOS window backdrop: {error}");
        }
        if let Err(error) =
            macos::set_blur(window, (enabled && !backdrop).then(|| frost.blur_scale()))
        {
            log::warn!(target: "zz::window_background", "could not update macOS window blur: {error}");
        }
    }

    #[cfg(target_os = "linux")]
    if let Err(error) = linux::set_x11_blur(window, enabled, corner_radius) {
        log::warn!(target: "zz::window_background", "could not update X11 window blur: {error}");
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (window, enabled, corner_radius, frost);

    #[cfg(target_os = "macos")]
    let _ = corner_radius;

    #[cfg(target_os = "linux")]
    let _ = frost;
}

#[cfg(any(target_os = "linux", test))]
fn rounded_region(
    width: u32,
    height: u32,
    radius: u32,
    corners: crate::window::corners::WindowCorners,
) -> Vec<u32> {
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 || corners == crate::window::corners::WindowCorners::NONE {
        return vec![0, 0, width, height];
    }

    let top_radius = if corners.top_left() || corners.top_right() {
        radius
    } else {
        0
    };
    let bottom_radius = if corners.bottom_left_is_rounded() || corners.bottom_right_is_rounded() {
        radius
    } else {
        0
    };
    let mut region = Vec::with_capacity(((top_radius + bottom_radius + 1) * 4) as usize);

    for row in 0..top_radius {
        push_corner_row(
            &mut region,
            width,
            radius,
            row,
            row,
            corners.top_left(),
            corners.top_right(),
        );
    }

    let middle_height = height - top_radius - bottom_radius;
    if middle_height > 0 {
        region.extend_from_slice(&[0, top_radius, width, middle_height]);
    }

    for row in 0..bottom_radius {
        push_corner_row(
            &mut region,
            width,
            radius,
            bottom_radius - row - 1,
            height - bottom_radius + row,
            corners.bottom_left_is_rounded(),
            corners.bottom_right_is_rounded(),
        );
    }

    region
}

#[cfg(any(target_os = "linux", test))]
fn push_corner_row(
    region: &mut Vec<u32>,
    width: u32,
    radius: u32,
    row: u32,
    y: u32,
    round_left: bool,
    round_right: bool,
) {
    let offset = corner_offset(radius, row);
    let left = if round_left { offset } else { 0 };
    let right = if round_right { offset } else { 0 };
    region.extend_from_slice(&[left, y, width.saturating_sub(left + right), 1]);
}

#[cfg(any(target_os = "linux", test))]
fn corner_offset(radius: u32, row: u32) -> u32 {
    let radius_f = radius as f32;
    let dy = (radius - row) as f32 - 0.5;
    let dx = (radius_f.mul_add(radius_f, -(dy * dy))).max(0.0).sqrt();
    radius.saturating_sub((dx + 0.5).round() as u32)
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "the raw AppKit handle, the WindowServer blur call, the backdrop layer and ImageIO require unsafe access"
)]
mod macos {
    use std::{
        fmt,
        path::{Path, PathBuf},
        ptr::NonNull,
    };

    use ala::Window;
    use objc2::{
        MainThreadMarker, MainThreadOnly as _, msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSScreen, NSUserInterfaceItemIdentification as _, NSView,
        NSWindowOrderingMode, NSWorkspace,
    };
    use objc2_core_foundation::{
        CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL, CGPoint, CGRect,
        CGSize,
    };
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
        CGImageByteOrderInfo, kCGColorSpaceSRGB,
    };
    use objc2_foundation::{NSArray, NSNumber, NSString};
    use objc2_quartz_core::{CAAutoresizingMask, CALayer};
    use raw_window_handle::RawWindowHandle;

    const BLUR_RADIUS: i32 = 160;
    const THUMBNAIL_PIXELS: i32 = 64;
    const BACKDROP_VIEW: &str = "zz-window-backdrop";
    const BACKDROP_GROUP: &str = "NSCGSWindowBehindWindowCaptureBackdropGroup";
    const BACKDROP_SCALE: f64 = 0.125;
    const BACKDROP_BLUR: f64 = 60.0;
    const BACKDROP_SATURATION: f64 = 1.8;
    const BRIGHTNESS_CAP: &str = "compressLuminance";

    type ConnectionId = u32;
    type WindowId = u32;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSMainConnectionID() -> ConnectionId;
        fn CGSSetWindowBackgroundBlurRadius(
            connection: ConnectionId,
            window: WindowId,
            radius: i32,
        ) -> i32;
    }

    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        static kCGImageSourceCreateThumbnailFromImageAlways: &'static CFString;
        static kCGImageSourceThumbnailMaxPixelSize: &'static CFString;
        fn CGImageSourceCreateWithURL(
            url: &CFURL,
            options: Option<&CFDictionary>,
        ) -> Option<NonNull<CFType>>;
        fn CGImageSourceCreateThumbnailAtIndex(
            source: &CFType,
            index: usize,
            options: Option<&CFDictionary>,
        ) -> Option<NonNull<CGImage>>;
    }

    #[derive(Debug)]
    pub(super) enum BlurError {
        WindowHandle(raw_window_handle::HandleError),
        UnexpectedWindowHandle,
        NotMainThread,
        MissingView,
        MissingWindow,
        WindowServer(i32),
    }

    impl fmt::Display for BlurError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::WindowHandle(error) => {
                    write!(formatter, "could not read native window handle: {error}")
                }
                Self::UnexpectedWindowHandle => {
                    formatter.write_str("ala returned a non-AppKit window handle")
                }
                Self::NotMainThread => {
                    formatter.write_str("WindowServer blur update ran outside the main thread")
                }
                Self::MissingView => formatter.write_str("AppKit window view is unavailable"),
                Self::MissingWindow => {
                    formatter.write_str("AppKit view is not attached to an on-screen window")
                }
                Self::WindowServer(status) => {
                    write!(
                        formatter,
                        "WindowServer rejected the blur radius (status {status})"
                    )
                }
            }
        }
    }

    pub(super) fn reduces_transparency() -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
    }

    pub(super) fn desktop_image_path() -> Option<PathBuf> {
        let screen = NSScreen::mainScreen(MainThreadMarker::new()?)?;
        let url = NSWorkspace::sharedWorkspace().desktopImageURLForScreen(&screen)?;
        Some(PathBuf::from(url.path()?.to_string()))
    }

    pub(super) fn thumbnail_rgba(path: &Path) -> Option<Vec<u8>> {
        let url = CFURL::from_file_path(path)?;
        let source = unsafe { CGImageSourceCreateWithURL(&url, None) }
            .map(|source| unsafe { CFRetained::from_raw(source) })?;
        let keys = unsafe {
            [
                kCGImageSourceCreateThumbnailFromImageAlways,
                kCGImageSourceThumbnailMaxPixelSize,
            ]
        };
        let max_size = CFNumber::new_i32(THUMBNAIL_PIXELS);
        let values: [&CFType; 2] = [CFBoolean::new(true).as_ref(), max_size.as_ref()];
        let options = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
        let image =
            unsafe { CGImageSourceCreateThumbnailAtIndex(&source, 0, Some(options.as_opaque())) }
                .map(|image| unsafe { CFRetained::from_raw(image) })?;
        let width = CGImage::width(Some(&image));
        let height = CGImage::height(Some(&image));
        let side = |pixels: usize| u32::try_from(pixels).ok().map(f64::from);
        let bounds = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(side(width)?, side(height)?),
        );
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
        let mut pixels = vec![0_u8; width * height * 4];
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width,
                height,
                8,
                width * 4,
                Some(&space),
                CGImageAlphaInfo::NoneSkipLast.0 | CGImageByteOrderInfo::Order32Big.0,
            )
        }?;
        CGContext::draw_image(Some(&context), bounds, Some(&image));
        drop(context);
        Some(pixels)
    }

    pub(super) fn backdrop_available() -> bool {
        AnyClass::get(c"CABackdropLayer").is_some() && AnyClass::get(c"CAFilter").is_some()
    }

    pub(super) fn set_backdrop(window: &Window, cap: Option<f32>) -> Result<(), BlurError> {
        let main_thread = MainThreadMarker::new().ok_or(BlurError::NotMainThread)?;
        let view = ala_view(window)?;
        let superview = unsafe { view.superview() }.ok_or(BlurError::MissingView)?;
        let identifier = NSString::from_str(BACKDROP_VIEW);
        let existing = superview
            .subviews()
            .iter()
            .find(|subview| subview.identifier().is_some_and(|id| *id == *identifier));
        match (existing, cap) {
            (Some(host), None) => host.removeFromSuperview(),
            (Some(host), Some(cap)) => {
                let backdrop = unsafe { host.layer().and_then(|layer| layer.sublayers()) }
                    .and_then(|layers| layers.firstObject())
                    .ok_or(BlurError::MissingView)?;
                let path = NSString::from_str(&format!("filters.{BRIGHTNESS_CAP}.inputAmount"));
                let amount = NSNumber::new_f64(f64::from(cap));
                unsafe {
                    let _: () = msg_send![&*backdrop, setValue: &*amount, forKeyPath: &*path];
                }
            }
            (None, Some(cap)) => {
                let host =
                    backdrop_view(main_thread, &superview, cap).ok_or(BlurError::MissingView)?;
                host.setIdentifier(Some(&identifier));
                superview.addSubview_positioned_relativeTo(
                    &host,
                    NSWindowOrderingMode::Below,
                    Some(&view),
                );
            }
            (None, None) => {}
        }
        Ok(())
    }

    fn backdrop_view(
        main_thread: MainThreadMarker,
        superview: &NSView,
        cap: f32,
    ) -> Option<Retained<NSView>> {
        let host = NSView::initWithFrame(NSView::alloc(main_thread), superview.bounds());
        host.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        host.setWantsLayer(true);
        let backing = host.layer()?;
        let backdrop: Retained<CALayer> =
            unsafe { msg_send![AnyClass::get(c"CABackdropLayer")?, layer] };
        set_value(&backdrop, "windowServerAware", &NSNumber::new_bool(true));
        set_value(&backdrop, "scale", &NSNumber::new_f64(BACKDROP_SCALE));
        set_value(&backdrop, "groupName", &NSString::from_str(BACKDROP_GROUP));
        backdrop.setFrame(backing.bounds());
        backdrop.setAutoresizingMask(
            CAAutoresizingMask::LayerWidthSizable | CAAutoresizingMask::LayerHeightSizable,
        );
        let filters = [
            filter("sdrNormalize", &[])?,
            filter(
                "gaussianBlur",
                &[
                    ("inputRadius", &NSNumber::new_f64(BACKDROP_BLUR)),
                    ("inputNormalizeEdges", &NSNumber::new_bool(true)),
                ],
            )?,
            filter(
                "colorSaturate",
                &[("inputAmount", &NSNumber::new_f64(BACKDROP_SATURATION))],
            )?,
            filter(
                BRIGHTNESS_CAP,
                &[("inputAmount", &NSNumber::new_f64(f64::from(cap)))],
            )?,
        ];
        unsafe { backdrop.setFilters(Some(&NSArray::from_retained_slice(&filters))) };
        backing.addSublayer(&backdrop);
        Some(host)
    }

    fn filter(kind: &str, inputs: &[(&str, &AnyObject)]) -> Option<Retained<AnyObject>> {
        let name = NSString::from_str(kind);
        let filter: Option<Retained<AnyObject>> =
            unsafe { msg_send![AnyClass::get(c"CAFilter")?, filterWithType: &*name] };
        let filter = filter?;
        unsafe {
            let _: () = msg_send![&*filter, setDefaults];
        }
        set_value(&filter, "name", &name);
        for (key, value) in inputs {
            set_value(&filter, key, value);
        }
        Some(filter)
    }

    fn set_value(object: &AnyObject, key: &str, value: &AnyObject) {
        let key = NSString::from_str(key);
        unsafe {
            let _: () = msg_send![object, setValue: value, forKey: &*key];
        }
    }

    fn ala_view(window: &Window) -> Result<Retained<NSView>, BlurError> {
        let handle = raw_window_handle::HasWindowHandle::window_handle(window)
            .map_err(BlurError::WindowHandle)?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(BlurError::UnexpectedWindowHandle);
        };
        unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }
            .ok_or(BlurError::MissingView)
    }

    pub(super) fn set_blur(window: &Window, scale: Option<f32>) -> Result<(), BlurError> {
        MainThreadMarker::new().ok_or(BlurError::NotMainThread)?;
        let view = ala_view(window)?;
        let native_window = view.window().ok_or(BlurError::MissingWindow)?;
        let window_id = WindowId::try_from(native_window.windowNumber())
            .ok()
            .filter(|id| *id != 0)
            .ok_or(BlurError::MissingWindow)?;
        let radius = scale.map_or(0, |scale| (BLUR_RADIUS as f32 * scale).round() as i32);
        let status =
            unsafe { CGSSetWindowBackgroundBlurRadius(CGSMainConnectionID(), window_id, radius) };
        if status == 0 {
            Ok(())
        } else {
            Err(BlurError::WindowServer(status))
        }
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
mod linux {
    use std::{fmt, sync::OnceLock};

    use ala::Window;
    use raw_window_handle::RawWindowHandle;
    use x11rb::{
        connection::Connection as _,
        protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode},
        rust_connection::RustConnection,
        wrapper::ConnectionExt as _,
    };

    use crate::{window::background::rounded_region, window::corners::WindowCorners};

    struct BlurConnection {
        connection: RustConnection,
        atom: u32,
        root: u32,
    }

    static BLUR_CONNECTION: OnceLock<Result<BlurConnection, String>> = OnceLock::new();

    #[derive(Debug)]
    pub(super) enum BlurError {
        WindowHandle(raw_window_handle::HandleError),
        Connection(String),
        X11(Box<dyn std::error::Error + Send + Sync>),
    }

    impl fmt::Display for BlurError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::WindowHandle(error) => {
                    write!(formatter, "could not read native window handle: {error}")
                }
                Self::Connection(error) => formatter.write_str(error),
                Self::X11(error) => write!(formatter, "X11 request failed: {error}"),
            }
        }
    }

    pub(super) fn set_x11_blur(
        window: &Window,
        enabled: bool,
        corner_radius: ala::Pixels,
    ) -> Result<(), BlurError> {
        let handle = raw_window_handle::HasWindowHandle::window_handle(window)
            .map_err(BlurError::WindowHandle)?;
        let RawWindowHandle::Xcb(handle) = handle.as_raw() else {
            return Ok(());
        };
        let window_id = handle.window.get();
        let blur = blur_connection()?;
        if enabled {
            let geometry = blur
                .connection
                .get_geometry(window_id)
                .map_err(x11_error)?
                .reply()
                .map_err(x11_error)?;
            let radius = (f32::from(corner_radius) * window.scale_factor())
                .round()
                .max(0.0) as u32;
            let region = rounded_region(
                u32::from(geometry.width),
                u32::from(geometry.height),
                radius,
                WindowCorners::for_window(window),
            );
            blur.connection
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    blur.atom,
                    AtomEnum::CARDINAL,
                    &region,
                )
                .map_err(x11_error)?;
        } else {
            blur.connection
                .delete_property(window_id, blur.atom)
                .map_err(x11_error)?;
        }
        blur.connection.flush().map_err(x11_error)?;
        Ok(())
    }

    pub(super) fn probe_compositor() -> bool {
        let wayland = ["WAYLAND_DISPLAY", "WAYLAND_SOCKET"]
            .iter()
            .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()));
        if wayland {
            wayland_advertises_blur()
        } else {
            x11_root_announces_blur()
        }
    }

    fn wayland_advertises_blur() -> bool {
        use wayland_client::{
            Connection as WaylandConnection, Dispatch, QueueHandle,
            protocol::wl_registry::{self, WlRegistry},
        };

        struct RegistryScan {
            blur: bool,
        }

        impl Dispatch<WlRegistry, ()> for RegistryScan {
            fn event(
                scan: &mut Self,
                _: &WlRegistry,
                event: wl_registry::Event,
                (): &(),
                _: &WaylandConnection,
                _: &QueueHandle<Self>,
            ) {
                if let wl_registry::Event::Global { interface, .. } = event
                    && matches!(
                        interface.as_str(),
                        "org_kde_kwin_blur_manager" | "ext_background_effect_manager_v1"
                    )
                {
                    scan.blur = true;
                }
            }
        }

        let Ok(connection) = WaylandConnection::connect_to_env() else {
            return false;
        };
        let mut queue = connection.new_event_queue();
        let _registry = connection.display().get_registry(&queue.handle(), ());
        let mut scan = RegistryScan { blur: false };
        queue.roundtrip(&mut scan).is_ok() && scan.blur
    }

    fn x11_root_announces_blur() -> bool {
        fn root_lists_blur_atom() -> Result<bool, BlurError> {
            let blur = blur_connection()?;
            let properties = blur
                .connection
                .list_properties(blur.root)
                .map_err(x11_error)?
                .reply()
                .map_err(x11_error)?;
            Ok(properties.atoms.contains(&blur.atom))
        }
        root_lists_blur_atom().unwrap_or(false)
    }

    fn blur_connection() -> Result<&'static BlurConnection, BlurError> {
        BLUR_CONNECTION
            .get_or_init(|| {
                let (connection, screen) =
                    x11rb::connect(None).map_err(|error| error.to_string())?;
                let root = connection.setup().roots[screen].root;
                let atom = connection
                    .intern_atom(false, b"_KDE_NET_WM_BLUR_BEHIND_REGION")
                    .map_err(|error| error.to_string())?
                    .reply()
                    .map_err(|error| error.to_string())?
                    .atom;
                Ok(BlurConnection {
                    connection,
                    atom,
                    root,
                })
            })
            .as_ref()
            .map_err(|error| BlurError::Connection(error.clone()))
    }

    fn x11_error(error: impl std::error::Error + Send + Sync + 'static) -> BlurError {
        BlurError::X11(Box::new(error))
    }
}

#[cfg(test)]
mod tests {
    use ala::Tiling;

    use super::*;
    use crate::window::corners::WindowCorners;

    #[test]
    fn square_blur_region_is_one_full_window_rectangle() {
        assert_eq!(
            rounded_region(100, 60, 14, WindowCorners::NONE),
            [0, 0, 100, 60]
        );
    }

    #[test]
    fn rounded_blur_region_carves_out_only_exposed_corners() {
        let corners = WindowCorners::from_tiling(Tiling::default());
        let region = rounded_region(100, 60, 4, corners);

        assert_eq!(region.len(), (4 + 1 + 4) * 4);
        assert_eq!(&region[4 * 4..5 * 4], &[0, 4, 100, 52]);
        assert!(region[0] > 0);
        assert!(region[2] < 100);
        let last = &region[region.len() - 4..];
        assert_eq!(last[1], 59);
        assert_eq!(last[0], region[0]);
        assert_eq!(last[2], region[2]);
    }

    #[test]
    fn tiled_top_edge_keeps_top_rows_square() {
        let corners = WindowCorners::from_tiling(Tiling {
            top: true,
            ..Tiling::default()
        });
        let region = rounded_region(100, 60, 4, corners);

        assert_eq!(&region[..4], &[0, 0, 100, 56]);
        assert_eq!(region.len(), (1 + 4) * 4);
    }

    #[test]
    fn frost_sets_the_backdrop_brightness_cap_and_the_chrome_opacity() {
        for (frost, cap, opacity) in [(0.0, 0.25, 0.30), (0.5, 0.10, 0.65), (1.0, 0.04, 1.0)] {
            let frost = Frost::new(frost);
            assert!(
                (backdrop_brightness_cap(frost) - cap).abs() < 1e-6,
                "cap at {frost:?}"
            );
            assert!(
                (backdrop_chrome_opacity(frost) - opacity).abs() < 1e-6,
                "opacity at {frost:?}"
            );
        }
        assert_eq!(backdrop_chrome_opacity(Frost::new(1.0)), 1.0);
    }

    #[test]
    fn wallpaper_hue_is_the_oklab_hue_of_the_mean_color_and_none_for_gray() {
        let red = [255, 0, 0, 255].repeat(16);
        let hue = mean_hue(&red).expect("red has a hue");
        assert!((hue - 29.234).abs() < 0.01, "red hue {hue}");

        let gray = [128, 128, 128, 255].repeat(16);
        assert_eq!(mean_hue(&gray), None);

        let mut mostly_gray = [128, 128, 128, 255].repeat(31);
        mostly_gray.extend([255, 0, 0, 255]);
        assert_eq!(mean_hue(&mostly_gray), None);
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn x11_backend_type_checks_on_other_test_platforms() {
        let _ = linux::set_x11_blur;
        let _ = linux::probe_compositor;
    }
}
