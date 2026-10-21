mod catalog;

use std::{collections::HashMap, sync::LazyLock};

use catalog::PROGRAMS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Mark { background: u32, foreground: u32 },
    Art { background: u32 },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Program {
    pub id: &'static str,
    pub name: &'static str,
    pub commands: &'static [&'static str],
    pub icon: Option<&'static str>,
    pub look: Look,
}

impl Program {
    #[must_use]
    pub fn by_id(id: &str) -> Option<&'static Self> {
        PROGRAMS.iter().find(|program| program.id == id)
    }

    #[must_use]
    pub fn all() -> &'static [Self] {
        PROGRAMS
    }
}

const TAIL: usize = 128;

type Index = HashMap<&'static str, Vec<(&'static str, &'static Program)>>;

static INDEX: LazyLock<Index> = LazyLock::new(|| {
    let mut index = Index::new();
    for program in PROGRAMS {
        for key in program.commands {
            index
                .entry(last_component(key))
                .or_default()
                .push((key, program));
        }
    }
    index
});

#[must_use]
pub fn resolve<'a>(candidates: impl IntoIterator<Item = &'a str>) -> Option<&'static Program> {
    let mut buffer = [0; TAIL];
    candidates.into_iter().find_map(|candidate| {
        let path = normalize(candidate, &mut buffer);
        lookup(path).or_else(|| lookup(without_extension(path)?))
    })
}

fn last_component(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn normalize<'b>(candidate: &str, buffer: &'b mut [u8; TAIL]) -> &'b [u8] {
    let bytes = candidate.trim().as_bytes();
    let tail = &bytes[bytes.len().saturating_sub(TAIL)..];
    for (slot, byte) in buffer.iter_mut().zip(tail) {
        *slot = if *byte == b'\\' {
            b'/'
        } else {
            byte.to_ascii_lowercase()
        };
    }
    let path = &buffer[..tail.len()];
    path.strip_suffix(b".exe").unwrap_or(path)
}

fn without_extension(path: &[u8]) -> Option<&[u8]> {
    let name = path.rsplit(|byte| *byte == b'/').next()?;
    let dot = name
        .iter()
        .rposition(|byte| *byte == b'.')
        .filter(|dot| *dot > 0)?;
    Some(&path[..path.len() - name.len() + dot])
}

fn lookup(path: &[u8]) -> Option<&'static Program> {
    let name = path.rsplit(|byte| *byte == b'/').next()?;
    INDEX
        .get(std::str::from_utf8(name).ok()?)?
        .iter()
        .filter(|(key, _)| suffix_at_boundary(path, key.as_bytes()))
        .max_by_key(|(key, _)| key.len())
        .map(|(_, program)| *program)
}

fn suffix_at_boundary(path: &[u8], key: &[u8]) -> bool {
    path.strip_suffix(key)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with(b"/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(candidates: &[&str]) -> Option<&'static str> {
        resolve(candidates.iter().copied()).map(|program| program.id)
    }

    #[test]
    fn matches_whole_path_components_only() {
        assert_eq!(id(&["/opt/homebrew/bin/nvim"]), Some("neovim"));
        assert_eq!(id(&["my-codex-helper"]), None);
        assert_eq!(id(&["/usr/local/bin/xcodex"]), None);
    }

    #[test]
    fn longest_key_wins() {
        assert_eq!(
            id(&["/opt/homebrew/lib/node_modules/@github/copilot/npm-loader.js"]),
            Some("copilot")
        );
        assert_eq!(
            id(&["/usr/lib/node_modules/@mariozechner/pi-coding-agent/dist/cli.js"]),
            Some("pi")
        );
    }

    #[test]
    fn first_candidate_that_matches_wins() {
        assert_eq!(id(&["/home/me/bin/claude.py", "python3"]), Some("claude"));
        assert_eq!(id(&["/opt/homebrew/bin/node", "claude"]), Some("node"));
        assert_eq!(id(&["codex", "sleep", "/bin/sleep"]), Some("codex"));
        assert_eq!(id(&["unknown-thing", "cargo"]), Some("rust"));
    }

    #[test]
    fn folds_case_windows_paths_and_exe() {
        assert_eq!(
            id(&[r"C:\Program Files\Neovim\bin\NVIM.EXE"]),
            Some("neovim")
        );
        assert_eq!(id(&["Python3.12"]), Some("python"));
    }

    #[test]
    fn every_command_resolves_to_its_own_program() {
        for program in Program::all() {
            for command in program.commands {
                assert_eq!(id(&[command]), Some(program.id), "{command}");
            }
        }
    }
}
