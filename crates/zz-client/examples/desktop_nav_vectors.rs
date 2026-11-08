use serde_json::{Value, json};
use std::{error::Error, fs, path::PathBuf};
use zz_client::desktop_nav::{
    NavActivation, NavTarget, activation_for_target, detach_command, kill_target_command,
    new_session_commands, new_window_command, select_pane_commands, select_window_command,
};
use zz_protocol::{
    Axis, CommandInvocation, CommandRequest, NEW_SESSION_ATTACH_CAPABILITY, PaneId,
    ProtocolMessage, SessionId, WindowId, encode_protocol_message,
};

fn frames(commands: &[CommandInvocation]) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    commands
        .iter()
        .map(|command| {
            Ok(encode_protocol_message(&ProtocolMessage::CommandRequest(
                CommandRequest {
                    request_id: 0,
                    command: command.clone(),
                    prepared: false,
                },
            ))?)
        })
        .collect()
}

fn activation(value: Option<NavActivation<u32>>) -> Result<Value, Box<dyn Error>> {
    let Some(value) = value else {
        return Ok(Value::Null);
    };
    let (kind, host, session, target, commands) = match value {
        NavActivation::AttachHost(host) => (0, host, None, String::new(), Vec::new()),
        NavActivation::Attach { host, session } => {
            (1, host, Some(session), String::new(), Vec::new())
        }
        NavActivation::AttachAt { host, target } => (2, host, None, target, Vec::new()),
        NavActivation::Execute { host, commands } => (3, host, None, String::new(), commands),
        NavActivation::AttachThenExecute {
            host,
            session,
            commands,
        } => (4, host, Some(session), String::new(), commands),
        NavActivation::Reconnect(host) => (5, host, None, String::new(), Vec::new()),
    };
    Ok(
        json!({"kind":kind,"host":host,"session":session.map(|session|session.0),"target":target,"frames":frames(&commands)?}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for host in [0, 17, u32::MAX] {
        for attached_host in [0, 17] {
            for kind in 0..3_u8 {
                for id in [0, 7, u64::MAX] {
                    let target = match kind {
                        0 => NavTarget::Session(SessionId(id)),
                        1 => NavTarget::Window(WindowId(id)),
                        _ => NavTarget::Pane(PaneId(id)),
                    };
                    for owner_session in [
                        None,
                        Some(SessionId(0)),
                        Some(SessionId(1)),
                        Some(SessionId(u64::MAX)),
                    ] {
                        for owner_window in [
                            None,
                            Some(WindowId(0)),
                            Some(WindowId(9)),
                            Some(WindowId(u64::MAX)),
                        ] {
                            for attached_session in [
                                None,
                                Some(SessionId(0)),
                                Some(SessionId(1)),
                                Some(SessionId(u64::MAX)),
                            ] {
                                for connected in [false, true] {
                                    let value = activation_for_target(
                                        host,
                                        target,
                                        owner_session,
                                        owner_window,
                                        attached_host,
                                        attached_session,
                                        connected,
                                    );
                                    cases.push(json!({"host":host,"kind":kind,"id":id,"owner_session":owner_session.map(|id|id.0),"owner_window":owner_window.map(|id|id.0),"attached_host":attached_host,"attached_session":attached_session.map(|id|id.0),"connected":connected,"value":activation(value)?}));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut paths = vec![
        None,
        Some(PathBuf::new()),
        Some(PathBuf::from("/tmp/work")),
        Some(PathBuf::from("a b")),
        Some(PathBuf::from("'quoted'")),
        Some(PathBuf::from("λ/中")),
    ];
    paths.push(Some(PathBuf::from("/tmp/\n")));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        paths.push(Some(PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b't', 255, 195, 169,
        ]))));
    }
    let mut sessions = Vec::new();
    for local in [false, true] {
        for capabilities in [
            Vec::new(),
            vec![NEW_SESSION_ATTACH_CAPABILITY.to_owned()],
            vec!["NEW-SESSION-ATTACH-V1".to_owned()],
            vec![
                "mux-v1".to_owned(),
                NEW_SESSION_ATTACH_CAPABILITY.to_owned(),
                NEW_SESSION_ATTACH_CAPABILITY.to_owned(),
            ],
        ] {
            for directory in &paths {
                sessions.push(json!({"local":local,"capabilities":capabilities,"directory":directory.as_ref().map(|path|path.as_os_str().as_encoded_bytes()),"lossy":directory.as_ref().map(|path|path.to_string_lossy()),"frames":frames(&new_session_commands(local, &capabilities, directory.as_deref()))?}));
            }
        }
    }
    let mut commands = Vec::new();
    for id in [0, 1, u64::MAX] {
        commands
            .push(json!({"kind":0,"id":id,"frames":frames(&[new_window_command(SessionId(id))])?}));
        commands.push(
            json!({"kind":1,"id":id,"frames":frames(&[select_window_command(WindowId(id))])?}),
        );
        commands
            .push(json!({"kind":2,"id":id,"frames":frames(&select_pane_commands(PaneId(id)))?}));
        for (kind, target) in [
            (3, NavTarget::Session(SessionId(id))),
            (4, NavTarget::Window(WindowId(id))),
            (5, NavTarget::Pane(PaneId(id))),
        ] {
            commands.push(
                json!({"kind":kind,"id":id,"frames":frames(&[kill_target_command(target)])?}),
            );
        }
        for (kind, axis) in [(6, Axis::Horizontal), (7, Axis::Vertical)] {
            commands.push(json!({"kind":kind,"id":id,"frames":frames(&[zz_client::picker_split_command(PaneId(id),axis)])?}));
        }
    }
    commands.push(json!({"kind":8,"id":0,"frames":frames(&[detach_command()])?}));
    fs::write(
        std::env::args().nth(1).ok_or("missing output")?,
        serde_json::to_vec(&json!({"activations":cases,"sessions":sessions,"commands":commands}))?,
    )?;
    Ok(())
}
