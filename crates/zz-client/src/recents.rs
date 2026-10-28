use serde::{Deserialize, Serialize};
use zz_protocol::{PaneId, SessionId, WindowId};

pub const RECENT_LIMIT: usize = 200;
pub const HALF_LIFE_SECS: f64 = 3.0 * 24.0 * 60.0 * 60.0;
const MAX_BOOST: f64 = 12.0;
const BOOST_SCALE: f64 = 6.0;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecentKey {
    Place {
        host: String,
        session: SessionId,
        window: Option<WindowId>,
        pane: Option<PaneId>,
    },
    Command(String),
    Gui(String),
    File {
        host: String,
        path: String,
        dir: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecentKind {
    Place,
    Command,
    Gui,
    File,
}

impl RecentKey {
    #[must_use]
    pub const fn kind(&self) -> RecentKind {
        match self {
            Self::Place { .. } => RecentKind::Place,
            Self::Command(_) => RecentKind::Command,
            Self::Gui(_) => RecentKind::Gui,
            Self::File { .. } => RecentKind::File,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub key: RecentKey,
    pub label: String,
    pub score: f64,
    pub last: u64,
}

impl Recent {
    #[must_use]
    pub fn frecency(&self, now: u64) -> f64 {
        let age = now.saturating_sub(self.last) as f64;
        self.score * 0.5_f64.powf(age / HALF_LIFE_SECS)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Recents {
    entries: Vec<Recent>,
}

impl Recents {
    #[must_use]
    pub fn entries(&self) -> &[Recent] {
        &self.entries
    }

    pub fn record(&mut self, key: RecentKey, label: impl Into<String>, now: u64) {
        let label = label.into();
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.score = entry.frecency(now) + 1.0;
            entry.last = now.max(entry.last);
            entry.label = label;
            return;
        }
        self.entries.push(Recent {
            key,
            label,
            score: 1.0,
            last: now,
        });
        if self.entries.len() > RECENT_LIMIT
            && let Some(weakest) = self
                .entries
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| left.frecency(now).total_cmp(&right.frecency(now)))
                .map(|(index, _)| index)
        {
            self.entries.remove(weakest);
        }
    }

    #[must_use]
    pub fn frecency(&self, key: &RecentKey, now: u64) -> f64 {
        self.entries
            .iter()
            .find(|entry| entry.key == *key)
            .map_or(0.0, |entry| entry.frecency(now))
    }

    #[must_use]
    pub fn boost(&self, key: &RecentKey, now: u64) -> i64 {
        (self.frecency(key, now).ln_1p() * BOOST_SCALE)
            .round()
            .min(MAX_BOOST) as i64
    }

    #[must_use]
    pub fn top(&self, kind: RecentKind, limit: usize, now: u64) -> Vec<&Recent> {
        let mut found = self
            .entries
            .iter()
            .filter(|entry| entry.key.kind() == kind)
            .collect::<Vec<_>>();
        found.sort_by(|left, right| right.frecency(now).total_cmp(&left.frecency(now)));
        found.truncate(limit);
        found
    }

    pub fn retain(&mut self, keep: impl FnMut(&Recent) -> bool) {
        self.entries.retain(keep);
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    #[must_use]
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }
}

#[must_use]
pub fn now_secs() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
#[path = "recents_tests.rs"]
mod tests;
