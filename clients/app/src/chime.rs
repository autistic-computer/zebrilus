use std::sync::atomic::{AtomicBool, Ordering};

use zz_client::attention::Chime;

static ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn play(chime: Chime) {
    if ENABLED.load(Ordering::Relaxed) {
        output(chime);
    }
}

#[cfg(target_family = "wasm")]
fn output(chime: Chime) {
    use base64::Engine as _;

    let source = format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(zz_client::attention::chime_wav(chime))
    );
    let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(&source) else {
        return;
    };
    if let Ok(playing) = audio.play() {
        wasm_bindgen_futures::spawn_local(async move {
            wasm_bindgen_futures::JsFuture::from(playing).await.ok();
        });
    }
}

#[cfg(target_os = "ios")]
#[allow(unsafe_code, unexpected_cfgs)]
fn output(chime: Chime) {
    use std::{ffi::CString, sync::OnceLock};

    use objc::{class, msg_send, runtime::Object, sel, sel_impl};

    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        fn AudioServicesCreateSystemSoundID(url: *mut Object, sound: *mut u32) -> i32;
        fn AudioServicesPlaySystemSound(sound: u32);
        fn AudioServicesPlayAlertSound(sound: u32);
    }

    fn load(chime: Chime) -> Option<u32> {
        let path = std::env::temp_dir().join(format!("zz-chime-{}.wav", chime.name()));
        std::fs::write(&path, zz_client::attention::chime_wav(chime)).ok()?;
        let path = CString::new(path.to_str()?).ok()?;
        let mut sound = 0;
        let status = unsafe {
            let path: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: path.as_ptr()];
            let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path];
            AudioServicesCreateSystemSoundID(url, &raw mut sound)
        };
        (status == 0).then_some(sound)
    }

    static SOUNDS: OnceLock<[Option<u32>; Chime::ALL.len()]> = OnceLock::new();
    let sounds = SOUNDS.get_or_init(|| Chime::ALL.map(load));
    match (chime, sounds[chime as usize]) {
        (_, None) => {}
        (Chime::Bell, Some(sound)) => unsafe { AudioServicesPlayAlertSound(sound) },
        (_, Some(sound)) => unsafe { AudioServicesPlaySystemSound(sound) },
    }
}

#[cfg(not(any(target_family = "wasm", target_os = "ios")))]
const fn output(_chime: Chime) {}
