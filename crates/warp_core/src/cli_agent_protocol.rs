use enum_iterator::Sequence;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

/// Sentinel title that identifies structured CLI-agent events sent via OSC 777.
pub const CLI_AGENT_NOTIFICATION_SENTINEL: &str = "warp://cli-agent";

/// Schema version emitted by the current CLI-agent notification protocol.
pub const CLI_AGENT_PROTOCOL_VERSION: u32 = 1;

/// Environment variable that advertises the host's CLI-agent protocol version.
pub const WARP_CLI_AGENT_PROTOCOL_VERSION_ENV: &str = "WARP_CLI_AGENT_PROTOCOL_VERSION";

/// Environment variable that identifies the hosting Warp client version.
pub const WARP_CLIENT_VERSION_ENV: &str = "WARP_CLIENT_VERSION";

/// Stable identity stored for a detected third-party CLI agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence, Serialize, Deserialize)]
pub enum CLIAgent {
    Claude,
    Gemini,
    Codex,
    Amp,
    Droid,
    OpenCode,
    Copilot,
    Pi,
    OhMyPi,
    Auggie,
    CursorCli,
    Goose,
    Hermes,
    Vibe,
    Antigravity,
    WarpTui,
    Unknown,
}

impl CLIAgent {
    /// Serializes the stable enum name used by historical session-sharing data and settings.
    pub fn to_serialized_name(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// Decodes a historical serialized enum name, preserving unknown values as [`Self::Unknown`].
    pub fn from_serialized_name(name: &str) -> Self {
        serde_json::from_value(name.into()).unwrap_or(Self::Unknown)
    }
}

/// Wire representation of a structured CLI-agent notification.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CLIAgentNotification {
    pub v: Option<u32>,
    pub agent: Option<String>,
    pub event: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub project: Option<String>,
    pub query: Option<String>,
    pub response: Option<String>,
    pub transcript_path: Option<String>,
    pub summary: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub plugin_version: Option<String>,
    pub error_type: Option<String>,
}

impl CLIAgentNotification {
    pub fn new(agent: impl Into<String>, event: impl Into<String>) -> Self {
        Self {
            v: Some(CLI_AGENT_PROTOCOL_VERSION),
            agent: Some(agent.into()),
            event: event.into(),
            session_id: None,
            cwd: None,
            project: None,
            query: None,
            response: None,
            transcript_path: None,
            summary: None,
            tool_name: None,
            tool_input: None,
            plugin_version: None,
            error_type: None,
        }
    }
}

#[cfg(test)]
#[path = "cli_agent_protocol_tests.rs"]
mod tests;
