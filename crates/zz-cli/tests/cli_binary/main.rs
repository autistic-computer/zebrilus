#![allow(
    clippy::disallowed_methods,
    reason = "tests spawn helper processes from threads with an empty signal mask"
)]
use std::process::Command;

fn isolated_zz() -> (tempfile::TempDir, Command) {
    let root = if cfg!(unix) {
        std::path::PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    let directory = tempfile::Builder::new()
        .prefix("zz-cli-env-")
        .tempdir_in(root)
        .expect("temporary CLI home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_zz_cli"));
    scratch_env(&mut command, directory.path())
        .env("APPDATA", directory.path())
        .env("LOCALAPPDATA", directory.path())
        .env("USERPROFILE", directory.path());
    (directory, command)
}

fn scratch_env<'a>(command: &'a mut Command, root: &std::path::Path) -> &'a mut Command {
    let [data, runtime, temporary] = ["share", "run", "tmp"].map(|name| root.join(name));
    for directory in [&data, &runtime, &temporary] {
        std::fs::create_dir_all(directory).expect("scratch directory");
    }
    command
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_DATA_HOME", data)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("TMPDIR", temporary)
        .env("ZZ_WSL_DISCOVERY", "0")
}

const TMUX_USAGE: &str = concat!(
    "usage: zz [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n",
    "            [-S socket-path] [-T features] [command [flags]]\n"
);

#[test]
fn tmux_version_is_exact() {
    let (_home, mut zz) = isolated_zz();
    let output = zz.args(["-2uV", "ignored"]).output().expect("run zz -V");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"tmux 3.8-zz\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_tmux_flag_uses_tmux_usage_shape() {
    let (_home, mut zz) = isolated_zz();
    let output = zz
        .arg("-8")
        .output()
        .expect("run zz with an unknown tmux flag");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        format!("zz: unknown option -- 8\n{TMUX_USAGE}").as_bytes()
    );
}

#[test]
fn catalog_help_is_available_without_a_daemon() {
    for arguments in [
        &["--help"][..],
        &["help"][..],
        &["list-panes", "--help"][..],
        &["lsp", "--help"][..],
        &["list-pan", "--help"][..],
    ] {
        let (home, mut zz) = isolated_zz();
        let socket = home.path().join("absent.sock");
        let output = zz
            .arg("-S")
            .arg(&socket)
            .args(arguments)
            .output()
            .expect("run catalog help");
        assert_eq!(output.status.code(), Some(0), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
        let help = String::from_utf8(output.stdout).expect("UTF-8 help");
        if arguments.len() == 1 {
            assert!(help.contains("agent-send"));
            assert!(help.contains("\nglobal flags\n  --host NAME|DEST|all  "));
            assert!(help.contains("\n  fleet  ") && help.contains("\n  list-hosts  "));
            for name in zz_protocol::INTERNAL_COMMAND_NAMES {
                assert!(!help.contains(name), "advertised {name}");
            }
            assert!(help.contains("tmux commands"));
            assert!(help.contains("not implemented"));
        } else {
            assert!(help.contains("list-panes"));
            assert!(help.contains("-F"));
            assert!(help.contains("--json"));
        }
        assert!(!socket.exists());
    }
}

#[test]
fn host_verbs_have_help_without_a_daemon() {
    for (name, expected) in [
        (
            "fleet",
            "usage: zz fleet add NAME DEST | remove NAME | install",
        ),
        ("list-hosts", "usage: zz list-hosts [-F format] [--json]"),
    ] {
        let (home, mut zz) = isolated_zz();
        let socket = home.path().join("absent.sock");
        let output = zz
            .arg("-S")
            .arg(&socket)
            .args([name, "--help"])
            .output()
            .expect("run host verb help");
        assert_eq!(output.status.code(), Some(0), "{name}");
        assert!(output.stderr.is_empty(), "{name}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected),
            "{name}"
        );
        assert!(!socket.exists());
    }
}

#[test]
fn help_after_a_bare_double_dash_belongs_to_the_command() {
    for arguments in [
        &["run-pane", "-t", "%1", "--", "ls", "--help"][..],
        &["send-keys", "-t", "%1", "--", "--help"][..],
    ] {
        let (home, mut zz) = isolated_zz();
        let socket = home.path().join("absent.sock");
        let output = zz
            .env_remove("TMUX")
            .arg("-S")
            .arg(&socket)
            .args(arguments)
            .output()
            .expect("run a command whose payload says --help");
        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("usage:"),
            "{arguments:?}"
        );
        assert!(!socket.exists());
    }
}

