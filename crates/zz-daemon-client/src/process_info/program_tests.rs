use std::{collections::BTreeMap, time::Instant};

use super::*;

fn arguments(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn settled_candidates(pid: u32, ready: impl Fn(&[String]) -> bool) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let candidates = program_candidates(pid, Some(pid));
        if ready(&candidates) || Instant::now() > deadline {
            return candidates;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn named(name: &str, parent: Option<u32>) -> ProgramProcess {
    ProgramProcess {
        name: name.to_owned(),
        arguments: arguments(&[name]),
        parent,
        ..ProgramProcess::default()
    }
}

#[test]
fn a_script_is_the_first_argument_naming_a_file_under_the_cwd() {
    let cwd = tempfile::tempdir().expect("cwd");
    let script = cwd.path().join("claude.py");
    fs::write(&script, "").expect("script");
    fs::create_dir(cwd.path().join("folder")).expect("folder");
    let found = |values: &[&str]| script_path(&arguments(values), Some(cwd.path()));

    assert_eq!(
        found(&["python3", "-u", "folder", "missing.py", "claude.py", "x"]),
        Some(script.clone())
    );
    assert_eq!(
        found(&["node", "-r", "dotenv/config", "claude.py"]),
        Some(script.clone())
    );
    assert_eq!(
        found(&["python3", "-Wignore", "-Xdev", "claude.py"]),
        Some(script.clone())
    );
    assert_eq!(
        found(&["python3", "1", "2", "3", "4", "5", "6", "7", "claude.py"]),
        Some(script.clone())
    );
    assert_eq!(
        found(&[
            "python3",
            "1",
            "2",
            "3",
            "4",
            "5",
            "6",
            "7",
            "8",
            "claude.py"
        ]),
        None
    );
    for inline in [
        &["python3", "-c", "print()", "claude.py"][..],
        &["python3", "-uc", "print()", "claude.py"],
        &["python3", "-m", "claude", "claude.py"],
        &["perl", "-ne", "print", "claude.py"],
        &["ruby", "-e", "puts 1", "claude.py"],
        &["node", "--eval=1", "claude.py"],
        &["node", "--print", "1", "claude.py"],
    ] {
        assert_eq!(found(inline), None, "{inline:?}");
    }

    let absolute = text(&script);
    assert_eq!(
        script_path(&arguments(&["node", &absolute]), None),
        Some(script)
    );
    assert_eq!(script_path(&arguments(&["node", "claude.py"]), None), None);
}

#[test]
fn an_interpreter_lists_script_bare_script_invoked_argv0_name_and_executable() {
    let root = tempfile::tempdir().expect("root");
    let bin = root.path().join("bin");
    let work = root.path().join("work");
    fs::create_dir_all(&bin).expect("bin");
    fs::create_dir_all(&work).expect("work");
    let executable = bin.join("python3");
    fs::write(&executable, "").expect("executable");
    fs::write(work.join("claude.py"), "").expect("script");
    let process = ProgramProcess {
        name: "Python".to_owned(),
        executable: Some(executable.clone()),
        arguments: arguments(&["python3", "-u", "claude.py", "--resume"]),
        cwd: Some(work.clone()),
        search_path: Some(
            env::join_paths([root.path().join("missing"), bin.clone()]).expect("PATH"),
        ),
        parent: None,
    };

    assert_eq!(
        process.candidates(),
        [
            text(&work.join("claude.py")),
            text(&work.join("claude")),
            text(&executable),
            "python3".to_owned(),
            "Python".to_owned(),
            text(&executable),
        ]
    );

    let cat = bin.join("cat");
    fs::write(&cat, "").expect("cat");
    let tool = ProgramProcess {
        name: "cat".to_owned(),
        executable: Some(cat.clone()),
        arguments: arguments(&["cat", "claude.py"]),
        ..process
    };
    assert_eq!(
        tool.candidates(),
        [text(&cat), "cat".to_owned(), "cat".to_owned(), text(&cat)]
    );
}

#[test]
fn the_invoked_path_must_name_the_running_executable() {
    let root = tempfile::tempdir().expect("root");
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).expect("bin");
    let sleep = bin.join("sleep");
    fs::write(&sleep, "").expect("sleep");
    fs::write(bin.join("codex"), "").expect("codex");
    let process = ProgramProcess {
        name: "sleep".to_owned(),
        executable: Some(sleep.clone()),
        arguments: arguments(&["codex", "30"]),
        cwd: Some(root.path().to_path_buf()),
        search_path: Some(bin.clone().into_os_string()),
        parent: None,
    };
    assert_eq!(
        process.candidates(),
        ["codex".to_owned(), "sleep".to_owned(), text(&sleep)]
    );

    let relative = Path::new("bin").join("sleep");
    let process = ProgramProcess {
        arguments: vec![relative.clone().into_os_string()],
        ..process
    };
    assert_eq!(
        process.candidates(),
        [
            text(&root.path().join(&relative)),
            text(&relative),
            "sleep".to_owned(),
            text(&sleep),
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_to_the_executable_is_the_invoked_path() {
    let root = tempfile::tempdir().expect("root");
    let versions = root.path().join("versions");
    let bin = root.path().join("bin");
    fs::create_dir_all(&versions).expect("versions");
    fs::create_dir_all(&bin).expect("bin");
    let executable = versions.join("2.1.99");
    fs::write(&executable, "").expect("executable");
    std::os::unix::fs::symlink(&executable, bin.join("claude")).expect("symlink");
    let process = ProgramProcess {
        name: "2.1.99".to_owned(),
        executable: Some(executable.clone()),
        arguments: arguments(&["claude", "--resume"]),
        cwd: None,
        search_path: Some(bin.clone().into_os_string()),
        parent: None,
    };
    assert_eq!(
        process.candidates(),
        [
            text(&bin.join("claude")),
            "claude".to_owned(),
            "2.1.99".to_owned(),
            text(&executable),
        ]
    );
}

#[test]
fn the_chain_runs_from_the_foreground_up_to_the_shell() {
    let table = BTreeMap::from([
        (30, named("vim", Some(20))),
        (20, named("claude", Some(10))),
        (10, named("zsh", Some(5))),
        (5, named("zz", Some(1))),
        (40, named("node", Some(41))),
        (41, named("bash", Some(20))),
        (50, named("htop", Some(5))),
        (60, named("orphan", Some(61))),
    ]);
    let chain = |start, shell| program_chain(start, shell, |pid| table.get(&pid).cloned());

    assert_eq!(chain(30, Some(10)), ["vim", "claude"]);
    assert_eq!(chain(30, Some(20)), ["vim"]);
    assert_eq!(chain(10, Some(10)), Vec::<String>::new());
    assert_eq!(chain(40, Some(10)), ["node"]);
    assert_eq!(chain(50, Some(50)), ["htop"]);
    assert_eq!(chain(60, Some(10)), ["orphan"]);
    assert_eq!(chain(0, None), Vec::<String>::new());
}

#[test]
fn the_chain_stops_after_sixteen_processes() {
    let table = (2..100)
        .map(|pid| (pid, named(&format!("p{pid}"), Some(pid + 1))))
        .collect::<BTreeMap<_, _>>();
    let chain = program_chain(2, None, |pid| table.get(&pid).cloned());
    assert_eq!(chain.len(), PROGRAM_DEPTH);
    assert_eq!(chain.first().map(String::as_str), Some("p2"));
    assert_eq!(chain.last().map(String::as_str), Some("p17"));
}

#[test]
fn shells_and_interpreters_are_known_in_any_spelling() {
    for name in [
        "zsh",
        "-zsh",
        "/bin/bash",
        "PowerShell.exe",
        r"C:\Windows\System32\cmd.exe",
        "nu",
    ] {
        assert!(is_shell_name(name), "{name}");
    }
    for name in ["nvim", "bashtop", "zshrc", "node", ""] {
        assert!(!is_shell_name(name), "{name}");
    }
    for name in [
        "python3.12",
        "Python",
        "/usr/bin/node",
        "nodejs",
        "ruby3.3",
        "perl5.34",
        "php8.3",
        "bun",
        "deno.exe",
    ] {
        assert!(is_interpreter_name(name), "{name}");
    }
    for name in ["pythonista", "nvim", "bunx", "sleep"] {
        assert!(!is_interpreter_name(name), "{name}");
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_running_process_leads_with_its_argv0() {
    use std::os::unix::process::CommandExt as _;

    let mut child = std::process::Command::new("/bin/sleep")
        .arg0("codex")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();
    let candidates = settled_candidates(pid, |candidates| {
        candidates.first().is_some_and(|first| first == "codex")
    });
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(candidates.first().map(String::as_str), Some("codex"));
    assert!(candidates.iter().any(|candidate| candidate == "sleep"));
    assert!(
        candidates
            .last()
            .is_some_and(|candidate| Path::new(candidate).is_absolute())
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_running_process_finds_its_invoked_link_on_its_own_path() {
    use std::os::unix::process::CommandExt as _;

    let bin = tempfile::tempdir().expect("bin");
    let link = bin.path().join("claude");
    std::os::unix::fs::symlink("/bin/sleep", &link).expect("symlink");
    let mut child = std::process::Command::new(&link)
        .arg0("claude")
        .arg("30")
        .env("PATH", bin.path())
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("spawn the link");
    let pid = child.id();
    let candidates = settled_candidates(pid, |candidates| candidates.first() == Some(&text(&link)));
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(candidates[..2], [text(&link), "claude".to_owned()]);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_new_process_has_run_for_less_time_than_this_one() {
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let young = running_for(child.id());
    let _ = child.kill();
    let _ = child.wait();
    let young = young.expect("the child's age");
    let old = running_for(std::process::id()).expect("this process's age");
    assert!(young < Duration::from_secs(5), "{young:?}");
    assert!(old >= young, "{old:?} {young:?}");
    assert_eq!(running_for(u32::MAX), None);
}
