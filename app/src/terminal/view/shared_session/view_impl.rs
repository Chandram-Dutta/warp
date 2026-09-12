//! [`TerminalView`]-specific implementation for shared sessions.

use chrono::{DateTime, Local};
use itertools::Itertools;
use session_sharing_protocol::common::{
    ParticipantId, ParticipantList, ParticipantPresenceUpdate, Role, RoleRequestId,
    RoleRequestResponse, SessionId, WindowSize,
};
use session_sharing_protocol::sharer::SessionSourceType;
use session_sharing_protocol::viewer::RoleUpdatedReason;
use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warp_core::semantic_selection::SemanticSelection;
use warp_core::ui::appearance::Appearance;
use warp_errors::report_error;
use warpui::clipboard::ClipboardContent;
use warpui::{AppContext, Element, ModelHandle, SingletonEntity, ViewContext};

use super::adapter::Adapter;
use super::cloud_conversation_continuation::{
    CloudConversationContinuationUiState, TombstoneCta, conversation_failed_before_task_creation,
    resolve_cloud_conversation_continuation_ui_state,
};
use super::viewer::Viewer;
use super::{ConversationEndedTombstoneEvent, ConversationEndedTombstoneView};
use crate::ai::agent_conversations_model::AgentConversationsModel;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::auth::{AuthStateProvider, UserUid};
use crate::context_chips::ContextChipKind;
use crate::drive::sharing::ShareableObject;
use crate::editor::{InteractionState, ReplicaId};
use crate::menu::Event as MenuEvent;
use crate::server::telemetry::SharingDialogSource;
use crate::settings::InputModeSettings;
use crate::terminal::TerminalModel;
use crate::terminal::session_settings::SessionSettings;
use crate::terminal::shared_session::manager::Manager;
use crate::terminal::shared_session::presence_manager::PresenceManager;
use crate::terminal::shared_session::{
    COPY_LINK_TEXT, SharedSessionActionSource, SharedSessionScrollbackType, SharedSessionSource,
    SharedSessionStatus, join_link,
};
use crate::terminal::view::{
    Event, InlineBannerItem, InlineBannerType, RichContentInsertionPosition, SharedSessionBanners,
    SizeUpdateBuilder, TerminalAction, TerminalView,
};
use crate::view_components::{DismissibleToast, ToastFlavor};
use crate::{TelemetryEvent, send_telemetry_from_ctx};

impl TerminalView {
    pub fn shared_session_viewer(&self) -> Option<&Viewer> {
        self.shared_session.as_ref().map(Adapter::viewer)
    }

    pub fn shared_session_viewer_mut(&mut self) -> Option<&mut Viewer> {
        self.shared_session.as_mut().map(Adapter::viewer_mut)
    }

    // TODO (suraj): do we actually need to expose this? It's a bit of a smell.
    pub fn shared_session_presence_manager(&self) -> Option<ModelHandle<PresenceManager>> {
        Some(self.shared_session.as_ref()?.presence_manager().clone())
    }

    pub fn shared_session_id(&self) -> Option<&SessionId> {
        Some(self.shared_session.as_ref()?.session_id())
    }

    fn shared_session_source_type(&self) -> Option<&SessionSourceType> {
        Some(self.shared_session.as_ref()?.source_type())
    }

    pub(crate) fn is_shared_session_for_ambient_agent(&self) -> bool {
        matches!(
            self.shared_session_source_type(),
            Some(SessionSourceType::AmbientAgent { .. })
        )
    }

    pub(in crate::terminal::view) fn cloud_conversation_continuation_ui_state(
        &self,
        ctx: &AppContext,
    ) -> Option<CloudConversationContinuationUiState> {
        let task_id = {
            let model = self.model.lock();
            if !FeatureFlag::CloudModeSetupV2.is_enabled()
                || !FeatureFlag::HandoffCloudCloud.is_enabled()
                || model.is_receiving_agent_conversation_replay()
            {
                return None;
            }

            let is_cloud_conversation_selection = model.is_shared_ambient_agent_session()
                || model.is_conversation_transcript_viewer()
                || self
                    .ambient_agent_view_model
                    .as_ref()
                    .is_some_and(|model| model.as_ref(ctx).is_ambient_agent());
            if !is_cloud_conversation_selection {
                return None;
            }

            self.ambient_agent_task_id_for_details_panel_from_model(&model, ctx)
        };
        let Some(task_id) = task_id else {
            return conversation_failed_before_task_creation(
                self.id(),
                BlocklistAIHistoryModel::as_ref(ctx),
            )
            .then_some(CloudConversationContinuationUiState::Tombstone { cta: None });
        };
        match resolve_cloud_conversation_continuation_ui_state(self.id(), task_id, ctx) {
            Ok(state) => Some(state),
            Err(error) => error
                .should_fallback_to_tombstone()
                .then_some(CloudConversationContinuationUiState::Tombstone { cta: None }),
        }
    }

