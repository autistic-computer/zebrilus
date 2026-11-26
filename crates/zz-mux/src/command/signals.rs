#[must_use]
pub fn tmux_signal_name(value: &str) -> String {
    #[cfg(target_os = "linux")]
    if let Some(signal) = value
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("real-time signal ")
        .and_then(|offset| offset.parse::<i32>().ok())
        .filter(|offset| *offset >= 0)
        .and_then(|offset| nix::libc::SIGRTMIN().checked_add(offset))
        .filter(|signal| *signal <= nix::libc::SIGRTMAX())
    {
        return signal.to_string();
    }
    let name = short_signal_name(value);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use nix::libc;

        let signal = match name.as_str() {
            "hup" => Some(libc::SIGHUP),
            "int" => Some(libc::SIGINT),
            "quit" => Some(libc::SIGQUIT),
            "ill" => Some(libc::SIGILL),
            "trap" => Some(libc::SIGTRAP),
            "abrt" => Some(libc::SIGABRT),
            #[cfg(target_os = "macos")]
            "emt" => Some(libc::SIGEMT),
            "fpe" => Some(libc::SIGFPE),
            "kill" => Some(libc::SIGKILL),
            "bus" => Some(libc::SIGBUS),
            "segv" => Some(libc::SIGSEGV),
            "sys" => Some(libc::SIGSYS),
            "pipe" => Some(libc::SIGPIPE),
            "alrm" => Some(libc::SIGALRM),
            "term" => Some(libc::SIGTERM),
            "urg" => Some(libc::SIGURG),
            "stop" => Some(libc::SIGSTOP),
            "tstp" => Some(libc::SIGTSTP),
            "cont" => Some(libc::SIGCONT),
            "chld" => Some(libc::SIGCHLD),
            "ttin" => Some(libc::SIGTTIN),
            "ttou" => Some(libc::SIGTTOU),
            "io" => Some(libc::SIGIO),
            "xcpu" => Some(libc::SIGXCPU),
            "xfsz" => Some(libc::SIGXFSZ),
            "vtalrm" => Some(libc::SIGVTALRM),
            "prof" => Some(libc::SIGPROF),
            "winch" => Some(libc::SIGWINCH),
            #[cfg(target_os = "macos")]
            "info" => Some(libc::SIGINFO),
            "usr1" => Some(libc::SIGUSR1),
            "usr2" => Some(libc::SIGUSR2),
            #[cfg(target_os = "linux")]
            "pwr" => Some(libc::SIGPWR),
            #[cfg(target_os = "linux")]
            "stkflt" => Some(libc::SIGSTKFLT),
            _ => name.parse::<i32>().ok(),
        };
        let Some(signal) = signal else {
            return name;
        };
        #[cfg(target_os = "linux")]
        return signal.to_string();
        #[cfg(target_os = "macos")]
        return macos_signal_name(signal);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    name
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn macos_signal_name(signal: i32) -> String {
    unsafe extern "C" {
        static sys_signame: [*const std::ffi::c_char; 32];
    }
    if signal > 0 && signal < 32 {
        let name = unsafe { sys_signame[signal as usize] };
        if !name.is_null() {
            return unsafe { std::ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned();
        }
    }
    signal.to_string()
}

fn short_signal_name(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let lower = value.to_ascii_lowercase();
    let short = lower.strip_prefix("sig").unwrap_or(&lower);
    if matches!(
        short,
        "hup"
            | "int"
            | "quit"
            | "ill"
            | "trap"
            | "abrt"
            | "emt"
            | "fpe"
            | "kill"
            | "bus"
            | "segv"
            | "sys"
            | "pipe"
            | "alrm"
            | "term"
            | "urg"
            | "stop"
            | "tstp"
            | "cont"
            | "chld"
            | "ttin"
            | "ttou"
            | "io"
            | "xcpu"
            | "xfsz"
            | "vtalrm"
            | "prof"
            | "winch"
            | "info"
            | "usr1"
            | "usr2"
            | "pwr"
            | "stkflt"
    ) {
        return short.to_owned();
    }
    for (description, name) in [
        ("hangup", "hup"),
        ("interrupt", "int"),
        ("quit", "quit"),
        ("illegal instruction", "ill"),
        ("trace/bpt trap", "trap"),
        ("trace/breakpoint trap", "trap"),
        ("trace trap", "trap"),
        ("abort trap", "abrt"),
        ("aborted", "abrt"),
        ("emt trap", "emt"),
        ("floating point exception", "fpe"),
        ("killed", "kill"),
        ("bus error", "bus"),
        ("segmentation fault", "segv"),
        ("bad system call", "sys"),
        ("broken pipe", "pipe"),
        ("alarm clock", "alrm"),
        ("terminated", "term"),
        ("urgent i/o condition", "urg"),
        ("stopped (signal)", "stop"),
        ("stopped (tty input)", "ttin"),
        ("stopped (tty output)", "ttou"),
        ("stopped", "tstp"),
        ("continued", "cont"),
        ("child exited", "chld"),
        ("i/o possible", "io"),
        ("cpu time limit exceeded", "xcpu"),
        ("cputime limit exceeded", "xcpu"),
        ("file size limit exceeded", "xfsz"),
        ("filesize limit exceeded", "xfsz"),
        ("virtual timer expired", "vtalrm"),
        ("profiling timer expired", "prof"),
        ("window size changes", "winch"),
        ("window changed", "winch"),
        ("information request", "info"),
        ("user defined signal 1", "usr1"),
        ("user defined signal 2", "usr2"),
        ("power failure", "pwr"),
        ("stack fault", "stkflt"),
    ] {
        if lower.starts_with(description) {
            return name.to_owned();
        }
    }
    let number = lower
        .split(|character: char| !character.is_ascii_digit())
        .rfind(|part| !part.is_empty())
        .map(str::to_owned);
    number.unwrap_or(lower)
}
