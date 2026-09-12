use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use ai::index::full_source_code_embedding::manager::{
    CodebaseIndexManager, CodebaseIndexManagerEvent,
};
use ai::project_context::model::{ProjectContextModel, ProjectContextModelEvent};
use ai::workspace::{WorkspaceMetadata, WorkspaceMetadataEvent};
use anyhow::Context;
use chrono::Utc;
use itertools::Itertools;
#[cfg(feature = "local_fs")]
use repo_metadata::RepoMetadataModel;
#[cfg(feature = "local_fs")]
use repo_metadata::repositories::{DetectedRepositories, DetectedRepositoriesEvent};
use warp_core::features::FeatureFlag;
use warp_errors::report_if_error;
#[cfg(feature = "local_fs")]
use warp_util::{local_or_remote_path::LocalOrRemotePath, standardized_path::StandardizedPath};
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use crate::ai::AIRequestUsageModel;
use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel};
#[cfg(feature = "local_fs")]
use crate::ai::codebase_auto_indexing::{
    CodebaseAutoIndexingSurface, auto_index_candidate_roots, should_auto_index_codebase,
};
use crate::ai::metadata_project_rules::read_project_rule_contents;
use crate::persistence::ModelEvent;
use crate::settings::CodeSettings;
use crate::terminal::TerminalView;
use crate::workspaces::user_workspaces::{UserWorkspaces, UserWorkspacesEvent};

pub struct Workspace {
    metadata: WorkspaceMetadata,
}

impl Workspace {
    /// Returns `true` if this workspace has been persisted to SQLite.
    ///
    /// A workspace created solely from available-server detection will have
    /// all metadata timestamps set to `None` and is considered non-persisted.
    fn is_persisted(&self) -> bool {
        let persisted = self.metadata.navigated_ts.is_some()
            || self.metadata.modified_ts.is_some()
            || self.metadata.queried_ts.is_some();

        persisted
    }
}

/// Manages code workspaces used by codebase indexing and project context.
pub struct PersistedWorkspace {
    workspaces: HashMap<PathBuf, Workspace>,
    model_event_sender: Option<SyncSender<ModelEvent>>,
}

#[derive(Debug, Clone)]
pub enum PersistedWorkspaceEvent {
    /// Emitted when the user explicitly adds a repo via a picker (e.g. the tab-config
    /// params modal's repo dropdown). Subscribers can use this to refresh their list.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    WorkspaceAdded { path: PathBuf },
}

impl Entity for PersistedWorkspace {
    type Event = PersistedWorkspaceEvent;
}

impl SingletonEntity for PersistedWorkspace {}

