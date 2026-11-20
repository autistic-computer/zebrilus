#![allow(
    unsafe_code,
    reason = "the simulator's input service is reached through libxpc's C API"
)]

use std::{
    ffi::{CStr, c_char, c_void},
    mem::transmute,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use block2::RcBlock;
use objc2::{
    ffi::objc_msgSend,
    runtime::{AnyObject, NSObject, Sel},
    sel,
};
use objc2_foundation::{NSError, NSString};

use crate::macos::{Button, Error, KeyDirection, TouchPhase, Turn};

const SERVICE: &CStr = c"com.apple.coredevice.feature.remote.hid.digitizer";
const ORIENTATION: &CStr = c"com.apple.coredevice.feature.remote.devicecontrol.orientation";
const TOUCH: &CStr = c"IndigoDigitizerEvent";
const KEY: &CStr = c"IndigoKeyboardButtonEvent";
const BUTTON: &CStr = c"IndigoButtonEvent";
const CONSUMER_PAGE: u64 = 0x0C;
const CONSUMER_MENU: u64 = 0x40;
const CONSUMER_POWER: u64 = 0x30;
const REPLY_TIMEOUT: Duration = Duration::from_secs(4);
const TOUCH_START: u64 = 0;
const TOUCH_POSITION: u64 = 1;
const TOUCH_END: u64 = 2;
const KEY_DOWN: u64 = 1;
const KEY_UP: u64 = 2;

type Xpc = *mut c_void;
type Send = unsafe extern "C-unwind" fn();
type Lookup =
    unsafe extern "C-unwind" fn(*const AnyObject, Sel, *const NSString, *mut *mut NSError) -> u32;
type EndpointFromPort = unsafe extern "C" fn(u32, u64, u64) -> Xpc;
type EnableSimulatorToHost = unsafe extern "C" fn(Xpc);

unsafe extern "C" {
    static _xpc_type_error: c_void;
    fn xpc_get_type(object: Xpc) -> *const c_void;
    fn xpc_release(object: Xpc);
    fn xpc_dictionary_create(keys: *const *const c_char, values: *const Xpc, count: usize) -> Xpc;
    fn xpc_dictionary_set_string(dictionary: Xpc, key: *const c_char, value: *const c_char);
    fn xpc_dictionary_set_bool(dictionary: Xpc, key: *const c_char, value: bool);
    fn xpc_dictionary_set_uint64(dictionary: Xpc, key: *const c_char, value: u64);
    fn xpc_dictionary_set_double(dictionary: Xpc, key: *const c_char, value: f64);
    fn xpc_dictionary_set_value(dictionary: Xpc, key: *const c_char, value: Xpc);
    fn xpc_connection_create_from_endpoint(endpoint: Xpc) -> Xpc;
    fn xpc_connection_set_event_handler(connection: Xpc, handler: *const c_void);
    fn xpc_connection_set_target_queue(connection: Xpc, queue: *mut c_void);
    fn xpc_connection_resume(connection: Xpc);
    fn xpc_connection_cancel(connection: Xpc);
    fn xpc_connection_send_message(connection: Xpc, message: Xpc);
    fn xpc_connection_send_message_with_reply(
        connection: Xpc,
        message: Xpc,
        queue: *mut c_void,
        handler: *const c_void,
    );
    fn dispatch_queue_create(label: *const c_char, attributes: *const c_void) -> *mut c_void;
    fn dispatch_release(object: *mut c_void);
}

struct Link {
    connection: Xpc,
    queue: *mut c_void,
    closed: Arc<AtomicBool>,
    _events: RcBlock<dyn Fn(Xpc)>,
}

impl Link {
    fn open(device: &NSObject, service: &CStr) -> Result<Option<Self>, Error> {
        let Some(endpoint_from_port) = private(c"xpc_endpoint_create_mach_port_4sim") else {
            return Ok(None);
        };
        let Some(enable) = private(c"xpc_connection_enable_sim2host_4sim") else {
            return Ok(None);
        };
        let endpoint_from_port =
            unsafe { transmute::<*mut c_void, EndpointFromPort>(endpoint_from_port) };
        let enable = unsafe { transmute::<*mut c_void, EnableSimulatorToHost>(enable) };

        let lookup = unsafe { transmute::<Send, Lookup>(objc_msgSend) };
        let name = NSString::from_str(&service.to_string_lossy());
        let mut error: *mut NSError = null_mut();
        let port = unsafe {
            lookup(
                std::ptr::from_ref(device).cast(),
                sel!(lookup:error:),
                &raw const *name,
                &raw mut error,
            )
        };
        if port == 0 {
            return Ok(None);
        }
        let endpoint = unsafe { endpoint_from_port(port, 0, 0) };
        if endpoint.is_null() {
            return Err(Error::Remote("could not be reached"));
        }
        let connection = unsafe { xpc_connection_create_from_endpoint(endpoint) };
        unsafe { xpc_release(endpoint) };
        if connection.is_null() {
            return Err(Error::Remote("could not be reached"));
        }
        let queue = unsafe { dispatch_queue_create(c"zz.simulator.input".as_ptr(), null()) };
        let closed = Arc::new(AtomicBool::new(false));
        let events = {
            let closed = Arc::clone(&closed);
            RcBlock::new(move |event: Xpc| {
                if is_error(event) {
                    closed.store(true, Ordering::Relaxed);
                }
            })
        };
        unsafe {
            enable(connection);
            xpc_connection_set_target_queue(connection, queue);
            xpc_connection_set_event_handler(connection, RcBlock::as_ptr(&events).cast());
            xpc_connection_resume(connection);
        }
        Ok(Some(Self {
            connection,
            queue,
            closed,
            _events: events,
        }))
    }

    fn send(&self, message: Xpc) -> Result<(), Error> {
        let closed = self.closed.load(Ordering::Relaxed);
        unsafe {
            if !closed {
                xpc_connection_send_message(self.connection, message);
            }
            xpc_release(message);
        }
        if closed {
            return Err(Error::Remote("closed the connection"));
        }
        Ok(())
    }

    fn request(&self, message: Xpc) -> Result<(), Error> {
        let (sender, receiver) = mpsc::channel();
        let reply = RcBlock::new(move |reply: Xpc| {
            let _ = sender.send(!is_error(reply));
        });
        unsafe {
            xpc_connection_send_message_with_reply(
                self.connection,
                message,
                self.queue,
                RcBlock::as_ptr(&reply).cast(),
            );
            xpc_release(message);
        }
        match receiver.recv_timeout(REPLY_TIMEOUT) {
            Ok(true) => Ok(()),
            Ok(false) => Err(Error::Remote("closed the connection")),
            Err(_) => Err(Error::Remote("did not answer")),
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        unsafe {
            xpc_connection_cancel(self.connection);
            xpc_release(self.connection);
            dispatch_release(self.queue);
        }
    }
}

pub(crate) struct Remote {
    link: Link,
}

impl Remote {
    pub(crate) fn connect(device: &NSObject, verify: bool) -> Result<Option<Self>, Error> {
        let Some(link) = Link::open(device, SERVICE)? else {
            return Ok(None);
        };
        let remote = Self { link };
        if verify {
            remote.flush()?;
        }
        Ok(Some(remote))
    }

    pub(crate) fn touch(&self, phase: TouchPhase, x: f64, y: f64) -> Result<(), Error> {
        let payload = dictionary();
        let point = dictionary();
        unsafe {
            xpc_dictionary_set_double(point, c"x".as_ptr(), x);
            xpc_dictionary_set_double(point, c"y".as_ptr(), y);
            xpc_dictionary_set_value(payload, c"pointOne".as_ptr(), point);
            xpc_release(point);
            xpc_dictionary_set_uint64(
                payload,
                c"eventType".as_ptr(),
                match phase {
                    TouchPhase::Down => TOUCH_START,
                    TouchPhase::Move => TOUCH_POSITION,
                    TouchPhase::Up => TOUCH_END,
                },
            );
            xpc_dictionary_set_uint64(payload, c"edge".as_ptr(), 0);
            xpc_dictionary_set_uint64(payload, c"target".as_ptr(), 0);
        }
        self.link.send(message(TOUCH, false, payload))
    }

    pub(crate) fn key(&self, usage: u32, direction: KeyDirection) -> Result<(), Error> {
        self.link
            .send(message(KEY, false, key_payload(usage, direction)))
    }

    pub(crate) fn button(&self, button: Button, direction: KeyDirection) -> Result<(), Error> {
        let payload = dictionary();
        unsafe {
            xpc_dictionary_set_uint64(payload, c"usagePage".as_ptr(), CONSUMER_PAGE);
            xpc_dictionary_set_uint64(
                payload,
                c"usageCode".as_ptr(),
                match button {
                    Button::Home => CONSUMER_MENU,
                    Button::Lock => CONSUMER_POWER,
                },
            );
            xpc_dictionary_set_uint64(payload, c"state".as_ptr(), state(direction));
        }
        self.link.send(message(BUTTON, false, payload))
    }

    pub(crate) fn flush(&self) -> Result<(), Error> {
        self.link
            .request(message(KEY, true, key_payload(0, KeyDirection::Up)))
    }
}

pub(crate) struct Orientation {
    link: Link,
}

impl Orientation {
    pub(crate) fn connect(device: &NSObject) -> Result<Option<Self>, Error> {
        Ok(Link::open(device, ORIENTATION)?.map(|link| Self { link }))
    }

    pub(crate) fn rotate(&self, turn: Turn) -> Result<(), Error> {
        let direction = dictionary();
        let payload = dictionary();
        let message = dictionary();
        unsafe {
            xpc_dictionary_set_string(
                direction,
                c"_0".as_ptr(),
                match turn {
                    Turn::Left => c"left".as_ptr(),
                    Turn::Right => c"right".as_ptr(),
                },
            );
            xpc_dictionary_set_value(payload, c"rotate".as_ptr(), direction);
            xpc_release(direction);
            xpc_dictionary_set_string(message, c"featureIdentifier".as_ptr(), ORIENTATION.as_ptr());
            xpc_dictionary_set_value(message, c"payload".as_ptr(), payload);
            xpc_release(payload);
        }
        self.link.request(message)
    }
}

fn dictionary() -> Xpc {
    unsafe { xpc_dictionary_create(null(), null(), 0) }
}

fn key_payload(usage: u32, direction: KeyDirection) -> Xpc {
    let payload = dictionary();
    unsafe {
        xpc_dictionary_set_uint64(payload, c"usageCode".as_ptr(), u64::from(usage));
        xpc_dictionary_set_uint64(payload, c"state".as_ptr(), state(direction));
    }
    payload
}

fn state(direction: KeyDirection) -> u64 {
    match direction {
        KeyDirection::Down => KEY_DOWN,
        KeyDirection::Up => KEY_UP,
    }
}

fn message(kind: &CStr, barrier: bool, payload: Xpc) -> Xpc {
    let message = dictionary();
    unsafe {
        xpc_dictionary_set_string(message, c"messageType".as_ptr(), kind.as_ptr());
        xpc_dictionary_set_bool(message, c"isBarrier".as_ptr(), barrier);
        xpc_dictionary_set_string(message, c"featureIdentifier".as_ptr(), SERVICE.as_ptr());
        xpc_dictionary_set_value(message, c"payload".as_ptr(), payload);
        xpc_release(payload);
    }
    message
}

fn is_error(object: Xpc) -> bool {
    unsafe { xpc_get_type(object) == &raw const _xpc_type_error }
}

fn private(name: &CStr) -> Option<*mut c_void> {
    let address = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    (!address.is_null()).then_some(address)
}
