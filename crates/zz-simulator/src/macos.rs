#![allow(
    unsafe_code,
    reason = "CoreSimulator and SimulatorKit are reached through the Objective-C runtime and C symbols"
)]

use std::{
    ffi::{CStr, CString, c_char, c_void},
    mem::transmute,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use block2::{ManualBlockEncoding, RcBlock};
use core_foundation::base::{TCFType, kCFAllocatorDefault};
use core_video::{
    pixel_buffer::{CVPixelBuffer, CVPixelBufferRef},
    pixel_buffer_io_surface::CVPixelBufferCreateWithIOSurface,
    r#return::kCVReturnSuccess,
};
use objc2::{
    ffi::objc_msgSend,
    msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool, NSObject, Sel},
    sel,
};
use objc2_core_foundation::{CGPoint, CGSize};
use objc2_foundation::{NSArray, NSError, NSString, NSUUID};
use thiserror::Error;

use crate::remote::{Orientation, Remote};

const CORE_SIMULATOR: &str =
    "/Library/Developer/PrivateFrameworks/CoreSimulator.framework/CoreSimulator";
const SIMULATOR_KIT: [&str; 2] = [
    "../SharedFrameworks/SimulatorKit.framework/SimulatorKit",
    "Library/PrivateFrameworks/SimulatorKit.framework/SimulatorKit",
];
const XCODE_SELECT_LINK: &str = "/var/db/xcode_select_link";
const DISPLAY_PORT: &str = "com.apple.framebuffer.display";
const TOUCH_TARGET: u32 = 0x32;
const MOUSE_DOWN: i32 = 1;
const MOUSE_UP: i32 = 2;
const KEY_DOWN: u32 = 1;
const KEY_UP: u32 = 2;
const BOOTED: &str = "Booted";
const LOCK_READ_ONLY: u32 = 1;
const BUTTON_TARGET: i32 = 0x33;
const BUTTON_HOME: i32 = 0x0;
const BUTTON_LOCK: i32 = 0x1;

#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    fn IOSurfaceGetWidth(surface: *const c_void) -> usize;
    fn IOSurfaceGetHeight(surface: *const c_void) -> usize;
    fn IOSurfaceGetPixelFormat(surface: *const c_void) -> u32;
    fn IOSurfaceGetSeed(surface: *const c_void) -> u32;
    fn IOSurfaceGetBytesPerRow(surface: *const c_void) -> usize;
    fn IOSurfaceGetBaseAddress(surface: *const c_void) -> *const u8;
    fn IOSurfaceLock(surface: *const c_void, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceUnlock(surface: *const c_void, options: u32, seed: *mut u32) -> i32;
}

unsafe extern "C" {
    fn dispatch_queue_create(label: *const c_char, attributes: *const c_void) -> *mut c_void;
    fn dispatch_release(object: *mut c_void);
}

type Send = unsafe extern "C-unwind" fn();
type MouseMessage =
    unsafe extern "C" fn(*const CGPoint, *const CGPoint, u32, i32, f64, f64, u32) -> *mut c_void;
type KeyMessage = unsafe extern "C" fn(u32, u32) -> *mut c_void;
type ButtonMessage = unsafe extern "C" fn(i32, i32, i32) -> *mut c_void;
type SendMessage = unsafe extern "C-unwind" fn(
    *const AnyObject,
    Sel,
    *mut c_void,
    Bool,
    *const AnyObject,
    *const AnyObject,
);
type RegisterCallbacks = unsafe extern "C-unwind" fn(
    *const AnyObject,
    Sel,
    *const NSUUID,
    *mut c_void,
    *const c_void,
    *const c_void,
    *const c_void,
);
type UnregisterCallbacks = unsafe extern "C-unwind" fn(*const AnyObject, Sel, *const NSUUID);
type Perform = unsafe extern "C-unwind" fn(*const AnyObject, Sel);
type GetObject = unsafe extern "C-unwind" fn(*const AnyObject, Sel) -> *mut AnyObject;
type GetUnsigned = unsafe extern "C-unwind" fn(*const AnyObject, Sel) -> u32;
type Responds = unsafe extern "C-unwind" fn(*const AnyObject, Sel, Sel) -> Bool;

struct Notify;

