use std::{borrow::Cow, io::Cursor, path::Path, sync::Arc};

use ala::{
    App, Image, ImageFormat, ImageSource, ObjectFit, RenderImage, Window, div, img, prelude::*, px,
};

use crate::{
    WindowExt as _,
    target::{TargetHost, path_reference},
};

const ATTACHMENT_PREVIEW_WIDTH: f32 = 880.0;
const ATTACHMENT_PREVIEW_HEIGHT: f32 = 560.0;

pub fn open_attachment_preview(image: Arc<Image>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, _| {
        let viewport = window.viewport_size();
        let image = Arc::clone(&image);
        dialog.width(px(ATTACHMENT_PREVIEW_WIDTH)).child(
            div()
                .w_full()
                .h(px(ATTACHMENT_PREVIEW_HEIGHT).min(viewport.height * 0.7))
                .child(
                    img(ImageSource::Image(image))
                        .size_full()
                        .object_fit(ObjectFit::ScaleDown),
                ),
        )
    });
}

pub fn open_render_image_preview(image: Arc<RenderImage>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, _| {
        let viewport = window.viewport_size();
        let image = Arc::clone(&image);
        dialog.width(px(ATTACHMENT_PREVIEW_WIDTH)).child(
            div()
                .w_full()
                .h(px(ATTACHMENT_PREVIEW_HEIGHT).min(viewport.height * 0.7))
                .child(
                    img(ImageSource::Render(image))
                        .size_full()
                        .object_fit(ObjectFit::ScaleDown),
                ),
        )
    });
}

const NATIVE_FORMATS: [ImageFormat; 4] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Gif,
    ImageFormat::Webp,
];

const MAX_EDGE: u32 = 1568;

const MAX_BYTES: usize = 5 * 1024 * 1024;
const JPEG_QUALITY: u8 = 85;

pub fn normalize(image: &Image) -> Result<Arc<Image>, Cow<'static, str>> {
    if image.format == ImageFormat::Svg {
        return Err(Cow::Borrowed("SVG images cannot be sent to an agent"));
    }
    let format = decoder_format(image.format)
        .ok_or_else(|| Cow::from(format!("cannot read {} images", image.format.mime_type())))?;
    let (width, height) = image::ImageReader::with_format(Cursor::new(&image.bytes), format)
        .into_dimensions()
        .map_err(|error| Cow::from(format!("could not read the pasted image: {error}")))?;

    let oversized = width.max(height) > MAX_EDGE;
    if !oversized && NATIVE_FORMATS.contains(&image.format) && image.bytes.len() <= MAX_BYTES {
        return Ok(Arc::new(image.clone()));
    }

    let decoded = image::load_from_memory_with_format(&image.bytes, format)
        .map_err(|error| Cow::from(format!("could not decode the pasted image: {error}")))?;
    let decoded = if oversized {
        decoded.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::CatmullRom)
    } else {
        decoded
    };
    let mut encoded = Cursor::new(Vec::new());
    let written = if image.format == ImageFormat::Jpeg {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, JPEG_QUALITY)
            .encode_image(&decoded.to_rgb8())
    } else {
        decoded.write_to(&mut encoded, image::ImageFormat::Png)
    };
    written.map_err(|error| Cow::from(format!("could not re-encode the pasted image: {error}")))?;
    let encoded = encoded.into_inner();
    if encoded.len() > MAX_BYTES {
        return Err(Cow::from(format!(
            "image is too large to send ({} MB after scaling)",
            encoded.len() / (1024 * 1024)
        )));
    }
    let format = if image.format == ImageFormat::Jpeg {
        ImageFormat::Jpeg
    } else {
        ImageFormat::Png
    };
    Ok(Arc::new(Image::from_bytes(format, encoded)))
}

