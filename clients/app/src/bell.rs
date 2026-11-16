use ala::App;

pub fn ring(cx: &mut App) {
    #[cfg(target_os = "ios")]
    ios::ring();
    #[cfg(target_family = "wasm")]
    web::ring(cx);
    #[cfg(not(target_family = "wasm"))]
    let _ = cx;
}

#[cfg(target_os = "ios")]
#[allow(unsafe_code)]
mod ios {
    use std::{
        cell::Cell,
        time::{Duration, Instant},
    };

    const BEEP: u32 = 1052;
    const QUIET: Duration = Duration::from_millis(200);

    thread_local! {
        static LAST: Cell<Option<Instant>> = const { Cell::new(None) };
    }

    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        fn AudioServicesPlayAlertSound(sound: u32);
    }

    pub(super) fn ring() {
        let now = Instant::now();
        if LAST
            .get()
            .is_some_and(|last| now.duration_since(last) < QUIET)
        {
            return;
        }
        LAST.set(Some(now));
        unsafe { AudioServicesPlayAlertSound(BEEP) };
    }
}

#[cfg(target_family = "wasm")]
mod web {
    use std::{cell::Cell, time::Duration};

    use ala::App;

    const MARK: &str = "• ";
    const FLASHES: usize = 3;
    const PHASE: Duration = Duration::from_millis(400);

    thread_local! {
        static FLASHING: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn ring(cx: &mut App) {
        if FLASHING.replace(true) {
            return;
        }
        cx.spawn(async move |cx| {
            for marked in [true, false].into_iter().cycle().take(FLASHES * 2) {
                mark_title(marked);
                cx.background_executor().timer(PHASE).await;
            }
            FLASHING.set(false);
        })
        .detach();
    }

    fn mark_title(marked: bool) {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let title = document.title();
        let bare = title.strip_prefix(MARK).unwrap_or(&title);
        if marked {
            document.set_title(&format!("{MARK}{bare}"));
        } else {
            document.set_title(bare);
        }
    }
}
