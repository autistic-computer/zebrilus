pub(super) const IDLE: &str = if cfg!(windows) {
    "pause >nul"
} else {
    "read _"
};

pub(super) const SUCCEED: &str = if cfg!(windows) { "exit 0" } else { "true" };

pub(super) const COUNTER: &str = if cfg!(windows) {
    "for /L %i in (1,1,2147483647) do @(echo count %i& ping -n 1 127.0.0.1 >nul)"
} else {
    "i=0; while :; do i=$((i+1)); printf 'count %d\\n' $i; sleep 0.01; done"
};

pub(super) fn print_and_idle(text: &str) -> String {
    if cfg!(windows) {
        format!("echo {text}& {IDLE}")
    } else {
        format!("printf {text}; {IDLE}")
    }
}
