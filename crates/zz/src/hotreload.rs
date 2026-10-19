#[cfg(all(feature = "hotreload", debug_assertions, not(target_family = "wasm")))]
pub(crate) fn init(cx: &mut zz_gpui::App) {
    if !zz_protocol::app_identity::DEVELOPMENT {
        return;
    }
    let (patch_applied, patches) = async_channel::bounded(1);
    dioxus_devtools::subsecond::register_handler(std::sync::Arc::new(move || {
        let _ = patch_applied.try_send(());
    }));
    cx.spawn(async move |cx| {
        while patches.recv().await.is_ok() {
            log::info!(target: "zz::hotreload", "patch applied; refreshing windows");
            cx.update(zz_gpui::App::refresh_windows);
        }
    })
    .detach();
    dioxus_devtools::connect_subsecond();
}
