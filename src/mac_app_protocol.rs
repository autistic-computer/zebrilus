use std::{
    ffi::{CStr, CString},
    mem, ptr,
    sync::{
        Once,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use objc2::{
    encode::Encode,
    ffi::{
        class_addMethod, class_addProtocol, method_getImplementation, method_setImplementation,
        objc_getProtocol, object_getClass,
    },
    msg_send,
    runtime::{AnyClass, AnyObject, Bool, Imp, Sel},
    sel,
};

static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);
static ORIGINAL_SEND_EVENT: AtomicUsize = AtomicUsize::new(0);

struct SendEventRestore(bool);

impl Drop for SendEventRestore {
    fn drop(&mut self) {
        HANDLING_SEND_EVENT.store(self.0, Ordering::Relaxed);
    }
}

type SendEvent = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);

const SEND_EVENT_TYPES: &CStr = c"v@:@";

pub(super) fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(install_once);
}

fn install_once() {
    let Some(class) = application_class() else {
        log::error!("could not resolve the NSApplication class for the CEF app protocol");
        return;
    };
    let Ok(is_handling_types) = CString::new(format!("{}@:", Bool::ENCODING)) else {
        return;
    };
    let Ok(set_handling_types) = CString::new(format!("v@:{}", Bool::ENCODING)) else {
        return;
    };

    add_method(
        class,
        sel!(isHandlingSendEvent),
        is_handling_send_event as *const (),
        &is_handling_types,
    );
    add_method(
        class,
        sel!(setHandlingSendEvent:),
        set_handling_send_event as *const (),
        &set_handling_types,
    );
    wrap_send_event(class);
    adopt_protocols(class);
}

fn application_class() -> Option<&'static AnyClass> {
    let class = AnyClass::get(c"NSApplication")?;
    let application: *mut AnyObject = unsafe { msg_send![class, sharedApplication] };
    if application.is_null() {
        return None;
    }
    unsafe { object_getClass(application).as_ref() }
}

fn add_method(class: &AnyClass, selector: Sel, function: *const (), types: &CStr) {
    let added =
        unsafe { class_addMethod(class_ptr(class), selector, erase(function), types.as_ptr()) };
    if !added.as_bool() {
        log::error!("could not add {selector} to {class}");
    }
}

fn wrap_send_event(class: &AnyClass) {
    let selector = sel!(sendEvent:);
    let Some(method) = class.instance_method(selector) else {
        log::error!("could not find sendEvent: on {class}");
        return;
    };
    let Some(original) = (unsafe { method_getImplementation(method) }) else {
        log::error!("could not read the sendEvent: implementation of {class}");
        return;
    };
    ORIGINAL_SEND_EVENT.store(original as usize, Ordering::Relaxed);

    let wrapper = erase(send_event as *const ());
    let added = unsafe {
        class_addMethod(
            class_ptr(class),
            selector,
            wrapper,
            SEND_EVENT_TYPES.as_ptr(),
        )
    };
    if !added.as_bool() {
        unsafe { method_setImplementation(method, wrapper) };
    }
}

fn adopt_protocols(class: &AnyClass) {
    for name in [c"CefAppProtocol", c"CrAppControlProtocol", c"CrAppProtocol"] {
        let protocol = unsafe { objc_getProtocol(name.as_ptr()) };
        if protocol.is_null() {
            continue;
        }
        unsafe { class_addProtocol(class_ptr(class), protocol) };
    }
}

unsafe extern "C-unwind" fn is_handling_send_event(_: *mut AnyObject, _: Sel) -> Bool {
    Bool::new(HANDLING_SEND_EVENT.load(Ordering::Relaxed))
}

unsafe extern "C-unwind" fn set_handling_send_event(_: *mut AnyObject, _: Sel, handling: Bool) {
    HANDLING_SEND_EVENT.store(handling.as_bool(), Ordering::Relaxed);
}

unsafe extern "C-unwind" fn send_event(
    application: *mut AnyObject,
    selector: Sel,
    event: *mut AnyObject,
) {
    let _restore = SendEventRestore(HANDLING_SEND_EVENT.swap(true, Ordering::Relaxed));
    let original = ORIGINAL_SEND_EVENT.load(Ordering::Relaxed);
    if original != 0 {
        let original: SendEvent = unsafe { mem::transmute(original) };
        unsafe { original(application, selector, event) };
    }
}

fn class_ptr(class: &AnyClass) -> *mut AnyClass {
    ptr::from_ref(class).cast_mut()
}

fn erase(function: *const ()) -> Imp {
    unsafe { mem::transmute(function) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_event_state_restores_during_unwind() {
        HANDLING_SEND_EVENT.store(false, Ordering::Relaxed);
        let result = std::panic::catch_unwind(|| {
            let _restore = SendEventRestore(HANDLING_SEND_EVENT.swap(true, Ordering::Relaxed));
            panic!("native dispatch unwound");
        });

        assert!(result.is_err());
        assert!(!HANDLING_SEND_EVENT.load(Ordering::Relaxed));
    }
}
