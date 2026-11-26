use super::*;

pub(super) fn format_time_value(value: &str, flags: &TimeFlags<'_>, now: Option<i64>) -> String {
    let Ok(timestamp) = value.parse::<i64>() else {
        return String::new();
    };
    if timestamp <= 0 {
        return String::new();
    }
    if flags.relative {
        return now.map_or_else(String::new, |now| relative_time(timestamp, now));
    }
    if flags.difference {
        return now.map_or_else(String::new, |now| now.saturating_sub(timestamp).to_string());
    }
    if flags.pretty {
        return now.map_or_else(String::new, |now| pretty_time(timestamp, now));
    }
    let Some(time) = crate::localtime::local_time(timestamp) else {
        return String::new();
    };
    format_datetime(&time, flags.format.unwrap_or("%a %b %e %H:%M:%S %Y"))
}

pub(crate) fn pretty_time(timestamp: i64, now: i64) -> String {
    let effective_now = now.max(timestamp);
    let age = effective_now.saturating_sub(timestamp);
    let Some(time) = crate::localtime::local_time(timestamp) else {
        return String::new();
    };
    let Some(now) = crate::localtime::local_time(effective_now) else {
        return String::new();
    };
    let format = if age < 24 * 3600 {
        "%H:%M"
    } else if (time.year() == now.year() && time.month() == now.month()) || age < 28 * 24 * 3600 {
        "%a%d"
    } else if (time.year() == now.year() && time.month() < now.month())
        || (time.year() == now.year() - 1 && time.month() > now.month())
    {
        "%d%b"
    } else {
        "%b%y"
    };
    format_datetime(&time, format)
}

fn relative_time(timestamp: i64, now: i64) -> String {
    if timestamp > now {
        return String::new();
    }
    let age = now - timestamp;
    if age == 0 {
        return "0s".to_owned();
    }
    let days = age / 86_400;
    let hours = age % 86_400 / 3_600;
    let minutes = age % 3_600 / 60;
    let seconds = age % 60;
    if days != 0 {
        if hours != 0 {
            format!("{days}d{hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours != 0 {
        if minutes != 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes != 0 {
        if seconds != 0 {
            format!("{minutes}m{seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

pub(super) fn format_datetime(time: &DateTime<FixedOffset>, format: &str) -> String {
    let Ok(items) = chrono::format::StrftimeItems::new(format).parse() else {
        return String::new();
    };
    let mut output = String::with_capacity(format.len());
    if write!(&mut output, "{}", time.format_with_items(items.iter())).is_err() {
        String::new()
    } else {
        output
    }
}
