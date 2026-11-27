use super::*;

impl Around {
    fn near(&self, hosts: &[TargetHost]) -> Near<'_> {
        let spot = self.own.and_then(|own| {
            hosts
                .iter()
                .filter(|host| host.here)
                .flat_map(|host| &host.sessions)
                .find_map(|session| {
                    session
                        .windows
                        .iter()
                        .find(|window| window.panes.iter().any(|pane| pane.id == own))
                        .map(|window| (session.id, window.id))
                })
        });
        Near {
            around: self,
            session: spot.map(|(session, _)| session),
            window: spot.map(|(_, window)| window),
        }
    }
}

impl Near<'_> {
    fn prior(
        &self,
        host: &TargetHost,
        session: Option<SessionId>,
        window: Option<WindowId>,
        depth: u32,
        key: Option<&RecentKey>,
    ) -> Prior {
        let closeness = if !host.here {
            Closeness::Elsewhere
        } else if window.is_some() && window == self.window {
            Closeness::Window
        } else if session.is_some() && session == self.session {
            Closeness::Session
        } else {
            Closeness::Host
        };
        let recent = key.map_or(0.0, |key| {
            self.around.recents.boost(key, self.around.now) as f32 / MAX_BOOST as f32
        });
        Prior {
            closeness,
            recent,
            depth,
            changed: false,
        }
    }
}

fn place_key(
    host: &TargetHost,
    session: SessionId,
    window: Option<WindowId>,
    pane: Option<PaneId>,
) -> RecentKey {
    RecentKey::Place {
        host: host.name.clone(),
        session,
        window,
        pane,
    }
}

fn host_item(host: &TargetHost, place: &Place, near: &Near) -> TargetItem {
    let detail = match host.status {
        PaletteStatus::Online | PaletteStatus::Running => count(host.sessions.len(), "session"),
        PaletteStatus::Waiting => "connecting".to_owned(),
        PaletteStatus::Offline => "offline".to_owned(),
    };
    let mut item = TargetItem::new(
        TargetKind::Host,
        host.name.clone(),
        "",
        place,
        "",
        host_icon(host),
    )
    .near(near.prior(host, None, None, 0, None), None);
    item.status = Some(host.status);
    item.meta = detail.into();
    item
}

fn session_item(
    host: &TargetHost,
    session: &TargetSession,
    place: &Place,
    label: String,
    near: &Near,
) -> TargetItem {
    let key = place_key(host, session.id, None, None);
    let prior = near.prior(host, Some(session.id), None, 1, Some(&key));
    let mut item = TargetItem::new(
        TargetKind::Session,
        label,
        "",
        place,
        &session_ref(session),
        IconName::Layers,
    )
    .at(PaletteTarget::Session(session.id))
    .near(prior, Some(key));
    item.meta = count(session.windows.len(), "window").into();
    item
}

fn window_item(
    host: &TargetHost,
    session: &TargetSession,
    window: &TargetWindow,
    place: &Place,
    label: String,
    near: &Near,
) -> TargetItem {
    let key = place_key(host, session.id, Some(window.id), None);
    let prior = near.prior(host, Some(session.id), Some(window.id), 2, Some(&key));
    let mut item = TargetItem::new(
        TargetKind::Window,
        label,
        "",
        place,
        &format!("{}:{}", session_ref(session), window.index),
        IconName::AppWindow,
    )
    .at(PaletteTarget::Window(window.id))
    .near(prior, Some(key));
    item.meta = count(window.panes.len(), "pane").into();
    item
}

impl Spot<'_> {
    fn crumbs(&self, pane: &TargetPane) -> String {
        let title = pane.title.trim();
        [self.session.name.as_str(), self.window.name.as_str()]
            .into_iter()
            .chain((pane.program.is_some() && !title.is_empty()).then_some(title))
            .collect::<Vec<_>>()
            .join(" › ")
    }
}

fn pane_item(
    spot: &Spot,
    pane: &TargetPane,
    place: &Place,
    rest: &str,
    where_: Option<String>,
    near: &Near,
) -> TargetItem {
    let key = place_key(
        spot.host,
        spot.session.id,
        Some(spot.window.id),
        Some(pane.id),
    );
    let prior = near.prior(
        spot.host,
        Some(spot.session.id),
        Some(spot.window.id),
        3,
        Some(&key),
    );
    TargetItem::new(
        TargetKind::Pane,
        pane_name(pane),
        pane_note(pane, where_),
        place,
        rest,
        pane.icon.clone(),
    )
    .at(PaletteTarget::Pane(pane.id))
    .near(prior, Some(key))
}