const fn decoder_format(format: ImageFormat) -> Option<image::ImageFormat> {
    Some(match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Webp => image::ImageFormat::WebP,
        ImageFormat::Gif => image::ImageFormat::Gif,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Tiff => image::ImageFormat::Tiff,
        ImageFormat::Ico => image::ImageFormat::Ico,
        ImageFormat::Pnm => image::ImageFormat::Pnm,
        ImageFormat::Svg => return None,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Recipient<'a> {
    pub host: &'a TargetHost,
    pub local: bool,
    pub takes_images: bool,
    pub home: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrival {
    Attach(ImageFormat),
    Mention(String),
    Refuse(String),
}

#[must_use]
pub fn arrival(path: &Path, folder: bool, chat: Recipient) -> Arrival {
    let image = image_format(path).filter(|_| !folder && chat.takes_images);
    if let Some(format) = image {
        return Arrival::Attach(format);
    }
    if chat.local {
        return Arrival::Mention(path_reference(chat.host, path, chat.home, folder));
    }
    let name = path
        .file_name()
        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
    Arrival::Refuse(format!(
        "{name} is on this machine; this chat runs on {}",
        chat.host.name
    ))
}

fn image_format(path: &Path) -> Option<ImageFormat> {
    Some(match image::ImageFormat::from_path(path).ok()? {
        image::ImageFormat::Png => ImageFormat::Png,
        image::ImageFormat::Jpeg => ImageFormat::Jpeg,
        image::ImageFormat::WebP => ImageFormat::Webp,
        image::ImageFormat::Gif => ImageFormat::Gif,
        image::ImageFormat::Bmp => ImageFormat::Bmp,
        image::ImageFormat::Tiff => ImageFormat::Tiff,
        image::ImageFormat::Ico => ImageFormat::Ico,
        image::ImageFormat::Pnm => ImageFormat::Pnm,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
        let buffer = image::RgbaImage::from_fn(width, height, |x, y| {
            let channel = |value: u32| u8::try_from(value % 256).unwrap_or(0xff);
            image::Rgba([channel(x), channel(y), 0x40, 0xff])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(&mut bytes, format)
            .expect("fixture should encode");
        bytes.into_inner()
    }

    #[test]
    fn a_small_png_is_sent_untouched() {
        let bytes = encode(32, 24, image::ImageFormat::Png);
        let pasted = Image::from_bytes(ImageFormat::Png, bytes.clone());
        let normalized = normalize(&pasted).expect("a small PNG should pass through");

        assert_eq!(normalized.format, ImageFormat::Png);
        assert_eq!(normalized.bytes, bytes);
    }

    #[test]
    fn a_tiff_becomes_a_png_agents_can_read() {
        let pasted = Image::from_bytes(ImageFormat::Tiff, encode(20, 10, image::ImageFormat::Tiff));
        let normalized = normalize(&pasted).expect("TIFF should transcode");

        assert_eq!(normalized.format, ImageFormat::Png);
        assert_eq!(normalized.format.mime_type(), "image/png");
        let decoded = image::ImageReader::with_format(
            Cursor::new(&normalized.bytes),
            image::ImageFormat::Png,
        )
        .into_dimensions()
        .expect("transcoded PNG should decode");
        assert_eq!(decoded, (20, 10));
    }

    #[test]
    fn an_oversized_screenshot_is_scaled_to_the_long_edge() {
        let pasted = Image::from_bytes(
            ImageFormat::Png,
            encode(MAX_EDGE * 2, MAX_EDGE, image::ImageFormat::Png),
        );
        let normalized = normalize(&pasted).expect("an oversized PNG should scale");

        let decoded = image::ImageReader::with_format(
            Cursor::new(&normalized.bytes),
            image::ImageFormat::Png,
        )
        .into_dimensions()
        .expect("scaled PNG should decode");
        assert_eq!(
            decoded,
            (MAX_EDGE, MAX_EDGE / 2),
            "the aspect ratio should survive"
        );
    }

    #[test]
    fn an_oversized_photo_stays_a_jpeg_when_scaled() {
        let mut jpeg = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(MAX_EDGE * 2, MAX_EDGE)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .expect("fixture should encode");
        let pasted = Image::from_bytes(ImageFormat::Jpeg, jpeg.into_inner());
        let normalized = normalize(&pasted).expect("an oversized JPEG should scale");

        assert_eq!(normalized.format, ImageFormat::Jpeg);
        let decoded = image::ImageReader::with_format(
            Cursor::new(&normalized.bytes),
            image::ImageFormat::Jpeg,
        )
        .into_dimensions()
        .expect("scaled JPEG should decode");
        assert_eq!(decoded, (MAX_EDGE, MAX_EDGE / 2));
    }

    #[test]
    fn svg_is_refused_with_a_reason() {
        let pasted = Image::from_bytes(ImageFormat::Svg, b"<svg/>".to_vec());
        assert!(normalize(&pasted).is_err());
    }

    #[test]
    fn a_file_from_outside_attaches_mentions_or_refuses() {
        use crate::{
            command::PaletteHostId,
            target::{RefTarget, resolve_target},
        };
        use penna::blocks::palette::PaletteStatus;

        let host = TargetHost {
            id: PaletteHostId(0),
            name: "macbook".to_owned(),
            status: PaletteStatus::Online,
            here: true,
            sessions: Vec::new(),
        };
        let chat = |local, takes_images| Recipient {
            host: &host,
            local,
            takes_images,
            home: Some("/Users/me"),
        };
        let shot = Path::new("/Users/me/Desktop/shot.png");
        let movie = Path::new("/Users/me/Desktop/Screen Recording 2026-11-23 at 20.47.02.mov");
        let reference = "@macbook:~/Desktop/Screen\\ Recording\\ 2026-11-23\\ at\\ 20.47.02.mov";

        assert_eq!(
            arrival(shot, false, chat(true, true)),
            Arrival::Attach(ImageFormat::Png)
        );
        assert_eq!(
            arrival(shot, false, chat(true, false)),
            Arrival::Mention("@macbook:~/Desktop/shot.png".to_owned())
        );
        assert_eq!(
            arrival(movie, false, chat(true, true)),
            Arrival::Mention(reference.to_owned())
        );
        assert_eq!(
            resolve_target(&reference[1..], std::slice::from_ref(&host))
                .map(|target| target.target),
            Some(RefTarget::Path {
                path: "~/Desktop/Screen Recording 2026-11-23 at 20.47.02.mov".to_owned(),
                line: None,
            })
        );
        assert_eq!(
            arrival(movie, false, chat(false, true)),
            Arrival::Refuse(
                "Screen Recording 2026-11-23 at 20.47.02.mov is on this machine; this chat runs on macbook"
                    .to_owned()
            )
        );
        assert_eq!(
            arrival(Path::new("/srv/build logs"), true, chat(true, true)),
            Arrival::Mention("@macbook:/srv/build\\ logs/".to_owned())
        );
    }
}
