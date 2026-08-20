use super::*;

#[test]
#[ignore = "CORE-3768 - need to clean up PREVIEW_FLAGS, but this is a temporary fix for the cluttered changelog"]
fn test_all_preview_flags_have_a_description() {
    for flag in PREVIEW_FLAGS {
        assert!(
            flag.flag_description()
                .is_some_and(|description| !description.is_empty()),
            "Missing description for preview-enabled flag {flag:?}"
        );
    }
}

#[test]
fn local_child_harnesses_are_local_only_by_default() {
    assert!(LOCAL_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
    assert!(!DEBUG_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
    assert!(!DOGFOOD_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
}

#[test]
fn local_only_profile_allows_terminal_features() {
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::ShellSelector));
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::SkipFirebaseAnonymousUser));
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::NativeShellCompletions));
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::CommandCorrectionKey));
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::PartialNextCommandSuggestions));
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::TabConfigs));
    assert!(LOCAL_ONLY_FLAGS.contains(&FeatureFlag::TerminalLifecycleRecovery));
}

#[test]
fn local_only_profile_rejects_agent_and_cloud_features() {
    assert!(!LOCAL_ONLY_FLAGS.contains(&FeatureFlag::AgentMode));
    assert!(!LOCAL_ONLY_FLAGS.contains(&FeatureFlag::AgentView));
    assert!(!LOCAL_ONLY_FLAGS.contains(&FeatureFlag::McpServer));
    assert!(!LOCAL_ONLY_FLAGS.contains(&FeatureFlag::CloudObjects));
    assert!(!LOCAL_ONLY_FLAGS.contains(&FeatureFlag::CreatingSharedSessions));
    assert!(!LOCAL_ONLY_FLAGS.contains(&FeatureFlag::RuntimeFeatureFlags));
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_profile_enforces_allow_list_after_user_overrides() {
    mark_initialized();
    FeatureFlag::SkipFirebaseAnonymousUser.set_user_preference(true);
    FeatureFlag::AgentMode.set_user_preference(true);

    assert!(FeatureFlag::SkipFirebaseAnonymousUser.is_enabled());
    assert!(!FeatureFlag::AgentMode.is_enabled());
}
