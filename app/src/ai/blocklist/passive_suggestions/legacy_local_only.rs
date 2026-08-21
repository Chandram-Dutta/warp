use std::sync::Arc;

use parking_lot::FairMutex;
use warpui::{Entity, EntityId, ModelContext, ModelHandle};

use crate::ai::agent::AIAgentExchangeId;
use crate::ai::blocklist::controller::BlocklistAIController;
use crate::ai::blocklist::controller::response_stream::ResponseStreamId;
use crate::server::telemetry::PromptSuggestionFallbackReason;
use crate::terminal::model::block::BlockId;
use crate::terminal::model::session::active_session::ActiveSession;
use crate::terminal::model::terminal_model::TerminalModel;
use crate::terminal::model_events::ModelEventDispatcher;
use crate::terminal::view::AgentModePromptSuggestion;

#[derive(Clone, Debug)]
pub enum PassiveSuggestionsEvent {
    PromptSuggestionsGenerated {
        prompt_suggestion: AgentModePromptSuggestion,
        block_id: BlockId,
        command: String,
        request_duration_ms: u64,
    },
    PassiveCodeDiffRequestStarted {
        prompt_suggestion_id: String,
        code_exchange_id: Option<AIAgentExchangeId>,
        block_id: BlockId,
    },
    PassiveCodeDiffFailed {
        reason: PromptSuggestionFallbackReason,
    },
}

pub struct PassiveSuggestionsModel;

impl PassiveSuggestionsModel {
    pub fn new(
        active_session: ModelHandle<ActiveSession>,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        ai_controller: ModelHandle<BlocklistAIController>,
        model_event_dispatcher: &ModelHandle<ModelEventDispatcher>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let _ = (
            active_session,
            terminal_model,
            ai_controller,
            model_event_dispatcher,
            terminal_view_id,
            ctx,
        );
        Self
    }

    pub fn is_passive_code_diff_being_generated(&self) -> bool {
        false
    }

    pub fn abort_pending_requests(
        &mut self,
        ctx: &mut ModelContext<Self>,
    ) -> Vec<ResponseStreamId> {
        let _ = (self, ctx);
        Vec::new()
    }
}

impl Entity for PassiveSuggestionsModel {
    type Event = PassiveSuggestionsEvent;
}
