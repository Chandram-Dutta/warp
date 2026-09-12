use super::WorkspaceState;

#[test]
#[cfg(feature = "local_only")]
fn local_only_ignores_stale_agent_and_cloud_workspace_state() {
    let state = WorkspaceState {
        is_resource_center_open: true,
        is_agent_management_popup_open: true,
        is_workflow_modal_open: true,
        is_prompt_editor_open: true,
        is_agent_toolbar_editor_open: true,
        is_suggested_agent_mode_workflow_modal_open: true,
        is_suggested_rule_modal_open: true,
        is_notification_mailbox_open: true,
        is_agent_management_view_open: true,
        is_cloud_agent_capacity_modal_open: true,
        is_new_worktree_modal_open: true,
        ..Default::default()
    };

    let terminal_state = WorkspaceState {
        is_header_toolbar_editor_open: true,
        ..Default::default()
    };

    assert!(!state.is_right_panel_open());
    assert!(!state.is_any_non_palette_modal_open());
    assert!(!state.is_any_non_terminal_view_open());
    assert!(terminal_state.is_any_non_palette_modal_open());
    assert!(terminal_state.is_any_non_terminal_view_open());
}
