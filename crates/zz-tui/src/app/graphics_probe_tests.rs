use super::*;

#[test]
fn kitty_probe_enables_on_a_matching_ok_response() {
    let mut probe = KittyProbe::new(None, true);
    let update = probe.observe(&TerminalEvent::KittyGraphicsResponse {
        image_id: PROBE_IMAGE_ID,
        ok: true,
    });
    assert_eq!(update.graphics, Some(true));
    assert!(update.consumed);
    assert_eq!(probe.state, KittyProbeState::Enabled);
    let fence = probe.observe(&TerminalEvent::DeviceAttributes);
    assert_eq!(fence.graphics, None);
    assert!(fence.finish_file_probe);
}

#[test]
fn an_unprobed_terminal_is_asked_only_once_something_needs_graphics() {
    let mut probe = KittyProbe::new(None, false);
    let reply = probe.observe(&TerminalEvent::DeviceAttributes);
    assert!(reply.consumed);
    assert_eq!(reply.graphics, None);
    assert!(!reply.finish_file_probe);
    assert_eq!(probe.state, KittyProbeState::Idle);

    assert!(probe.start());
    assert!(!probe.start());
    let fence = probe.observe(&TerminalEvent::DeviceAttributes);
    assert_eq!(fence.graphics, Some(false));
    assert!(fence.finish_file_probe);
    assert_eq!(probe.state, KittyProbeState::Disabled);
    assert!(!probe.start());
}

#[test]
fn kitty_probe_disables_when_device_attributes_arrive_first() {
    let mut probe = KittyProbe::new(None, true);
    let fence = probe.observe(&TerminalEvent::DeviceAttributes);
    assert_eq!(fence.graphics, Some(false));
    assert!(fence.finish_file_probe);
    assert_eq!(probe.state, KittyProbeState::Disabled);
    let late = probe.observe(&TerminalEvent::KittyGraphicsResponse {
        image_id: PROBE_IMAGE_ID,
        ok: true,
    });
    assert_eq!(late.graphics, None);
    assert!(late.consumed);
}

#[test]
fn file_probe_selects_file_only_on_ok_and_honors_the_override() {
    let mut supported = KittyProbe::new(None, true);
    let update = supported.observe(&TerminalEvent::KittyGraphicsResponse {
        image_id: FILE_PROBE_IMAGE_ID,
        ok: true,
    });
    assert_eq!(update.transport, Some(FrameTransport::File));
    assert!(update.finish_file_probe);
    assert_eq!(supported.transport(), FrameTransport::File);

    let mut rejected = KittyProbe::new(None, true);
    let update = rejected.observe(&TerminalEvent::KittyGraphicsResponse {
        image_id: FILE_PROBE_IMAGE_ID,
        ok: false,
    });
    assert_eq!(update.transport, None);
    assert_eq!(rejected.transport(), FrameTransport::Inline);

    assert_eq!(
        resolve_frame_transport(true, Some(FrameTransport::Inline)),
        FrameTransport::Inline
    );
    assert_eq!(
        resolve_frame_transport(false, Some(FrameTransport::File)),
        FrameTransport::File
    );
    assert_eq!(
        parse_frame_transport_override(" file "),
        Some(FrameTransport::File)
    );
    assert_eq!(
        parse_frame_transport_override("INLINE"),
        Some(FrameTransport::Inline)
    );
    assert_eq!(parse_frame_transport_override("auto"), None);
}
