use std::process::ExitCode;

#[cfg(not(windows))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    ala_cef::install_early_malloc_zone();
    zz::run()
}
