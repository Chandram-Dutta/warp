use std::collections::HashMap;

use itertools::Itertools;
use session_sharing_protocol::common::SessionId;
use warpui::{
    AppContext, Entity, EntityId, ModelContext, SingletonEntity, ViewHandle, WeakViewHandle,
};

use super::SharedSessionStatus;
use crate::terminal::TerminalView;

struct SharedSessionState {
    session_id: SessionId,
    view_handle: WeakViewHandle<TerminalView>,
}

/// A global model that tracks shared session metadata for sessions across all windows.
pub struct Manager {
    /// Sessions that were joined by this client.
    joined: HashMap<EntityId, SharedSessionState>,

    /// IDs of sessions that were joined by this client,
    /// but have since been stopped. This state is maintained so that the
    /// copy link button can still work for ended sessions.
    ended_session_ids: HashMap<EntityId, SessionId>,
}

impl Manager {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            joined: Default::default(),
            ended_session_ids: Default::default(),
        }
    }

    /// Returns true iff there are >= 1 active joined sessions.
    pub fn is_some_session_being_viewed(&self) -> bool {
        !self.joined.is_empty()
    }

    /// Returns the session id for the given terminal view.
    pub fn session_id(&self, terminal_view_id: &EntityId) -> Option<SessionId> {
        self.joined
            .get(terminal_view_id)
            .map(|state| state.session_id)
    }

    /// Returns the most recently ended session id for the given terminal view.
    pub fn ended_session_id(&self, terminal_view_id: &EntityId) -> Option<SessionId> {
        self.ended_session_ids.get(terminal_view_id).copied()
    }

    /// Returns the session id that may currently be exposed as a link for the given terminal view.
    /// Active session ids always take precedence. Ended ids are eligible only after the terminal
    /// has actually left its active or pending sharing state.
    pub fn session_id_for_link(
        &self,
        terminal_view_id: &EntityId,
        shared_session_status: &SharedSessionStatus,
    ) -> Option<SessionId> {
        self.session_id(terminal_view_id)
            .or_else(|| match shared_session_status {
                SharedSessionStatus::NotShared | SharedSessionStatus::FinishedViewer => {
                    self.ended_session_id(terminal_view_id)
                }
                SharedSessionStatus::ViewPending | SharedSessionStatus::ActiveViewer { .. } => None,
            })
    }

    /// Returns true iff the Manager has a session id that may currently be exposed as a link.
    pub fn has_session_link(
        &self,
        terminal_view_id: &EntityId,
        shared_session_status: &SharedSessionStatus,
    ) -> bool {
        self.session_id_for_link(terminal_view_id, shared_session_status)
            .is_some()
    }

    /// Returns the view handle to the joined terminal view, identified by `terminal_view_id`, if it's being viewed.
    pub fn joined_view_by_id(
        &self,
        terminal_view_id: &EntityId,
        ctx: &AppContext,
    ) -> Option<ViewHandle<TerminalView>> {
        let weak_handle = self
            .joined
            .get(terminal_view_id)
            .map(|state| state.view_handle.clone())?;

        let view_handle = weak_handle.upgrade(ctx);
        if view_handle.is_none() {
            log::warn!("Failed to upgrade a terminal view in the joined session manager");
        }

        view_handle
    }

    pub fn joined_view_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.joined.keys().cloned()
    }

    pub fn joined_share(
        &mut self,
        terminal_view: WeakViewHandle<TerminalView>,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let view_id = terminal_view.id();
        let state = SharedSessionState {
            session_id,
            view_handle: terminal_view,
        };
        self.joined.insert(view_id, state);
        ctx.emit(ManagerEvent::JoinedSession {
            session_id,
            view_id,
        });
    }

    pub fn left_share(&mut self, terminal_view_id: EntityId) {
        // Remove the shared session from the shared sessions map and persist the session id.
        if let Some(removed_session) = self.joined.remove(&terminal_view_id) {
            self.ended_session_ids
                .insert(terminal_view_id, removed_session.session_id);
        }
    }

    pub fn clear_joined(&mut self) {
        self.joined.clear();
    }

    pub fn rejoin_all_shared_sessions(&mut self, ctx: &mut ModelContext<Self>) {
        if self.is_some_session_being_viewed() {
            let view_ids = self.joined_view_ids().collect_vec();

            for view_id in view_ids {
                if let Some(terminal_view) = self.joined_view_by_id(&view_id, ctx) {
                    terminal_view.update(ctx, |view, ctx| {
                        view.rejoin_session_share(ctx);
                    });
                }
            }
        }
    }
}

pub enum ManagerEvent {
    /// A shared session has been successfully joined.
    JoinedSession {
        /// the session_id of the session that was joined.
        session_id: SessionId,
        /// The view_id of the terminal that joined the session.
        view_id: EntityId,
    },
}

impl Entity for Manager {
    type Event = ManagerEvent;
}

impl SingletonEntity for Manager {}
