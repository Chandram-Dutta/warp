use warpui::EntityId;

use super::WorkspaceAction;
#[cfg(feature = "local_only")]
use crate::palette::PaletteMode;
use crate::pane_group::TerminalPaneId;
#[cfg(feature = "local_only")]
use crate::server::telemetry::PaletteSource;
#[cfg(feature = "local_only")]
use crate::settings::DefaultSessionMode;
#[cfg(feature = "local_only")]
use crate::settings_view::{SettingsAction, SettingsSection};
use crate::workspace::PaneViewLocator;
use crate::workspace::tab_settings::{
    VerticalTabsDisplayGranularity, VerticalTabsPrimaryInfo, VerticalTabsTabItemMode,
    VerticalTabsViewMode,
};

#[test]
#[cfg(feature = "local_only")]
fn local_only_product_rejects_cloud_agent_and_ide_actions() {
    assert!(WorkspaceAction::AddDefaultTab.is_available_in_product());
    assert!(WorkspaceAction::ShowSettings.is_available_in_product());
    assert!(!WorkspaceAction::LogOut.is_available_in_product());
    assert!(!WorkspaceAction::AddAgentTab.is_available_in_product());
    assert!(!WorkspaceAction::OpenWarpDrive.is_available_in_product());
    assert!(!WorkspaceAction::OpenMCPServerCollection.is_available_in_product());
    assert!(!WorkspaceAction::NewCodeFile.is_available_in_product());
    assert!(!WorkspaceAction::JoinSlack.is_available_in_product());
    assert!(!WorkspaceAction::SendFeedback.is_available_in_product());
    assert!(!WorkspaceAction::CheckForUpdate.is_available_in_product());
    assert!(
        !WorkspaceAction::OpenPalette {
            mode: PaletteMode::WarpDrive,
            source: PaletteSource::Keybinding,
            query: None,
        }
        .is_available_in_product()
    );
    assert!(
        WorkspaceAction::OpenPalette {
            mode: PaletteMode::Navigation,
            source: PaletteSource::Keybinding,
            query: None,
        }
        .is_available_in_product()
    );
    assert!(
        !WorkspaceAction::TabConfigSidecarMakeDefault {
            mode: DefaultSessionMode::Agent,
            tab_config_path: None,
            shell: None,
        }
        .is_available_in_product()
    );

    assert!(
        WorkspaceAction::DispatchToSettingsTab(SettingsAction::SelectAndRefresh(
            SettingsSection::Appearance,
        ))
        .is_available_in_product()
    );
    assert!(
        !WorkspaceAction::DispatchToSettingsTab(SettingsAction::SelectAndRefresh(
            SettingsSection::Account,
        ))
        .is_available_in_product()
    );
    assert!(
        !WorkspaceAction::DispatchToSettingsTab(SettingsAction::SelectAndRefresh(
            SettingsSection::WarpAgent,
        ))
        .is_available_in_product()
    );
}

#[test]
fn vertical_tabs_view_mode_change_does_not_save_workspace_state() {
    assert!(
        !WorkspaceAction::SetVerticalTabsViewMode(VerticalTabsViewMode::Compact)
            .should_save_app_state_on_action()
    );
}

#[test]
fn vertical_tabs_panel_toggle_still_saves_workspace_state() {
    assert!(WorkspaceAction::ToggleVerticalTabsPanel.should_save_app_state_on_action());
}

#[test]
fn settings_popup_toggle_does_not_save_workspace_state() {
    assert!(!WorkspaceAction::ToggleVerticalTabsSettingsPopup.should_save_app_state_on_action());
}

#[test]
fn display_granularity_change_does_not_save_workspace_state() {
    assert!(
        !WorkspaceAction::SetVerticalTabsDisplayGranularity(VerticalTabsDisplayGranularity::Panes)
            .should_save_app_state_on_action()
    );
    assert!(
        !WorkspaceAction::SetVerticalTabsDisplayGranularity(VerticalTabsDisplayGranularity::Tabs)
            .should_save_app_state_on_action()
    );
}

#[test]
fn tab_item_mode_change_does_not_save_workspace_state() {
    assert!(
        !WorkspaceAction::SetVerticalTabsTabItemMode(VerticalTabsTabItemMode::FocusedSession)
            .should_save_app_state_on_action()
    );
    assert!(
        !WorkspaceAction::SetVerticalTabsTabItemMode(VerticalTabsTabItemMode::Summary)
            .should_save_app_state_on_action()
    );
}

#[test]
fn primary_info_change_does_not_save_workspace_state() {
    assert!(
        !WorkspaceAction::SetVerticalTabsPrimaryInfo(VerticalTabsPrimaryInfo::Command)
            .should_save_app_state_on_action()
    );
    assert!(
        !WorkspaceAction::SetVerticalTabsPrimaryInfo(VerticalTabsPrimaryInfo::WorkingDirectory)
            .should_save_app_state_on_action()
    );
    assert!(
        !WorkspaceAction::SetVerticalTabsPrimaryInfo(VerticalTabsPrimaryInfo::Branch)
            .should_save_app_state_on_action()
    );
}

#[test]
fn pane_name_actions_save_workspace_state() {
    let locator = PaneViewLocator {
        pane_group_id: EntityId::new(),
        pane_id: TerminalPaneId::dummy_terminal_pane_id().into(),
    };

    assert!(WorkspaceAction::RenamePane(locator).should_save_app_state_on_action());
    assert!(WorkspaceAction::ResetPaneName(locator).should_save_app_state_on_action());
    // GH-9351: the keyboard-bindable variant must persist app state on the
    // same conditions as the locator-based one, since both ultimately drive
    // `rename_pane` which mutates `pane_configuration`.
    assert!(WorkspaceAction::RenameActivePane.should_save_app_state_on_action());
}
