use std::{
    io::{self, BufRead as _, IsTerminal as _, Write as _},
    path::{Path, PathBuf},
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use semver::Version;
use zz_config::{
    owned::{self, BuildInfo, InstallError},
    update::{Channel, Progress, fetch_latest},
};
use zz_daemon_client::{
    CommandClient, DaemonError, Endpoint, SshEndpoint, TailnetEndpoint,
    classify_local_connect_error,
};
use zz_protocol::{CommandInvocation, RawText};

use crate::{CliFailure, daemon_is_missing, exit_code_for};

const USAGE: &str =
    "usage: zz update [--check | --remove] [--version V | --beta] [--restart [-y]] [-a | host...]";
const THIS_MACHINE: &str = "this machine";
const DAEMON_FACTS: &str = "#{zz_version}|#{zz_revision}|#{server_sessions}|#{zz_exit_rule}|#{zz_owned_version}|#{zz_owned_epoch}";
const SUCCESSOR_WAIT: Duration = Duration::from_secs(30);
const STOPPED: &str = "daemon stopped, the next zz start runs it";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Install,
    Check,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Targets {
    Here,
    Hosts(Vec<String>),
    Everywhere,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    mode: Mode,
    version: Option<Version>,
    beta: bool,
    restart: bool,
    yes: bool,
    targets: Targets,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Daemon {
    Missing,
    Running {
        version: Option<Version>,
        revision: Option<u32>,
        sessions: usize,
        exit_rule: Option<String>,
        owned: Option<Version>,
        owned_epoch: Option<u16>,
    },
    OtherEpoch(Option<u16>),
    Unreachable(String),
}

enum Machine {
    Here(PathBuf),
    Ssh(SshEndpoint),
    Tailnet(TailnetEndpoint),
}

struct Target {
    name: String,
    machine: Result<Machine, String>,
}

#[derive(Clone, Copy)]
struct Run<'a> {
    request: &'a Request,
    host_version: &'a Version,
    here_version: Option<&'a Version>,
    interactive: bool,
}

pub(crate) fn run(arguments: &[String], socket_path: &Path) -> ExitCode {
    let request = match parse(arguments) {
        Ok(request) => request,
        Err(usage) => {
            eprintln!("zz: {usage}");
            return exit_code_for(CliFailure::Usage);
        }
    };
    if zz_protocol::app_identity::DEVELOPMENT {
        eprintln!("zz: zz update does not run on zz Dev");
        return exit_code_for(CliFailure::Runtime);
    }
    let result = match &request.targets {
        Targets::Here => run_here(socket_path, &request),
        Targets::Hosts(names) => run_targets(
            &request,
            &names
                .iter()
                .map(|name| Target {
                    name: name.clone(),
                    machine: crate::configured_host_endpoint(name).map(Machine::from),
                })
                .collect::<Vec<_>>(),
        ),
        Targets::Everywhere => match zz_daemon_client::fleet_hosts_with_tailnet() {
            Ok((hosts, _)) => run_targets(
                &request,
                &std::iter::once(Target {
                    name: THIS_MACHINE.to_owned(),
                    machine: Ok(Machine::Here(socket_path.to_path_buf())),
                })
                .chain(hosts.into_iter().map(|host| Target {
                    name: host.name,
                    machine: Ok(Machine::from(host.endpoint)),
                }))
                .collect::<Vec<_>>(),
            ),
            Err(error) => Err(format!("could not read zz/config: {error}")),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("zz: {error}");
            }
            exit_code_for(CliFailure::Runtime)
        }
    }
}

impl From<Endpoint> for Machine {
    fn from(endpoint: Endpoint) -> Self {
        match endpoint {
            Endpoint::Local(socket) => Self::Here(socket),
            Endpoint::Ssh(endpoint) => Self::Ssh(endpoint),
            Endpoint::Tailnet(endpoint) => Self::Tailnet(endpoint),
        }
    }
}