fn chat_items(hosts: &[TargetHost], query: &Query, near: &Near) -> Vec<TargetItem> {
    let mut items = Vec::new();
    let own = near.around.own;
    for host in hosts
        .iter()
        .filter(|host| host.here)
        .chain(hosts.iter().filter(|host| !host.here))
        .filter(|host| matches!(host.status, PaletteStatus::Online | PaletteStatus::Running))
    {
        let place = Place {
            host,
            written: false,
        };
        for session in &host.sessions {
            for window in &session.windows {
                for pane in &window.panes {
                    let mine = host.here && Some(pane.id) == own;
                    let Some(chat) = pane.chat.as_ref().filter(|_| !mine) else {
                        continue;
                    };
                    let detail = pane
                        .program
                        .iter()
                        .map(ToString::to_string)
                        .chain(std::iter::once(format!(
                            "{}:{}",
                            session.name, window.index
                        )))
                        .chain((!host.here).then(|| host.name.clone()))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    let key = place_key(host, session.id, Some(window.id), Some(pane.id));
                    let prior = near.prior(host, Some(session.id), Some(window.id), 3, Some(&key));
                    let item = TargetItem::new(
                        TargetKind::Chat,
                        chat.title.clone(),
                        detail.clone(),
                        &place,
                        &pane.id.to_string(),
                        chat.icon.clone(),
                    )
                    .at(PaletteTarget::Pane(pane.id))
                    .near(prior, Some(key));
                    items.extend(item.scored(query, &detail));
                }
            }
        }
    }
    items
}

fn loose_item(
    kind: TargetKind,
    label: impl Into<SharedString>,
    icon: IconName,
    typed: String,
    token: String,
) -> TargetItem {
    TargetItem {
        kind,
        label: label.into(),
        matches: Vec::new(),
        detail: SharedString::default(),
        status: None,
        mark: None,
        icon,
        host: String::new(),
        host_id: PaletteHostId::default(),
        target: None,
        entry: None,
        typed,
        token,
        shown: None,
        meta: SharedString::default(),
        score: 0,
        recent: None,
    }
}

fn agent_items(agents: &[TargetAgent], query: &Query) -> Vec<TargetItem> {
    let prior = Prior {
        closeness: Closeness::Window,
        recent: 0.0,
        depth: 0,
        changed: false,
    };
    agents
        .iter()
        .filter_map(|agent| {
            let mut item = loose_item(
                TargetKind::Agent,
                agent.name.clone(),
                agent.icon.clone(),
                agent.name.to_string(),
                format!("+{}", agent.name),
            )
            .near(prior, None);
            item.meta = agent.model.clone().into();
            item.scored(query, "")
        })
        .collect()
}

pub(super) fn model_name(choice: &Choice) -> &str {
    if choice.name.is_empty() {
        &choice.value
    } else {
        &choice.name
    }
}

pub(super) fn model_items(
    agent: &TargetAgent,
    models: &[Choice],
    first: &str,
    query: &Query,
) -> Vec<TargetItem> {
    let name = &agent.name;
    let mut models = models.iter().collect::<Vec<_>>();
    if let Some(at) = models.iter().position(|choice| choice.value == first) {
        let picked = models.remove(at);
        models.insert(0, picked);
    }
    models
        .into_iter()
        .filter_map(|choice| {
            let value = escape_model(&choice.value);
            loose_item(
                TargetKind::Model,
                model_name(choice).to_owned(),
                agent.icon.clone(),
                format!("{name}/{value}"),
                format!("+{name}/{value}"),
            )
            .scored(query, &choice.value)
        })
        .collect()
}

fn home_item(place: &Place, near: &Near) -> TargetItem {
    TargetItem::new(
        TargetKind::Dir,
        "~/",
        "files",
        place,
        "~/",
        IconName::Folder,
    )
    .near(near.prior(place.host, None, None, 0, None), None)
}

fn folder_item(place: &Place, folder: &str, near: &Near) -> TargetItem {
    let name = folder
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(folder)
        .to_owned();
    TargetItem::new(
        TargetKind::Dir,
        "./",
        name,
        place,
        &escape(folder),
        IconName::Folder,
    )
    .near(near.prior(place.host, None, None, 0, None), None)
}

fn spots(host: &TargetHost) -> impl Iterator<Item = Spot<'_>> {
    host.sessions.iter().flat_map(move |session| {
        session.windows.iter().map(move |window| Spot {
            host,
            session,
            window,
        })
    })
}

