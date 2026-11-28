use super::*;

#[cfg(unix)]
#[test]
fn builds_forward_ssh_argv_for_user_and_port_permutations() {
    for (user, port, options) in ssh_option_cases() {
        for socks_port in [None, Some(41_080)] {
            let endpoint = test_ssh_endpoint(user, port);
            let command = ssh_forward_command(
                &endpoint,
                test_session(),
                Path::new("/tmp/local.sock"),
                Path::new("/run/user/1000/zz/default.sock"),
                socks_port,
            );
            let mut expected = vec![
                "-N",
                "-n",
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "StreamLocalBindMask=0177",
                "-L",
                "/tmp/local.sock:/run/user/1000/zz/default.sock",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
            if let Some(socks_port) = socks_port {
                expected.extend(["-D".to_owned(), format!("127.0.0.1:{socks_port}")]);
            }
            expected.extend(shared_ssh_options());
            expected.extend(options.iter().copied().map(str::to_owned));
            expected.extend(["--", "host"].into_iter().map(str::to_owned));
            assert_eq!(command_args(&command), expected);
        }
    }
}

#[cfg(unix)]
#[test]
fn control_cancel_names_the_forward_kind_it_gives_back() {
    let endpoint = test_ssh_endpoint(None, None);
    let control_path = Path::new(TEST_CONTROL_PATH);
    for (flag, spec) in [
        ("-L", "/tmp/local.sock:/run/zz.sock"),
        ("-D", "127.0.0.1:41080"),
    ] {
        for request in ["cancel", "forward"] {
            let command = ssh_control_command(
                &endpoint,
                control_path,
                request,
                &[(flag, OsStr::new(spec))],
            );
            let arguments = command_args(&command);
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == [flag.to_owned(), spec.to_owned()]),
                "{arguments:?}",
            );
            assert_eq!(arguments[..2], ["-O".to_owned(), request.to_owned()]);
        }
    }
    let both = command_args(&ssh_control_command(
        &endpoint,
        control_path,
        "forward",
        &[
            ("-L", OsStr::new("/tmp/local.sock:/run/zz.sock")),
            ("-D", OsStr::new("127.0.0.1:41080")),
        ],
    ));
    assert!(
        both.windows(4).any(|window| window
            == [
                "-L",
                "/tmp/local.sock:/run/zz.sock",
                "-D",
                "127.0.0.1:41080"
            ]),
        "{both:?}",
    );
    let check = command_args(&ssh_control_command(&endpoint, control_path, "check", &[]));
    assert!(
        !check
            .iter()
            .any(|argument| argument == "-L" || argument == "-D"),
        "{check:?}",
    );
}

#[cfg(unix)]
#[test]
fn builds_probe_ssh_argv_for_user_and_port_permutations() {
    for (user, port, options) in ssh_option_cases() {
        let endpoint = test_ssh_endpoint(user, port);
        let command = ssh_probe_command(&endpoint, test_session(), None);
        let mut expected = shared_ssh_options();
        expected.extend(options.into_iter().map(str::to_owned));
        expected.extend(
            ["--", "host", "sh", "-lc", &remote_socket_probe(None)]
                .into_iter()
                .map(str::to_owned),
        );
        assert_eq!(command_args(&command), expected);
    }
}

#[cfg(unix)]
#[test]
fn builds_autostart_ssh_argv_for_user_and_port_permutations() {
    let remote_socket = Path::new("/run/user/1000/zz/default.sock");
    for (user, port, options) in ssh_option_cases() {
        let endpoint = test_ssh_endpoint(user, port);
        let command = ssh_daemon_start_command(&endpoint, test_session(), remote_socket);
        let mut expected = shared_ssh_options();
        expected.extend(options.into_iter().map(str::to_owned));
        expected.extend(["--", "host", "sh", "-lc"].into_iter().map(str::to_owned));
        expected.push(shell_quote(&remote_daemon_start_script(remote_socket)));
        assert_eq!(command_args(&command), expected);
    }
}

#[cfg(unix)]
#[test]
fn a_user_master_keeps_its_own_persistence() {
    let endpoint = test_ssh_endpoint(None, None);
    for (user_persist, persists) in [(true, false), (false, true)] {
        let control = SshControl {
            path: PathBuf::from("/home/ada/.ssh/cm-f5de03fd"),
            user_persist,
        };
        let session = SshSession {
            control: Some(&control),
            ..SshSession::default()
        };
        let arguments = command_args(&ssh_probe_command(&endpoint, session, None));
        assert!(
            arguments.contains(&"ControlPath=/home/ada/.ssh/cm-f5de03fd".to_owned()),
            "{arguments:?}"
        );
        assert!(
            arguments.contains(&"ControlMaster=auto".to_owned()),
            "{arguments:?}"
        );
        assert_eq!(
            arguments
                .iter()
                .any(|argument| argument.starts_with("ControlPersist=")),
            persists,
            "{arguments:?}"
        );
    }
}

#[test]
fn a_session_without_a_master_uses_platform_safe_options() {
    let endpoint = test_ssh_endpoint(None, None);
    let command = ssh_probe_command(&endpoint, SshSession::default(), None);
    let mut expected = [
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=5",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "Compression=yes",
    ]
    .map(str::to_owned)
    .to_vec();
    #[cfg(windows)]
    expected.extend(["-o", "ControlMaster=no", "-o", "ControlPath=none"].map(str::to_owned));
    expected.extend(["--", "host", "sh", "-lc", &remote_socket_probe(None)].map(str::to_owned));
    assert_eq!(command_args(&command), expected);
    assert_eq!(
        command_envs(&command),
        Vec::<(String, Option<String>)>::new()
    );
}

#[test]
fn remote_probe_requires_a_matching_reported_protocol() {
    let output = format!(
        "zz-probe-socket=/run/user/1000/zz/default.sock\nzz-probe-protocol={}\n",
        zz_protocol::PROTOCOL_VERSION
    );
    assert_eq!(
        parse_remote_probe_output("host", output.as_bytes())
            .socket
            .unwrap(),
        Path::new("/run/user/1000/zz/default.sock")
    );

    assert!(matches!(
        parse_remote_probe_output(
            "host",
            b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=48\n"
        )
        .socket,
        Err(EndpointError::RemoteProtocolMismatch {
            target,
            daemon: 48,
            client: zz_protocol::PROTOCOL_VERSION,
        }) if target == "host"
    ));
    assert_eq!(
        parse_remote_probe_output(
            "host",
            b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=unknown\n"
        )
        .socket
        .unwrap_err()
        .to_string(),
        "zz on host predates protocol version reporting; update it, then reconnect"
    );
    assert!(matches!(
        parse_remote_probe_output(
            "host",
            b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=banana\n"
        )
        .socket,
        Err(EndpointError::ProbeFailure { .. })
    ));
}

#[test]
fn remote_probe_tolerates_login_profile_noise_and_a_pathless_zz() {
    let noisy = format!(
        "Welcome to devbox!\nzz-probe-socket=/run/zz.sock\nmotd: 3 updates\nzz-probe-protocol={}\n",
        zz_protocol::PROTOCOL_VERSION
    );
    assert_eq!(
        parse_remote_probe_output("host", noisy.as_bytes())
            .socket
            .unwrap(),
        Path::new("/run/zz.sock")
    );

    assert_eq!(
        parse_remote_probe_output(
            "host",
            b"zz-probe-socket=/run/zz.sock\nzz-probe-protocol=missing\n"
        )
        .socket
        .unwrap(),
        Path::new("/run/zz.sock")
    );

    assert!(matches!(
        parse_remote_probe_output("host", b"zz-probe-socket=/run/zz.sock\n").socket,
        Err(EndpointError::ProbeFailure { .. })
    ));
    assert!(matches!(
        parse_remote_probe_output("host", b"only noise, no sentinels\n").socket,
        Err(EndpointError::ProbeFailure { .. })
    ));
}

