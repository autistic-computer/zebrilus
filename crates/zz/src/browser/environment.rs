use std::{env, ffi::OsStr};

use zz_browser::RuntimeConfig;

const LOCAL_SCHEME: &str = "zz";
const PAGE_GLOBAL: &str = "zz";
const MAX_FRAME_RATE: i32 = 240;

#[must_use]
pub fn runtime_config() -> RuntimeConfig {
    let gpu = on_unless_zero(env::var_os("ZZ_BROWSER_GPU").as_deref());
    let shared_texture_setting = env::var_os("ZZ_BROWSER_SHARED_TEXTURE");
    let shared_texture = on_unless_zero(shared_texture_setting.as_deref());
    if shared_texture_setting.is_some() && shared_texture && !gpu {
        log::warn!(
            "ZZ_BROWSER_SHARED_TEXTURE is enabled but ZZ_BROWSER_GPU=0; shared textures are disabled and CEF will use software readback OSR"
        );
    }
    RuntimeConfig {
        scheme: LOCAL_SCHEME,
        page_global: PAGE_GLOBAL,
        frame_rate: frame_rate(env::var_os("ZZ_BROWSER_FPS").as_deref()),
        gpu,
        shared_texture,
        external_begin_frame: external_begin_frame(
            env::var_os("ZZ_BROWSER_EXTERNAL_BEGIN_FRAME").as_deref(),
            cfg!(target_os = "macos"),
        ),
        begin_frame_adaptive: exactly_one(env::var_os("ZZ_BROWSER_BF_ADAPTIVE").as_deref()),
        remote_debugging_port: remote_debugging_port(
            env::var_os("ZZ_BROWSER_REMOTE_DEBUGGING_PORT").as_deref(),
        ),
    }
}

fn on_unless_zero(value: Option<&OsStr>) -> bool {
    value.is_none_or(|value| value != "0")
}

fn exactly_one(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

fn external_begin_frame(value: Option<&OsStr>, macos: bool) -> bool {
    if macos {
        on_unless_zero(value)
    } else {
        exactly_one(value)
    }
}

fn frame_rate(value: Option<&OsStr>) -> Option<i32> {
    let value = value?;
    let frame_rate = value
        .to_str()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|frame_rate| (1..=MAX_FRAME_RATE).contains(frame_rate));
    if frame_rate.is_none() {
        log::warn!(
            "ZZ_BROWSER_FPS must be an integer from 1 through {MAX_FRAME_RATE}; ignoring it"
        );
    }
    frame_rate
}

fn remote_debugging_port(value: Option<&OsStr>) -> Option<u16> {
    let value = value?;
    let port = value
        .to_str()
        .and_then(|value| value.trim().parse::<u16>().ok());
    if port.is_none() {
        log::warn!(
            "ZZ_BROWSER_REMOTE_DEBUGGING_PORT must be 0 or a port from 1024 through 65535; ignoring it"
        );
    }
    port
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rate_accepts_one_through_the_ceiling() {
        let parse = |value: &str| frame_rate(Some(OsStr::new(value)));
        assert_eq!(frame_rate(None), None);
        assert_eq!(parse("1"), Some(1));
        assert_eq!(parse(" 120 "), Some(120));
        assert_eq!(parse("240"), Some(240));
        assert_eq!(parse("0"), None);
        assert_eq!(parse("241"), None);
        assert_eq!(parse("fast"), None);
    }

    #[test]
    fn gpu_features_are_on_unless_zero() {
        assert!(on_unless_zero(None));
        assert!(on_unless_zero(Some(OsStr::new("1"))));
        assert!(!on_unless_zero(Some(OsStr::new("0"))));
    }

    #[test]
    fn external_begin_frames_macos_default_on_with_zero_kill_switch() {
        assert!(external_begin_frame(None, true));
        assert!(external_begin_frame(Some(OsStr::new("1")), true));
        assert!(external_begin_frame(Some(OsStr::new("enabled")), true));
        assert!(!external_begin_frame(Some(OsStr::new("0")), true));
    }

    #[test]
    fn external_begin_frames_elsewhere_require_exact_opt_in() {
        assert!(!external_begin_frame(None, false));
        assert!(external_begin_frame(Some(OsStr::new("1")), false));
        for value in ["0", "true", " 1 ", "01"] {
            assert!(!external_begin_frame(Some(OsStr::new(value)), false));
        }
    }

    #[test]
    fn adaptive_begin_frame_throttle_requires_exact_opt_in() {
        assert!(!exactly_one(None));
        assert!(exactly_one(Some(OsStr::new("1"))));
        assert!(!exactly_one(Some(OsStr::new("enabled"))));
        assert!(!exactly_one(Some(OsStr::new("0"))));
    }

    #[test]
    fn remote_debugging_port_parses_or_is_ignored() {
        let parse = |value: &str| remote_debugging_port(Some(OsStr::new(value)));
        assert_eq!(remote_debugging_port(None), None);
        assert_eq!(parse("0"), Some(0));
        assert_eq!(parse(" 9333 "), Some(9333));
        assert_eq!(parse("1023"), Some(1023));
        for value in ["", "abc", "-1", "65536", "9222.0"] {
            assert_eq!(parse(value), None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_values_are_ignored() {
        use std::os::unix::ffi::OsStrExt;
        let value = OsStr::from_bytes(&[0xff]);
        assert_eq!(remote_debugging_port(Some(value)), None);
        assert_eq!(frame_rate(Some(value)), None);
    }
}