fn panes_by_id(host: &TargetHost, place: &Place, query: &Query, near: &Near) -> Vec<TargetItem> {
    spots(host)
        .flat_map(|spot| spot.window.panes.iter().map(move |pane| (spot, pane)))
        .filter_map(|(spot, pane)| {
            let mut item = pane_item(
                &spot,
                pane,
                place,
                &pane.id.to_string(),
                Some(format!("{}:{}", spot.session.name, spot.window.index)),
                near,
            );
            item.shown = Some(item.label.clone());
            item.label = pane.id.to_string().into();
            item.scored(query, "")
        })
        .collect()
}

fn fleet_items(hosts: &[TargetHost], query: &Query, near: &Near) -> Vec<TargetItem> {
    let mut items = Vec::new();
    for host in hosts {
        let place = Place {
            host,
            written: false,
        };
        items.extend(host_item(host, &place, near).scored(query, ""));
        if !matches!(host.status, PaletteStatus::Online | PaletteStatus::Running) {
            continue;
        }
        for session in &host.sessions {
            let (typed, _) = place.target(&session_ref(session));
            items.extend(
                session_item(host, session, &place, typed.clone(), near).scored(query, &host.name),
            );
            for window in &session.windows {
                let label = format!("{typed}:{}", window.index);
                let mut item = window_item(host, session, window, &place, label.clone(), near);
                item.detail = window.name.clone().into();
                let crumbs = format!("{} › {}", session.name, window.name);
                items.extend(item.scored(query, &crumbs));
                let spot = Spot {
                    host,
                    session,
                    window,
                };
                for pane in &window.panes {
                    let rest = format!("{}:{}.{}", session_ref(session), window.index, pane.id);
                    let item = pane_item(&spot, pane, &place, &rest, Some(label.clone()), near);
                    items.extend(item.scored(query, &spot.crumbs(pane)));
                }
            }
        }
    }
    items
}

pub(super) fn ranked(mut items: Vec<TargetItem>, typed: bool) -> Vec<TargetItem> {
    if typed {
        items.sort_by_key(|item| Reverse(item.score));
        let best = items.first().map_or(0, |item| item.score);
        items.retain(|item| pick::keeps(item.score, best));
    }
    items.truncate(TARGET_MAX_ROWS);
    items
}

pub(super) fn root_sections(
    hosts: &[TargetHost],
    around: &Around,
    changed: Vec<TargetItem>,
) -> Sections {
    let near = around.near(hosts);
    let everything = Query::new("");
    let agents = agent_items(&around.agents, &everything);
    let Some(here) = hosts.iter().find(|host| host.here) else {
        return vec![
            ("Agents", agents),
            ("", fleet_items(hosts, &everything, &near)),
        ];
    };
    let place = Place {
        host: here,
        written: false,
    };
    let window = spots(here).find(|spot| Some(spot.window.id) == near.window);
    let panes = window.as_ref().map_or_else(Vec::new, |spot| {
        spot.window
            .panes
            .iter()
            .filter(|pane| Some(pane.id) != around.own)
            .map(|pane| {
                let rest = format!(
                    "{}:{}.{}",
                    session_ref(spot.session),
                    spot.window.index,
                    pane.id
                );
                pane_item(spot, pane, &place, &rest, None, &near)
            })
            .collect()
    });
    let sessions = here
        .sessions
        .iter()
        .filter(|session| near.session.is_none_or(|id| id == session.id))
        .map(|session| session_item(here, session, &place, session.name.clone(), &near));
    let others = hosts.iter().filter(|host| !host.here).map(|host| {
        host_item(
            host,
            &Place {
                host,
                written: true,
            },
            &near,
        )
    });
    let files = around
        .folder
        .as_deref()
        .map(|folder| folder_item(&place, folder, &near))
        .into_iter()
        .chain(std::iter::once(home_item(&place, &near)))
        .collect();
    vec![
        ("Agents", agents),
        ("Here", panes),
        ("Changed", changed),
        ("Chats", chat_items(hosts, &everything, &near)),
        ("Places", sessions.chain(others).collect()),
        ("Files", files),
    ]
}

