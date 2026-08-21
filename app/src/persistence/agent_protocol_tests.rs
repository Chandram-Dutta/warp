use std::collections::HashMap;

use persistence::model::{
    ContextWindowSegment, ContextWindowSegmentType, ModelTokenUsage, RunCommandStats,
    ToolUsageMetadata,
};

use super::*;

fn parentless_task(id: &str, message_count: usize) -> api::Task {
    api::Task {
        id: id.to_string(),
        messages: (0..message_count)
            .map(|index| api::Message {
                id: format!("{id}-message-{index}"),
                task_id: id.to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn child_task(id: &str, parent_id: &str) -> api::Task {
    api::Task {
        id: id.to_string(),
        dependencies: Some(api::task::Dependencies {
            parent_task_id: parent_id.to_string(),
        }),
        ..Default::default()
    }
}

fn user_query_message(task_id: &str, query: &str, pwd: &str) -> api::Message {
    api::Message {
        id: format!("{task_id}-user-query"),
        task_id: task_id.to_string(),
        message: Some(api::message::Message::UserQuery(api::message::UserQuery {
            query: query.to_string(),
            context: Some(api::InputContext {
                directory: Some(api::input_context::Directory {
                    pwd: pwd.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn auto_code_diff_message(task_id: &str) -> api::Message {
    api::Message {
        id: format!("{task_id}-auto-code-diff"),
        task_id: task_id.to_string(),
        message: Some(api::message::Message::SystemQuery(
            api::message::SystemQuery {
                r#type: Some(api::message::system_query::Type::AutoCodeDiff(
                    api::message::AutoCodeDiff {
                        query: "diff".to_string(),
                    },
                )),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
}

#[test]
fn legacy_stub_and_real_root_remains_restorable() {
    let conversation = AgentConversation {
        conversation: Default::default(),
        tasks: vec![
            parentless_task("optimistic-stub", 0),
            parentless_task("server-root", 2),
            child_task("child", "server-root"),
        ],
    };

    assert!(conversation.is_restorable());
}

#[test]
fn multiple_real_roots_remain_non_restorable() {
    let conversation = AgentConversation {
        conversation: Default::default(),
        tasks: vec![parentless_task("root-a", 1), parentless_task("root-b", 1)],
    };

    assert!(!conversation.is_restorable());
}

#[test]
fn empty_roots_remain_non_restorable() {
    let conversation = AgentConversation {
        conversation: Default::default(),
        tasks: vec![parentless_task("root-a", 0), parentless_task("root-b", 0)],
    };

    assert!(!conversation.is_restorable());
}

#[test]
fn summary_derivation_stays_in_agent_protocol_adapter() {
    let mut root = parentless_task("root", 0);
    root.description = "Repository status".to_string();
    root.messages = vec![user_query_message("root", "git status", "/tmp/repo")];

    let summary = agent_conversation_summary_from_tasks([&root]);

    assert_eq!(summary.initial_query, "git status");
    assert_eq!(summary.title, "Repository status");
    assert_eq!(
        summary.initial_working_directory.as_deref(),
        Some("/tmp/repo")
    );
    assert!(summary.is_restorable);
}

#[test]
fn passive_diff_summary_visibility_is_preserved() {
    let mut root = parentless_task("root", 0);
    root.messages = vec![auto_code_diff_message("root")];
    assert!(agent_conversation_summary_from_tasks([&root]).is_unlisted_auto_code_diff);

    root.messages
        .push(user_query_message("root", "Explain", "/tmp/repo"));
    assert!(!agent_conversation_summary_from_tasks([&root]).is_unlisted_auto_code_diff);
}

#[test]
fn usage_protocol_adapters_preserve_persisted_values() {
    let usage = ModelTokenUsage {
        model_id: "local-model".to_string(),
        custom_endpoint_tokens: 6,
        custom_endpoint_token_usage_by_category: HashMap::from([("primary_agent".to_string(), 6)]),
        ..Default::default()
    };
    let (model_id, proto_usage) = usage
        .to_proto_custom_endpoint_usage()
        .expect("non-empty usage should convert");
    assert_eq!(model_id, "local-model");
    assert_eq!(proto_usage.total_tokens, 6);
    assert!(
        ModelTokenUsage {
            model_id: "warp-model".to_string(),
            warp_tokens: 4,
            ..Default::default()
        }
        .to_proto_custom_endpoint_usage()
        .is_none()
    );

    let metadata = ToolUsageMetadata {
        run_command_stats: RunCommandStats {
            count: 2,
            commands_executed: 1,
        },
        ..Default::default()
    };
    let roundtripped = tool_usage_metadata_from_proto(&tool_usage_metadata_to_proto(&metadata));
    assert_eq!(roundtripped.run_command_stats.count, 2);
    assert_eq!(roundtripped.run_command_stats.commands_executed, 1);

    let segment = ContextWindowSegment {
        segment_type: ContextWindowSegmentType::LatestInput,
        token_count: 42,
    };
    let roundtripped =
        context_window_segment_from_proto(&context_window_segment_to_proto(&segment));
    assert_eq!(
        roundtripped.segment_type,
        ContextWindowSegmentType::LatestInput
    );
    assert_eq!(roundtripped.token_count, 42);
}
