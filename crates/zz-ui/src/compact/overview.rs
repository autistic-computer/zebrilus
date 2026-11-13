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
pub fn session_in_view(tops: &[f32], line: f32) -> usize {
    tops.iter().rposition(|top| *top <= line).unwrap_or(0)
}

#[must_use]
pub fn session_tail(page: f32, line: f32, bottom: f32, sessions: usize) -> f32 {
    if sessions > 1 {
        (page - line - bottom).max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_in_view_is_the_last_one_risen_under_the_top_row() {
        let line = 98.0;
        assert_eq!(session_in_view(&[], line), 0);
        assert_eq!(session_in_view(&[150.0, 610.0, 900.0], line), 0);
        assert_eq!(session_in_view(&[-310.0, 150.0, 440.0], line), 0);
        assert_eq!(session_in_view(&[-362.0, 98.0, 388.0], line), 1);
        assert_eq!(session_in_view(&[-900.0, -440.0, 97.5], line), 2);
        assert_eq!(session_tail(874.0, line, 108.0, 3), 668.0);
        assert_eq!(session_tail(874.0, line, 108.0, 1), 0.0);
    }
}
