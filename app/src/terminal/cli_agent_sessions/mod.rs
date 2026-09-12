use std::collections::HashMap;

use warp_core::cli_agent_protocol::CLIAgent;
use warpui::{Entity, EntityId, ModelContext, SingletonEntity};

#[derive(Debug, Clone)]
pub struct CLIAgentSession {
    pub agent: CLIAgent,
}

#[derive(Debug, Clone)]
pub enum CLIAgentSessionsModelEvent {
    Started { terminal_view_id: EntityId },
    Ended { terminal_view_id: EntityId },
}

impl CLIAgentSessionsModelEvent {
    pub fn terminal_view_id(&self) -> EntityId {
        match self {
            Self::Started { terminal_view_id } | Self::Ended { terminal_view_id } => {
                *terminal_view_id
            }
        }
    }
}

pub struct CLIAgentSessionsModel {
    sessions: HashMap<EntityId, CLIAgentSession>,
}

impl Entity for CLIAgentSessionsModel {
    type Event = CLIAgentSessionsModelEvent;
}

impl SingletonEntity for CLIAgentSessionsModel {}

impl CLIAgentSessionsModel {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn session(&self, terminal_view_id: EntityId) -> Option<&CLIAgentSession> {
        self.sessions.get(&terminal_view_id)
    }

    pub fn remove_session(&mut self, terminal_view_id: EntityId, ctx: &mut ModelContext<Self>) {
        if self.sessions.remove(&terminal_view_id).is_some() {
            ctx.emit(CLIAgentSessionsModelEvent::Ended { terminal_view_id });
        }
    }

    pub fn set_session(
        &mut self,
        terminal_view_id: EntityId,
        session: CLIAgentSession,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.sessions.insert(terminal_view_id, session).is_some() {
            ctx.emit(CLIAgentSessionsModelEvent::Ended { terminal_view_id });
        }
        ctx.emit(CLIAgentSessionsModelEvent::Started { terminal_view_id });
    }
}
