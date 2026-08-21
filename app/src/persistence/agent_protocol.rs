//! Agent protocol adapters for the terminal-safe persisted data model.

use std::collections::HashMap;

use persistence::model::{
    AgentConversationRecord, AgentConversationSummary, ApplyFileDiffStats, ContextWindowSegment,
    ContextWindowSegmentType, ModelTokenUsage, RunCommandStats, TokenUsageCategory, ToolCallStats,
    ToolUsageMetadata,
};
use warp_multi_agent_api::response_event::stream_finished;
use warp_multi_agent_api::{self as api};

#[derive(Debug, PartialEq, Default, Clone)]
pub struct AgentConversation {
    pub conversation: AgentConversationRecord,
    pub tasks: Vec<api::Task>,
}

impl AgentConversation {
    pub fn is_restorable(&self) -> bool {
        tasks_are_restorable(self.tasks.iter())
    }
}

fn tasks_are_restorable<'a>(tasks: impl IntoIterator<Item = &'a api::Task>) -> bool {
    let tasks: Vec<&api::Task> = tasks.into_iter().collect();
    if tasks.len() <= 1 {
        return true;
    }

    let root_tasks: Vec<_> = tasks
        .iter()
        .filter(|task| {
            task.dependencies
                .as_ref()
                .map(|dependencies| dependencies.parent_task_id.is_empty())
                .unwrap_or(true)
        })
        .collect();

    match root_tasks.len() {
        0 => false,
        1 => true,
        // Older writers could persist an empty optimistic root beside the real root. Restore is
        // safe only when exactly one root contains the conversation messages.
        _ => {
            root_tasks
                .iter()
                .filter(|task| !task.messages.is_empty())
                .count()
                == 1
        }
    }
}

fn api_task_initial_working_directory(task: &api::Task) -> Option<String> {
    task.messages
        .iter()
        .find_map(|message| {
            message.message.as_ref().and_then(|content| {
                let context = match content {
                    api::message::Message::UserQuery(user_query) => user_query.context.as_ref(),
                    api::message::Message::ToolCallResult(tool_call_result) => {
                        tool_call_result.context.as_ref()
                    }
                    api::message::Message::SystemQuery(system_query) => {
                        system_query.context.as_ref()
                    }
                    _ => None,
                };

                context
                    .and_then(|context| context.directory.as_ref())
                    .map(|directory| directory.pwd.clone())
            })
        })
        .filter(|pwd| !pwd.is_empty())
}

