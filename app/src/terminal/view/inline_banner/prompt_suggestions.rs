use serde::Serialize;

use crate::ai::agent::StaticQueryType;

/// Types of zero-state prompt suggestions.
#[derive(Debug, Copy, Clone, Serialize)]
pub enum ZeroStatePromptSuggestionType {
    Explain,
    Fix,
    Install,
    Code,
    Deploy,
    SomethingElse,
}

/// Places zero-stage prompt suggestions are surfaced.
#[derive(Debug, Copy, Clone, Serialize)]
pub enum ZeroStatePromptSuggestionTriggeredFrom {
    InputBar,
    AgentModeHomepage,
    TryAgentModeBanner,
    AgentManagementPopup,
}

impl ZeroStatePromptSuggestionType {
    /// Constant for the number of zero-state prompt suggestion types.
    pub const COUNT: usize = 5;

    pub fn query(&self) -> &'static str {
        match self {
            Self::Explain => "Explain this to me.",
            Self::Fix => "Help me fix this.",
            Self::Install => {
                "Help me install a binary/dependency. What information do I need to provide to you to do this?"
            }
            Self::Code => {
                "Help me write some code. What information do I need to provide to you to do this?"
            }
            Self::Deploy => {
                "Help me deploy my project. What information do I need to provide to you to do this?"
            }
            Self::SomethingElse => "Something else?",
        }
    }

    pub fn static_query_type(&self) -> Option<StaticQueryType> {
        match self {
            Self::Explain | Self::Fix => None,
            Self::Install => Some(StaticQueryType::Install),
            Self::Code => Some(StaticQueryType::Code),
            Self::Deploy => Some(StaticQueryType::Deploy),
            Self::SomethingElse => Some(StaticQueryType::SomethingElse),
        }
    }
}
