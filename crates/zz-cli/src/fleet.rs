use zz_config::{remove_fleet_host, write_fleet_host};
use zz_daemon_client::{
    Endpoint, RejectedHost, configured_fleet_hosts, install_remote, ssh_public_key, sync_fleet,
    validate_fleet_host,
};

const FLEET_USAGE: &str = "usage: zz fleet add <name> <ssh-destination>\n       zz fleet install [--restart-daemon] <name>\n       zz fleet key\n       zz fleet list [-F <format>] [--json]\n       zz fleet remove <name>\n       zz fleet sync";
const FLEET_INSTALL_USAGE: &str = "usage: zz fleet install [--restart-daemon] <name>";
const FLEET_ADD_USAGE: &str = "usage: zz fleet add <name> <ssh-destination>";
const FLEET_KEY_USAGE: &str = "usage: zz fleet key";
const FLEET_LIST_USAGE: &str = "usage: zz fleet list [-F <format>] [--json]";
const FLEET_REMOVE_USAGE: &str = "usage: zz fleet remove <name>";
const FLEET_SYNC_USAGE: &str = "usage: zz fleet sync";

pub(crate) fn run(arguments: impl IntoIterator<Item = String>) -> Result<String, String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let Some((command, arguments)) = arguments.split_first() else {
        return Err(FLEET_USAGE.to_owned());
    };
    match command.as_str() {
        "add" => fleet_add(&parse_fleet_add(arguments)?),
        "install" => {
            let (name, restart_daemon) = parse_fleet_install(arguments)?;
            fleet_install(name, restart_daemon)
        }
        "key" => {
            if !arguments.is_empty() {
                return Err(FLEET_KEY_USAGE.to_owned());
            }
            ssh_public_key().map_err(|error| format!("could not make the zz key: {error}"))
        }
        "list" => match crate::list_hosts::parse(arguments) {
            Ok(None) => fleet_list(),
            Ok(Some(listing)) => crate::list_hosts::output(listing),
            Err(error) if error.starts_with("usage:") => Err(FLEET_LIST_USAGE.to_owned()),
            Err(error) => Err(error),
        },
        "remove" => {
            let [name] = arguments else {
                return Err(FLEET_REMOVE_USAGE.to_owned());
            };
            Ok(format_forget_outcome(&forget_host(name)?))
        }
        "sync" => {
            if !arguments.is_empty() {
                return Err(FLEET_SYNC_USAGE.to_owned());
            }
            fleet_sync()
        }
        _ => Err(FLEET_USAGE.to_owned()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForgetOutcome {
    pub(crate) name: String,
    pub(crate) endpoint: String,
    pub(crate) config_removed: bool,
}

pub(crate) fn forget_host(name: &str) -> Result<ForgetOutcome, String> {
    let (hosts, rejected) =
        configured_fleet_hosts().map_err(|error| format!("could not read zz/config: {error}"))?;
    let endpoint = hosts
        .iter()
        .find(|host| host.name == name)
        .map(|host| host.endpoint.to_string())
        .or_else(|| {
            rejected
                .iter()
                .find(|host| host.name == name)
                .map(|host| host.value.clone())
        });
    let Some(endpoint) = endpoint else {
        let known = hosts
            .iter()
            .map(|host| host.name.as_str())
            .chain(rejected.iter().map(|host| host.name.as_str()))
            .collect::<Vec<_>>();
        let known = if known.is_empty() {
            "(none)".to_owned()
        } else {
            known.join(", ")
        };
        return Err(format!("unknown fleet host `{name}`; known hosts: {known}"));
    };

    let config_removed = remove_fleet_host(name)
        .map_err(|error| format!("could not remove host-{name} from zz/config: {error}"))?;

    Ok(ForgetOutcome {
        name: name.to_owned(),
        endpoint,
        config_removed,
    })
}

fn fleet_list() -> Result<String, String> {
    let (hosts, rejected) =
        configured_fleet_hosts().map_err(|error| format!("could not read zz/config: {error}"))?;
    let rows = hosts
        .into_iter()
        .map(|host| (host.name, host.endpoint.to_string()))
        .collect::<Vec<_>>();
    Ok(format_fleet_listing(&rows, &rejected))
}

fn format_fleet_listing(rows: &[(String, String)], rejected: &[RejectedHost]) -> String {
    let listing = format_fleet_rows(rows);
    if rejected.is_empty() {
        return listing;
    }
    let dropped = std::iter::once("dropped (`zz fleet remove <name>` deletes these):".to_owned())
        .chain(
            rejected
                .iter()
                .map(|host| format!("  host-{}: {}", host.name, host.reason)),
        )
        .collect::<Vec<_>>()
        .join("\n");
    if listing.is_empty() {
        dropped
    } else {
        format!("{listing}\n\n{dropped}")
    }
}

fn format_fleet_rows(rows: &[(String, String)]) -> String {
    let name_width = rows
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(name, endpoint)| format!("{name:<name_width$}  {endpoint}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_forget_outcome(outcome: &ForgetOutcome) -> String {
    format!(
        "forgot {} ({})\nconfig line removed: {}",
        outcome.name,
        outcome.endpoint,
        yes_no(outcome.config_removed),
    )
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FleetAddArguments {
    name: String,
    endpoint: String,
}

fn parse_fleet_add(arguments: &[String]) -> Result<FleetAddArguments, String> {
    let [name, destination] = arguments else {
        return Err(FLEET_ADD_USAGE.to_owned());
    };
    let destination = destination
        .strip_prefix("ssh://")
        .unwrap_or(destination.as_str());
    if destination.starts_with('-') {
        return Err("ssh destination must not start with `-`".to_owned());
    }
    let endpoint = format!("ssh://{destination}");
    Endpoint::parse(&endpoint).map_err(|error| error.to_string())?;
    validate_fleet_host(name, &endpoint)?;
    Ok(FleetAddArguments {
        name: name.clone(),
        endpoint,
    })
}

fn parse_fleet_install(arguments: &[String]) -> Result<(&str, bool), String> {
    match arguments {
        [name] if !name.starts_with('-') => Ok((name, false)),
        [flag, name] if flag == "--restart-daemon" && !name.starts_with('-') => Ok((name, true)),
        _ => Err(FLEET_INSTALL_USAGE.to_owned()),
    }
}

fn fleet_install(name: &str, restart_daemon: bool) -> Result<String, String> {
    let (hosts, _) =
        configured_fleet_hosts().map_err(|error| format!("could not read zz/config: {error}"))?;
    let Some(host) = hosts.iter().find(|host| host.name == name) else {
        return Err(format!("unknown fleet host `{name}`"));
    };
    let Endpoint::Ssh(endpoint) = &host.endpoint else {
        return Err(format!("host-{name} is not an ssh host"));
    };
    install_remote(endpoint, None, restart_daemon)
        .map_err(|error| error.ssh_reason().unwrap_or_else(|| error.to_string()))?;
    Ok(format!(
        "installed zz {} on {name}",
        env!("CARGO_PKG_VERSION")
    ))
}

fn fleet_sync() -> Result<String, String> {
    let lines = sync_fleet().map_err(|error| format!("fleet sync: {error}"))?;
    let failed = lines.iter().filter(|line| line.outcome.is_err()).count();
    for line in &lines {
        match &line.outcome {
            Ok(text) => println!("{}: {text}", line.name),
            Err(reason) => eprintln!("{}: {reason}", line.name),
        }
    }
    if failed == 0 {
        Ok(String::new())
    } else {
        Err(format!(
            "fleet sync: {failed} of {} machines did not sync",
            lines.len()
        ))
    }
}

fn fleet_add(arguments: &FleetAddArguments) -> Result<String, String> {
    write_fleet_host(&arguments.name, &arguments.endpoint)
        .map_err(|error| format!("could not write zz/config: {error}"))?;
    Ok(format!("host-{} = {}", arguments.name, arguments.endpoint))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, process::Command};

    use super::*;

    #[test]
    fn install_takes_one_host_and_an_optional_restart_flag() {
        let arguments = |values: &[&str]| {
            values
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse_fleet_install(&arguments(&["desk"])),
            Ok(("desk", false))
        );
        assert_eq!(
            parse_fleet_install(&arguments(&["--restart-daemon", "desk"])),
            Ok(("desk", true))
        );
        for rejected in [&[][..], &["-x"], &["desk", "--restart-daemon"], &["a", "b"]] {
            assert_eq!(
                parse_fleet_install(&arguments(rejected)),
                Err(FLEET_INSTALL_USAGE.to_owned()),
                "{rejected:?}"
            );
        }
    }

    #[test]
    fn list_rows_align_on_the_name_column() {
        assert_eq!(
            format_fleet_rows(&[
                ("gpu".to_owned(), "ssh://gpu-box".to_owned()),
                ("desktop".to_owned(), "ssh://desktop".to_owned()),
            ]),
            "gpu      ssh://gpu-box\ndesktop  ssh://desktop"
        );
    }

    #[test]
    fn listing_names_the_lines_the_parser_dropped() {
        assert_eq!(
            format_fleet_listing(
                &[("desktop".to_owned(), "ssh://desktop".to_owned())],
                &[RejectedHost {
                    name: "gpu".to_owned(),
                    value: "quic://gpu:9922".to_owned(),
                    reason: "quic endpoints were removed; use ssh://".to_owned(),
                }],
            ),
            "desktop  ssh://desktop\n\ndropped (`zz fleet remove <name>` deletes these):\n  \
             host-gpu: quic endpoints were removed; use ssh://"
        );
        assert_eq!(
            format_fleet_listing(&[("desktop".to_owned(), "ssh://desktop".to_owned())], &[]),
            "desktop  ssh://desktop"
        );
    }

    #[test]
    fn add_arguments_become_an_ssh_endpoint_and_reject_bad_names() {
        assert_eq!(
            parse_fleet_add(&["desktop".to_owned(), "fabrico@arch-desktop".to_owned()]).unwrap(),
            FleetAddArguments {
                name: "desktop".to_owned(),
                endpoint: "ssh://fabrico@arch-desktop".to_owned(),
            }
        );
        assert_eq!(
            parse_fleet_add(&["gpu".to_owned(), "gpu-box:2222".to_owned()])
                .unwrap()
                .endpoint,
            "ssh://gpu-box:2222"
        );
        assert_eq!(
            parse_fleet_add(&["gpu".to_owned(), "ssh://gpu-box".to_owned()])
                .unwrap()
                .endpoint,
            "ssh://gpu-box"
        );

        assert_eq!(
            parse_fleet_add(&["local".to_owned(), "gpu-box".to_owned()]).unwrap_err(),
            "invalid `host-local`: host name `local` is reserved"
        );
        assert!(parse_fleet_add(&["gpu".to_owned(), "user@@gpu-box".to_owned()]).is_err());
        assert!(parse_fleet_add(&["gpu".to_owned(), "gpu-box:0".to_owned()]).is_err());
        assert!(parse_fleet_add(&["gpu".to_owned()]).is_err());
    }

    #[test]
    fn add_list_and_remove_round_trip_through_the_config_file() {
        const TEST_NAME: &str =
            "fleet::tests::add_list_and_remove_round_trip_through_the_config_file";
        if std::env::var("ZZ_FLEET_TEST_CHILD").as_deref() == Ok(TEST_NAME) {
            run_config_round_trip_test();
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(TEST_NAME)
            .arg("--nocapture")
            .env("ZZ_FLEET_TEST_CHILD", TEST_NAME)
            .env("ZZ_DATA_DIR", directory.path().join("data"))
            .env("XDG_CONFIG_HOME", directory.path().join("config"))
            .env("XDG_DATA_HOME", directory.path().join("share"))
            .env("HOME", directory.path().join("home"))
            .status()
            .unwrap();
        assert!(status.success(), "isolated fleet add test failed");
    }

    fn run_config_round_trip_test() {
        let config_home =
            PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").expect("test config home"));
        let config_path = config_home.join("zz/config");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "# keep\nhost-desktop = ssh://old-desktop # keep too\ntray = false\n",
        )
        .unwrap();

        for _ in 0..2 {
            let output = run([
                "add".to_owned(),
                "desktop".to_owned(),
                "fabrico@arch-desktop".to_owned(),
            ])
            .unwrap();
            assert_eq!(output, "host-desktop = ssh://fabrico@arch-desktop");
        }
        assert_eq!(
            fs::read_to_string(&config_path).unwrap(),
            "# keep\nhost-desktop = ssh://fabrico@arch-desktop # keep too\ntray = false\n"
        );

        let key = run(["key".to_owned()]).unwrap();
        assert!(
            key.starts_with("ssh-ed25519 ") && key.contains(" zz@"),
            "{key}"
        );
        assert_eq!(run(["key".to_owned()]).unwrap(), key);
        assert_eq!(
            run(["key".to_owned(), "-f".to_owned()]).unwrap_err(),
            FLEET_KEY_USAGE
        );

        assert_eq!(
            run(["list".to_owned()]).unwrap(),
            "desktop  ssh://fabrico@arch-desktop"
        );
        assert_eq!(
            run([
                "list".to_owned(),
                "-F".to_owned(),
                "#{host_name}=#{host_endpoint}#{?host_name,!,}".to_owned()
            ])
            .unwrap(),
            "desktop=ssh://fabrico@arch-desktop!"
        );
        assert_eq!(
            run(["list".to_owned(), "-F".to_owned()]).unwrap_err(),
            FLEET_LIST_USAGE
        );
        assert_eq!(
            run(["remove".to_owned(), "missing".to_owned()]).unwrap_err(),
            "unknown fleet host `missing`; known hosts: desktop"
        );

        let removed = run(["remove".to_owned(), "desktop".to_owned()]).unwrap();
        assert!(removed.contains("config line removed: yes"));
        assert_eq!(
            fs::read_to_string(&config_path).unwrap(),
            "# keep\ntray = false\n"
        );
        assert_eq!(run(["list".to_owned()]).unwrap(), "");
        assert_eq!(
            run(["sync".to_owned()]).unwrap_err(),
            "fleet sync: zz/config lists no ssh hosts; add one with zz fleet add NAME DEST"
        );
        assert_eq!(
            run(["sync".to_owned(), "xps".to_owned()]).unwrap_err(),
            FLEET_SYNC_USAGE
        );

        assert_eq!(
            run(["remove".to_owned(), "desktop".to_owned()]).unwrap_err(),
            "unknown fleet host `desktop`; known hosts: (none)"
        );

        fs::write(
            &config_path,
            "# keep\nhost-gpu = quic://gpu:9922\ntray = false\n",
        )
        .unwrap();
        assert_eq!(
            run(["list".to_owned()]).unwrap(),
            "dropped (`zz fleet remove <name>` deletes these):\n  host-gpu: invalid endpoint URI \
             `quic://gpu:9922`: quic endpoints were removed; use ssh://"
        );
        assert_eq!(
            run(["remove".to_owned(), "missing".to_owned()]).unwrap_err(),
            "unknown fleet host `missing`; known hosts: gpu"
        );
        assert_eq!(
            run(["remove".to_owned(), "gpu".to_owned()]).unwrap(),
            "forgot gpu (quic://gpu:9922)\nconfig line removed: yes"
        );
        assert_eq!(
            fs::read_to_string(&config_path).unwrap(),
            "# keep\ntray = false\n"
        );
    }
}