fn parse(arguments: &[String]) -> Result<Request, String> {
    let mut request = Request {
        mode: Mode::Install,
        version: None,
        beta: false,
        restart: false,
        yes: false,
        targets: Targets::Here,
    };
    let mut hosts = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--check" if request.mode == Mode::Install => request.mode = Mode::Check,
            "--remove" if request.mode == Mode::Install => request.mode = Mode::Remove,
            "--beta" if !request.beta => request.beta = true,
            "--restart" if !request.restart => request.restart = true,
            "-y" if !request.yes => request.yes = true,
            "-a" if request.targets == Targets::Here => request.targets = Targets::Everywhere,
            "--version" if request.version.is_none() => {
                let text = arguments.next().ok_or(USAGE)?;
                let version = Version::parse(text.strip_prefix('v').unwrap_or(text))
                    .map_err(|_| format!("not a zz version: {text}"))?;
                request.version = Some(version);
            }
            host if !host.starts_with('-') && !host.is_empty() => hosts.push(host.to_owned()),
            _ => return Err(USAGE.to_owned()),
        }
    }
    if !hosts.is_empty() {
        if request.targets == Targets::Everywhere {
            return Err(USAGE.to_owned());
        }
        request.targets = Targets::Hosts(hosts);
    }
    let targeted = request.version.is_some() || request.beta;
    if (request.version.is_some() && request.beta)
        || (request.mode == Mode::Remove && targeted)
        || (request.yes && !request.restart)
        || (request.restart && request.mode != Mode::Install)
    {
        return Err(USAGE.to_owned());
    }
    let several = match &request.targets {
        Targets::Here => false,
        Targets::Hosts(hosts) => hosts.len() > 1,
        Targets::Everywhere => true,
    };
    if request.restart && several && !request.yes {
        return Err("--restart on several machines needs -y".to_owned());
    }
    Ok(request)
}

fn bundle_refusal() -> Option<&'static str> {
    let bundled = cfg!(target_os = "macos")
        && std::env::current_exe()
            .and_then(|executable| executable.canonicalize())
            .is_ok_and(|executable| in_app_bundle(&executable));
    bundled.then_some("this Mac's zz updates through the app")
}

fn in_app_bundle(executable: &Path) -> bool {
    let macos = executable.parent();
    macos.and_then(Path::file_name) == Some("MacOS".as_ref())
        && macos.and_then(Path::parent).and_then(Path::file_name) == Some("Contents".as_ref())
}

fn newest(channel: Channel) -> Result<Version, String> {
    match fetch_latest(channel) {
        Ok(Some(release)) => Ok(release.version),
        Ok(None) => Err(format!("found no {} release", channel.label())),
        Err(error) => Err(format!(
            "could not find the newest {} release: {error}",
            channel.label()
        )),
    }
}

fn here_target(request: &Request) -> Result<Version, String> {
    match (&request.version, request.beta) {
        (Some(version), _) => Ok(version.clone()),
        (None, true) => newest(Channel::Beta),
        (None, false) => newest(Channel::of(&BuildInfo::current().version)),
    }
}

fn host_target(request: &Request) -> Result<Version, String> {
    match (&request.version, request.beta) {
        (Some(version), _) => Ok(version.clone()),
        (None, true) => newest(Channel::Beta),
        (None, false) => Ok(BuildInfo::current().version),
    }
}

fn run_here(socket_path: &Path, request: &Request) -> Result<(), String> {
    if request.mode == Mode::Check {
        check_here(socket_path, request);
        return Ok(());
    }
    if let Some(refusal) = bundle_refusal() {
        return Err(refusal.to_owned());
    }
    let line = match request.mode {
        Mode::Remove => remove_here()?,
        Mode::Install | Mode::Check => {
            let version = here_target(request)?;
            install_here(socket_path, &version, request, true)?
        }
    };
    println!("{line}");
    Ok(())
}

