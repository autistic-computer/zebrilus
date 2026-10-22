use std::{path::PathBuf, time::Duration};

use zz_gpui::BackgroundExecutor;

const CONFIG_POLL_INTERVAL: Duration = Duration::from_millis(500);

pub(super) struct ConfigChanges {
    #[cfg(target_os = "linux")]
    watch: Option<linux::ConfigWatch>,
}

impl ConfigChanges {
    pub(super) fn new(candidates: &[PathBuf]) -> Self {
        #[cfg(not(target_os = "linux"))]
        let _ = candidates;
        Self {
            #[cfg(target_os = "linux")]
            watch: linux::ConfigWatch::new(candidates),
        }
    }

    pub(super) async fn next(&mut self, candidates: &[PathBuf], background: &BackgroundExecutor) {
        #[cfg(target_os = "linux")]
        if let Some(watch) = self.watch.as_mut() {
            if watch.changed().await {
                background.timer(linux::CONFIG_SETTLE_DELAY).await;
                watch.drain();
                if !watch.sync(candidates) {
                    self.watch = None;
                }
                return;
            }
            self.watch = None;
        }
        #[cfg(not(target_os = "linux"))]
        let _ = candidates;
        background.timer(CONFIG_POLL_INTERVAL).await;
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::BTreeSet,
        path::{Path, PathBuf},
        time::Duration,
    };

    use notify::{EventKindMask, RecursiveMode, Watcher as _};

    use super::super::{discover_config_path, themes};

    pub(super) const CONFIG_SETTLE_DELAY: Duration = Duration::from_millis(50);

    pub(super) struct ConfigWatch {
        watcher: notify::RecommendedWatcher,
        watched: BTreeSet<PathBuf>,
        changes: async_channel::Receiver<()>,
    }

    impl ConfigWatch {
        pub(super) fn new(candidates: &[PathBuf]) -> Option<Self> {
            let (sender, changes) = async_channel::bounded(1);
            let watcher = notify::RecommendedWatcher::new(
                move |_| {
                    sender.try_send(()).ok();
                },
                notify::Config::default().with_event_kinds(EventKindMask::CORE),
            )
            .ok()?;
            let mut watch = Self {
                watcher,
                watched: BTreeSet::new(),
                changes,
            };
            (watch.sync(candidates) && !watch.watched.is_empty()).then_some(watch)
        }

        pub(super) async fn changed(&self) -> bool {
            self.changes.recv().await.is_ok()
        }

        pub(super) fn drain(&self) {
            while self.changes.try_recv().is_ok() {}
        }

        pub(super) fn sync(&mut self, candidates: &[PathBuf]) -> bool {
            let wanted = watched_directories(candidates);
            for gone in self.watched.difference(&wanted) {
                self.watcher.unwatch(gone).ok();
            }
            for added in wanted.difference(&self.watched) {
                if self
                    .watcher
                    .watch(added, RecursiveMode::NonRecursive)
                    .is_err()
                {
                    return false;
                }
            }
            self.watched = wanted;
            true
        }
    }

    fn watched_directories(candidates: &[PathBuf]) -> BTreeSet<PathBuf> {
        let configs = candidates.iter().filter_map(|candidate| {
            let directory = candidate.parent()?;
            if directory.is_dir() {
                return Some(directory.to_path_buf());
            }
            directory
                .parent()
                .filter(|base| base.is_dir())
                .map(Path::to_path_buf)
        });
        let discovered = discover_config_path(candidates)
            .into_iter()
            .flat_map(|path| {
                let resolved = path
                    .canonicalize()
                    .ok()
                    .and_then(|resolved| resolved.parent().map(Path::to_path_buf));
                [Some(themes::themes_directory_for(&path)), resolved]
            })
            .flatten()
            .filter(|directory| directory.is_dir());
        configs.chain(discovered).collect()
    }

    #[cfg(test)]
    mod tests {
        use std::{fs, time::Instant};

        use super::super::super::ConfigFileStamp;
        use super::*;

        fn changed_within(watch: &ConfigWatch, limit: Duration) -> bool {
            let deadline = Instant::now() + limit;
            while Instant::now() < deadline {
                if watch.changes.try_recv().is_ok() {
                    watch.drain();
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        }

        #[test]
        fn config_and_theme_writes_wake_the_watch() {
            let root = tempfile::tempdir().expect("temporary directory");
            let config = root.path().join("zz").join("config");
            let candidates = vec![config.clone()];
            let mut watch = ConfigWatch::new(&candidates).expect("the base directory is watched");
            assert!(!changed_within(&watch, Duration::from_millis(100)));

            fs::create_dir(config.parent().unwrap()).unwrap();
            assert!(changed_within(&watch, Duration::from_secs(5)));
            assert!(watch.sync(&candidates));

            let staged = root.path().join("zz").join(".config.tmp");
            fs::write(&staged, "theme-mode = dark\n").unwrap();
            fs::rename(&staged, &config).unwrap();
            assert!(changed_within(&watch, Duration::from_secs(5)));
            assert!(watch.sync(&candidates));

            let themes = themes::themes_directory_for(&config);
            fs::create_dir(&themes).unwrap();
            assert!(changed_within(&watch, Duration::from_secs(5)));
            assert!(watch.sync(&candidates));

            fs::write(themes.join("neon.json"), "{}").unwrap();
            assert!(changed_within(&watch, Duration::from_secs(5)));

            watch.drain();
            fs::write(root.path().join("unrelated"), "x").unwrap();
            assert!(!changed_within(&watch, Duration::from_millis(100)));

            assert!(ConfigFileStamp::detect(&candidates).path.is_some());
            fs::read_to_string(&config).unwrap();
            assert!(!changed_within(&watch, Duration::from_millis(100)));
        }

        #[test]
        fn edits_behind_a_symlinked_config_wake_the_watch() {
            let root = tempfile::tempdir().expect("temporary directory");
            let dotfiles = root.path().join("dotfiles");
            fs::create_dir(&dotfiles).unwrap();
            let target = dotfiles.join("config");
            fs::write(&target, "theme-mode = dark\n").unwrap();
            let config = root.path().join("zz").join("config");
            fs::create_dir(config.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&target, &config).unwrap();
            let watch = ConfigWatch::new(&[config]).expect("the config directory is watched");

            fs::write(&target, "theme-mode = light\n").unwrap();
            assert!(changed_within(&watch, Duration::from_secs(5)));
        }
    }
}
