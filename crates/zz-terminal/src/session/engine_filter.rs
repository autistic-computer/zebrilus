use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AllowPassthrough {
    #[default]
    Off,
    All,
}

impl AllowPassthrough {
    const fn unwraps(self) -> bool {
        matches!(self, Self::All)
    }
}

const MAX_ENGINE_SEQUENCE_BYTES: usize = 256;
const MAX_ENGINE_RENAME_BYTES: usize = 1024;
const MAX_ENGINE_OSC_BYTES: usize = 64;
const SHELL_INTEGRATION_TITLE_MARK: &[u8] = b"2626";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum EngineState {
    #[default]
    Ground,
    Escape,
    Csi,
    CsiOverflow,
    Osc,
    Rename,
}

const CURSOR_BLINK_MODE_BIT: usize = 3;
const MOUSE_MODE_BITS: u16 = 0b111 << 5;

impl EngineFilter {
    fn track_private_modes(&mut self, parameters: &[u8], final_byte: u8) {
        if final_byte == b'q'
            && let Some(style) = parameters.strip_suffix(b" ")
        {
            if style.is_empty() || style == b"0" {
                self.cursor_blink_set = false;
            }
            return;
        }
        let set = match final_byte {
            b'h' => true,
            b'l' => false,
            _ => return,
        };
        let Some(modes) = parameters.strip_prefix(b"?") else {
            return;
        };
        for mode in modes.split(|byte| *byte == b';') {
            match (mode, set) {
                (b"12", _) => self.cursor_blink_set = true,
                (b"1000", true) => self.mouse_mode = Some(5),
                (b"1002", true) => self.mouse_mode = Some(6),
                (b"1003", true) => self.mouse_mode = Some(7),
                (b"1000" | b"1001" | b"1002" | b"1003", false) => self.mouse_mode = None,
                _ => {}
            }
        }
    }

