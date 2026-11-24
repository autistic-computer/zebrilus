pub(crate) mod add_host;
mod host_update;
mod new_session;
pub(crate) mod sidebar;
mod ssh_prompt;
pub(crate) mod tree;
mod view;
mod which_key;

pub use view::AppView;
pub(crate) use view::ClosePane;
pub(crate) use view::maybe_prompt_stale_daemon;
pub(crate) use view::pip_fill_rows;
pub(crate) use view::top_right_pane;

pub fn init(cx: &mut ala::App) {
    sidebar::init(cx);
}