impl PersistedWorkspace {
    #[cfg(test)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            workspaces: HashMap::new(),
            model_event_sender: None,
        }
    }

    pub fn new(
        metadata: Vec<WorkspaceMetadata>,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let metadata: HashMap<PathBuf, Workspace> = metadata
            .into_iter()
            .map(|metadata| {
                let path = metadata.path.clone();
                (path, Workspace { metadata })
            })
            .collect();

        if FeatureFlag::FullSourceCodeEmbedding.is_enabled() {
            ctx.subscribe_to_model(&CodebaseIndexManager::handle(ctx), |me, _, event, ctx| {
                match event {
                    CodebaseIndexManagerEvent::IndexMetadataUpdated { root_path, event } => {
                        me.handle_index_metadata_event(root_path, *event);
                    }
                    CodebaseIndexManagerEvent::RemoveExpiredIndexMetadata { expired_metadata } => {
                        // TODO: Disable expired metadata removal once we have other consumers of the workspace metadata.
                        me.clean_up_expired_metadata(expired_metadata.clone(), ctx);
                    }
                    _ => {}
                }
            });

            // Subscribe to AI conversation events to trigger incremental sync
            ctx.subscribe_to_model(
                &BlocklistAIHistoryModel::handle(ctx),
                |me, _, event, ctx| {
                    if let BlocklistAIHistoryEvent::StartedNewConversation {
                        terminal_surface_id,
                        ..
                    } = event
                    {
                        #[cfg(feature = "local_fs")]
                        me.clean_up_deleted_indices(ctx);

                        me.trigger_incremental_sync_for_conversation(*terminal_surface_id, ctx);
                    }
                },
            );

            // Subscribe to changes in workspace settings.
            ctx.subscribe_to_model(
                &UserWorkspaces::handle(ctx),
                |me, _, user_workspaces_event, ctx| {
                    if let UserWorkspacesEvent::CodebaseContextEnablementChanged =
                        user_workspaces_event
                    {
                        me.on_settings_changed(ctx);
                    }
                },
            );

            // Subscribe to ProjectContextModel events to persist rule changes
            ctx.subscribe_to_model(&ProjectContextModel::handle(ctx), |me, _, event, _ctx| {
                if let ProjectContextModelEvent::KnownRulesChanged(delta) = event {
                    let mut events = vec![];

                    if !delta.discovered_rules.is_empty() {
                        events.push(ModelEvent::UpsertProjectRules {
                            project_rule_paths: delta.discovered_rules.clone(),
                        });
                    }

                    if !delta.deleted_rules.is_empty() {
                        events.push(ModelEvent::DeleteProjectRules {
                            path: delta.deleted_rules.clone(),
                        });
                    }

                    if !events.is_empty() {
                        me.save_to_db(events);
                    }
                }
            });
        }

        // Registered regardless of whether codebase indexing is enabled:
        // `index_repo` also drives project-rules (and, transitively, project
        // skills) discovery, which must work in modes that keep codebase
        // indexing off (e.g. the TUI front-end). The embedding half of
        // `index_repo` stays behind its own gates, and
        // `CodebaseIndexManager::index_directory` no-ops when indexing is
        // disabled.
        #[cfg(feature = "local_fs")]
        if !cfg!(any(
            test,
            feature = "fast_dev",
            feature = "integration_tests"
        )) {
            ctx.subscribe_to_model(&DetectedRepositories::handle(ctx), |me, _, event, ctx| {
                let DetectedRepositoriesEvent::DetectedGitRepo { repository, .. } = event;
                let repo_path = repository.as_ref(ctx).root_dir().to_local_path_lossy();

                me.index_repo(repo_path, ctx);
            });
        }

        Self {
            workspaces: metadata,
            model_event_sender,
        }
    }

    pub fn root_for_workspace<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.ancestors()
            .find(|&path| self.workspaces.contains_key(path))
    }

    fn on_settings_changed(&mut self, ctx: &mut ModelContext<Self>) {
        Self::maybe_enable_codebase_indexing(ctx);
    }

    pub fn on_user_changed(&self, ctx: &mut ModelContext<Self>) {
        Self::maybe_enable_codebase_indexing(ctx);
    }

    /// Enables or disables codebase indexing according to the setting.
    fn maybe_enable_codebase_indexing(ctx: &mut ModelContext<Self>) {
        CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            if !manager.is_indexing_enabled() {
                return;
            }
            let codebase_context_enabled =
                UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx);
            if codebase_context_enabled {
                Self::enable_codebase_indexing(manager, ctx);
            } else {
                manager.reset_codebase_indexing(ctx);
            }
        });
    }

    fn enable_codebase_indexing(
        manager: &mut CodebaseIndexManager,
        ctx: &mut ModelContext<CodebaseIndexManager>,
    ) {
        let request_model = AIRequestUsageModel::handle(ctx);
        let codebase_limits = request_model.as_ref(ctx).codebase_context_limits();
        manager.update_max_limits(
            codebase_limits.max_indices_allowed,
            codebase_limits.max_files_per_repo,
            codebase_limits.embedding_generation_batch_size,
            ctx,
        );

        #[cfg(feature = "local_fs")]
        if should_auto_index_codebase(CodebaseAutoIndexingSurface::Local, ctx) {
            let roots = all_working_directories(ctx).into_iter().filter_map(|dir| {
                DetectedRepositories::as_ref(ctx)
                    .get_root_for_path(&LocalOrRemotePath::Local(dir))
                    .and_then(|root| root.to_local_path().map(Path::to_path_buf))
            });
            for root in auto_index_candidate_roots(roots, |_| true) {
                manager.index_directory(root, ctx);
            }
        }
    }

    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn index_repo(&self, directory_path: PathBuf, ctx: &mut ModelContext<Self>) {
        ProjectContextModel::handle(ctx).update(ctx, |model, ctx| {
            let _ = model.index_and_store_rules(
                directory_path.clone(),
                read_project_rule_contents,
                ctx,
            );
        });
        if FeatureFlag::FullSourceCodeEmbedding.is_enabled()
            && UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx)
            && *CodeSettings::as_ref(ctx).auto_indexing_enabled
        {
            CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
                manager.index_directory(directory_path, ctx);
            });
        }
    }

    /// Explicitly registers a directory as a workspace, as if the user had navigated there.
    ///
    /// Creates or updates the entry with `navigated_ts = now`, persists to SQLite,
    /// starts full repo-metadata indexing before triggering project-rules and codebase-index
    /// scanning, and emits
    /// [`PersistedWorkspaceEvent::WorkspaceAdded`] so subscribers can refresh their UI.
    pub fn user_added_workspace(&mut self, path: PathBuf, ctx: &mut ModelContext<Self>) {
        let now = Utc::now();

        match self.workspaces.get_mut(&path) {
            Some(workspace) => {
                workspace.metadata.navigated_ts = Some(now);
            }
            None => {
                self.workspaces.insert(
                    path.clone(),
                    Workspace {
                        metadata: WorkspaceMetadata {
                            path: path.clone(),
                            navigated_ts: Some(now),
                            modified_ts: None,
                            queried_ts: None,
                        },
                    },
                );
            }
        }

        self.persist_metadata_for_index(&path);
        #[cfg(feature = "local_fs")]
        match StandardizedPath::from_local_canonicalized(&path) {
            Ok(path) => {
                if let Err(error) = RepoMetadataModel::handle(ctx).update(ctx, |model, ctx| {
                    model.index_local_directory_path(&path, ctx)
                }) {
                    log::warn!("Failed to start full repo metadata indexing for {path}: {error}");
                }
            }
            Err(error) => {
                log::warn!(
                    "Failed to canonicalize user-added workspace {} for full repo metadata indexing: {error}",
                    path.display()
                );
            }
        }
        self.index_repo(path.clone(), ctx);
        ctx.emit(PersistedWorkspaceEvent::WorkspaceAdded { path });
    }

    pub fn workspaces<'a>(&'a self) -> impl Iterator<Item = WorkspaceMetadata> + use<'a> {
        self.workspaces
            .values()
            .filter(|workspace| workspace.is_persisted())
            .map(|workspace| workspace.metadata.clone())
            .sorted_by(WorkspaceMetadata::most_recently_touched)
            .dedup_by(|a, b| a.path == b.path)
    }

    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    pub fn navigated_to_path(&mut self, directory: &PathBuf) {
        if let Some(workspace) = self.workspaces.get_mut(directory) {
            workspace.metadata.navigated_ts = Some(Utc::now());
            self.persist_metadata_for_index(directory);
        }
    }

    fn handle_index_metadata_event(&mut self, root_path: &PathBuf, event: WorkspaceMetadataEvent) {
        match event {
            WorkspaceMetadataEvent::Queried => {
                if let Some(workspace) = self.workspaces.get_mut(root_path) {
                    workspace.metadata.queried_ts = Some(Utc::now());
                }
                self.persist_metadata_for_index(root_path);
            }
            WorkspaceMetadataEvent::Modified => {
                if let Some(workspace) = self.workspaces.get_mut(root_path) {
                    workspace.metadata.modified_ts = Some(Utc::now());
                }
                self.persist_metadata_for_index(root_path);
            }
            WorkspaceMetadataEvent::Created => {
                let new_metadata = WorkspaceMetadata {
                    path: root_path.clone(),
                    navigated_ts: None,
                    // Count creation as a modification event.
                    modified_ts: Some(Utc::now()),
                    queried_ts: None,
                };

                if let Some(existing) = self.workspaces.get_mut(root_path) {
                    // Preserve existing language server settings when re-creating
                    // workspace metadata (e.g. after an expired index is cleaned up
                    // and the user navigates back to the same directory).
                    existing.metadata = new_metadata;
                } else {
                    self.workspaces.insert(
                        root_path.clone(),
                        Workspace {
                            metadata: new_metadata,
                        },
                    );
                }
                self.persist_metadata_for_index(root_path);
            }
        }
    }

    pub fn workspace_for_path(&self, root_path: &Path) -> Option<WorkspaceMetadata> {
        self.workspaces
            .get(root_path)
            .map(|workspace| workspace.metadata.clone())
    }

    fn persist_metadata_for_index(&self, path: &PathBuf) {
        log::info!("Saving workspace metadata for {path:?} to SQLite");

        if let Some(single_metadata) = self.workspace_for_path(path) {
            self.save_to_db(vec![ModelEvent::UpsertCodebaseIndexMetadata {
                index_metadata: Box::new(single_metadata),
            }]);
        }
    }

    /// Triggers an incremental sync for the codebase context when a new conversation starts.
    /// This ensures that the codebase index is up-to-date before the conversation begins.
    fn trigger_incremental_sync_for_conversation(
        &mut self,
        terminal_view_id: warpui::EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        if !UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx) {
            return;
        }

        // Get the current working directory for the terminal view that started the conversation
        // Collect window IDs first to avoid borrowing conflicts
        let window_ids: Vec<_> = ctx.window_ids().collect();

        for window_id in window_ids {
            let terminal_views = ctx.views_of_type::<TerminalView>(window_id);

            for terminal_view in terminal_views.into_iter().flatten() {
                let terminal_view_ref = terminal_view.as_ref(ctx);
                if terminal_view_ref.view_id() == terminal_view_id {
                    if terminal_view_ref.active_session_is_local(ctx) != Some(true) {
                        log::info!(
                            "Skipping local codebase incremental sync for non-local agent conversation"
                        );
                        return;
                    }

                    let pwd = terminal_view_ref.pwd();
                    if let Some(pwd) = pwd {
                        let directory_path = PathBuf::from(pwd);

                        // Trigger an incremental sync through the CodebaseIndexManager
                        CodebaseIndexManager::handle(ctx).update(ctx, |codebase_manager, ctx| {
                            if let Err(e) = codebase_manager
                                .trigger_incremental_sync_for_path(&directory_path, ctx)
                            {
                                log::warn!("Failed to trigger incremental sync {e}");
                            }
                        });
                    }
                    return; // Found the terminal view, exit both loops
                }
            }
        }
    }

    fn clean_up_expired_metadata(
        &self,
        indices_to_remove: Arc<Vec<PathBuf>>,
        _ctx: &mut ModelContext<Self>,
    ) {
        log::info!("Cleaning up index metadata from SQLite");

        let indices_to_remove = indices_to_remove.as_ref();
        self.save_to_db(indices_to_remove.iter().filter_map(|path| {
            let Some(ws) = self.workspaces.get(path) else {
                return Some(ModelEvent::DeleteCodebaseIndexMetadata {
                    repo_path: path.to_path_buf(),
                });
            };

            // Skip non-persisted workspaces — they have no DB row to delete.
            if !ws.is_persisted() {
                return None;
            }

            Some(ModelEvent::DeleteCodebaseIndexMetadata {
                repo_path: path.to_path_buf(),
            })
        }));
    }

    #[cfg(feature = "local_fs")]
    fn clean_up_deleted_indices(&self, ctx: &mut ModelContext<Self>) {
        CodebaseIndexManager::handle(ctx).update(ctx, |codebase_manager, ctx| {
            codebase_manager.clean_up_deleted_indices(ctx);
        });
    }

    fn save_to_db(&self, events: impl IntoIterator<Item = ModelEvent>) {
        let model_event_sender = self.model_event_sender.clone();
        if let Some(model_event_sender) = &model_event_sender {
            for event in events {
                report_if_error!(
                    model_event_sender
                        .send(event)
                        .with_context(|| "Unable to save codebase index metadata to sqlite")
                );
            }
        }
    }
}

#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
pub fn all_working_directories(app: &AppContext) -> HashSet<PathBuf> {
    let mut working_directories = HashSet::new();
    for window_id in app.window_ids() {
        for terminal_view in app
            .views_of_type::<TerminalView>(window_id)
            .into_iter()
            .flatten()
            .map(|handle| handle.as_ref(app))
        {
            let working_directory = terminal_view.pwd();
            if let Some(dir) = working_directory {
                working_directories.insert(dir.into());
            }
        }
    }
    working_directories
}
