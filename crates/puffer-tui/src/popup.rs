use puffer_core::CommandSpec;

/// Returns slash-command popup rows for the current slash-input prefix.
pub(crate) fn popup_rows<'a>(input: &str, commands: &'a [CommandSpec]) -> Vec<&'a CommandSpec> {
    let filter = input.trim_start_matches('/');
    let mut rows = commands
        .iter()
        .filter(|command| command_matches(command, filter))
        .collect::<Vec<_>>();
    rows.sort_by_key(|command| sort_key(command, filter));
    rows.truncate(8);
    rows
}

fn command_matches(command: &CommandSpec, filter: &str) -> bool {
    filter.is_empty()
        || command.name.starts_with(filter)
        || command
            .aliases
            .iter()
            .any(|alias| alias.starts_with(filter))
        || command.name.contains(filter)
        || command.aliases.iter().any(|alias| alias.contains(filter))
}

fn sort_key(command: &CommandSpec, filter: &str) -> (u8, String) {
    if filter.is_empty() {
        return (0, command.name.to_string());
    }
    if command.name == filter || command.aliases.contains(&filter) {
        return (0, command.name.to_string());
    }
    if command.name.starts_with(filter) {
        return (1, command.name.to_string());
    }
    if command
        .aliases
        .iter()
        .any(|alias| alias.starts_with(filter))
    {
        return (2, command.name.to_string());
    }
    (3, command.name.to_string())
}

#[cfg(test)]
mod tests {
    use super::popup_rows;
    use puffer_core::supported_commands;

    #[test]
    fn popup_prefers_prefix_matches() {
        let commands = supported_commands();
        let rows = popup_rows("/re", &commands);
        let names = rows.iter().map(|row| row.name).collect::<Vec<_>>();
        assert!(names.starts_with(&["reload-plugins", "remote-control", "remote-env"]));
        assert!(names.contains(&"review"));
    }
}
