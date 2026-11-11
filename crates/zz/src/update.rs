use std::{
    fs, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use ala::{AnyElement, App, AsyncApp, Entity, Global, Window, prelude::*};
use penna::{
    WindowExt as _,
    blocks::notification::Notification,
    components::{
        button::{Button, ButtonVariants as _},
        progress::Progress,
    },
    h_flex, v_flex,
};
use semver::Version;
pub(crate) use zz_config::update::{Channel, Release};
use zz_config::update::{
    Facts, Progress as Step, Staged, Target, UpdateError, apply, assets_for, checks_enabled,
    clear_staged, detect, fetch_latest, homebrew_cask, prepare, staged, staged_for, tarball_prefix,
};
use zz_protocol::CommandInvocation;

use crate::{
    config,
    mux::{
        client::MuxClient,
        hosts::{HostId, session_count},
    },
    user_data,
    window::toast,
};

const INSTALL_SCRIPT_URL: &str = "https://zzmux.sh/install.sh";
const INITIAL_DELAY: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_hours(24);
const TOAST_KEY: &str = "update-available";
const PROGRESS_ID: &str = "update-progress";
const DISMISSED_FILE_NAME: &str = "update-dismissed";
const LOG_TARGET: &str = "zz::update";
const BYTES_PER_MB: u64 = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckState {
    Unchecked,
    Checking,
    UpToDate,
    Available(Release),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trigger {
    Scheduled,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Downloading { received: u64, total: Option<u64> },
    Verifying,
    Staging,
    Ready(Staged),
    Restarting,
    Failed(String),
}

impl Stage {
    const fn busy(&self) -> bool {
        matches!(
            self,
            Self::Downloading { .. } | Self::Verifying | Self::Staging | Self::Restarting
        )
    }

    pub(crate) fn bar(&self) -> Option<Bar> {
        match self {
            Self::Downloading {
                received,
                total: Some(total),
            } if *total > 0 => Some(Bar::Filled(fraction(*received, *total))),
            Self::Downloading { .. } | Self::Verifying | Self::Staging | Self::Restarting => {
                Some(Bar::Sweeping)
            }
            Self::Ready(_) | Self::Failed(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Bar {
    Filled(f32),
    Sweeping,
}

impl Bar {
    pub(crate) fn element(self, label: &'static str) -> Progress {
        match self {
            Self::Filled(value) => Progress::new(PROGRESS_ID, value),
            Self::Sweeping => Progress::indeterminate(PROGRESS_ID),
        }
        .label(label)
    }
}

struct Install {
    release: Release,
    stage: Stage,
    cancel: Arc<AtomicBool>,
}

impl Install {
    fn new(release: Release, stage: Stage) -> Self {
        Self {
            release,
            stage,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub(crate) struct UpdateState {
    mux: Entity<MuxClient>,
    current: Version,
    check: CheckState,
    dismissed: Option<Version>,
    install: Option<Install>,
}

impl Global for UpdateState {}

pub(crate) struct Status {
    pub current: Version,
    pub channel: Channel,
    pub check: CheckState,
    pub install: Option<(Version, Stage)>,
}

pub(crate) fn status(cx: &App) -> Option<Status> {
    let state = cx.try_global::<UpdateState>()?;
    Some(Status {
        current: state.current.clone(),
        channel: Channel::of(&state.current),
        check: state.check.clone(),
        install: state
            .install
            .as_ref()
            .map(|install| (install.release.version.clone(), install.stage.clone())),
    })
}

pub(crate) fn available_release(cx: &App) -> Option<Version> {
    match &cx.try_global::<UpdateState>()?.check {
        CheckState::Available(release) => Some(release.version.clone()),
        _ => None,
    }
}

pub(crate) fn restarting(cx: &App) -> bool {
    cx.try_global::<UpdateState>().is_some_and(|state| {
        state
            .install
            .as_ref()
            .is_some_and(|install| install.stage == Stage::Restarting)
    })
}

pub(crate) fn daemon_note(release: &Release, mux: &MuxClient) -> Option<String> {
    let wire = release.wire?;
    let revision = mux.daemon_revision(HostId::LOCAL)?;
    Some(
        if wire.keeps_daemon(zz_protocol::PROTOCOL_VERSION, revision) {
            "The update keeps your sessions running.".to_owned()
        } else {
            format!(
                "The update needs a daemon restart, which ends {}.",
                session_count(mux.host_session_count(HostId::LOCAL))
            )
        },
    )
}

pub(crate) fn download_text(received: u64, total: Option<u64>) -> String {
    let megabytes = |bytes: u64| bytes.saturating_add(BYTES_PER_MB / 2) / BYTES_PER_MB;
    match total {
        Some(total) if total > 0 => {
            format!(
                "{} of {} MB",
                megabytes(received.min(total)),
                megabytes(total)
            )
        }
        _ => format!("{} MB", megabytes(received)),
    }
}

fn fraction(received: u64, total: u64) -> f32 {
    let permille = received.min(total).saturating_mul(1000) / total;
    f32::from(u16::try_from(permille).unwrap_or(1000)) / 1000.0
}

pub(crate) fn start(mux: Entity<MuxClient>, cx: &mut App) {
    let current = match Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(version) => version,
        Err(error) => {
            log::warn!(target: LOG_TARGET, "update checks disabled: unparsable build version: {error}");
            return;
        }
    };
    cx.set_global(UpdateState {
        mux,
        current,
        check: CheckState::Unchecked,
        dismissed: read_dismissed(),
        install: None,
    });
    if !checks_enabled() {
        log::debug!(target: LOG_TARGET, "scheduled update checks are off for this build");
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(INITIAL_DELAY).await;
        loop {
            if cx.update(|cx| config::check_for_updates(cx)) {
                run_check(Trigger::Scheduled, cx).await;
            }
            cx.background_executor().timer(CHECK_INTERVAL).await;
        }
    })
    .detach();
}

pub(crate) fn check_now(cx: &mut App) {
    let checking = cx
        .try_global::<UpdateState>()
        .is_some_and(|state| state.check == CheckState::Checking);
    if checking {
        return;
    }
    cx.spawn(async move |cx| run_check(Trigger::Manual, cx).await)
        .detach();
}

pub(crate) fn install(window: &mut Window, cx: &mut App) {
    let Some(state) = cx.try_global::<UpdateState>() else {
        return;
    };
    match state.install.as_ref().map(|install| &install.stage) {
        Some(Stage::Ready(_)) => return restart(cx),
        Some(step) if step.busy() => return,
        _ => {}
    }
    let CheckState::Available(release) = &state.check else {
        return;
    };
    let release = release.clone();
    if let Some(target) = detect().filter(|target| assets_for(&release, target).is_some()) {
        download(release, target, cx);
        return;
    }
    let channel = Channel::of(&state.current);
    let mux = state.mux.clone();
    let plan = install_plan(&release, channel, &Environment::detect());
    log::info!(target: LOG_TARGET, "applying update to {} via {plan:?}", release.version);
    match plan {
        InstallPlan::Terminal(command)
            if mux.read(cx).attached_session().is_some() && !config::quit_daemon_on_exit(cx) =>
        {
            let script = terminal_script(&command);
            mux.read(cx).execute(CommandInvocation::new(
                "new-window",
                ["-n", "update", "sh", "-c", script.as_str()],
            ));
        }
        InstallPlan::Terminal(_) => cx.open_url(&release.url),
        InstallPlan::Open(url) => cx.open_url(&url),
    }
    window.dismiss_notification(TOAST_KEY, cx);
}

pub(crate) fn cancel(cx: &mut App) {
    if let Some(install) = install_mut(cx)
        && matches!(install.stage, Stage::Downloading { .. })
    {
        install.cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn restart(cx: &mut App) {
    let Some(install) = install_mut(cx) else {
        return;
    };
    let Stage::Ready(staged) = &install.stage else {
        return;
    };
    let staged = staged.clone();
    install.stage = Stage::Restarting;
    show_toast(Show::IfShowing, cx);
    log::info!(target: LOG_TARGET, "installing zz {} from {}", staged.version, staged.path.display());
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { apply(&staged) })
            .await;
        cx.update(|cx| match result {
            Ok(relaunch) => {
                log::info!(target: LOG_TARGET, "relaunching {}", relaunch.display());
                cx.set_restart_path(relaunch);
                cx.restart();
            }
            Err(error) => fail(&error, cx),
        });
    })
    .detach();
}

fn install_mut(cx: &mut App) -> Option<&mut Install> {
    if !cx.has_global::<UpdateState>() {
        return None;
    }
    cx.global_mut::<UpdateState>().install.as_mut()
}

fn download(release: Release, target: Target, cx: &mut App) {
    log::info!(target: LOG_TARGET, "downloading zz {} for {target:?}", release.version);
    let install = Install::new(
        release.clone(),
        Stage::Downloading {
            received: 0,
            total: None,
        },
    );
    let cancel = Arc::clone(&install.cancel);
    cx.global_mut::<UpdateState>().install = Some(install);
    show_toast(Show::Always, cx);
    let (steps, received) = async_channel::unbounded();
    cx.spawn(async move |cx| {
        let prepared = cx.background_executor().spawn(async move {
            prepare(&release, &target, &cancel, |step| {
                let _ = steps.try_send(step);
            })
        });
        while let Ok(step) = received.recv().await {
            cx.update(|cx| advance(step, cx));
        }
        let result = prepared.await;
        cx.update(|cx| finish_download(result, cx));
    })
    .detach();
}

fn advance(step: Step, cx: &mut App) {
    let Some(install) = install_mut(cx) else {
        return;
    };
    if !install.stage.busy() {
        return;
    }
    install.stage = match step {
        Step::Downloading { received, total } => Stage::Downloading { received, total },
        Step::Verifying => Stage::Verifying,
        Step::Staging => Stage::Staging,
    };
    show_toast(Show::IfShowing, cx);
}

fn finish_download(result: Result<Staged, UpdateError>, cx: &mut App) {
    if !cx.has_global::<UpdateState>() {
        return;
    }
    let state = cx.global_mut::<UpdateState>();
    match result {
        Ok(staged) => {
            log::info!(target: LOG_TARGET, "zz {} is staged at {}", staged.version, staged.path.display());
            if let Some(install) = state.install.as_mut() {
                install.stage = Stage::Ready(staged);
            }
            show_toast(Show::Always, cx);
        }
        Err(UpdateError::Cancelled) => {
            log::info!(target: LOG_TARGET, "update download cancelled");
            state.install = None;
            show_toast(Show::IfShowing, cx);
        }
        Err(error) => fail(&error, cx),
    }
}

fn fail(error: &UpdateError, cx: &mut App) {
    log::warn!(target: LOG_TARGET, "update failed: {error}");
    if let Some(install) = install_mut(cx) {
        install.stage = Stage::Failed(error.to_string());
    }
    show_toast(Show::Always, cx);
}

fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

async fn run_check(trigger: Trigger, cx: &mut AsyncApp) {
    let Some(current) = cx.update(|cx| {
        cx.try_global::<UpdateState>()
            .map(|state| state.current.clone())
    }) else {
        return;
    };
    cx.update(|cx| set_check(CheckState::Checking, cx));
    let channel = Channel::of(&current);
    let idle = cx.update(|cx| {
        cx.try_global::<UpdateState>().is_some_and(|state| {
            state.install.as_ref().is_none_or(|install| {
                !install.stage.busy() && !matches!(install.stage, Stage::Ready(_))
            })
        })
    });
    let (result, found) = cx
        .background_executor()
        .spawn(async move {
            let result = fetch_latest(channel);
            let latest = match &result {
                Ok(Some(release)) if release.version > current => Some(&release.version),
                _ => None,
            };
            let found = idle.then(|| reconcile_staged(latest)).flatten();
            (result, found)
        })
        .await;
    cx.update(|cx| finish_check(result, found, trigger, cx));
}

fn reconcile_staged(latest: Option<&Version>) -> Option<Staged> {
    if let (Some(version), Some(target)) = (latest, detect())
        && let Some(staged) = staged_for(version, &target)
    {
        return Some(staged);
    }
    if staged().is_some()
        && let Err(error) = clear_staged()
    {
        log::warn!(target: LOG_TARGET, "could not clear a stale staged update: {error}");
    }
    None
}

fn set_check(check: CheckState, cx: &mut App) {
    if cx.has_global::<UpdateState>() {
        cx.global_mut::<UpdateState>().check = check;
    }
}

fn finish_check(
    result: Result<Option<Release>, String>,
    found: Option<Staged>,
    trigger: Trigger,
    cx: &mut App,
) {
    if !cx.has_global::<UpdateState>() {
        return;
    }
    let state = cx.global_mut::<UpdateState>();
    if state
        .install
        .as_ref()
        .is_some_and(|install| matches!(install.stage, Stage::Failed(_)))
    {
        state.install = None;
    }
    let current = state.current.clone();
    let (check, offer) = match result {
        Err(error) => (CheckState::Failed(error), false),
        Ok(None) => (
            CheckState::Failed("the release list carries no release".to_owned()),
            false,
        ),
        Ok(Some(release)) if release.version > current => {
            let offer =
                trigger == Trigger::Manual || state.dismissed.as_ref() != Some(&release.version);
            if let Some(staged) = found.filter(|staged| staged.version == release.version) {
                log::info!(target: LOG_TARGET, "zz {} is already staged", staged.version);
                state.install = Some(Install::new(release.clone(), Stage::Ready(staged)));
            }
            (CheckState::Available(release), offer)
        }
        Ok(Some(_)) => (CheckState::UpToDate, false),
    };
    state.check = check.clone();

    match &check {
        CheckState::Failed(error) => {
            log::warn!(target: LOG_TARGET, "update check failed: {error}");
            if trigger == Trigger::Manual {
                toast::push(
                    Notification::error(format!("Could not check for updates: {error}")),
                    cx,
                );
            }
        }
        CheckState::UpToDate => {
            log::info!(target: LOG_TARGET, "zz {current} is the newest release");
            if trigger == Trigger::Manual {
                toast::push(
                    Notification::success(format!("zz {current} is the newest release")),
                    cx,
                );
            }
        }
        CheckState::Available(release) => {
            log::info!(target: LOG_TARGET, "zz {} is available (running {current})", release.version);
        }
        CheckState::Unchecked | CheckState::Checking => {}
    }
    if offer {
        show_toast(Show::Always, cx);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Show {
    Always,
    IfShowing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Update,
    WhatsNew,
    Dismiss,
    Cancel,
    Restart,
    Hide,
    TryAgain,
    ReleasePage,
}

#[derive(Clone, Debug, PartialEq)]
struct ToastView {
    title: String,
    message: Option<String>,
    bar: Option<Bar>,
    actions: &'static [Action],
}

fn toast_view(
    current: &Version,
    version: &Version,
    stage: Option<&Stage>,
    note: Option<String>,
) -> ToastView {
    let view = |title: String, message: Option<String>, actions| ToastView {
        title,
        message,
        bar: stage.and_then(Stage::bar),
        actions,
    };
    match stage {
        None => view(
            format!("zz {version} is available"),
            Some(note.map_or_else(
                || format!("You are running {current}."),
                |note| format!("You are running {current}. {note}"),
            )),
            &[Action::Update, Action::WhatsNew, Action::Dismiss],
        ),
        Some(Stage::Downloading { received, total }) => view(
            format!("Downloading zz {version}"),
            Some(download_text(*received, *total)),
            &[Action::Cancel],
        ),
        Some(Stage::Verifying) => view(format!("Verifying zz {version}"), None, &[]),
        Some(Stage::Staging) => view(format!("Preparing zz {version}"), None, &[]),
        Some(Stage::Ready(_)) => view(
            format!("zz {version} is ready"),
            note,
            &[Action::Restart, Action::Hide],
        ),
        Some(Stage::Restarting) => view(format!("Installing zz {version}"), None, &[]),
        Some(Stage::Failed(message)) => view(
            "Update failed".to_owned(),
            Some(sentence(message)),
            &[Action::TryAgain, Action::ReleasePage],
        ),
    }
}

fn show_toast(show: Show, cx: &mut App) {
    let Some(state) = cx.try_global::<UpdateState>() else {
        return;
    };
    let (release, shown) = match (&state.install, &state.check) {
        (Some(install), _) => (&install.release, Some(&install.stage)),
        (None, CheckState::Available(release)) => (release, None),
        (None, _) => return,
    };
    let note = daemon_note(release, state.mux.read(cx));
    let view = toast_view(&state.current, &release.version, shown, note);
    let notification = toast(view, release.version.clone(), release.url.clone());
    match show {
        Show::Always => toast::push(notification, cx),
        Show::IfShowing => toast::replace(notification, cx),
    }
}

fn toast(view: ToastView, version: Version, url: String) -> Notification {
    let ToastView {
        title,
        message,
        bar,
        actions,
    } = view;
    let notification = Notification::new()
        .key(TOAST_KEY)
        .autohide(false)
        .closable(bar.is_none())
        .title(title);
    let notification = match message {
        Some(message) => notification.message(message),
        None => notification,
    };
    notification.content(move |_, _, _| {
        v_flex()
            .gap_1()
            .when_some(bar, |this, bar| this.child(bar.element("Update")))
            .when(!actions.is_empty(), |this| {
                this.child(
                    h_flex().gap_2().pt_1().children(
                        actions
                            .iter()
                            .map(|action| toast_button(*action, &version, &url)),
                    ),
                )
            })
            .into_any_element()
    })
}

fn toast_button(action: Action, version: &Version, url: &str) -> AnyElement {
    let url = url.to_owned();
    let version = version.clone();
    match action {
        Action::Update => Button::new("update-install")
            .primary()
            .label("Update")
            .on_click(|_, window, cx| install(window, cx)),
        Action::WhatsNew => Button::new("update-notes")
            .ghost()
            .label("What's new")
            .on_click(move |_, _, cx| cx.open_url(&url)),
        Action::Dismiss => {
            Button::new("update-later")
                .ghost()
                .label("Later")
                .on_click(move |_, window, cx| {
                    dismiss(&version, cx);
                    window.dismiss_notification(TOAST_KEY, cx);
                })
        }
        Action::Cancel => Button::new("update-cancel")
            .ghost()
            .label("Cancel")
            .on_click(|_, _, cx| cancel(cx)),
        Action::Restart => Button::new("update-restart")
            .primary()
            .label("Restart to update")
            .on_click(|_, _, cx| restart(cx)),
        Action::Hide => {
            Button::new("update-hide")
                .ghost()
                .label("Later")
                .on_click(|_, window, cx| {
                    window.dismiss_notification(TOAST_KEY, cx);
                })
        }
        Action::TryAgain => Button::new("update-retry")
            .primary()
            .label("Try again")
            .on_click(|_, window, cx| install(window, cx)),
        Action::ReleasePage => Button::new("update-release-page")
            .ghost()
            .label("Release page")
            .on_click(move |_, _, cx| cx.open_url(&url)),
    }
    .into_any_element()
}

fn dismiss(version: &Version, cx: &mut App) {
    if cx.has_global::<UpdateState>() {
        cx.global_mut::<UpdateState>().dismissed = Some(version.clone());
    }
    if let Err(error) = write_dismissed(version) {
        log::warn!(target: LOG_TARGET, "could not remember the dismissed release: {error}");
    }
}

fn dismissed_path() -> Option<PathBuf> {
    Some(
        user_data::platform_data_dir()?
            .join(zz_protocol::app_identity::DIRECTORY)
            .join(DISMISSED_FILE_NAME),
    )
}

fn read_dismissed() -> Option<Version> {
    let text = fs::read_to_string(dismissed_path()?).ok()?;
    Version::parse(text.trim()).ok()
}

fn write_dismissed(version: &Version) -> io::Result<()> {
    let path = dismissed_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not resolve the current user's application-data directory",
        )
    })?;
    let parent = path
        .parent()
        .expect("the dismissed-release path has a parent");
    fs::create_dir_all(parent)?;
    user_data::restrict_directory_to_current_user(parent)?;
    fs::write(&path, format!("{version}\n"))?;
    user_data::restrict_to_current_user(&path)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Environment {
    facts: Facts,
    homebrew: Option<(PathBuf, &'static str)>,
    pacman_owned: bool,
    aur_helper: Option<&'static str>,
}

impl Environment {
    fn detect() -> Self {
        let facts = Facts::current().unwrap_or_else(|| Facts {
            os: std::env::consts::OS,
            ..Facts::default()
        });
        Self {
            homebrew: facts.homebrew.then(homebrew_cask).flatten(),
            pacman_owned: fs::read_dir("/var/lib/pacman/local").is_ok_and(|entries| {
                entries.flatten().any(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("zz-bin-") || name.starts_with("zz-beta-bin-")
                })
            }),
            aur_helper: ["paru", "yay"].into_iter().find(|helper| on_path(helper)),
            facts,
        }
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InstallPlan {
    Terminal(String),
    Open(String),
}

fn install_plan(release: &Release, channel: Channel, environment: &Environment) -> InstallPlan {
    match environment.facts.os {
        "macos" => match &environment.homebrew {
            Some((prefix, cask)) => InstallPlan::Terminal(format!(
                "{}/bin/brew upgrade --cask {cask}",
                prefix.display()
            )),
            None => InstallPlan::Terminal(install_script_command(channel, None)),
        },
        "linux" if environment.facts.appimage.is_some() => InstallPlan::Open(release.url.clone()),
        "linux" if environment.pacman_owned => match environment.aur_helper {
            Some(helper) => InstallPlan::Terminal(format!(
                "{helper} -S {}",
                match channel {
                    Channel::Stable => "zz-bin",
                    Channel::Beta => "zz-beta-bin",
                }
            )),
            None => InstallPlan::Open(release.url.clone()),
        },
        "linux" => InstallPlan::Terminal(install_script_command(
            channel,
            tarball_prefix(&environment.facts.executable),
        )),
        "windows" => InstallPlan::Open(format!(
            "https://github.com/autistic-computer/zebrilus/releases/download/v{version}/zz-{version}-windows-x64-setup.exe",
            version = release.version
        )),
        _ => InstallPlan::Open(release.url.clone()),
    }
}

fn install_script_command(channel: Channel, prefix: Option<&std::path::Path>) -> String {
    let mut arguments = Vec::new();
    if channel == Channel::Beta {
        arguments.push("--beta".to_owned());
    }
    if let Some(prefix) = prefix {
        arguments.push(format!("--prefix '{}'", prefix.display()));
    }
    if arguments.is_empty() {
        format!("curl -fsSL {INSTALL_SCRIPT_URL} | sh")
    } else {
        format!(
            "curl -fsSL {INSTALL_SCRIPT_URL} | sh -s -- {}",
            arguments.join(" ")
        )
    }
}

fn terminal_script(command: &str) -> String {
    let quoted = command.replace('\'', "'\\''");
    format!(
        "printf '%s\\n' '$ {quoted}'\n\
         {command}\n\
         status=$?\n\
         if [ \"$status\" -ne 0 ]; then printf '\\nzz update failed (exit %s).\\n' \"$status\"; fi\n\
         printf '\\nPress Enter to close this window.\\n'\n\
         read -r _\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("valid version")
    }

    fn offered() -> Release {
        Release {
            version: version("0.3.2"),
            url: "https://github.com/autistic-computer/zebrilus/releases/tag/v0.3.2".to_owned(),
            assets: Vec::new(),
            wire: None,
        }
    }

    fn environment(os: &'static str, executable: &str) -> Environment {
        Environment {
            facts: Facts {
                os,
                executable: PathBuf::from(executable),
                ..Facts::default()
            },
            ..Environment::default()
        }
    }

    #[test]
    fn each_stage_shows_its_own_toast() {
        let current = version("0.3.1");
        let next = version("0.3.2");
        let note = || Some("The update keeps your sessions running.".to_owned());
        let staged = Staged {
            version: next.clone(),
            target: Target::MacBundle {
                app: PathBuf::from("/Applications/zz.app"),
            },
            path: PathBuf::from("/Applications/.zz-update.app"),
        };
        let cases = [
            (
                None,
                ToastView {
                    title: "zz 0.3.2 is available".to_owned(),
                    message: Some(
                        "You are running 0.3.1. The update keeps your sessions running.".to_owned(),
                    ),
                    bar: None,
                    actions: &[Action::Update, Action::WhatsNew, Action::Dismiss],
                },
            ),
            (
                Some(Stage::Downloading {
                    received: 84_400_000,
                    total: Some(186_000_000),
                }),
                ToastView {
                    title: "Downloading zz 0.3.2".to_owned(),
                    message: Some("84 of 186 MB".to_owned()),
                    bar: Some(Bar::Filled(0.453)),
                    actions: &[Action::Cancel],
                },
            ),
            (
                Some(Stage::Downloading {
                    received: 2_600_000,
                    total: None,
                }),
                ToastView {
                    title: "Downloading zz 0.3.2".to_owned(),
                    message: Some("3 MB".to_owned()),
                    bar: Some(Bar::Sweeping),
                    actions: &[Action::Cancel],
                },
            ),
            (
                Some(Stage::Staging),
                ToastView {
                    title: "Preparing zz 0.3.2".to_owned(),
                    message: None,
                    bar: Some(Bar::Sweeping),
                    actions: &[],
                },
            ),
            (
                Some(Stage::Ready(staged)),
                ToastView {
                    title: "zz 0.3.2 is ready".to_owned(),
                    message: note(),
                    bar: None,
                    actions: &[Action::Restart, Action::Hide],
                },
            ),
            (
                Some(Stage::Failed("cannot write to /Applications".to_owned())),
                ToastView {
                    title: "Update failed".to_owned(),
                    message: Some("Cannot write to /Applications".to_owned()),
                    bar: None,
                    actions: &[Action::TryAgain, Action::ReleasePage],
                },
            ),
        ];
        for (stage, expected) in cases {
            assert_eq!(
                toast_view(&current, &next, stage.as_ref(), note()),
                expected,
                "{stage:?}"
            );
        }
    }

    #[test]
    fn macos_uses_the_installer_unless_homebrew_owns_the_app() {
        let plain = environment("macos", "/Applications/zz.app/Contents/MacOS/zz");
        assert_eq!(
            install_plan(&offered(), Channel::Stable, &plain),
            InstallPlan::Terminal("curl -fsSL https://zzmux.sh/install.sh | sh".to_owned())
        );
        assert_eq!(
            install_plan(&offered(), Channel::Beta, &plain),
            InstallPlan::Terminal(
                "curl -fsSL https://zzmux.sh/install.sh | sh -s -- --beta".to_owned()
            )
        );
        let brewed = Environment {
            homebrew: Some((PathBuf::from("/opt/homebrew"), "zz@beta")),
            ..plain
        };
        assert_eq!(
            install_plan(&offered(), Channel::Beta, &brewed),
            InstallPlan::Terminal("/opt/homebrew/bin/brew upgrade --cask zz@beta".to_owned())
        );
    }

    #[test]
    fn linux_routes_by_package_owner() {
        let deb = environment("linux", "/usr/lib/zz/zz");
        assert_eq!(
            install_plan(&offered(), Channel::Stable, &deb),
            InstallPlan::Terminal("curl -fsSL https://zzmux.sh/install.sh | sh".to_owned())
        );
        let tarball = environment("linux", "/home/me/.local/lib/zz/zz");
        assert_eq!(
            install_plan(&offered(), Channel::Beta, &tarball),
            InstallPlan::Terminal(
                "curl -fsSL https://zzmux.sh/install.sh | sh -s -- --beta --prefix '/home/me/.local'"
                    .to_owned()
            )
        );
        let aur = Environment {
            pacman_owned: true,
            aur_helper: Some("paru"),
            ..environment("linux", "/usr/lib/zz/zz")
        };
        assert_eq!(
            install_plan(&offered(), Channel::Stable, &aur),
            InstallPlan::Terminal("paru -S zz-bin".to_owned())
        );
        let aur_without_helper = Environment {
            aur_helper: None,
            ..aur
        };
        assert_eq!(
            install_plan(&offered(), Channel::Stable, &aur_without_helper),
            InstallPlan::Open(offered().url)
        );
        let mut appimage = environment("linux", "/tmp/.mount_zz/usr/lib/zz/zz");
        appimage.facts.appimage = Some(PathBuf::from("/home/me/Apps/zz.AppImage"));
        assert_eq!(
            install_plan(&offered(), Channel::Stable, &appimage),
            InstallPlan::Open(offered().url)
        );
    }

    #[test]
    fn windows_opens_the_installer_download() {
        let windows = environment("windows", "C:/Program Files/zz/zz.exe");
        assert_eq!(
            install_plan(&offered(), Channel::Stable, &windows),
            InstallPlan::Open(
                "https://github.com/autistic-computer/zebrilus/releases/download/v0.3.2/zz-0.3.2-windows-x64-setup.exe"
                    .to_owned()
            )
        );
    }

    #[test]
    fn terminal_script_keeps_the_window_open_and_reports_failure() {
        let script = terminal_script("false");
        assert!(script.contains("printf '%s\\n' '$ false'\nfalse\nstatus=$?\n"));
        assert!(script.contains("zz update failed (exit %s)"));
        assert!(script.ends_with("read -r _\n"));
    }
}
