#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for device in zz_simulator::devices()? {
        println!(
            "{} {} {} {}",
            device.udid,
            if device.booted { "booted" } else { "off" },
            device.system,
            device.name
        );
    }
    Ok(())
}