pub(super) fn target_items(
    query: &TargetQuery,
    hosts: &[TargetHost],
    around: &Around,
) -> Vec<TargetItem> {
    let filter = query.filter.as_str();
    let typed = !filter.is_empty();
    let near = around.near(hosts);
    let matcher = Query::new(filter);
    let host_named = |name: &str| hosts.iter().find(|host| host.name == name);
    let items: Vec<TargetItem> = match &query.level {
        TargetLevel::Top => {
            let Some(here) = hosts.iter().find(|host| host.here) else {
                let mut items = agent_items(&around.agents, &matcher);
                items.extend(fleet_items(hosts, &matcher, &near));
                return ranked(items, typed);
            };
            let place = Place {
                host: here,
                written: false,
            };
            if filter.starts_with('%') {
                panes_by_id(here, &place, &matcher, &near)
            } else if !typed {
                return root_sections(hosts, around, Vec::new())
                    .into_iter()
                    .flat_map(|(_, items)| items)
                    .collect();
            } else {
                let mut items = agent_items(&around.agents, &matcher);
                items.extend(fleet_items(hosts, &matcher, &near));
                items.extend(chat_items(hosts, &matcher, &near));
                items.extend(home_item(&place, &near).scored(&matcher, ""));
                if let Some(folder) = &around.folder {
                    items.extend(folder_item(&place, folder, &near).scored(&matcher, ""));
                }
                items
            }
        }
        TargetLevel::Sessions { host } => {
            let Some(found) = host_named(host) else {
                return Vec::new();
            };
            let place = Place {
                host: found,
                written: query.written,
            };
            if filter.starts_with('%') {
                panes_by_id(found, &place, &matcher, &near)
            } else {
                found
                    .sessions
                    .iter()
                    .filter_map(|session| {
                        session_item(found, session, &place, session.name.clone(), &near)
                            .scored(&matcher, "")
                    })
                    .chain(home_item(&place, &near).scored(&matcher, ""))
                    .collect()
            }
        }
        TargetLevel::Windows { host, session } => {
            let Some((found, session)) =
                host_named(host).and_then(|found| Some((found, find_session(found, session)?)))
            else {
                return Vec::new();
            };
            let place = Place {
                host: found,
                written: query.written,
            };
            session
                .windows
                .iter()
                .filter_map(|window| {
                    window_item(
                        found,
                        session,
                        window,
                        &place,
                        format!("{}:{}", window.index, window.name),
                        &near,
                    )
                    .scored(&matcher, "")
                })
                .collect()
        }
        TargetLevel::Panes {
            host,
            session,
            window,
        } => {
            let Some((found, session, window)) = host_named(host).and_then(|found| {
                let session = find_session(found, session)?;
                Some((found, session, find_window(session, window)?))
            }) else {
                return Vec::new();
            };
            let place = Place {
                host: found,
                written: query.written,
            };
            let spot = Spot {
                host: found,
                session,
                window,
            };
            let target = format!("{}:{}", session_ref(session), window.index);
            window
                .panes
                .iter()
                .filter_map(|pane| {
                    pane_item(
                        &spot,
                        pane,
                        &place,
                        &format!("{target}.{}", pane.id),
                        None,
                        &near,
                    )
                    .scored(&matcher, &spot.crumbs(pane))
                })
                .collect()
        }
        TargetLevel::Agents => agent_items(&around.agents, &matcher),
        TargetLevel::Files { .. } | TargetLevel::Models { .. } => return Vec::new(),
    };
    let own = around.own.map(PaletteTarget::Pane);
    let mine = |item: &TargetItem| {
        own.is_some()
            && item.target == own
            && hosts
                .iter()
                .any(|host| host.here && host.id == item.host_id)
    };
    ranked(
        items.into_iter().filter(|item| !mine(item)).collect(),
        typed,
    )
}

pub(super) fn breadcrumb(query: &TargetQuery, root: Option<&PathListRoot>) -> String {
    let parts: Vec<String> = match &query.level {
        TargetLevel::Top | TargetLevel::Agents => Vec::new(),
        TargetLevel::Models { agent } => vec![agent.clone()],
        TargetLevel::Sessions { host } => vec![host.clone()],
        TargetLevel::Windows { host, session } => vec![host.clone(), session.clone()],
        TargetLevel::Panes {
            host,
            session,
            window,
        } => vec![host.clone(), session.clone(), window.clone()],
        TargetLevel::Files { host, dir } => {
            let root = root.map_or_else(|| unescape(dir), |root| root.root.clone());
            vec![host.clone(), root]
        }
    };
    std::iter::once("@".to_owned())
        .chain(parts)
        .collect::<Vec<_>>()
        .join(" › ")
        .replacen(" › ", " ", 1)
}
