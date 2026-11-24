use zz_protocol::StdoutClaim;

use super::*;

fn ran(exit_code: u8, stdout: &str, stderr: &str) -> CommandOutcome {
    CommandOutcome {
        stdout: stdout.into(),
        stderr: stderr.to_owned(),
        exit_code,
        stdout_claim: StdoutClaim::default(),
        client_exit: false,
    }
}

#[test]
fn a_remote_turn_becomes_the_message_the_calling_chat_reads() {
    let pane = PaneId(5);
    let report = |ran| {
        RemoteReport {
            host: "macbook".to_owned(),
            report: relayed_report("macbook", pane, AgentProvider::ClaudeCode, ran),
        }
        .to_string()
    };
    let follow = "If this answers the user, answer the user now and send nothing to macbook:%5. Only if something is missing: zz --host macbook agent-send -t %5 'TEXT'";
    let again = "Tell the user, or try again with: zz --host macbook agent-send -t %5 'TEXT'";
    assert_eq!(
        report(Ok(ran(
            0,
            r#"{"pane":"%5","stop_reason":"end_turn","final_text":"  board is up  ","transcript":"ran ls\nboard is up"}"#,
            "%5",
        ))),
        format!("[zz] macbook:%5 (claude) finished:\nboard is up\n\n{follow}")
    );
    assert_eq!(
        report(Ok(ran(
            1,
            r#"{"pane":"%5","stop_reason":"max_tokens","final_text":"","transcript":"half"}"#,
            "agent-send: turn stopped: max_tokens\n%5",
        ))),
        format!("[zz] macbook:%5 (claude) stopped (max_tokens):\nhalf\n\n{follow}")
    );
    assert_eq!(
        report(Err(DaemonError::Server(ServerError::InvalidCommand(
            "%5: turn failed: rate limited".to_owned()
        )))),
        format!("[zz] macbook:%5 (claude) failed: rate limited\n{again}")
    );
    assert_eq!(
        report(Err(DaemonError::Server(ServerError::InvalidCommand(
            "%5 has 4 prompts waiting; wait for its turn with: zz agent-send -t %5 --wait ''"
                .to_owned()
        )))),
        format!(
            "[zz] macbook:%5 (claude) failed: %5 has 4 prompts waiting; wait for its turn with: zz agent-send -t %5 --wait ''\n{again}"
        )
    );
    assert_eq!(
        report(Err(DaemonError::Server(ServerError::PaneExited(pane)))),
        "[zz] macbook:%5 (claude) was closed before it replied. Tell the user; do not open it again."
    );
    assert_eq!(
        report(Err(DaemonError::Io(std::io::Error::other("ssh exited")))),
        format!(
            "[zz] macbook:%5 (claude) failed: lost the connection to macbook (daemon I/O error: ssh exited); read its last reply with: zz --host macbook capture-pane -p -t %5\n{again}"
        )
    );
}

#[test]
fn relay_is_an_option_of_a_plain_send() {
    let parsed = parse_agent_send_args(
        &["--relay", "macbook", "-t", "%5", "status", "please"].map(RawText::from),
    )
    .expect("relay send");
    assert_eq!(parsed.relay.as_deref(), Some("macbook"));
    assert_eq!(parsed.target.as_deref(), Some("%5"));
    assert_eq!(parsed.payload().expect("payload"), "status please");
}
