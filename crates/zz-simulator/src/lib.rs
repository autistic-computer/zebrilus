#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod remote;

#[cfg(target_os = "macos")]
pub use macos::{Error, Input, KeyDirection, Route, Screen, Simulator, Surface, TouchPhase};
