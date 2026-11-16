use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use ala::{App, Global, Task};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use zz_client::recent_pages::{HistorySuggestion, RecentPage, RecentPages};

const SAVE_DELAY: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct BrowserHistory {
    hosts: HashMap<String, RecentPages>,
    unsaved: HashSet<String>,
    save: Option<Task<()>>,
}

impl Global for BrowserHistory {}

fn pages<'a>(host: &str, cx: &'a mut App) -> &'a mut RecentPages {
    cx.default_global::<BrowserHistory>()
        .hosts
        .entry(host.to_owned())
        .or_insert_with(|| load(&path(host)))
}

pub(super) fn recent(host: &str, profile: &str, limit: usize, cx: &mut App) -> Vec<RecentPage> {
    zz_client::recent_pages::dedupe_recents(pages(host, cx).recent(profile, limit * 8), limit)
}

pub(super) fn suggestions(
    host: &str,
    profile: &str,
    input: &str,
    limit: usize,
    cx: &mut App,
) -> Vec<HistorySuggestion> {
    pages(host, cx).suggestions(profile, input, limit)
}

pub(super) fn update(
    host: &str,
    cx: &mut App,
    change: impl FnOnce(&mut RecentPages) -> bool,
) -> bool {
    if !change(pages(host, cx)) {
        return false;
    }
    schedule_save(host, cx);
    true
}

fn schedule_save(host: &str, cx: &mut App) {
    let history = cx.global_mut::<BrowserHistory>();
    history.unsaved.insert(host.to_owned());
    if history.save.is_some() {
        return;
    }
    let task = cx.spawn(async move |cx| {
        cx.background_executor().timer(SAVE_DELAY).await;
        let writes = cx.update(|cx| {
            let history = cx.global_mut::<BrowserHistory>();
            if let Some(task) = history.save.take() {
                task.detach();
            }
            std::mem::take(&mut history.unsaved)
                .into_iter()
                .filter_map(|host| {
                    let pages = history.hosts.get(&host)?;
                    Some((path(&host), pages.serialize()))
                })
                .collect::<Vec<_>>()
        });
        cx.background_executor()
            .spawn(async move {
                for (path, contents) in writes {
                    if let Err(error) = save(&path, &contents) {
                        log::warn!("Could not save browser history: {error}");
                    }
                }
            })
            .await;
    });
    cx.global_mut::<BrowserHistory>().save = Some(task);
}

fn path(host: &str) -> PathBuf {
    crate::preferences::support_directory()
        .join("browser-history")
        .join(URL_SAFE_NO_PAD.encode(host))
}

fn load(path: &Path) -> RecentPages {
    match std::fs::read_to_string(path) {
        Ok(source) => RecentPages::deserialize(&source),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Could not load browser history: {error}");
            }
            RecentPages::default()
        }
    }
}

fn save(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, contents)?;
    std::fs::rename(temporary, path)
}
