use zz_protocol::{CommandInvocation, PaneId, SessionId, WindowId};

use super::*;
use crate::{
    gui_actions::{PANE_GAPS, THEME},
    place::{FleetHost, FleetPane, FleetSession, FleetWindow},
};

fn fleet() -> Fleet {
    let pane = |id: u64, index: u32| FleetPane {
        id: PaneId(id),
        index,
        title: String::new(),
        program: Some("zsh".to_owned()),
        cwd: "/home/fabrico".to_owned(),
        terminal: true,
        needs_you: false,
    };
    Fleet {
        here: 0,
        hosts: vec![FleetHost {
            id: 0,
            name: "macbook".to_owned(),
            online: true,
            current: Some(SessionId(3)),
            sessions: vec![FleetSession {
                id: SessionId(3),
                name: "foo".to_owned(),
                label: "foo".to_owned(),
                windows: vec![FleetWindow {
                    id: WindowId(10),
                    index: 1,
                    name: "editor".to_owned(),
                    active: true,
                    active_pane: PaneId(20),
                    panes: vec![pane(20, 1), pane(21, 2)],
                }],
            }],
        }],
    }
}

fn pane_place() -> Place {
    Place {
        host: Some(0),
        session: Some(SessionId(3)),
        window: Some(WindowId(10)),
        pane: Some(PaneId(21)),
    }
}

fn all() -> Vec<Verb> {
    verbs(
        PaneKindAvailability::default(),
        &[PANE_GAPS, THEME],
        true,
        "",
    )
}

fn named<'a>(verbs: &'a [Verb], name: &str) -> &'a Verb {
    verbs
        .iter()
        .find(|verb| verb.name == name)
        .unwrap_or_else(|| panic!("no verb {name}"))
}

#[test]
fn zz_names_come_first_and_tmux_commands_read_with_spaces() {
    let verbs = all();
    assert_eq!(verbs[0].name, "read pane");
    assert_eq!(named(&verbs, "read pane").spelled(), "capture-pane -p");
    assert_eq!(
        named(&verbs, "read pane").slots,
        vec![Slot::Target(Level::Pane)]
    );
    let kill = named(&verbs, "kill window");
    assert_eq!(kill.spelled(), "kill-window");
    assert_eq!(kill.slots, vec![Slot::Target(Level::Window)]);
    let rename = named(&verbs, "rename window");
    assert_eq!(
        rename.slots,
        vec![
            Slot::Target(Level::Window),
            Slot::Text("new name".to_owned())
        ]
    );
    assert_eq!(
        named(&verbs, "new window").slots,
        vec![Slot::Target(Level::Session)]
    );
    assert!(named(&verbs, "new session").slots.is_empty());
    assert_eq!(
        verbs.iter().filter(|verb| verb.name == "send keys").count(),
        1
    );
}

#[test]
fn gui_actions_and_inserting_a_path_are_verbs_too() {
    let verbs = all();
    let theme = named(&verbs, "select theme");
    assert_eq!(theme.kind, VerbKind::Gui);
    assert_eq!(theme.slots, vec![Slot::Value(THEME)]);
    assert_eq!(theme.spelled(), "gui");
    assert_eq!(named(&verbs, "insert path").slots, vec![Slot::Path]);
    let without = super::verbs(PaneKindAvailability::default(), &[], false, "");
    assert!(without.iter().all(|verb| verb.kind == VerbKind::Tmux));
}

#[test]
fn a_command_alias_is_a_verb_that_keeps_its_commands_slots() {
    let list = "split-pane=split-window\nzoom=resize-pane -Z\npin=select-pane -t %3\n";
    let aliases = alias_verbs(list, PaneKindAvailability::default());
    assert_eq!(
        aliases
            .iter()
            .map(|verb| verb.name.as_str())
            .collect::<Vec<_>>(),
        ["zoom", "pin"]
    );
    assert_eq!(aliases[0].spelled(), "resize-pane -Z");
    assert_eq!(aliases[0].slots, vec![Slot::Target(Level::Pane)]);
    assert!(aliases[1].slots.is_empty(), "the alias names its target");
    assert!(
        alias_verbs(
            "broken\n=kill-pane\nx=not-a-command",
            PaneKindAvailability::default()
        )
        .is_empty()
    );
}