#[cfg(unix)]
#[test]
fn zz_key_phase_offers_only_the_zz_identity() {
    let endpoint = test_ssh_endpoint(Some("user"), None);
    let identity = Path::new("/Users/a/Library/Application Support/zz-100%/ssh/id_ed25519");
    let line = "ssh-ed25519 AAAAzz zz@desk";
    let mut expected = [
        "-o",
        "IdentitiesOnly=yes",
        "-o",
        "IdentityFile=\"/Users/a/Library/Application Support/zz-100%%/ssh/id_ed25519\"",
        "-o",
        "PreferredAuthentications=publickey",
        "-o",
        "BatchMode=yes",
    ]
    .map(str::to_owned)
    .to_vec();
    expected.extend(shared_ssh_options());
    expected.extend(["-l", "user", "--", "host", "sh", "-lc"].map(str::to_owned));
    expected.push(remote_socket_probe(Some(line)));
    assert_eq!(
        command_args(&ssh_probe_command(
            &endpoint,
            SshSession {
                identity: Some(identity),
                ..test_session()
            },
            Some(line)
        )),
        expected
    );

    let fallback = command_args(&ssh_probe_command(&endpoint, test_session(), Some(line)));
    assert_eq!(fallback[..shared_ssh_options().len()], shared_ssh_options());
    for option in [
        "IdentitiesOnly",
        "IdentityFile",
        "PreferredAuthentications",
        "BatchMode",
    ] {
        assert!(
            !fallback.iter().any(|argument| argument.starts_with(option)),
            "the user's own login must keep OpenSSH's default identities: {fallback:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_login_the_zz_key_won_and_a_batch_login_reach_every_later_command() {
    let endpoint = test_ssh_endpoint(None, None);
    let identity = Path::new("/home/a/.local/share/zz/ssh/id_ed25519");
    let signed_in = SshSession {
        identity: Some(identity),
        ..test_session()
    };
    let batch = SshSession {
        batch: true,
        ..test_session()
    };
    let socket = Path::new("/run/zz.sock");
    let local = Path::new("/tmp/local.sock");
    let commands = |session| {
        [
            ssh_daemon_start_command(&endpoint, session, socket),
            ssh_forward_command(&endpoint, session, local, socket, None),
            ssh_cli_command(&endpoint, session, &["protocol-version".to_owned()]),
            ssh_install_command(&endpoint, session, false),
        ]
        .map(|command| command_args(&command))
    };
    let login = |arguments: &[String]| {
        arguments
            .iter()
            .filter(|argument| {
                [
                    "Identit",
                    "PreferredAuthentications",
                    "BatchMode",
                    "PubkeyAccepted",
                ]
                .iter()
                .any(|option| argument.starts_with(option))
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    for arguments in commands(signed_in) {
        assert_eq!(
            login(&arguments),
            [
                "IdentitiesOnly=yes".to_owned(),
                format!("IdentityFile=\"{}\"", identity.display()),
                "PreferredAuthentications=publickey".to_owned(),
                "BatchMode=yes".to_owned(),
            ],
            "{arguments:?}"
        );
    }
    for arguments in commands(batch) {
        assert_eq!(
            login(&arguments),
            ["BatchMode=yes", NO_SECURITY_KEYS],
            "{arguments:?}"
        );
    }
}

#[test]
fn only_a_login_a_person_can_answer_is_retried_on_the_terminal() {
    let failed = |stderr: &str| EndpointError::SshFailed {
        target: "ssh://host".to_owned(),
        reason: ssh_failure_reason(stderr, "exit status: 255"),
    };
    for stderr in [
        "you@host: Permission denied (publickey).",
        "you@host: Permission denied (publickey,password).",
        "Host key verification failed.",
        "sign_and_send_pubkey: signing failed for ED25519-SK \"/k\": device not found",
    ] {
        assert!(failed(stderr).needs_sign_in(), "{stderr}");
    }
    for stderr in [
        "ssh: connect to host host port 22: Operation timed out",
        "ssh: Could not resolve hostname host: nodename nor servname",
        "bash: line 1: zz: command not found",
    ] {
        assert!(!failed(stderr).needs_sign_in(), "{stderr}");
    }
    assert!(
        !EndpointError::RemoteBinaryMissing {
            target: "ssh://host".to_owned()
        }
        .needs_sign_in()
    );
}

#[test]
fn only_a_refused_login_falls_back_to_the_users_own() {
    for stderr in [
        "user@host: Permission denied (publickey).",
        "Host key verification failed.",
        "sign_and_send_pubkey: signing failed for ED25519-SK \"/k\": device not found",
    ] {
        assert!(refused_login(Some(255), stderr.as_bytes()), "{stderr}");
    }
    for (code, stderr) in [
        (
            Some(255),
            "ssh: connect to host desk port 22: Operation timed out",
        ),
        (
            Some(255),
            "ssh: Could not resolve hostname desk: nodename nor servname",
        ),
        (
            Some(255),
            "ssh: connect to host desk port 22: Connection refused",
        ),
        (Some(1), "mkdir: Permission denied"),
        (None, "Permission denied"),
    ] {
        assert!(!refused_login(code, stderr.as_bytes()), "{code:?} {stderr}");
    }
}

#[cfg(unix)]
fn path_with_a_current_zz() -> (tempfile::TempDir, String) {
    let bin = tempfile::tempdir().expect("temporary bin");
    let zz = bin.path().join(zz_protocol::app_identity::DIRECTORY);
    fs::write(
        &zz,
        format!(
            "#!/bin/sh\ncase \"$1\" in protocol-version) echo {};; esac\n",
            zz_protocol::PROTOCOL_VERSION
        ),
    )
    .expect("write zz");
    fs::set_permissions(&zz, fs::Permissions::from_mode(0o755)).expect("make zz runnable");
    let path = format!("{}:/usr/bin:/bin", bin.path().display());
    (bin, path)
}

#[cfg(unix)]
#[test]
fn the_probe_enrolls_this_machine_once_and_reports_the_hosts_own_key() {
    let line =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGW3/OPNnD2l/h6LfRGUUDgkTB5X9f1CU1CuD1GC4d5u zz@hub";
    let (_bin, path) = path_with_a_current_zz();
    let shells = ["/bin/sh", "/bin/tcsh"]
        .into_iter()
        .filter(|shell| Path::new(shell).exists());
    for shell in shells {
        let home = tempfile::tempdir().expect("temporary home");
        let probe = || {
            let output = Command::new(shell)
                .args(["-c", &format!("sh -c {}", remote_socket_probe(Some(line)))])
                .env("HOME", home.path())
                .env("PATH", &path)
                .env_remove("XDG_DATA_HOME")
                .env_remove("XDG_RUNTIME_DIR")
                .output()
                .expect("run the probe");
            assert!(output.status.success(), "{shell}: {output:?}");
            parse_remote_probe_output("host", &output.stdout)
        };

        let first = probe();
        assert!(first.socket.is_ok(), "{shell}: {:?}", first.socket);
        assert_eq!(first.key, Some(KeyEnrollment::Added), "{shell}");
        let host_key = first.pubkey.expect("the probe makes the host's own key");
        assert!(host_key.starts_with("ssh-ed25519 "), "{host_key}");

        let second = probe();
        assert_eq!(second.key, Some(KeyEnrollment::Present), "{shell}");
        assert_eq!(second.pubkey.as_deref(), Some(host_key.as_str()));

        let ssh = home.path().join(".ssh");
        let authorized = ssh.join("authorized_keys");
        assert_eq!(
            fs::read_to_string(&authorized).unwrap(),
            format!("{line}\n")
        );
        assert_eq!(
            fs::metadata(&ssh).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&authorized).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let data = if cfg!(target_os = "macos") {
            home.path().join("Library/Application Support")
        } else {
            home.path().join(".local/share")
        };
        let key_directory = data.join(zz_protocol::app_identity::DIRECTORY).join("ssh");
        assert_eq!(
            fs::metadata(&key_directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(key_directory.join("id_ed25519"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn a_probe_without_a_key_line_writes_nothing() {
    let home = tempfile::tempdir().expect("temporary home");
    let (_bin, path) = path_with_a_current_zz();
    let output = Command::new("/bin/sh")
        .args(["-c", &format!("sh -c {}", remote_socket_probe(None))])
        .env("HOME", home.path())
        .env("PATH", &path)
        .env_remove("XDG_DATA_HOME")
        .output()
        .expect("run the probe");
    let probe = parse_remote_probe_output("host", &output.stdout);
    assert!(probe.socket.is_ok());
    assert_eq!((probe.key, probe.pubkey), (None, None));
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn the_probe_reports_enrollment_and_only_a_well_formed_host_key() {
    let output = format!(
        "zz-probe-key=added\nzz-probe-pubkey=ssh-ed25519 AAAAzz zz@xps\n\
             zz-probe-socket=/run/zz.sock\nzz-probe-protocol={}\n",
        zz_protocol::PROTOCOL_VERSION
    );
    let probe = parse_remote_probe_output("host", output.as_bytes());
    assert_eq!(probe.key, Some(KeyEnrollment::Added));
    assert_eq!(probe.pubkey.as_deref(), Some("ssh-ed25519 AAAAzz zz@xps"));
    assert!(probe.socket.is_ok());

    let probe = parse_remote_probe_output(
        "host",
        b"zz-probe-key=failed\nzz-probe-pubkey=command=\"id\" ssh-ed25519 AAAAzz zz@xps\n\
              zz-probe-socket=/run/zz.sock\nzz-probe-protocol=48\n",
    );
    assert_eq!(probe.key, Some(KeyEnrollment::Failed));
    assert_eq!(probe.pubkey, None);
    assert!(matches!(
        probe.socket,
        Err(EndpointError::RemoteProtocolMismatch { daemon: 48, .. })
    ));
}

#[cfg(unix)]
#[test]
fn a_percent_in_the_control_path_is_escaped_not_expanded() {
    assert_eq!(
        control_path_option(Path::new("/tmp/zz-100%-a/c1")),
        "ControlPath=/tmp/zz-100%%-a/c1",
    );
}

#[cfg(unix)]
#[test]
fn askpass_reaches_every_ssh_child_through_the_environment() {
    let endpoint = test_ssh_endpoint(None, None);
    let helper = PathBuf::from("/opt/zz/bin/zz");
    let socket = PathBuf::from("/tmp/zz-ssh/a1");
    let session = SshSession {
        askpass: Some(SshAskpass {
            helper: &helper,
            socket: &socket,
        }),
        ..SshSession::default()
    };
    for command in [
        ssh_probe_command(&endpoint, session, None),
        ssh_daemon_start_command(&endpoint, session, Path::new("/run/zz.sock")),
        ssh_forward_command(
            &endpoint,
            session,
            Path::new("/tmp/local.sock"),
            Path::new("/run/zz.sock"),
            Some(41_080),
        ),
    ] {
        let mut envs = command_envs(&command);
        envs.sort();
        assert_eq!(
            envs,
            [
                ("SSH_ASKPASS", "/opt/zz/bin/zz"),
                ("SSH_ASKPASS_REQUIRE", "force"),
                ("ZZ_ASKPASS_SOCKET", "/tmp/zz-ssh/a1"),
            ]
            .map(|(name, value)| (name.to_owned(), Some(value.to_owned())))
        );
    }
}

#[cfg(unix)]
#[test]
fn remote_cli_runs_the_hosts_own_zz_with_quoted_arguments() {
    let endpoint = test_ssh_endpoint(Some("user"), Some(2222));
    let arguments = ["fleet", "add", "box", "ssh://ada@box", "it's"].map(str::to_owned);
    let command = ssh_cli_command(&endpoint, test_session(), &arguments);
    let mut expected = vec!["-T".to_owned()];
    expected.extend(shared_ssh_options());
    expected.extend(["-p", "2222", "-l", "user", "--", "host", "sh", "-lc"].map(str::to_owned));
    expected.push(shell_quote(&format!(
        "{}exec \"$zz_cli\" 'fleet' 'add' 'box' 'ssh://ada@box' 'it'\\''s'",
        remote_path_fallback()
    )));
    assert_eq!(command_args(&command), expected);

    let directory = tempfile::tempdir().unwrap();
    let bin = directory.path().join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    let cli = bin.join(zz_protocol::app_identity::DIRECTORY);
    fs::write(
        &cli,
        "#!/bin/sh\nfor a in \"$@\"; do printf '[%s]' \"$a\"; done; exit 7\n",
    )
    .unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
    let script = command_args(&command).pop().unwrap();
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("sh -c {script}"))
        .env("HOME", directory.path())
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "[fleet][add][box][ssh://ada@box][it's]"
    );
}

#[test]
fn every_remote_script_looks_where_the_installer_puts_the_cli() {
    let scripts = [
        remote_socket_probe(None),
        remote_daemon_start_script(Path::new("/run/zz.sock")),
        remote_proxy_script(Path::new("/run/zz.sock")),
        remote_install_script(Some(Path::new("/run/zz.sock")), true),
    ];
    for script in scripts {
        let unquoted = script.strip_prefix('\'').unwrap_or(&script);
        assert_eq!(
            unquoted.split_once(" export PATH; ").map(|(path, _)| path),
            Some("PATH=\"$PATH:$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin\";"),
            "{script}"
        );
    }
}

#[cfg(unix)]
#[test]
fn one_windows_login_probes_starts_the_daemon_and_runs_the_proxy() {
    let home = tempfile::tempdir().expect("temporary home");
    let bin = home.path().join("bin");
    fs::create_dir(&bin).expect("bin directory");
    let socket = home.path().join("s.sock");
    let _daemon = std::os::unix::net::UnixListener::bind(&socket).expect("daemon socket");
    let zz = bin.join(zz_protocol::app_identity::DIRECTORY);
    let script = remote_connect_script(Some(&socket), None);
    let run = || {
        Command::new("/bin/sh")
            .args(["-c", &format!("sh -c {}", shell_quote(&script))])
            .env("HOME", home.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .expect("run the connect script")
    };
    let ours = zz_protocol::PROTOCOL_VERSION.to_string();
    for (protocol, proxied) in [(ours.as_str(), true), ("1", false)] {
        fs::write(
            &zz,
            format!(
                "#!/bin/sh\ncase \"$1\" in protocol-version) echo {protocol};; \
                     proxy) echo \"proxy $3\";; esac\n"
            ),
        )
        .expect("write zz");
        fs::set_permissions(&zz, fs::Permissions::from_mode(0o755)).expect("make zz runnable");
        let output = run();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            stdout.contains(&format!("zz-probe-protocol={protocol}\n")),
            "{stdout}"
        );
        assert_eq!(
            stdout.ends_with(&format!("proxy {}\n", socket.display())),
            proxied,
            "{stdout}"
        );
    }
}

#[cfg(unix)]
#[test]
fn remote_probe_and_proxy_select_the_builds_executable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    for name in ["zz", "zz-dev"] {
        let binary = directory.path().join(name);
        fs::write(&binary, format!("#!/bin/sh\nprintf '%s\\n' '{name}'\n"))
            .expect("write executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .expect("executable permissions");
    }
    let home = tempfile::tempdir().expect("temporary home");
    let path = format!("{}:/usr/bin:/bin", directory.path().display());
    let name = zz_protocol::app_identity::DIRECTORY;
    for (script, expected) in [
        (
            remote_socket_probe(None),
            format!("zz-probe-protocol={name}"),
        ),
        (
            shell_quote(&remote_proxy_script(Path::new("/tmp/explicit.sock"))),
            name.to_owned(),
        ),
    ] {
        let output = Command::new("/bin/sh")
            .args(["-c", &format!("sh -c {script}")])
            .env("PATH", &path)
            .env("HOME", home.path())
            .output()
            .expect("run remote script");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(last_line(&output.stdout), expected);
    }
}

#[cfg(unix)]
#[test]
fn remote_scripts_prefer_the_managed_copy_over_an_earlier_path_zz() {
    let name = zz_protocol::app_identity::DIRECTORY;
    let home = tempfile::tempdir().expect("temporary home");
    let package = tempfile::tempdir().expect("package prefix");
    let managed = home.path().join(".local/bin");
    fs::create_dir_all(&managed).expect("managed bin");
    for (directory, label) in [(package.path(), "package"), (managed.as_path(), "managed")] {
        let binary = directory.join(name);
        fs::write(
            &binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = daemon ]; then : > \"$HOME/{label}-ran\"; fi\n\
                     printf '%s\\n' '{label}'\n"
            ),
        )
        .expect("write executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .expect("executable permissions");
    }
    let socket_dir = tempfile::tempdir().expect("socket directory");
    let socket = socket_dir.path().join("zz.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind socket");
    let path = format!("{}:/usr/bin:/bin", package.path().display());
    let run = |script: String| {
        let output = Command::new("/bin/sh")
            .args(["-c", &format!("sh -c {script}")])
            .env("PATH", &path)
            .env("HOME", home.path())
            .output()
            .expect("run remote script");
        assert!(output.status.success(), "{output:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    assert_eq!(
        last_line(run(remote_socket_probe(None)).as_bytes()),
        "zz-probe-protocol=managed"
    );
    assert_eq!(run(shell_quote(&remote_proxy_script(&socket))), "managed\n");
    run(shell_quote(&remote_daemon_start_script(&socket)));
    let started = home.path().join("managed-ran");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !started.exists() && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(started.exists(), "the auto-start ran another zz");
    assert!(!home.path().join("package-ran").exists());
}

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("parent directory")).unwrap();
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn fake_zz(protocol: &str, calls: &str) -> String {
    format!(
        "#!/bin/sh\ncase \"$1\" in protocol-version) echo {protocol} ;; \
             --version) echo \"zz 0.0.1\" ;; *) echo \"$*\" >> \"$HOME/{calls}\" ;; esac\n"
    )
}

#[test]
fn an_install_toast_says_what_happened_on_the_host() {
    let version = env!("CARGO_PKG_VERSION");
    let outcome = |restart, warnings: &[&str]| RemoteInstall {
        installed: RemoteInstalled::Headless,
        restart,
        warnings: warnings
            .iter()
            .map(|warning| (*warning).to_owned())
            .collect(),
    };
    assert_eq!(
        outcome(RemoteRestart::Restarted, &[]).notice("xps"),
        (
            zz_protocol::ClientMessageKind::Success,
            format!("xps: zz {version} installed (headless), daemon restarted")
        )
    );
    assert_eq!(
        outcome(RemoteRestart::NotAsked, &["restart it"]).notice("xps"),
        (
            zz_protocol::ClientMessageKind::Warning,
            format!("xps: zz {version} installed (headless)\nrestart it")
        )
    );
    assert_eq!(
        outcome(
            RemoteRestart::Failed("recovery failed".to_owned()),
            &["restart it"]
        )
        .notice("xps"),
        (
            zz_protocol::ClientMessageKind::Error,
            format!(
                "xps: zz {version} installed (headless), but its daemon did not stop: \
                     recovery failed\nrestart it"
            )
        )
    );
}

#[cfg(unix)]
fn fake_installer(protocol: &str) -> String {
    format!(
        "printf '%s\\n' \"$*\" > \"$HOME/installer-args\"\n\
             zz_body='{body}'\n\
             case \"$*\" in\n\
             \"--headless --version {version}\")\n\
             mkdir -p \"$HOME/.local/bin\"; rm -f \"$HOME/.local/bin/{name}\"\n\
             printf '%s' \"$zz_body\" > \"$HOME/.local/bin/{name}\"\n\
             chmod 755 \"$HOME/.local/bin/{name}\" ;;\n\
             \"--prefix $HOME/.local --version {version}\")\n\
             rm -rf \"$HOME/.local/lib/zz\"; mkdir -p \"$HOME/.local/lib/zz\" \"$HOME/.local/bin\"\n\
             printf '%s' \"$zz_body\" > \"$HOME/.local/lib/zz/cli\"\n\
             chmod 755 \"$HOME/.local/lib/zz/cli\"\n\
             ln -sfn ../lib/zz/cli \"$HOME/.local/bin/{name}\" ;;\n\
             *) echo \"error: unexpected arguments $*\" >&2; exit 1 ;;\n\
             esac\n",
        body = fake_zz(protocol, "calls"),
        version = env!("CARGO_PKG_VERSION"),
        name = zz_protocol::app_identity::DIRECTORY,
    )
}

#[cfg(unix)]
fn run_install_script(
    shell: &str,
    home: &Path,
    path: &str,
    installer: &str,
    restart: bool,
) -> Result<RemoteInstall, EndpointError> {
    let script = shell_quote(&remote_install_script(
        Some(Path::new("/run/zz.sock")),
        restart,
    ));
    let mut child = Command::new(shell)
        .args(["-c", &format!("sh -c {script}")])
        .env("HOME", home)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_unmasked()
        .expect("run install script");
    let _ = child
        .stdin
        .take()
        .expect("install stdin")
        .write_all(installer.as_bytes());
    let output = child.wait_with_output().expect("install output");
    parse_remote_install_output(
        "ssh://desk",
        output.status.code(),
        &output.stdout,
        &output.stderr,
        &output.status.to_string(),
    )
}

#[cfg(unix)]
fn install_shells() -> impl Iterator<Item = &'static str> {
    ["/bin/sh", "/bin/tcsh"]
        .into_iter()
        .filter(|shell| Path::new(shell).exists())
}

#[cfg(unix)]
fn installer_args(home: &Path) -> String {
    fs::read_to_string(home.join("installer-args")).unwrap_or_default()
}

#[cfg(unix)]
#[test]
fn remote_install_replaces_its_own_binary_and_restarts_the_daemon() {
    for shell in install_shells() {
        let home = tempfile::tempdir().expect("temporary home");
        let bin = home.path().join(".local/bin");
        write_executable(
            &bin.join(zz_protocol::app_identity::DIRECTORY),
            "#!/bin/sh\necho 1\n",
        );
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

        let outcome = run_install_script(shell, home.path(), &path, &installer, true)
            .unwrap_or_else(|error| panic!("{shell}: {error}"));
        assert_eq!(
            outcome,
            RemoteInstall {
                installed: RemoteInstalled::Headless,
                restart: RemoteRestart::Restarted,
                warnings: Vec::new(),
            },
            "{shell}",
        );
        assert_eq!(
            installer_args(home.path()),
            format!("--headless --version {}\n", env!("CARGO_PKG_VERSION")),
            "{shell}",
        );
        assert_eq!(
            fs::read_to_string(home.path().join("calls")).unwrap(),
            "--socket /run/zz.sock kill-server\n",
            "{shell}",
        );

        fs::remove_file(home.path().join("calls")).unwrap();
        write_executable(
            &bin.join(zz_protocol::app_identity::DIRECTORY),
            "#!/bin/sh\necho 1\n",
        );
        let outcome = run_install_script(shell, home.path(), &path, &installer, false).unwrap();
        assert_eq!(outcome.restart, RemoteRestart::NotAsked, "{shell}");
        assert!(!home.path().join("calls").exists(), "{shell}");
    }
}

#[cfg(unix)]
#[test]
fn remote_install_puts_its_own_copy_beside_a_zz_it_did_not_install() {
    let home = tempfile::tempdir().expect("temporary home");
    let other = tempfile::tempdir().expect("other prefix");
    let name = zz_protocol::app_identity::DIRECTORY;
    let foreign = other.path().join(name);
    write_executable(&foreign, &fake_zz("1", "foreign-calls"));
    let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

    let path = format!("{}:/usr/bin:/bin", other.path().display());
    run_install_script("/bin/sh", home.path(), &path, &installer, true)
        .expect("a zz elsewhere on PATH gets a managed copy beside it");
    assert_eq!(
        installer_args(home.path()),
        format!("--headless --version {}\n", env!("CARGO_PKG_VERSION")),
    );
    let managed = home.path().join(".local/bin").join(name);
    assert!(fs::symlink_metadata(&managed).unwrap().is_file());
    assert_eq!(
        fs::read_to_string(home.path().join("calls")).unwrap(),
        "--socket /run/zz.sock kill-server\n",
    );
    assert!(!home.path().join("foreign-calls").exists());
    assert_eq!(
        fs::read_to_string(&foreign).unwrap(),
        fake_zz("1", "foreign-calls")
    );
}

#[cfg(unix)]
#[test]
fn remote_install_refreshes_the_desktop_tarball_it_linked() {
    let name = zz_protocol::app_identity::DIRECTORY;
    for shell in install_shells() {
        for absolute in [false, true] {
            let home = tempfile::tempdir().expect("temporary home");
            let lib = home.path().join(".local/lib/zz");
            write_executable(&lib.join("cli"), "#!/bin/sh\necho 1\n");
            write_executable(&lib.join("zz"), "#!/bin/sh\nexit 0\n");
            let bin = home.path().join(".local/bin");
            fs::create_dir_all(&bin).unwrap();
            let link = if absolute {
                lib.join("cli")
            } else {
                PathBuf::from("../lib/zz/cli")
            };
            std::os::unix::fs::symlink(&link, bin.join(name)).unwrap();
            let path = format!("{}:/usr/bin:/bin", bin.display());
            let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

            let outcome = run_install_script(shell, home.path(), &path, &installer, true)
                .unwrap_or_else(|error| panic!("{shell}: {error}"));
            assert_eq!(outcome.installed, RemoteInstalled::Desktop, "{shell}");
            assert_eq!(
                installer_args(home.path()),
                format!(
                    "--prefix {}/.local --version {}\n",
                    home.path().display(),
                    env!("CARGO_PKG_VERSION")
                ),
                "{shell}",
            );
            assert_eq!(
                fs::read_link(bin.join(name)).unwrap(),
                Path::new("../lib/zz/cli"),
                "{shell}",
            );
            assert_eq!(
                fs::read_to_string(home.path().join("calls")).unwrap(),
                "--socket /run/zz.sock kill-server\n",
                "{shell}",
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn remote_install_refuses_a_link_it_did_not_make_and_names_its_target() {
    let home = tempfile::tempdir().expect("temporary home");
    let other = tempfile::tempdir().expect("other prefix");
    let name = zz_protocol::app_identity::DIRECTORY;
    let foreign = other.path().join(name);
    write_executable(&foreign, &fake_zz("1", "foreign-calls"));
    let bin = home.path().join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(&foreign, bin.join(name)).unwrap();
    let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

    let path = format!("{}:/usr/bin:/bin", bin.display());
    let error = run_install_script("/bin/sh", home.path(), &path, &installer, true)
        .expect_err("a link in ~/.local/bin points at someone else's install");
    let EndpointError::RemoteInstallElsewhere {
        path: found, link, ..
    } = &error
    else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(*found, bin.join(name).display().to_string());
    assert_eq!(*link, foreign.display().to_string());
    let reason = error.ssh_reason().unwrap();
    assert!(
        reason.contains(&format!("links to {}", foreign.display())),
        "{reason}"
    );
    assert!(installer_args(home.path()).is_empty());
    assert!(!home.path().join("calls").exists());
    assert!(!home.path().join("foreign-calls").exists());
}

#[cfg(unix)]
fn current_zz(protocol: u16, kill_server: &str) -> String {
    format!(
        "#!/bin/sh\ncase \"$1\" in protocol-version) echo {protocol} ;; \
             --version) echo 'zz {version}' ;; *) echo \"$*\" >> \"$HOME/calls\"; {kill_server} ;; esac\n",
        version = env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(unix)]
#[test]
fn remote_install_skips_the_download_when_its_copy_is_current() {
    for shell in install_shells() {
        let home = tempfile::tempdir().expect("temporary home");
        let bin = home.path().join(".local/bin");
        write_executable(
            &bin.join(zz_protocol::app_identity::DIRECTORY),
            &current_zz(zz_protocol::PROTOCOL_VERSION, "exit 0"),
        );
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let installer = fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string());

        let outcome = run_install_script(shell, home.path(), &path, &installer, false)
            .unwrap_or_else(|error| panic!("{shell}: {error}"));
        assert_eq!(outcome.installed, RemoteInstalled::Current, "{shell}");
        assert_eq!(outcome.restart, RemoteRestart::NotAsked, "{shell}");
        assert!(installer_args(home.path()).is_empty(), "{shell}");
        assert!(!home.path().join("calls").exists(), "{shell}");

        let outcome = run_install_script(shell, home.path(), &path, &installer, true)
            .unwrap_or_else(|error| panic!("{shell}: {error}"));
        assert_eq!(outcome.installed, RemoteInstalled::Current, "{shell}");
        assert_eq!(outcome.restart, RemoteRestart::Restarted, "{shell}");
        assert!(installer_args(home.path()).is_empty(), "{shell}");
        assert_eq!(
            outcome.summary(),
            format!(
                "already {} {}, daemon restarted",
                zz_protocol::app_identity::DIRECTORY,
                env!("CARGO_PKG_VERSION")
            ),
        );

        write_executable(
            &bin.join(zz_protocol::app_identity::DIRECTORY),
            &current_zz(zz_protocol::PROTOCOL_VERSION + 1, "exit 0"),
        );
        let outcome = run_install_script(shell, home.path(), &path, &installer, false)
            .unwrap_or_else(|error| panic!("{shell}: {error}"));
        assert_eq!(
            outcome.installed,
            RemoteInstalled::Headless,
            "{shell}: the same version speaking another protocol is replaced"
        );
        assert_eq!(
            installer_args(home.path()),
            format!("--headless --version {}\n", env!("CARGO_PKG_VERSION")),
            "{shell}",
        );
    }
}

#[cfg(unix)]
#[test]
fn remote_install_reports_a_restart_that_failed_and_one_with_nothing_to_stop() {
    let home = tempfile::tempdir().expect("temporary home");
    let bin = home.path().join(".local/bin");
    let binary = bin.join(zz_protocol::app_identity::DIRECTORY);
    let path = format!("{}:/usr/bin:/bin", bin.display());
    for (kill_server, expected) in [
        (
            "echo 'no server running on /run/zz.sock' >&2; exit 1",
            RemoteRestart::NotRunning,
        ),
        (
            "echo 'error connecting to /run/zz.sock (No such file or directory)' >&2; exit 1",
            RemoteRestart::NotRunning,
        ),
        (
            "echo 'error connecting to /run/zz.sock (Permission denied)' >&2; exit 1",
            RemoteRestart::Failed(
                "error connecting to /run/zz.sock (Permission denied)".to_owned(),
            ),
        ),
        (
            "exit 2",
            RemoteRestart::Failed("kill-server exited with 2".to_owned()),
        ),
        (
            "echo 'zz: daemon I/O error; recovery failed: no daemon pid' >&2; exit 1",
            RemoteRestart::Failed("daemon I/O error; recovery failed: no daemon pid".to_owned()),
        ),
        (
            "echo 'stopping the daemon' >&2; echo 'zz: no daemon pid' >&2; exit 1",
            RemoteRestart::Failed("no daemon pid".to_owned()),
        ),
    ] {
        write_executable(
            &binary,
            &current_zz(zz_protocol::PROTOCOL_VERSION, kill_server),
        );
        let outcome = run_install_script("/bin/sh", home.path(), &path, "exit 1\n", true)
            .expect("the binary is in place whatever the daemon did");
        assert_eq!(outcome.restart, expected, "{kill_server}");
    }

    let stdout = format!(
        "zz-install-current=1\nzz-install-restart=2\nzz-install-restart-error=\n\
             zz-install-protocol={}\n",
        zz_protocol::PROTOCOL_VERSION
    );
    let outcome = parse_remote_install_output(
        "ssh://desk",
        Some(0),
        stdout.as_bytes(),
        b"Last login banner from the profile\nwarning: zz is running from ~/.local\n",
        "exit status: 0",
    )
    .expect("installed");
    assert_eq!(
        outcome.restart,
        RemoteRestart::Failed("kill-server exited with 2".to_owned()),
        "other stderr lines in the session are not kill-server's"
    );
}

#[cfg(unix)]
#[test]
fn remote_install_passes_on_the_installers_warnings() {
    let home = tempfile::tempdir().expect("temporary home");
    let installer = format!(
        "echo 'warning: zz is running from ~/.local; restart it after the install' >&2\n{}",
        fake_installer(&zz_protocol::PROTOCOL_VERSION.to_string())
    );
    let outcome = run_install_script("/bin/sh", home.path(), "/usr/bin:/bin", &installer, false)
        .expect("install");
    assert_eq!(
        outcome.warnings,
        ["zz is running from ~/.local; restart it after the install"]
    );
}

#[cfg(unix)]
#[test]
fn remote_install_reports_why_it_failed() {
    let home = tempfile::tempdir().expect("temporary home");
    let bin = home.path().join(".local/bin");
    let binary = bin.join(zz_protocol::app_identity::DIRECTORY);
    write_executable(&binary, "#!/bin/sh\necho 1\n");
    let path = format!("{}:/usr/bin:/bin", bin.display());

    let error = run_install_script(
        "/bin/sh",
        home.path(),
        &path,
        "echo 'error: curl is missing' >&2; exit 1\n",
        true,
    )
    .expect_err("the installer failed");
    let EndpointError::RemoteInstallFailed { reason, .. } = error else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(reason, "curl is missing");

    let broken = format!(
        "printf '#!/bin/sh\\necho \"GLIBC_2.39 not found\" >&2; exit 1\\n' > \"{}\"\n",
        binary.display()
    );
    let error = run_install_script("/bin/sh", home.path(), &path, &broken, true)
        .expect_err("the installed binary cannot run");
    let EndpointError::RemoteInstallFailed { reason, .. } = error else {
        panic!("unexpected error: {error}");
    };
    assert!(reason.ends_with("GLIBC_2.39 not found"), "{reason}");

    let error = run_install_script("/bin/sh", home.path(), &path, &fake_installer("1"), true)
        .expect_err("the release speaks another protocol");
    assert!(
        matches!(
            error,
            EndpointError::RemoteInstallMismatch { remote: 1, .. }
        ),
        "{error}"
    );
    assert!(!home.path().join("calls").exists());
}

#[cfg(unix)]
#[test]
fn authorized_key_script_appends_once_on_its_own_line() {
    let home = tempfile::tempdir().expect("temporary home");
    let file = home.path().join(".ssh/authorized_keys");
    let run = || {
        let script = authorized_key_script("ssh-ed25519 AAAAzz", "ssh-ed25519 AAAAzz zz-iphone");
        let status = Command::new("/bin/sh")
            .args(["-c", &format!("sh -c {}", shell_quote(&script))])
            .env("HOME", home.path())
            .status()
            .expect("run key script");
        assert!(status.success());
    };
    run();
    assert_eq!(
        fs::metadata(home.path().join(".ssh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "ssh-ed25519 AAAAzz zz-iphone\n"
    );
    run();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "ssh-ed25519 AAAAzz zz-iphone\n"
    );

    fs::write(&file, "ssh-rsa AAAAother laptop").unwrap();
    run();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAzz zz-iphone\n"
    );
}

#[cfg(unix)]
#[test]
fn remote_scripts_fall_back_to_the_mac_app_bundle_cli() {
    let name = zz_protocol::app_identity::DIRECTORY;
    let app = format!("{}.app", zz_protocol::app_identity::DISPLAY_NAME);
    if ["/opt/homebrew/bin", "/usr/local/bin"]
        .iter()
        .any(|dir| Path::new(dir).join(name).exists())
        || Path::new("/Applications").join(&app).exists()
    {
        return;
    }
    let home = tempfile::tempdir().expect("temporary home");
    let macos = home
        .path()
        .join("Applications")
        .join(&app)
        .join("Contents/MacOS");
    fs::create_dir_all(&macos).expect("app bundle");
    let cli = macos.join("cli");
    fs::write(&cli, "#!/bin/sh\nprintf 'bundle %s\\n' \"$1\"\n").expect("write cli");
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).expect("executable permissions");
    let tools = home.path().join("bin");
    fs::create_dir(&tools).expect("tool directory");
    for tool in ["sh", "uname", "getconf"] {
        if let Some(path) = ["/usr/bin", "/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists())
        {
            std::os::unix::fs::symlink(path, tools.join(tool)).expect("tool link");
        }
    }
    for (script, expected) in [
        (
            remote_socket_probe(None),
            "zz-probe-protocol=bundle protocol-version",
        ),
        (
            shell_quote(&remote_proxy_script(Path::new("/tmp/explicit.sock"))),
            "bundle proxy",
        ),
    ] {
        let output = Command::new("/bin/sh")
            .args(["-c", &format!("sh -c {script}")])
            .env("PATH", &tools)
            .env("HOME", home.path())
            .output()
            .expect("run remote script");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(last_line(&output.stdout), expected, "{script}");
    }
}

#[test]
fn autostart_script_starts_the_daemon_on_the_resolved_socket() {
    let script = remote_daemon_start_script(Path::new("/run/user/1000/zz/default.sock"));
    assert_eq!(
        script,
        format!(
            "{}command -v \"$zz_cli\" >/dev/null 2>&1 || exit 127; \
             if setsid true >/dev/null 2>&1; \
             then setsid \"$zz_cli\" daemon --socket '/run/user/1000/zz/default.sock' \
             >/dev/null 2>&1 </dev/null & \
             else nohup \"$zz_cli\" daemon --socket '/run/user/1000/zz/default.sock' \
             >/dev/null 2>&1 </dev/null & fi; \
             if sleep 0.1 2>/dev/null; then delay=0.1; attempts=50; \
             else delay=1; attempts=5; fi; \
             attempt=0; \
             while [ \"$attempt\" -lt \"$attempts\" ]; do \
             if [ -S '/run/user/1000/zz/default.sock' ]; then exit 0; fi; \
             sleep \"$delay\"; attempt=$((attempt + 1)); done; \
             exit 3",
            remote_path_fallback(),
        )
    );
}

#[test]
fn ssh_hosts_may_not_open_with_a_dash() {
    for input in [
        "ssh://-oProxyCommand=id",
        "ssh://user@-oProxyCommand=id",
        "ssh://-",
    ] {
        let error = Endpoint::parse(input).expect_err(input);
        assert!(
            matches!(
                &error,
                EndpointError::UriParse { input: refused, reason }
                    if refused == input && reason == "ssh host must not start with `-`"
            ),
            "{input}: {error:?}"
        );
    }
}

#[test]
fn autostart_script_stays_on_one_line_for_csh_logins() {
    for script in [
        remote_daemon_start_script(Path::new("/run/user/1000/zz/default.sock")),
        remote_install_script(Some(Path::new("/run/user/1000/zz/default.sock")), true),
    ] {
        assert!(!script.contains('\n'), "{script}");
    }
}

#[test]
fn autostart_script_quotes_a_remote_socket_path_containing_a_quote() {
    assert_eq!(shell_quote("o'brien"), "'o'\\''brien'");
    let script = remote_daemon_start_script(Path::new("/tmp/zz-o'brien/default.sock"));
    assert!(
        script.contains("[ -S '/tmp/zz-o'\\''brien/default.sock' ]"),
        "{script}"
    );
}

#[cfg(unix)]
#[test]
fn shell_quoting_survives_the_login_shell_and_the_inner_sh() {
    let path = "/tmp/zz-o'brien $HOME `hostname`/default.sock";
    let script = format!("printf %s {}", shell_quote(path));
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("sh -c {}", shell_quote(&script)))
        .output()
        .expect("sh should run");
    assert_eq!(String::from_utf8_lossy(&output.stdout), path);
}

#[test]
fn classifies_ssh_stderr_into_actionable_hints() {
    for (stderr, expected) in [
        (
            "user@host: Permission denied (publickey).",
            "ssh rejected the login: add your key to the host or start an ssh agent",
        ),
        (
            "Host key verification failed.",
            "known_hosts has no matching key for the host: verify the host before connecting",
        ),
        (
            "ssh: Could not resolve hostname desk: Name or service not known",
            "the host name did not resolve",
        ),
        (
            "ssh: connect to host desk port 22: Connection refused",
            "nothing accepted the ssh connection: start sshd there, or on a Mac turn on Remote \
             Login in System Settings › General › Sharing",
        ),
        (
            "ssh: connect to host desk port 22: Operation timed out",
            "the ssh connection timed out",
        ),
        (
            "ssh: connect to host desk port 22: No route to host",
            "the host is not reachable from this network",
        ),
        (
            "sign_and_send_pubkey: signing failed for ED25519-SK \"/Users/f/.ssh/id_ed25519_sk\": \
                 device not found\nuser@host: Permission denied (publickey).",
            "the security key was not touched",
        ),
        (
            "sign_and_send_pubkey: signing failed for ECDSA-SK \"/k\" from agent: invalid format",
            "the security key was not touched",
        ),
    ] {
        assert_eq!(
            ssh_failure_reason(stderr, "exit status: 255"),
            expected,
            "{stderr:?}"
        );
    }
}

#[test]
fn unclassified_ssh_stderr_falls_back_to_its_last_line_then_the_status() {
    assert_eq!(
        ssh_failure_reason(
            "Warning: Permanently added 'desk' to the list of known hosts.\nbanner exchange \
                 failed\n",
            "exit status: 255",
        ),
        "banner exchange failed"
    );
    assert_eq!(
        ssh_failure_reason("  \n", "exit status: 255"),
        "ssh exited with exit status: 255"
    );
}

#[test]
fn ssh_failures_read_as_advice_and_parse_errors_stay_out_of_the_host_row() {
    let reason = |error: EndpointError| error.ssh_reason().expect("ssh reason");
    assert!(
        reason(EndpointError::RemoteBinaryMissing {
            target: "ssh://desk".to_owned(),
        })
        .starts_with(&format!(
            "{} is not installed on ssh://desk",
            zz_protocol::app_identity::DIRECTORY
        ))
    );
    assert!(
        reason(EndpointError::RemoteDaemonUnavailable {
            target: "ssh://desk".to_owned(),
        })
        .contains("never appeared")
    );
    assert_eq!(
        reason(EndpointError::SshFailed {
            target: "ssh://desk".to_owned(),
            reason: "the ssh connection timed out".to_owned(),
        }),
        "Could not reach ssh://desk over ssh: the ssh connection timed out"
    );
    assert!(
        reason(EndpointError::ForwardTimeout {
            target: "ssh://desk".to_owned(),
            local_socket: PathBuf::from("/tmp/zz-fwd-1-1.sock"),
        })
        .contains("forwarded socket"),
    );
    assert!(parse_error("nope://x", "bad").ssh_reason().is_none());
}

#[cfg(unix)]
#[test]
fn forwarded_socket_path_stays_short() {
    let path = forwarded_local_socket_path().expect("ssh runtime directory");
    assert!(path.as_os_str().as_encoded_bytes().len() < 100, "{path:?}");
    assert_eq!(path.parent(), Some(ssh_runtime_dir().unwrap()));
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("fwd-")
    );
}

#[cfg(unix)]
#[test]
fn each_host_keeps_one_short_forward_beside_its_master() {
    let endpoint = test_ssh_endpoint(Some("ada"), Some(2222));
    let kept = kept_forward_path(&endpoint).unwrap();
    assert_eq!(kept, kept_forward_path(&endpoint.clone()).unwrap());
    assert_eq!(kept.parent(), Some(ssh_runtime_dir().unwrap()));
    assert!(kept.as_os_str().as_encoded_bytes().len() < 100, "{kept:?}");
    let mut master = control_master_path(&endpoint).unwrap().into_os_string();
    master.push(".sock");
    assert_eq!(kept, PathBuf::from(master));

    let mut elsewhere = endpoint.clone();
    elsewhere.remote_socket = Some(PathBuf::from("/run/user/1000/zz/other.sock"));
    assert_ne!(kept_forward_path(&elsewhere).unwrap(), kept);
    assert_eq!(
        control_master_path(&elsewhere).unwrap(),
        control_master_path(&endpoint).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn a_step_riding_a_master_gives_up_but_a_login_may_take_its_time() {
    let directory = tempfile::tempdir().unwrap();
    let step = SshStep {
        endpoint: test_ssh_endpoint(None, None),
        control: Some(SshControl {
            path: directory.path().join("c0"),
            user_persist: false,
        }),
        askpass: None,
        identity: None,
        batch: false,
    };
    let stalled = |_: &SshStep| {
        thread::sleep(Duration::from_secs(30));
        Ok(())
    };
    let slow = |_: &SshStep| {
        thread::sleep(Duration::from_millis(300));
        Ok(())
    };
    let started = std::time::Instant::now();
    let error = step
        .run_within(
            Duration::from_millis(20),
            true,
            "remote socket probe",
            stalled,
        )
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(
        matches!(&error, EndpointError::SshFailed { reason, .. }
            if reason.starts_with("the remote socket probe got no answer")),
        "{error}"
    );
    step.run_within(
        Duration::from_millis(20),
        false,
        "remote socket probe",
        slow,
    )
    .expect("a step without a live master is not time-boxed");
}

#[cfg(unix)]
#[test]
fn only_a_dead_link_remakes_the_kept_forward() {
    for kind in [
        io::ErrorKind::NotFound,
        io::ErrorKind::ConnectionRefused,
        io::ErrorKind::UnexpectedEof,
    ] {
        assert!(
            kept_forward_failed(&DaemonError::Io(kind.into())),
            "{kind:?}"
        );
    }
    assert!(kept_forward_failed(&DaemonError::Server(
        zz_protocol::ServerError::ProtocolMismatch {
            client: zz_protocol::PROTOCOL_VERSION,
            server: 48,
        }
    )));
    assert!(!kept_forward_failed(&DaemonError::Server(
        zz_protocol::ServerError::InvalidCommand("can't find session".to_owned())
    )));
}

#[cfg(unix)]
#[test]
fn ssh_runtime_dir_is_stable_and_private() {
    use std::os::unix::fs::MetadataExt as _;

    let directory = ssh_runtime_dir().expect("ssh runtime directory");
    assert_eq!(ssh_runtime_dir().unwrap(), directory);
    assert_eq!(
        directory == stable_ssh_runtime_dir(),
        private_directory(&stable_ssh_runtime_dir()).is_ok()
    );
    let metadata = fs::symlink_metadata(directory).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.mode() & 0o777, 0o700);

    let root = tempfile::tempdir().unwrap();
    let fresh = root.path().join("zz/ssh");
    private_directory(&fresh).expect("a fresh directory is made private");
    assert_eq!(fs::metadata(&fresh).unwrap().mode() & 0o777, 0o700);
    private_directory(&fresh).expect("the same directory is accepted again");

    let open = root.path().join("open");
    fs::create_dir(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(private_directory(&open).is_err());

    let link = root.path().join("link");
    std::os::unix::fs::symlink(&fresh, &link).unwrap();
    assert!(private_directory(&link).is_err());

    let file = root.path().join("file");
    fs::write(&file, b"").unwrap();
    assert!(private_directory(&file).is_err());
}

#[cfg(unix)]
#[test]
fn only_empty_settled_legacy_directories_are_removed() {
    let root = tempfile::tempdir().unwrap();
    let stale = root.path().join("zz-ssh-Ab12Cd");
    let busy = root.path().join("zz-ssh-Ef34Gh");
    let fresh = root.path().join("zz-ssh-Ij56Kl");
    let live = [
        "zz-ssh-501",
        "zz-ssh-100000",
        "zz-dev-ssh-501",
        "zz-ssh-501-Mn78Op",
        "zz-other",
    ]
    .map(|name| root.path().join(name));
    for directory in [&stale, &busy, &fresh].into_iter().chain(&live) {
        fs::create_dir(directory).unwrap();
    }
    fs::write(busy.join("a1"), b"").unwrap();
    let old = std::time::SystemTime::now() - LEGACY_SSH_DIRECTORY_AGE * 2;
    for directory in [&stale, &busy].into_iter().chain(&live) {
        fs::File::open(directory)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    remove_legacy_ssh_dirs(root.path());
    let mut kept = fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    kept.sort();
    assert_eq!(
        kept,
        [
            "zz-dev-ssh-501",
            "zz-other",
            "zz-ssh-100000",
            "zz-ssh-501",
            "zz-ssh-501-Mn78Op",
            "zz-ssh-Ef34Gh",
            "zz-ssh-Ij56Kl",
        ]
    );
}

#[cfg(unix)]
#[test]
fn user_control_path_is_reused() {
    let config = |master: &str, path: Option<&str>, persist: &str| {
        let path = path.map_or_else(String::new, |path| format!("controlpath {path}\n"));
        format!("user ada\ncontrolmaster {master}\n{path}controlpersist {persist}\nhostname box\n")
    };
    let user_path = "/home/ada/.ssh/cm-f5de03fd";
    for (master, persist, user_persist) in [
        ("auto", "14400", true),
        ("true", "yes", true),
        ("auto", "no", false),
    ] {
        assert_eq!(
            user_control(&config(master, Some(user_path), persist)),
            Some(SshControl {
                path: PathBuf::from(user_path),
                user_persist,
            }),
            "{master} {persist}",
        );
    }
    for master in ["false", "ask", "autoask"] {
        assert_eq!(user_control(&config(master, Some(user_path), "600")), None);
    }
    assert_eq!(user_control(&config("auto", None, "600")), None);
    assert_eq!(user_control(&config("auto", Some("none"), "600")), None);
    assert_eq!(
        user_control(&config("auto", Some("/tmp/zz rv/cm-1"), "600")),
        None
    );
    assert_eq!(user_control(""), None);
}

#[cfg(unix)]
const TEST_CONTROL_PATH: &str = "/tmp/zz-ssh/c0";

#[cfg(unix)]
static TEST_CONTROL: std::sync::LazyLock<SshControl> = std::sync::LazyLock::new(|| SshControl {
    path: PathBuf::from(TEST_CONTROL_PATH),
    user_persist: false,
});

#[cfg(unix)]
fn test_session() -> SshSession<'static> {
    SshSession {
        control: Some(&TEST_CONTROL),
        ..SshSession::default()
    }
}

#[cfg(unix)]
fn shared_ssh_options() -> Vec<String> {
    vec![
        "-o".to_owned(),
        "ConnectTimeout=10".to_owned(),
        "-o".to_owned(),
        "ServerAliveInterval=5".to_owned(),
        "-o".to_owned(),
        "ServerAliveCountMax=3".to_owned(),
        "-o".to_owned(),
        "Compression=yes".to_owned(),
        "-o".to_owned(),
        "ControlMaster=auto".to_owned(),
        "-o".to_owned(),
        format!("ControlPath={TEST_CONTROL_PATH}"),
        "-o".to_owned(),
        format!("ControlPersist={CONTROL_PERSIST_SECONDS}"),
    ]
}

fn ssh_option_cases() -> Vec<(Option<&'static str>, Option<u16>, Vec<&'static str>)> {
    vec![
        (None, None, vec![]),
        (Some("user"), None, vec!["-l", "user"]),
        (None, Some(2222), vec!["-p", "2222"]),
        (Some("user"), Some(2222), vec!["-p", "2222", "-l", "user"]),
    ]
}

fn test_ssh_endpoint(user: Option<&str>, port: Option<u16>) -> SshEndpoint {
    SshEndpoint {
        user: user.map(str::to_owned),
        host: "host".to_owned(),
        port,
        remote_socket: None,
    }
}

fn command_args(command: &Command) -> Vec<String> {
    command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

#[cfg(unix)]
fn probe_socket_path(environment: &[(&str, Option<&str>)]) -> String {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!("sh -c {}", remote_socket_probe(None)));
    for (key, value) in environment {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    let output = command.output().expect("sh should run the probe");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("zz-probe-socket="))
        .map(str::to_owned)
        .expect("the probe should print a socket line")
}

#[cfg(unix)]
#[test]
fn remote_socket_probe_prefers_xdg_runtime_dir_then_tmpdir() {
    for (environment, expected) in [
        (
            [
                ("XDG_RUNTIME_DIR", Some("/run/user/1000")),
                ("TMPDIR", Some("/launchd/tmp")),
                ("USER", Some("ada")),
            ],
            "/run/user/1000/zz/default.sock",
        ),
        (
            [
                ("XDG_RUNTIME_DIR", None),
                ("TMPDIR", Some("/launchd/tmp/")),
                ("USER", Some("ada")),
            ],
            "/launchd/tmp/zz-ada/default.sock",
        ),
        (
            [
                ("XDG_RUNTIME_DIR", None),
                ("TMPDIR", Some("relative")),
                ("USER", Some("ada")),
            ],
            "/tmp/zz-ada/default.sock",
        ),
    ] {
        assert_eq!(
            probe_socket_path(&environment),
            expected.replace("/zz", &format!("/{}", zz_protocol::app_identity::DIRECTORY))
        );
    }
}

#[cfg(unix)]
#[test]
fn remote_socket_probe_resolves_the_launchd_temporary_directory() {
    let expected_root = Command::new("getconf")
        .arg("DARWIN_USER_TEMP_DIR")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|root| root.starts_with('/'))
        .map_or_else(
            || "/tmp".to_owned(),
            |root| root.trim_end_matches('/').to_owned(),
        );
    assert!(
        cfg!(not(target_os = "macos")) || expected_root != "/tmp",
        "{expected_root}"
    );
    assert_eq!(
        probe_socket_path(&[
            ("XDG_RUNTIME_DIR", None),
            ("TMPDIR", None),
            ("USER", Some("ada")),
        ]),
        format!(
            "{expected_root}/{}-ada/default.sock",
            zz_protocol::app_identity::DIRECTORY
        )
    );
}

#[cfg(unix)]
#[test]
fn remote_socket_probe_prefers_existing_macos_desktop_socket() {
    let name = zz_protocol::app_identity::DIRECTORY;
    let desktop = tempfile::Builder::new()
        .prefix(&format!("{name}-"))
        .tempdir_in("/tmp")
        .expect("desktop directory");
    let desktop_name = desktop.path().file_name().unwrap().to_str().unwrap();
    let user = desktop_name.strip_prefix(&format!("{name}-")).unwrap();
    let desktop_socket = desktop.path().join("default.sock");
    let listener = std::os::unix::net::UnixListener::bind(&desktop_socket).expect("desktop socket");
    let temporary = tempfile::tempdir_in("/tmp").expect("SSH temporary directory");
    let ssh_directory = temporary.path().join(desktop_name);
    fs::create_dir(&ssh_directory).expect("SSH socket directory");
    let ssh_socket = ssh_directory.join("default.sock");
    let _ssh_listener = std::os::unix::net::UnixListener::bind(&ssh_socket).expect("SSH socket");
    let uname = temporary.path().join("uname");
    fs::write(&uname, "#!/bin/sh\nprintf 'Darwin\\n'\n").expect("uname stub");
    fs::set_permissions(&uname, fs::Permissions::from_mode(0o755)).expect("stub permissions");
    let path = format!(
        "{}:{}",
        temporary.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut environment = [
        ("XDG_RUNTIME_DIR", None),
        ("TMPDIR", temporary.path().to_str()),
        ("USER", Some(user)),
        ("PATH", Some(path.as_str())),
    ];
    assert_eq!(
        probe_socket_path(&environment),
        desktop_socket.to_str().unwrap()
    );
    environment[1].1 = None;
    assert_eq!(
        probe_socket_path(&environment),
        desktop_socket.to_str().unwrap()
    );
    environment[1].1 = temporary.path().to_str();
    environment[0].1 = Some("/run/zz-test-runtime");
    assert_eq!(
        probe_socket_path(&environment),
        format!("/run/zz-test-runtime/{name}/default.sock")
    );
    environment[0].1 = None;
    fs::write(&uname, "#!/bin/sh\nprintf 'Linux\\n'\n").expect("Linux uname stub");
    assert_eq!(
        probe_socket_path(&environment),
        ssh_socket.to_str().unwrap()
    );
    fs::write(&uname, "#!/bin/sh\nprintf 'Darwin\\n'\n").expect("Darwin uname stub");
    drop(listener);
    fs::remove_file(desktop_socket).expect("remove desktop socket");
    assert_eq!(
        probe_socket_path(&environment),
        ssh_socket.to_str().unwrap()
    );
}

#[cfg(unix)]
#[test]
fn remote_socket_probe_falls_back_to_slash_tmp_when_getconf_rejects_the_variable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let stub = directory.path().join("getconf");
    fs::write(&stub, "#!/bin/sh\necho unsupported >&2\nexit 1\n").expect("getconf stub");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("stub permissions");
    let path = format!(
        "{}:{}",
        directory.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    assert_eq!(
        probe_socket_path(&[
            ("XDG_RUNTIME_DIR", None),
            ("TMPDIR", None),
            ("USER", Some("ada")),
            ("PATH", Some(&path)),
        ]),
        format!(
            "/tmp/{}-ada/default.sock",
            zz_protocol::app_identity::DIRECTORY
        )
    );
}

fn command_envs(command: &Command) -> Vec<(String, Option<String>)> {
    command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

#[cfg(unix)]
fn last_line(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout)
        .lines()
        .last()
        .unwrap_or_default()
        .to_owned()
}
