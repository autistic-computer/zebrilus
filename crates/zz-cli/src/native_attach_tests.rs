use zz_protocol::RawText;

use super::{NativeAttachArgumentError, native_attach_command, parse_native_attach_arguments};
use zz_protocol::ServerError;

#[test]
fn native_attach_parser_keeps_the_zz_superset_and_tmux_target() {
    let target =
        parse_native_attach_arguments(["-d", "-t", "work", "--restart-daemon"].map(RawText::from))
            .unwrap();
    assert!(target.detach_others);
    assert!(target.restart_daemon);
    assert!(!target.no_update_environment);
    assert!(!target.read_only);
    assert_eq!(target.working_directory, None);
    assert_eq!(target.session.as_deref(), Some("work"));
    assert_eq!(native_attach_command(&target).args, ["-d", "-t", "work"]);

    let positional =
        parse_native_attach_arguments(["work", "--restart-daemon"].map(RawText::from)).unwrap();
    assert!(!positional.detach_others);
    assert!(positional.restart_daemon);
    assert_eq!(positional.session.as_deref(), Some("work"));
    assert_eq!(
        parse_native_attach_arguments(["work", "-@"].map(RawText::from)),
        Err(NativeAttachArgumentError::Usage)
    );

    let read_only =
        parse_native_attach_arguments(["-dr", "-t", "work"].map(RawText::from)).unwrap();
    assert!(read_only.detach_others);
    assert!(read_only.read_only);
    assert_eq!(read_only.client_flags, None);
    assert_eq!(read_only.session.as_deref(), Some("work"));

    let no_update = parse_native_attach_arguments(["-E", "-t", "work"].map(RawText::from)).unwrap();
    assert!(no_update.no_update_environment);
    assert_eq!(no_update.session.as_deref(), Some("work"));
    assert_eq!(native_attach_command(&no_update).args, ["-E", "-t", "work"]);

    let flags = parse_native_attach_arguments(
        [
            "-f",
            "ignore-size",
            "-fread-only,no-detach-on-destroy",
            "work",
        ]
        .map(RawText::from),
    )
    .unwrap();
    assert_eq!(
        flags.client_flags.as_deref(),
        Some("read-only,no-detach-on-destroy")
    );

    let cwd =
        parse_native_attach_arguments(["-dc/tmp/work", "-t", "work"].map(RawText::from)).unwrap();
    assert!(cwd.detach_others);
    assert_eq!(cwd.working_directory.as_deref(), Some("/tmp/work"));
    assert_eq!(cwd.session.as_deref(), Some("work"));
    assert_eq!(
        native_attach_command(&cwd).args,
        ["-d", "-c", "/tmp/work", "-t", "work"]
    );

    let bundled = parse_native_attach_arguments(
        ["-dEr", "-fignore-size", "-c/tmp/work", "-twork"].map(RawText::from),
    )
    .unwrap();
    assert!(bundled.detach_others);
    assert!(bundled.no_update_environment);
    assert!(bundled.read_only);
    let command = native_attach_command(&bundled);
    assert_eq!(command.name, "attach-session");
    assert_eq!(
        command.args,
        [
            "-d",
            "-E",
            "-r",
            "-f",
            "ignore-size",
            "-c",
            "/tmp/work",
            "-t",
            "work"
        ]
    );

    assert_eq!(
        native_attach_command(&read_only).args,
        ["-d", "-r", "-t", "work"]
    );

    assert!(matches!(
        parse_native_attach_arguments(["-t", "one", "two"].map(RawText::from)),
        Err(super::NativeAttachArgumentError::Usage)
    ));

    for (arguments, expected) in [
        (
            vec!["-0"],
            ServerError::CommandParse(
                "command attach-session: unknown flag -0".to_owned(),
            ),
        ),
        (
            vec!["-@"],
            ServerError::CommandParse(
                "command attach-session: invalid flag -@".to_owned(),
            ),
        ),
        (
            vec!["--bogus"],
            ServerError::CommandParse(
                "command attach-session: invalid flag --".to_owned(),
            ),
        ),
        (
            vec!["-?"],
            ServerError::CommandParse(
                "usage: attach-session [-dErx] [-c working-directory] [-f flags] [-t target-session]"
                    .to_owned(),
            ),
        ),
        (
            vec!["-t"],
            ServerError::CommandParse(
                "command attach-session: -t expects an argument".to_owned(),
            ),
        ),
        (
            vec!["-x0"],
            ServerError::CommandParse(
                "command attach-session: unknown flag -0".to_owned(),
            ),
        ),
    ] {
        assert_eq!(
            parse_native_attach_arguments(arguments.into_iter().map(RawText::from)),
            Err(NativeAttachArgumentError::Command(expected))
        );
    }
    let hangup =
        parse_native_attach_arguments(["-x"].map(RawText::from)).expect("attach-session -x parses");
    assert!(hangup.detach_others_hangup);
    assert!(!hangup.detach_others);
    assert_eq!(native_attach_command(&hangup).args, ["-x".to_owned()]);
    assert_eq!(
        parse_native_attach_arguments(["-t", "-?"].map(RawText::from))
            .unwrap()
            .session
            .as_deref(),
        Some("-?")
    );
    assert_eq!(
        parse_native_attach_arguments(["--", "work"].map(RawText::from))
            .unwrap()
            .session
            .as_deref(),
        Some("work")
    );
    let literal_restart =
        parse_native_attach_arguments(["--", "--restart-daemon"].map(RawText::from)).unwrap();
    assert!(!literal_restart.restart_daemon);
    assert_eq!(literal_restart.session.as_deref(), Some("--restart-daemon"));
}