    pub(super) fn count_output(&mut self) {
        self.output.output(unix_now());
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

pub(super) fn osc_133_exit_status(mark: &[u8]) -> u8 {
    let Some(rest) = mark.get(1..).and_then(|rest| rest.strip_prefix(b";")) else {
        return 0;
    };
    if rest.is_empty() || rest[0] == b'=' || rest[0] == b';' {
        return 0;
    }
    let field = rest.split(|byte| *byte == b';').next().unwrap_or_default();
    if field.contains(&b'=') {
        return 0;
    }
    std::str::from_utf8(field)
        .ok()
        .and_then(|field| field.parse::<i64>().ok())
        .and_then(|status| u8::try_from(status).ok())
        .unwrap_or(255)
}

impl EngineFilter {
    fn mark_output_start(&mut self, terminal: &Terminal<'_, '_>) {
        let (Ok(screen), Ok(row)) = (terminal.active_screen(), terminal.cursor_y()) else {
            return;
        };
        let Ok(mark) = terminal.track_grid_ref(Point::Active(PointCoordinate {
            x: 0,
            y: u32::from(row),
        })) else {
            return;
        };
        self.output_marks.retain(|(_, mark)| mark.has_value());
        let row = mark.point(PointSpace::Screen).ok().flatten();
        if self.output_marks.last().is_some_and(|(last, previous)| {
            *last == screen && previous.point(PointSpace::Screen).ok().flatten() == row
        }) {
            return;
        }
        self.output_marks.push((screen, mark));
    }

    fn erased_rows(
        parameters: &[u8],
        final_byte: u8,
        terminal: &Terminal<'_, '_>,
    ) -> Option<(Screen, u32, u32)> {
        if final_byte != b'J'
            || parameters
                .first()
                .is_some_and(|byte| (0x3c..0x40).contains(byte))
        {
            return None;
        }
        let screen = terminal.active_screen().ok()?;
        let rows = u32::from(terminal.rows().ok()?);
        let columns = terminal.cols().ok()?;
        let x = terminal.cursor_x().ok()?;
        let y = u32::from(terminal.cursor_y().ok()?);
        let first = parameters.split(|byte| *byte == b';').next().unwrap_or(&[]);
        let (start, end) = match engine_parameter(first).unwrap_or(0) {
            0 => (if x == 0 { y } else { y + 1 }, rows.checked_sub(1)?),
            1 => {
                let end = if x >= columns.saturating_sub(1) {
                    y
                } else {
                    y.checked_sub(1)?
                };
                (0, end)
            }
            2 => (0, rows.checked_sub(1)?),
            _ => return None,
        };
        (start <= end).then_some((screen, start, end))
    }

    fn drop_erased_marks(&mut self, erased: (Screen, u32, u32)) {
        let (screen, start, end) = erased;
        self.output_marks.retain(|(owner, mark)| {
            *owner != screen
                || mark
                    .point(PointSpace::Active)
                    .ok()
                    .flatten()
                    .is_none_or(|point| point.y < start || point.y > end)
        });
    }

    fn drop_alternate_marks(
        &mut self,
        parameters: &[u8],
        final_byte: u8,
        terminal: &Terminal<'_, '_>,
    ) {
        if final_byte == b'h'
            && terminal.active_screen().ok() == Some(Screen::Primary)
            && parameters.first() == Some(&b'?')
            && parameters[1..]
                .split(|byte| *byte == b';')
                .any(|parameter| matches!(engine_parameter(parameter), Some(47 | 1047 | 1049)))
        {
            self.output_marks
                .retain(|(screen, _)| *screen != Screen::Alternate);
        }
    }

    fn csi_knobs(&self, knobs: EngineKnobs) -> EngineKnobs {
        EngineKnobs {
            scroll_on_clear: knobs.scroll_on_clear && !self.shell_starting,
            ..knobs
        }
    }

    pub(super) fn output_rows(&self, terminal: &Terminal<'_, '_>) -> Vec<u64> {
        let Ok(active) = terminal.active_screen() else {
            return Vec::new();
        };
        let mut rows = self
            .output_marks
            .iter()
            .filter(|(screen, _)| *screen == active)
            .filter_map(|(_, mark)| mark.point(PointSpace::Screen).ok().flatten())
            .map(|point| u64::from(point.y))
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    pub(super) fn facts(&self, terminal: &Terminal<'_, '_>) -> Result<TerminalFacts, WorkerError> {
        let alternate_on = terminal.active_screen()? == Screen::Alternate;
        let mut private_modes = 0_u16;
        for (bit, number) in PRIVATE_MODE_NUMBERS.iter().enumerate() {
            if terminal
                .mode(Mode::new(*number, ModeKind::Dec))
                .unwrap_or(false)
            {
                private_modes |= 1 << bit;
            }
        }
        if !self.cursor_blink_set {
            private_modes &= !(1 << CURSOR_BLINK_MODE_BIT);
        }
        private_modes &= !MOUSE_MODE_BITS;
        if let Some(bit) = self.mouse_mode {
            private_modes |= 1 << bit;
        }
        Ok(TerminalFacts {
            output: self.output,
            private_modes,
            history_size: if alternate_on {
                self.primary_history_size
            } else {
                terminal.scrollback_rows()?
            },
            cursor_x: if terminal.is_cursor_pending_wrap()? {
                terminal.cols()?
            } else {
                terminal.cursor_x()?
            },
            cursor_y: terminal.cursor_y()?,
            alternate_on,
            mouse_tracking: terminal.is_mouse_tracking()?,
            cursor_hidden: !terminal.is_cursor_visible()?,
            program_title_writes: self.program_title_writes,
        })
    }

    pub(super) fn write(
        &mut self,
        mut bytes: &[u8],
        knobs: EngineKnobs,
        terminal: &mut Terminal<'_, '_>,
        renames: &mut Vec<String>,
        bar: &mut Option<ProgressBar>,
        last_command_status: &mut Option<CommandStatusUpdate>,
    ) {
        while !bytes.is_empty() {
            match self.state {
                EngineState::Ground => {
                    bytes = self.write_ground(bytes, knobs, terminal);
                }
                EngineState::Escape => match bytes[0] {
                    b'[' => {
                        self.sequence.clear();
                        self.state = EngineState::Csi;
                        bytes = &bytes[1..];
                    }
                    b']' => {
                        terminal.vt_write(b"\x1b]");
                        self.state = EngineState::Osc;
                        bytes = &bytes[1..];
                    }
                    b'k' => {
                        self.title.clear();
                        self.state = EngineState::Rename;
                        bytes = &bytes[1..];
                    }
                    other => {
                        if other == b'c' {
                            self.full_reset();
                        }
                        terminal.vt_write(b"\x1b");
                        self.state = EngineState::Ground;
                    }
                },
                EngineState::Csi => {
                    let byte = bytes[0];
                    if byte == 0x1b {
                        terminal.vt_write(b"\x1b[");
                        terminal.vt_write(&self.sequence);
                        self.sequence.clear();
                        self.state = EngineState::Ground;
                        continue;
                    }
                    bytes = &bytes[1..];
                    if byte >= 0x40 {
                        let knobs = self.csi_knobs(knobs);
                        self.metadata_hint |= csi_touches_metadata(&self.sequence, byte);
                        let sequence = std::mem::take(&mut self.sequence);
                        self.track_private_modes(&sequence, byte);
                        let erased = Self::erased_rows(&sequence, byte, terminal);
                        self.drop_alternate_marks(&sequence, byte, terminal);
                        self.sequence = sequence;
                        if csi_needs_rewrite(&self.sequence, byte, knobs) {
                            write_engine_csi(
                                &self.sequence,
                                byte,
                                knobs,
                                terminal,
                                &mut self.primary_history_size,
                            );
                        } else {
                            self.sequence.push(byte);
                            self.sequence.splice(0..0, *b"\x1b[");
                            terminal.vt_write(&self.sequence);
                        }
                        if let Some(erased) = erased {
                            self.drop_erased_marks(erased);
                        }
                        self.sequence.clear();
                        self.state = EngineState::Ground;
                    } else if self.sequence.len() >= MAX_ENGINE_SEQUENCE_BYTES {
                        terminal.vt_write(b"\x1b[");
                        terminal.vt_write(&self.sequence);
                        terminal.vt_write(&[byte]);
                        self.sequence.clear();
                        self.state = EngineState::CsiOverflow;
                    } else {
                        self.sequence.push(byte);
                    }
                }
                EngineState::CsiOverflow => {
                    let byte = bytes[0];
                    if byte == 0x1b {
                        self.state = EngineState::Ground;
                        continue;
                    }
                    bytes = &bytes[1..];
                    terminal.vt_write(&[byte]);
                    if byte >= 0x40 {
                        self.state = EngineState::Ground;
                    }
                }
                EngineState::Osc => {
                    bytes = self.write_osc(bytes, terminal, bar, last_command_status);
                }
                EngineState::Rename => {
                    let byte = bytes[0];
                    bytes = &bytes[1..];
                    match byte {
                        0x1b => {
                            self.finish_rename(knobs, renames);
                            self.state = EngineState::Escape;
                        }
                        0x18 | 0x1a => {
                            self.finish_rename(knobs, renames);
                            self.state = EngineState::Ground;
                        }
                        0x00..=0x1f => {}
                        _ => {
                            if self.title.len() < MAX_ENGINE_RENAME_BYTES {
                                self.title.push(byte);
                            }
                        }
                    }
                }
            }
        }
    }

    fn write_ground<'b>(
        &mut self,
        bytes: &'b [u8],
        knobs: EngineKnobs,
        terminal: &mut Terminal<'_, '_>,
    ) -> &'b [u8] {
        let mut start = 0;
        let mut cursor = 0;
        loop {
            let Some(offset) = find_escape(&bytes[cursor..]) else {
                terminal.vt_write(&bytes[start..]);
                return &[];
            };
            let escape = cursor + offset;
            let Some(&next) = bytes.get(escape + 1) else {
                terminal.vt_write(&bytes[start..escape]);
                self.state = EngineState::Escape;
                return &[];
            };
            match next {
                b'[' => {
                    let mut end = escape + 2;
                    while end < bytes.len() && bytes[end] < 0x40 && bytes[end] != 0x1b {
                        end += 1;
                    }
                    if end >= bytes.len() {
                        let parameters = &bytes[escape + 2..];
                        if parameters.len() >= MAX_ENGINE_SEQUENCE_BYTES {
                            terminal.vt_write(&bytes[start..]);
                            self.state = EngineState::CsiOverflow;
                        } else {
                            terminal.vt_write(&bytes[start..escape]);
                            self.sequence.clear();
                            self.sequence.extend_from_slice(parameters);
                            self.state = EngineState::Csi;
                        }
                        return &[];
                    }
                    if bytes[end] == 0x1b {
                        cursor = end;
                        continue;
                    }
                    let final_byte = bytes[end];
                    cursor = end + 1;
                    if !csi_is_tracked(final_byte) {
                        continue;
                    }
                    let parameters = &bytes[escape + 2..end];
                    let knobs = self.csi_knobs(knobs);
                    self.metadata_hint |= csi_touches_metadata(parameters, final_byte);
                    self.track_private_modes(parameters, final_byte);
                    self.drop_alternate_marks(parameters, final_byte, terminal);
                    let clears_rows = final_byte == b'J' && !self.output_marks.is_empty();
                    if clears_rows || csi_needs_rewrite(parameters, final_byte, knobs) {
                        terminal.vt_write(&bytes[start..escape]);
                        let erased = Self::erased_rows(parameters, final_byte, terminal);
                        if csi_needs_rewrite(parameters, final_byte, knobs) {
                            write_engine_csi(
                                parameters,
                                final_byte,
                                knobs,
                                terminal,
                                &mut self.primary_history_size,
                            );
                        } else {
                            terminal.vt_write(&bytes[escape..=end]);
                        }
                        if let Some(erased) = erased {
                            self.drop_erased_marks(erased);
                        }
                        start = end + 1;
                    }
                }
                b']' => {
                    terminal.vt_write(&bytes[start..escape + 2]);
                    self.state = EngineState::Osc;
                    return &bytes[escape + 2..];
                }
                b'k' => {
                    terminal.vt_write(&bytes[start..escape]);
                    self.title.clear();
                    self.state = EngineState::Rename;
                    return &bytes[escape + 2..];
                }
                b'c' => {
                    self.full_reset();
                    cursor = escape + 2;
                }
                _ => {
                    cursor = escape + 1;
                }
            }
        }
    }

    fn write_osc<'b>(
        &mut self,
        bytes: &'b [u8],
        terminal: &mut Terminal<'_, '_>,
        bar: &mut Option<ProgressBar>,
        last_command_status: &mut Option<CommandStatusUpdate>,
    ) -> &'b [u8] {
        let end = bytes
            .iter()
            .position(|byte| matches!(byte, 0x07 | 0x18 | 0x1a | 0x1b))
            .unwrap_or(bytes.len());
        self.collect_osc(&bytes[..end]);
        let Some(&terminator) = bytes.get(end) else {
            terminal.vt_write(bytes);
            return &[];
        };
        let output_start = !self.osc_overflowed && self.osc.starts_with(b"133;C");
        if terminator == 0x1b {
            terminal.vt_write(&bytes[..end]);
            self.finish_osc(bar, last_command_status, terminator);
            self.state = EngineState::Escape;
        } else {
            terminal.vt_write(&bytes[..=end]);
            self.finish_osc(bar, last_command_status, terminator);
            self.state = EngineState::Ground;
        }
        if output_start {
            self.mark_output_start(terminal);
        }
        &bytes[end + 1..]
    }

    fn collect_osc(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.osc_overflowed {
                return;
            }
            let program_status = self.osc.starts_with(PROGRAM_STATUS_PREFIX);
            if byte < 0x20 && !program_status {
                continue;
            }
            let cap = if program_status {
                MAX_PROGRAM_STATUS_OSC_BYTES
            } else {
                MAX_ENGINE_OSC_BYTES
            };
            if self.osc.len() + self.osc_dropped == cap {
                self.osc_overflowed = true;
            } else if byte < 0x20 {
                self.osc_dropped += 1;
            } else {
                self.osc.push(byte);
            }
        }
    }

    fn finish_osc(
        &mut self,
        bar: &mut Option<ProgressBar>,
        last_command_status: &mut Option<CommandStatusUpdate>,
        terminator: u8,
    ) {
        let osc = std::mem::take(&mut self.osc);
        let overflowed = std::mem::take(&mut self.osc_overflowed);
        self.osc_dropped = 0;
        let integration_title = std::mem::take(&mut self.integration_title);
        self.metadata_hint |= overflowed || osc_touches_metadata(&osc);
        if matches!(osc_command(&osc), Some((0 | 2, _))) {
            self.program_title_writes += u64::from(!integration_title);
            return;
        }
        if !overflowed && let Some(mark) = osc.strip_prefix(b"133;") {
            self.output.osc_133(mark, unix_now());
            let kind = match mark.first() {
                Some(b'A' | b'N') => Some(ShellMarkKind::Prompt),
                Some(b'C') => Some(ShellMarkKind::CommandStarted),
                Some(b'D') => Some(ShellMarkKind::CommandFinished),
                _ => None,
            };
            if let Some(kind) = kind {
                self.shell_marks.push(ShellMark {
                    kind,
                    facts: self.output,
                });
            }
        }
        if is_prompt_start(&osc) {
            self.shell_starting = false;
            self.program_status_changed |= self.program_status.program_left();
            return;
        }
        if overflowed {
            return;
        }
        if osc == SHELL_INTEGRATION_TITLE_MARK {
            self.integration_title = true;
            return;
        }
        if let Some(body) = osc.strip_prefix(PROGRAM_STATUS_PREFIX) {
            match parse_program_status(body) {
                Some(ProgramStatusReport::Query) => self.answer_program_status_query(terminator),
                Some(report) => self.program_status_changed |= self.program_status.apply(report),
                None => {}
            }
            return;
        }
        if let Some(status) = parse_osc_command_status(&osc) {
            *last_command_status = Some(status);
            return;
        }
        let Some((state, progress)) = parse_osc_progress(&osc) else {
            return;
        };
        let before = self.bar;
        self.bar.apply(state, progress);
        if self.bar != before {
            *bar = Some(self.bar);
        }
    }

    fn answer_program_status_query(&self, terminator: u8) {
        if let Some(replies) = &self.replies {
            let reply: &[u8] = if terminator == 0x07 {
                b"\x1b]7501;?\x07"
            } else {
                b"\x1b]7501;?\x1b\\"
            };
            replies.borrow_mut().push(reply);
        }
    }

    fn full_reset(&mut self) {
        self.metadata_hint = true;
        self.cursor_blink_set = false;
        self.mouse_mode = None;
        self.output_marks.clear();
        self.program_status_changed |= self.program_status.reset();
    }

    pub(super) fn take_shell_marks(&mut self) -> Vec<ShellMark> {
        std::mem::take(&mut self.shell_marks)
    }

    pub(super) fn take_program_status(&mut self) -> Option<ProgramStatus> {
        std::mem::take(&mut self.program_status_changed).then(|| self.program_status.clone())
    }

    fn finish_rename(&mut self, knobs: EngineKnobs, renames: &mut Vec<String>) {
        if knobs.allow_rename
            && let Ok(name) = std::str::from_utf8(&self.title)
        {
            renames.push(name.to_owned());
        }
        self.title.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommandStatusUpdate {
    Unknown,
    Exit(i32),
}

impl CommandStatusUpdate {
    pub(super) fn code(self) -> Option<i32> {
        match self {
            Self::Unknown => None,
            Self::Exit(code) => Some(code),
        }
    }
}

fn osc_touches_metadata(payload: &[u8]) -> bool {
    let digits = payload
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(payload.len());
    matches!(&payload[..digits], b"0" | b"1" | b"2" | b"7")
}

pub(super) fn csi_is_tracked(final_byte: u8) -> bool {
    matches!(final_byte, b'J' | b'h' | b'l' | b'p' | b'q' | b't' | b'u')
}

pub(super) fn csi_touches_metadata(parameters: &[u8], final_byte: u8) -> bool {
    match final_byte {
        b'h' | b'l' => parameters.strip_prefix(b"?").is_some_and(|modes| {
            modes.split(|byte| *byte == b';').any(|mode| {
                matches!(
                    mode,
                    b"9" | b"1000"
                        | b"1001"
                        | b"1002"
                        | b"1003"
                        | b"1005"
                        | b"1006"
                        | b"1015"
                        | b"1016"
                )
            })
        }),
        b'u' => matches!(parameters.first(), Some(b'>' | b'<' | b'=')),
        b'p' => parameters == b"!",
        b't' => parameters.starts_with(b"22") || parameters.starts_with(b"23"),
        _ => false,
    }
}

fn is_prompt_start(payload: &[u8]) -> bool {
    payload
        .strip_prefix(b"133;A")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(b";"))
}

