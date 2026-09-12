//! Command history in session order, oldest first and deduplicated by the history model.

use chrono::Local;
use ordered_float::OrderedFloat;
use warpui::{AppContext, Entity, ModelHandle, SingletonEntity};

use crate::search::SyncDataSource;
use crate::search::data_source::{Query, QueryFilter, QueryResult};
use crate::search::mixer::DataSourceRunErrorWrapper;
use crate::terminal::history::{History, LinkedWorkflowData};
use crate::terminal::input::inline_history::search_item::InlineHistoryItem;
use crate::terminal::input::inline_menu::{
    InlineMenuAction, InlineMenuClickBehavior, InlineMenuType,
};
use crate::terminal::model::session::active_session::ActiveSession;

#[derive(Clone, Debug)]
pub struct AcceptHistoryItem {
    pub command: String,
    pub linked_workflow_data: Option<LinkedWorkflowData>,
}

impl InlineMenuAction for AcceptHistoryItem {
    const MENU_TYPE: InlineMenuType = InlineMenuType::InlineHistoryMenu;

    fn click_behavior(&self) -> InlineMenuClickBehavior {
        InlineMenuClickBehavior::SelectOnClick
    }
}

pub struct InlineHistoryMenuDataSource {
    active_session: ModelHandle<ActiveSession>,
}

impl InlineHistoryMenuDataSource {
    pub fn new(active_session: ModelHandle<ActiveSession>) -> Self {
        Self { active_session }
    }
}

impl SyncDataSource for InlineHistoryMenuDataSource {
    type Action = AcceptHistoryItem;

    fn run_query(
        &self,
        query: &Query,
        app: &AppContext,
    ) -> Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper> {
        if !query.filters.is_empty() && !query.filters.contains(&QueryFilter::Commands) {
            return Ok(Vec::new());
        }
        let trimmed_query = query.text.trim();
        let session_id = self.active_session.as_ref(app).session(app).map(|s| s.id());
        let history = History::handle(app).as_ref(app);
        Ok(history
            .up_arrow_suggestions(session_id, app)
            .into_iter()
            .filter_map(|suggestion| {
                let command = suggestion.normalized_text().to_owned();
                let entry = suggestion.entry;
                if !command.starts_with(trimmed_query) {
                    return None;
                }
                Some(
                    InlineHistoryItem::command(
                        command,
                        entry.linked_workflow_data(),
                        entry.start_ts.unwrap_or_else(Local::now),
                    )
                    .with_prefix_match_len(trimmed_query.len()),
                )
            })
            .enumerate()
            .map(|(index, item)| QueryResult::from(item.with_score(OrderedFloat(index as f64))))
            .collect())
    }
}

impl Entity for InlineHistoryMenuDataSource {
    type Event = ();
}
