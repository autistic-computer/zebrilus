use zz_protocol::{Binding, KeyBindingSnapshot, KeyTables, TmuxOption};
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

pub fn is_sidebar_picker_input(bindings: &[KeyBindingSnapshot], input: &KeyInput) -> bool {
    let Some(binding) = prefix_binding(bindings, input) else {
        return false;
    };
    let [command] = binding.commands.as_slice() else {
        return false;
    };
    if command.name != "choose-tree" {
        return false;
    }
    let Some(spec) = zz_protocol::command_spec(&command.name) else {
        return false;
    };
    let Ok(parsed) = zz_protocol::parse_tmux_command_options(spec, command) else {
        return false;
    };
    parsed.positionals.is_empty()
        && (parsed.options.contains(&TmuxOption::Flag("-s"))
            ^ parsed.options.contains(&TmuxOption::Flag("-w")))
        && parsed
            .options
            .iter()
            .all(|option| matches!(option, TmuxOption::Flag("-s" | "-w" | "-Z")))
}