fn parse_osc_command_status(payload: &[u8]) -> Option<CommandStatusUpdate> {
    let rest = payload.strip_prefix(b"133;D")?;
    if rest.is_empty() {
        return Some(CommandStatusUpdate::Unknown);
    }
    let status = std::str::from_utf8(rest.strip_prefix(b";")?).ok()?;
    Some(CommandStatusUpdate::Exit(status.parse().ok()?))
}

fn osc_command(payload: &[u8]) -> Option<(u32, &[u8])> {
    let digits = payload
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(payload.len());
    if digits == 0 {
        return None;
    }
    let mut option = 0_u32;
    for byte in &payload[..digits] {
        option = option
            .saturating_mul(10)
            .saturating_add(u32::from(byte - b'0'));
    }
    let rest = match payload.get(digits) {
        None => &payload[digits..],
        Some(b';') => &payload[digits + 1..],
        Some(_) => return None,
    };
    Some((option, rest))
}

fn parse_osc_progress(payload: &[u8]) -> Option<(ProgressBarState, Option<u8>)> {
    let (option, rest) = osc_command(payload)?;
    (option == 9).then(|| parse_osc_9_progress(rest)).flatten()
}

fn parse_osc_9_progress(payload: &[u8]) -> Option<(ProgressBarState, Option<u8>)> {
    let rest = payload.strip_prefix(b"4")?;
    if rest.is_empty() || rest == b";" {
        return None;
    }
    let rest = rest.strip_prefix(b";")?;
    let state = ProgressBarState::from_digit(*rest.first()?)?;
    let rest = &rest[1..];
    if rest.is_empty() || rest == b";" {
        return Some((state, None));
    }
    let rest = rest.strip_prefix(b";")?;
    let mut progress = 0_u32;
    let mut digits = 0_usize;
    for byte in rest {
        if !byte.is_ascii_digit() {
            break;
        }
        if progress > 100 {
            return None;
        }
        progress = progress * 10 + u32::from(byte - b'0');
        digits += 1;
    }
    if digits != rest.len() || progress > 100 {
        return None;
    }
    Some((state, Some(u8::try_from(progress).ok()?)))
}

