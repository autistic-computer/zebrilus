use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{self, BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use zz_client::driver_log::{ChatLine, ChatRole, DriverLog, packet};
use zz_daemon_client::{
    CommandClient, InteractiveClient, SSH_BATCH_ENVIRONMENT_VARIABLE, configured_fleet_hosts,
    shell_quote, short_device_name, unmasked::SpawnUnmasked as _,
};
use zz_protocol::{
    CommandInvocation, EventPayload, PaneId, ProtocolMessage, agent_stream::AgentStreamItem,
    app_identity,
};

pub const SESSION: &str = "driver";
pub const GENERATION_VARIABLE: &str = "ZZ_DRIVER_GENERATION";
const SKILL: &str = include_str!("../../../.agents/skills/zz-driver/SKILL.md");
const USAGE: &str = "usage: zz driver state [--brief] [--json] [--since GEN:SEQ] [--follow] | keep \
                     | start [--command CMD] [--generation N] [--from HOST] \
                     | recover [--force] [--on HOSTS] [--command CMD] | stop-hook";
const DEFAULT_COMMAND: &str = "claude";
const PROVIDERS: [&str; 2] = ["claude", "codex"];
const RECORD: &str = "record.json";
const STREAM: &str = "stream.jsonl";
const INSTRUCTIONS: &str = "DRIVER.md";
const SETTINGS: &str = ".driver-settings.json";
const PACKET: &str = ".packet";
const PLACEHOLDER: &str = "lens-";
const LEDGER_FILE: &str = "ledger.md";
const HOOK_TIMEOUT: u64 = 30;
const CODEX_HOOK_KEY: &str = "/<session-flags>/config.toml:stop:0:0";
const LOGIN_PATH: &str = "login_path=$(\"${SHELL:-/bin/sh}\" -lc env 2>/dev/null | sed -n \
                          's/^PATH=//p'); [ -n \"$login_path\" ] && PATH=$login_path; \
                          PATH=\"${ZZ_TMUX_SHIM_DIR:+$ZZ_TMUX_SHIM_DIR:}$PATH:$HOME/.local/bin\"; \
                          export PATH; ";
const RECHECK: Duration = Duration::from_secs(10);
const IDLE: Duration = Duration::from_secs(60);
const SILENCE: Duration = Duration::from_secs(45);
const HEARTBEAT: Duration = Duration::from_secs(10);
const TAIL: Duration = Duration::from_millis(500);
const LEDGER_NOT_WRITTEN: &str = "You acted this turn but did not rewrite ledger.md. Rewrite it \
                                  whole, in the shape your instructions give, before you end the \
                                  turn.";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Record {
    pub generation: u64,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub session_base: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Brief {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    pub host: String,
    pub generation: u64,
    pub seq: u64,
    pub alive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl Brief {
    #[must_use]
    pub const fn position(&self) -> (u64, u64) {
        (self.generation, self.seq)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum Recovery {
    Alive {
        host: String,
        pane: Option<String>,
    },
    Started {
        generation: u64,
        from: Option<String>,
        on: Option<String>,
    },
    Unreachable {
        answered: usize,
        hosts: usize,
    },
}

#[derive(Deserialize)]
struct RecordLine {
    record: Record,
}

#[derive(Deserialize, Serialize)]
struct Heartbeat {
    alive: bool,
}

struct LocalDriver {
    pane: PaneId,
    provider: &'static str,
    cwd: String,
}

pub(crate) fn run(socket: &Path, arguments: &[String]) -> ExitCode {
    let (verb, rest) = arguments
        .split_first()
        .map_or(("", &[][..]), |(verb, rest)| (verb.as_str(), rest));
    let result = match verb {
        "state" => state(socket, rest),
        "keep" if rest.is_empty() => detach_keeper(socket),
        "keep" if rest == ["--here"] => keep(socket),
        "start" => options(rest).and_then(|options| {
            start(
                socket,
                options.generation,
                options.from.as_deref(),
                &options.command,
            )
            .map(|generation| println!("started generation {generation}"))
        }),
        "recover" => options(rest).and_then(|options| {
            recover(socket, options.force, &options.command, &options.on).map(|recovery| {
                println!("{}", serde_json::to_string(&recovery).unwrap_or_default());
            })
        }),
        "stop-hook" if rest.is_empty() => stop_hook(),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error == USAGE => {
            eprintln!("zz: {USAGE}");
            crate::exit_code_for(crate::CliFailure::Usage)
        }
        Err(error) => {
            eprintln!("zz driver: {error}");
            crate::exit_code_for(crate::CliFailure::Runtime)
        }
    }
}

struct Options {
    command: String,
    generation: Option<u64>,
    from: Option<String>,
    force: bool,
    on: Vec<String>,
}

fn options(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        command: DEFAULT_COMMAND.to_owned(),
        generation: None,
        from: None,
        force: false,
        on: Vec::new(),
    };
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let mut value = || arguments.next().cloned().ok_or_else(|| USAGE.to_owned());
        match argument.as_str() {
            "--command" => options.command = value()?,
            "--from" => options.from = Some(value()?),
            "--generation" => {
                options.generation = Some(value()?.parse().map_err(|_| USAGE.to_owned())?);
            }
            "--force" => options.force = true,
            "--on" => {
                options.on = value()?
                    .split(',')
                    .filter(|host| !host.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            _ => return Err(USAGE.to_owned()),
        }
    }
    Ok(options)
}

pub fn directory() -> io::Result<PathBuf> {
    app_identity::platform_data_dir()
        .map(|data| data.join(app_identity::DIRECTORY).join(SESSION))
        .ok_or_else(|| io::Error::other("this machine has no data directory"))
}

fn read_record(directory: &Path) -> Option<Record> {
    serde_json::from_slice(&fs::read(directory.join(RECORD)).ok()?).ok()
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

fn write_record(directory: &Path, record: &Record) -> io::Result<()> {
    write_atomically(
        &directory.join(RECORD),
        &serde_json::to_vec(record).map_err(io::Error::other)?,
    )
}

fn read_lines(directory: &Path) -> Vec<ChatLine> {
    fs::read_to_string(directory.join(STREAM))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn encode(lines: &[ChatLine]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in lines {
        if let Ok(json) = serde_json::to_vec(line) {
            bytes.extend_from_slice(&json);
            bytes.push(b'\n');
        }
    }
    bytes
}

fn append_lines(directory: &Path, lines: &[ChatLine]) -> io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(STREAM))?
        .write_all(&encode(lines))
}

fn last_position(lines: &[ChatLine]) -> (u64, u64) {
    lines.last().map(ChatLine::position).unwrap_or_default()
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or_default();
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn local_driver(socket: &Path) -> Option<LocalDriver> {
    let mut client = CommandClient::connect(socket).ok()?;
    let listing = client
        .execute(CommandInvocation::new(
            "list-panes",
            [
                "-a",
                "-F",
                "#{pane_id} #{pane_current_command} #{session_name}",
            ],
        ))
        .ok()?;
    let (pane, provider) = listing.lines().find_map(|line| {
        let mut fields = line.splitn(3, ' ');
        let (pane, command, session) = (fields.next()?, fields.next()?, fields.next()?);
        if session != SESSION {
            return None;
        }
        let provider = PROVIDERS
            .into_iter()
            .find(|provider| command == *provider)?;
        Some((pane.to_owned(), provider))
    })?;
    let cwd = client
        .execute(CommandInvocation::new(
            "display-message",
            ["-p", "-t", pane.as_str(), "#{pane_current_path}"],
        ))
        .unwrap_or_default();
    Some(LocalDriver {
        pane: PaneId(pane.strip_prefix('%')?.parse().ok()?),
        provider,
        cwd: cwd.trim().to_owned(),
    })
}

fn local_session_exists(socket: &Path) -> bool {
    CommandClient::connect(socket)
        .and_then(|mut client| {
            client.execute(CommandInvocation::new(
                "list-sessions",
                ["-F", "#{session_name}"],
            ))
        })
        .is_ok_and(|listing| listing.lines().any(|name| name == SESSION))
}

fn host_name() -> String {
    short_device_name().unwrap_or_else(|| "localhost".to_owned())
}

#[must_use]
pub fn brief(socket: &Path) -> Brief {
    let directory = directory().ok();
    let record = directory.as_deref().and_then(read_record);
    let last = directory
        .as_deref()
        .map(|directory| last_position(&read_lines(directory)))
        .unwrap_or_default();
    let generation = record
        .as_ref()
        .map_or(0, |record| record.generation)
        .max(last.0);
    let driver = local_driver(socket);
    Brief {
        host_name: None,
        host: host_name(),
        generation,
        seq: if last.0 == generation { last.1 } else { 0 },
        alive: driver.is_some(),
        pane: driver.as_ref().map(|driver| format!("%{}", driver.pane.0)),
        provider: driver.map(|driver| driver.provider.to_owned()),
    }
}

fn parse_since(value: &str) -> Option<(u64, u64)> {
    let (generation, seq) = value.split_once(':')?;
    Some((generation.parse().ok()?, seq.parse().ok()?))
}

fn state(socket: &Path, arguments: &[String]) -> Result<(), String> {
    let mut brief_only = false;
    let mut follow = false;
    let mut since = (0, 0);
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--brief" => brief_only = true,
            "--json" => {}
            "--follow" => follow = true,
            "--since" => {
                since = arguments
                    .next()
                    .and_then(|value| parse_since(value))
                    .ok_or_else(|| USAGE.to_owned())?;
            }
            _ => return Err(USAGE.to_owned()),
        }
    }
    if brief_only {
        println!(
            "{}",
            serde_json::to_string(&brief(socket)).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let directory = directory().map_err(|error| error.to_string())?;
    let mut output = io::stdout().lock();
    let mut record = read_record(&directory);
    if let Some(record) = &record {
        print_record(&mut output, record).map_err(|error| error.to_string())?;
    }
    let bytes = fs::read(directory.join(STREAM)).unwrap_or_default();
    let mut printed = since;
    for line in bytes.split(|byte| *byte == b'\n') {
        if let Ok(chat) = serde_json::from_slice::<ChatLine>(line)
            && chat.position() > printed
        {
            printed = chat.position();
            output
                .write_all(line)
                .and_then(|()| output.write_all(b"\n"))
                .map_err(|error| error.to_string())?;
        }
    }
    output.flush().map_err(|error| error.to_string())?;
    if !follow {
        return Ok(());
    }
    let mut offset = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let mut partial = Vec::new();
    let mut beat = Instant::now();
    loop {
        thread::sleep(TAIL);
        let length = fs::metadata(directory.join(STREAM)).map_or(0, |metadata| metadata.len());
        if length < offset {
            offset = 0;
            partial.clear();
        }
        if length > offset
            && let Ok(mut file) = File::open(directory.join(STREAM))
        {
            let mut fresh = Vec::new();
            file.seek(SeekFrom::Start(offset))
                .and_then(|_| file.read_to_end(&mut fresh))
                .map_err(|error| error.to_string())?;
            offset += u64::try_from(fresh.len()).unwrap_or_default();
            partial.extend_from_slice(&fresh);
            while let Some(end) = partial.iter().position(|byte| *byte == b'\n') {
                let line = partial.drain(..=end).collect::<Vec<_>>();
                if let Ok(chat) = serde_json::from_slice::<ChatLine>(&line[..end])
                    && chat.position() > printed
                {
                    printed = chat.position();
                    output.write_all(&line).map_err(|error| error.to_string())?;
                }
            }
        }
        let current = read_record(&directory);
        if current != record
            && let Some(current) = &current
        {
            print_record(&mut output, current).map_err(|error| error.to_string())?;
        }
        record = current;
        if beat.elapsed() >= HEARTBEAT {
            beat = Instant::now();
            let heartbeat = Heartbeat {
                alive: local_driver(socket).is_some(),
            };
            writeln!(
                output,
                "{}",
                serde_json::to_string(&heartbeat).unwrap_or_default()
            )
            .map_err(|error| error.to_string())?;
        }
        output.flush().map_err(|error| error.to_string())?;
    }
}

fn print_record(output: &mut impl io::Write, record: &Record) -> io::Result<()> {
    writeln!(output, "{}", serde_json::json!({ "record": record }))
}

fn executable() -> io::Result<PathBuf> {
    crate::daemon_executable()
}

fn own_command(socket: &Path) -> io::Result<Command> {
    let mut command = Command::new(executable()?);
    command
        .arg(crate::diagnostics::SOCKET_ARGUMENT)
        .arg(socket)
        .stdin(Stdio::null());
    Ok(command)
}

fn host_command(host: &str) -> io::Result<Command> {
    let mut command = Command::new(executable()?);
    command
        .arg("--host")
        .arg(host)
        .env(SSH_BATCH_ENVIRONMENT_VARIABLE, "1")
        .env_remove(GENERATION_VARIABLE)
        .stdin(Stdio::null());
    Ok(command)
}

#[must_use]
pub fn survey(socket: &Path) -> (Vec<Brief>, usize) {
    let mut briefs = vec![brief(socket)];
    let hosts = configured_fleet_hosts().map_or(0, |(hosts, _)| hosts.len());
    if hosts == 0 {
        return (briefs, 1);
    }
    let output = Command::new(executable().unwrap_or_default())
        .args(["--host", "all", SESSION, "state", "--brief", "--json"])
        .env(SSH_BATCH_ENVIRONMENT_VARIABLE, "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output_unmasked();
    if let Ok(output) = output {
        briefs.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| serde_json::from_str::<Brief>(line).ok())
                .filter(|brief| brief.host_name.is_some()),
        );
    }
    (briefs, hosts + 1)
}

fn detach_keeper(socket: &Path) -> Result<(), String> {
    let directory = directory().map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    if lock(&directory).is_none() {
        return Ok(());
    }
    let mut command = own_command(socket).map_err(|error| error.to_string())?;
    command
        .args([SESSION, "keep", "--here"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    crate::without_controlling_terminal(&mut command);
    command
        .spawn_unmasked()
        .map(drop)
        .map_err(|error| error.to_string())
}

fn keep(socket: &Path) -> Result<(), String> {
    let directory = directory().map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let Some(_lock) = lock(&directory) else {
        return Ok(());
    };
    let mut surveyed: Option<Instant> = None;
    loop {
        if let Some(driver) = local_driver(socket) {
            if let Err(error) = record(socket, &directory, &driver) {
                eprintln!("zz driver keep: {error}");
            }
            surveyed = None;
        } else if surveyed.is_none_or(|at| at.elapsed() >= IDLE) {
            surveyed = Some(Instant::now());
            if let Some(source) = source(socket) {
                follow(socket, &directory, &source);
                surveyed = None;
            }
        }
        thread::sleep(RECHECK);
    }
}

#[cfg(unix)]
fn lock(directory: &Path) -> Option<File> {
    let file = File::create(directory.join("keep.lock")).ok()?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive).ok()?;
    Some(file)
}

#[cfg(not(unix))]
fn lock(directory: &Path) -> Option<File> {
    File::create(directory.join("keep.lock")).ok()
}

fn source(socket: &Path) -> Option<String> {
    let (briefs, _) = survey(socket);
    briefs
        .into_iter()
        .filter(|brief| brief.alive)
        .max_by_key(Brief::position)?
        .host_name
}

fn record(socket: &Path, directory: &Path, driver: &LocalDriver) -> Result<(), String> {
    let host = host_name();
    let pane = format!("%{}", driver.pane.0);
    let lines = read_lines(directory);
    let last = last_position(&lines);
    let mut record = read_record(directory).unwrap_or_default();
    let started_here = record.host == host && record.pane.is_none() && record.generation > last.0;
    let same = record.host == host && record.pane.as_deref() == Some(pane.as_str());
    if !started_here && !same {
        record = Record {
            generation: record.generation.max(last.0) + 1,
            host,
            ..Record::default()
        };
    }
    record.pane = Some(pane);
    record.provider = Some(driver.provider.to_owned());
    if record.cwd.is_none() {
        record.cwd = Some(driver.cwd.clone());
    }
    write_record(directory, &record).map_err(|error| error.to_string())?;
    let mut written = u64::try_from(
        lines
            .iter()
            .filter(|line| line.generation == record.generation)
            .count(),
    )
    .unwrap_or_default();
    let client =
        Arc::new(InteractiveClient::connect_control(socket).map_err(|error| error.to_string())?);
    client
        .agent_replay(driver.pane, 0)
        .map_err(|error| error.to_string())?;
    let (sender, messages) = mpsc::channel();
    let reader = Arc::clone(&client);
    thread::spawn(move || {
        while let Ok(message) = reader.recv() {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    let mut log = DriverLog::default();
    let mut checked = Instant::now();
    let mut surveyed = Instant::now();
    let mut heard = false;
    loop {
        match messages.recv_timeout(RECHECK) {
            Ok(ProtocolMessage::Event(event)) => match event.payload {
                EventPayload::AgentUpdates { pane, items, .. } if pane == driver.pane => {
                    heard = true;
                    for item in items {
                        if let Ok(item) = serde_json::from_slice::<AgentStreamItem>(&item) {
                            log.apply(item.payload);
                        }
                    }
                    written = flush(directory, &mut record, &log, written)
                        .map_err(|error| error.to_string())?;
                }
                EventPayload::AgentLagged { pane, next_seq } if pane == driver.pane => {
                    client
                        .agent_replay(pane, next_seq)
                        .map_err(|error| error.to_string())?;
                }
                _ => {}
            },
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if checked.elapsed() >= RECHECK {
            checked = Instant::now();
            if local_driver(socket).map(|local| local.pane) != Some(driver.pane) {
                break;
            }
            if !heard {
                client
                    .agent_replay(driver.pane, 0)
                    .map_err(|error| error.to_string())?;
            }
        }
        if surveyed.elapsed() >= IDLE {
            surveyed = Instant::now();
            if let Some(newer) = newer_driver(&survey(socket).0, &record) {
                stand_down(socket, &record, &newer);
                break;
            }
        }
    }
    #[cfg(unix)]
    let _ = client.shutdown();
    Ok(())
}

pub(crate) fn fence(
    socket: &Path,
    host: Option<&str>,
    chain: &[CommandInvocation],
) -> Option<String> {
    let mine = std::env::var(GENERATION_VARIABLE)
        .ok()?
        .parse::<u64>()
        .ok()?;
    if crate::chain_is_read_only(chain) {
        return None;
    }
    let theirs = match host {
        None => brief(socket),
        Some(host) => remote_brief(host)?,
    };
    stale(mine, &theirs)
}

fn stale(mine: u64, theirs: &Brief) -> Option<String> {
    (theirs.generation > mine).then(|| {
        format!(
            "a newer Driver Agent (generation {}) has taken over from this one (generation {mine}), \
             so zz changes nothing for it; stop and tell the user",
            theirs.generation
        )
    })
}

fn remote_brief(host: &str) -> Option<Brief> {
    let output = host_command(host)
        .ok()?
        .args([SESSION, "state", "--brief"])
        .stderr(Stdio::null())
        .output_unmasked()
        .ok()
        .filter(|output| output.status.success())?;
    serde_json::from_slice(&output.stdout).ok()
}

fn supersedes(brief: &Brief, record: &Record) -> bool {
    (brief.generation, brief.host.as_str()) > (record.generation, record.host.as_str())
}

fn newer_driver(briefs: &[Brief], record: &Record) -> Option<Brief> {
    briefs
        .iter()
        .filter(|brief| brief.alive && brief.host_name.is_some() && supersedes(brief, record))
        .max_by(|left, right| (left.generation, &left.host).cmp(&(right.generation, &right.host)))
        .cloned()
}

fn stand_down(socket: &Path, record: &Record, newer: &Brief) {
    let _ = run_own(
        socket,
        &["kill-session".as_ref(), "-t".as_ref(), SESSION.as_ref()],
    );
    let (Some(host), Some(pane)) = (newer.host_name.as_deref(), newer.pane.as_deref()) else {
        return;
    };
    let notice = format!(
        "[zz] The driver on {} (generation {}) was still running after you started as generation \
         {}; zz stopped it. What it did after you started is only in its copy: {} --host {} \
         driver state",
        record.host,
        record.generation,
        newer.generation,
        app_identity::DIRECTORY,
        record.host
    );
    if let Ok(mut command) = host_command(host) {
        let _ = command
            .args(["send-text", "-t", pane, &notice])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status_unmasked();
    }
}

fn real_session(session: Option<&str>) -> Option<&str> {
    session.filter(|session| !session.starts_with(PLACEHOLDER))
}

fn flush(directory: &Path, record: &mut Record, log: &DriverLog, written: u64) -> io::Result<u64> {
    if !log.settled() {
        return Ok(written);
    }
    if let Some(session) = real_session(log.session())
        && real_session(record.session.as_deref()) != Some(session)
    {
        if real_session(record.session.as_deref()).is_some() {
            record.session_base = written;
        }
        record.session = Some(session.to_owned());
        write_record(directory, record)?;
    }
    let done = usize::try_from(written.saturating_sub(record.session_base)).unwrap_or(usize::MAX);
    let ts = timestamp();
    let ledger = || {
        record
            .cwd
            .as_deref()
            .and_then(|cwd| fs::read_to_string(Path::new(cwd).join("ledger.md")).ok())
            .unwrap_or_else(|| "(ledger.md could not be read)".to_owned())
    };
    let fresh = log
        .lines()
        .into_iter()
        .skip(done)
        .zip(written + 1..)
        .map(|((role, text), seq)| ChatLine {
            generation: record.generation,
            seq,
            ts: Some(ts.clone()),
            text: if role == ChatRole::Ledger && text.is_empty() {
                ledger()
            } else {
                text
            },
            role,
        })
        .collect::<Vec<_>>();
    append_lines(directory, &fresh)?;
    Ok(written + u64::try_from(fresh.len()).unwrap_or_default())
}

fn follow(socket: &Path, directory: &Path, source: &str) {
    let last = last_position(&read_lines(directory));
    let Ok(mut command) = host_command(source) else {
        return;
    };
    let child = command
        .args([SESSION, "state", "--follow", "--since"])
        .arg(format!("{}:{}", last.0, last.1))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn_unmasked();
    let Ok(mut child) = child else {
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        stop(&mut child);
        return;
    };
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut newest = last;
    let mut heard = Instant::now();
    let mut checked = Instant::now();
    loop {
        match lines.recv_timeout(RECHECK) {
            Ok(line) => {
                heard = Instant::now();
                if let Ok(chat) = serde_json::from_str::<ChatLine>(&line) {
                    if chat.position() > newest {
                        newest = chat.position();
                        if append_lines(directory, &[chat]).is_err() {
                            break;
                        }
                    }
                } else if let Ok(RecordLine { record }) = serde_json::from_str(&line) {
                    let _ = write_record(directory, &record);
                } else if let Ok(Heartbeat { alive: false }) = serde_json::from_str(&line) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if heard.elapsed() >= SILENCE {
            break;
        }
        if checked.elapsed() >= RECHECK {
            checked = Instant::now();
            if local_driver(socket).is_some() {
                break;
            }
        }
    }
    stop(&mut child);
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn import(host: &str, directory: &Path) -> Result<(), String> {
    let output = host_command(host)
        .and_then(|mut command| {
            command
                .args([SESSION, "state"])
                .stderr(Stdio::null())
                .output_unmasked()
        })
        .map_err(|error| format!("could not read {host}'s copy: {error}"))?;
    if !output.status.success() {
        return Err(format!("could not read {host}'s copy"));
    }
    let theirs = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<ChatLine>(line).ok())
        .collect::<Vec<_>>();
    if last_position(&theirs) > last_position(&read_lines(directory)) {
        write_atomically(&directory.join(STREAM), &encode(&theirs))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn instructions() -> &'static str {
    SKILL
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(SKILL, |(_, body)| body.trim_start())
}

fn provider(command: &str) -> &'static str {
    let program = command.split_whitespace().next().unwrap_or_default();
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name.starts_with("codex") {
        "codex"
    } else {
        "claude"
    }
}

fn trust_claude_folder(folder: &Path) -> io::Result<()> {
    let Some(home) = std::env::var_os("HOME") else {
        return Ok(());
    };
    let path = Path::new(&home).join(".claude.json");
    let Ok(bytes) = fs::read(&path) else {
        return Ok(());
    };
    let mut config =
        serde_json::from_slice::<serde_json::Value>(&bytes).map_err(io::Error::other)?;
    let key = folder.to_string_lossy().into_owned();
    if config["projects"][&key]["hasTrustDialogAccepted"] == serde_json::Value::Bool(true) {
        return Ok(());
    }
    let Some(root) = config.as_object_mut() else {
        return Ok(());
    };
    let projects = root
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    let Some(projects) = projects.as_object_mut() else {
        return Ok(());
    };
    let project = projects.entry(key).or_insert_with(|| serde_json::json!({}));
    if let Some(project) = project.as_object_mut() {
        project.insert(
            "hasTrustDialogAccepted".to_owned(),
            serde_json::Value::Bool(true),
        );
    }
    let temporary = path.with_extension("json.zz-driver");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&config).map_err(io::Error::other)?,
    )?;
    #[cfg(unix)]
    if let Ok(metadata) = fs::metadata(&path) {
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }
    fs::rename(temporary, path)
}

fn run_own(socket: &Path, arguments: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = own_command(socket)
        .and_then(|mut command| command.args(arguments).output_unmasked())
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

pub fn start(
    socket: &Path,
    generation: Option<u64>,
    from: Option<&str>,
    command: &str,
) -> Result<u64, String> {
    if let Some(driver) = local_driver(socket) {
        return Err(format!(
            "a driver already runs here in %{}; kill its session first",
            driver.pane.0
        ));
    }
    let directory = directory().map_err(|error| error.to_string())?;
    let work = directory.join("work");
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    if let Some(host) = from
        && let Err(error) = import(host, &directory)
    {
        eprintln!("zz driver: starting from this host's own copy: {error}");
    }
    let lines = read_lines(&directory);
    let current = read_record(&directory).map_or(0, |record| record.generation);
    let generation = generation.unwrap_or_else(|| current.max(last_position(&lines).0) + 1);
    let restart = packet(&lines);
    let write = |name: &str, contents: &str| {
        fs::write(work.join(name), contents).map_err(|error| error.to_string())
    };
    write(INSTRUCTIONS, instructions())?;
    write("AGENTS.md", instructions())?;
    if let Some(ledger) = restart
        .as_ref()
        .and_then(|restart| restart.ledger.as_deref())
    {
        write("ledger.md", ledger)?;
    }
    let provider = provider(command);
    let mut line = format!("{LOGIN_PATH}export {GENERATION_VARIABLE}={generation}; exec {command}");
    if provider == "claude" {
        let hook = format!(
            "{} {SESSION} stop-hook",
            shell_quote(
                &executable()
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
            )
        );
        let settings = serde_json::json!({
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": hook, "timeout": 30}]}]}
        });
        write(SETTINGS, &settings.to_string())?;
        if let Err(error) = trust_claude_folder(&directory) {
            eprintln!(
                "zz driver: could not mark {} trusted: {error}",
                directory.display()
            );
        }
        let _ = write!(
            line,
            " --append-system-prompt-file {INSTRUCTIONS} --settings {SETTINGS}"
        );
    } else {
        let program = executable().map_err(|error| error.to_string())?;
        let program = program.to_string_lossy();
        let hook = if program
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/._-+".contains(character))
        {
            format!("{program} {SESSION} stop-hook")
        } else {
            format!("\"{program}\" {SESSION} stop-hook")
        };
        if hook.contains('\'') {
            eprintln!("zz driver: {program} has a quote in its path, so Codex gets no Stop hook");
        } else {
            line.push_str(&codex_stop_hook(&hook));
        }
    }
    match &restart {
        Some(restart) => {
            write(PACKET, &restart.prompt)?;
            let _ = write!(line, " \"$(cat {PACKET})\"");
        }
        None => {
            let _ = fs::remove_file(work.join(PACKET));
        }
    }
    write_record(
        &directory,
        &Record {
            generation,
            host: host_name(),
            provider: Some(provider.to_owned()),
            command: Some(command.to_owned()),
            cwd: Some(work.to_string_lossy().into_owned()),
            ..Record::default()
        },
    )
    .map_err(|error| error.to_string())?;
    if local_session_exists(socket) {
        run_own(
            socket,
            &["kill-session".as_ref(), "-t".as_ref(), SESSION.as_ref()],
        )?;
    }
    run_own(
        socket,
        &[
            "new-session".as_ref(),
            "-d".as_ref(),
            "-s".as_ref(),
            SESSION.as_ref(),
            "-c".as_ref(),
            work.as_os_str(),
            line.as_ref(),
        ],
    )?;
    detach_keeper(socket)?;
    Ok(generation)
}

pub fn recover(
    socket: &Path,
    force: bool,
    command: &str,
    on: &[String],
) -> Result<Recovery, String> {
    let (briefs, hosts) = survey(socket);
    if let Some(alive) = briefs
        .iter()
        .filter(|brief| brief.alive)
        .max_by(|left, right| (left.generation, &left.host).cmp(&(right.generation, &right.host)))
    {
        return Ok(Recovery::Alive {
            host: alive
                .host_name
                .clone()
                .unwrap_or_else(|| alive.host.clone()),
            pane: alive.pane.clone(),
        });
    }
    let answered = briefs.len();
    if !force && answered * 2 <= hosts {
        return Ok(Recovery::Unreachable { answered, hosts });
    }
    let best = briefs
        .iter()
        .max_by_key(|brief| brief.position())
        .filter(|brief| brief.position() > (0, 0));
    let generation = briefs
        .iter()
        .map(|brief| brief.generation)
        .max()
        .unwrap_or_default()
        + 1;
    let target = start_host(&briefs, on);
    let Some(target) = target else {
        let from = best.and_then(|brief| brief.host_name.clone());
        start(socket, Some(generation), from.as_deref(), command)?;
        return Ok(Recovery::Started {
            generation,
            from,
            on: None,
        });
    };
    let from = best
        .filter(|brief| brief.host_name.as_deref() != Some(target.as_str()))
        .map(|brief| brief.host_name.clone().unwrap_or_else(host_name));
    let mut start = host_command(&target).map_err(|error| error.to_string())?;
    start.args([
        SESSION,
        "start",
        "--generation",
        &generation.to_string(),
        "--command",
        command,
    ]);
    if let Some(from) = &from {
        start.args(["--from", from]);
    }
    let output = start
        .output_unmasked()
        .map_err(|error| format!("could not start the driver on {target}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not start the driver on {target}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(Recovery::Started {
        generation,
        from,
        on: Some(target),
    })
}

fn start_host(briefs: &[Brief], on: &[String]) -> Option<String> {
    let here = host_name();
    on.iter().find_map(|name| {
        if name == "local" || *name == here {
            Some(None)
        } else {
            briefs
                .iter()
                .any(|brief| brief.host_name.as_deref() == Some(name.as_str()))
                .then(|| Some(name.clone()))
        }
    })?
}

fn stop_hook() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let event =
        serde_json::from_str::<serde_json::Value>(&input).map_err(|error| error.to_string())?;
    if event["stop_hook_active"].as_bool() == Some(true) {
        return Ok(());
    }
    let Some(path) = event["transcript_path"].as_str() else {
        return Ok(());
    };
    let entries = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let codex = entries
        .iter()
        .any(|entry| entry["type"] == "response_item" || entry["type"] == "event_msg");
    let acted = if codex {
        codex_acted_without_ledger(&entries, event["turn_id"].as_str())
    } else {
        acted_without_ledger(&entries)
    };
    if acted {
        println!(
            "{}",
            serde_json::json!({ "decision": "block", "reason": LEDGER_NOT_WRITTEN })
        );
    }
    Ok(())
}

fn acted_without_ledger(entries: &[serde_json::Value]) -> bool {
    let typed = |entry: &serde_json::Value| -> bool {
        entry["type"] == "user"
            && entry["isMeta"].as_bool() != Some(true)
            && !entry["message"]["content"]
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "tool_result"))
    };
    let start = entries.iter().rposition(typed).map_or(0, |index| index + 1);
    let calls = entries[start..]
        .iter()
        .filter(|entry| entry["type"] == "assistant")
        .filter_map(|entry| entry["message"]["content"].as_array())
        .flatten()
        .filter(|block| block["type"] == "tool_use")
        .collect::<Vec<_>>();
    !calls.is_empty() && !calls.iter().any(|call| writes_ledger(call))
}

fn codex_stop_hook(hook: &str) -> String {
    let command = serde_json::Value::from(hook).to_string();
    let identity = format!(
        "{{\"event_name\":\"stop\",\"hooks\":[{{\"async\":false,\"command\":{command},\"timeout\":\
         {HOOK_TIMEOUT},\"type\":\"command\"}}]}}"
    );
    let hash = Sha256::digest(identity.as_bytes())
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        });
    format!(
        " -c 'hooks.Stop=[{{hooks=[{{type=\"command\",command={command},timeout={HOOK_TIMEOUT}}}]}}]' \
         -c 'hooks.state={{\"{CODEX_HOOK_KEY}\"={{trusted_hash=\"sha256:{hash}\"}}}}'"
    )
}

fn codex_acted_without_ledger(entries: &[serde_json::Value], turn: Option<&str>) -> bool {
    let start = entries
        .iter()
        .rposition(|entry| {
            entry["type"] == "event_msg"
                && entry["payload"]["type"] == "task_started"
                && turn.is_none_or(|turn| entry["payload"]["turn_id"] == turn)
        })
        .map_or(0, |index| index + 1);
    let mut acted = false;
    let mut wrote = false;
    for entry in &entries[start..] {
        let payload = &entry["payload"];
        match (entry["type"].as_str(), payload["type"].as_str()) {
            (Some("response_item"), Some("function_call")) => {
                acted = true;
                let arguments = payload["arguments"]
                    .as_str()
                    .and_then(|arguments| serde_json::from_str::<serde_json::Value>(arguments).ok())
                    .unwrap_or_default();
                wrote |= command_writes_ledger(&command_text(&arguments["cmd"]))
                    || command_writes_ledger(&command_text(&arguments["command"]));
            }
            (Some("response_item"), Some("custom_tool_call" | "local_shell_call")) => {
                acted = true;
                let input = payload["input"].as_str().unwrap_or_default();
                wrote |= patch_writes_ledger(input)
                    || command_writes_ledger(input)
                    || command_writes_ledger(&command_text(&payload["action"]["command"]));
            }
            (Some("event_msg"), Some("item_completed")) => {
                let item = &payload["item"];
                wrote |= match item["type"].as_str() {
                    Some("CommandExecution") => {
                        command_writes_ledger(&command_text(&item["command"]))
                    }
                    Some("FileChange") => item["changes"].as_object().is_some_and(|changes| {
                        changes.keys().any(|path| path.ends_with(LEDGER_FILE))
                    }),
                    _ => false,
                };
            }
            _ => {}
        }
    }
    acted && !wrote
}

fn command_text(command: &serde_json::Value) -> String {
    match command {
        serde_json::Value::String(command) => command.clone(),
        serde_json::Value::Array(words) => words
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn command_writes_ledger(command: &str) -> bool {
    command.contains(LEDGER_FILE)
        && [">", "tee ", "sed -i", "mv ", "cp "]
            .iter()
            .any(|writer| command.contains(writer))
}

fn patch_writes_ledger(patch: &str) -> bool {
    patch.lines().any(|line| {
        ["*** Add File:", "*** Update File:", "*** Move to:"]
            .iter()
            .any(|marker| line.starts_with(marker))
            && line.trim_end().ends_with(LEDGER_FILE)
    })
}

fn writes_ledger(call: &serde_json::Value) -> bool {
    let input = &call["input"];
    match call["name"].as_str() {
        Some("Write" | "Edit") => input["file_path"]
            .as_str()
            .is_some_and(|path| path.ends_with(LEDGER_FILE)),
        Some("Bash") => input["command"].as_str().is_some_and(command_writes_ledger),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        Brief, LOGIN_PATH, Record, acted_without_ledger, civil_from_days,
        codex_acted_without_ledger, codex_stop_hook, instructions, newer_driver, provider, stale,
        start_host,
    };

    #[cfg(unix)]
    #[test]
    fn the_driver_finds_the_zz_of_the_daemon_it_runs_under_first() {
        let path = |shim: Option<&str>| {
            let mut shell = std::process::Command::new("/bin/sh");
            shell
                .arg("-c")
                .arg(format!("{LOGIN_PATH}printf %s \"$PATH\""))
                .env_clear()
                .env("SHELL", "/bin/sh")
                .env("HOME", "/home/u")
                .env("PATH", "/usr/bin:/bin");
            if let Some(shim) = shim {
                shell.env("ZZ_TMUX_SHIM_DIR", shim);
            }
            String::from_utf8(shell.output().expect("sh").stdout).expect("utf-8")
        };
        let shimmed = path(Some("/tmp/zz-tmux-shim-1-0"));
        assert!(shimmed.starts_with("/tmp/zz-tmux-shim-1-0:"), "{shimmed}");
        assert!(shimmed.ends_with(":/home/u/.local/bin"), "{shimmed}");
        let plain = path(None);
        assert!(
            !plain.starts_with(':') && !plain.contains("zz-tmux-shim"),
            "{plain}"
        );
        assert!(plain.ends_with(":/home/u/.local/bin"), "{plain}");
    }

    #[test]
    fn a_turn_that_acted_must_write_the_ledger() {
        let user = json!({"type": "user", "message": {"content": "go"}});
        let bash = json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "name": "Bash", "input": {"command": "zz list-panes"}}
        ]}});
        let ledger = json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "name": "Write", "input": {"file_path": "/w/ledger.md"}}
        ]}});
        assert!(acted_without_ledger(&[user.clone(), bash.clone()]));
        assert!(!acted_without_ledger(&[user.clone(), bash, ledger]));
        assert!(!acted_without_ledger(&[user]));
    }

    #[test]
    fn a_codex_turn_that_acted_must_write_the_ledger() {
        let started = |turn: &str| json!({"type": "event_msg", "payload": {"type": "task_started", "turn_id": turn}});
        let shell = |cmd: &str| {
            json!({"type": "response_item", "payload": {"type": "function_call", "name": "exec_command",
                "arguments": json!({"cmd": cmd}).to_string()}})
        };
        let patch = json!({"type": "response_item", "payload": {"type": "custom_tool_call",
            "name": "apply_patch", "input": "*** Begin Patch\n*** Update File: /w/ledger.md\n@@\n-a\n+b\n*** End Patch"}});
        let changed = json!({"type": "event_msg", "payload": {"type": "item_completed",
            "item": {"type": "FileChange", "changes": {"/w/ledger.md": {"type": "update"}}}}});
        let old_turn = [started("t1"), shell("echo hi > ledger.md")];
        let listed = [started("t2"), shell("zz list-panes")];
        assert!(codex_acted_without_ledger(
            &[&old_turn[..], &listed[..]].concat(),
            Some("t2")
        ));
        assert!(!codex_acted_without_ledger(
            &[started("t2"), shell("ls"), patch],
            Some("t2")
        ));
        assert!(!codex_acted_without_ledger(
            &[started("t2"), shell("ls"), changed],
            Some("t2")
        ));
        assert!(!codex_acted_without_ledger(
            &[started("t2"), shell("cat > ledger.md")],
            Some("t2")
        ));
        assert!(!codex_acted_without_ledger(&[started("t2")], Some("t2")));
    }

    #[test]
    fn the_codex_hook_carries_its_own_trust() {
        let flags = codex_stop_hook("/home/u/.local/bin/zz driver stop-hook");
        assert!(flags.contains("hooks.Stop=[{hooks=[{type=\"command\",command=\"/home/u/.local/bin/zz driver stop-hook\",timeout=30}]}]"));
        assert!(
            flags.contains("\"/<session-flags>/config.toml:stop:0:0\"={trusted_hash=\"sha256:")
        );
    }

    #[test]
    fn the_skill_goes_in_without_its_frontmatter() {
        assert!(instructions().starts_with("# Driving through zz"));
        assert_eq!(provider("codex -c model_reasoning_effort=high"), "codex");
        assert_eq!(provider("claude --model opus"), "claude");
    }

    #[test]
    fn days_become_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_386), (2025, 10, 25));
    }

    fn alive(host_name: &str, host: &str, generation: u64) -> Brief {
        Brief {
            host_name: Some(host_name.to_owned()),
            host: host.to_owned(),
            generation,
            alive: true,
            pane: Some("%3".to_owned()),
            ..Brief::default()
        }
    }

    #[test]
    fn an_older_driver_stands_down_for_a_newer_one_and_only_then() {
        let record = Record {
            generation: 4,
            host: "xps".to_owned(),
            ..Record::default()
        };
        let newer = alive("alienware", "alienware", 5);
        let older = alive("ubuntu", "ubuntu", 3);
        let mut dead = alive("mac", "mac", 9);
        dead.alive = false;
        assert_eq!(
            newer_driver(&[older.clone(), newer.clone(), dead], &record),
            Some(newer)
        );
        assert_eq!(newer_driver(&[older], &record), None);
        let tie_above = alive("zeta", "zeta", 4);
        let tie_below = alive("alpha", "alpha", 4);
        assert_eq!(
            newer_driver(std::slice::from_ref(&tie_below), &record),
            None,
            "the same generation keeps the host that sorts last"
        );
        assert_eq!(
            newer_driver(std::slice::from_ref(&tie_above), &record),
            Some(tie_above)
        );
    }

    #[test]
    fn a_stale_driver_is_fenced() {
        let theirs = |generation| Brief {
            generation,
            ..Brief::default()
        };
        assert!(stale(4, &theirs(5)).is_some_and(|message| message.contains("generation 5")));
        assert_eq!(stale(5, &theirs(5)), None);
        assert_eq!(stale(5, &theirs(2)), None);
    }

    #[test]
    fn a_driver_starts_on_the_first_listed_host_that_answered() {
        let briefs = [Brief::default(), alive("xps", "xps", 1)];
        let on = |hosts: &[&str]| {
            hosts
                .iter()
                .map(|host| (*host).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            start_host(&briefs, &on(&["ubuntu", "xps"])),
            Some("xps".to_owned())
        );
        assert_eq!(start_host(&briefs, &on(&["ubuntu", "local", "xps"])), None);
        assert_eq!(start_host(&briefs, &on(&["ubuntu"])), None);
        assert_eq!(start_host(&briefs, &[]), None);
    }

    #[test]
    fn briefs_order_by_generation_then_seq() {
        let brief = |generation, seq| Brief {
            generation,
            seq,
            ..Brief::default()
        };
        assert!(brief(3, 1).position() > brief(2, 90).position());
        assert!(brief(3, 5).position() > brief(3, 4).position());
    }
}
