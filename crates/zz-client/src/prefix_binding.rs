use zz_protocol::{Binding, KeyBindingSnapshot, KeyTables};
use zz_terminal::KeyInput;

pub fn prefix_binding(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> Option<Binding> {
    let mut tables = KeyTables::empty();
    for binding in bindings {
        tables.bind(
            "prefix",
            &binding.key,
            Binding {
                commands: binding.commands.clone(),
                repeat: binding.repeat,
                note: None,
            },
        );
    }
    tables.resolve_input("prefix", input).cloned()
}

pub fn binds_input(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> bool {
    prefix_binding(bindings, input).is_some()
}

pub fn is_all_keys_input(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> bool {
    prefix_binding(bindings, input)
        .is_some_and(|binding| crate::which_key::opens_all_keys(&binding.commands))
}
