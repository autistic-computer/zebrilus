fn main() -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(
        std::io::stdout().lock(),
        &zz_ui::files::rank_fixture::vectors(),
    )?;
    Ok(())
}
