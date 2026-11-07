use std::{error::Error, fs, sync::Arc};
use zz_client::{
    browser_state::{
        BrowserContent, active_tab_index, close_neighbor, cycle_tab, load_error, restore_tabs,
        switch_tab,
    },
    url_input::{SearchProvider, UrlInputError, diagnostic_url, normalize_url, resolve_address},
};
use zz_protocol::{BrowserDescriptor, normalize_browser_profile_name};

fn uint(out: &mut Vec<u8>, value: u64, count: usize) {
    out.extend_from_slice(&value.to_le_bytes()[..count]);
}

fn text(out: &mut Vec<u8>, value: &str) {
    uint(out, value.len() as u64, 4);
    out.extend_from_slice(value.as_bytes());
}

fn error_code(error: &UrlInputError) -> u8 {
    match error {
        UrlInputError::Empty => 1,
        UrlInputError::UnsupportedScheme => 2,
        UrlInputError::Invalid => 3,
    }
}

fn url_result(out: &mut Vec<u8>, result: Result<String, UrlInputError>) {
    match result {
        Ok(value) => {
            out.push(0);
            text(out, &value);
        }
        Err(error) => {
            out.push(error_code(&error));
            text(out, "");
        }
    }
}

fn write_content(out: &mut Vec<u8>, value: &BrowserContent) {
    text(out, &value.url);
    text(out, &value.title);
    out.push(u8::from(value.loading));
    out.push(u8::from(value.can_go_back));
    out.push(u8::from(value.can_go_forward));
    uint(out, value.page_zoom_factor.to_bits(), 8);
    uint(out, u64::from(value.page_zoom_percent), 2);
    out.push(u8::from(value.error.is_some()));
    if let Some(error) = &value.error {
        text(out, error);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut out = Vec::new();
    let values = [
        "",
        " ",
        "ABOUT:blank",
        " about:blank ",
        "example.com",
        "https://EXAMPLE.com:443/a/../b?q=a b#f",
        "localhost:3000",
        "127.0.0.1",
        "0.0.0.0:80/a",
        "[::1]:8080",
        "foo.localhost",
        "file:///tmp/a b",
        "zz://agent/status",
        "data:text/plain,a",
        "javascript:alert(1)",
        "mailto:a@b.test",
        "https://",
        "http://[",
        "https://user:pass@example.com/a?q=secret#private",
        "https://例え.テスト/道",
        "search words",
        "a+b & c?",
        "\u{2003}localhost\u{2003}",
        "xn--r8jz45g.xn--zckzah",
        "a:123",
        "a:port",
        "https://x/\0a",
        "http://x/é",
    ];
    uint(&mut out, (values.len() * 5) as u64, 4);
    for value in values {
        for operation in 0_u8..5 {
            out.push(match operation {
                0 => 0,
                1 => 2,
                _ => 1,
            });
            out.push(operation.saturating_sub(2));
            text(&mut out, value);
            match operation {
                0 => url_result(&mut out, normalize_url(value)),
                1 => url_result(&mut out, Ok(diagnostic_url(value))),
                _ => url_result(
                    &mut out,
                    resolve_address(value, SearchProvider::ALL[usize::from(operation - 2)]),
                ),
            }
        }
    }
    let profiles = [
        "default",
        "zz-default",
        " ZZ-default ",
        "\u{2003}work\u{a0}",
        "",
        " ",
        "a\0b",
        "a\u{85}b",
        "例え",
        "a\u{200b}b",
    ];
    uint(&mut out, profiles.len() as u64 + 3, 4);
    for profile in profiles.into_iter().map(str::to_owned).chain([
        "a".repeat(64),
        "a".repeat(65),
        "é".repeat(33),
    ]) {
        text(&mut out, &profile);
        let result = normalize_browser_profile_name(&profile);
        out.push(u8::from(result.is_ok()));
        text(&mut out, result.as_deref().unwrap_or(""));
    }
    let tab_sets: Vec<Vec<String>> = vec![
        vec![],
        vec!["about:blank".to_owned()],
        vec!["example.com".to_owned(), "localhost:80".to_owned()],
        vec![
            "javascript:bad".to_owned(),
            "file:///tmp/x".to_owned(),
            "https://x/y".to_owned(),
        ],
        vec![
            String::new(),
            "ABOUT:blank".to_owned(),
            "https://user:p@x/?s#t".to_owned(),
            "https://例え.テスト".to_owned(),
        ],
    ];
    uint(&mut out, (tab_sets.len() * 5 * 3) as u64, 4);
    for tabs in tab_sets {
        for active in [0, 1, 2, 3, usize::MAX] {
            for profile in ["default", "zz-default", "bad\0profile"] {
                uint(&mut out, tabs.len() as u64, 4);
                for url in &tabs {
                    text(&mut out, url);
                }
                uint(&mut out, active as u64, 8);
                text(&mut out, profile);
                let state = restore_tabs(
                    &BrowserDescriptor {
                        tabs: tabs.clone(),
                        active_tab: active,
                        profile: profile.to_owned(),
                    },
                    |id| id,
                );
                uint(&mut out, state.tabs.len() as u64, 4);
                uint(&mut out, state.active_tab, 8);
                uint(&mut out, state.next_tab_id, 8);
                text(&mut out, &state.current_url);
                text(&mut out, &state.profile);
                out.push(state.initial_url_error.as_ref().map_or(0, error_code));
                for tab in &state.tabs {
                    uint(&mut out, tab.id, 8);
                    text(&mut out, &tab.url);
                    out.push(u8::from(tab.started));
                    out.push(u8::from(tab.popup));
                }
                uint(
                    &mut out,
                    active_tab_index(&state.tabs, &state.active_tab) as u64,
                    4,
                );
                for id in [0, 1, 2, 3, u64::MAX] {
                    let neighbor = close_neighbor(&state.tabs, &id);
                    uint(&mut out, neighbor.unwrap_or(u64::MAX), 8);
                }
                for offset in [-17, -1, 0, 1, 17] {
                    uint(
                        &mut out,
                        cycle_tab(&state.tabs, &state.active_tab, offset).unwrap_or(u64::MAX),
                        8,
                    );
                }
            }
        }
    }
    let mut tab = zz_client::browser_state::BrowserTab::restored(42_u64, "old".to_owned());
    let content = BrowserContent {
        url: "https://new/".to_owned(),
        title: "title\0東京".to_owned(),
        loading: true,
        can_go_back: true,
        can_go_forward: false,
        page_zoom_factor: 1.25,
        page_zoom_percent: 125,
        error: Some(Arc::from("ERR")),
    };
    tab.save(&content);
    let saved = tab.content();
    text(&mut out, &saved.url);
    text(&mut out, &saved.title);
    out.push(u8::from(saved.loading));
    out.push(u8::from(saved.can_go_back));
    out.push(u8::from(saved.can_go_forward));
    uint(&mut out, saved.page_zoom_factor.to_bits(), 8);
    uint(&mut out, u64::from(saved.page_zoom_percent), 2);
    text(&mut out, saved.error.as_deref().unwrap_or(""));
    let mut switches = Vec::new();
    let mut switch_count = 0_u64;
    for count in 1_u64..9 {
        for active in [0, count - 1, u64::MAX] {
            for target in 0..=count {
                switch_count += 1;
                let mut tabs: Vec<_> = (0..count)
                    .map(|id| {
                        let mut tab = zz_client::browser_state::BrowserTab::restored(
                            id,
                            format!("https://tab-{id}/"),
                        );
                        tab.title = format!("saved-{id}");
                        tab.loading = id % 2 == 0;
                        tab.can_go_back = id % 3 == 0;
                        tab.can_go_forward = id % 4 == 0;
                        tab.page_zoom_factor = 0.5 + id as f64 * 0.25;
                        tab.page_zoom_percent = 50 + u16::try_from(id).unwrap_or_default() * 25;
                        tab.error = (id % 2 == 1).then(|| Arc::from("saved error"));
                        tab.popup = id % 3 == 1;
                        tab
                    })
                    .collect();
                let current = BrowserContent {
                    url: "https://current/".to_owned(),
                    title: "active title".to_owned(),
                    loading: false,
                    can_go_back: true,
                    can_go_forward: true,
                    page_zoom_factor: 1.75,
                    page_zoom_percent: 175,
                    error: Some(Arc::from("active error")),
                };
                uint(&mut switches, count, 4);
                uint(&mut switches, active, 8);
                uint(&mut switches, target, 8);
                for tab in &tabs {
                    uint(&mut switches, tab.id, 8);
                    write_content(&mut switches, &tab.content());
                    switches.push(u8::from(tab.started));
                    switches.push(u8::from(tab.popup));
                }
                write_content(&mut switches, &current);
                let mut selected = active;
                let updated = switch_tab(&mut tabs, &mut selected, &current, target);
                let current = updated.unwrap_or(current);
                if selected != active {
                    tabs.iter_mut()
                        .find(|tab| tab.id == selected)
                        .ok_or("missing selected tab")?
                        .started = true;
                }
                uint(&mut switches, selected, 8);
                write_content(&mut switches, &current);
                for tab in &tabs {
                    write_content(&mut switches, &tab.content());
                    switches.push(u8::from(tab.started));
                    switches.push(u8::from(tab.popup));
                }
            }
        }
    }
    uint(&mut out, switch_count, 4);
    out.extend(switches);
    uint(&mut out, 6, 4);
    for url in [
        "https://u:p@host/a?secret#private",
        "about:blank",
        "not a valid url",
    ] {
        for background in [false, true] {
            text(&mut out, url);
            text(&mut out, "ERR 東京");
            out.push(u8::from(background));
            text(&mut out, &load_error(url, "ERR 東京", background));
        }
    }
    fs::write(std::env::args().nth(1).ok_or("missing output path")?, out)?;
    Ok(())
}
