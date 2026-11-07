fn main() {
    let ui_keys = std::env::args().nth(1).as_deref() == Some("--ui-keys");
    let value = if ui_keys {
        zz_ui::files::rank_fixture::adapter_fixture::ui_keys()
    } else {
        zz_ui::files::rank_fixture::adapter_fixture::vectors()
    };
    println!("{value}");
}
