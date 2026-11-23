use std::{
    fs, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use ala::{AnyElement, App, AsyncApp, ClipboardItem, Entity, Global, Window, prelude::*};
use penna::{
    IconName,
    blocks::notification::Notification,
    components::{
        button::{Button, ButtonVariants as _},
        progress::Progress,
    },
    h_flex, v_flex,
};
use semver::Version;
use zz_config::update::{
    ApplyOutcome, Progress as Step, Staged, Target, UpdateError, apply, checks_enabled,
    clear_staged, detect, fetch_latest, plan, prepare, staged, staged_for,
};
pub(crate) use zz_config::update::{Channel, InstallPlan, ManualUpdate, Release};

use crate::{
    config,
    mux::{
        client::MuxClient,
        hosts::{HostId, session_count},
    },
    user_data,
    window::toast,
};

const INITIAL_DELAY: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_hours(24);
const TOAST_KEY: &str = "update-available";
const COPIED_TOAST_KEY: &str = "update-copied";
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
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    Running {
        detail: String,
    },
    Verifying,
    Staging,
    Ready(Staged),
    Restarting,
    Failed {
        message: String,
        command: Option<String>,
    },
}

impl Stage {
    const fn busy(&self) -> bool {
        matches!(
            self,
            Self::Downloading { .. }
                | Self::Running { .. }
                | Self::Verifying
                | Self::Staging
                | Self::Restarting
        )
    }

