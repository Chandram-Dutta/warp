use warp_cli::agent::Harness;

use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::agent_conversations_model::AgentConversationEntry;
use crate::terminal::CLIAgent;
use crate::terminal::cli_agent::CLIAgentRuntimeExt as _;
use crate::ui_components::icon_with_status::IconWithStatusVariant;

pub(crate) fn agent_conversation_entry_icon_variant(
    entry: &AgentConversationEntry,
) -> IconWithStatusVariant {
    let status = entry.display.status.to_conversation_status();
    agent_icon_variant_for_run(
        entry.display.harness.unwrap_or(Harness::Oz),
        status,
        entry.is_cloud_agent_run(),
    )
}

/// Pure run-card logic: maps a [`Harness`], status, and ambient flag into an
/// [`IconWithStatusVariant`]. Falls back to the Oz variant for [`Harness::Oz`] and
/// [`Harness::Unknown`], the latter so a future-server harness this client doesn't
/// recognize doesn't render an unbranded gray circle.
pub(crate) fn agent_icon_variant_for_run(
    harness: Harness,
    status: ConversationStatus,
    is_ambient: bool,
) -> IconWithStatusVariant {
    let cli_agent =
        CLIAgent::from_harness(harness).filter(|agent| !matches!(agent, CLIAgent::Unknown));
    match cli_agent {
        Some(agent) => IconWithStatusVariant::CLIAgent {
            agent,
            status: Some(status),
            is_ambient,
        },
        None => IconWithStatusVariant::OzAgent {
            status: Some(status),
            is_ambient,
        },
    }
}

#[cfg(test)]
#[path = "agent_icon_tests.rs"]
mod tests;
