use anyhow::anyhow;

use super::{
    CloudAgentStartupBlocker, CloudAgentStartupFailure, CloudAgentStartupIssue,
    CloudAgentStartupPresentation, classify_cloud_agent_startup_error,
};
use crate::server::server_api::{AIApiError, ClientError, CloudAgentCapacityError};

#[test]
fn github_auth_error_is_a_shared_blocker_with_cloud_callback_url() {
    let error = anyhow::Error::new(ClientError {
        error: "GitHub authentication required".to_string(),
        auth_url: Some("https://example.com/auth?scheme=warpdev".to_string()),
    });
    let CloudAgentStartupIssue::Blocked(CloudAgentStartupBlocker::GitHubAuthRequired {
        message,
        auth_url,
    }) = classify_cloud_agent_startup_error(&error)
    else {
        panic!("expected GitHub auth blocker");
    };
    assert_eq!(message, "GitHub authentication required");
    assert!(auth_url.starts_with("https://example.com/auth?"));
    assert!(auth_url.contains("next="));
}

#[test]
fn cloud_startup_presentations_preserve_gui_copy() {
    assert_eq!(
        CloudAgentStartupPresentation::failure("Server error"),
        CloudAgentStartupPresentation {
            title: "Failed to start environment",
            detail: "Server error".to_string(),
            action_label: None,
            primary_url: None,
        }
    );
    assert_eq!(
        CloudAgentStartupPresentation::github_auth("https://example.com/auth"),
        CloudAgentStartupPresentation {
            title: "GitHub Authentication Required",
            detail: "Please authenticate with GitHub to continue".to_string(),
            action_label: Some("Authenticate with GitHub"),
            primary_url: Some("https://example.com/auth".to_string()),
        }
    );
}

#[test]
fn capacity_quota_and_fallback_errors_keep_their_semantics() {
    let capacity = anyhow::Error::new(CloudAgentCapacityError {
        error: "Too many agents".to_string(),
        running_agents: 4,
    });
    assert_eq!(
        classify_cloud_agent_startup_error(&capacity),
        CloudAgentStartupIssue::Failed(CloudAgentStartupFailure::Capacity {
            message: "Too many agents".to_string(),
        })
    );

    let quota = anyhow::Error::new(AIApiError::QuotaLimit {
        user_display_message: Some("Buy more credits".to_string()),
    });
    assert_eq!(
        classify_cloud_agent_startup_error(&quota),
        CloudAgentStartupIssue::Failed(CloudAgentStartupFailure::OutOfCredits {
            message: "Buy more credits".to_string(),
        })
    );

    let fallback = anyhow!("network unavailable");
    assert_eq!(
        classify_cloud_agent_startup_error(&fallback),
        CloudAgentStartupIssue::Failed(CloudAgentStartupFailure::Other {
            message: "network unavailable".to_string(),
        })
    );
}