fn run_targets(request: &Request, targets: &[Target]) -> Result<(), String> {
    let host_version = host_target(request)?;
    let here_needed = request.mode != Mode::Remove
        && targets
            .iter()
            .any(|target| matches!(target.machine, Ok(Machine::Here(_))));
    let here_version = if here_needed {
        Some(here_target(request)?)
    } else {
        None
    };
    let run = Run {
        request,
        host_version: &host_version,
        here_version: here_version.as_ref(),
        interactive: targets.len() == 1,
    };
    let lines = thread::scope(|scope| {
        targets
            .iter()
            .map(|target| scope.spawn(move || (target.name.as_str(), on_machine(run, target))))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|job| job.join().expect("an update thread panicked"))
            .collect::<Vec<_>>()
    });
    let mut failed = false;
    for (name, line) in lines {
        match line {
            Ok(line) => println!("{}", host_line(name, &line)),
            Err(line) => {
                failed = true;
                eprintln!("{}", host_line(name, &line));
            }
        }
    }
    if failed { Err(String::new()) } else { Ok(()) }
}

fn host_line(name: &str, line: &str) -> String {
    format!("{name}: {line}")
}

fn on_machine(run: Run<'_>, target: &Target) -> Result<String, String> {
    let machine = target.machine.as_ref().map_err(Clone::clone)?;
    let request = run.request;
    let version = match machine {
        Machine::Here(_) => run.here_version.unwrap_or(run.host_version),
        Machine::Ssh(_) | Machine::Tailnet(_) => run.host_version,
    };
    match (machine, request.mode) {
        (Machine::Here(socket), Mode::Check) => Ok(check_line(
            &local_daemon(socket),
            Some(owned::installed().map(|copy| copy.info.version)).as_ref(),
            version,
        )),
        (Machine::Here(socket), mode) => {
            if let Some(refusal) = bundle_refusal() {
                return Ok(format!("skipped, {refusal}"));
            }
            match mode {
                Mode::Remove => remove_here(),
                Mode::Install | Mode::Check => {
                    install_here(socket, version, request, run.interactive)
                }
            }
        }
        (Machine::Ssh(endpoint), Mode::Check) => {
            check_with(&ssh_daemon(endpoint), version, &target.name)
        }
        (Machine::Ssh(endpoint), Mode::Remove) => ssh_update(endpoint, &target.name, &["--remove"]),
        (Machine::Ssh(endpoint), Mode::Install) => {
            ssh_install(endpoint, &target.name, version, run)
        }
        (Machine::Tailnet(endpoint), Mode::Check) => {
            check_with(&tailnet_daemon(endpoint), version, &target.name)
        }
        (Machine::Tailnet(endpoint), Mode::Remove) => update_server(
            &Endpoint::Tailnet(endpoint.clone()),
            &target.name,
            &["--remove"],
            run.interactive,
        ),
        (Machine::Tailnet(endpoint), Mode::Install) => {
            tailnet_install(endpoint, &target.name, version, run)
        }
    }
}

fn install_here(
    socket_path: &Path,
    version: &Version,
    request: &Request,
    interactive: bool,
) -> Result<String, String> {
    let terminal = interactive && io::stderr().is_terminal();
    let mut shown = false;
    let result = owned::install(version, |progress| {
        if let (true, Progress::Downloading { received, total }) = (terminal, progress) {
            let amount = total.filter(|total| *total > 0).map_or_else(
                || format!("{} KiB", received / 1024),
                |total| format!("{}%", received * 100 / total),
            );
            eprint!("\rdownloading zz {version}: {amount}");
            shown = true;
        }
    });
    if shown {
        eprintln!();
    }
    let (head, copy) = match result {
        Ok(copy) => (format!("zz {} installed", copy.info.version), copy.info),
        Err(InstallError::Current(current)) => (
            format!("already zz {current}"),
            owned::installed().map_or_else(BuildInfo::current, |copy| copy.info),
        ),
        Err(error) => return Err(error.to_string()),
    };
    let daemon = local_daemon(socket_path);
    let tail = if request.restart && daemon_behind(&daemon, &copy.version) {
        confirm(
            &restart_notice(THIS_MACHINE, &daemon, Some(copy.epoch)),
            request.yes,
        )?;
        restart_here(socket_path, &daemon)?
    } else {
        daemon_tail(&daemon, &copy.version)
    };
    Ok(format!("{head}; {tail}"))
}