#[test]
fn typing_matches_names_before_keywords() {
    let verbs = all();
    let found = match_verbs(&verbs, "rea", |_| true);
    assert_eq!(verbs[found[0].index].name, "read pane");
    let found = match_verbs(&verbs, "capture", |_| true);
    assert_eq!(verbs[found[0].index].name, "capture pane");
    assert!(
        found
            .iter()
            .any(|found| verbs[found.index].name == "read pane" && found.score >= KEYWORD_PENALTY)
    );
    let on_a_pane = match_verbs(&verbs, "kil", |verb| verb.target().is_some());
    assert!(
        on_a_pane
            .iter()
            .all(|found| verbs[found.index].target().is_some())
    );
}

#[test]
fn a_filled_verb_compiles_with_ids_and_reads_with_names() {
    let verbs = all();
    let fleet = fleet();
    let read = named(&verbs, "read pane");
    assert_eq!(
        compile(read, &[Filled::Place(pane_place())], &fleet),
        Some(Compiled {
            host: Some(0),
            line: "capture-pane -p -t %21".to_owned(),
            display: "capture-pane -p -t foo:1.2".to_owned(),
            invocation: Some(CommandInvocation::new("capture-pane", ["-p", "-t", "%21"])),
        })
    );
    let rename = named(&verbs, "rename window");
    let window = Place {
        pane: None,
        ..pane_place()
    };
    assert_eq!(
        compile(
            rename,
            &[Filled::Place(window), Filled::Text("api logs".to_owned())],
            &fleet
        )
        .map(|compiled| (compiled.line, compiled.display)),
        Some((
            "rename-window -t @10 'api logs'".to_owned(),
            "rename-window -t foo:1 'api logs'".to_owned(),
        ))
    );
    assert_eq!(compile(rename, &[Filled::Place(window)], &fleet), None);
    let session = Place {
        window: None,
        pane: None,
        ..pane_place()
    };
    assert_eq!(
        compile(
            named(&verbs, "new window"),
            &[Filled::Place(session)],
            &fleet
        )
        .map(|compiled| compiled.line),
        Some("new-window -t '$3'".to_owned())
    );
    assert_eq!(compile(named(&verbs, "select theme"), &[], &fleet), None);
}

#[test]
fn a_deeper_place_targets_the_level_the_verb_takes() {
    let verbs = all();
    let fleet = fleet();
    assert_eq!(
        compile(
            named(&verbs, "kill session"),
            &[Filled::Place(pane_place())],
            &fleet
        )
        .map(|compiled| (compiled.line, compiled.display)),
        Some((
            "kill-session -t '$3'".to_owned(),
            "kill-session -t foo".to_owned()
        ))
    );
    assert_eq!(
        compile(
            named(&verbs, "kill window"),
            &[Filled::Place(pane_place())],
            &fleet
        )
        .map(|compiled| compiled.display),
        Some("kill-window -t foo:1".to_owned())
    );
}

#[test]
fn quoting_keeps_one_argument_whole() {
    assert_eq!(quote("foo:1.2"), "foo:1.2");
    assert_eq!(quote("api logs"), "'api logs'");
    assert_eq!(quote("it's $HOME"), "\"it's \\$HOME\"");
    assert_eq!(quote(""), "''");
}

#[test]
fn going_to_a_place_reads_as_the_tmux_command_it_amounts_to() {
    let fleet = fleet();
    assert_eq!(
        go_to_line(&fleet, &pane_place()).as_deref(),
        Some("select-pane -t foo:1.2")
    );
    let window = Place {
        pane: None,
        ..pane_place()
    };
    assert_eq!(
        go_to_line(&fleet, &window).as_deref(),
        Some("select-window -t foo:1")
    );
    assert_eq!(go_to_line(&fleet, &Place::host(0)), None);
}
