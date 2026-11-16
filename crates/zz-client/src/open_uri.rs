use zz_protocol::{LayoutNode, MuxSnapshot, PaneId, PaneKindSnapshot, SessionId, WindowSnapshot};

use crate::{terminal_view::pasted_image_number, url_input::normalize_url};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenUriRoute {
    Embedded { pane: PaneId, url: String },
    PastedImage { pane: PaneId, number: u32 },
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BrowserTargetScore {
    tree_distance: usize,
    layout_distance: usize,
    before_source: bool,
    layout_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BrowserTarget {
    score: BrowserTargetScore,
    pane: PaneId,
}

#[must_use]
pub fn open_uri_route(
    snapshot: &MuxSnapshot,
    attached_session: Option<SessionId>,
    source: PaneId,
    uri: &str,
) -> OpenUriRoute {
    if let Some(number) = pasted_image_number(uri) {
        return OpenUriRoute::PastedImage {
            pane: source,
            number,
        };
    }
    let Ok(url) = normalize_url(uri) else {
        return OpenUriRoute::External;
    };
    if url.starts_with("file:") {
        return OpenUriRoute::External;
    }
    let Some(pane) = embedded_browser_target(snapshot, attached_session, source) else {
        return OpenUriRoute::External;
    };
    OpenUriRoute::Embedded { pane, url }
}

fn embedded_browser_target(
    snapshot: &MuxSnapshot,
    attached_session: Option<SessionId>,
    source: PaneId,
) -> Option<PaneId> {
    let session = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached_session)?;
    let window = session
        .windows
        .iter()
        .find(|window| window.layout.contains(source))?;
    nearest_browser_in_window(window, source)
}

fn nearest_browser_in_window(window: &WindowSnapshot, source: PaneId) -> Option<PaneId> {
    let mut source_path = Vec::new();
    let mut next_index = 0;
    let (source_path, source_index) =
        pane_path_and_index(&window.layout, source, &mut source_path, &mut next_index)?;

    let mut path = Vec::new();
    let mut next_index = 0;
    let mut best = None;
    visit_browser_panes(
        &window.layout,
        window,
        &source_path,
        source_index,
        &mut path,
        &mut next_index,
        &mut best,
    );
    best.map(|candidate| candidate.pane)
}

fn pane_path_and_index(
    node: &LayoutNode,
    target: PaneId,
    path: &mut Vec<bool>,
    next_index: &mut usize,
) -> Option<(Vec<bool>, usize)> {
    match node {
        LayoutNode::Pane(pane) => {
            let index = *next_index;
            *next_index += 1;
            (*pane == target).then(|| (path.clone(), index))
        }
        LayoutNode::Empty => None,
        LayoutNode::Split { first, second, .. } => {
            path.push(false);
            let found = pane_path_and_index(first, target, path, next_index);
            path.pop();
            if found.is_some() {
                return found;
            }
            path.push(true);
            let found = pane_path_and_index(second, target, path, next_index);
            path.pop();
            found
        }
    }
}

fn visit_browser_panes(
    node: &LayoutNode,
    window: &WindowSnapshot,
    source_path: &[bool],
    source_index: usize,
    path: &mut Vec<bool>,
    next_index: &mut usize,
    best: &mut Option<BrowserTarget>,
) {
    match node {
        LayoutNode::Pane(pane) => {
            let index = *next_index;
            *next_index += 1;
            if !matches!(
                window.panes.get(pane).map(|pane| &pane.kind),
                Some(PaneKindSnapshot::Browser(_))
            ) {
                return;
            }
            let shared_depth = source_path
                .iter()
                .zip(path.iter())
                .take_while(|(source, candidate)| source == candidate)
                .count();
            let tree_distance = source_path.len() + path.len() - 2 * shared_depth;
            let score = BrowserTargetScore {
                tree_distance,
                layout_distance: index.abs_diff(source_index),
                before_source: index < source_index,
                layout_index: index,
            };
            if best
                .as_ref()
                .is_none_or(|candidate| score < candidate.score)
            {
                *best = Some(BrowserTarget { score, pane: *pane });
            }
        }
        LayoutNode::Empty => {}
        LayoutNode::Split { first, second, .. } => {
            path.push(false);
            visit_browser_panes(
                first,
                window,
                source_path,
                source_index,
                path,
                next_index,
                best,
            );
            path.pop();
            path.push(true);
            visit_browser_panes(
                second,
                window,
                source_path,
                source_index,
                path,
                next_index,
                best,
            );
            path.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use zz_protocol::{
        Axis, BrowserDescriptor, PaneSnapshot, PaneSummary, SessionSnapshot, SplitId, WindowId,
    };

    use super::*;

    fn routing_pane(id: u64, browser: bool) -> PaneSnapshot {
        let id = PaneId(id);
        PaneSnapshot {
            id,
            title: id.to_string(),
            kind: if browser {
                PaneKindSnapshot::Browser(BrowserDescriptor::single(
                    "about:blank".to_owned(),
                    "zz-default".to_owned(),
                ))
            } else {
                PaneKindSnapshot::Terminal
            },
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
            border_colour: None,
            active_border_colour: None,
            border_status_text: String::new(),
            mode: None,
            status: None,
            summary: PaneSummary::default(),
        }
    }

    fn routing_window(id: u64, layout: LayoutNode, panes: &[PaneSnapshot]) -> WindowSnapshot {
        let panes = panes
            .iter()
            .cloned()
            .map(|pane| (pane.id, pane))
            .collect::<BTreeMap<_, _>>();
        WindowSnapshot {
            id: WindowId(id),
            index: u32::try_from(id).expect("small fixture window id"),
            name: format!("window-{id}"),
            automatic_rename: true,
            active_pane: panes.keys().next().copied().expect("fixture pane"),
            zoomed_pane: None,
            layout,
            panes,
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: Vec::new(),
            pane_z_order: Vec::new(),
            floating: Vec::new(),
            modal: None,
            sx: 0,
            sy: 0,
            pane_base_index: 0,
        }
    }

    fn routing_snapshot(windows: Vec<WindowSnapshot>) -> MuxSnapshot {
        MuxSnapshot {
            generation: 1,
            focused_window: None,
            sessions: vec![SessionSnapshot {
                id: SessionId(7),
                name: "routing".to_owned(),
                active_window: windows.first().expect("fixture window").id,
                windows,
                viewers: Vec::new(),
            }],
        }
    }

    #[test]
    fn web_uri_routes_to_topologically_nearest_browser_in_the_source_window() {
        let layout = LayoutNode::Split {
            id: SplitId(10),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                id: SplitId(11),
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(PaneId(1))),
                second: Box::new(LayoutNode::Pane(PaneId(2))),
            }),
            second: Box::new(LayoutNode::Split {
                id: SplitId(12),
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(PaneId(3))),
                second: Box::new(LayoutNode::Pane(PaneId(4))),
            }),
        };
        let window = routing_window(
            1,
            layout,
            &[
                routing_pane(1, true),
                routing_pane(2, false),
                routing_pane(3, true),
                routing_pane(4, false),
            ],
        );
        let snapshot = routing_snapshot(vec![window]);

        assert_eq!(
            open_uri_route(
                &snapshot,
                Some(SessionId(7)),
                PaneId(2),
                "https://example.com/docs?q=1"
            ),
            OpenUriRoute::Embedded {
                pane: PaneId(1),
                url: "https://example.com/docs?q=1".to_owned(),
            }
        );
    }

    #[test]
    fn uri_routing_falls_back_for_other_windows_and_unsupported_schemes() {
        let terminal_window =
            routing_window(1, LayoutNode::Pane(PaneId(2)), &[routing_pane(2, false)]);
        let browser_window =
            routing_window(2, LayoutNode::Pane(PaneId(3)), &[routing_pane(3, true)]);
        let snapshot = routing_snapshot(vec![terminal_window, browser_window]);

        assert_eq!(
            open_uri_route(
                &snapshot,
                Some(SessionId(7)),
                PaneId(2),
                "https://example.com"
            ),
            OpenUriRoute::External
        );

        let side_by_side = routing_window(
            3,
            LayoutNode::Split {
                id: SplitId(13),
                axis: Axis::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(PaneId(2))),
                second: Box::new(LayoutNode::Pane(PaneId(3))),
            },
            &[routing_pane(2, false), routing_pane(3, true)],
        );
        let snapshot = routing_snapshot(vec![side_by_side]);
        assert_eq!(
            open_uri_route(
                &snapshot,
                Some(SessionId(7)),
                PaneId(2),
                "mailto:hello@example.com"
            ),
            OpenUriRoute::External
        );
    }

    #[test]
    fn image_placeholders_route_back_to_their_own_pane() {
        let snapshot = routing_snapshot(vec![routing_window(
            1,
            LayoutNode::Split {
                id: SplitId(11),
                axis: Axis::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(PaneId(1))),
                second: Box::new(LayoutNode::Pane(PaneId(2))),
            },
            &[routing_pane(1, true), routing_pane(2, false)],
        )]);

        assert_eq!(
            open_uri_route(&snapshot, Some(SessionId(7)), PaneId(2), "zz-image://3"),
            OpenUriRoute::PastedImage {
                pane: PaneId(2),
                number: 3,
            }
        );
        for uri in ["zz-image://", "zz-image://x", "zz-image:3", "https://3"] {
            assert!(
                !matches!(
                    open_uri_route(&snapshot, Some(SessionId(7)), PaneId(2), uri),
                    OpenUriRoute::PastedImage { .. }
                ),
                "{uri} is not an image placeholder"
            );
        }
    }
}
