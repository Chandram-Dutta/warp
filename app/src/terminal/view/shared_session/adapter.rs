//! An adapter to make session-sharing work with the [`TerminalView`].

use chrono::{DateTime, Local};
use markdown_parser::FormattedTextFragment;
use session_sharing_protocol::common::{ParticipantId, ParticipantList, Role, SessionId};
use session_sharing_protocol::sharer::SessionSourceType;
use warp_core::features::FeatureFlag;
use warpui::{ModelHandle, ViewContext, ViewHandle};

use super::viewer::Viewer;
use crate::auth::UserUid;
use crate::banner::{Banner, BannerTextContent};
use crate::terminal::shared_session::presence_manager::PresenceManager;
use crate::terminal::view::{TerminalAction, TerminalView};
use crate::ui_components::icons::Icon;

/// An adapter to make session-sharing work with the [`TerminalView`].
pub struct Adapter {
    viewer: Viewer,
    presence_manager: ModelHandle<PresenceManager>,
    reconnecting_banner: ViewHandle<Banner<TerminalAction>>,
    is_reconnecting_banner_open: bool,
    session_id: SessionId,
    started_at: DateTime<Local>,
    source_type: SessionSourceType,
}

impl Adapter {
    fn new(
        viewer: Viewer,
        presence_manager: ModelHandle<PresenceManager>,
        session_id: SessionId,
        started_at: DateTime<Local>,
        source_type: SessionSourceType,
        ctx: &mut ViewContext<TerminalView>,
    ) -> Self {
        let reconnecting_banner = ctx.add_typed_action_view(|_| {
            Banner::new_without_close(BannerTextContent::formatted_text(vec![
                FormattedTextFragment::plain_text("Offline, trying to reconnect..."),
            ]))
            .with_icon(Icon::CloudOffline)
        });

        Self {
            presence_manager,
            viewer,
            reconnecting_banner,
            is_reconnecting_banner_open: false,
            session_id,
            started_at,
            source_type,
        }
    }

    pub fn new_for_viewer(
        viewer_id: ParticipantId,
        firebase_uid: UserUid,
        participant_list: Box<ParticipantList>,
        session_id: SessionId,
        started_at: DateTime<Local>,
        source_type: SessionSourceType,
        ctx: &mut ViewContext<TerminalView>,
    ) -> Self {
        let presence_manager = ctx.add_model(|ctx| {
            PresenceManager::new_for_viewer(viewer_id, firebase_uid, *participant_list, ctx)
        });
        let viewer = Viewer::new(ctx);
        Self::new(
            viewer,
            presence_manager,
            session_id,
            started_at,
            source_type,
            ctx,
        )
    }

    pub fn started_at(&self) -> &DateTime<Local> {
        &self.started_at
    }

    pub fn presence_manager(&self) -> &ModelHandle<PresenceManager> {
        &self.presence_manager
    }

    pub fn update_participant_role(
        &mut self,
        participant_id: &ParticipantId,
        role: Role,
        ctx: &mut ViewContext<TerminalView>,
    ) {
        if !FeatureFlag::SessionSharingAcls.is_enabled() {
            self.update_participant_role_internal(participant_id, role, ctx);
            return;
        }

        let presence_manager = self.presence_manager.as_ref(ctx);
        if let Some(firebase_uid) = presence_manager.viewer_firebase_uid(participant_id) {
            // Update the local state for all participants that have the same UID.
            let participant_ids: Vec<ParticipantId> = presence_manager
                .present_viewer_ids_for_uid(firebase_uid)
                .cloned()
                .collect();
            for participant_id in participant_ids {
                self.update_participant_role_internal(&participant_id, role, ctx);
            }
        } else {
            log::warn!(
                "Unable to find firebase uid for viewer {participant_id:?} when updating role"
            );
            self.update_participant_role_internal(participant_id, role, ctx);
        }
    }

    fn update_participant_role_internal(
        &mut self,
        participant_id: &ParticipantId,
        role: Role,
        ctx: &mut ViewContext<TerminalView>,
    ) {
        self.presence_manager()
            .update(ctx, |presence_manager, ctx| {
                presence_manager.update_participant_role(participant_id, role, ctx);
            });
    }

    pub fn viewer(&self) -> &Viewer {
        &self.viewer
    }

    pub(super) fn viewer_mut(&mut self) -> &mut Viewer {
        &mut self.viewer
    }

    pub fn on_reconnection_status_changed(
        &mut self,
        is_reconnecting: bool,
        ctx: &mut ViewContext<TerminalView>,
    ) {
        self.is_reconnecting_banner_open = is_reconnecting;

        self.presence_manager()
            .update(ctx, |presence_manager, ctx| {
                presence_manager.set_is_reconnecting(is_reconnecting, ctx);
            });

        self.viewer.set_is_reconnecting(is_reconnecting);
    }

    pub fn reconnecting_banner(&self) -> Option<&ViewHandle<Banner<TerminalAction>>> {
        self.is_reconnecting_banner_open
            .then_some(&self.reconnecting_banner)
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn source_type(&self) -> &SessionSourceType {
        &self.source_type
    }
}
