use std::collections::HashSet;

use warp_core::context_flag::ContextFlag;
use warp_core::features::FeatureFlag;
use warpui::keymap::BindingId;
use warpui::{AppContext, Entity, ModelContext, ModelHandle};

use crate::search::QueryFilter;
use crate::search::action::CommandBindingDataSource;
use crate::search::binding_source::BindingSource;
use crate::search::command_palette::mixer::{CommandPaletteItemAction, ItemSummary};
use crate::search::command_palette::new_session::NewSessionDataSource;
use crate::search::command_palette::{CommandPaletteMixer, launch_config, navigation, tabs};
use crate::search::data_source::QueryResult;
use crate::session_management::SessionSource;

/// Store of all of the [`crate::search::DataSource`]s for the command palette.
pub struct DataSourceStore {
    actions_data_source: ModelHandle<CommandBindingDataSource>,
    sessions_data_source: ModelHandle<navigation::DataSource>,
    launch_config_data_source: ModelHandle<launch_config::DataSource>,
    new_session_data_source: Option<ModelHandle<NewSessionDataSource>>,
    tabs_data_source: Option<ModelHandle<tabs::DataSource>>,
}

impl DataSourceStore {
    pub fn new(
        binding_source: ModelHandle<BindingSource>,
        active_session_handle: ModelHandle<SessionSource>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let actions_data_source =
            ctx.add_model(|ctx| CommandBindingDataSource::new(binding_source.clone(), ctx));

        let sessions_data_source =
            ctx.add_model(|_| navigation::DataSource::new(active_session_handle));

        let launch_config_data_source = ctx.add_model(launch_config::DataSource::new);

        let new_session_data_source = (FeatureFlag::ShellSelector.is_enabled()
            && cfg!(feature = "local_tty"))
        .then_some(ctx.add_model(|ctx| NewSessionDataSource::new(binding_source, ctx)));

        Self {
            actions_data_source,
            sessions_data_source,
            launch_config_data_source,
            new_session_data_source,
            tabs_data_source: None,
        }
    }

    /// Resets the [`CommandPaletteMixer`] to the set of data sources that are relevant for the command palette.
    pub fn reset_search_mixer(
        &mut self,
        mixer: ModelHandle<CommandPaletteMixer>,
        ctx: &mut ModelContext<Self>,
    ) {
        mixer.update(ctx, |mixer, ctx| {
            mixer.reset(ctx);

            if ContextFlag::LaunchConfigurations.is_enabled() {
                mixer.add_sync_source(
                    self.launch_config_data_source.clone(),
                    HashSet::from([QueryFilter::LaunchConfigurations]),
                );
            }

            mixer.add_sync_source(
                self.sessions_data_source.clone(),
                HashSet::from([QueryFilter::Sessions]),
            );

            mixer.add_sync_source(
                self.actions_data_source.clone(),
                HashSet::from([QueryFilter::Actions]),
            );

            if let Some(new_session_data_source) = &self.new_session_data_source {
                mixer.add_sync_source(
                    new_session_data_source.clone(),
                    HashSet::from([QueryFilter::Actions]),
                );
            }

            ctx.notify();
        });
    }

    /// Resets the [`CommandPaletteMixer`] to the set of data sources relevant for the Ctrl+Tab
    /// palette, which shows tabs sorted by MRU order.
    pub fn reset_ctrl_tab_mixer(
        &mut self,
        mixer: ModelHandle<CommandPaletteMixer>,
        tabs: Vec<crate::session_management::TabNavigationData>,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.tabs_data_source.is_none() {
            self.tabs_data_source = Some(ctx.add_model(|_| tabs::DataSource::new()));
        }

        if let Some(tabs_data_source) = &self.tabs_data_source {
            tabs_data_source.update(ctx, |ds, _| ds.set_tabs(tabs));
            mixer.update(ctx, |mixer, ctx| {
                mixer.reset(ctx);
                mixer.add_sync_source(tabs_data_source.clone(), HashSet::from([QueryFilter::Tabs]));
                ctx.notify();
            });
        }
    }

    /// Restores the [`CommandPaletteMixer`] to the sessions-only source for Ctrl+Tab,
    /// undoing any previous `reset_ctrl_tab_mixer` call.
    pub fn restore_ctrl_tab_session_mixer(
        &self,
        mixer: ModelHandle<CommandPaletteMixer>,
        ctx: &mut ModelContext<Self>,
    ) {
        mixer.update(ctx, |mixer, ctx| {
            mixer.reset(ctx);
            mixer.add_sync_source(
                self.sessions_data_source.clone(),
                HashSet::from([QueryFilter::Sessions]),
            );
            ctx.notify();
        });
    }

    /// Returns a [`QueryResult`] from the data sources identified by the `summary`. `None` if none
    /// of the data sources contained an item with given summary.
    pub fn query_result_from_summary(
        &self,
        summary: &ItemSummary,
        app: &AppContext,
    ) -> Option<QueryResult<CommandPaletteItemAction>> {
        match summary {
            ItemSummary::Action { binding_id } => self
                .actions_data_source
                .as_ref(app)
                .query_result(*binding_id),
            ItemSummary::Session { pane_view_locator } => self
                .sessions_data_source
                .as_ref(app)
                .query_result(*pane_view_locator, app),
            ItemSummary::LaunchConfiguration => {
                // TODO(CLD-205): Launch configurations are not supported in the recent section of the
                // zero state yet.
                None
            }
            ItemSummary::NewSession { id } => self
                .new_session_data_source
                .as_ref()
                .and_then(|source| source.as_ref(app).query_result(id)),
            ItemSummary::NoOp => {
                // No-op action (used for non-interactable separator items that don't do anything on click).
                None
            }

            ItemSummary::Tab { .. } => {
                // Tabs are only shown in the ctrl_tab palette, not in recent commands.
                None
            }
        }
    }

    /// Returns a [`QueryResult`] for a binding with `binding_id`. `None` if no result was found
    /// with the given ID.
    pub fn query_result_for_binding_id(
        &self,
        binding_id: BindingId,
        app: &AppContext,
    ) -> Option<QueryResult<CommandPaletteItemAction>> {
        self.query_result_from_summary(&ItemSummary::Action { binding_id }, app)
    }
}

impl Entity for DataSourceStore {
    type Event = ();
}
