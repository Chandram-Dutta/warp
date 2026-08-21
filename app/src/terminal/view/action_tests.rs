use super::TerminalAction;

#[test]
#[cfg(feature = "local_only")]
fn local_only_rejects_agent_cloud_sharing_and_ide_terminal_actions() {
    assert!(TerminalAction::SetInputModeTerminal.is_available_in_product());
    assert!(TerminalAction::InsertMostRecentCommandCorrection.is_available_in_product());
    assert!(TerminalAction::ToggleSessionRecording.is_available_in_product());

    assert!(!TerminalAction::SetInputModeAgent.is_available_in_product());
    assert!(!TerminalAction::OpenViewMCPPane.is_available_in_product());
    assert!(!TerminalAction::OpenShareModal.is_available_in_product());
    assert!(!TerminalAction::OpenWorkflowModal.is_available_in_product());
    assert!(!TerminalAction::OpenConversationsPalette.is_available_in_product());
    assert!(!TerminalAction::StartLspServer.is_available_in_product());
}
