use std::time::Duration;

#[cfg(unix)]
use std::sync::atomic::Ordering;

use ala::TestAppContext;
#[cfg(unix)]
use zz_protocol::{read_protocol_message, write_protocol_message};
use zz_terminal::{AppearanceProvenance, Color, PackedStyle, UnderlineStyle};

use super::test_support::*;
use super::*;

#[ala::test]
fn terminals_follow_the_theme_mode_even_when_it_is_pinned(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new_inner(
                None,
                zz_daemon_client::default_socket_path(),
                TerminalColorScheme::Dark,
                cx,
            )
        });
        crate::theme::report_terminal_color_scheme(&mux, cx);
        mux
    });
    let os_is_dark = cx.update(|cx| {
        matches!(
            cx.window_appearance(),
            ala::WindowAppearance::Dark | ala::WindowAppearance::VibrantDark
        )
    });
    let (against_the_os, scheme) = if os_is_dark {
        (
            crate::theme::ThemeModeSetting::Light,
            TerminalColorScheme::Light,
        )
    } else {
        (
            crate::theme::ThemeModeSetting::Dark,
            TerminalColorScheme::Dark,
        )
    };
    cx.update(|cx| {
        let mut config = crate::config::AppConfig::default();
        config.theme_mode.value = against_the_os;
        cx.set_global(config);
        crate::theme::refresh_current_theme(cx);
    });
    cx.run_until_parked();
    assert_eq!(mux.read_with(cx, |mux, _| mux.color_scheme), scheme);
    cx.update(|cx| crate::theme::sync_system_appearance(None, cx));
    cx.run_until_parked();
    assert_eq!(mux.read_with(cx, |mux, _| mux.color_scheme), scheme);
}

#[cfg(unix)]
#[ala::test]
fn a_connection_hears_each_color_scheme_once(cx: &mut TestAppContext) {
    use std::os::unix::net::UnixListener;

    cx.executor().allow_parking();
    let socket = std::env::temp_dir().join(format!(
        "zcs-{}-{}.sock",
        std::process::id(),
        TEST_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind test protocol socket: {error}"),
    };
    let server_socket = socket.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test client");
        assert!(matches!(
            read_protocol_message(&mut stream).expect("read ClientHello"),
            ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
        ));
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::ServerHello(Box::new(test_server_hello())),
        )
        .expect("write ServerHello");
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .expect("set read timeout");
        let mut schemes = Vec::new();
        while let Ok(message) = read_protocol_message(&mut stream) {
            if let ProtocolMessage::SetColorScheme(scheme) = message {
                schemes.push(scheme);
            }
        }
        let _ = std::fs::remove_file(server_socket);
        schemes
    });
    let client = InteractiveClient::connect_endpoint(
        &zz_daemon_client::Endpoint::Local(socket),
        TerminalColorScheme::Dark,
    )
    .expect("connect test client");
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new_inner(
                None,
                zz_daemon_client::default_socket_path(),
                TerminalColorScheme::Light,
                cx,
            )
        });
        mux.update(cx, |mux, cx| {
            let connection = HostConnection::connected(Arc::new(client), HostId::LOCAL, cx)
                .expect("install test connection");
            mux.connections.insert(HostId::LOCAL, connection);
            mux.report_color_scheme();
            mux.set_color_scheme(TerminalColorScheme::Light, cx);
            mux.set_color_scheme(TerminalColorScheme::Dark, cx);
            mux.set_color_scheme(TerminalColorScheme::Dark, cx);
        });
    });
    assert_eq!(
        server.join().expect("join test server"),
        [TerminalColorScheme::Light, TerminalColorScheme::Dark]
    );
}

#[ala::test]
fn a_remote_host_renders_with_the_local_appearance(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                "unix:///tmp/zz-background-appearance.sock",
            )],
            cx,
        );
        crate::config::set_daemon_config_overrides_for_test(
            vec![
                ("font-size".to_owned(), "13".to_owned()),
                ("background".to_owned(), "#102030".to_owned()),
            ],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;

        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            let remote_foreground = Color::rgb(0x01, 0x02, 0x03);
            let remote_background = Color::rgb(0x04, 0x05, 0x06);
            let appearance = TerminalAppearance {
                font_size_points: 27.0,
                foreground: remote_foreground,
                background: remote_background,
                ..TerminalAppearance::default()
            };
            mux.handle_message(
                remote,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: EventPayload::AppearanceChanged {
                        appearance: Box::new(appearance),
                        provenance: AppearanceProvenance::default(),
                    },
                }),
                cx,
            );

            assert!(mux.attach_to_host(remote, SessionId(9), cx));
            let local_background = Color::rgb(0x10, 0x20, 0x30);
            assert_eq!(mux.appearance.font_size_points, 13.0);
            assert_eq!(mux.appearance.background, local_background);
            let cell = mux
                .appearance
                .local_colors
                .expect("remote panes are recolored")
                .recolor(PackedStyle::new(
                    remote_foreground,
                    remote_background,
                    None,
                    0,
                    UnderlineStyle::None,
                ));
            assert_eq!(
                (cell.foreground(), cell.background()),
                (
                    TerminalAppearance::default_for(TerminalColorScheme::Dark).foreground,
                    local_background,
                ),
            );

            assert!(mux.attach_to_host(HostId::LOCAL, SessionId(1), cx));
            assert_eq!(mux.appearance.local_colors, None);
        });
    });
}

fn assert_points_eq(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < f32::EPSILON,
        "expected {expected} points, got {actual}"
    );
}

#[test]
fn terminal_font_adjustments_step_and_clamp_in_points() {
    assert_points_eq(
        adjusted_terminal_font_size(13.0, TerminalFontSizeAdjustment::Increase),
        14.0,
    );
    assert_points_eq(
        adjusted_terminal_font_size(13.0, TerminalFontSizeAdjustment::Decrease),
        12.0,
    );
    assert_points_eq(
        adjusted_terminal_font_size(
            MAX_TERMINAL_FONT_SIZE_POINTS,
            TerminalFontSizeAdjustment::Increase,
        ),
        MAX_TERMINAL_FONT_SIZE_POINTS,
    );
    assert_points_eq(
        adjusted_terminal_font_size(
            MIN_TERMINAL_FONT_SIZE_POINTS,
            TerminalFontSizeAdjustment::Decrease,
        ),
        MIN_TERMINAL_FONT_SIZE_POINTS,
    );
}

#[test]
fn terminal_font_offset_survives_reload_and_normalizes_at_bounds() {
    let mut appearance = TerminalAppearance {
        font_size_points: 12.0,
        ..TerminalAppearance::default()
    };
    let offset = apply_terminal_font_size_offset(&mut appearance, 3.0);
    assert_points_eq(appearance.font_size_points, 15.0);
    assert_points_eq(offset, 3.0);

    let mut clamped = TerminalAppearance {
        font_size_points: 255.0,
        ..TerminalAppearance::default()
    };
    let offset = apply_terminal_font_size_offset(&mut clamped, 3.0);
    assert_points_eq(clamped.font_size_points, MAX_TERMINAL_FONT_SIZE_POINTS);
    assert_points_eq(offset, 1.0);
}
