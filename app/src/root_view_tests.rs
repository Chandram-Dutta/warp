use super::workspace_action_for_open_settings;
use crate::uri::OpenSettingsArgs;
use crate::workspace::WorkspaceAction;

#[test]
fn settings_deeplink_preserves_search_query() {
    let action = workspace_action_for_open_settings(&OpenSettingsArgs::Search {
        query: "next command".into(),
    });
    let WorkspaceAction::ShowSettingsPageWithSearch {
        search_query,
        section,
    } = action
    else {
        panic!("expected settings search");
    };
    assert_eq!(search_query, "next command");
    assert!(section.is_none());
}

#[test]
fn default_settings_deeplink_opens_settings() {
    assert!(matches!(
        workspace_action_for_open_settings(&OpenSettingsArgs::Default),
        WorkspaceAction::ShowSettings
    ));
}
