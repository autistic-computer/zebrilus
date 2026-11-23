use std::borrow::Cow;

pub const UI_FONT: &str = "Plumula Sans";
pub const CHAT_FONT: &str = "Plumula Serif";

macro_rules! plumula {
    ($($file:literal),+ $(,)?) => {
        vec![$(Cow::Borrowed(
            include_bytes!(concat!("../assets/fonts/plumula/", $file)).as_slice()
        )),+]
    };
}

#[cfg(any(target_family = "wasm", target_os = "ios", target_os = "android"))]
#[must_use]
pub fn plumula() -> Vec<Cow<'static, [u8]>> {
    plumula!("PlumulaSans-Variable.ttf", "PlumulaSerif-Variable.ttf")
}

#[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
#[must_use]
pub fn plumula() -> Vec<Cow<'static, [u8]>> {
    plumula!(
        "PlumulaSans-Light.ttf",
        "PlumulaSans-Regular.ttf",
        "PlumulaSans-Medium.ttf",
        "PlumulaSans-SemiBold.ttf",
        "PlumulaSans-Bold.ttf",
        "PlumulaSans-ExtraBold.ttf",
        "PlumulaSerif-Light.ttf",
        "PlumulaSerif-Regular.ttf",
        "PlumulaSerif-Medium.ttf",
        "PlumulaSerif-SemiBold.ttf",
        "PlumulaSerif-Bold.ttf",
        "PlumulaSerif-ExtraBold.ttf",
    )
}
