use zz_protocol::{Axis, WindowId};

use super::test_support::*;
use super::*;
use crate::mux::nav::picker_split_command;

#[test]
fn split_actions_require_an_attached_active_pane() {
    let mut snapshot = snapshot_with_two_panes();

    assert_eq!(
        active_pane_for_split(&snapshot, Some(SessionId(1))),
        Some(PaneId(202))
    );
    assert_eq!(active_pane_for_split(&snapshot, None), None);

    snapshot.sessions[0].windows.clear();
    assert_eq!(active_pane_for_split(&snapshot, Some(SessionId(1))), None);
}

#[test]
fn selecting_a_target_in_another_session_attaches_directly_to_it() {
    assert_eq!(
        activation_for_target(
            HostId::LOCAL,
            TreeTarget::Pane(PaneId(21)),
            Some(SessionId(2)),
            Some(WindowId(13)),
            HostId::LOCAL,
            Some(SessionId(1)),
            true,
        ),
        Some(SidebarActivation::AttachAt {
            host: HostId::LOCAL,
            target: "$2:@13.%21".to_owned(),
        })
    );
    assert_eq!(
        activation_for_target(
            HostId::LOCAL,
            TreeTarget::Window(WindowId(13)),
            Some(SessionId(2)),
            Some(WindowId(13)),
            HostId::LOCAL,
            None,
            true,
        ),
        Some(SidebarActivation::AttachAt {
            host: HostId::LOCAL,
            target: "$2:@13".to_owned(),
        })
    );
}

#[test]
fn clicking_a_pane_in_an_inactive_window_selects_its_window_and_pane() {
    let mut engine = zz_mux::MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    for command in [
        CommandInvocation::new("new-session", ["-s", "work", "-n", "A"]),
        CommandInvocation::new("new-window", ["-n", "B"]),
    ] {
        engine.execute(&mut context, &command).unwrap();
    }
    let session = context.session.unwrap();
    let target_window = context.window.unwrap();
    let target_pane = context.pane.unwrap();
    for command in [
        CommandInvocation::new("split-window", ["-h"]),
        CommandInvocation::new("new-window", ["-n", "C"]),
    ] {
        engine.execute(&mut context, &command).unwrap();
    }
    assert_ne!(context.window, Some(target_window));
    assert_ne!(
        engine.state.windows[&target_window].active_pane,
        target_pane
    );

    let snapshot = engine.state.snapshot();
    let model = local_model(&snapshot, Some(session));
    let activation = model.activation_for_node(
        TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(target_pane)),
        HostId::LOCAL,
        Some(session),
    );
    let Some(SidebarActivation::Execute { commands, .. }) = activation else {
        panic!("a pane in this session must be directly selectable");
    };
    for command in commands {
        engine.execute(&mut context, &command).unwrap();
    }

    assert_eq!(engine.state.sessions[&session].active_window, target_window);
    assert_eq!(
        engine.state.windows[&target_window].active_pane,
        target_pane
    );
    assert_eq!(context.window, Some(target_window));
    assert_eq!(context.pane, Some(target_pane));
}

#[test]
fn sidebar_commands_use_stable_mux_targets() {
    assert_eq!(
        new_window_command(SessionId(8)),
        CommandInvocation::new("new-window", ["-t", "$8"])
    );
    assert_eq!(
        select_window_command(WindowId(13)),
        CommandInvocation::new("select-window", ["-t", "@13"])
    );
    assert_eq!(
        select_pane_commands(PaneId(21)),
        vec![
            CommandInvocation::new("select-window", ["-t", "%21"]),
            CommandInvocation::new("select-pane", ["-t", "%21"]),
        ]
    );
    assert_eq!(
        kill_target_command(TreeTarget::Session(SessionId(8))),
        CommandInvocation::new("kill-session", ["-t", "$8"])
    );
    assert_eq!(
        kill_target_command(TreeTarget::Window(WindowId(13))),
        CommandInvocation::new("kill-window", ["-t", "@13"])
    );
    assert_eq!(
        kill_target_command(TreeTarget::Pane(PaneId(21))),
        CommandInvocation::new("kill-pane", ["-t", "%21"])
    );
    assert_eq!(
        picker_split_command(PaneId(21), Axis::Horizontal),
        CommandInvocation::new("split-window", ["--kind", "picker", "-h", "-t", "%21"])
    );
    assert_eq!(
        picker_split_command(PaneId(21), Axis::Vertical),
        CommandInvocation::new("split-window", ["--kind", "picker", "-v", "-t", "%21"])
    );
}

#[test]
fn rename_prompts_prefill_exact_names_and_keep_stable_targets() {
    let (session_label, session_command) =
        rename_prompt_command(TreeTarget::Session(SessionId(8)), "main workspace")
            .expect("sessions are renameable");
    assert_eq!(session_label, "Rename Session…");
    assert_eq!(
        session_command,
        CommandInvocation::new(
            "command-prompt",
            [
                "-p",
                "rename-session: ",
                "-I",
                "main workspace",
                "rename-session -t '$8' -- '%%'",
            ],
        )
    );

    let (window_label, window_command) =
        rename_prompt_command(TreeTarget::Window(WindowId(13)), "editor pane")
            .expect("windows are renameable");
    assert_eq!(window_label, "Rename Window…");
    assert_eq!(
        window_command,
        CommandInvocation::new(
            "command-prompt",
            [
                "-p",
                "rename-window: ",
                "-I",
                "editor pane",
                "rename-window -t '@13' -- '%%'",
            ],
        )
    );
    assert!(rename_prompt_command(TreeTarget::Pane(PaneId(21)), "shell").is_none());

    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    assert_eq!(
        model.renameable_name(HostId::LOCAL, TreeTarget::Session(SessionId(1))),
        Some("main workspace")
    );
    assert_eq!(
        model.renameable_name(HostId::LOCAL, TreeTarget::Window(WindowId(11))),
        Some("work")
    );
    assert_eq!(
        model.renameable_name(HostId::LOCAL, TreeTarget::Pane(PaneId(202))),
        None
    );
    assert_eq!(
        model.rename_target_for_node(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Session(SessionId(1))
        )),
        Some((HostId::LOCAL, TreeTarget::Session(SessionId(1))))
    );
    assert_eq!(
        model.rename_target_for_node(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Window(WindowId(11))
        )),
        Some((HostId::LOCAL, TreeTarget::Window(WindowId(11))))
    );
    assert_eq!(
        model.rename_target_for_node(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Pane(PaneId(202))
        )),
        Some((HostId::LOCAL, TreeTarget::Window(WindowId(11))))
    );
    assert_eq!(
        model.rename_target_for_node(TreeNode::Host(HostId::LOCAL)),
        None
    );
}