    pub(crate) fn bar(&self) -> Option<Bar> {
        match self {
            Self::Downloading {
                received,
                total: Some(total),
            } if *total > 0 => Some(Bar::Filled(fraction(*received, *total))),
            Self::Downloading { .. }
            | Self::Running { .. }
            | Self::Verifying
            | Self::Staging
            | Self::Restarting => Some(Bar::Sweeping),
            Self::Ready(_) | Self::Failed { .. } => None,
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
    fallback_command: Option<String>,
}

impl Install {
    fn new(release: Release, stage: Stage, fallback_command: Option<String>) -> Self {
        Self {
            release,
            stage,
            cancel: Arc::new(AtomicBool::new(false)),
            fallback_command,
        }
    }
}

pub(crate) struct UpdateState {
    mux: Entity<MuxClient>,
    current: Version,
    check: CheckState,
    plan: Option<InstallPlan>,
    dismissed: Option<Version>,
    install: Option<Install>,
}

impl Global for UpdateState {}

pub(crate) struct Status {
    pub current: Version,
    pub channel: Channel,
    pub check: CheckState,
    pub plan: Option<InstallPlan>,
    pub install: Option<(Version, Stage)>,
}

pub(crate) fn status(cx: &App) -> Option<Status> {
    let state = cx.try_global::<UpdateState>()?;
    Some(Status {
        current: state.current.clone(),
        channel: Channel::of(&state.current),
        check: state.check.clone(),
        plan: state.plan.clone(),
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

pub(crate) fn copy_command(command: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(command.to_owned()));
    toast::push(
        Notification::success(command.to_owned())
            .title("Copied update command")
            .key(COPIED_TOAST_KEY),
        cx,
    );
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
        plan: None,
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

pub(crate) fn install(_window: &mut Window, cx: &mut App) {
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
    let channel = Channel::of(&state.current);
    let chosen = state
        .plan
        .clone()
        .unwrap_or_else(|| plan(&release, channel));
    match chosen {
        InstallPlan::InApp(target) => start_install(release, target, cx),
        InstallPlan::Manual(manual) => {
            if let Some(command) = manual.command {
                copy_command(&command, cx);
            } else {
                cx.open_url(&release.url);
            }
        }
    }
}

pub(crate) fn cancel(cx: &mut App) {
    if let Some(install) = install_mut(cx)
        && matches!(
            install.stage,
            Stage::Downloading { .. } | Stage::Running { .. }
        )
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
        let applying = staged.clone();
        let result = cx
            .background_executor()
            .spawn(async move { apply(&applying) })
            .await;
        cx.update(|cx| match result {
            Ok(ApplyOutcome::Relaunch(relaunch)) => {
                log::info!(target: LOG_TARGET, "relaunching {}", relaunch.display());
                cx.set_restart_path(relaunch);
                cx.restart();
            }
            Ok(ApplyOutcome::Quit) => {
                log::info!(target: LOG_TARGET, "quitting for external installer");
                cx.quit();
            }
            Err(UpdateError::Cancelled) => {
                log::info!(target: LOG_TARGET, "update elevation cancelled");
                if let Some(install) = install_mut(cx) {
                    install.stage = Stage::Ready(staged);
                }
                show_toast(Show::Always, cx);
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

fn start_install(release: Release, target: Target, cx: &mut App) {
    log::info!(target: LOG_TARGET, "updating zz {} for {target:?}", release.version);
    let initial = match &target {
        Target::Homebrew { .. } => Stage::Running {
            detail: String::new(),
        },
        _ => Stage::Downloading {
            received: 0,
            total: None,
        },
    };
    let install = Install::new(release.clone(), initial, target.fallback_command());
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
        cx.update(|cx| finish_install(result, cx));
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
        Step::Running { detail } => Stage::Running { detail },
        Step::Verifying => Stage::Verifying,
        Step::Staging => Stage::Staging,
    };
    show_toast(Show::IfShowing, cx);
}

fn finish_install(result: Result<Staged, UpdateError>, cx: &mut App) {
    if !cx.has_global::<UpdateState>() {
        return;
    }
    let state = cx.global_mut::<UpdateState>();
    match result {
        Ok(staged) => {
            log::info!(target: LOG_TARGET, "zz {} is ready at {}", staged.version, staged.path.display());
            if let Some(install) = state.install.as_mut() {
                install.stage = Stage::Ready(staged);
            }
            show_toast(Show::Always, cx);
        }
        Err(UpdateError::Cancelled) => {
            log::info!(target: LOG_TARGET, "update cancelled");
            state.install = None;
            show_toast(Show::IfShowing, cx);
        }
        Err(error) => fail(&error, cx),
    }
}

fn fail(error: &UpdateError, cx: &mut App) {
    log::warn!(target: LOG_TARGET, "update failed: {error}");
    if let Some(install) = install_mut(cx) {
        let command = install.fallback_command.clone();
        install.stage = Stage::Failed {
            message: error.to_string(),
            command,
        };
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
    let (result, chosen, found) = cx
        .background_executor()
        .spawn(async move {
            let result = fetch_latest(channel);
            let available = match &result {
                Ok(Some(release)) if release.version > current => Some(release),
                _ => None,
            };
            let chosen = available.map(|release| plan(release, channel));
            let found = idle
                .then(|| reconcile_staged(available.map(|release| &release.version)))
                .flatten();
            (result, chosen, found)
        })
        .await;
    cx.update(|cx| finish_check(result, chosen, found, trigger, cx));
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
    chosen: Option<InstallPlan>,
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
        .is_some_and(|install| matches!(install.stage, Stage::Failed { .. }))
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
                let fallback = staged.target.fallback_command();
                state.install = Some(Install::new(
                    release.clone(),
                    Stage::Ready(staged),
                    fallback,
                ));
            }
            (CheckState::Available(release), offer)
        }
        Ok(Some(_)) => (CheckState::UpToDate, false),
    };
    state.plan = chosen;
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
    CopyCommand,
    CopyFallback,
    WhatsNew,
    Dismiss,
    Cancel,
    Restart,
    Hide,
    TryAgain,
    ReleasePage,
    ReleasePagePrimary,
}

#[derive(Clone, Debug, PartialEq)]
struct ToastView {
    title: String,
    message: Option<String>,
    command: Option<String>,
    bar: Option<Bar>,
    actions: &'static [Action],
}

fn toast_view(
    current: &Version,
    version: &Version,
    stage: Option<&Stage>,
    plan: Option<&InstallPlan>,
    note: Option<String>,
) -> ToastView {
    let view = |title: String,
                message: Option<String>,
                command: Option<String>,
                actions: &'static [Action]| ToastView {
        title,
        message,
        command,
        bar: stage.and_then(Stage::bar),
        actions,
    };
    let with_note = |base: String| match &note {
        Some(note) => format!("{base} {note}"),
        None => base,
    };
    match stage {
        None => match plan {
            Some(InstallPlan::Manual(ManualUpdate { owner, command })) => match command {
                Some(command) => view(
                    format!("zz {version} is available"),
                    Some(with_note(format!(
                        "You are running {current}. Installed by {owner}. Run: {command}"
                    ))),
                    Some(command.clone()),
                    &[Action::CopyCommand, Action::WhatsNew, Action::Dismiss],
                ),
                None => view(
                    format!("zz {version} is available"),
                    Some(with_note(format!(
                        "You are running {current}. Installed by {owner}."
                    ))),
                    None,
                    &[Action::ReleasePagePrimary, Action::Dismiss],
                ),
            },
            Some(InstallPlan::InApp(_)) | None => view(
                format!("zz {version} is available"),
                Some(with_note(format!("You are running {current}."))),
                None,
                &[Action::Update, Action::WhatsNew, Action::Dismiss],
            ),
        },
        Some(Stage::Downloading { received, total }) => view(
            format!("Downloading zz {version}"),
            Some(download_text(*received, *total)),
            None,
            &[Action::Cancel],
        ),
        Some(Stage::Running { detail }) => view(
            format!("Updating zz {version}"),
            Some(if detail.is_empty() {
                "Running Homebrew…".to_owned()
            } else {
                detail.clone()
            }),
            None,
            &[Action::Cancel],
        ),
        Some(Stage::Verifying) => view(format!("Verifying zz {version}"), None, None, &[]),
        Some(Stage::Staging) => view(format!("Preparing zz {version}"), None, None, &[]),
        Some(Stage::Ready(_)) => view(
            format!("zz {version} is ready"),
            note,
            None,
            &[Action::Restart, Action::Hide],
        ),
        Some(Stage::Restarting) => view(format!("Installing zz {version}"), None, None, &[]),
        Some(Stage::Failed { message, command }) => view(
            "Update failed".to_owned(),
            Some(sentence(message)),
            command.clone(),
            if command.is_some() {
                &[Action::TryAgain, Action::CopyFallback, Action::ReleasePage]
            } else {
                &[Action::TryAgain, Action::ReleasePage]
            },
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
    let view = toast_view(
        &state.current,
        &release.version,
        shown,
        state.plan.as_ref(),
        note,
    );
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
        command,
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
                        actions.iter().map(|action| {
                            toast_button(*action, &version, &url, command.as_deref())
                        }),
                    ),
                )
            })
            .into_any_element()
    })
}

fn toast_button(action: Action, version: &Version, url: &str, command: Option<&str>) -> AnyElement {
    let url = url.to_owned();
    let version = version.clone();
    match action {
        Action::Update => Button::new("update-install")
            .primary()
            .label("Update")
            .on_click(|_, window, cx| install(window, cx)),
        Action::CopyCommand => {
            let copied = command.unwrap_or_default().to_owned();
            Button::new("update-copy")
                .primary()
                .icon(IconName::Copy)
                .label("Copy command")
                .on_click(move |_, _, cx| copy_command(&copied, cx))
        }
        Action::CopyFallback => {
            let copied = command.unwrap_or_default().to_owned();
            Button::new("update-copy-fallback")
                .ghost()
                .icon(IconName::Copy)
                .label("Copy command")
                .on_click(move |_, _, cx| copy_command(&copied, cx))
        }
        Action::WhatsNew => Button::new("update-notes")
            .ghost()
            .label("What's new")
            .on_click(move |_, _, cx| cx.open_url(&url)),
        Action::Dismiss => {
            use penna::WindowExt as _;
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
            use penna::WindowExt as _;
            Button::new("update-hide")
                .ghost()
                .label("Later")
                .on_click(move |_, window, cx| {
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
        Action::ReleasePagePrimary => Button::new("update-release-page-primary")
            .primary()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("valid version")
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
                elevated: false,
            },
            path: PathBuf::from("/Applications/.zz-update.app"),
        };
        let manual_with_command = InstallPlan::Manual(ManualUpdate {
            owner: "pacman (zz-bin)".to_owned(),
            command: Some("paru -S zz-bin".to_owned()),
        });
        let manual_without_command = InstallPlan::Manual(ManualUpdate {
            owner: "Nix".to_owned(),
            command: None,
        });
        let cases = [
            (
                None,
                None,
                ToastView {
                    title: "zz 0.3.2 is available".to_owned(),
                    message: Some(
                        "You are running 0.3.1. The update keeps your sessions running.".to_owned(),
                    ),
                    command: None,
                    bar: None,
                    actions: &[Action::Update, Action::WhatsNew, Action::Dismiss],
                },
            ),
            (
                None,
                Some(&manual_with_command),
                ToastView {
                    title: "zz 0.3.2 is available".to_owned(),
                    message: Some(
                        "You are running 0.3.1. Installed by pacman (zz-bin). Run: paru -S zz-bin \
                         The update keeps your sessions running."
                            .to_owned(),
                    ),
                    command: Some("paru -S zz-bin".to_owned()),
                    bar: None,
                    actions: &[Action::CopyCommand, Action::WhatsNew, Action::Dismiss],
                },
            ),
            (
                None,
                Some(&manual_without_command),
                ToastView {
                    title: "zz 0.3.2 is available".to_owned(),
                    message: Some(
                        "You are running 0.3.1. Installed by Nix. \
                         The update keeps your sessions running."
                            .to_owned(),
                    ),
                    command: None,
                    bar: None,
                    actions: &[Action::ReleasePagePrimary, Action::Dismiss],
                },
            ),
            (
                Some(Stage::Downloading {
                    received: 84_400_000,
                    total: Some(186_000_000),
                }),
                None,
                ToastView {
                    title: "Downloading zz 0.3.2".to_owned(),
                    message: Some("84 of 186 MB".to_owned()),
                    command: None,
                    bar: Some(Bar::Filled(0.453)),
                    actions: &[Action::Cancel],
                },
            ),
            (
                Some(Stage::Running {
                    detail: "==> Downloading zz".to_owned(),
                }),
                None,
                ToastView {
                    title: "Updating zz 0.3.2".to_owned(),
                    message: Some("==> Downloading zz".to_owned()),
                    command: None,
                    bar: Some(Bar::Sweeping),
                    actions: &[Action::Cancel],
                },
            ),
            (
                Some(Stage::Staging),
                None,
                ToastView {
                    title: "Preparing zz 0.3.2".to_owned(),
                    message: None,
                    command: None,
                    bar: Some(Bar::Sweeping),
                    actions: &[],
                },
            ),
            (
                Some(Stage::Ready(staged)),
                None,
                ToastView {
                    title: "zz 0.3.2 is ready".to_owned(),
                    message: note(),
                    command: None,
                    bar: None,
                    actions: &[Action::Restart, Action::Hide],
                },
            ),
            (
                Some(Stage::Failed {
                    message: "cannot write to /Applications".to_owned(),
                    command: None,
                }),
                None,
                ToastView {
                    title: "Update failed".to_owned(),
                    message: Some("Cannot write to /Applications".to_owned()),
                    command: None,
                    bar: None,
                    actions: &[Action::TryAgain, Action::ReleasePage],
                },
            ),
            (
                Some(Stage::Failed {
                    message: "Homebrew does not have zz 0.3.2 yet".to_owned(),
                    command: Some("/opt/homebrew/bin/brew upgrade --cask zz".to_owned()),
                }),
                None,
                ToastView {
                    title: "Update failed".to_owned(),
                    message: Some("Homebrew does not have zz 0.3.2 yet".to_owned()),
                    command: Some("/opt/homebrew/bin/brew upgrade --cask zz".to_owned()),
                    bar: None,
                    actions: &[Action::TryAgain, Action::CopyFallback, Action::ReleasePage],
                },
            ),
        ];
        for (stage, plan, expected) in cases {
            assert_eq!(
                toast_view(&current, &next, stage.as_ref(), plan, note()),
                expected,
                "{stage:?} {plan:?}"
            );
        }
    }
}
