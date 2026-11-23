pub use ala_cef::*;

pub const USES_EXTERNAL_MESSAGE_PUMP: bool = cfg!(feature = "chromium");

#[cfg(all(target_os = "linux", feature = "ladybird", not(feature = "chromium")))]
mod ladybird;
#[cfg(all(target_os = "linux", feature = "ladybird", not(feature = "chromium")))]
pub use ladybird::*;

#[cfg(all(
    feature = "ladybird",
    not(feature = "chromium"),
    not(target_os = "linux")
))]
compile_error!("The experimental Ladybird desktop backend requires Linux.");
