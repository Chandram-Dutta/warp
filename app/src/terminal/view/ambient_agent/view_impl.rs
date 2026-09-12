//! [`TerminalView`]-specific implementation for ambient agent functionality.

use warp_cli::agent::Harness;
use warp_core::features::FeatureFlag;
use warp_core::send_telemetry_from_ctx;
use warp_terminal::model::BlockId;
use warpui::{AppContext, EntityId, ModelHandle, SingletonEntity, ViewContext};

use super::{AmbientAgentViewModel, AmbientAgentViewModelEvent};
use crate::ai::AIRequestUsageModel;
use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
use crate::ai::agent::{RenderableAIError, display_user_query_with_mode};
use crate::ai::ambient_agents::telemetry::{CloudAgentTelemetryEvent, CloudModeEntryPoint};
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;
use crate::terminal::CLIAgent;
use crate::terminal::cli_agent::CLIAgentRuntimeExt as _;
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::terminal::view::rich_content::{RichContentInsertionPosition, RichContentMetadata};
use crate::terminal::view::{Event as TerminalViewEvent, TerminalView};
use crate::workspace::view::cloud_agent_capacity_modal::CloudAgentCapacityModalVariant;
use crate::workspaces::user_workspaces::UserWorkspaces;

const CHILD_AGENT_GITHUB_AUTH_REQUIRED_BLOCKED_ACTION: &str =
    "GitHub authentication required before starting the child agent.";

impl TerminalView {
    fn active_ambient_agent_conversation_id(&self, ctx: &AppContext) -> Option<AIConversationId> {
        self.agent_view_controller
            .as_ref(ctx)
            .agent_view_state()
            .active_conversation_id()
    }

