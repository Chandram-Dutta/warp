use settings::Setting;
use warpui::{App, SingletonEntity};

use super::*;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspace::header_toolbar_item::HeaderToolbarItemKind;

#[test]
fn use_latest_user_prompt_as_conversation_title_in_tab_names_defaults_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.use_latest_user_prompt_as_conversation_title_in_tab_names);
        });
    });
}

#[test]
fn use_latest_user_prompt_as_conversation_title_in_tab_names_uses_vertical_tabs_path() {
    assert_eq!(
        UseLatestUserPromptAsConversationTitleInTabNames::toml_path(),
        Some("appearance.vertical_tabs.use_latest_prompt_as_title")
    );
    assert_eq!(
        UseLatestUserPromptAsConversationTitleInTabNames::hierarchy(),
        Some("appearance.vertical_tabs")
    );
    assert_eq!(
        UseLatestUserPromptAsConversationTitleInTabNames::toml_key(),
        "use_latest_prompt_as_title"
    );
}

#[test]
fn show_vertical_tab_panel_in_restored_windows_defaults_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.show_vertical_tab_panel_in_restored_windows);
        });
    });
}

#[test]
fn show_vertical_tab_panel_in_restored_windows_uses_vertical_tabs_path() {
    assert_eq!(
        ShowVerticalTabPanelInRestoredWindows::toml_path(),
        Some("appearance.vertical_tabs.show_panel_in_restored_windows")
    );
    assert_eq!(
        ShowVerticalTabPanelInRestoredWindows::hierarchy(),
        Some("appearance.vertical_tabs")
    );
    assert_eq!(
        ShowVerticalTabPanelInRestoredWindows::toml_key(),
        "show_panel_in_restored_windows"
    );
}

#[test]
fn hide_title_bar_search_bar_in_vertical_tabs_defaults_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.hide_title_bar_search_bar_in_vertical_tabs);
        });
    });
}

#[test]
fn hide_title_bar_search_bar_in_vertical_tabs_uses_vertical_tabs_path() {
    assert_eq!(
        HideTitleBarSearchBarInVerticalTabs::toml_path(),
        Some("appearance.vertical_tabs.hide_title_bar_search_bar")
    );
    assert_eq!(
        HideTitleBarSearchBarInVerticalTabs::hierarchy(),
        Some("appearance.vertical_tabs")
    );
    assert_eq!(
        HideTitleBarSearchBarInVerticalTabs::toml_key(),
        "hide_title_bar_search_bar"
    );
}

#[test]
fn header_toolbar_chip_selection_default_contains_tabs_panel() {
    let config = HeaderToolbarChipSelection::Default;
    assert!(config.contains_item(&HeaderToolbarItemKind::TabsPanel));
    assert!(config.right_items().is_empty());
}

#[test]
fn header_toolbar_chip_selection_custom_tabs_panel_on_right_reports_present() {
    let config = HeaderToolbarChipSelection::Custom {
        left: vec![],
        right: vec![HeaderToolbarItemKind::TabsPanel],
    };
    assert!(config.contains_item(&HeaderToolbarItemKind::TabsPanel));
    assert!(config.left_items().is_empty());
    assert_eq!(config.right_items(), vec![HeaderToolbarItemKind::TabsPanel]);
}

#[test]
fn header_toolbar_chip_selection_custom_tabs_panel_on_left_reports_present() {
    let config = HeaderToolbarChipSelection::Custom {
        left: vec![HeaderToolbarItemKind::TabsPanel],
        right: vec![],
    };
    assert!(config.contains_item(&HeaderToolbarItemKind::TabsPanel));
    assert_eq!(config.left_items(), vec![HeaderToolbarItemKind::TabsPanel]);
    assert!(config.right_items().is_empty());
}

#[test]
fn header_toolbar_chip_selection_custom_empty_reports_all_absent() {
    let config = HeaderToolbarChipSelection::Custom {
        left: vec![],
        right: vec![],
    };
    for item in HeaderToolbarItemKind::all_items() {
        assert!(!config.contains_item(&item));
    }
}

#[test]
fn header_toolbar_prunes_persisted_agent_and_ide_items() {
    let selection: HeaderToolbarChipSelection = serde_json::from_str(
        r#"{"Custom":{"left":["AgentManagement","TabsPanel"],"right":["CodeReview","NotificationsMailbox"]}}"#,
    ).unwrap();

    assert_eq!(
        selection.left_items(),
        vec![HeaderToolbarItemKind::TabsPanel]
    );
    assert!(selection.right_items().is_empty());
    assert_eq!(
        HeaderToolbarItemKind::all_items(),
        vec![HeaderToolbarItemKind::TabsPanel]
    );
}

#[test]
fn removed_products_preserve_saved_tabs_panel_side() {
    let selection: HeaderToolbarChipSelection = serde_json::from_str(
        r#"{"Custom":{"left":["ToolsPanel","CodeReview"],"right":["TabsPanel","ToolsPanel","NotificationsMailbox"]}}"#,
    )
    .unwrap();
    assert_eq!(
        selection,
        HeaderToolbarChipSelection::Custom {
            left: vec![],
            right: vec![HeaderToolbarItemKind::TabsPanel],
        }
    );
    let serialized = serde_json::to_string(&selection).unwrap();
    assert_eq!(
        serialized,
        r#"{"Custom":{"left":[],"right":["TabsPanel"]}}"#
    );
    assert_eq!(
        serde_json::from_str::<HeaderToolbarChipSelection>(&serialized).unwrap(),
        selection
    );
}

#[test]
fn toolbar_compatibility_does_not_silently_discard_invalid_items() {
    for item in [r#""UnknownItem""#, "42", r#"{"ToolsPanel":true}"#] {
        let json = format!(r#"{{"Custom":{{"left":[],"right":[{item}]}}}}"#);
        assert!(serde_json::from_str::<HeaderToolbarChipSelection>(&json).is_err());
    }
}