pub(super) fn csi_needs_rewrite(parameters: &[u8], final_byte: u8, knobs: EngineKnobs) -> bool {
    let private = parameters
        .first()
        .is_some_and(|byte| (0x3c..0x40).contains(byte));
    match final_byte {
        b'J' if knobs.scroll_on_clear && !private => {
            let first = parameters.split(|byte| *byte == b';').next().unwrap_or(&[]);
            matches!(engine_parameter(first).unwrap_or(0), 0 | 2)
        }
        b'h' | b'l' if parameters.first() == Some(&b'?') => parameters[1..]
            .split(|byte| *byte == b';')
            .any(|parameter| matches!(engine_parameter(parameter), Some(47 | 1047 | 1049))),
        _ => false,
    }
}

fn write_engine_csi(
    sequence: &[u8],
    final_byte: u8,
    knobs: EngineKnobs,
    terminal: &mut Terminal<'_, '_>,
    primary_history_size: &mut usize,
) {
    let controls = sequence
        .iter()
        .copied()
        .filter(|byte| *byte < 0x20)
        .collect::<Vec<u8>>();
    let cleaned;
    let sequence = if controls.is_empty() {
        sequence
    } else {
        terminal.vt_write(&controls);
        cleaned = sequence
            .iter()
            .copied()
            .filter(|byte| *byte >= 0x20)
            .collect::<Vec<u8>>();
        cleaned.as_slice()
    };
    if final_byte == b'h'
        && sequence.first() == Some(&b'?')
        && sequence[1..]
            .split(|byte| *byte == b';')
            .any(|parameter| matches!(engine_parameter(parameter), Some(47 | 1047 | 1049)))
        && terminal
            .active_screen()
            .is_ok_and(|screen| screen == Screen::Primary)
    {
        *primary_history_size = terminal.scrollback_rows().unwrap_or_default();
    }
    let private = sequence
        .first()
        .is_some_and(|byte| (0x3c..0x40).contains(byte));
    let mut scrolled_prompt = false;
    if knobs.scroll_on_clear
        && final_byte == b'J'
        && !private
        && erases_whole_screen(sequence, terminal)
        && terminal
            .active_screen()
            .is_ok_and(|screen| screen == Screen::Primary)
        && screen_holds_output(terminal)
        && let Some(rows) = used_screen_rows(terminal)
    {
        scrolled_prompt = cursor_row_semantic_prompt(terminal) == Some(RowSemanticPrompt::Prompt);
        terminal.vt_write(format!("\x1b[{rows}S").as_bytes());
    }
    if !knobs.alternate_screen
        && matches!(final_byte, b'h' | b'l')
        && sequence.first() == Some(&b'?')
    {
        let kept = sequence[1..]
            .split(|byte| *byte == b';')
            .filter(|parameter| !matches!(engine_parameter(parameter), Some(47 | 1047 | 1049)))
            .collect::<Vec<_>>();
        if kept.is_empty() {
            return;
        }
        let mut rewritten = b"\x1b[?".to_vec();
        for (index, parameter) in kept.iter().enumerate() {
            if index > 0 {
                rewritten.push(b';');
            }
            rewritten.extend_from_slice(parameter);
        }
        rewritten.push(final_byte);
        terminal.vt_write(&rewritten);
        return;
    }
    let mut raw = Vec::with_capacity(sequence.len() + 3);
    raw.extend_from_slice(b"\x1b[");
    raw.extend_from_slice(sequence);
    raw.push(final_byte);
    terminal.vt_write(&raw);
    if scrolled_prompt {
        terminal.vt_write(b"\x1b]133;A\x07");
    }
}

