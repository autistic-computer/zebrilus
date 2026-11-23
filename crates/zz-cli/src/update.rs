use std::{
    io::{self, IsTerminal as _},
    path::Path,
    process::ExitCode,
};

use semver::Version;
use zz_config::{
    owned::{self, BuildInfo},
    update::{Channel, Progress, fetch_latest},
};
use zz_daemon_client::CommandClient;
use zz_protocol::CommandInvocation;

use crate::{CliFailure, daemon_is_missing, exit_code_for};

const USAGE: &str = "usage: zz update [--check | --remove] [--version V | --beta]";

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
        Mode::Install => install(request.version, request.beta),
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
    };
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--check" if request.mode == Mode::Install => request.mode = Mode::Check,
            "--remove" if request.mode == Mode::Install => request.mode = Mode::Remove,
            "--beta" if !request.beta => request.beta = true,
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
    if (request.version.is_some() && request.beta) || (request.mode == Mode::Remove && targeted) {
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

fn install(version: Option<Version>, beta: bool) -> Result<(), String> {
    let version = target(version, beta)?;
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
    let copy = result.map_err(|error| error.to_string())?;
    println!(
        "installed zz {} at {}",
        copy.info.version,
        copy.binary.display()
    );
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
    println!("daemon: {}", daemon(socket_path));
    match target(version, beta) {
        Ok(version) => println!("target: zz {version}"),
        Err(error) => println!("target: unknown, {error}"),
    }
}

fn daemon(socket_path: &Path) -> String {
    let answer = CommandClient::connect(socket_path).and_then(|mut client| {
        client.execute(CommandInvocation::new(
            "display-message",
            ["-p", "#{zz_version} #{zz_revision}"],
        ))
    });
    match answer {
        Ok(output) => match output.trim().split_once(' ') {
            Some((version, revision)) if !version.is_empty() => {
                format!("zz {version}, revision {revision}")
            }
            _ => "running, too old to say its version".to_owned(),
        },
        Err(error) if daemon_is_missing(&error) => "not running".to_owned(),
        Err(error) => error.to_string(),
    }
}