#[test]
fn tools_answers_from_the_cli_without_a_daemon() {
    for (arguments, start, status) in [
        (&["tools"][..], "# Workspace tools\n", 0),
        (
            &["tools", "terminal"][..],
            "Run `zz tools` for targets, remote hosts, exit codes and every section.\n\n## Terminal panes\n",
            0,
        ),
        (&["tools", "--skill"][..], "---\nname: zz-workspace\n", 0),
        (&["tools", "bogus"][..], "", 2),
    ] {
        let (home, mut zz) = isolated_zz();
        let socket = home.path().join("absent.sock");
        let output = zz
            .env_remove("TMUX")
            .arg("-S")
            .arg(&socket)
            .args(arguments)
            .output()
            .expect("run zz tools");
        assert_eq!(output.status.code(), Some(status), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).starts_with(start),
            "{arguments:?}"
        );
        assert_eq!(output.stderr.is_empty(), status == 0, "{arguments:?}");
        assert!(!socket.exists());
    }
}

#[cfg(unix)]
#[test]
fn host_connect_failures_over_openssh_name_the_host_and_the_fix() {
    use std::os::unix::fs::PermissionsExt as _;

    let (home, _) = isolated_zz();
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("fake ssh directory");
    let ssh = bin.join("ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\n\
         case \"$*\" in *zz-probe-socket*) ;; *) exit 127 ;; esac\n\
         if [ -n \"$FAKE_SSH_ERROR\" ]; then printf '%s\\n' \"$FAKE_SSH_ERROR\" >&2; exit 255; fi\n\
         printf 'zz-probe-socket=/tmp/zz-fake.sock\\nzz-probe-protocol=%s\\n' \"$FAKE_SSH_PROTOCOL\"\n",
    )
    .expect("write fake ssh");
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod ssh");
    std::fs::create_dir_all(home.path().join("zz")).expect("config directory");
    std::fs::write(
        home.path().join("zz/config"),
        "host-gpu = ssh://you@gpu-box:2222\n",
    )
    .expect("write host config");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH");
    let newer = (zz_protocol::PROTOCOL_VERSION + 1).to_string();
    let missing = "zz: gpu has no zz; run: zz fleet install gpu\n";
    for (error, protocol, arguments, expected) in [
        (
            "Permission denied (publickey).",
            "",
            &["list-sessions"][..],
            "zz: gpu refused every login zz tried; run ssh ssh://you@gpu-box:2222 once, or zz fleet sync from a machine that reaches it\n".to_owned(),
        ),
        (
            "Host key verification failed.",
            "",
            &["list-sessions"][..],
            "zz: gpu's ssh host key is not trusted; run ssh ssh://you@gpu-box:2222 once to check and accept it\n".to_owned(),
        ),
        (
            "",
            newer.as_str(),
            &["list-sessions"][..],
            format!(
                "zz: gpu runs protocol v{newer}, newer than this zz (v{}); update zz on this machine\n",
                zz_protocol::PROTOCOL_VERSION
            ),
        ),
        ("", "missing", &["list-sessions"][..], missing.to_owned()),
        ("", "missing", &[][..], missing.to_owned()),
        ("", "missing", &["attach"][..], missing.to_owned()),
        ("", "missing", &["-C", "list-sessions"][..], missing.to_owned()),
        ("", "missing", &["events"][..], format!("events: {missing}")),
    ] {
        let output = scratch_env(&mut Command::new(env!("CARGO_BIN_EXE_zz_cli")), home.path())
            .env("PATH", &path)
            .env("FAKE_SSH_ERROR", error)
            .env("FAKE_SSH_PROTOCOL", protocol)
            .env_remove("TMUX")
            .env_remove("ZZ_SOCKET")
            .args(["--host", "gpu"])
            .args(arguments)
            .output()
            .expect("run zz --host gpu");
        assert_eq!(output.status.code(), Some(1), "{arguments:?} {error}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            expected,
            "{arguments:?} {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_unknown_host_that_does_not_connect_lists_the_known_hosts() {
    use std::os::unix::fs::PermissionsExt as _;

    let (home, mut zz) = isolated_zz();
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("fake ssh directory");
    let ssh = bin.join("ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\n\
         printf '%s\\n' 'ssh: Could not resolve hostname gpz: Name or service not known' >&2\n\
         exit 255\n",
    )
    .expect("write fake ssh");
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod ssh");
    std::fs::create_dir_all(home.path().join("zz")).expect("config directory");
    std::fs::write(
        home.path().join("zz/config"),
        "host-gpu = ssh://you@gpu-box\nhost-xps = ssh://you@xps\n",
    )
    .expect("write host config");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH");
    let output = zz
        .env("XDG_DATA_HOME", home.path())
        .env("XDG_RUNTIME_DIR", home.path())
        .env("TMPDIR", home.path())
        .env("PATH", &path)
        .env_remove("TMUX")
        .env_remove("ZZ_SOCKET")
        .args(["--host", "gpz", "list-sessions"])
        .output()
        .expect("run zz --host gpz");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.starts_with("zz: can't reach gpz: "), "{stderr}");
    assert!(stderr.ends_with("; known hosts: gpu, xps\n"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn every_login_offers_the_zz_key_and_parallel_work_never_prompts() {
    use std::os::unix::fs::PermissionsExt as _;

    let (home, _) = isolated_zz();
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("fake ssh directory");
    let ssh = bin.join("ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\n\
         printf '%s\\n' \"$*\" >> \"$FAKE_SSH_LOG\"\n\
         case \" $* \" in *\" -G \"*) printf 'user you\\nhostname fakebox\\n'; exit 0 ;; *\" -O \"*) exit 255 ;; esac\n\
         refuse() { printf 'you@fakebox: Permission denied (publickey).\\n' >&2; exit 255; }\n\
         case \"$*\" in\n\
         *IdentityFile=*) [ \"$FAKE_ZZ_KEY\" = refuse ] && refuse ;;\n\
         *BatchMode=yes*) refuse ;;\n\
         *) [ \"$FAKE_USER_LOGIN\" = ok ] || refuse ;;\n\
         esac\n\
         for script; do :; done\n\
         case \"$script\" in *authorized_keys*) printf 'zz-probe-key=added\\n' ;; esac\n\
         case \"$script\" in\n\
         *zz-probe-socket=*) printf 'zz-probe-socket=/tmp/zz-fake.sock\\nzz-probe-protocol=%s\\n' \"$FAKE_PROTOCOL\" ;;\n\
         *zz-install-protocol*) printf 'zz-install-current=1\\nzz-install-protocol=%s\\n' \"$FAKE_PROTOCOL\" ;;\n\
         *) printf 'remote cli ran\\n' ;;\n\
         esac\n",
    )
    .expect("write fake ssh");
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod ssh");
    let config = home.path().join(zz_protocol::app_identity::DIRECTORY);
    std::fs::create_dir_all(&config).expect("config directory");
    std::fs::write(
        config.join("config"),
        "tailnet = off\nhost-fake = ssh://you@fakebox\n",
    )
    .expect("write host config");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH");
    let log = home.path().join("ssh.log");
    let run = |arguments: &[&str], zz_key: &str, user_login: &str| {
        let _ = std::fs::remove_file(&log);
        let output = scratch_env(&mut Command::new(env!("CARGO_BIN_EXE_zz_cli")), home.path())
            .env("PATH", &path)
            .env("FAKE_SSH_LOG", &log)
            .env("FAKE_ZZ_KEY", zz_key)
            .env("FAKE_USER_LOGIN", user_login)
            .env("FAKE_PROTOCOL", zz_protocol::PROTOCOL_VERSION.to_string())
            .env_remove("TMUX")
            .env_remove("ZZ_SOCKET")
            .env_remove("ZZ_SSH_BATCH")
            .env_remove("RUST_LOG")
            .args(arguments)
            .output()
            .expect("run zz");
        let logins = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.starts_with("-G ") && !line.starts_with("-O "))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        (output, logins)
    };

    let (output, logins) = run(&["--host", "fake", "protocol-version"], "ok", "refuse");
    assert_eq!(output.status.code(), Some(0), "{output:?} {logins:#?}");
    assert_eq!(output.stdout, b"remote cli ran\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(logins[0].contains("authorized_keys"), "{logins:#?}");
    assert!(
        logins.iter().all(|line| line.contains("IdentityFile=")),
        "{logins:#?}"
    );

    let (output, logins) = run(
        &["--host", "you@elsewhere", "protocol-version"],
        "ok",
        "refuse",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?} {logins:#?}");
    assert!(
        logins.iter().all(|line| !line.contains("authorized_keys")),
        "{logins:#?}"
    );

    let (output, logins) = run(&["--host", "all", "protocol-version"], "refuse", "ok");
    assert_eq!(output.status.code(), Some(1), "{output:?} {logins:#?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(!logins.is_empty());
    assert!(
        logins.iter().all(|line| line.contains("BatchMode=yes")
            && (line.contains("IdentityFile=")
                || line.contains("PubkeyAcceptedKeyTypes=-sk-*,webauthn-sk-*"))),
        "{logins:#?}"
    );

    if zz_protocol::app_identity::DEVELOPMENT {
        return;
    }
    let installed = format!("fake: already zz {}\n", env!("CARGO_PKG_VERSION"));
    let (output, logins) = run(&["fleet", "install", "fake"], "refuse", "ok");
    assert_eq!(output.status.code(), Some(0), "{output:?} {logins:#?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), installed);
    let first_user_login = logins
        .iter()
        .position(|line| !line.contains("IdentityFile="))
        .expect("a login without the zz key");
    assert!(
        logins[first_user_login].contains("BatchMode=yes"),
        "{logins:#?}"
    );
    assert!(
        logins[first_user_login + 1..]
            .iter()
            .any(|line| !line.contains("BatchMode")),
        "{logins:#?}"
    );

    let (output, logins) = run(&["fleet", "install", "fake"], "ok", "refuse");
    assert_eq!(output.status.code(), Some(0), "{output:?} {logins:#?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), installed);
    assert!(
        logins.iter().all(|line| line.contains("IdentityFile=")),
        "{logins:#?}"
    );
}

#[cfg(unix)]
#[test]
fn a_host_probe_riding_a_master_that_stopped_answering_gives_up() {
    use std::os::unix::fs::PermissionsExt as _;

    let (home, _) = isolated_zz();
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("fake ssh directory");
    let master = home.path().join("cm");
    std::fs::write(&master, b"").expect("fake master socket");
    let ssh = bin.join("ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\n\
         case \" $* \" in\n\
         *\" -G \"*) printf 'controlmaster auto\\ncontrolpath %s\\n' \"$FAKE_MASTER\"; exit 0 ;;\n\
         *\" -O stop \"*) kill \"$(cat \"$FAKE_MASTER.pid\")\"; exit 0 ;;\n\
         *\" -O \"*) exit 0 ;;\n\
         esac\n\
         echo $$ > \"$FAKE_MASTER.pid\"\n\
         exec sleep 60\n",
    )
    .expect("write fake ssh");
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod ssh");
    let config = home.path().join(zz_protocol::app_identity::DIRECTORY);
    std::fs::create_dir_all(&config).expect("config directory");
    std::fs::write(
        config.join("config"),
        "tailnet = off\nhost-fake = ssh://you@fakebox\n",
    )
    .expect("write host config");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH");
    let started = std::time::Instant::now();
    let output = scratch_env(&mut Command::new(env!("CARGO_BIN_EXE_zz_cli")), home.path())
        .env("PATH", &path)
        .env("FAKE_MASTER", &master)
        .env_remove("ZZ_SSH_BATCH")
        .args([
            "list-hosts",
            "-F",
            "#{host_name} #{host_state} #{host_error}",
        ])
        .output()
        .expect("run zz list-hosts");
    let elapsed = started.elapsed();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "fake unreachable could not run ssh: the remote script got no answer over the open ssh \
         connection within 15 s\n"
    );
    assert!((14..30).contains(&elapsed.as_secs()), "{elapsed:?}");
}

#[cfg(unix)]
#[test]
fn a_terminal_chain_that_stalls_on_a_kept_link_is_not_sent_again() {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let (home, _) = isolated_zz();
    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).expect("fake ssh directory");
    let ssh = bin.join("ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\n\
         printf '%s\\n' \"$*\" >> \"$FAKE_SSH_LOG\"\n\
         case \" $* \" in *\" -G \"*) exit 0 ;; esac\n\
         printf 'you@fakebox: Permission denied (publickey).\\n' >&2\n\
         exit 255\n",
    )
    .expect("write fake ssh");
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod ssh");
    let config = home.path().join(zz_protocol::app_identity::DIRECTORY);
    std::fs::create_dir_all(&config).expect("config directory");
    std::fs::write(
        config.join("config"),
        "tailnet = off\nhost-fake = ssh://you@fakebox\n",
    )
    .expect("write host config");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH");
    let log = home.path().join("ssh.log");
    let run = |arguments: &[&str]| {
        let _ = std::fs::remove_file(&log);
        let output = scratch_env(&mut Command::new(env!("CARGO_BIN_EXE_zz_cli")), home.path())
            .env("PATH", &path)
            .env("FAKE_SSH_LOG", &log)
            .env_remove("TMUX")
            .env_remove("ZZ_SOCKET")
            .env_remove("ZZ_SSH_BATCH")
            .env_remove("RUST_LOG")
            .args(arguments)
            .output()
            .expect("run zz");
        (output, std::fs::read_to_string(&log).unwrap_or_default())
    };

    let (output, _) = run(&["--host", "fake", "list-sessions"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let mut pending = vec![home.path().to_path_buf()];
    let mut kept = None;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).expect("read scratch directory") {
            let entry = entry.expect("scratch entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().expect("entry type").is_dir() {
                pending.push(entry.path());
            } else if name.starts_with('c')
                && let Some(stem) = name.strip_suffix(".sock.lock")
            {
                kept = Some(entry.path().with_file_name(format!("{stem}.sock")));
            }
        }
    }
    let kept = kept.expect("the kept forward's lock");
    let listener = std::os::unix::net::UnixListener::bind(&kept).expect("bind the kept forward");
    let hellos = std::sync::Arc::new(AtomicUsize::new(0));
    let accepted = hellos.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            accepted.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || std::io::copy(&mut stream, &mut std::io::sink()));
        }
    });

    let started = std::time::Instant::now();
    let (output, log) = run(&[
        "--host",
        "fake",
        "new-session",
        "-A",
        "-s",
        "tui",
        ";",
        "new-window",
        "-d",
        "-t",
        "tui",
        "-n",
        "dup",
    ]);
    let elapsed = started.elapsed();
    assert_eq!(output.status.code(), Some(1), "{output:?} {log}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "zz: fake stopped answering for 15 s; the command may have run there, check before \
         retrying\n"
    );
    assert_eq!(hellos.load(Ordering::SeqCst), 1);
    assert!(
        log.lines().any(|line| line.starts_with("-O stop ")),
        "{log}"
    );
    assert!(
        log.lines()
            .all(|line| line.starts_with("-G ") || line.starts_with("-O ")),
        "{log}"
    );
    assert!(!kept.exists());
    assert!((14..30).contains(&elapsed.as_secs()), "{elapsed:?}");
}

#[test]
fn internal_command_help_remains_available_without_a_daemon() {
    for name in zz_protocol::INTERNAL_COMMAND_NAMES {
        let (home, mut zz) = isolated_zz();
        let socket = home.path().join("absent.sock");
        let output = zz
            .arg("-S")
            .arg(&socket)
            .args([name, "--help"])
            .output()
            .expect("run internal command help");
        assert_eq!(output.status.code(), Some(0), "{name}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains(name));
        assert!(!socket.exists());
    }
}

#[test]
fn usage_errors_keep_their_surface_status_without_a_daemon() {
    for (arguments, expected, status) in [
        (&["bogus-verb"][..], "unknown command: bogus-verb", 1),
        (
            &["bogus-verb", "--help"][..],
            "unknown command: bogus-verb",
            2,
        ),
        (&["split-window", "--nope"][..], "invalid flag", 1),
        (&["-Z"][..], "unknown option -- Z", 1),
        (&["list-sessions", "-F", "x", "--json"][..], "--json", 2),
    ] {
        let (home, mut zz) = isolated_zz();
        let socket = home.path().join("absent.sock");
        let output = zz
            .env_remove("TMUX")
            .arg("-S")
            .arg(&socket)
            .args(arguments)
            .output()
            .expect("run invalid CLI arguments");
        assert_eq!(output.status.code(), Some(status), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{arguments:?}"
        );
        assert!(!socket.exists());
    }
}

#[test]
fn list_hosts_reads_zz_config_without_a_daemon_or_ssh() {
    let (home, _) = isolated_zz();
    let config = home
        .path()
        .join(zz_protocol::app_identity::DIRECTORY)
        .join("config");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        "tailnet = off\nhost-xps = ssh://demfabris@xps\nhost-gpu = quic://gpu:9922\n",
    )
    .unwrap();
    let run = |arguments: &[&str]| {
        scratch_env(&mut Command::new(env!("CARGO_BIN_EXE_zz_cli")), home.path())
            .env_remove("TMUX")
            .arg("-S")
            .arg(home.path().join("absent.sock"))
            .args(arguments)
            .output()
            .expect("run zz")
    };

    let listed = run(&["list-hosts", "-F", "#{host_name}: #{host_url}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"xps: ssh://demfabris@xps\n");
    assert!(
        String::from_utf8_lossy(&listed.stderr).contains("host-gpu: "),
        "{listed:?}"
    );

    let both = run(&["list-hosts", "--json", "-F", "x"]);
    assert_eq!(both.status.code(), Some(2));
    assert_eq!(
        both.stderr,
        b"command list-hosts: --json cannot be combined with -F\n"
    );

    let alias = run(&["fleet", "list", "--json", "-F", "x"]);
    assert_eq!(alias.status.code(), Some(2));
    assert_eq!(
        alias.stderr,
        b"zz: usage: zz fleet list [-F <format> | --json]\n"
    );

    let fleet = run(&["fleet", "list"]);
    assert_eq!(fleet.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&fleet.stdout).starts_with("xps  ssh://demfabris@xps\n\ndropped"),
        "{fleet:?}"
    );
    assert!(!home.path().join("absent.sock").exists());
}

#[test]
fn short_help_retains_tmux_usage() {
    let (_home, mut zz) = isolated_zz();
    let output = zz.arg("-h").output().expect("run zz -h");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, TMUX_USAGE.as_bytes());
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn tmux_without_a_zz_socket_fails_before_dialing_a_foreign_server() {
    let (_home, mut zz) = isolated_zz();
    let output = zz
        .env("TMUX", "/tmp/real-tmux.sock,123,0")
        .env_remove("ZZ_SOCKET")
        .arg("list-sessions")
        .output()
        .expect("run zz inside a foreign tmux environment");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"zz: TMUX is set but ZZ_SOCKET is not; refusing to treat a tmux server as zz\nUse `zz app` to open the GUI, or pass `-S` / set `ZZ_SOCKET` to target a zz daemon.\n"
    );
}

#[cfg(unix)]
mod daemon_autostart;
