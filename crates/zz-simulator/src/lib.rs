mod keys;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod remote;

pub use keys::{SHIFT, key_usage};
#[cfg(target_os = "macos")]
pub use macos::{
    Button, Device, Error, Input, KeyDirection, Route, Screen, Simulator, Surface, TouchPhase,
    boot, devices,
};
