use std::collections::HashSet;

use super::*;

#[test]
fn removed_commands_are_not_registered() {
    let registry = Registry::new();
    for name in [
        "/index",
        "/version",
        "/reset-statusline",
        "/statusline",
        "/auto-approve",
        "/mcp",
        "/add-mcp",
        "/open-mcp-servers",
        "/view-logs",
        "/voice",
        "/natural-language-detection",
        "/api-keys",
        "/connect-grok",
        "/manage-billing",
        "/upgrade",
        "/theme",
        "/exit",
        "/status",
        "/logout",
        "/clear",
        "/vim-mode",
    ] {
        assert!(registry.get_command_with_name(name).is_none(), "{name}");
    }
}

#[test]
fn command_names_and_kinds_are_unique() {
    let mut names = HashSet::new();
    let mut kinds = HashSet::new();
    for command in all_commands() {
        assert!(
            names.insert(command.name),
            "duplicate name: {}",
            command.name
        );
        assert!(
            kinds.insert(command.kind),
            "duplicate kind: {:?}",
            command.kind
        );
        assert!(
            !command.icon_path.is_empty(),
            "{} needs an icon",
            command.name
        );
    }
}

#[test]
fn rename_tab_command_requires_argument() {
    let command = COMMAND_REGISTRY
        .get_command_with_name(RENAME_TAB.name)
        .expect("expected /rename-tab to be registered");
    let argument = command.argument.as_ref().unwrap();
    assert!(!argument.is_optional);
    assert!(!argument.should_execute_on_selection);
    assert_eq!(argument.hint_text, Some("<tab name>"));
}

#[test]
fn rename_conversation_command_is_active_conversation_scoped_and_requires_argument() {
    let command = COMMAND_REGISTRY
        .get_command_with_name(RENAME_CONVERSATION.name)
        .expect("expected /rename-conversation to be registered");
    let argument = command.argument.as_ref().unwrap();
    assert_eq!(command.name, "/rename-conversation");
    assert_eq!(command.icon_path, "bundled/svg/pencil-line.svg");
    assert!(!command.auto_enter_ai_mode);
    assert_eq!(
        command.availability,
        Availability::AGENT_VIEW | Availability::ACTIVE_CONVERSATION | Availability::AI_ENABLED
    );
    assert!(!argument.is_optional);
    assert!(!argument.should_execute_on_selection);
    assert_eq!(argument.hint_text, Some("<new title>"));
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn continue_locally_command_is_registered() {
    let command = COMMAND_REGISTRY
        .get_command_with_name(CONTINUE_LOCALLY.name)
        .expect("expected /continue-locally to be registered");
    assert_eq!(command.name, "/continue-locally");
    assert_eq!(command.icon_path, "bundled/svg/arrow-split.svg");
    assert!(command.auto_enter_ai_mode);
    assert_eq!(
        command.availability,
        Availability::AGENT_VIEW
            | Availability::ACTIVE_CONVERSATION
            | Availability::AI_ENABLED
            | Availability::CLOUD_AGENT
    );
    let argument = command.argument.as_ref().unwrap();
    assert!(argument.is_optional);
    assert!(!argument.should_execute_on_selection);
    assert_eq!(
        argument.hint_text,
        Some("<optional prompt to send in local conversation>")
    );
}

#[test]
fn set_tab_color_command_requires_argument() {
    let command = COMMAND_REGISTRY
        .get_command_with_name(SET_TAB_COLOR.name)
        .expect("expected /set-tab-color to be registered");
    let argument = command.argument.as_ref().unwrap();
    assert!(!argument.is_optional);
    assert!(!argument.should_execute_on_selection);
    let hint = argument.hint_text.unwrap();
    for color in color_dot::TAB_COLOR_OPTIONS {
        let lower = color.to_string().to_ascii_lowercase();
        assert!(hint.contains(&lower), "hint should mention `{lower}`");
    }
    assert!(hint.contains("none"), "hint should mention `none`");
}

#[test]
fn strip_command_prefix_matches_orchestrate() {
    assert_eq!(
        strip_command_prefix("/orchestrate deploy services", "/orchestrate"),
        Some("deploy services".to_string())
    );
}

#[test]
fn strip_command_prefix_no_match() {
    assert_eq!(strip_command_prefix("just a normal query", "/plan"), None);
}

#[test]
fn strip_command_prefix_empty() {
    assert_eq!(strip_command_prefix("", "/plan"), None);
}

#[test]
fn strip_command_prefix_no_trailing_space() {
    assert_eq!(strip_command_prefix("/plan", "/plan"), None);
}

#[test]
fn strip_command_prefix_trailing_space_only() {
    assert_eq!(strip_command_prefix("/plan ", "/plan"), Some(String::new()));
}

#[test]
fn strip_command_prefix_substring_not_matched() {
    assert_eq!(strip_command_prefix("/planning something", "/plan"), None);
}

#[test]
fn copy_debugging_id_command_has_correct_registry_metadata() {
    let command = COMMAND_REGISTRY
        .get_command_with_name("/copy-debugging-id")
        .unwrap();
    assert_eq!(command.kind, SlashCommandKind::CopyDebuggingId);
    assert_eq!(command.icon_path, "bundled/svg/copy.svg");
    assert!(!command.auto_enter_ai_mode);
    assert_eq!(command.availability, Availability::ACTIVE_CONVERSATION);
    assert!(command.argument.is_none());
    assert!(command.is_active(Availability::ACTIVE_CONVERSATION));
    assert!(!command.is_active(Availability::ALWAYS));
}
