use std::collections::HashSet;

use warpui::{AppContext, SingletonEntity};

use super::History;
use crate::input_suggestions::HistoryInputSuggestion;
use crate::suggestions::ignored_suggestions_model::{IgnoredSuggestionsModel, SuggestionType};
use crate::terminal::model::session::SessionId;

impl History {
    pub(crate) fn up_arrow_suggestions<'a>(
        &'a self,
        session_id: Option<SessionId>,
        ctx: &'a AppContext,
    ) -> Vec<HistoryInputSuggestion<'a>> {
        let ignored = ctx
            .has_singleton_model::<IgnoredSuggestionsModel>()
            .then(|| IgnoredSuggestionsModel::handle(ctx).as_ref(ctx));
        let mut suggestions: Vec<_> = session_id
            .and_then(|session_id| self.commands(session_id))
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| {
                ignored.is_none_or(|ignored| {
                    !ignored.is_ignored(&entry.command, SuggestionType::ShellCommand)
                })
            })
            .map(|entry| HistoryInputSuggestion { entry })
            .collect();
        suggestions.sort_by(|a, b| a.cmp(b, session_id));

        // Keep the latest occurrence without changing session precedence.
        let mut seen = HashSet::new();
        suggestions.reverse();
        suggestions.retain(|suggestion| {
            let text = suggestion.normalized_text();
            !text.is_empty() && seen.insert(text.to_owned())
        });
        suggestions.reverse();
        suggestions
    }
}

#[cfg(test)]
#[path = "up_arrow_tests.rs"]
mod tests;