fn remove_here() -> Result<String, String> {
    Ok(match owned::remove().map_err(|error| error.to_string())? {
        Some(info) => format!(
            "removed zz {}; the packaged zz runs from the next start",
            info.version
        ),
        None => "no owned copy to remove".to_owned(),
    })
}

fn check_here(socket_path: &Path, request: &Request) {
    let this = BuildInfo::current().version;
    match crate::handed_over_from() {
        Some(packaged) => {
            println!("this zz: {this}, the owned copy, started by the packaged zz {packaged}");
        }
        None => println!("this zz: {this}"),
    }
    match owned::installed() {
        Some(copy) => println!(
            "owned copy: {} ({})",
            copy.info.to_string().replace('\n', " "),
            copy.binary.display()
        ),
        None => println!("owned copy: none"),
    }
    println!("daemon: {}", daemon_summary(&local_daemon(socket_path)));
    match here_target(request) {
        Ok(version) => println!("target: zz {version}"),
        Err(error) => println!("target: unknown, {error}"),
    }
}

fn restart_here(socket_path: &Path, daemon: &Daemon) -> Result<String, String> {
    if let Daemon::OtherEpoch(_) = daemon {
        zz_daemon_client::terminate_incompatible_daemon(socket_path)
            .map_err(|error| error.to_string())?;
        return Ok(STOPPED.to_owned());
    }
    let endpoint = Endpoint::Local(socket_path.to_path_buf());
    if let Some(version) = restart_server(&endpoint)? {
        return Ok(format!("daemon restarted on zz {version}"));
    }
    let _ = CommandClient::connect(socket_path).and_then(|mut client| {
        client.execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
    });
    Ok(STOPPED.to_owned())
}

