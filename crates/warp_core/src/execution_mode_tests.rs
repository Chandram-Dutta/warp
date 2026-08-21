use super::{AppExecutionMode, ExecutionMode};

#[test]
#[cfg(feature = "local_only")]
fn local_only_profile_disables_cloud_and_agent_capabilities_in_every_execution_mode() {
    for mode in [
        ExecutionMode::App,
        ExecutionMode::Tui,
        ExecutionMode::Sdk,
        ExecutionMode::RemoteServerDaemon,
    ] {
        let execution_mode = AppExecutionMode {
            mode,
            is_sandboxed: false,
        };

        assert!(!execution_mode.allows_active_ai());
        assert!(!execution_mode.can_sync_preferences());
        assert!(!execution_mode.can_autoupdate());
        assert!(!execution_mode.can_autostart_mcp_servers());
        assert!(!execution_mode.can_show_onboarding());
        assert!(!execution_mode.can_fetch_agent_runs_for_management());
        assert!(!execution_mode.send_telemetry_at_shutdown());
    }
}
