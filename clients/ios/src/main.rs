#[cfg(target_os = "ios")]
use zz_app::{app, input};

#[cfg(target_os = "ios")]
fn main() {
    use ala::{App, Application, px};
    use penna::{Theme, UiZoom};
    use std::{borrow::Cow, rc::Rc};

    init_logging();
    let application = Application::with_platform(Rc::new(
        ala_platform::ios::IosPlatform::new().with_touch_gestures(true),
    ))
    .with_assets(zz_ui::Assets);
    let (url_sender, mut url_receiver) = futures::channel::mpsc::unbounded::<Vec<String>>();
    application.on_open_urls(move |urls| {
        url_sender.unbounded_send(urls).ok();
    });
    application.on_reopen(open_workspace);
    application.run(|cx: &mut App| {
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!(
                    "../../web/assets/fonts/inter/InterVariable.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../web/assets/fonts/inter/InterVariable-Italic.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../web/assets/fonts/lilex/Lilex-Regular.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../web/assets/fonts/lilex/Lilex-Bold.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../web/assets/fonts/lilex/Lilex-Italic.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../web/assets/fonts/lilex/Lilex-BoldItalic.ttf"
                )),
            ])
            .expect("load interface fonts");
        zz_ui::init(cx);
        zz_app::ios_preview::show_html_in_chats(cx);
        cx.bind_keys(input::raw_key_bindings());
        let theme = Theme::global_mut(cx);
        theme.font_family = "Inter Variable".into();
        theme.mono_font_family = "Lilex".into();
        theme.radius = px(6.);
        cx.set_global(UiZoom(1.0));
        open_workspace(cx);
        cx.spawn(async move |cx| {
            use futures::StreamExt as _;
            while let Some(urls) = url_receiver.next().await {
                for endpoint in urls.iter().filter_map(|url| app::host_from_url(url)) {
                    cx.update(|cx| cx.dispatch_action(&app::OpenHost { endpoint }));
                }
                for name in urls.iter().filter_map(|url| app::session_from_url(url)) {
                    cx.update(|cx| cx.dispatch_action(&app::OpenSession { name }));
                }
            }
        })
        .detach();
    });
}

#[cfg(target_os = "ios")]
struct StderrAndSystemLog {
    stderr: env_logger::Logger,
    system: oslog::OsLogger,
}

#[cfg(target_os = "ios")]
impl log::Log for StderrAndSystemLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.stderr.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.stderr.matches(record) {
            self.stderr.log(record);
            self.system.log(record);
        }
    }

    fn flush(&self) {
        self.stderr.flush();
    }
}

#[cfg(target_os = "ios")]
fn init_logging() {
    let stderr =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
    let max_level = stderr.filter();
    let system = oslog::OsLogger::new("dev.zz.ios");
    if log::set_boxed_logger(Box::new(StderrAndSystemLog { stderr, system })).is_ok() {
        log::set_max_level(max_level);
    }
}

#[cfg(target_os = "ios")]
fn open_workspace(cx: &mut ala::App) {
    use ala::{AppContext, WindowOptions};
    use penna::Root;

    cx.open_window(WindowOptions::default(), |window, cx| {
        let app = cx.new(|cx| app::AppShell::new(window, cx));
        window.focus(&app.read(cx).focus.clone(), cx);
        cx.new(|cx| Root::new(app, window, cx).bordered(false).own_layers(false))
    })
    .expect("open zz iOS window");
}

#[cfg(not(target_os = "ios"))]
fn main() {}