fn restart_server(endpoint: &Endpoint) -> Result<Option<Version>, String> {
    let mut client =
        CommandClient::connect_endpoint(endpoint, false).map_err(|error| error.to_string())?;
    let old = client.server_id().ok();
    match client.execute(CommandInvocation::new("restart-server", [] as [&str; 0])) {
        Ok(_) => {}
        Err(error) if lacks_command(&error.to_string(), "restart-server") => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    drop(client);
    let deadline = Instant::now() + SUCCESSOR_WAIT;
    loop {
        let replaced = CommandClient::connect_endpoint(endpoint, true)
            .and_then(|mut client| client.server_id())
            .is_ok_and(|server| Some(server) != old);
        if replaced
            && let Daemon::Running {
                version: Some(version),
                ..
            } = endpoint_daemon(endpoint)
        {
            return Ok(Some(version));
        }
        if Instant::now() >= deadline {
            return Err("the daemon did not come back".to_owned());
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn local_daemon(socket_path: &Path) -> Daemon {
    let answer = CommandClient::connect(socket_path).and_then(|mut client| {
        client.execute(CommandInvocation::new(
            "display-message",
            ["-p", DAEMON_FACTS],
        ))
    });
    match answer {
        Ok(output) => parse_daemon_facts(&output),
        Err(error) if daemon_is_missing(&error) => Daemon::Missing,
        Err(error) => match classify_local_connect_error(socket_path, error) {
            DaemonError::IncompatibleDaemon { daemon, .. } => Daemon::OtherEpoch(daemon),
            DaemonError::DaemonTooOld { .. } => Daemon::OtherEpoch(None),
            error => Daemon::Unreachable(error.to_string()),
        },
    }
}

fn endpoint_daemon(endpoint: &Endpoint) -> Daemon {
    match endpoint {
        Endpoint::Local(socket) => local_daemon(socket),
        Endpoint::Tailnet(endpoint) => tailnet_daemon(endpoint),
        Endpoint::Ssh(endpoint) => ssh_daemon(endpoint),
    }
}

fn tailnet_daemon(endpoint: &TailnetEndpoint) -> Daemon {
    let answer = CommandClient::connect_endpoint(&Endpoint::Tailnet(endpoint.clone()), true)
        .and_then(|mut client| {
            client.execute(CommandInvocation::new(
                "display-message",
                ["-p", DAEMON_FACTS],
            ))
        });
    match answer {
        Ok(output) => parse_daemon_facts(&output),
        Err(error) => Daemon::Unreachable(error.to_string()),
    }
}

fn ssh_daemon(endpoint: &SshEndpoint) -> Daemon {
    let arguments = ["display-message", "-p", DAEMON_FACTS].map(str::to_owned);
    match zz_daemon_client::remote_cli_output(endpoint, &arguments) {
        Ok(output) if output.status.success() => {
            parse_daemon_facts(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if [
                "No such file or directory",
                "Connection refused",
                "no server running",
            ]
            .iter()
            .any(|missing| stderr.contains(missing))
            {
                Daemon::Missing
            } else {
                Daemon::Unreachable(last_line(&stderr, &output.status.to_string()))
            }
        }
        Err(error) => Daemon::Unreachable(error.to_string()),
    }
}

fn parse_daemon_facts(output: &str) -> Daemon {
    let mut fields = output.trim_end_matches('\n').split('|');
    let mut field = || fields.next().filter(|field| !field.is_empty());
    let version = field().and_then(|version| Version::parse(version).ok());
    let revision = field().and_then(|revision| revision.parse().ok());
    let sessions = field()
        .and_then(|sessions| sessions.parse().ok())
        .unwrap_or_default();
    let exit_rule = field().map(str::to_owned);
    let owned = field().and_then(|version| Version::parse(version).ok());
    let owned_epoch = field().and_then(|epoch| epoch.parse().ok());
    Daemon::Running {
        version,
        revision,
        sessions,
        exit_rule,
        owned,
        owned_epoch,
    }
}

fn ssh_install(
    endpoint: &SshEndpoint,
    host: &str,
    version: &Version,
    run: Run<'_>,
) -> Result<String, String> {
    let version = version.to_string();
    let mut arguments = vec!["--version", version.as_str()];
    if run.request.restart {
        if !run.request.yes {
            let daemon = ssh_daemon(endpoint);
            confirm(&restart_notice(host, &daemon, None), false)?;
        }
        arguments.extend(["--restart", "-y"]);
    }
    ssh_update(endpoint, host, &arguments)
}

fn ssh_update(endpoint: &SshEndpoint, host: &str, arguments: &[&str]) -> Result<String, String> {
    let arguments = std::iter::once("update")
        .chain(arguments.iter().copied())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let output = zz_daemon_client::remote_cli_output(endpoint, &arguments)
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        return Ok(last_line(&stdout, "done"));
    }
    Err(too_old(host, &stderr).unwrap_or_else(|| {
        let line = last_line(&stderr, &output.status.to_string());
        line.strip_prefix("zz: ")
            .map_or(line.clone(), str::to_owned)
    }))
}

fn tailnet_install(
    endpoint: &TailnetEndpoint,
    host: &str,
    version: &Version,
    run: Run<'_>,
) -> Result<String, String> {
    let route = Endpoint::Tailnet(endpoint.clone());
    let before = tailnet_daemon(endpoint);
    if let Daemon::Unreachable(error) = before {
        return Err(error);
    }
    let head = update_server(
        &route,
        host,
        &["--version", &version.to_string()],
        run.interactive,
    )?;
    let after = tailnet_daemon(endpoint);
    let owned = match &after {
        Daemon::Running {
            owned: Some(owned), ..
        } => owned.clone(),
        _ => version.clone(),
    };
    if !(run.request.restart && daemon_behind(&before, &owned)) {
        return Ok(format!("{head}; {}", daemon_tail(&before, &owned)));
    }
    let epoch = match after {
        Daemon::Running { owned_epoch, .. } => owned_epoch,
        _ => None,
    };
    confirm(&restart_notice(host, &before, epoch), run.request.yes)?;
    Ok(match restart_server(&route)? {
        Some(version) => format!("{head}; daemon restarted on zz {version}"),
        None => format!("{head}; the daemon there is too old to restart itself, restart it there"),
    })
}

fn update_server(
    endpoint: &Endpoint,
    host: &str,
    arguments: &[&str],
    interactive: bool,
) -> Result<String, String> {
    let mut client =
        CommandClient::connect_endpoint(endpoint, false).map_err(|error| error.to_string())?;
    if interactive && io::stderr().is_terminal() {
        client.set_stdout_handler(print_progress);
    }
    match client.execute(CommandInvocation::new(
        "update-server",
        arguments.iter().copied(),
    )) {
        Ok(output) => Ok(last_line(&output, "done")),
        Err(error) => {
            let error = error.to_string();
            Err(too_old(host, &error).unwrap_or(error))
        }
    }
}

fn print_progress(text: &RawText) {
    eprint!("{text}");
}

fn check_with(daemon: &Daemon, target: &Version, host: &str) -> Result<String, String> {
    match daemon {
        Daemon::Unreachable(error) => Err(too_old(host, error).unwrap_or_else(|| error.clone())),
        daemon => Ok(check_line(daemon, None, target)),
    }
}

fn last_line(text: &str, fallback: &str) -> String {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn too_old(host: &str, error: &str) -> Option<String> {
    (lacks_command(error, "update-server") || lacks_command(error, "update"))
        .then(|| format!("zz on {host} is too old to update itself; update it there once"))
}

fn lacks_command(error: &str, command: &str) -> bool {
    error.split(['\n', ';']).any(|line| {
        line.trim()
            .ends_with(&format!("unknown command: {command}"))
    })
}

fn daemon_behind(daemon: &Daemon, target: &Version) -> bool {
    match daemon {
        Daemon::Running { version, .. } => version.as_ref().is_none_or(|version| version < target),
        Daemon::OtherEpoch(_) => true,
        Daemon::Missing | Daemon::Unreachable(_) => false,
    }
}

fn sessions(count: usize) -> String {
    if count == 1 {
        "1 session".to_owned()
    } else {
        format!("{count} sessions")
    }
}

fn daemon_summary(daemon: &Daemon) -> String {
    match daemon {
        Daemon::Missing => "not running".to_owned(),
        Daemon::Running {
            version: Some(version),
            revision,
            sessions: count,
            ..
        } => format!(
            "zz {version}, revision {}, {}",
            revision.map_or_else(|| "unknown".to_owned(), |revision| revision.to_string()),
            sessions(*count)
        ),
        Daemon::Running {
            version: None,
            sessions: count,
            ..
        } => format!("an older zz, {}", sessions(*count)),
        Daemon::OtherEpoch(Some(epoch)) => format!("on epoch {epoch}"),
        Daemon::OtherEpoch(None) => "on another epoch".to_owned(),
        Daemon::Unreachable(error) => format!("did not answer: {error}"),
    }
}

fn check_line(daemon: &Daemon, owned: Option<&Option<Version>>, target: &Version) -> String {
    let daemon_part = match daemon {
        Daemon::Missing => "no daemon running".to_owned(),
        Daemon::Running {
            version: Some(version),
            revision,
            sessions: count,
            ..
        } => format!(
            "daemon zz {version} (revision {}, {})",
            revision.map_or_else(|| "unknown".to_owned(), |revision| revision.to_string()),
            sessions(*count)
        ),
        Daemon::Running {
            version: None,
            sessions: count,
            ..
        } => format!("daemon on an older zz ({})", sessions(*count)),
        Daemon::OtherEpoch(Some(epoch)) => format!("daemon on epoch {epoch}"),
        Daemon::OtherEpoch(None) => "daemon on another epoch".to_owned(),
        Daemon::Unreachable(error) => format!("daemon did not answer: {error}"),
    };
    let reported = match daemon {
        Daemon::Running {
            version: Some(_),
            owned,
            ..
        } => Some(owned.clone()),
        _ => owned.cloned(),
    };
    let owned_part = match reported {
        Some(Some(version)) => format!("owned zz {version}"),
        Some(None) => "no owned copy".to_owned(),
        None => "owned copy unknown".to_owned(),
    };
    format!("{daemon_part}; {owned_part}; target zz {target}")
}

fn daemon_tail(daemon: &Daemon, target: &Version) -> String {
    match daemon {
        Daemon::Missing => "no daemon running".to_owned(),
        Daemon::Running {
            version: Some(version),
            ..
        } if version >= target => "the daemon runs it".to_owned(),
        Daemon::Running {
            version: Some(version),
            sessions: count,
            ..
        } => format!(
            "daemon still {version} with {}, restart with --restart",
            sessions(*count)
        ),
        Daemon::Running {
            version: None,
            sessions: count,
            ..
        } => format!(
            "daemon still on an older zz with {}, restart with --restart",
            sessions(*count)
        ),
        Daemon::OtherEpoch(_) => "daemon still on another epoch, restart with --restart".to_owned(),
        Daemon::Unreachable(error) => format!("the daemon did not answer: {error}"),
    }
}

fn restart_notice(host: &str, daemon: &Daemon, target_epoch: Option<u16>) -> Vec<String> {
    let mut lines = vec![match daemon {
        Daemon::Running {
            sessions: count, ..
        } => format!("This ends {} on {host}.", sessions(*count)),
        _ => format!("This ends every session on {host}."),
    }];
    let epoch = match daemon {
        Daemon::OtherEpoch(epoch) => *epoch,
        _ => Some(BuildInfo::current().epoch),
    };
    if let (Some(epoch), Some(target)) = (epoch, target_epoch)
        && epoch != target
    {
        lines.push(format!(
            "The desktop app on {host} is on epoch {epoch} and cannot attach to it until it updates."
        ));
    }
    if let Daemon::Running {
        exit_rule: Some(rule),
        ..
    } = daemon
    {
        lines.push(format!(
            "{rule} is on there, so the new daemon will not stay up without a session."
        ));
    }
    lines
}

fn confirm(lines: &[String], yes: bool) -> Result<(), String> {
    if yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err("--restart asks first; pass -y to restart without a terminal".to_owned());
    }
    for line in lines {
        eprintln!("{line}");
    }
    eprint!("Restart the daemon? [y/N] ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|error| error.to_string())?;
    if matches!(answer.trim(), "y" | "Y" | "yes") {
        Ok(())
    } else {
        Err("left the daemon running".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("valid version")
    }

    fn arguments(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn running(version: Option<&str>, sessions: usize) -> Daemon {
        Daemon::Running {
            version: version.map(super::tests::version),
            revision: Some(9),
            sessions,
            exit_rule: None,
            owned: Some(super::tests::version("0.31.0")),
            owned_epoch: Some(126),
        }
    }

    #[test]
    fn update_arguments_pick_the_mode_target_and_machines() {
        let request = |mode, version: Option<&str>, beta, restart, yes, targets| Request {
            mode,
            version: version.map(super::tests::version),
            beta,
            restart,
            yes,
            targets,
        };
        let hosts =
            |names: &[&str]| Targets::Hosts(names.iter().map(|name| (*name).to_owned()).collect());
        let several = Err("--restart on several machines needs -y".to_owned());
        let cases = [
            (
                "",
                Ok(request(
                    Mode::Install,
                    None,
                    false,
                    false,
                    false,
                    Targets::Here,
                )),
            ),
            (
                "--check -a",
                Ok(request(
                    Mode::Check,
                    None,
                    false,
                    false,
                    false,
                    Targets::Everywhere,
                )),
            ),
            (
                "xps --version v0.31.0",
                Ok(request(
                    Mode::Install,
                    Some("0.31.0"),
                    false,
                    false,
                    false,
                    hosts(&["xps"]),
                )),
            ),
            (
                "--beta --restart alienware",
                Ok(request(
                    Mode::Install,
                    None,
                    true,
                    true,
                    false,
                    hosts(&["alienware"]),
                )),
            ),
            (
                "--restart -y xps alienware",
                Ok(request(
                    Mode::Install,
                    None,
                    false,
                    true,
                    true,
                    hosts(&["xps", "alienware"]),
                )),
            ),
            (
                "--remove win-desktop",
                Ok(request(
                    Mode::Remove,
                    None,
                    false,
                    false,
                    false,
                    hosts(&["win-desktop"]),
                )),
            ),
            ("--restart xps alienware", several.clone()),
            ("-a --restart", several),
            ("-a xps", Err(USAGE.to_owned())),
            ("--check --restart", Err(USAGE.to_owned())),
            ("--remove --version 0.31.0", Err(USAGE.to_owned())),
            ("--version 0.31.0 --beta", Err(USAGE.to_owned())),
            ("-y", Err(USAGE.to_owned())),
            ("--host xps", Err(USAGE.to_owned())),
            ("--version next", Err("not a zz version: next".to_owned())),
        ];
        for (line, expected) in cases {
            assert_eq!(parse(&arguments(line)), expected, "zz update {line}");
        }
    }

    #[test]
    fn each_machine_reports_on_one_line() {
        let target = version("0.31.0");
        let cases = [
            (
                host_line(
                    "alienware",
                    &format!(
                        "zz 0.31.0 installed; {}",
                        daemon_tail(&running(Some("0.30.0"), 3), &target)
                    ),
                ),
                "alienware: zz 0.31.0 installed; daemon still 0.30.0 with 3 sessions, restart with --restart",
            ),
            (
                host_line(
                    "xps",
                    &format!(
                        "already zz 0.31.0; {}",
                        daemon_tail(&running(None, 1), &target)
                    ),
                ),
                "xps: already zz 0.31.0; daemon still on an older zz with 1 session, restart with --restart",
            ),
            (
                host_line(
                    "win-desktop",
                    &format!(
                        "zz 0.31.0 installed; {}",
                        daemon_tail(&Daemon::Missing, &target)
                    ),
                ),
                "win-desktop: zz 0.31.0 installed; no daemon running",
            ),
            (
                host_line(
                    "alienware",
                    &check_line(&running(Some("0.30.0"), 3), None, &target),
                ),
                "alienware: daemon zz 0.30.0 (revision 9, 3 sessions); owned zz 0.31.0; target zz 0.31.0",
            ),
            (
                host_line(
                    THIS_MACHINE,
                    &check_line(&Daemon::Missing, Some(&None), &target),
                ),
                "this machine: no daemon running; no owned copy; target zz 0.31.0",
            ),
            (
                host_line(
                    "xps",
                    &check_line(&Daemon::OtherEpoch(Some(124)), None, &target),
                ),
                "xps: daemon on epoch 124; owned copy unknown; target zz 0.31.0",
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(line, expected);
        }
        assert_eq!(
            restart_notice("alienware", &running(Some("0.30.0"), 3), Some(126)),
            [
                "This ends 3 sessions on alienware.".to_owned(),
                format!(
                    "The desktop app on alienware is on epoch {} and cannot attach to it until it updates.",
                    BuildInfo::current().epoch
                ),
            ]
        );
    }

    #[test]
    fn a_host_without_update_server_is_too_old_to_update_itself() {
        let line =
            Some("zz on alienware is too old to update itself; update it there once".to_owned());
        assert_eq!(too_old("alienware", "unknown command: update-server"), line);
        assert_eq!(too_old("alienware", "zz: unknown command: update"), line);
        assert_eq!(
            too_old("alienware", "unknown command: update-servers"),
            None
        );
        assert_eq!(too_old("alienware", "could not reach alienware"), None);
    }
}
