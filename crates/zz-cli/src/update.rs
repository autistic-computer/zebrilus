use std::{
    io::{self, BufRead as _, IsTerminal as _, Write as _},
    path::Path,
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use semver::Version;
use zz_config::{
    owned::{self, BuildInfo, InstallError},
    update::{Channel, Progress, fetch_latest},
};
use zz_daemon_client::{CommandClient, DaemonError, classify_local_connect_error};
use zz_protocol::CommandInvocation;

use crate::{CliFailure, daemon_is_missing, exit_code_for};

const USAGE: &str = "usage: zz update [--check | --remove] [--version V | --beta] [--restart [-y]]";
const THIS_MACHINE: &str = "this machine";
const DAEMON_FACTS: &str = "#{zz_version}|#{zz_revision}|#{server_sessions}|#{zz_exit_rule}";
const SUCCESSOR_WAIT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Install,
    Check,
    Remove,
}

#[derive(Debug, PartialEq, Eq)]
struct Request {
    mode: Mode,
    version: Option<Version>,
    beta: bool,
    restart: bool,
    yes: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Daemon {
    Missing,
    Running {
        version: Option<Version>,
        revision: Option<u32>,
        sessions: usize,
        exit_rule: Option<String>,
    },
    OtherEpoch(Option<u16>),
    Unreachable(String),
}

pub(crate) fn run(arguments: &[String], socket_path: &Path) -> ExitCode {
    let request = match parse(arguments) {
        Ok(request) => request,
        Err(usage) => {
            eprintln!("zz: {usage}");
            return exit_code_for(CliFailure::Usage);
        }
    };
    if let Some(refusal) = refusal(request.mode) {
        eprintln!("zz: {refusal}");
        return exit_code_for(CliFailure::Runtime);
    }
    let result = match request.mode {
        Mode::Install => install(socket_path, &request),
        Mode::Check => {
            check(socket_path, request.version, request.beta);
            Ok(())
        }
        Mode::Remove => remove(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zz: {error}");
            exit_code_for(CliFailure::Runtime)
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
    };
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--check" if request.mode == Mode::Install => request.mode = Mode::Check,
            "--remove" if request.mode == Mode::Install => request.mode = Mode::Remove,
            "--beta" if !request.beta => request.beta = true,
            "--restart" if !request.restart => request.restart = true,
            "-y" if !request.yes => request.yes = true,
            "--version" if request.version.is_none() => {
                let text = arguments.next().ok_or(USAGE)?;
                let version = Version::parse(text.strip_prefix('v').unwrap_or(text))
                    .map_err(|_| format!("not a zz version: {text}"))?;
                request.version = Some(version);
            }
            _ => return Err(USAGE.to_owned()),
        }
    }
    let targeted = request.version.is_some() || request.beta;
    if (request.version.is_some() && request.beta)
        || (request.mode == Mode::Remove && targeted)
        || (request.yes && !request.restart)
        || (request.restart && request.mode != Mode::Install)
    {
        return Err(USAGE.to_owned());
    }
    Ok(request)
}

fn refusal(mode: Mode) -> Option<&'static str> {
    if zz_protocol::app_identity::DEVELOPMENT {
        return Some("zz update does not run on zz Dev");
    }
    let bundled = mode != Mode::Check
        && cfg!(target_os = "macos")
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

fn target(version: Option<Version>, beta: bool) -> Result<Version, String> {
    if let Some(version) = version {
        return Ok(version);
    }
    let channel = if beta {
        Channel::Beta
    } else {
        Channel::of(&BuildInfo::current().version)
    };
    match fetch_latest(channel) {
        Ok(Some(release)) => Ok(release.version),
        Ok(None) => Err(format!("found no {} release", channel.label())),
        Err(error) => Err(format!(
            "could not find the newest {} release: {error}",
            channel.label()
        )),
    }
}

fn install(socket_path: &Path, request: &Request) -> Result<(), String> {
    let version = target(request.version.clone(), request.beta)?;
    let terminal = io::stderr().is_terminal();
    let mut shown = false;
    let result = owned::install(&version, |progress| {
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
        restart_local(socket_path, &daemon, &copy, request.yes)?
    } else {
        daemon_tail(&daemon, &copy.version)
    };
    println!("{head}; {tail}");
    Ok(())
}

fn remove() -> Result<(), String> {
    match owned::remove().map_err(|error| error.to_string())? {
        Some(info) => println!(
            "removed zz {}; the packaged zz runs from the next start",
            info.version
        ),
        None => println!("no owned copy to remove"),
    }
    Ok(())
}

fn check(socket_path: &Path, version: Option<Version>, beta: bool) {
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
    match target(version, beta) {
        Ok(version) => println!("target: zz {version}"),
        Err(error) => println!("target: unknown, {error}"),
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

fn parse_daemon_facts(output: &str) -> Daemon {
    let mut fields = output.trim_end_matches('\n').split('|');
    let mut field = || fields.next().filter(|field| !field.is_empty());
    let version = field().and_then(|version| Version::parse(version).ok());
    let revision = field().and_then(|revision| revision.parse().ok());
    let sessions = field()
        .and_then(|sessions| sessions.parse().ok())
        .unwrap_or_default();
    let exit_rule = field().map(str::to_owned);
    Daemon::Running {
        version,
        revision,
        sessions,
        exit_rule,
    }
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

fn restart_notice(host: &str, daemon: &Daemon, target: &BuildInfo) -> Vec<String> {
    let mut lines = vec![match daemon {
        Daemon::Running {
            sessions: count, ..
        } => {
            format!("This ends {} on {host}.", sessions(*count))
        }
        _ => format!("This ends every session on {host}."),
    }];
    let epoch = match daemon {
        Daemon::OtherEpoch(epoch) => *epoch,
        _ => Some(BuildInfo::current().epoch),
    };
    if let Some(epoch) = epoch.filter(|epoch| *epoch != target.epoch) {
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

fn restart_local(
    socket_path: &Path,
    daemon: &Daemon,
    target: &BuildInfo,
    yes: bool,
) -> Result<String, String> {
    confirm(&restart_notice(THIS_MACHINE, daemon, target), yes)?;
    if let Daemon::OtherEpoch(_) = daemon {
        zz_daemon_client::terminate_incompatible_daemon(socket_path)
            .map_err(|error| error.to_string())?;
        return Ok("daemon stopped, the next zz start runs it".to_owned());
    }
    let mut client = CommandClient::connect(socket_path).map_err(|error| error.to_string())?;
    let old = client.server_id().ok();
    match client.execute(CommandInvocation::new("restart-server", [] as [&str; 0])) {
        Ok(_) => {}
        Err(error) if lacks_command(&error.to_string(), "restart-server") => {
            let _ = CommandClient::connect(socket_path).and_then(|mut client| {
                client.execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
            });
            return Ok("daemon stopped, the next zz start runs it".to_owned());
        }
        Err(error) => return Err(error.to_string()),
    }
    let deadline = Instant::now() + SUCCESSOR_WAIT;
    loop {
        let replaced = CommandClient::connect(socket_path)
            .and_then(|mut client| client.server_id())
            .is_ok_and(|server| Some(server) != old);
        if replaced
            && let Daemon::Running {
                version: Some(version),
                ..
            } = local_daemon(socket_path)
        {
            return Ok(format!("daemon restarted on zz {version}"));
        }
        if Instant::now() >= deadline {
            return Err("the daemon did not come back on its socket".to_owned());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn lacks_command(error: &str, command: &str) -> bool {
    error.contains(&format!("unknown command: {command}"))
}
