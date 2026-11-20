#[cfg(target_os = "macos")]
mod live;
#[cfg(not(target_os = "macos"))]
mod stub;

#[cfg(target_os = "macos")]
pub(crate) use live::SimulatorView;
#[cfg(not(target_os = "macos"))]
pub(crate) use stub::SimulatorView;