    pub(in crate::terminal::view) fn blocks_cloud_followups_for_ambient_agent_session_from_model(
        &self,
        model: &TerminalModel,
        ctx: &AppContext,
    ) -> bool {
        if self
            .ambient_agent_view_model
            .as_ref()
            .is_some_and(|model| model.as_ref(ctx).blocks_cloud_followups())
        {
            return true;
        }

        let Some(task_id) = self.ambient_agent_task_id_for_details_panel_from_model(model, ctx)
        else {
            return false;
        };

        AgentConversationsModel::as_ref(ctx)
            .get_task_data(&task_id)
            .is_some_and(|task| task.blocks_cloud_followups())
    }

    pub(crate) fn owned_ambient_agent_task_id(
        &self,
        ctx: &AppContext,
    ) -> Option<AmbientAgentTaskId> {
        let task_id = self.ambient_agent_task_id_for_details_panel(ctx)?;

        AgentConversationsModel::as_ref(ctx)
            .get_task_data(&task_id)
            .is_some_and(|task| {
                let Some(current_user_uid) = AuthStateProvider::as_ref(ctx)
                    .get()
                    .user_id()
                    .map(|uid| uid.as_string())
                else {
                    return false;
                };
                task.creator
                    .is_some_and(|creator| creator.uid == current_user_uid)
            })
            .then_some(task_id)
    }

    pub(in crate::terminal::view) fn enable_cloud_followup_input(
        &mut self,
        task_id: AmbientAgentTaskId,
        ctx: &mut ViewContext<Self>,
    ) {
        self.pending_cloud_followup_task_id = Some(task_id);
        self.input.update(ctx, |input, ctx| {
            input.reset_after_cloud_followup_submission(ctx);
            input.set_input_mode_agent(true, ctx);
            input.editor().update(ctx, |editor, ctx| {
                editor.set_interaction_state(InteractionState::Editable, ctx);
            });
        });
        self.update_pane_configuration(ctx);
        ctx.notify();
    }

    /// Clears the finished/read-only state a pane accumulates when its shared session ends, so it
    /// can host a live session again. Idempotent.
    ///
    /// A failed run whose environment is retained for debugging leaves the pane read-only with an
    /// ended-conversation tombstone even though its session is still reachable; reattaching must
    /// produce a writable terminal rather than that ended-run view.
    pub(crate) fn prepare_for_live_session_reattach(&mut self, ctx: &mut ViewContext<Self>) {
        self.remove_conversation_ended_tombstone(ctx);

        {
            let mut model = self.model.lock();
            if model.shared_session_status().is_finished_viewer() {
                // The join performed by the caller moves this to `ViewPending` and then
                // `ActiveViewer`; clearing it here just lifts `TerminalModel::is_read_only`.
                model.set_shared_session_status(SharedSessionStatus::NotShared);
            }
        }

        self.input().update(ctx, |input, ctx| {
            input.editor().update(ctx, |editor, ctx| {
                editor.set_interaction_state(InteractionState::Editable, ctx);
            });
        });
        self.update_pane_configuration(ctx);
        ctx.notify();
    }

    fn enable_cloud_followup_input_after_conversation_end(
        &mut self,
        task_id: AmbientAgentTaskId,
        ctx: &mut ViewContext<Self>,
    ) {
        self.remove_conversation_ended_tombstone(ctx);

        {
            let mut model = self.model.lock();
            if model.shared_session_status().is_finished_viewer() {
                model.set_shared_session_status(SharedSessionStatus::NotShared);
            }
        }

        self.enable_cloud_followup_input(task_id, ctx);
    }

    /// Enables the established continuation input after pane hydration has
    /// already resolved explicit conversation Edit access.
    pub(crate) fn enable_completed_cloud_continuation(
        &mut self,
        task_id: AmbientAgentTaskId,
        ctx: &mut ViewContext<Self>,
    ) {
        self.enable_cloud_followup_input_after_conversation_end(task_id, ctx);
    }

    pub(super) fn handle_viewer_role_change_menu_event(
        &mut self,
        event: &MenuEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        if let MenuEvent::Close { .. } = event {
            self.close_viewer_role_change_menu(ctx);
        }
    }

    fn close_viewer_role_change_menu(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(viewer) = self.shared_session_viewer_mut() {
            viewer.close_role_change_menu();
            ctx.notify();
        }
        self.update_shared_session_pane_header(ctx);
    }

    pub fn update_session_link_permissions(
        &mut self,
        role: Option<Role>,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.emit(Event::UpdateSessionLinkPermissions { role });
    }

    pub fn update_session_team_permissions(
        &mut self,
        role: Option<Role>,
        team_uid: String,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.emit(Event::UpdateSessionTeamPermissions { role, team_uid });
    }