pub(crate) fn agent_conversation_summary_from_tasks<'a>(
    tasks: impl IntoIterator<Item = &'a api::Task>,
) -> AgentConversationSummary {
    let tasks: Vec<&api::Task> = tasks.into_iter().collect();

    let mut has_user_query = false;
    let mut has_auto_code_diff = false;
    for task in &tasks {
        for message in &task.messages {
            match &message.message {
                Some(api::message::Message::UserQuery(_)) => {
                    has_user_query = true;
                }
                Some(api::message::Message::SystemQuery(system_query)) => {
                    if let Some(api::message::system_query::Type::AutoCodeDiff(_)) =
                        &system_query.r#type
                    {
                        has_auto_code_diff = true;
                    }
                }
                _ => {}
            }
        }
    }

    let root_task = tasks.iter().find(|task| task.dependencies.is_none());
    let initial_query = root_task
        .map(|task| {
            task.messages
                .iter()
                .find_map(|message| match &message.message {
                    Some(api::message::Message::UserQuery(user_query)) => {
                        Some(user_query.query.clone())
                    }
                    Some(api::message::Message::ToolCall(tool_call)) => {
                        match tool_call.tool.as_ref()? {
                            api::message::tool_call::Tool::ApplyFileDiffs(diff_suggestion) => {
                                Some(diff_suggestion.summary.clone())
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let title = root_task
        .map(|task| task.description.clone())
        .filter(|description| !description.is_empty())
        .unwrap_or_else(|| initial_query.clone());
    let initial_working_directory = tasks
        .iter()
        .find_map(|task| api_task_initial_working_directory(task));

    AgentConversationSummary {
        initial_query,
        title,
        initial_working_directory,
        is_restorable: tasks_are_restorable(tasks.iter().copied()),
        is_unlisted_auto_code_diff: has_auto_code_diff && !has_user_query,
    }
}

pub(crate) trait ModelTokenUsageProtoExt {
    fn to_proto_warp_usage(&self) -> Option<(String, stream_finished::ModelTokenUsage)>;
    fn to_proto_byok_usage(&self) -> Option<(String, stream_finished::ModelTokenUsage)>;
    fn to_proto_custom_endpoint_usage(&self) -> Option<(String, stream_finished::ModelTokenUsage)>;
    fn to_proto_combined(&self) -> stream_finished::ModelTokenUsage;
}

impl ModelTokenUsageProtoExt for ModelTokenUsage {
    fn to_proto_warp_usage(&self) -> Option<(String, stream_finished::ModelTokenUsage)> {
        model_token_usage_to_proto(self, self.warp_tokens, &self.warp_token_usage_by_category)
    }

    fn to_proto_byok_usage(&self) -> Option<(String, stream_finished::ModelTokenUsage)> {
        model_token_usage_to_proto(self, self.byok_tokens, &self.byok_token_usage_by_category)
    }

    #[allow(deprecated)]
    fn to_proto_custom_endpoint_usage(&self) -> Option<(String, stream_finished::ModelTokenUsage)> {
        model_token_usage_to_proto(
            self,
            self.custom_endpoint_tokens,
            &self.custom_endpoint_token_usage_by_category,
        )
    }

    #[allow(deprecated)]
    fn to_proto_combined(&self) -> stream_finished::ModelTokenUsage {
        stream_finished::ModelTokenUsage {
            model_id: self.model_id.clone(),
            total_tokens: self.warp_tokens + self.byok_tokens + self.custom_endpoint_tokens,
            token_usage_by_category: self
                .warp_token_usage_by_category
                .iter()
                .chain(self.byok_token_usage_by_category.iter())
                .chain(self.custom_endpoint_token_usage_by_category.iter())
                .fold(HashMap::new(), |mut usage, (category, tokens)| {
                    *usage.entry(category.clone()).or_insert(0) += tokens;
                    usage
                }),
        }
    }
}

#[allow(deprecated)]
fn model_token_usage_to_proto(
    usage: &ModelTokenUsage,
    total_tokens: u32,
    usage_by_category: &HashMap<TokenUsageCategory, u32>,
) -> Option<(String, stream_finished::ModelTokenUsage)> {
    if total_tokens == 0 {
        return None;
    }
    Some((
        usage.model_id.clone(),
        stream_finished::ModelTokenUsage {
            model_id: usage.model_id.clone(),
            total_tokens,
            token_usage_by_category: usage_by_category.clone(),
        },
    ))
}

fn tool_call_stats_to_proto(stats: &ToolCallStats) -> stream_finished::ToolCallStats {
    stream_finished::ToolCallStats { count: stats.count }
}

fn tool_call_stats_from_proto(stats: Option<&stream_finished::ToolCallStats>) -> ToolCallStats {
    ToolCallStats {
        count: stats.map_or(0, |stats| stats.count),
    }
}

fn run_command_stats_to_proto(stats: &RunCommandStats) -> stream_finished::RunCommandStats {
    stream_finished::RunCommandStats {
        count: stats.count,
        command_executed: stats.commands_executed,
    }
}

fn run_command_stats_from_proto(
    stats: Option<&stream_finished::RunCommandStats>,
) -> RunCommandStats {
    RunCommandStats {
        count: stats.map_or(0, |stats| stats.count),
        commands_executed: stats.map_or(0, |stats| stats.command_executed),
    }
}

fn apply_file_diff_stats_to_proto(
    stats: &ApplyFileDiffStats,
) -> stream_finished::ApplyFileDiffStats {
    stream_finished::ApplyFileDiffStats {
        count: stats.count,
        lines_added: stats.lines_added,
        lines_removed: stats.lines_removed,
        files_changed: stats.files_changed,
    }
}

fn apply_file_diff_stats_from_proto(
    stats: Option<&stream_finished::ApplyFileDiffStats>,
) -> ApplyFileDiffStats {
    ApplyFileDiffStats {
        count: stats.map_or(0, |stats| stats.count),
        lines_added: stats.map_or(0, |stats| stats.lines_added),
        lines_removed: stats.map_or(0, |stats| stats.lines_removed),
        files_changed: stats.map_or(0, |stats| stats.files_changed),
    }
}

pub(crate) fn tool_usage_metadata_to_proto(
    metadata: &ToolUsageMetadata,
) -> stream_finished::ToolUsageMetadata {
    stream_finished::ToolUsageMetadata {
        run_command_stats: Some(run_command_stats_to_proto(&metadata.run_command_stats)),
        read_files_stats: Some(tool_call_stats_to_proto(&metadata.read_files_stats)),
        search_codebase_stats: Some(tool_call_stats_to_proto(&metadata.search_codebase_stats)),
        grep_stats: Some(tool_call_stats_to_proto(&metadata.grep_stats)),
        file_glob_stats: Some(tool_call_stats_to_proto(&metadata.file_glob_stats)),
        apply_file_diff_stats: Some(apply_file_diff_stats_to_proto(
            &metadata.apply_file_diff_stats,
        )),
        write_to_long_running_shell_command_stats: Some(tool_call_stats_to_proto(
            &metadata.write_to_long_running_shell_command_stats,
        )),
        read_mcp_resource_stats: Some(tool_call_stats_to_proto(&metadata.read_mcp_resource_stats)),
        call_mcp_tool_stats: Some(tool_call_stats_to_proto(&metadata.call_mcp_tool_stats)),
        suggest_plan_stats: Some(tool_call_stats_to_proto(&metadata.suggest_plan_stats)),
        suggest_create_plan_stats: Some(tool_call_stats_to_proto(
            &metadata.suggest_create_plan_stats,
        )),
        read_shell_command_output_stats: Some(tool_call_stats_to_proto(
            &metadata.read_shell_command_output_stats,
        )),
        use_computer_stats: Some(tool_call_stats_to_proto(&metadata.use_computer_stats)),
    }
}

pub(crate) fn tool_usage_metadata_from_proto(
    metadata: &stream_finished::ToolUsageMetadata,
) -> ToolUsageMetadata {
    ToolUsageMetadata {
        run_command_stats: run_command_stats_from_proto(metadata.run_command_stats.as_ref()),
        read_files_stats: tool_call_stats_from_proto(metadata.read_files_stats.as_ref()),
        search_codebase_stats: tool_call_stats_from_proto(metadata.search_codebase_stats.as_ref()),
        grep_stats: tool_call_stats_from_proto(metadata.grep_stats.as_ref()),
        file_glob_stats: tool_call_stats_from_proto(metadata.file_glob_stats.as_ref()),
        apply_file_diff_stats: apply_file_diff_stats_from_proto(
            metadata.apply_file_diff_stats.as_ref(),
        ),
        write_to_long_running_shell_command_stats: tool_call_stats_from_proto(
            metadata.write_to_long_running_shell_command_stats.as_ref(),
        ),
        read_mcp_resource_stats: tool_call_stats_from_proto(
            metadata.read_mcp_resource_stats.as_ref(),
        ),
        call_mcp_tool_stats: tool_call_stats_from_proto(metadata.call_mcp_tool_stats.as_ref()),
        suggest_plan_stats: tool_call_stats_from_proto(metadata.suggest_plan_stats.as_ref()),
        suggest_create_plan_stats: tool_call_stats_from_proto(
            metadata.suggest_create_plan_stats.as_ref(),
        ),
        read_shell_command_output_stats: tool_call_stats_from_proto(
            metadata.read_shell_command_output_stats.as_ref(),
        ),
        use_computer_stats: tool_call_stats_from_proto(metadata.use_computer_stats.as_ref()),
    }
}

fn context_window_segment_type_from_proto(value: i32) -> ContextWindowSegmentType {
    match stream_finished::ContextWindowSegmentType::try_from(value) {
        Ok(stream_finished::ContextWindowSegmentType::SystemPrompt) => {
            ContextWindowSegmentType::SystemPrompt
        }
        Ok(stream_finished::ContextWindowSegmentType::ToolDefinitions) => {
            ContextWindowSegmentType::ToolDefinitions
        }
        Ok(stream_finished::ContextWindowSegmentType::ConversationHistory) => {
            ContextWindowSegmentType::ConversationHistory
        }
        Ok(stream_finished::ContextWindowSegmentType::LatestInput) => {
            ContextWindowSegmentType::LatestInput
        }
        Ok(stream_finished::ContextWindowSegmentType::Images) => ContextWindowSegmentType::Images,
        Ok(stream_finished::ContextWindowSegmentType::Other) => ContextWindowSegmentType::Other,
        _ => ContextWindowSegmentType::Unknown,
    }
}

fn context_window_segment_type_to_proto(value: ContextWindowSegmentType) -> i32 {
    match value {
        ContextWindowSegmentType::Unknown => {
            stream_finished::ContextWindowSegmentType::Unknown as i32
        }
        ContextWindowSegmentType::SystemPrompt => {
            stream_finished::ContextWindowSegmentType::SystemPrompt as i32
        }
        ContextWindowSegmentType::ToolDefinitions => {
            stream_finished::ContextWindowSegmentType::ToolDefinitions as i32
        }
        ContextWindowSegmentType::ConversationHistory => {
            stream_finished::ContextWindowSegmentType::ConversationHistory as i32
        }
        ContextWindowSegmentType::LatestInput => {
            stream_finished::ContextWindowSegmentType::LatestInput as i32
        }
        ContextWindowSegmentType::Images => {
            stream_finished::ContextWindowSegmentType::Images as i32
        }
        ContextWindowSegmentType::Other => stream_finished::ContextWindowSegmentType::Other as i32,
    }
}

pub(crate) fn context_window_segment_from_proto(
    segment: &stream_finished::ContextWindowSegment,
) -> ContextWindowSegment {
    ContextWindowSegment {
        segment_type: context_window_segment_type_from_proto(segment.segment_type),
        token_count: segment.token_count,
    }
}

pub(crate) fn context_window_segment_to_proto(
    segment: &ContextWindowSegment,
) -> stream_finished::ContextWindowSegment {
    stream_finished::ContextWindowSegment {
        segment_type: context_window_segment_type_to_proto(segment.segment_type),
        token_count: segment.token_count,
    }
}

#[cfg(test)]
#[path = "agent_protocol_tests.rs"]
mod tests;
