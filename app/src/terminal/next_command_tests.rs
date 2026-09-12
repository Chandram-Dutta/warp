use super::*;

#[test]
fn provider_context_contains_only_command_context() {
    let context = CommandContext {
        shell_name: "zsh".to_owned(),
        shell_version: Some("5.9".to_owned()),
        history_contexts: vec![],
        recent_commands: vec![RecentCommand {
            command: "git status".to_owned(),
            working_directory: Some("/repo".to_owned()),
            exit_code: 0,
        }],
    };

    let provider_context = context.provider_context(Some("git "));

    assert_eq!(provider_context.shell.name, "zsh");
    assert_eq!(provider_context.shell.version.as_deref(), Some("5.9"));
    assert_eq!(provider_context.working_directory.as_deref(), Some("/repo"));
    assert_eq!(provider_context.command_prefix, "git ");
    assert_eq!(provider_context.recent_commands, context.recent_commands);
}

#[test]
fn provider_context_defaults_missing_prefix_and_shell_version() {
    let context = CommandContext {
        shell_name: "fish".to_owned(),
        shell_version: None,
        history_contexts: vec![],
        recent_commands: vec![],
    };

    let provider_context = context.provider_context(None);

    assert_eq!(provider_context.shell.name, "fish");
    assert_eq!(provider_context.shell.version, None);
    assert_eq!(provider_context.command_prefix, "");
}