    pub fn update_role(
        &mut self,
        participant_id: ParticipantId,
        role: Role,
        ctx: &mut ViewContext<Self>,
    ) {
        self.on_participant_role_changed(&participant_id, role, ctx);
        ctx.emit(Event::UpdateRole {
            participant_id,
            role,
        });
    }

    pub fn update_role_for_user(
        &mut self,
        user_uid: UserUid,
        role: Role,
        ctx: &mut ViewContext<Self>,
    ) {
        // If the user is present in the session, we should update our local state.
        if let Some(participant_id) = self.shared_session.as_ref().and_then(|session| {
            session
                .presence_manager()
                .as_ref(ctx)
                .present_viewer_id_for_uid(user_uid)
                .cloned()
        }) {
            self.on_participant_role_changed(&participant_id, role, ctx);
        }

        ctx.emit(Event::UpdateUserRole { user_uid, role });
    }

    pub fn update_role_for_pending_user(
        &mut self,
        email: String,
        role: Role,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.emit(Event::UpdatePendingUserRole { email, role });
    }

    pub fn add_guests(&mut self, emails: Vec<String>, role: Role, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::AddGuests { emails, role })
    }

    pub fn remove_guest(&mut self, user_uid: UserUid, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::RemoveGuest { user_uid })
    }

    pub fn remove_pending_guest(&mut self, email: String, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::RemovePendingGuest { email })
    }

    fn update_shared_session_pane_header(&mut self, ctx: &mut ViewContext<Self>) {
        let self_handle = ctx.handle();
        let Some(shared_session) = &self.shared_session else {
            return;
        };
        self.pane_configuration.update(ctx, |pane_config, ctx| {
            pane_config.set_shareable_object(
                Some(ShareableObject::Session {
                    handle: self_handle,
                    session_id: *shared_session.session_id(),
                    started_at: *shared_session.started_at(),
                }),
                ctx,
            );
            ctx.notify();
        });
    }

    /// Focuses the view by telling the parent view to focus this session.
    /// For example, in the common case, the parent pane group would consume
    /// this event and focus the pane that this session lives in.
    pub fn focus_shared_session(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.windows().show_window_and_focus_app(ctx.window_id());
        ctx.emit(Event::FocusSession);
    }

    pub(crate) fn notify_shared_session_link_changed(&mut self, ctx: &mut ViewContext<Self>) {
        self.pane_configuration.update(ctx, |pane_config, ctx| {
            pane_config.notify_shared_session_link_changed(ctx);
        });
    }

    // TODO: why do we need to pass through input replica ID as a separate argument?
    // It should be in `participant_list`.
    #[allow(clippy::too_many_arguments)]
    pub fn on_session_share_joined(
        &mut self,
        viewer_id: ParticipantId,
        firebase_uid: UserUid,
        input_replica_id: ReplicaId,
        participant_list: Box<ParticipantList>,
        session_id: SessionId,
        source_type: SessionSourceType,
        ctx: &mut ViewContext<Self>,
    ) {
        let started_at = Local::now();
        let self_handle = ctx.handle();
        let adapter = Adapter::new_for_viewer(
            viewer_id.clone(),
            firebase_uid,
            participant_list,
            session_id,
            started_at,
            source_type.clone(),
            ctx,
        );
        let presence_manager = adapter.presence_manager().clone();
        let role = presence_manager.as_ref(ctx).role();
        self.shared_session = Some(adapter);

        self.insert_shared_session_started_banner(
            SharedSessionScrollbackType::All,
            false,
            started_at,
            ctx,
        );

        self.input.update(ctx, |input, ctx| {
            input.on_session_share_joined(input_replica_id, presence_manager, ctx);
        });

        // Mark this terminal as a viewer for chips and AI context menu once on join
        self.input().update(ctx, |input, ctx| {
            input
                .prompt_render_helper
                .prompt_view()
                .update(ctx, |prompt_display, ctx| {
                    prompt_display.update_shared_session_viewer_status(true, ctx);
                });
        });

        // If viewer joined as an executor, make sure the view state is updated.
        if let Some(role) = role {
            self.on_self_role_updated(role, ctx);
        }

        self.pane_configuration.update(ctx, |pane_config, ctx| {
            pane_config.refresh_pane_header_overflow_menu_items(ctx);
            pane_config.set_shareable_object(
                Some(ShareableObject::Session {
                    handle: self_handle,
                    session_id,
                    started_at,
                }),
                ctx,
            );
            pane_config.notify_header_content_changed(ctx);
        });

        // When we join a shared session, we get a snapshot of the sharer's chip states,
        // including the working directory chip. We can use this chip value to set the terminal title
        // with the correct pwd on-join (even if there is no active block yet to populate the TerminalView's pwd).
        if let Some(pwd) = self
            .current_prompt
            .as_ref(ctx)
            .latest_chip_value(&ContextChipKind::WorkingDirectory, ctx)
        {
            self.terminal_title = pwd.to_string();
        }

        // Update the pane title, which will show either the conversation title/status
        // if there's an active conversation, or fall back to the terminal_title (pwd).
        self.update_pane_configuration(ctx);

        self.update_shared_session_pane_header(ctx);

        send_telemetry_from_ctx!(
            TelemetryEvent::JoinedSharedSession {
                session_id,
                source_type,
            },
            ctx
        );
    }

    pub fn rejoin_session_share(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::RejoinCurrentSession)
    }

    /// Clear the presence manager and handle any UI necessary on shared session end.
    /// Applies to both sharer and viewer when the session sharing ends.
    pub fn on_session_share_ended(&mut self, ctx: &mut ViewContext<Self>) {
        let viewed_ambient_task_id = self.ambient_agent_task_id_for_details_panel(ctx);
        let handoff_continuation_state = self.cloud_conversation_continuation_ui_state(ctx);
        let should_insert_legacy_tombstone = {
            let model = self.model.lock();
            !FeatureFlag::CloudModeSetupV2.is_enabled()
                && model.is_shared_ambient_agent_session()
                && self.conversation_ended_tombstone_view_id.is_none()
                && !model.is_receiving_agent_conversation_replay()
        };
        if let Some(state) = handoff_continuation_state {
            match state {
                CloudConversationContinuationUiState::Tombstone { cta } => {
                    self.insert_conversation_ended_tombstone_with_cta(cta, ctx);
                }
                CloudConversationContinuationUiState::FollowupInput => {
                    self.remove_conversation_ended_tombstone(ctx);
                }
            }
        } else if should_insert_legacy_tombstone {
            self.insert_conversation_ended_tombstone_with_cta(None, ctx);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.active_viewer_driven_size.is_some() && !self.is_shared_session_for_ambient_agent() {
            self.restore_pty_to_sharer_size(ctx);
        }

        // For ambient agent tasks, preserve the shareable object so the share dialog remains visible
        let is_ambient_agent = self.is_ambient_agent_session(ctx);
        let shareable_object_to_keep = if is_ambient_agent {
            self.shared_session
                .as_ref()
                .map(|session| ShareableObject::Session {
                    handle: ctx.handle(),
                    session_id: *session.session_id(),
                    started_at: *session.started_at(),
                })
        } else {
            None
        };

        self.shared_session = None;
        self.insert_shared_session_ended_banner(ctx);
        self.on_shared_session_reconnection_status_changed(false, ctx);

        self.input().update(ctx, |input, ctx| {
            input.editor().update(ctx, |editor, ctx| {
                editor.unregister_all_remote_peers(ctx);
            });
        });

        if self.pending_cloud_followup_task_id.is_none() {
            if matches!(
                handoff_continuation_state,
                Some(CloudConversationContinuationUiState::FollowupInput)
            ) {
                if let Some(task_id) = viewed_ambient_task_id {
                    self.enable_cloud_followup_input(task_id, ctx);
                }
            } else if self.model.lock().shared_session_status().is_viewer() {
                // When the session is ended, the input should be uneditable iff this is a viewer.
                self.input().update(ctx, |input, ctx| {
                    input.editor().update(ctx, |editor, ctx| {
                        editor.set_interaction_state(InteractionState::Selectable, ctx);
                    });
                });
            }
        }

        self.pane_configuration.update(ctx, |pane_config, ctx| {
            pane_config.refresh_pane_header_overflow_menu_items(ctx);
            pane_config.set_shareable_object(shareable_object_to_keep, ctx);
            pane_config.notify_header_content_changed(ctx);
            ctx.notify();
        });
    }

    pub fn on_ambient_agent_execution_ended(&mut self, ctx: &mut ViewContext<Self>) {
        self.handle_non_running_ambient_agent_task(ctx);
    }

    fn handle_non_running_ambient_agent_task(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(task_id) = self.ambient_agent_task_id_for_details_panel(ctx) {
            AgentConversationsModel::handle(ctx).update(ctx, |model, ctx| {
                model.mark_task_execution_ended(task_id, ctx);
            });
        }
        let has_live_shared_session = {
            let status = self.model.lock().shared_session_status().clone();
            status.is_active_viewer()
        };
        if has_live_shared_session {
            return;
        }
        let has_pending_cloud_followup = self.pending_cloud_followup_task_id.is_some();
        if !FeatureFlag::CloudModeSetupV2.is_enabled() || has_pending_cloud_followup {
            return;
        }
        if !FeatureFlag::HandoffCloudCloud.is_enabled() {
            self.insert_conversation_ended_tombstone_with_cta(None, ctx);
            return;
        }
        let Some(state) = self.cloud_conversation_continuation_ui_state(ctx) else {
            return;
        };
        match state {
            CloudConversationContinuationUiState::Tombstone { cta } => {
                self.insert_conversation_ended_tombstone_with_cta(cta, ctx);
            }
            CloudConversationContinuationUiState::FollowupInput => {
                if let Some(task_id) = self.ambient_agent_task_id_for_details_panel(ctx) {
                    self.enable_cloud_followup_input_after_conversation_end(task_id, ctx);
                }
            }
        }
    }

    fn start_cloud_followup_from_tombstone(
        &mut self,
        task_id: crate::ai::ambient_agents::AmbientAgentTaskId,
        ctx: &mut ViewContext<Self>,
    ) {
        if !FeatureFlag::HandoffCloudCloud.is_enabled() {
            return;
        }

        let Some(ambient_agent_view_model) = self.ambient_agent_view_model.as_ref() else {
            self.show_error_toast("Couldn't continue this cloud task.".to_string(), ctx);
            return;
        };

        if ambient_agent_view_model.as_ref(ctx).task_id() != Some(task_id) {
            self.show_error_toast("Couldn't continue this cloud task.".to_string(), ctx);
            return;
        }
        self.enable_cloud_followup_input_after_conversation_end(task_id, ctx);
        self.focus_input_box(ctx);
        ctx.notify();
    }

    pub fn get_shared_session_presence_selection(
        &self,
        ctx: &AppContext,
    ) -> session_sharing_protocol::common::Selection {
        let model_lock = self.model.lock();
        let input_mode = *InputModeSettings::as_ref(ctx).input_mode.value();
        let semantic_selection = SemanticSelection::as_ref(ctx);

        // First check if we have any selected blocks.
        let selected_block_ids = self
            .selected_blocks
            .to_block_ids(model_lock.block_list())
            .map(|id| id.to_string().into())
            .collect_vec();
        if !selected_block_ids.is_empty() {
            return session_sharing_protocol::common::Selection::Blocks {
                block_ids: selected_block_ids,
            };
        }

        // Then check if we have selected text in the alt screen or block list.
        if model_lock.is_alt_screen_active() {
            if let Some(selection_range) =
                model_lock.alt_screen().selection_range(semantic_selection)
            {
                return session_sharing_protocol::common::Selection::AltScreenText {
                    start: (*selection_range.start()).into(),
                    end: (*selection_range.end()).into(),
                    is_reversed: selection_range.is_reversed(),
                };
            }
        } else if let Some((start, end, is_reversed)) = model_lock
            .block_list()
            .text_selection_range(semantic_selection, input_mode.is_inverted_blocklist())
        {
            let Some(start) = start.to_session_sharing_block_point(model_lock.block_list()) else {
                report_error!("Failed convert start of selection range to BlockPoint");
                return session_sharing_protocol::common::Selection::None;
            };
            let Some(end) = end.to_session_sharing_block_point(model_lock.block_list()) else {
                report_error!("Failed convert end of selection range to BlockPoint");
                return session_sharing_protocol::common::Selection::None;
            };
            return session_sharing_protocol::common::Selection::BlockText {
                start,
                end,
                is_reversed,
            };
        }
        session_sharing_protocol::common::Selection::None
    }

    pub fn open_shared_session_viewer_role_menu(&mut self, ctx: &mut ViewContext<Self>) {
        let status = self.model.lock().shared_session_status().clone();
        let SharedSessionStatus::ActiveViewer { role } = status else {
            return;
        };

        if let Some(viewer) = self.shared_session_viewer_mut() {
            viewer.open_role_change_menu(role, ctx);
        }

        self.update_shared_session_pane_header(ctx);
    }

    pub fn request_shared_session_role(&mut self, role: Role, ctx: &mut ViewContext<Self>) {
        if let Some(old_role) = self
            .shared_session_presence_manager()
            .as_ref()
            .and_then(|pm| pm.as_ref(ctx).role())
        {
            // Ensure viewer is requesting a role different to their existing one
            if old_role == role {
                return;
            }
        };

        ctx.emit(Event::RequestSharedSessionRole(role));
    }

    pub fn open_shared_session_on_desktop(
        &mut self,
        source: SharedSessionActionSource,
        ctx: &mut ViewContext<Self>,
    ) {
        #[cfg(target_family = "wasm")]
        {
            let shared_session_status = self.model.lock().shared_session_status().clone();
            let manager = Manager::as_ref(ctx);
            let Some(session_id) =
                manager.session_id_for_link(&ctx.view_id(), &shared_session_status)
            else {
                return;
            };
            if let Ok(url) = url::Url::parse(&join_link(&session_id)) {
                crate::uri::web_intent_parser::open_url_on_desktop(&url);
            }
        }

        send_telemetry_from_ctx!(TelemetryEvent::WebSessionOpenedOnDesktop { source }, ctx);
    }

    pub fn cancel_shared_session_role_request(
        &mut self,
        role_request_id: RoleRequestId,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.emit(Event::CancelRoleRequest(role_request_id));
    }

    /// Updates view state when our own role was changed.
    fn on_self_role_updated(&mut self, role: Role, ctx: &mut ViewContext<Self>) {
        // Update shared session status only if we are an active viewer.
        // This avoids a race condition if a viewer receives a role change
        // before catching up, by ensuring the view is still pending.
        if self.model.lock().shared_session_status().is_active_viewer() {
            // If not an active viewer now, role and status will be updated
            // in the call `process_ordered_terminal_event`.
            self.model
                .lock()
                .set_shared_session_status(SharedSessionStatus::ActiveViewer { role });
        }

        // Enable/disable the editor based on the new role
        self.input().update(ctx, |input, ctx| {
            input.editor().update(ctx, |editor, ctx| {
                let role = &role;
                editor.set_interaction_state(role.into(), ctx);
            });
        });
    }

    /// Called when we (as a viewer) receive a response to our own role request.
    pub fn on_shared_session_role_request_response(
        &mut self,
        role_request_response: RoleRequestResponse,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(shared_session) = self.shared_session.as_mut()
            && let RoleRequestResponse::Approved { new_role } = role_request_response
        {
            let self_id = shared_session.presence_manager().as_ref(ctx).id();
            shared_session.update_participant_role(&self_id, new_role, ctx);
            self.on_self_role_updated(new_role, ctx);
        }

        self.update_shared_session_pane_header(ctx);
    }

    // TODO: consider refactoring this so that we don't have to repeat this
    // logic in TerminalView and Workspace (when starting a share).
    pub fn copy_shared_session_link(
        &mut self,
        source: SharedSessionActionSource,
        ctx: &mut ViewContext<Self>,
    ) {
        let view_id = ctx.view_id();
        let shared_session_status = self.model.lock().shared_session_status().clone();
        let session_id_opt = {
            let manager = Manager::as_ref(ctx);
            manager.session_id_for_link(&view_id, &shared_session_status)
        };
        let Some(session_id) = session_id_opt else {
            let window_id = ctx.window_id();
            crate::workspace::ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                let toast = DismissibleToast::error("Sharing link not yet available".to_string());
                toast_stack.add_ephemeral_toast(toast, window_id, ctx);
            });
            return;
        };

        ctx.clipboard()
            .write(ClipboardContent::plain_text(join_link(&session_id)));

        let window_id = ctx.window_id();
        crate::workspace::ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
            let toast = DismissibleToast::default(COPY_LINK_TEXT.to_string());
            toast_stack.add_ephemeral_toast(toast, window_id, ctx);
        });

        send_telemetry_from_ctx!(TelemetryEvent::CopiedSharedSessionLink { source }, ctx);
    }

    fn insert_shared_session_started_banner(
        &mut self,
        scrollback_type: SharedSessionScrollbackType,
        is_remote_control: bool,
        started_at: DateTime<Local>,
        ctx: &mut ViewContext<Self>,
    ) {
        let banner_id = self.inline_banners_state.next_banner_id();

        let mut model = self.model.lock();

        // TODO: technically the first block index could change between the time we insert
        // the banner and the time we actually compute the scrollback.
        let block_index = scrollback_type.first_block_index(&model);

        // Remove any existing banners if any.
        if let SharedSessionBanners::LastShared {
            started_banner_id,
            ended_banner_id,
            ..
        } = self.inline_banners_state.shared_session_banner_state
        {
            model
                .block_list_mut()
                .remove_inline_banner(started_banner_id);
            model.block_list_mut().remove_inline_banner(ended_banner_id);
        }

        self.inline_banners_state.shared_session_banner_state = SharedSessionBanners::ActiveShare {
            started_banner_id: banner_id,
            started_at,
            is_remote_control,
        };

        model.block_list_mut().insert_inline_banner_before_block(
            block_index,
            InlineBannerItem::new(banner_id, InlineBannerType::SharedSessionStart),
            None,
        );

        ctx.notify();
    }

    pub fn on_participant_presence_updated(
        &mut self,
        update: &ParticipantPresenceUpdate,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(presence_manager) = &self.shared_session_presence_manager() else {
            return;
        };
        presence_manager.update(ctx, |manager, ctx| {
            manager.update_participant_presence(update.to_owned(), ctx);
        });
        ctx.notify();
    }

    /// Only show toast if role is new and reason is valid.
    pub fn maybe_show_role_changed_toast(
        &mut self,
        participant_id: &ParticipantId,
        reason: RoleUpdatedReason,
        new_role: Role,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(presence_manager) = self.shared_session_presence_manager() else {
            return;
        };
        let is_self_role_updated = participant_id == &presence_manager.as_ref(ctx).id();
        let is_new_role_reader = match presence_manager.as_ref(ctx).role() {
            Some(old_role) => old_role.can_execute() && matches!(new_role, Role::Reader),
            None => false,
        };

        if is_self_role_updated
            && is_new_role_reader
            && matches!(reason, RoleUpdatedReason::InactivityLimitReached)
        {
            self.show_persistent_toast(
                "Editing permissions were revoked because the sharer is idle".to_owned(),
                ToastFlavor::Error,
                ctx,
            );
        }
    }

    // Called by both sharer and viewer when a participant's role has changed.
    pub fn on_participant_role_changed(
        &mut self,
        participant_id: &ParticipantId,
        new_role: Role,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(shared_session) = self.shared_session.as_mut() {
            shared_session.update_participant_role(participant_id, new_role, ctx);

            let is_self = {
                let presence_manager = shared_session.presence_manager().as_ref(ctx);
                if FeatureFlag::SessionSharingAcls.is_enabled() {
                    // If the participant has the same UID as us, then our ACL
                    // changed too and we need to update our state.
                    let self_uid = presence_manager.firebase_uid();
                    let participant_uid = presence_manager.viewer_firebase_uid(participant_id);
                    participant_uid.is_some_and(|uid| uid == self_uid)
                } else {
                    *participant_id == presence_manager.id()
                }
            };
            if is_self {
                self.on_self_role_updated(new_role, ctx);
            }
        }
        self.update_shared_session_pane_header(ctx);
    }

    pub fn on_self_role_maybe_changed(
        &mut self,
        participant_list: &ParticipantList,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(shared_session) = self.shared_session.as_ref() else {
            return;
        };
        let presence_manager = shared_session.presence_manager().as_ref(ctx);
        let self_id = presence_manager.id();
        let Some(existing_role) = presence_manager.role() else {
            return;
        };

        let Some(new_role) = participant_list
            .present_viewers
            .iter()
            .find(|v| v.info.id == self_id)
            .map(|v| v.max_acl)
        else {
            log::warn!("Could not find new role for viewer {self_id:?} in participant list");
            return;
        };

        if existing_role != new_role {
            self.on_self_role_updated(new_role, ctx);
        }
    }

    pub fn insert_shared_session_ended_banner(&mut self, ctx: &mut ViewContext<Self>) {
        let banner_id = self.inline_banners_state.next_banner_id();
        let banner = InlineBannerItem::new(banner_id, InlineBannerType::SharedSessionEnd);

        if let SharedSessionBanners::ActiveShare {
            started_banner_id,
            started_at,
            is_remote_control,
        } = self.inline_banners_state.shared_session_banner_state
        {
            self.inline_banners_state.shared_session_banner_state =
                SharedSessionBanners::LastShared {
                    started_banner_id,
                    started_at,
                    is_remote_control,
                    ended_at: Local::now(),
                    ended_banner_id: banner_id,
                };
        }

        let mut model = self.model.lock();
        if model.shared_session_status().is_active_viewer() {
            // For viewers, the banner goes after the long running block so no content appears after the banner.
            model
                .block_list_mut()
                .append_inline_banner_after_long_running(banner);
        } else {
            // For sharers, it goes before the long running block so the banner doesn't end up pinned at the bottom while the block above changes.
            model.block_list_mut().append_inline_banner(banner);
        }

        ctx.notify();
    }

    pub(crate) fn insert_conversation_ended_tombstone_with_cta(
        &mut self,
        tombstone_cta: Option<TombstoneCta>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.conversation_ended_tombstone_view_id.is_some() {
            self.remove_conversation_ended_tombstone(ctx);
        }
        let task_id = self.ambient_agent_task_id_for_details_panel(ctx);
        let terminal_view_id = self.id();

        let tombstone_view_handle = ctx.add_typed_action_view(|ctx| {
            ConversationEndedTombstoneView::new(ctx, terminal_view_id, task_id, tombstone_cta)
        });
        ctx.subscribe_to_view(&tombstone_view_handle, |me, _, event, ctx| match event {
            ConversationEndedTombstoneEvent::ContinueInCloud { task_id } => {
                me.start_cloud_followup_from_tombstone(*task_id, ctx);
            }
        });
        let tombstone_view_id = tombstone_view_handle.id();
        let insertion_position = RichContentInsertionPosition::Append {
            insert_below_long_running_block: true,
        };
        self.insert_rich_content(None, tombstone_view_handle, None, insertion_position, ctx);
        self.conversation_ended_tombstone_view_id = Some(tombstone_view_id);
    }

    pub(crate) fn insert_conversation_ended_tombstone_with_resolved_cta(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        if !FeatureFlag::HandoffCloudCloud.is_enabled() {
            self.insert_conversation_ended_tombstone_with_cta(None, ctx);
            return;
        }

        match self.cloud_conversation_continuation_ui_state(ctx) {
            Some(CloudConversationContinuationUiState::Tombstone { cta }) => {
                self.insert_conversation_ended_tombstone_with_cta(cta, ctx);
            }
            Some(CloudConversationContinuationUiState::FollowupInput) => {
                if let Some(task_id) = self.ambient_agent_task_id_for_details_panel(ctx) {
                    self.enable_cloud_followup_input_after_conversation_end(task_id, ctx);
                } else {
                    self.insert_conversation_ended_tombstone_with_cta(None, ctx);
                }
            }
            None => {
                self.insert_conversation_ended_tombstone_with_cta(None, ctx);
            }
        }
    }

    pub(in crate::terminal::view) fn remove_conversation_ended_tombstone(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(view_id) = self.conversation_ended_tombstone_view_id.take() else {
            return;
        };
        self.model
            .lock()
            .block_list_mut()
            .remove_rich_content(view_id);
        self.rich_content_views.retain(|rc| rc.view_id() != view_id);
        ctx.notify();
    }

    /// Updates the reconnection banner and input interaction state.
    pub fn on_shared_session_reconnection_status_changed(
        &mut self,
        is_reconnecting: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        if is_reconnecting && !self.model.lock().shared_session_status().is_viewer() {
            log::warn!(
                "Tried to open shared session reconnecting banner for a session that isn't shared"
            );
            return;
        }

        if let Some(shared_session) = self.shared_session.as_mut() {
            shared_session.on_reconnection_status_changed(is_reconnecting, ctx);
        }

        // Input is disabled for an offline executor and re-enabled when back online.
        if self.model.lock().shared_session_status().is_executor() {
            let interaction_state = if is_reconnecting {
                InteractionState::Selectable
            } else {
                InteractionState::Editable
            };
            self.input().update(ctx, |input, ctx| {
                input.editor().update(ctx, |editor, ctx| {
                    editor.set_interaction_state(interaction_state, ctx);
                });
            });
        }

        self.update_shared_session_pane_header(ctx);
        ctx.notify();
    }

    /// Resizes the terminal from when the sharer updates size.
    pub fn resize_from_sharer_update(
        &mut self,
        new_sharer_size: WindowSize,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(viewer) = self.shared_session_viewer_mut() {
            viewer.sharer_size = Some(new_sharer_size);

            let size_update = SizeUpdateBuilder::for_shared_session_update(
                *self.size_info,
                new_sharer_size.num_rows,
                new_sharer_size.num_cols,
            )
            .build(self, ctx);
            self.resize_internal(size_update, ctx);
        }
    }

    /// Returns true if viewer-driven sizing should be active.
    /// For cloud agent sessions (AmbientAgent), the same-user identity check is skipped.
    /// Otherwise, conditions: exactly 1 viewer, and that viewer is the same user as the sharer.
    pub(crate) fn is_viewer_driven_sizing_eligible(
        &self,
        is_sharer: bool,
        ctx: &ViewContext<Self>,
    ) -> bool {
        let skip_uid_check = self.is_shared_session_for_ambient_agent();
        self.shared_session_presence_manager()
            .map(|manager| {
                let manager = manager.as_ref(ctx);
                if is_sharer {
                    manager
                        .single_distinct_present_viewer_uid()
                        .is_some_and(|viewer_uid| {
                            skip_uid_check || viewer_uid == manager.firebase_uid().as_str()
                        })
                } else {
                    // No other distinct user should be viewing.
                    // Stale copies of our own connection share our UID.
                    let no_other_user = manager.get_present_viewers().all(|v| {
                        v.info.profile_data.firebase_uid == manager.firebase_uid().as_string()
                    });
                    no_other_user
                        && (skip_uid_check
                            || manager.get_sharer().is_some_and(|s| {
                                s.info.profile_data.firebase_uid
                                    == manager.firebase_uid().as_string()
                            }))
                }
            })
            .unwrap_or(false)
    }

    /// Restores the PTY to the sharer's own terminal size by refreshing
    /// through the normal resize pipeline.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn restore_pty_to_sharer_size(&mut self, ctx: &mut ViewContext<Self>) {
        self.active_viewer_driven_size = None;
        self.refresh_size(ctx);
    }

    /// Forces a fresh viewer-size report to the sharer by clearing the dedup cache and
    /// refreshing size. No-op when not an active viewer or when viewer-driven sizing is
    /// not eligible. Used when a new process (e.g. the harness CLI starting for a non-oz
    /// Cloud Mode run) needs the sharer to resize its PTY so the new process picks up
    /// correct terminal dimensions at startup.
    pub(in crate::terminal::view) fn force_report_viewer_terminal_size(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(viewer) = self.shared_session_viewer_mut() {
            viewer.last_reported_natural_size = None;
        }
        self.refresh_size(ctx);
    }

    /// Resizes the sharer's terminal to match the viewer's reported size,
    /// going through the normal view/model/PTY resize pipeline.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn resize_from_viewer_report(
        &mut self,
        viewer_size: WindowSize,
        ctx: &mut ViewContext<Self>,
    ) {
        self.active_viewer_driven_size = Some((viewer_size.num_rows, viewer_size.num_cols));
        let size_update = SizeUpdateBuilder::for_viewer_size_report(
            *self.size_info,
            viewer_size.num_rows,
            viewer_size.num_cols,
        )
        .build(self, ctx);
        self.resize_internal(size_update, ctx);
    }
}

#[cfg(test)]
#[path = "view_impl_tests.rs"]
mod tests;
