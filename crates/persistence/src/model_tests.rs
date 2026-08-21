use super::{AgentConversationData, AgentConversationSummary, ConversationUsageMetadata};

#[test]
fn conversation_usage_metadata_defaults_missing_provider_cost_to_unknown() {
    let metadata: ConversationUsageMetadata = serde_json::from_str(
        r#"{"was_summarized":false,"context_window_usage":0.0,"credits_spent":0.0}"#,
    )
    .unwrap();

    assert_eq!(metadata.total_provider_cost_in_cents, None);
    assert!(
        !serde_json::to_string(&metadata)
            .unwrap()
            .contains("total_provider_cost_in_cents")
    );
}

#[test]
fn conversation_usage_metadata_preserves_known_zero_provider_cost() {
    let metadata: ConversationUsageMetadata = serde_json::from_str(
        r#"{"was_summarized":false,"context_window_usage":0.0,"credits_spent":0.0,"total_provider_cost_in_cents":0.0}"#,
    )
    .unwrap();

    assert_eq!(metadata.total_provider_cost_in_cents, Some(0.0));
    assert!(
        serde_json::to_string(&metadata)
            .unwrap()
            .contains("\"total_provider_cost_in_cents\":0.0")
    );
}

#[test]
fn agent_conversation_summary_decodes_existing_rows_without_runtime_types() {
    let summary: AgentConversationSummary = serde_json::from_str(
        r#"{"initial_query":"git status","title":"Inspect repository","initial_working_directory":"/tmp/repo","is_restorable":true}"#,
    )
    .expect("existing summary rows must remain decodable");

    assert_eq!(summary.initial_query, "git status");
    assert_eq!(summary.title, "Inspect repository");
    assert_eq!(
        summary.initial_working_directory.as_deref(),
        Some("/tmp/repo")
    );
    assert!(summary.is_restorable);
    assert!(!summary.is_unlisted_auto_code_diff);
}

#[test]
fn agent_conversation_data_roundtrips_last_event_sequence() {
    let data = AgentConversationData {
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        orchestration_harness_type: Some("claude".to_string()),
        parent_conversation_id: None,
        is_remote_child: false,
        root_task_is_optimistic: None,
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: Some(42),
        pinned: false,
    };
    let json = serde_json::to_string(&data).expect("serialize");
    let roundtripped: AgentConversationData = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(roundtripped.last_event_sequence, Some(42));
    assert_eq!(
        roundtripped.orchestration_harness_type.as_deref(),
        Some("claude")
    );
}

#[test]
fn agent_conversation_data_accepts_legacy_orchestration_avatar_id() {
    let legacy_json = r#"{"orchestration_avatar_id":"orbit"}"#;
    let data: AgentConversationData =
        serde_json::from_str(legacy_json).expect("legacy rows must deserialize");

    assert_eq!(data.orchestration_harness_type.as_deref(), Some("orbit"));
}

#[test]
fn agent_conversation_data_roundtrips_remote_child_marker() {
    let data = AgentConversationData {
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        orchestration_harness_type: None,
        parent_conversation_id: None,
        is_remote_child: true,
        root_task_is_optimistic: None,
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: None,
        pinned: false,
    };
    let json = serde_json::to_string(&data).expect("serialize");
    let roundtripped: AgentConversationData = serde_json::from_str(&json).expect("deserialize");
    assert!(roundtripped.is_remote_child);
}

#[test]
fn agent_conversation_data_roundtrips_optimistic_root_marker() {
    let data = AgentConversationData {
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        orchestration_harness_type: None,
        parent_conversation_id: None,
        is_remote_child: false,
        root_task_is_optimistic: Some(true),
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: None,
        pinned: false,
    };
    let json = serde_json::to_string(&data).expect("serialize");
    let roundtripped: AgentConversationData = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(roundtripped.root_task_is_optimistic, Some(true));
}

#[test]
fn agent_conversation_data_deserializes_legacy_payload_without_last_event_sequence() {
    // Legacy rows persisted before this feature landed omit the field
    // entirely. `#[serde(default)]` must accept them as `None`.
    let legacy_json = r#"{"server_conversation_token":null}"#;
    let data: AgentConversationData =
        serde_json::from_str(legacy_json).expect("legacy rows must deserialize");
    assert_eq!(data.last_event_sequence, None);
    assert_eq!(data.orchestration_harness_type, None);
    assert!(!data.is_remote_child);
}

#[test]
fn agent_conversation_data_skips_serializing_none_last_event_sequence() {
    let data = AgentConversationData {
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        orchestration_harness_type: None,
        parent_conversation_id: None,
        is_remote_child: false,
        root_task_is_optimistic: None,
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: None,
        pinned: false,
    };
    let json = serde_json::to_string(&data).expect("serialize");
    assert!(
        !json.contains("last_event_sequence"),
        "None should be skipped in serialized output: {json}"
    );
}

#[test]
fn agent_conversation_data_roundtrips_pinned() {
    let data = AgentConversationData {
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        orchestration_harness_type: None,
        parent_conversation_id: None,
        is_remote_child: false,
        root_task_is_optimistic: None,
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: None,
        pinned: true,
    };
    let json = serde_json::to_string(&data).expect("serialize");
    let roundtripped: AgentConversationData = serde_json::from_str(&json).expect("deserialize");
    assert!(roundtripped.pinned);
}

#[test]
fn agent_conversation_data_skips_serializing_unpinned() {
    let data = AgentConversationData {
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        orchestration_harness_type: None,
        parent_conversation_id: None,
        is_remote_child: false,
        root_task_is_optimistic: None,
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: None,
        pinned: false,
    };
    let json = serde_json::to_string(&data).expect("serialize");
    assert!(
        !json.contains("pinned"),
        "Unpinned default should be skipped: {json}"
    );
}

#[test]
fn agent_conversation_data_legacy_rows_default_to_unpinned() {
    let legacy_json = r#"{"server_conversation_token":null}"#;
    let data: AgentConversationData =
        serde_json::from_str(legacy_json).expect("legacy rows must deserialize");
    assert!(!data.pinned);
}
