use std::{collections::BTreeSet, error::Error};

use serde_json::{Value, json};
use zz_protocol::{
    ClientMessageKind, Event, EventPayload, ModePresentation, PaneBorderPresentation, PaneId,
    ProtocolMessage, StatusLine, StatusPosition, ThemeColours, TmuxColour, decode_protocol_frame,
    encode_protocol_message, parse_style,
};

fn frame(payload: EventPayload) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: u64::MAX,
        payload,
    }))?)
}

fn unchecked(payload: EventPayload) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(&ProtocolMessage::Event(Event {
        sequence: 1,
        payload,
    }))?);
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

fn status() -> StatusLine {
    StatusLine {
        left: "#[fg=themegreen]left 世界".to_owned(),
        right: "right 🦀".to_owned(),
        title: "title\0\t世界".to_owned(),
        base_style: "fg=default,bg=colour235,bold".to_owned(),
        rows: vec![
            "row 0".to_owned(),
            "row 1".to_owned(),
            String::new(),
            "row 3".to_owned(),
            "row 4".to_owned(),
        ],
        position: StatusPosition::Top,
        message_line: 4,
        customized: true,
        theme: ThemeColours(std::array::from_fn(|i| match i % 5 {
            0 => TmuxColour::Basic(255),
            1 => TmuxColour::Indexed(255),
            2 => TmuxColour::Rgb(0x00ff_ffff),
            3 => TmuxColour::Default,
            _ => TmuxColour::Terminal,
        })),
        message_style: "fg=red,reverse".to_owned(),
        message_command_style: "fg=green".to_owned(),
        modes: vec![
            ModePresentation {
                pane: PaneId(u64::MAX),
                view: false,
                position: "0/123 世界".to_owned(),
                position_style: "align=right,fg=red".to_owned(),
                selection_style: "bg=navyblue".to_owned(),
                vi_keys: true,
                match_style: "fg=#aabbcc".to_owned(),
                current_match_style: "bold,reverse".to_owned(),
                line_numbers: 255,
                line_number_style: "opaque line numbers".to_owned(),
                current_line_number_style: "#[not-a-style]".to_owned(),
                current_line_style: "not-a-style".to_owned(),
            },
            ModePresentation {
                pane: PaneId(0),
                view: true,
                position: String::new(),
                position_style: String::new(),
                selection_style: String::new(),
                vi_keys: false,
                match_style: String::new(),
                current_match_style: String::new(),
                line_numbers: 0,
                line_number_style: String::new(),
                current_line_number_style: String::new(),
                current_line_style: String::new(),
            },
        ],
        pane_borders: (0..256)
            .map(|id| PaneBorderPresentation {
                pane: PaneId(id),
                style: "fg=colour+1,bold".to_owned(),
                window_style: "bg=default".to_owned(),
                window_active_style: "bg=red,dim=5%,width=5%%,range=user|text".to_owned(),
            })
            .collect(),
    }
}

fn case(name: &str, payload: EventPayload) -> Result<Value, Box<dyn Error>> {
    Ok(json!({"name":name,"bytes":frame(payload)?}))
}

