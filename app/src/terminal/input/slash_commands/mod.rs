#[cfg(not(target_family = "wasm"))]
use warp_cli::agent::Harness;
use warpui::{AppContext, SingletonEntity};

use crate::ai::agent::conversation::AIConversationId;
#[cfg(not(target_family = "wasm"))]
use crate::ai::agent_conversations_model::AgentConversationsModel;
use crate::ai::blocklist::BlocklistAIHistoryModel;
#[cfg(not(target_family = "wasm"))]
use crate::search::slash_command_menu::static_commands::commands;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SlashCommandTrigger {
    Input { cmd_or_ctrl_enter: bool },
    Keybinding,
}

impl SlashCommandTrigger {
    pub fn input() -> Self {
        Self::Input {
            cmd_or_ctrl_enter: false,
        }
    }

    pub fn is_keybinding(&self) -> bool {
        matches!(self, Self::Keybinding)
    }
}

/// Returns true when the conversation with `conversation_id` is associated with an Oz
/// `AmbientAgentTask`. Callers deciding between `/fork` and `/continue-locally` should also
/// check the same `CLOUD_AGENT` context that gates `/continue-locally`.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn conversation_is_cloud_oz_for_slash_command(
    conversation_id: AIConversationId,
    ctx: &AppContext,
) -> bool {
    let history = BlocklistAIHistoryModel::as_ref(ctx);
    let Some(conversation) = history.conversation(&conversation_id) else {
        return false;
    };
    let Some(task_id) = conversation.task_id() else {
        return false;
    };

    let Some(task) = AgentConversationsModel::as_ref(ctx).get_task_data(&task_id) else {
        // Permissive: not yet fetched. Matches the data-source default so the command isn't
        // wrongly blocked while the task fetch is in flight.
        return true;
    };

    match task
        .agent_config_snapshot
        .as_ref()
        .and_then(|s| s.harness.as_ref())
    {
        Some(config) => config.harness_type == Harness::Oz,
        None => true,
    }
}

/// Tooltip and slash command name for the fork button, returned as a unit so
/// callers rendering the button and callers inserting the command always agree.
#[cfg(not(target_family = "wasm"))]
pub(crate) struct ForkButtonAction {
    pub tooltip: &'static str,
    pub command_name: &'static str,
}

/// Returns the tooltip and slash command for the fork button given an optional
/// conversation ID. Uses `/continue-locally` for Oz conversations when `/fork`
/// is unavailable in the current cloud-agent context, and `/fork` otherwise.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn fork_button_action(
    conversation_id: Option<AIConversationId>,
    is_cloud_agent_context: bool,
    ctx: &AppContext,
) -> ForkButtonAction {
    if is_cloud_agent_context
        && conversation_id.is_some_and(|id| conversation_is_cloud_oz_for_slash_command(id, ctx))
    {
        ForkButtonAction {
            tooltip: "Continue locally",
            command_name: commands::CONTINUE_LOCALLY.name,
        }
    } else {
        ForkButtonAction {
            tooltip: "Fork conversation",
            command_name: commands::FORK.name,
        }
    }
}
