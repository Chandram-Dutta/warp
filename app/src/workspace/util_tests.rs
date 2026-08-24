use warpui::App;

use super::WorkspaceState;

#[test]
#[cfg(feature = "local_only")]
fn local_only_ignores_stale_agent_and_cloud_workspace_state() {
    App::test((), |app| async move {
        let state = WorkspaceState {
            is_resource_center_open: true,
            is_ai_assistant_panel_open: true,
            is_warp_drive_open: true,
            is_agent_management_popup_open: true,
            is_reward_modal_open: true,
            is_workflow_modal_open: true,
            is_prompt_editor_open: true,
            is_agent_toolbar_editor_open: true,
            is_shared_objects_creation_denied_modal_open: true,
            is_suggested_agent_mode_workflow_modal_open: true,
            is_suggested_rule_modal_open: true,
            is_enable_auto_reload_modal_open: true,
            is_notification_mailbox_open: true,
            is_agent_management_view_open: true,
            is_codex_modal_open: true,
            is_cloud_agent_capacity_modal_open: true,
            is_prompt_suggestions_unavailable_modal_open: true,
            is_new_worktree_modal_open: true,
            ..Default::default()
        };

        assert!(!state.is_right_panel_open());
        assert!(!state.is_any_non_palette_modal_open(&app));
        assert!(!state.is_any_non_terminal_view_open(&app));

        let terminal_state = WorkspaceState {
            is_header_toolbar_editor_open: true,
            ..Default::default()
        };
        assert!(terminal_state.is_any_non_palette_modal_open(&app));
        assert!(terminal_state.is_any_non_terminal_view_open(&app));
    });
}