unsafe impl ManualBlockEncoding for Notify {
    type Arguments = ();
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v8@?0";
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("no Xcode developer directory is selected")]
    DeveloperDir,
    #[error("class {0} is missing from the simulator frameworks")]
    MissingClass(&'static str),
    #[error("symbol {0} is missing from the simulator frameworks")]
    MissingSymbol(&'static str),
    #[error("simulator service failed: {0}")]
    Service(String),
    #[error("no simulator has the identifier {0}")]
    DeviceNotFound(String),
    #[error("simulator {udid} is {state}, not Booted")]
    NotBooted { udid: String, state: String },
    #[error("simulator has no framebuffer display with a surface")]
    NoDisplay,
    #[error("simulator refused the {0} message")]
    Message(&'static str),
    #[error("the simulator's screen could not be wrapped for drawing (status {0})")]
    PixelBuffer(i32),
    #[error("simulator input service {0}")]
    Remote(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    pub width: usize,
    pub height: usize,
    pub pixel_format: u32,
    pub seed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Home,
    Lock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub udid: String,
    pub name: String,
    pub system: String,
    pub booted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDirection {
    Down,
    Up,
}

#[derive(Debug)]
pub struct Simulator {
    device: Retained<NSObject>,
}

impl Simulator {
    pub fn attach(udid: &str) -> Result<Self, Error> {
        let device = find(udid)?;
        let state = state(&device);
        if state != BOOTED {
            return Err(Error::NotBooted {
                udid: udid.to_owned(),
                state,
            });
        }
        Ok(Self { device })
    }

    pub fn name(&self) -> String {
        name(&self.device)
    }

    pub fn rotate(&self, turn: Turn) -> Result<(), Error> {
        let orientation = Orientation::connect(&self.device)?
            .ok_or(Error::Remote("has no orientation service"))?;
        orientation.rotate(turn)
    }

    pub fn set_hardware_keyboard(&self, enabled: bool) -> Result<(), Error> {
        let selector = sel!(setHardwareKeyboardEnabled:keyboardType:error:);
        if !responds(&self.device, selector) {
            return Err(Error::MissingSymbol(
                "setHardwareKeyboardEnabled:keyboardType:error:",
            ));
        }
        let set: Result<(), Retained<NSError>> = unsafe {
            msg_send![&*self.device, setHardwareKeyboardEnabled: enabled, keyboardType: 0_u8, error: _]
        };
        set.map_err(|error| service_error(&error))
    }

    pub fn screen_size(&self) -> Option<(usize, usize)> {
        Self::native_size(&self.device)
    }

    pub fn screen(&self) -> Result<Screen, Error> {
        Screen::open(&self.device, Arc::new(|| {}))
    }

    pub fn screen_with(
        &self,
        on_frame: impl Fn() + std::marker::Send + Sync + 'static,
    ) -> Result<Screen, Error> {
        Screen::open(&self.device, Arc::new(on_frame))
    }

    pub fn input(&self) -> Result<Input, Error> {
        Input::open(&self.device, None, true)
    }

    pub fn input_unverified(&self) -> Result<Input, Error> {
        Input::open(&self.device, None, false)
    }

    pub fn input_on(&self, route: Route) -> Result<Input, Error> {
        Input::open(&self.device, Some(route), true)
    }

    fn native_size(device: &NSObject) -> Option<(usize, usize)> {
        let kind: Option<Retained<NSObject>> = unsafe { msg_send![device, deviceType] };
        let kind = kind?;
        if !responds(&kind, sel!(mainScreenSize)) {
            return None;
        }
        let size: CGSize = unsafe { msg_send![&*kind, mainScreenSize] };
        (size.width >= 1.0 && size.height >= 1.0)
            .then(|| (size.width.round() as usize, size.height.round() as usize))
    }
}

struct Registration {
    descriptor: Retained<AnyObject>,
    identifier: Retained<NSUUID>,
}

impl Registration {
    fn surface(&self) -> Option<Retained<AnyObject>> {
        object(&self.descriptor, sel!(framebufferSurface))
    }

    fn cancel(&self) {
        let unregister = unsafe { transmute::<Send, UnregisterCallbacks>(objc_msgSend) };
        unsafe {
            unregister(
                Retained::as_ptr(&self.descriptor),
                sel!(unregisterScreenCallbacksWithUUID:),
                Retained::as_ptr(&self.identifier),
            );
        }
    }
}

pub struct Screen {
    registration: Registration,
    surface: Retained<AnyObject>,
    queue: *mut c_void,
    frames: Arc<AtomicU64>,
    surface_changes: Arc<AtomicU64>,
    displays: Vec<(usize, usize)>,
}

impl Screen {
    fn open(
        device: &NSObject,
        notify: Arc<dyn Fn() + std::marker::Send + Sync>,
    ) -> Result<Self, Error> {
        let io: Option<Retained<NSObject>> = unsafe { msg_send![device, io] };
        let io = io.ok_or(Error::NoDisplay)?;
        let perform = unsafe { transmute::<Send, Perform>(objc_msgSend) };
        unsafe { perform(Retained::as_ptr(&io).cast(), sel!(updateIOPorts)) };
        let ports: Retained<NSArray<AnyObject>> = unsafe { msg_send![&*io, ioPorts] };

        let queue = unsafe { dispatch_queue_create(c"zz.simulator.screen".as_ptr(), null()) };
        let frames = Arc::new(AtomicU64::new(0));
        let surface_changes = Arc::new(AtomicU64::new(0));
        let on_frame = {
            let frames = Arc::clone(&frames);
            let notify = Arc::clone(&notify);
            RcBlock::with_encoding::<_, _, _, Notify>(move || {
                frames.fetch_add(1, Ordering::Relaxed);
                notify();
            })
        };
        let on_surfaces = {
            let surface_changes = Arc::clone(&surface_changes);
            let notify = Arc::clone(&notify);
            RcBlock::with_encoding::<_, _, _, Notify>(move || {
                surface_changes.fetch_add(1, Ordering::Relaxed);
                notify();
            })
        };
        let on_properties = RcBlock::with_encoding::<_, _, _, Notify>(move || notify());
        let register = unsafe { transmute::<Send, RegisterCallbacks>(objc_msgSend) };
        let register_selector = sel!(registerScreenCallbacksWithUUID:callbackQueue:frameCallback:surfacesChangedCallback:propertiesChangedCallback:);

        let mut registrations = Vec::new();
        for port in &ports {
            let Some(identifier) = object(&port, sel!(portIdentifier)) else {
                continue;
            };
            let Some(name) = object(&identifier, sel!(description)) else {
                continue;
            };
            let name: &NSString = unsafe { &*Retained::as_ptr(&name).cast() };
            if name.to_string() != DISPLAY_PORT {
                continue;
            }
            let Some(descriptor) = object(&port, sel!(descriptor)) else {
                continue;
            };
            if !responds(&descriptor, register_selector)
                || !responds(&descriptor, sel!(framebufferSurface))
            {
                continue;
            }
            let identifier = NSUUID::new();
            unsafe {
                register(
                    Retained::as_ptr(&descriptor),
                    register_selector,
                    Retained::as_ptr(&identifier),
                    queue,
                    RcBlock::as_ptr(&on_frame).cast(),
                    RcBlock::as_ptr(&on_surfaces).cast(),
                    RcBlock::as_ptr(&on_properties).cast(),
                );
            }
            registrations.push(Registration {
                descriptor,
                identifier,
            });
        }

        let native = Simulator::native_size(device);
        let mut displays = Vec::new();
        let mut chosen: Option<(usize, Retained<AnyObject>)> = None;
        for (index, registration) in registrations.iter().enumerate() {
            let Some(surface) = registration.surface() else {
                continue;
            };
            let size = surface_size(&surface);
            displays.push(size);
            let matches_native = native == Some(size);
            if chosen.is_none()
                || (matches_native && native != chosen.as_ref().map(|(_, s)| surface_size(s)))
            {
                chosen = Some((index, surface));
            }
        }
        let Some((index, surface)) = chosen else {
            for registration in &registrations {
                registration.cancel();
            }
            unsafe { dispatch_release(queue) };
            return Err(Error::NoDisplay);
        };
        let registration = registrations.swap_remove(index);
        for other in &registrations {
            other.cancel();
        }
        Ok(Self {
            registration,
            surface,
            queue,
            frames,
            surface_changes,
            displays,
        })
    }

    pub fn surface(&self) -> Surface {
        let surface = Retained::as_ptr(&self.surface).cast::<c_void>();
        let (width, height) = surface_size(&self.surface);
        Surface {
            width,
            height,
            pixel_format: unsafe { IOSurfaceGetPixelFormat(surface) },
            seed: unsafe { IOSurfaceGetSeed(surface) },
        }
    }

    pub fn turns(&self) -> u8 {
        let Some(properties) = object(&self.registration.descriptor, sel!(screenProperties)) else {
            return 0;
        };
        if !responds(&properties, sel!(uiOrientation)) {
            return 0;
        }
        let get = unsafe { transmute::<Send, GetUnsigned>(objc_msgSend) };
        match unsafe { get(Retained::as_ptr(&properties), sel!(uiOrientation)) } {
            2 => 2,
            3 => 1,
            4 => 3,
            _ => 0,
        }
    }

    pub fn refresh(&mut self) -> Result<(), Error> {
        self.surface = self.registration.surface().ok_or(Error::NoDisplay)?;
        Ok(())
    }

    pub fn read_rgba(&self) -> Result<(u32, u32, Vec<u8>), Error> {
        let surface = Retained::as_ptr(&self.surface).cast::<c_void>();
        let (width, height) = surface_size(&self.surface);
        let status = unsafe { IOSurfaceLock(surface, LOCK_READ_ONLY, null_mut()) };
        if status != 0 {
            return Err(Error::PixelBuffer(status));
        }
        let stride = unsafe { IOSurfaceGetBytesPerRow(surface) };
        let base = unsafe { IOSurfaceGetBaseAddress(surface) };
        let mut pixels = Vec::with_capacity(width * height * 4);
        if !base.is_null() && stride >= width * 4 {
            for row in 0..height {
                let line = unsafe { std::slice::from_raw_parts(base.add(row * stride), width * 4) };
                for pixel in line.as_chunks::<4>().0 {
                    pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
                }
            }
        }
        unsafe { IOSurfaceUnlock(surface, LOCK_READ_ONLY, null_mut()) };
        if pixels.is_empty() {
            return Err(Error::NoDisplay);
        }
        Ok((width as u32, height as u32, pixels))
    }

    pub fn pixel_buffer(&self) -> Result<CVPixelBuffer, Error> {
        let mut buffer: CVPixelBufferRef = null_mut();
        let status = unsafe {
            CVPixelBufferCreateWithIOSurface(
                kCFAllocatorDefault,
                &*Retained::as_ptr(&self.surface).cast(),
                null(),
                &raw mut buffer,
            )
        };
        if status != kCVReturnSuccess || buffer.is_null() {
            return Err(Error::PixelBuffer(status));
        }
        Ok(unsafe { CVPixelBuffer::wrap_under_create_rule(buffer) })
    }

    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    pub fn surface_changes(&self) -> u64 {
        self.surface_changes.load(Ordering::Relaxed)
    }

    pub fn displays(&self) -> &[(usize, usize)] {
        &self.displays
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        self.registration.cancel();
        unsafe { dispatch_release(self.queue) };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Remote,
    Legacy,
}

enum Channel {
    Remote(Remote),
    Legacy(Legacy),
}

pub struct Input {
    channel: Channel,
}

impl Input {
    fn open(device: &NSObject, route: Option<Route>, verify: bool) -> Result<Self, Error> {
        let channel = match route {
            Some(Route::Legacy) => Channel::Legacy(Legacy::open(device)?),
            Some(Route::Remote) => Channel::Remote(
                Remote::connect(device, verify)?
                    .ok_or(Error::Remote("is not offered by this simulator"))?,
            ),
            None => match Remote::connect(device, verify)? {
                Some(remote) => Channel::Remote(remote),
                None => Channel::Legacy(Legacy::open(device)?),
            },
        };
        Ok(Self { channel })
    }

    pub fn route(&self) -> Route {
        match self.channel {
            Channel::Remote(_) => Route::Remote,
            Channel::Legacy(_) => Route::Legacy,
        }
    }

    pub fn touch(&self, phase: TouchPhase, x: f64, y: f64) -> Result<(), Error> {
        match &self.channel {
            Channel::Remote(remote) => remote.touch(phase, x, y),
            Channel::Legacy(legacy) => legacy.touch(phase, x, y),
        }
    }

    pub fn key(&self, usage: u32, direction: KeyDirection) -> Result<(), Error> {
        match &self.channel {
            Channel::Remote(remote) => remote.key(usage, direction),
            Channel::Legacy(legacy) => legacy.key(usage, direction),
        }
    }

    pub fn button(&self, button: Button, direction: KeyDirection) -> Result<(), Error> {
        match &self.channel {
            Channel::Remote(remote) => remote.button(button, direction),
            Channel::Legacy(legacy) => legacy.button(button, direction),
        }
    }

    pub fn flush(&self) -> Result<(), Error> {
        match &self.channel {
            Channel::Remote(remote) => remote.flush(),
            Channel::Legacy(_) => Ok(()),
        }
    }
}

struct Legacy {
    client: Retained<AnyObject>,
    mouse: MouseMessage,
    key: KeyMessage,
    button: Option<ButtonMessage>,
}

impl Legacy {
    fn open(device: &NSObject) -> Result<Self, Error> {
        const CLIENT: &CStr = c"_TtC12SimulatorKit24SimDeviceLegacyHIDClient";
        let class = class(CLIENT)?;
        let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        let client: Result<Retained<AnyObject>, Retained<NSError>> =
            unsafe { msg_send![allocated, initWithDevice: device, error: _] };
        let client = client.map_err(|error| service_error(&error))?;
        let mouse = symbol(c"IndigoHIDMessageForMouseNSEvent")?;
        let key = symbol(c"IndigoHIDMessageForKeyboardArbitrary")?;
        let button = symbol(c"IndigoHIDMessageForButton").ok();
        Ok(Self {
            client,
            mouse: unsafe { transmute::<*mut c_void, MouseMessage>(mouse) },
            key: unsafe { transmute::<*mut c_void, KeyMessage>(key) },
            button: button.map(|button| unsafe { transmute::<*mut c_void, ButtonMessage>(button) }),
        })
    }

    fn touch(&self, phase: TouchPhase, x: f64, y: f64) -> Result<(), Error> {
        let point = CGPoint { x, y };
        let event = match phase {
            TouchPhase::Down | TouchPhase::Move => MOUSE_DOWN,
            TouchPhase::Up => MOUSE_UP,
        };
        let message =
            unsafe { (self.mouse)(&raw const point, null(), TOUCH_TARGET, event, 1.0, 1.0, 0) };
        self.send(message, "touch")
    }

    fn key(&self, usage: u32, direction: KeyDirection) -> Result<(), Error> {
        let direction = match direction {
            KeyDirection::Down => KEY_DOWN,
            KeyDirection::Up => KEY_UP,
        };
        let message = unsafe { (self.key)(usage, direction) };
        self.send(message, "key")
    }

    fn button(&self, button: Button, direction: KeyDirection) -> Result<(), Error> {
        let Some(message) = self.button else {
            return Err(Error::MissingSymbol("IndigoHIDMessageForButton"));
        };
        let source = match button {
            Button::Home => BUTTON_HOME,
            Button::Lock => BUTTON_LOCK,
        };
        let direction = match direction {
            KeyDirection::Down => KEY_DOWN,
            KeyDirection::Up => KEY_UP,
        };
        let message = unsafe { message(source, direction as i32, BUTTON_TARGET) };
        self.send(message, "button")
    }

    fn send(&self, message: *mut c_void, kind: &'static str) -> Result<(), Error> {
        if message.is_null() {
            return Err(Error::Message(kind));
        }
        let send = unsafe { transmute::<Send, SendMessage>(objc_msgSend) };
        unsafe {
            send(
                Retained::as_ptr(&self.client),
                sel!(sendWithMessage:freeWhenDone:completionQueue:completion:),
                message,
                Bool::YES,
                null(),
                null(),
            );
        }
        Ok(())
    }
}

fn device_list() -> Result<Retained<NSArray<NSObject>>, Error> {
    let developer = load_frameworks()?;
    let context_class = class(c"SimServiceContext")?;
    let developer = NSString::from_str(&developer.to_string_lossy());
    let context: Result<Retained<NSObject>, Retained<NSError>> = unsafe {
        msg_send![context_class, sharedServiceContextForDeveloperDir: &*developer, error: _]
    };
    let context = context.map_err(|error| service_error(&error))?;
    let set: Result<Retained<NSObject>, Retained<NSError>> =
        unsafe { msg_send![&*context, defaultDeviceSetWithError: _] };
    let set = set.map_err(|error| service_error(&error))?;
    Ok(unsafe { msg_send![&*set, devices] })
}

fn identifier(device: &NSObject) -> String {
    let identifier: Retained<NSUUID> = unsafe { msg_send![device, UDID] };
    identifier.UUIDString().to_string()
}

fn text(receiver: &AnyObject, selector: Sel) -> String {
    object(receiver, selector).map_or_else(String::new, |value| {
        let value: &NSString = unsafe { &*Retained::as_ptr(&value).cast() };
        value.to_string()
    })
}

fn state(device: &NSObject) -> String {
    text(device, sel!(stateString))
}

fn name(device: &NSObject) -> String {
    text(device, sel!(name))
}

fn system(device: &NSObject) -> String {
    object(device, sel!(runtime)).map_or_else(String::new, |runtime| text(&runtime, sel!(name)))
}

fn find(udid: &str) -> Result<Retained<NSObject>, Error> {
    device_list()?
        .iter()
        .find(|device| identifier(device).eq_ignore_ascii_case(udid))
        .ok_or_else(|| Error::DeviceNotFound(udid.to_owned()))
}

pub fn devices() -> Result<Vec<Device>, Error> {
    let mut devices: Vec<Device> = device_list()?
        .iter()
        .map(|device| Device {
            udid: identifier(&device),
            name: name(&device),
            system: system(&device),
            booted: state(&device) == BOOTED,
        })
        .filter(|device| !device.system.is_empty())
        .collect();
    devices.sort_by(|a, b| b.booted.cmp(&a.booted).then_with(|| a.name.cmp(&b.name)));
    Ok(devices)
}

pub fn boot(udid: &str) -> Result<(), Error> {
    let device = find(udid)?;
    if state(&device) == BOOTED {
        return Ok(());
    }
    if !responds(&device, sel!(bootWithOptions:error:)) {
        return Err(Error::MissingSymbol("bootWithOptions:error:"));
    }
    let booted: Result<(), Retained<NSError>> =
        unsafe { msg_send![&*device, bootWithOptions: None::<&NSObject>, error: _] };
    booted.map_err(|error| service_error(&error))
}

fn load_frameworks() -> Result<&'static Path, Error> {
    static DEVELOPER: OnceLock<Option<PathBuf>> = OnceLock::new();
    let developer = DEVELOPER.get_or_init(|| {
        let developer = std::env::var_os("DEVELOPER_DIR")
            .map(PathBuf::from)
            .or_else(|| std::fs::read_link(XCODE_SELECT_LINK).ok())
            .filter(|path| path.is_dir())?;
        open_library(Path::new(CORE_SIMULATOR));
        for relative in SIMULATOR_KIT {
            open_library(&developer.join(relative));
        }
        Some(developer)
    });
    developer.as_deref().ok_or(Error::DeveloperDir)
}

fn open_library(path: &Path) {
    if let Ok(path) = CString::new(path.as_os_str().as_encoded_bytes()) {
        unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW) };
    }
}

fn class(name: &'static CStr) -> Result<&'static AnyClass, Error> {
    AnyClass::get(name).ok_or_else(|| Error::MissingClass(name.to_str().unwrap_or("?")))
}

fn symbol(name: &'static CStr) -> Result<*mut c_void, Error> {
    let address = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    if address.is_null() {
        return Err(Error::MissingSymbol(name.to_str().unwrap_or("?")));
    }
    Ok(address)
}

fn responds(object: &AnyObject, selector: Sel) -> bool {
    let responds = unsafe { transmute::<Send, Responds>(objc_msgSend) };
    unsafe { responds(object, sel!(respondsToSelector:), selector) }.as_bool()
}

fn object(receiver: &AnyObject, selector: Sel) -> Option<Retained<AnyObject>> {
    if !responds(receiver, selector) {
        return None;
    }
    let get = unsafe { transmute::<Send, GetObject>(objc_msgSend) };
    unsafe { Retained::retain(get(receiver, selector)) }
}

fn surface_size(surface: &Retained<AnyObject>) -> (usize, usize) {
    let surface = Retained::as_ptr(surface).cast::<c_void>();
    unsafe { (IOSurfaceGetWidth(surface), IOSurfaceGetHeight(surface)) }
}

fn service_error(error: &NSError) -> Error {
    Error::Service(error.localizedDescription().to_string())
}
