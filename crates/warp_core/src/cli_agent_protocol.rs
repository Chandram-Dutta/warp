use enum_iterator::Sequence;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
#[path = "cli_agent_protocol_tests.rs"]
mod tests;
