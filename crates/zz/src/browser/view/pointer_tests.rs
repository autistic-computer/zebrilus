use super::*;

#[test]
fn only_a_pointer_cef_is_still_holding_survives_leaving_the_page() {
    assert!(pointer_needs_clamping(PointerPhase::Up, None));
    assert!(pointer_needs_clamping(
        PointerPhase::Move,
        Some(MouseButton::Left)
    ));
    assert!(!pointer_needs_clamping(PointerPhase::Move, None));
    assert!(!pointer_needs_clamping(
        PointerPhase::Down,
        Some(MouseButton::Left)
    ));
    assert!(!pointer_needs_clamping(PointerPhase::Leave, None));
}

#[test]
fn only_a_button_the_page_pressed_carries_its_motion() {
    let mut buttons = 0;
    assert!(!owns_pressed_button(buttons, Some(MouseButton::Left)));
    assert!(owns_pressed_button(buttons, None));

    buttons |= pointer_button_bit(MouseButton::Left);
    assert!(owns_pressed_button(buttons, Some(MouseButton::Left)));
    assert!(!owns_pressed_button(buttons, Some(MouseButton::Right)));
    assert!(owns_pressed_button(buttons, None));

    buttons &= !pointer_button_bit(MouseButton::Left);
    assert!(!owns_pressed_button(buttons, Some(MouseButton::Left)));
}
