use std::cell::Cell;

use ala::App;
use web_time::Instant;
use zz_client::attention::{Chime, bell_due};

thread_local! {
    static LAST: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub fn ring(cx: &mut App) {
    let mut last = LAST.get();
    if bell_due(&mut last, Instant::now()) {
        crate::chime::play(Chime::Bell);
    }
    LAST.set(last);
    #[cfg(target_family = "wasm")]
    web::ring(cx);
    #[cfg(not(target_family = "wasm"))]
    let _ = cx;
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