fn cursor_row_semantic_prompt(terminal: &Terminal<'_, '_>) -> Option<RowSemanticPrompt> {
    let row = terminal.cursor_y().ok()?;
    terminal
        .grid_ref(Point::Active(PointCoordinate {
            x: 0,
            y: u32::from(row),
        }))
        .ok()?
        .row()
        .ok()?
        .semantic_prompt()
        .ok()
}

fn engine_parameter(parameter: &[u8]) -> Option<u32> {
    if parameter.is_empty() || !parameter.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(parameter).ok()?.parse().ok()
}

fn erases_whole_screen(sequence: &[u8], terminal: &Terminal<'_, '_>) -> bool {
    let first = sequence.split(|byte| *byte == b';').next().unwrap_or(&[]);
    match engine_parameter(first).unwrap_or(0) {
        2 => true,
        0 => terminal.cursor_x().unwrap_or(1) == 0 && terminal.cursor_y().unwrap_or(1) == 0,
        _ => false,
    }
}

fn screen_text_cell(
    terminal: &Terminal<'_, '_>,
    column: u16,
    row: u16,
) -> Option<libghostty_vt::screen::Cell> {
    let cell = terminal
        .grid_ref(Point::Active(PointCoordinate {
            x: column,
            y: u32::from(row),
        }))
        .ok()?
        .cell()
        .ok()?;
    let blank = !cell.has_text().ok()? || cell.codepoint().ok()? == u32::from(' ');
    (!blank).then_some(cell)
}

