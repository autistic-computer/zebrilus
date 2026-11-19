pub fn overview_count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostState {
    Connected,
    Connecting,
    Offline,
    Failed,
}

#[must_use]
pub fn session_in_view(tops: &[f32], line: f32, at_end: bool) -> usize {
    if at_end && !tops.is_empty() {
        return tops.len() - 1;
    }
    tops.iter().rposition(|top| *top <= line).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_in_view_is_the_last_one_risen_under_the_top_row() {
        let line = 98.0;
        assert_eq!(session_in_view(&[], line, false), 0);
        assert_eq!(session_in_view(&[], line, true), 0);
        assert_eq!(session_in_view(&[150.0, 610.0, 900.0], line, false), 0);
        assert_eq!(session_in_view(&[-310.0, 150.0, 440.0], line, false), 0);
        assert_eq!(session_in_view(&[-362.0, 98.0, 388.0], line, false), 1);
        assert_eq!(session_in_view(&[-900.0, -440.0, 97.5], line, false), 2);
        assert_eq!(session_in_view(&[-310.0, 150.0, 440.0], line, true), 2);
    }
}
