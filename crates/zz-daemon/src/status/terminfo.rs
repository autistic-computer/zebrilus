use super::*;

type TerminfoEntries = BTreeMap<String, Option<Arc<Vec<String>>>>;

static TERMINFO_ENTRIES: OnceLock<Mutex<TerminfoEntries>> = OnceLock::new();

pub(crate) fn terminfo_is_warm(environment: &[RawText]) -> bool {
    let Some(term) = environment
        .iter()
        .find_map(|entry| entry.strip_prefix("TERM="))
    else {
        return true;
    };
    TERMINFO_ENTRIES
        .get()
        .is_some_and(|cache| cache.lock().is_ok_and(|cache| cache.contains_key(term)))
}

pub(super) fn terminfo_entries(term: &str) -> Option<Arc<Vec<String>>> {
    terminfo_entries_using(term, &|mut command| command.output_unmasked().ok())
}

fn terminfo_entries_using(
    term: &str,
    output: &dyn Fn(Command) -> Option<std::process::Output>,
) -> Option<Arc<Vec<String>>> {
    let cache = TERMINFO_ENTRIES.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(cached) = cache.lock().ok()?.get(term) {
        return cached.clone();
    }
    let read = read_terminfo_entries(term, output).map(Arc::new);
    if read.is_none() {
        log::warn!(
            target: "zz_daemon::diagnostics::status",
            "no terminfo entry for TERM={term}; the client interrogate answers empty"
        );
    }
    if let Ok(mut cache) = cache.lock() {
        cache.insert(term.to_owned(), read.clone());
    }
    read
}

fn read_terminfo_entries(
    term: &str,
    output: &dyn Fn(Command) -> Option<std::process::Output>,
) -> Option<Vec<String>> {
    if term.is_empty() || term.contains('/') || term.starts_with('-') {
        return None;
    }
    let mut command = Command::new("infocmp");
    command.args(["-x", "-1", term]).stdin(Stdio::null());
    let output = output(command)?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(parse_infocmp_entries(&text))
}

pub(super) fn parse_infocmp_entries(text: &str) -> Vec<String> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(line) = line.strip_suffix(',') else {
            continue;
        };
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            entries.push(format!("{name}={}", decode_terminfo_string(value)));
        } else if let Some((name, value)) = line.split_once('#') {
            let number = match value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
            {
                Some(hex) => i64::from_str_radix(hex, 16).ok(),
                None => value.parse::<i64>().ok(),
            };
            if let Some(number) = number {
                entries.push(format!("{name}={number}"));
            }
        } else if !line.ends_with('@') {
            entries.push(format!("{line}=1"));
        }
    }
    entries
}

fn decode_terminfo_string(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        match byte {
            b'^' => {
                let Some(&control) = bytes.get(index) else {
                    out.push(byte);
                    break;
                };
                index += 1;
                out.push(if control == b'?' {
                    0x7f
                } else {
                    control & 0x1f
                });
            }
            b'\\' => {
                let Some(&next) = bytes.get(index) else {
                    out.push(byte);
                    break;
                };
                index += 1;
                let decoded = match next {
                    b'E' | b'e' => 0x1b,
                    b'n' | b'l' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'f' => 0x0c,
                    b's' => b' ',
                    b'a' => 0x07,
                    b'0'..=b'7' => {
                        let mut number = u32::from(next - b'0');
                        for _ in 0..2 {
                            match bytes.get(index) {
                                Some(digit @ b'0'..=b'7') => {
                                    number = number * 8 + u32::from(digit - b'0');
                                    index += 1;
                                }
                                _ => break,
                            }
                        }
                        if number == 0 {
                            0x80
                        } else {
                            (number & 0xff) as u8
                        }
                    }
                    other => other,
                };
                out.push(decoded);
            }
            _ => out.push(byte),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn warm_terminfo_entries(environment: &[RawText]) {
    warm_terminfo_entries_using(environment, &|mut command| command.output_unmasked().ok());
}

pub(crate) fn warm_terminfo_entries_using(
    environment: &[RawText],
    output: &dyn Fn(Command) -> Option<std::process::Output>,
) {
    for entry in environment {
        if let Some(term) = entry.strip_prefix("TERM=") {
            let _ = terminfo_entries_using(term, output);
            return;
        }
    }
}

struct CachedTtyTerm {
    term: String,
    colour_term: Option<String>,
    negotiated: String,
    terminal_features: Vec<String>,
    terminal_overrides: Vec<String>,
    built: Option<Arc<TtyTerm>>,
}

const TTY_TERM_CACHE_ENTRIES: usize = 64;

pub(crate) fn client_terminal_facts(
    term: &str,
    colour_term: Option<&str>,
    negotiated: &str,
    terminal_features: &[String],
    terminal_overrides: &[String],
) -> Option<Arc<TtyTerm>> {
    static TERMS: parking_lot::Mutex<Vec<CachedTtyTerm>> = parking_lot::Mutex::new(Vec::new());
    let matches = |cached: &CachedTtyTerm| {
        cached.term == term
            && cached.colour_term.as_deref() == colour_term
            && cached.negotiated == negotiated
            && cached.terminal_features == terminal_features
            && cached.terminal_overrides == terminal_overrides
    };
    if let Some(cached) = TERMS.lock().iter().find(|cached| matches(cached)) {
        return cached.built.clone();
    }
    let built = terminfo_entries(term).map(|entries| {
        Arc::new(TtyTerm::create(
            term,
            &entries,
            colour_term,
            negotiated,
            terminal_features,
            terminal_overrides,
        ))
    });
    let mut cache = TERMS.lock();
    if cache.len() >= TTY_TERM_CACHE_ENTRIES {
        cache.clear();
    }
    cache.push(CachedTtyTerm {
        term: term.to_owned(),
        colour_term: colour_term.map(str::to_owned),
        negotiated: negotiated.to_owned(),
        terminal_features: terminal_features.to_vec(),
        terminal_overrides: terminal_overrides.to_vec(),
        built: built.clone(),
    });
    built
}
