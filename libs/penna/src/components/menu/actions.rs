use ala::{Action, actions};
use serde::Deserialize;

#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = penna_menu, no_json)]
pub struct Confirm {
    pub secondary: bool,
}

actions!(
    penna_menu,
    [Cancel, SelectUp, SelectDown, SelectLeft, SelectRight]
);