fn screen_holds_output(terminal: &Terminal<'_, '_>) -> bool {
    let (Ok(rows), Ok(columns)) = (terminal.rows(), terminal.cols()) else {
        return true;
    };
    (0..rows).any(|row| {
        (0..columns).any(|column| {
            screen_text_cell(terminal, column, row).is_some_and(|cell| {
                cell.semantic_content()
                    .is_ok_and(|content| content == CellSemanticContent::Output)
            })
        })
    })
}

pub(super) fn used_screen_rows(terminal: &Terminal<'_, '_>) -> Option<u16> {
    let rows = terminal.rows().ok()?;
    let columns = terminal.cols().ok()?;
    (0..rows).rev().find_map(|row| {
        (0..columns)
            .any(|column| screen_text_cell(terminal, column, row).is_some())
            .then_some(row + 1)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PassthroughState {
    #[default]
    Ground,
    Prefix(usize),
    Payload {
        escape: bool,
    },
    Discard {
        escape: bool,
    },
}

impl PassthroughFilter {
    pub(super) fn set_mode(&mut self, mode: AllowPassthrough) {
        self.mode = mode;
    }

    #[inline]
    fn start_payload(&mut self) {
        self.state = PassthroughState::Payload { escape: false };
    }

    #[inline]
    fn append_payload(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > MAX_TMUX_PASSTHROUGH_PAYLOAD_BYTES.saturating_sub(self.payload.len()) {
            self.payload = Vec::new();
            false
        } else {
            self.payload.extend_from_slice(bytes);
            true
        }
    }

    #[inline]
    pub(super) fn write(&mut self, mut bytes: &[u8], mut sink: impl FnMut(&[u8])) -> usize {
        let mut written = 0;
        loop {
            match self.state {
                PassthroughState::Ground => {
                    let (escape, matched) = if let Some(escape) =
                        memchr::memmem::find(bytes, TMUX_PASSTHROUGH_PREFIX)
                    {
                        (escape, TMUX_PASSTHROUGH_PREFIX.len())
                    } else {
                        let held = held_passthrough_prefix(bytes);
                        if held == 0 {
                            if !bytes.is_empty() {
                                sink(bytes);
                                written += bytes.len();
                            }
                            return written;
                        }
                        (bytes.len() - held, held)
                    };
                    if escape > 0 {
                        sink(&bytes[..escape]);
                        written += escape;
                    }
                    bytes = &bytes[escape + matched..];
                    if matched == TMUX_PASSTHROUGH_PREFIX.len() {
                        self.start_payload();
                    } else {
                        self.state = PassthroughState::Prefix(matched);
                        return written;
                    }
                }
                PassthroughState::Prefix(matched) => {
                    let remaining = &TMUX_PASSTHROUGH_PREFIX[matched..];
                    let continued = common_prefix(bytes, remaining);
                    if continued == remaining.len() {
                        self.start_payload();
                        bytes = &bytes[continued..];
                    } else if continued == bytes.len() {
                        self.state = PassthroughState::Prefix(matched + continued);
                        return written;
                    } else {
                        sink(&TMUX_PASSTHROUGH_PREFIX[..matched]);
                        written += matched;
                        self.state = PassthroughState::Ground;
                    }
                }
                PassthroughState::Payload { escape: false } => {
                    let Some(escape) = find_escape(bytes) else {
                        if !self.append_payload(bytes) {
                            self.state = PassthroughState::Discard { escape: false };
                        }
                        return written;
                    };
                    self.state = if self.append_payload(&bytes[..escape]) {
                        PassthroughState::Payload { escape: true }
                    } else {
                        PassthroughState::Discard { escape: true }
                    };
                    bytes = &bytes[escape + 1..];
                }
                PassthroughState::Payload { escape: true } => {
                    let Some((&next, remaining)) = bytes.split_first() else {
                        return written;
                    };
                    match next {
                        0x1b => {
                            self.state = if self.append_payload(&[0x1b]) {
                                PassthroughState::Payload { escape: false }
                            } else {
                                PassthroughState::Discard { escape: false }
                            };
                            bytes = remaining;
                        }
                        b'\\' => {
                            if self.mode.unwraps() && !self.payload.is_empty() {
                                sink(&self.payload);
                                written += self.payload.len();
                            }
                            self.payload.clear();
                            self.state = PassthroughState::Ground;
                            bytes = remaining;
                        }
                        _ => {
                            self.state = if self.append_payload(&[next]) {
                                PassthroughState::Payload { escape: false }
                            } else {
                                PassthroughState::Discard { escape: false }
                            };
                            bytes = remaining;
                        }
                    }
                }
                PassthroughState::Discard { escape: false } => {
                    let Some(escape) = find_escape(bytes) else {
                        return written;
                    };
                    self.state = PassthroughState::Discard { escape: true };
                    bytes = &bytes[escape + 1..];
                }
                PassthroughState::Discard { escape: true } => {
                    let Some((&next, remaining)) = bytes.split_first() else {
                        return written;
                    };
                    if next == b'\\' {
                        self.state = PassthroughState::Ground;
                    } else {
                        self.state = PassthroughState::Discard { escape: false };
                    }
                    bytes = remaining;
                }
            }
        }
    }
}

#[inline]
#[cfg(unix)]
pub(super) fn force_pty_erase(descriptor: std::os::fd::RawFd, erase: u8) {
    use rustix::termios::{OptionalActions, SpecialCodeIndex, tcgetattr, tcsetattr};

    let Ok(handle) = filedescriptor::FileDescriptor::dup(&descriptor) else {
        return;
    };
    let Ok(mut termios) = tcgetattr(&handle) else {
        return;
    };
    termios.special_codes[SpecialCodeIndex::VERASE] = erase;
    let _ = tcsetattr(&handle, OptionalActions::Now, &termios);
}

fn find_escape(bytes: &[u8]) -> Option<usize> {
    if bytes.first() == Some(&0x1b) {
        return Some(0);
    }
    memchr::memchr(0x1b, bytes)
}

fn held_passthrough_prefix(bytes: &[u8]) -> usize {
    let tail = bytes
        .len()
        .saturating_sub(TMUX_PASSTHROUGH_PREFIX.len() - 1);
    memchr::memrchr(0x1b, &bytes[tail..]).map_or(0, |offset| {
        let held = &bytes[tail + offset..];
        if TMUX_PASSTHROUGH_PREFIX.starts_with(held) {
            held.len()
        } else {
            0
        }
    })
}

fn common_prefix(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

pub(super) fn diagnostic_timer() -> Option<Instant> {
    log::log_enabled!(target: "zz_terminal::diagnostics", log::Level::Trace).then(Instant::now)
}

pub(super) fn diagnostic_elapsed_us(started: Option<Instant>) -> u128 {
    started.map_or(0, |started| started.elapsed().as_micros())
}

#[derive(Default)]
pub(super) struct VtWriteDiagnostics {
    bytes: usize,
    calls: u32,
    micros: u128,
}

impl VtWriteDiagnostics {
    pub(super) fn record(&mut self, bytes: usize, started: Option<Instant>) {
        if started.is_some() && bytes > 0 {
            self.bytes = self.bytes.saturating_add(bytes);
            self.calls = self.calls.saturating_add(1);
            self.micros = self.micros.saturating_add(diagnostic_elapsed_us(started));
        }
    }

    pub(super) fn emit(&mut self) {
        if self.calls > 0 {
            log::trace!(
                target: "zz_terminal::diagnostics::vt",
                "vt_write parsed_bytes={} calls={} elapsed_us={}",
                self.bytes,
                self.calls,
                self.micros,
            );
            *self = Self::default();
        }
    }
}
