#[cfg(target_os = "macos")]
use std::path::Path;

use zz_protocol::CommandInvocation;

#[test]
fn a_bare_command_line_is_a_client_unless_launch_services_or_windows_opened_it() {
    use super::{CommandLineOrigin, bare_command_line_opens_application};

    assert!(!bare_command_line_opens_application(
        CommandLineOrigin::Launcher,
        true
    ));
    assert!(!bare_command_line_opens_application(
        CommandLineOrigin::Launcher,
        false
    ));
    assert_eq!(
        bare_command_line_opens_application(CommandLineOrigin::Application, false),
        cfg!(windows)
    );
    assert!(bare_command_line_opens_application(
        CommandLineOrigin::Application,
        true
    ));
}

#[test]
fn the_option_default_keeps_the_launcher_create_or_attach_and_a_set_list_wins() {
    assert!(super::is_launcher_default_client_command(&[
        CommandInvocation::new("new-session", [] as [&str; 0])
    ]));
    assert!(!super::is_launcher_default_client_command(&[
        CommandInvocation::new("new-session", ["-A"])
    ]));
    assert!(!super::is_launcher_default_client_command(&[
        CommandInvocation::new("new-session", [] as [&str; 0]),
        CommandInvocation::new("new-session", [] as [&str; 0]),
    ]));
    assert!(!super::is_launcher_default_client_command(&[]));
}

#[test]
fn zz_app_finds_the_desktop_binary_and_never_picks_the_launcher() {
    let directory = tempfile::tempdir().expect("temporary installation");
    let directory = directory
        .path()
        .canonicalize()
        .expect("canonical directory");
    let name = if cfg!(windows) { "zz.exe" } else { "zz" };
    let bin = directory.join("bin");
    let lib = directory.join("lib/zz");
    let packaged = directory.join("usr/lib/zz");
    for path in [&bin, &lib, &packaged] {
        std::fs::create_dir_all(path).expect("create directory");
    }
    let headless = bin.join(name);
    std::fs::write(&headless, b"").expect("write headless zz");
    assert_eq!(super::desktop_executable_from(&headless, &[]), None);

    let cli = lib.join(if cfg!(windows) { "cli.exe" } else { "cli" });
    std::fs::write(&cli, b"").expect("write bundle cli");
    std::fs::write(lib.join(name), b"").expect("write bundle gui");
    assert_eq!(
        super::desktop_executable_from(&cli, std::slice::from_ref(&packaged)),
        Some(lib.join(name))
    );

    assert_eq!(
        super::desktop_executable_from(&headless, &[bin.clone(), packaged.clone()]),
        None
    );
    std::fs::write(packaged.join(name), b"").expect("write packaged gui");
    assert_eq!(
        super::desktop_executable_from(&headless, &[bin, packaged.clone()]),
        Some(packaged.join(name))
    );
}

#[cfg(target_os = "macos")]
#[test]
fn app_launch_preserves_the_callers_working_directory() {
    assert_eq!(
        super::startup_directory_environment(Path::new("/tmp/a project")),
        std::ffi::OsString::from("ZZ_APP_STARTUP_DIRECTORY=/tmp/a project")
    );
}
