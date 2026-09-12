//! Classifies cloud-agent startup errors.
use warpui::{AppContext, SingletonEntity as _};

use crate::ai::ambient_agents::{
    OUT_OF_CREDITS_TASK_FAILURE_MESSAGE, SERVER_OVERLOADED_TASK_FAILURE_MESSAGE, github_auth_url,
};
use crate::server::server_api::{AIApiError, ClientError, CloudAgentCapacityError};
use crate::settings::PrivacySettings;
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::workspaces::workspace::AdminEnablementSetting;

/// A recoverable startup condition that requires user action.
///
/// The GUI represents this as `ambient_agent::Status::NeedsGithubAuth`.
/// Orchestrated children retain their surface so the user can follow the
/// remediation link, but the original child launch still resolves as failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudAgentStartupBlocker {
    GitHubAuthRequired { message: String, auth_url: String },
}

/// A terminal cloud-agent startup failure.
///
/// The GUI represents these as `ambient_agent::Status::Failed`. Unlike a
/// blocker, a failure has no remediation action that requires retaining an
/// optimistic orchestrated-child surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudAgentStartupFailure {
    Capacity { message: String },
    OutOfCredits { message: String },
    ServerOverloaded { message: String },
    Other { message: String },
}

/// Renderer-neutral content for a cloud-agent startup card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudAgentStartupPresentation {
    pub title: &'static str,
    pub detail: String,
    pub action_label: Option<&'static str>,
    pub primary_url: Option<String>,
}

impl CloudAgentStartupPresentation {
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            title: "Failed to start environment",
            detail: message.into(),
            action_label: None,
            primary_url: None,
        }
    }

    pub fn github_auth(auth_url: impl Into<String>) -> Self {
        Self {
            title: "GitHub Authentication Required",
            detail: "Please authenticate with GitHub to continue".to_string(),
            action_label: Some("Authenticate with GitHub"),
            primary_url: Some(auth_url.into()),
        }
    }
}
/// Shared interpretation of an error returned while starting a cloud agent.
///
/// This distinction preserves the existing orchestrated-child contract:
/// blockers remain visible for user action, while terminal failures are
/// eligible for failed-launch cleanup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudAgentStartupIssue {
    Blocked(CloudAgentStartupBlocker),
    Failed(CloudAgentStartupFailure),
}

/// Maps server/client launch failures into shared startup presentation.
pub fn classify_cloud_agent_startup_error(error: &anyhow::Error) -> CloudAgentStartupIssue {
    if let Some(client_error) = error.downcast_ref::<ClientError>()
        && let Some(auth_url) = &client_error.auth_url
    {
        return CloudAgentStartupIssue::Blocked(CloudAgentStartupBlocker::GitHubAuthRequired {
            message: client_error.error.clone(),
            auth_url: github_auth_url::cloud_setup_auth_url_with_next(auth_url),
        });
    }
    if let Some(capacity_error) = error.downcast_ref::<CloudAgentCapacityError>() {
        return CloudAgentStartupIssue::Failed(CloudAgentStartupFailure::Capacity {
            message: capacity_error.error.clone(),
        });
    }
    if let Some(ai_api_error) = error.downcast_ref::<AIApiError>() {
        match ai_api_error {
            AIApiError::QuotaLimit {
                user_display_message,
            } => {
                return CloudAgentStartupIssue::Failed(CloudAgentStartupFailure::OutOfCredits {
                    message: user_display_message
                        .clone()
                        .unwrap_or_else(|| OUT_OF_CREDITS_TASK_FAILURE_MESSAGE.to_string()),
                });
            }
            AIApiError::ServerOverloaded => {
                return CloudAgentStartupIssue::Failed(
                    CloudAgentStartupFailure::ServerOverloaded {
                        message: SERVER_OVERLOADED_TASK_FAILURE_MESSAGE.to_string(),
                    },
                );
            }
            AIApiError::Transport(_)
            | AIApiError::Deserialization(_)
            | AIApiError::NoContextFound
            | AIApiError::ErrorStatus(_, _)
            | AIApiError::Other(_)
            | AIApiError::Stream { .. }
            | AIApiError::UnexpectedEof
            | AIApiError::GrokSubscriptionTokenRefreshFailed => {}
        }
    }
    CloudAgentStartupIssue::Failed(CloudAgentStartupFailure::Other {
        message: error.to_string(),
    })
}

pub(crate) fn should_disable_snapshot(ctx: &AppContext) -> bool {
    let privacy = PrivacySettings::as_ref(ctx);
    if !privacy.is_cloud_conversation_storage_enabled {
        return true;
    }
    matches!(
        UserWorkspaces::as_ref(ctx).get_cloud_conversation_storage_enablement_setting(),
        AdminEnablementSetting::Disable
    )
}

#[cfg(test)]
#[path = "remote_child_tests.rs"]
mod tests;
