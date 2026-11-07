use std::sync::Arc;

use zz_protocol::{BrowserDescriptor, DEFAULT_BROWSER_PROFILE, normalize_browser_profile_name};

use crate::url_input::{UrlInputError, normalize_url};

pub const BLANK_URL: &str = "about:blank";

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserContent {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub page_zoom_factor: f64,
    pub page_zoom_percent: u16,
    pub error: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserTab<I> {
    pub id: I,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub page_zoom_factor: f64,
    pub page_zoom_percent: u16,
    pub error: Option<Arc<str>>,
    pub started: bool,
    pub popup: bool,
}

impl<I> BrowserTab<I> {
    #[must_use]
    pub fn new(id: I, url: String) -> Self {
        Self {
            id,
            url,
            title: String::new(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            page_zoom_factor: 1.0,
            page_zoom_percent: 100,
            error: None,
            started: true,
            popup: false,
        }
    }

    #[must_use]
    pub fn restored(id: I, url: String) -> Self {
        Self {
            started: false,
            ..Self::new(id, url)
        }
    }

    #[must_use]
    pub fn content(&self) -> BrowserContent {
        BrowserContent {
            url: self.url.clone(),
            title: self.title.clone(),
            loading: self.loading,
            can_go_back: self.can_go_back,
            can_go_forward: self.can_go_forward,
            page_zoom_factor: self.page_zoom_factor,
            page_zoom_percent: self.page_zoom_percent,
            error: self.error.clone(),
        }
    }

    pub fn save(&mut self, content: &BrowserContent) {
        self.url.clone_from(&content.url);
        self.title.clone_from(&content.title);
        self.loading = content.loading;
        self.can_go_back = content.can_go_back;
        self.can_go_forward = content.can_go_forward;
        self.page_zoom_factor = content.page_zoom_factor;
        self.page_zoom_percent = content.page_zoom_percent;
        self.error.clone_from(&content.error);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserRestore<I> {
    pub tabs: Vec<BrowserTab<I>>,
    pub active_tab: I,
    pub next_tab_id: u64,
    pub current_url: String,
    pub profile: String,
    pub initial_url_error: Option<UrlInputError>,
}

#[must_use]
pub fn restore_tabs<I: Copy>(
    descriptor: &BrowserDescriptor,
    mut id: impl FnMut(u64) -> I,
) -> BrowserRestore<I> {
    let normalized = normalize_url(descriptor.url());
    let initial_url_error = normalized.as_ref().err().cloned();
    let current_url = normalized.unwrap_or_else(|_| BLANK_URL.to_owned());
    let profile = normalize_browser_profile_name(&descriptor.profile)
        .unwrap_or_else(|_| DEFAULT_BROWSER_PROFILE.to_owned());
    let active_index = descriptor
        .active_tab
        .min(descriptor.tabs.len().saturating_sub(1));
    let mut tabs = Vec::with_capacity(descriptor.tabs.len().max(1));
    let mut next_tab_id = 0_u64;
    for (index, url) in descriptor.tabs.iter().enumerate() {
        let tab_id = id(next_tab_id);
        next_tab_id += 1;
        tabs.push(if index == active_index {
            BrowserTab::new(tab_id, current_url.clone())
        } else {
            BrowserTab::restored(
                tab_id,
                normalize_url(url).unwrap_or_else(|_| BLANK_URL.to_owned()),
            )
        });
    }
    if tabs.is_empty() {
        tabs.push(BrowserTab::new(id(0), current_url.clone()));
        next_tab_id = 1;
    }
    let active_tab = tabs[active_index.min(tabs.len() - 1)].id;
    BrowserRestore {
        tabs,
        active_tab,
        next_tab_id,
        current_url,
        profile,
        initial_url_error,
    }
}

#[must_use]
pub fn active_tab_index<I: PartialEq>(tabs: &[BrowserTab<I>], active: &I) -> usize {
    tabs.iter().position(|tab| &tab.id == active).unwrap_or(0)
}

#[must_use]
pub fn close_neighbor<I: Copy + PartialEq>(tabs: &[BrowserTab<I>], tab: &I) -> Option<I> {
    if tabs.len() <= 1 {
        return None;
    }
    let index = tabs.iter().position(|entry| &entry.id == tab)?;
    tabs.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|index| tabs.get(index)))
        .map(|entry| entry.id)
}

#[must_use]
pub fn cycle_tab<I: Copy + PartialEq>(
    tabs: &[BrowserTab<I>],
    active: &I,
    offset: i64,
) -> Option<I> {
    if tabs.len() <= 1 {
        return None;
    }
    let len = i64::try_from(tabs.len()).unwrap_or(i64::MAX);
    let index = i64::try_from(active_tab_index(tabs, active)).unwrap_or_default();
    let next = usize::try_from((index + offset).rem_euclid(len)).unwrap_or_default();
    Some(tabs[next].id)
}

#[must_use]
pub fn session_owner<I: Copy>(
    sessions: impl IntoIterator<Item = (I, u64)>,
    session: u64,
) -> Option<I> {
    sessions
        .into_iter()
        .find_map(|(key, current)| (current == session).then_some(key))
}

#[must_use]
pub fn switch_tab<I: Copy + PartialEq>(
    tabs: &mut [BrowserTab<I>],
    active: &mut I,
    current: &BrowserContent,
    target: I,
) -> Option<BrowserContent> {
    if target == *active {
        return None;
    }
    let index = tabs.iter().position(|tab| tab.id == target)?;
    let previous = active_tab_index(tabs, active);
    tabs[previous].save(current);
    *active = target;
    Some(tabs[index].content())
}

#[must_use]
pub fn load_error(url: &str, description: &str, background: bool) -> Arc<str> {
    let url = crate::url_input::diagnostic_url(url);
    if background {
        Arc::from(format!("Could not load {url}: {description}"))
    } else {
        Arc::from(format!("{url}: {description}"))
    }
}
