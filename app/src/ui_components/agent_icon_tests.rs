use chrono::Utc;
use warp_cli::agent::Harness;

use super::{agent_conversation_entry_icon_variant, agent_icon_variant_for_run};
use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
use crate::ai::agent_conversations_model::entry::{
    AgentConversationBackingData, AgentConversationCapabilities, AgentConversationDisplayData,
    AgentConversationIdentity, AgentConversationPrincipal, AgentConversationProvenance,
};
use crate::ai::agent_conversations_model::{
    AgentConversationEntry, AgentConversationEntryId, AgentRunDisplayStatus,
};
use crate::ai::ambient_agents::ExecutionLocation;
use crate::terminal::CLIAgent;
use crate::terminal::cli_agent::CLIAgentRuntimeExt as _;
use crate::ui_components::icon_with_status::IconWithStatusVariant;

/// Projection of the fields we care about for cross-surface equivalence.
/// [`IconWithStatusVariant`] itself can't derive `PartialEq` because `NeutralElement`
/// carries a `Box<dyn Element>`, so we extract the agent-variant fields here.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentIconFields {
    is_cli: bool,
    cli_agent: Option<CLIAgent>,
    status: Option<ConversationStatus>,
    is_ambient: bool,
}

impl AgentIconFields {
    fn from_variant(variant: &IconWithStatusVariant) -> Option<Self> {
        match variant {
            IconWithStatusVariant::OzAgent { status, is_ambient } => Some(Self {
                is_cli: false,
                cli_agent: None,
                status: status.clone(),
                is_ambient: *is_ambient,
            }),
            IconWithStatusVariant::CLIAgent {
                agent,
                status,
                is_ambient,
            } => Some(Self {
                is_cli: true,
                cli_agent: Some(*agent),
                status: status.clone(),
                is_ambient: *is_ambient,
            }),
            IconWithStatusVariant::Neutral { .. }
            | IconWithStatusVariant::NeutralElement { .. }
            | IconWithStatusVariant::CustomAvatar { .. } => None,
        }
    }
}

#[test]
fn cli_agent_from_harness_maps_known_harnesses() {
    assert_eq!(CLIAgent::from_harness(Harness::Oz), None);
    assert_eq!(
        CLIAgent::from_harness(Harness::Claude),
        Some(CLIAgent::Claude)
    );
    assert_eq!(
        CLIAgent::from_harness(Harness::Gemini),
        Some(CLIAgent::Gemini)
    );
    assert_eq!(
        CLIAgent::from_harness(Harness::OpenCode),
        Some(CLIAgent::OpenCode)
    );
}

#[test]
fn run_card_with_oz_or_unknown_harness_renders_as_oz() {
    // Oz harness explicitly: local Oz is the spec-defined fallback.
    let variant = agent_icon_variant_for_run(Harness::Oz, ConversationStatus::Success, true);
    let fields = AgentIconFields::from_variant(&variant).unwrap();
    assert!(!fields.is_cli);
    assert!(fields.is_ambient);

    // Unknown harness (e.g. server surfaced a future variant): also falls back to Oz so we
    // don't render an unbranded gray circle.
    let variant = agent_icon_variant_for_run(Harness::Unknown, ConversationStatus::Success, true);
    let fields = AgentIconFields::from_variant(&variant).unwrap();
    assert!(!fields.is_cli);
    assert!(fields.is_ambient);
}

#[test]
fn entry_icon_uses_harness_and_execution_location() {
    let conversation_id = AIConversationId::new();
    let entry = AgentConversationEntry {
        id: AgentConversationEntryId::Conversation(conversation_id),
        identity: AgentConversationIdentity {
            local_conversation_id: Some(conversation_id),
            ambient_agent_task_id: None,
            server_conversation_token: None,
            session_id: None,
        },
        provenance: AgentConversationProvenance::CloudSyncedConversation,
        execution_location: None,
        display: AgentConversationDisplayData {
            title: "Codex conversation".to_string(),
            initial_query: None,
            created_at: Utc::now(),
            last_updated: Utc::now(),
            status: AgentRunDisplayStatus::ConversationSucceeded,
            creator: AgentConversationPrincipal::default(),
            executor: None,
            request_usage: None,
            run_time: None,
            session_status: None,
            source: None,
            working_directory: None,
            environment_id: None,
            harness: Some(Harness::Codex),
            artifacts: Vec::new(),
        },
        backing: AgentConversationBackingData {
            has_loaded_conversation: true,
            has_local_persisted_data: true,
            has_cloud_data: true,
            has_ambient_run: false,
        },
        capabilities: AgentConversationCapabilities {
            can_open: true,
            can_copy_link: false,
            can_share: false,
            can_delete: false,
            can_fork_locally: false,
            can_cancel: false,
        },
    };

    let variant = agent_conversation_entry_icon_variant(&entry);
    assert_eq!(
        AgentIconFields::from_variant(&variant).unwrap(),
        AgentIconFields {
            is_cli: true,
            cli_agent: Some(CLIAgent::Codex),
            status: Some(ConversationStatus::Success),
            is_ambient: false,
        }
    );
    assert!(!entry.is_cloud_agent_run());

    let mut task_backed_local = entry.clone();
    task_backed_local.provenance = AgentConversationProvenance::AmbientRun;
    task_backed_local.backing.has_ambient_run = true;
    task_backed_local.identity.ambient_agent_task_id =
        Some("00000000-0000-0000-0000-000000000001".parse().unwrap());
    assert!(task_backed_local.is_cloud_agent_run());

    task_backed_local.execution_location = Some(ExecutionLocation::Local);
    let variant = agent_conversation_entry_icon_variant(&task_backed_local);
    assert!(!AgentIconFields::from_variant(&variant).unwrap().is_ambient);
    assert!(!task_backed_local.is_cloud_agent_run());

    let mut remote_task = task_backed_local;
    remote_task.execution_location = Some(ExecutionLocation::Remote);
    let variant = agent_conversation_entry_icon_variant(&remote_task);
    assert!(AgentIconFields::from_variant(&variant).unwrap().is_ambient);
    assert!(remote_task.is_cloud_agent_run());
}
