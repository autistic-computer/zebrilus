use async_channel::Sender;
use penna::tray::{Menu, Tray, TrayEvent, TrayIcon, TrayOptions};
use zz_protocol::app_identity::{DIRECTORY, DISPLAY_NAME};

pub(super) fn spawn(menu: Menu, events: Sender<TrayEvent>) -> Option<Tray> {
    let options = TrayOptions {
        id: DIRECTORY.into(),
        tooltip: tooltip(0),
        icon: icon(0)?,
        icon_name: (cfg!(target_os = "linux") && !zz_protocol::app_identity::DEVELOPMENT)
            .then(|| "zz".to_owned()),
        menu_on_click: false,
    };
    Tray::spawn(options, menu, events)
}

pub(super) fn tooltip(attention: usize) -> String {
    if attention == 0 {
        DISPLAY_NAME.into()
    } else {
        format!("{DISPLAY_NAME} · {attention} waiting")
    }
}

pub(super) fn badge(attention: usize) -> String {
    if attention == 0 {
        String::new()
    } else {
        attention.to_string()
    }
}

#[cfg(target_os = "macos")]
const TRAY_GLYPH_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/macos/tray-glyph-36.png"
));

#[cfg(target_os = "macos")]
fn tray_glyph(rgb: [u8; 3]) -> image::RgbaImage {
    let mut glyph = image::load_from_memory_with_format(TRAY_GLYPH_PNG, image::ImageFormat::Png)
        .expect("the embedded zz glyph must be a valid PNG")
        .into_rgba8();
    for pixel in glyph.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&rgb);
    }
    glyph
}

#[cfg(target_os = "macos")]
pub(super) fn icon(_attention: usize) -> Option<TrayIcon> {
    let development = zz_protocol::app_identity::DEVELOPMENT;
    let glyph = tray_glyph(if development {
        [242, 140, 40]
    } else {
        [0, 0, 0]
    });
    Some(TrayIcon {
        png: png(&glyph)?,
        template: !development,
    })
}

#[cfg(target_os = "linux")]
const TRAY_ICON_PNG: &[u8] = if zz_protocol::app_identity::DEVELOPMENT {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/linux/hicolor/256x256/apps/zz-dev.png"
    ))
} else {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/linux/hicolor/256x256/apps/zz.png"
    ))
};

#[cfg(target_os = "linux")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the macOS and Windows icons are encoded and can fail"
)]
pub(super) fn icon(_attention: usize) -> Option<TrayIcon> {
    Some(TrayIcon {
        png: TRAY_ICON_PNG.to_vec(),
        template: false,
    })
}

#[cfg(target_os = "windows")]
const TRAY_ICON_ICO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/windows/zz.ico"
));

#[cfg(target_os = "windows")]
pub(super) fn icon(attention: usize) -> Option<TrayIcon> {
    let decoded = image::load_from_memory_with_format(TRAY_ICON_ICO, image::ImageFormat::Ico)
        .inspect_err(
            |error| log::warn!(target: "zz::tray", "could not decode the tray icon: {error}"),
        )
        .ok()?;
    let mut rgba = decoded.into_rgba8();
    if attention > 0 {
        let (width, height) = rgba.dimensions();
        let radius = i64::from((width.min(height) / 6).max(1));
        let center_x = i64::from(width) - radius - 1;
        let center_y = radius;
        for (x, y, pixel) in rgba.enumerate_pixels_mut() {
            if (i64::from(x) - center_x).pow(2) + (i64::from(y) - center_y).pow(2) <= radius.pow(2)
            {
                *pixel = image::Rgba([255, 90, 60, 255]);
            }
        }
    }
    Some(TrayIcon {
        png: png(&rgba)?,
        template: false,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(super) fn icon(_attention: usize) -> Option<TrayIcon> {
    None
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn png(image: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .inspect_err(
            |error| log::warn!(target: "zz::tray", "could not encode the tray icon: {error}"),
        )
        .ok()?;
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_says_how_many_panes_wait() {
        assert_eq!(tooltip(0), DISPLAY_NAME);
        assert_eq!(tooltip(2), format!("{DISPLAY_NAME} · 2 waiting"));
        assert_eq!(badge(0), "");
        assert_eq!(badge(3), "3");
        assert!(icon(0).is_some_and(|icon| !icon.png.is_empty()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_prescaled_glyph_matches_the_icon_source() {
        use image::{ImageFormat, imageops::FilterType};
        let source = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/zz.icon/Assets/layer-z-1024.png"
        ));
        for rgb in [[0, 0, 0], [242, 140, 40]] {
            let mut glyph = image::load_from_memory_with_format(source, ImageFormat::Png)
                .unwrap()
                .into_rgba8();
            for pixel in glyph.as_chunks_mut::<4>().0 {
                pixel[..3].copy_from_slice(&rgb);
            }
            let scaled = image::imageops::resize(&glyph, 36, 36, FilterType::Lanczos3);
            assert_eq!(tray_glyph(rgb).as_raw(), scaled.as_raw());
        }
    }
}
