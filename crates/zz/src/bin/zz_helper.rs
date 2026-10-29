fn main() -> std::process::ExitCode {
    let code = u8::try_from(ala_cef::run_subprocess(&zz::browser_runtime_config()).clamp(0, 255))
        .unwrap_or_default();
    std::process::ExitCode::from(code)
}