    fn active_ambient_agent_conversation_is_child(&self, ctx: &AppContext) -> bool {
        let Some(conversation_id) = self.active_ambient_agent_conversation_id(ctx) else {
            return false;
        };

        BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&conversation_id)
            .is_some_and(|conversation| conversation.is_child_agent_conversation())
    }

    fn update_active_ambient_agent_conversation_status(
        &self,
        status: ConversationStatus,
        error: Option<RenderableAIError>,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(conversation_id) = self.active_ambient_agent_conversation_id(ctx) else {
            return;
        };

        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
            history_model.update_conversation_status_with_error(
                self.id(),
                conversation_id,
                status,
                error,
                ctx,
            );
        });
    }

    pub(in crate::terminal::view) fn show_out_of_credits_modal(&self, ctx: &mut ViewContext<Self>) {
        let is_on_paid_plan = UserWorkspaces::as_ref(ctx)
            .current_workspace()
            .is_some_and(|workspace| workspace.billing_metadata.is_user_on_paid_plan());

        if is_on_paid_plan {
            ctx.emit(crate::terminal::view::Event::ShowCloudAgentCapacityModal {
                variant: CloudAgentCapacityModalVariant::OutOfCredits,
            });
        } else {
            AIRequestUsageModel::handle(ctx).update(ctx, |model, ctx| {
                model.refresh_request_usage_async(ctx);
            });
        }
    }

    /// Handles ambient agent view model events.
    pub(in crate::terminal::view) fn handle_ambient_agent_event(
        &mut self,
        event: &AmbientAgentViewModelEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(ambient_agent_view_model) = self.ambient_agent_view_model.clone() else {
            return;
        };

        match event {
            AmbientAgentViewModelEvent::EnteredSetupState => {
                // Re-render to show the setup view.
                self.update_pane_configuration(ctx);
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::EnteredComposingState => {
                // Update pane configuration to show cloud indicator.
                self.update_pane_configuration(ctx);
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
            }
            AmbientAgentViewModelEvent::DispatchedAgent => {
                // Pane chrome (e.g. cloud indicator, task id) must update on viewer surfaces
                // too, so this runs above the viewer short-circuit below.
                self.update_pane_configuration(ctx);
                // Only the spawner's view handles `DispatchedAgent`. Viewer surfaces (shared
                // ambient agent session or transcript viewer) have no submitted prompt to render
                // and should not insert cloud-mode rich content here.
                let is_viewer = self.is_shared_ambient_agent_session()
                    || self.model.lock().is_conversation_transcript_viewer();
                if is_viewer {
                    ctx.notify();
                    return;
                }
                // Re-render to show loading state.
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::FollowupDispatched => {
                if FeatureFlag::CloudModeSetupV2.is_enabled() {
                    ambient_agent_view_model.update(ctx, |model, ctx| {
                        model.start_new_setup_command_group(ctx);
                    });
                }
                self.update_active_ambient_agent_conversation_status(
                    ConversationStatus::InProgress,
                    None,
                    ctx,
                );
                ctx.notify();
            }
            AmbientAgentViewModelEvent::SessionReady { .. }
            | AmbientAgentViewModelEvent::ExecutionSessionReady { .. } => {
                if matches!(
                    event,
                    AmbientAgentViewModelEvent::ExecutionSessionReady { .. }
                ) {
                    self.pending_cloud_followup_task_id = None;
                    self.remove_conversation_ended_tombstone(ctx);
                }
                // Re-render to hide the loading screen now that the session is ready.
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::EnvironmentSelected => {}
            AmbientAgentViewModelEvent::ProgressUpdated => {
                // Update pane header to reflect any changes (e.g., task_id being set)
                self.update_pane_configuration(ctx);
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::Failed { error_message } => {
                self.pending_cloud_followup_task_id = None;
                self.update_active_ambient_agent_conversation_status(
                    ConversationStatus::Error,
                    Some(RenderableAIError::other(error_message.clone(), false)),
                    ctx,
                );

                if FeatureFlag::CloudModeSetupV2.is_enabled() {
                    self.insert_conversation_ended_tombstone_with_resolved_cta(ctx);
                }

                // Re-render to show the error state.
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::ShowCloudAgentCapacityModal => {
                if FeatureFlag::CloudMode.is_enabled()
                    && ambient_agent_view_model.as_ref(ctx).is_ambient_agent()
                    && !self.model.lock().is_shared_ambient_agent_session()
                {
                    ctx.emit(crate::terminal::view::Event::ShowCloudAgentCapacityModal {
                        variant: CloudAgentCapacityModalVariant::ConcurrentLimit,
                    });
                }

                ctx.notify();
            }
            AmbientAgentViewModelEvent::ShowAICreditModal => {
                if FeatureFlag::CloudMode.is_enabled()
                    && ambient_agent_view_model.as_ref(ctx).is_ambient_agent()
                    && !self.model.lock().is_shared_ambient_agent_session()
                {
                    self.show_out_of_credits_modal(ctx);
                }

                ctx.notify();
            }
            AmbientAgentViewModelEvent::NeedsGithubAuth => {
                self.pending_cloud_followup_task_id = None;
                if self.active_ambient_agent_conversation_is_child(ctx) {
                    self.update_active_ambient_agent_conversation_status(
                        ConversationStatus::Blocked {
                            blocked_action: CHILD_AGENT_GITHUB_AUTH_REQUIRED_BLOCKED_ACTION
                                .to_string(),
                        },
                        None,
                        ctx,
                    );
                }
                // Re-render to show the GitHub auth required state in the footer.
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::Cancelled => {
                self.pending_cloud_followup_task_id = None;
                self.update_active_ambient_agent_conversation_status(
                    ConversationStatus::Cancelled,
                    None,
                    ctx,
                );
                // Re-render to show the cancelled state in the footer.
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::HarnessSelected => {
                self.update_pane_configuration(ctx);
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::ViewerHarnessResolved => {
                // Once we know which harness we're using from the server, try and enter the agent
                // view if we haven't already.
                self.sync_agent_view_for_shared_third_party_viewer(ctx);
                self.update_pane_configuration(ctx);
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::HostSelected => {}
            AmbientAgentViewModelEvent::HarnessModelSelected => {}
            AmbientAgentViewModelEvent::HarnessCommandStarted { block_id } => {
                // Stop classifying the harness block as an environment setup command, mirroring
                // the Oz path in the `AppendedExchange` handler.
                let conversation_id = self
                    .agent_view_controller
                    .as_ref(ctx)
                    .agent_view_state()
                    .active_conversation_id();
                {
                    let mut model = self.model.lock();
                    if model
                        .block_list()
                        .is_executing_oz_environment_startup_commands()
                    {
                        model
                            .block_list_mut()
                            .finish_oz_environment_startup_commands_at_block(
                                block_id,
                                conversation_id,
                            );
                    }
                }
                // Collapse the setup-commands summary, matching the oz first-exchange behavior.
                ambient_agent_view_model.update(ctx, |model, ctx| {
                    let group_id = model.setup_command_state().current_group_id();
                    model.finish_setup_command_group(group_id, ctx);
                    model.set_setup_command_visibility(false, ctx);
                });

                // Hide the command for the CLI agent block.
                if FeatureFlag::HarnessSessionHeader.is_enabled() {
                    let cli_agent = ambient_agent_view_model
                        .as_ref(ctx)
                        .selected_third_party_cli_agent();
                    let block_index = {
                        let mut model = self.model.lock();
                        if let Some(block) = model.block_list_mut().mut_block_from_id(block_id) {
                            block.set_should_hide_command_grid(true);
                        }
                        model.block_list().block_index_for_id(block_id)
                    };
                    if let Some(block_index) = block_index {
                        let header_view = ctx.add_typed_action_view(|_| {
                            super::HarnessSessionHeader::new(block_id.clone(), cli_agent)
                        });
                        ctx.subscribe_to_view(&header_view, |me, _, event, _| {
                            let super::HarnessSessionHeaderEvent::ToggleCommandGridVisibility(
                                block_id,
                            ) = event;
                            let mut model = me.model.lock();
                            if let Some(block) = model.block_list_mut().mut_block_from_id(block_id)
                            {
                                let hidden = block.should_hide_command_grid();
                                block.set_should_hide_command_grid(!hidden);
                            }
                        });
                        self.insert_rich_content(
                            None,
                            header_view,
                            Some(RichContentMetadata::HarnessSessionHeader),
                            RichContentInsertionPosition::BeforeBlockIndex(block_index),
                            ctx,
                        );
                    }
                }

                // Force a fresh viewer size report to the sharer so the harness CLI (e.g.
                // the claude TUI) starts at our terminal's actual dimensions instead of
                // whatever the sandbox PTY was sized to during setup.
                self.force_report_viewer_terminal_size(ctx);
                ctx.emit(TerminalViewEvent::TerminalViewStateChanged);
                ctx.notify();
            }
            AmbientAgentViewModelEvent::UpdatedSetupCommandVisibility
            | AmbientAgentViewModelEvent::AuthSecretSelected
            | AmbientAgentViewModelEvent::RunLifecycleChanged => (),
        }
    }

    /// Returns whether this view is a live shared-session viewer for a non-Oz cloud run.
    fn is_third_party_cloud_agent_viewer(&self, ctx: &AppContext) -> bool {
        // The ambient model's harness resolves asynchronously after join, when we fetch the task.
        // Until then, use the synced CLI-agent session (which we get on shared session join, if the
        // CLI agent is currently active) as the live third-party harness signal.
        let has_ambient_third_party_harness = self
            .ambient_agent_view_model
            .as_ref()
            .is_some_and(|model| model.as_ref(ctx).is_third_party_harness());
        let has_cli_agent_session = FeatureFlag::AgentHarness.is_enabled() && {
            CLIAgentSessionsModel::as_ref(ctx)
                .session(self.view_id)
                .is_some()
        };
        let is_shared_ambient_agent_session = self.is_shared_ambient_agent_session();

        (has_ambient_third_party_harness || has_cli_agent_session)
            && is_shared_ambient_agent_session
    }

    /// Syncs agent view for a live shared-session viewer of a non-oz cloud run, so every
    /// viewer lands in the same agent-view chrome regardless of which entry point opened the
    /// conversation.
    ///
    /// Transcript viewer entry is handled directly in `load_data_into_transcript_viewer` so
    /// the snapshot block exists before we retag — we intentionally do not trigger that path
    /// here.
    pub(crate) fn sync_agent_view_for_shared_third_party_viewer(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) -> Option<AIConversationId> {
        if !self.is_third_party_cloud_agent_viewer(ctx) {
            return None;
        }
        if let Some(conversation_id) = self
            .agent_view_controller
            .as_ref(ctx)
            .agent_view_state()
            .active_conversation_id()
        {
            return Some(conversation_id);
        }
        self.enter_agent_view_for_new_conversation(
            None,
            AgentViewEntryOrigin::ThirdPartyCloudAgent,
            ctx,
        );

        let vehicle_conversation_id = self
            .agent_view_controller
            .as_ref(ctx)
            .agent_view_state()
            .active_conversation_id()?;

        // Retag existing non-setup blocks so the harness content passes the agent view filter.
        self.model
            .lock()
            .block_list_mut()
            .attach_non_startup_blocks_to_conversation(vehicle_conversation_id);

        // Retag rich content inserted in terminal mode (setup-commands summary, tombstone, …)
        // so it stays visible under the vehicle conversation. Rich content with
        // `agent_view_conversation_id == None` is hidden in full-screen agent view by
        // `RichContentItem::should_hide_for_agent_view_state`.
        let ids_to_retag: Vec<EntityId> = self
            .rich_content_views
            .iter()
            .filter(|rc| rc.agent_view_conversation_id().is_none())
            .map(|rc| rc.view_id())
            .collect();
        for view_id in ids_to_retag {
            self.set_rich_content_agent_view_conversation_id(view_id, vehicle_conversation_id);
        }

        Some(vehicle_conversation_id)
    }
}
