use serde::Deserialize;
use zz_gpui::{Action, actions};

#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = zz_menu, no_json)]
pub struct Confirm {
    pub secondary: bool,
}

actions!(
    zz_menu,
    [Cancel, SelectUp, SelectDown, SelectLeft, SelectRight]
);
