use ala::{App, Context, Window};
use penna::blocks::touch::{PhoneFind, StackGroup};

use super::super::AppShell;
use super::Page;

impl AppShell {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn shown_browser(
        &self,
        cx: &App,
    ) -> Option<ala::Entity<super::super::browser_pane::BrowserPane>> {
        let model = self.status_model(cx);
        let (pages, _) = super::pages(&model, &self.unseen_agents);
        pages
            .get(self.compact.pager.index())
            .and_then(|page| self.browsers.get(&page.pane).cloned())
    }

    pub(super) fn browser_groups(
        &self,
        page: Option<&Page>,
        cx: &App,
    ) -> Option<(Vec<StackGroup>, usize)> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = page.and_then(|page| self.browsers.get(&page.pane)) {
            let state = browser.read(cx).phone_state(cx);
            let menu = zz_ui::compact::BrowserMenu {
                address: &state.url,
                back: state.back,
                forward: state.forward,
                loading: state.loading,
                tabs: state.tabs,
                open_on_phone: super::super::settings::phone()
                    && zz_client::terminal_view::openable_uri(&state.url),
            };
            return Some(zz_ui::compact::browser_menu(menu));
        }
        let _ = (page, cx);
        None
    }

    pub(super) fn browser_menu_pick(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            use super::super::browser_pane::BrowserPane;
            use zz_ui::compact::{
                ADDRESS, BACK, FORWARD, OPEN_ON_PHONE, PICK_ELEMENT, RELOAD, STOP, TABS,
            };
            if ![
                ADDRESS,
                BACK,
                FORWARD,
                OPEN_ON_PHONE,
                PICK_ELEMENT,
                RELOAD,
                STOP,
                TABS,
            ]
            .contains(&id)
            {
                return false;
            }
            let Some(browser) = self.shown_browser(cx) else {
                return true;
            };
            match id {
                BACK => browser.update(cx, |browser, cx| browser.go_back(cx)),
                FORWARD => browser.update(cx, |browser, cx| browser.go_forward(cx)),
                RELOAD => browser.update(cx, BrowserPane::reload_page),
                STOP => browser.update(cx, BrowserPane::stop_page),
                ADDRESS => browser.update(cx, |browser, cx| browser.open_address(window, cx)),
                TABS => browser.update(cx, BrowserPane::open_tabs),
                PICK_ELEMENT => browser.update(cx, |browser, cx| browser.start_pick(window, cx)),
                _ => {
                    let url = browser.read(cx).phone_state(cx).url;
                    if zz_client::terminal_view::openable_uri(&url) {
                        cx.open_url(&url);
                    }
                }
            }
            cx.notify();
            true
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = (id, window, cx);
            false
        }
    }

    pub(super) fn browser_address_field(&self, cx: &App) -> Option<PhoneFind> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.shown_browser(cx) {
            use penna::ActiveTheme as _;
            let input = browser.read(cx).address_field()?;
            let close = browser.downgrade();
            let clear = browser.downgrade();
            return Some(
                PhoneFind::new("compact-address", &input, cx.theme().ground())
                    .on_close(move |window, cx| {
                        let _ = close.update(cx, |browser, cx| browser.close_address(window, cx));
                    })
                    .on_clear(move |window, cx| {
                        let _ = clear.update(cx, |browser, cx| browser.clear_address(window, cx));
                    }),
            );
        }
        let _ = cx;
        None
    }

    pub(super) fn browser_suggestions(&self, cx: &App) -> Option<ala::AnyElement> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.shown_browser(cx) {
            use ala::{IntoElement as _, ParentElement as _, Styled as _, canvas, div};
            use penna::ActiveTheme as _;
            let pane = browser.read(cx);
            pane.address_field()?;
            let pick = browser.downgrade();
            let rows = zz_ui::compact::address_suggestions(
                pane.phone_suggestions(),
                move |url, window, cx| {
                    let _ = pick.update(cx, |browser, cx| browser.pick_page(&url, window, cx));
                },
                cx.theme().ground(),
                cx,
            )?;
            let hole = pane.suggestions_hole();
            let notify = browser.downgrade();
            let radius = cx.theme().phone_radius();
            return Some(
                div()
                    .relative()
                    .w_full()
                    .child(rows)
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                let next = Some((bounds, radius));
                                if hole.replace(next) != next {
                                    let notify = notify.clone();
                                    window.on_next_frame(move |_, cx| {
                                        let _ = notify.update(cx, |_, cx| cx.notify());
                                    });
                                }
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .into_any_element(),
            );
        }
        let _ = cx;
        None
    }

    pub(super) fn browser_tabs_open(&self, cx: &App) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.shown_browser(cx) {
            return browser.read(cx).tabs_open();
        }
        let _ = cx;
        false
    }

    pub(super) fn browser_picking(&self, cx: &App) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.shown_browser(cx) {
            return browser.read(cx).phone_state(cx).picking;
        }
        let _ = cx;
        false
    }

    pub(super) fn end_browser_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.shown_browser(cx) {
            browser.update(cx, |browser, cx| browser.end_pick(window, cx));
        }
        let _ = (window, cx);
    }
}
