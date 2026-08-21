use std::sync::Arc;

use parking_lot::FairMutex;
use warpui::{Entity, EntityId, ModelContext, ModelHandle};

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{PassiveCodeDiffEntry, PassiveSuggestionTrigger};
use crate::ai::blocklist::RequestFileEditsFormatKind;
use crate::ai::blocklist::controller::BlocklistAIController;
use crate::ai::blocklist::diff_types::FileDiff;
use crate::terminal::model::session::active_session::ActiveSession;
use crate::terminal::model::terminal_model::TerminalModel;
use crate::terminal::model_events::ModelEventDispatcher;
use crate::terminal::view::ambient_agent::AmbientAgentViewModel;

pub enum PassiveSuggestionsEvent {
    NewPromptSuggestion {
        prompt: String,
        label: Option<String>,
        request_duration_ms: u64,
        trigger: Option<PassiveSuggestionTrigger>,
        conversation_id: Option<AIConversationId>,
        server_request_token: Option<String>,
    },
    NewCodeDiffSuggestion {
        diffs: Vec<FileDiff>,
        edit_format_kind: RequestFileEditsFormatKind,
        title: Option<String>,
        original_edits: Vec<PassiveCodeDiffEntry>,
        conversation_id: Option<AIConversationId>,
        request_duration_ms: u64,
        trigger: PassiveSuggestionTrigger,
        server_request_token: Option<String>,
    },
}

pub struct PassiveSuggestionsModel {
    #[cfg_attr(not(test), allow(dead_code))]
    terminal_model: Arc<FairMutex<TerminalModel>>,
    #[cfg_attr(not(test), allow(dead_code))]
    ambient_agent_view_model: Option<ModelHandle<AmbientAgentViewModel>>,
}

impl PassiveSuggestionsModel {
    pub fn new(
        active_session: ModelHandle<ActiveSession>,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        ai_controller: ModelHandle<BlocklistAIController>,
        model_event_dispatcher: &ModelHandle<ModelEventDispatcher>,
        ambient_agent_view_model: Option<ModelHandle<AmbientAgentViewModel>>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let _ = (
            active_session,
            ai_controller,
            model_event_dispatcher,
            terminal_view_id,
            ctx,
        );
        Self {
            terminal_model,
            ambient_agent_view_model,
        }
    }

    pub fn abort_pending_requests(&mut self, ctx: &mut ModelContext<Self>) {
        let _ = (self, ctx);
    }

    #[cfg(test)]
    pub(crate) fn is_ambient_agent_session_for_test(&self, ctx: &ModelContext<Self>) -> bool {
        if self
            .ambient_agent_view_model
            .as_ref()
            .is_some_and(|model| model.as_ref(ctx).is_ambient_agent())
        {
            return true;
        }

        let terminal_model = self.terminal_model.lock();
        terminal_model.is_shared_ambient_agent_session()
            || terminal_model.is_conversation_transcript_viewer()
    }
}

impl Entity for PassiveSuggestionsModel {
    type Event = PassiveSuggestionsEvent;
}
