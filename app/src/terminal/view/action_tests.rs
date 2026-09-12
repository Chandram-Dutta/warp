use super::TerminalAction;
#[cfg(feature = "local_only")]
use crate::terminal::view::{ContextMenuAction, InputContextMenuAction};

#[test]
#[cfg(feature = "local_only")]
fn local_only_rejects_agent_cloud_sharing_and_ide_terminal_actions() {
    assert!(TerminalAction::InsertMostRecentCommandCorrection.is_available_in_product());
    assert!(TerminalAction::ToggleSessionRecording.is_available_in_product());

    assert!(!TerminalAction::OpenWorkflowModal.is_available_in_product());

    assert!(TerminalAction::ContextMenu(ContextMenuAction::CopyBlocks).is_available_in_product());
    assert!(
        TerminalAction::InputContextMenuItem(InputContextMenuAction::ShowCommandSearch)
            .is_available_in_product()
    );
    assert!(
        !TerminalAction::ContextMenu(ContextMenuAction::OpenWorkflowModal)
            .is_available_in_product()
    );
    assert!(
        !TerminalAction::InputContextMenuItem(InputContextMenuAction::SaveAsWorkflow)
            .is_available_in_product()
    );
}