fn rejected(name: &str, payload: EventPayload) -> Result<Value, Box<dyn Error>> {
    let bytes = unchecked(payload)?;
    match decode_protocol_frame(&bytes) {
        Err(error) => Ok(json!({"name":name,"bytes":bytes,"error":error.to_string()})),
        Ok(_) => Err(format!("malformed reference unexpectedly accepted: {name}").into()),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = vec![
        case(
            "default-status",
            EventPayload::StatusChanged {
                status: StatusLine::default(),
            },
        )?,
        case(
            "full-status-metadata-and-bounds",
            EventPayload::StatusChanged { status: status() },
        )?,
    ];
    let mut edge = status();
    edge.left = "é".repeat(2048);
    edge.rows = vec!["x".repeat(4096)];
    edge.message_line = 0;
    cases.push(case(
        "status-exact-text-byte-bound",
        EventPayload::StatusChanged { status: edge },
    )?);
    for kind in [
        ClientMessageKind::Info,
        ClientMessageKind::Success,
        ClientMessageKind::Warning,
        ClientMessageKind::Error,
    ] {
        cases.push(case(
            &format!("client-message-{kind:?}"),
            EventPayload::ClientMessage {
                pane: None,
                kind,
                text: "hello\0 世界 🦀".to_owned(),
            },
        )?);
        for (duration_ms, message_id) in [(0, 0), (u32::MAX, u64::MAX)] {
            cases.push(case(
                &format!("timed-message-{kind:?}-{duration_ms}"),
                EventPayload::TimedClientMessage {
                    pane: Some(PaneId(u64::MAX)),
                    kind,
                    text: "timed message 世界".to_owned(),
                    duration_ms,
                    message_id,
                },
            )?);
        }
    }
    for message_id in [0, u64::MAX] {
        cases.push(case(
            &format!("message-clear-{message_id}"),
            EventPayload::TimedClientMessageCleared { message_id },
        )?);
    }
    for output_id in [0, 1, u64::MAX] {
        cases.push(case(
            &format!("output-close-{output_id}"),
            EventPayload::CommandOutput {
                pane: PaneId(u64::MAX),
                output_id,
                viewport: None,
            },
        )?);
    }
    cases.push(case(
        "pane-removed",
        EventPayload::PaneRemoved(PaneId(u64::MAX)),
    )?);
    cases.push(case(
        "terminal-negotiation",
        EventPayload::TerminalNegotiation {
            features: vec!["RGB".to_owned(), "cstyle".to_owned(), "世界".to_owned()],
            user_keys: vec!["\x1b[99~".to_owned(), "\0\t🦀".to_owned()],
            application_escape: vec!["Escape".to_owned(), "C-[".to_owned()],
        },
    )?);
    cases.push(case(
        "empty-terminal-negotiation",
        EventPayload::TerminalNegotiation {
            features: vec![],
            user_keys: vec![],
            application_escape: vec![],
        },
    )?);
    let mut rejections = Vec::new();
    for (name, change) in [
        ("left-over-bound", 0),
        ("rows-over-bound", 1),
        ("message-line-without-rows", 2),
        ("message-line-over-rows", 3),
        ("invalid-base-style", 4),
        ("circular-theme", 5),
        ("invalid-message-style", 6),
        ("modes-over-bound", 7),
        ("invalid-mode-style", 8),
        ("borders-over-bound", 9),
        ("invalid-border-style", 10),
    ] {
        let mut value = status();
        match change {
            0 => value.left = "x".repeat(4097),
            1 => value.rows.push(String::new()),
            2 => {
                value.rows.clear();
                value.message_line = 1;
            }
            3 => value.message_line = 5,
            4 => "not-a-style".clone_into(&mut value.base_style),
            5 => value.theme.0[0] = TmuxColour::Theme(0),
            6 => "fg=red|blue".clone_into(&mut value.message_style),
            7 => value.modes.push(value.modes[0].clone()),
            8 => "fg=invalid-colour".clone_into(&mut value.modes[0].position_style),
            9 => value.pane_borders.push(value.pane_borders[0].clone()),
            _ => "align=left|centre".clone_into(&mut value.pane_borders[0].window_style),
        }
        rejections.push(rejected(
            name,
            EventPayload::StatusChanged { status: value },
        )?);
    }
    let mut styles = BTreeSet::new();
    for row in include_str!("../../zz-terminal/src/x11-rgb.txt").lines() {
        let fields = row.split_whitespace().collect::<Vec<_>>();
        if fields.len() > 3 && fields[..3].iter().all(|v| v.parse::<u8>().is_ok()) {
            styles.insert(format!("fg={}", fields[3..].join(" ")));
        }
    }
    for value in [
        "",
        "default",
        "none",
        "noattr",
        "nodefault",
        "noignore",
        "push-default,pop-default,set-default",
        "nolist,norange,noalign,nolink",
        "fg=#ABCDEF,bg=colour255,us=terminal,fill=themegreen",
        "fg=colour+1",
        "fg=colour256",
        "fg=red|blue",
        "align=left|centre",
        "fg=#123",
        "fg=#1234567",
        "fg=red\t",
        "bold\t",
        "bold|underscore",
        "nobold|underscore",
        "no",
        "range=left",
        "range=left|value",
        "range=pane|%+12",
        "range=session|$18446744073709551615",
        "range=window|18446744073709551616",
        "range=control|+9",
        "range=control|10",
        "range=user|hello|world",
        "range=unknown",
        "range=unknown|value",
        "range=",
        "dim=100%",
        "dim=101",
        "dim=5%%",
        "width=5%%",
        "width=5%%%",
        "width=+4294967295",
        "width=4294967296",
        "pad=+0",
        "pad=-0",
        "fg=LightGoldenrodYellow",
        "fg=light goldenrod yellow",
        "fg=rgb:aa/bb/cc",
        "fg=\u{212a}haki",
        "align=absolute-centre,list=right-marker",
        "link=\tvalue",
        "fg=blue\nbold",
    ] {
        styles.insert(value.to_owned());
    }
    styles.insert(format!("link={}", "x".repeat(250)));
    styles.insert(format!("link={}", "x".repeat(251)));
    let styles = styles
        .into_iter()
        .map(|style| json!({"value":style,"valid":parse_style(&style).is_some()}))
        .collect::<Vec<_>>();
    println!(
        "{}",
        json!({"reference":"actual zz-protocol client-event/status encoder, decoder and style parser","cases":cases,"rejections":rejections,"styles":styles})
    );
    Ok(())
}
