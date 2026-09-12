//! CLI agent detection and configuration.
//!
//! This module provides types for detecting and working with CLI-based AI agents
//! like Claude Code, Gemini CLI, Codex, Amp, and Droid.

use std::borrow::Cow;
use std::collections::HashMap;

use ai::skills::SkillProvider;
use pathfinder_color::ColorU;
use smol_str::SmolStr;
use warp_cli::agent::Harness;
use warp_completer::parsers::simple::top_level_command;
pub use warp_core::cli_agent_protocol::CLIAgent;
use warp_util::path::EscapeChar;
use warpui::{AppContext, SingletonEntity};

use crate::ai::blocklist::CLAUDE_ORANGE;
use crate::server::telemetry::CLIAgentType;
use crate::ui_components::icons::Icon;
use crate::workspaces::user_workspaces::UserWorkspaces;

/// UID for the Uber team.
/// See https://warp.metabaseapp.com/dashboard/1454?team_id=46347
const UBER_TEAM_UID: &str = "BdVbYjy9LRZcZrYBemSfAF";

/// Gemini brand blue color
pub(crate) const GEMINI_BLUE: ColorU = ColorU {
    r: 66,
    g: 133,
    b: 244,
    a: 255,
};

/// OpenAI brand color (dark gray/black)
pub(crate) const OPENAI_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

/// Amp brand color (#F34E3F)
const AMP_COLOR: ColorU = ColorU {
    r: 243,
    g: 78,
    b: 63,
    a: 255,
};

/// Droid brand color (white)
const DROID_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// OpenCode brand color (gray, used for contrast calculation only)
pub(crate) const OPENCODE_COLOR: ColorU = ColorU {
    r: 128,
    g: 128,
    b: 128,
    a: 255,
};

/// Copilot brand color (Copilot purple selected from https://brand.github.com/brand-identity/copilot)
const COPILOT_COLOR: ColorU = ColorU {
    r: 133,
    g: 52,
    b: 243,
    a: 255,
};

/// Pi brand color (white, monochrome logo)
const PI_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// Antigravity brand color (white, monochrome logo)
const ANTIGRAVITY_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// Auggie brand color (white, monochrome logo)
const AUGGIE_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// Cursor brand color (#26251E, from official brand assets)
const CURSOR_COLOR: ColorU = ColorU {
    r: 38,
    g: 37,
    b: 30,
    a: 255,
};

/// Goose brand color (#101010, from Block's official Goose logo)
const GOOSE_COLOR: ColorU = ColorU {
    r: 16,
    g: 16,
    b: 16,
    a: 255,
};

/// Hermes brand color (Nous Research purple #7C3AED)
const HERMES_PURPLE: ColorU = ColorU {
    r: 124,
    g: 58,
    b: 237,
    a: 255,
};

/// Mistral brand orange (#FA520F)
const MISTRAL_ORANGE: ColorU = ColorU {
    r: 250,
    g: 82,
    b: 15,
    a: 255,
};

pub(crate) trait CLIAgentRuntimeExt {
    fn command_prefixes(&self) -> &'static [&'static str];
    fn command_prefix(&self) -> &'static str;
    fn from_harness(harness: Harness) -> Option<Self>
    where
        Self: Sized;
    fn display_name(&self) -> &'static str;
    fn icon(&self) -> Option<Icon>;
    fn supported_skill_providers(&self) -> &'static [SkillProvider];
    fn skill_command_prefix(&self) -> &'static str;
    fn supports_bash_mode(&self) -> bool;
    fn supports_cli_agent_footer(&self) -> bool;
    fn brand_color(&self) -> Option<ColorU>;
    fn brand_icon_color(&self) -> ColorU;
    fn matches_command(&self, command: &str, escape_char: Option<EscapeChar>) -> bool;
    fn detect(
        command: &str,
        escape_char: Option<EscapeChar>,
        aliases: Option<&HashMap<SmolStr, String>>,
        ctx: &AppContext,
    ) -> Option<CLIAgent>;
}

impl CLIAgentRuntimeExt for CLIAgent {
    fn command_prefixes(&self) -> &'static [&'static str] {
        match self {
            CLIAgent::Claude => &["claude"],
            CLIAgent::Gemini => &["gemini"],
            CLIAgent::Codex => &["codex"],
            CLIAgent::Amp => &["amp"],
            CLIAgent::Droid => &["droid"],
            CLIAgent::OpenCode => &["opencode"],
            CLIAgent::Copilot => &["copilot"],
            CLIAgent::Pi => &["pi"],
            CLIAgent::OhMyPi => &["omp"],
            CLIAgent::Auggie => &["auggie"],
            CLIAgent::CursorCli => &["agent"],
            CLIAgent::Goose => &["goose"],
            CLIAgent::Hermes => &["hermes"],
            CLIAgent::Vibe => &["vibe", "vibe-acp"],
            CLIAgent::Antigravity => &["agy"],
            CLIAgent::WarpTui => &[
                "warp",
                "warp-preview",
                "warp-dev",
                "warp-tui",
                "warp-tui-oss",
                "run-tui",
            ],
            CLIAgent::Unknown => &[],
        }
    }

    fn command_prefix(&self) -> &'static str {
        self.command_prefixes().first().copied().unwrap_or_default()
    }

    fn from_harness(harness: Harness) -> Option<Self> {
        match harness {
            Harness::Oz => None,
            Harness::Claude => Some(CLIAgent::Claude),
            Harness::Gemini => Some(CLIAgent::Gemini),
            Harness::OpenCode => Some(CLIAgent::OpenCode),
            Harness::Codex => Some(CLIAgent::Codex),
            Harness::Unknown => Some(CLIAgent::Unknown),
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            CLIAgent::Claude => "Claude Code",
            CLIAgent::Gemini => "Gemini",
            CLIAgent::Codex => "Codex",
            CLIAgent::Amp => "Amp",
            CLIAgent::Droid => "Droid",
            CLIAgent::OpenCode => "OpenCode",
            CLIAgent::Copilot => "Copilot",
            CLIAgent::Pi => "Pi",
            CLIAgent::OhMyPi => "oh-my-pi",
            CLIAgent::Auggie => "Auggie",
            CLIAgent::CursorCli => "Cursor",
            CLIAgent::Goose => "Goose",
            CLIAgent::Hermes => "Hermes",
            CLIAgent::Vibe => "Mistral Vibe",
            CLIAgent::Antigravity => "Antigravity",
            CLIAgent::WarpTui => "Warp TUI",
            CLIAgent::Unknown => "CLI Agent",
        }
    }

    /// Returns the Icon for this CLI agent, or `None` for unknown/custom agents.
    fn icon(&self) -> Option<Icon> {
        match self {
            CLIAgent::Claude => Some(Icon::ClaudeLogo),
            CLIAgent::Gemini => Some(Icon::GeminiLogo),
            CLIAgent::Codex => Some(Icon::OpenAILogo),
            CLIAgent::Amp => Some(Icon::AmpLogo),
            CLIAgent::Droid => Some(Icon::DroidLogo),
            CLIAgent::OpenCode => Some(Icon::OpenCodeLogo),
            CLIAgent::Copilot => Some(Icon::CopilotLogo),
            CLIAgent::Pi => Some(Icon::PiLogo),
            CLIAgent::OhMyPi => Some(Icon::OhMyPiLogo),
            CLIAgent::Auggie => Some(Icon::AuggieLogo),
            CLIAgent::CursorCli => Some(Icon::CursorLogo),
            CLIAgent::Goose => Some(Icon::GooseLogo),
            CLIAgent::Hermes => None,
            // Vibe is recognized but ships without a brand asset. The brand color
            // still drives the toolbar tile; an `Icon::MistralLogo` can be wired
            // up in a follow-up once an officially licensed SVG is available.
            CLIAgent::Vibe => None,
            CLIAgent::Antigravity => Some(Icon::AntigravityLogo),
            CLIAgent::WarpTui => Some(Icon::Warp),
            CLIAgent::Unknown => None,
        }
    }

    /// Returns the skill providers whose skills this CLI agent can natively interpret.
    /// When the CLI agent rich input is open, only skills from these providers are shown
    /// in the slash menu. Returns an empty slice for agents with no known skills support.
    fn supported_skill_providers(&self) -> &'static [SkillProvider] {
        match self {
            CLIAgent::Claude => &[SkillProvider::Claude],
            CLIAgent::Codex => &[
                SkillProvider::Agents,
                SkillProvider::Claude,
                SkillProvider::Codex,
            ],
            CLIAgent::OpenCode => &[
                SkillProvider::OpenCode,
                SkillProvider::Agents,
                SkillProvider::Claude,
            ],
            CLIAgent::Gemini => &[SkillProvider::Agents, SkillProvider::Gemini],
            CLIAgent::Amp => &[SkillProvider::Agents],
            CLIAgent::Copilot => &[SkillProvider::Agents, SkillProvider::Copilot],
            CLIAgent::Droid => &[SkillProvider::Droid, SkillProvider::Agents],
            CLIAgent::Pi => &[SkillProvider::Agents],
            CLIAgent::OhMyPi => &[SkillProvider::Agents],
            CLIAgent::Auggie => &[SkillProvider::Agents],
            CLIAgent::CursorCli => &[SkillProvider::Agents],
            CLIAgent::Goose => &[SkillProvider::Agents],
            CLIAgent::Hermes => &[SkillProvider::Agents],
            CLIAgent::Vibe => &[SkillProvider::Agents],
            CLIAgent::Antigravity => &[],
            CLIAgent::WarpTui => &[],
            CLIAgent::Unknown => &[],
        }
    }

    fn skill_command_prefix(&self) -> &'static str {
        match self {
            CLIAgent::Codex => "$",
            _ => "/",
        }
    }

    fn supports_bash_mode(&self) -> bool {
        matches!(
            self,
            CLIAgent::Claude | CLIAgent::Codex | CLIAgent::OpenCode | CLIAgent::OhMyPi
        )
    }

    fn supports_cli_agent_footer(&self) -> bool {
        !matches!(self, CLIAgent::WarpTui)
    }

    /// Returns the brand color for this CLI agent, or `None` for unknown/custom agents.
    fn brand_color(&self) -> Option<ColorU> {
        match self {
            CLIAgent::Claude => Some(CLAUDE_ORANGE),
            CLIAgent::Gemini => Some(GEMINI_BLUE),
            CLIAgent::Codex => Some(OPENAI_COLOR),
            CLIAgent::Amp => Some(AMP_COLOR),
            CLIAgent::Droid => Some(DROID_COLOR),
            CLIAgent::OpenCode => Some(OPENCODE_COLOR),
            CLIAgent::Copilot => Some(COPILOT_COLOR),
            CLIAgent::Pi => Some(PI_COLOR),
            CLIAgent::OhMyPi => Some(PI_COLOR),
            CLIAgent::Auggie => Some(AUGGIE_COLOR),
            CLIAgent::CursorCli => Some(CURSOR_COLOR),
            CLIAgent::Goose => Some(GOOSE_COLOR),
            CLIAgent::Hermes => Some(HERMES_PURPLE),
            CLIAgent::Vibe => Some(MISTRAL_ORANGE),
            CLIAgent::Antigravity => Some(ANTIGRAVITY_COLOR),
            CLIAgent::WarpTui => Some(ColorU::black()),
            CLIAgent::Unknown => None,
        }
    }

    /// Returns the icon color to use when rendered on the brand-colored circle background.
    /// Agents with light brand colors use a dark icon for contrast.
    fn brand_icon_color(&self) -> ColorU {
        match self {
            CLIAgent::Pi
            | CLIAgent::OhMyPi
            | CLIAgent::Auggie
            | CLIAgent::Droid
            | CLIAgent::Antigravity => ColorU::new(0, 0, 0, 255),
            _ => ColorU::white(),
        }
    }

    /// Returns whether the command's executable name identifies this CLI agent.
    fn matches_command(&self, command: &str, escape_char: Option<EscapeChar>) -> bool {
        let Some(first_word) = extract_first_command(command.trim_start(), escape_char) else {
            return false;
        };
        let basename = first_word.rsplit(['/', '\\']).next().unwrap_or(&first_word);
        self.command_prefixes().contains(&basename)
    }

    /// Detects the CLI agent from a command string.
    ///
    /// When `escape_char` is provided, full shell parsing is used to skip leading
    /// env-var assignments (e.g. `FOO=1 claude`). Otherwise falls back to a simple
    /// whitespace split.
    ///
    /// If `aliases` is provided, the first word of the command will be looked up
    /// in the alias map. If found, the alias value replaces the first word to
    /// produce the resolved command used for detection.
    ///
    /// Returns `Some(CLIAgent)` if the command matches a known CLI agent, `None` otherwise.
    fn detect(
        command: &str,
        escape_char: Option<EscapeChar>,
        aliases: Option<&HashMap<SmolStr, String>>,
        ctx: &AppContext,
    ) -> Option<CLIAgent> {
        let trimmed = command.trim_start();
        let first_word = extract_first_command(trimmed, escape_char)?;

        // Resolve the full command through aliases. If the first word matches an
        // alias, replace it with the alias value to produce the resolved command.
        let resolved_command: Cow<'_, str> = aliases
            .and_then(|a| a.get(first_word.as_str()))
            .map(|alias_value| {
                let rest = trimmed
                    .find(first_word.as_str())
                    .map(|pos| &trimmed[pos + first_word.len()..])
                    .unwrap_or("");
                Cow::Owned(format!("{}{}", alias_value.trim(), rest))
            })
            .unwrap_or(Cow::Borrowed(trimmed));

        // Check if resolved command matches any known CLI agent.
        // Also matches `aifx agent run claude` as Claude for Uber employees.
        enum_iterator::all::<CLIAgent>()
            .filter(|agent| !matches!(agent, CLIAgent::Unknown))
            .find(|agent| {
                agent.matches_command(&resolved_command, escape_char)
                    || (matches!(agent, CLIAgent::Claude)
                        && is_aifx_agent_run_claude(&resolved_command, ctx))
            })
    }
}

fn extract_first_command(command: &str, escape_char: Option<EscapeChar>) -> Option<String> {
    match escape_char {
        Some(escape_char) => top_level_command(command, escape_char),
        None => command.split_whitespace().next().map(String::from),
    }
}

fn is_aifx_agent_run_claude(resolved_command: &str, ctx: &AppContext) -> bool {
    resolved_command.starts_with("aifx agent run claude")
        && UserWorkspaces::as_ref(ctx)
            .workspaces()
            .iter()
            .flat_map(|workspace| workspace.teams.iter())
            .any(|team| team.uid.uid() == UBER_TEAM_UID)
}

impl From<CLIAgent> for CLIAgentType {
    fn from(agent: CLIAgent) -> Self {
        match agent {
            CLIAgent::Claude => CLIAgentType::Claude,
            CLIAgent::Gemini => CLIAgentType::Gemini,
            CLIAgent::Codex => CLIAgentType::Codex,
            CLIAgent::Amp => CLIAgentType::Amp,
            CLIAgent::Droid => CLIAgentType::Droid,
            CLIAgent::OpenCode => CLIAgentType::OpenCode,
            CLIAgent::Copilot => CLIAgentType::Copilot,
            CLIAgent::Pi => CLIAgentType::Pi,
            CLIAgent::OhMyPi => CLIAgentType::OhMyPi,
            CLIAgent::Auggie => CLIAgentType::Auggie,
            CLIAgent::CursorCli => CLIAgentType::Cursor,
            CLIAgent::Goose => CLIAgentType::Goose,
            CLIAgent::Hermes => CLIAgentType::Hermes,
            CLIAgent::Vibe => CLIAgentType::Vibe,
            CLIAgent::Antigravity => CLIAgentType::Antigravity,
            CLIAgent::WarpTui => CLIAgentType::WarpTui,
            CLIAgent::Unknown => CLIAgentType::Unknown,
        }
    }
}

#[cfg(test)]
#[path = "cli_agent_tests.rs"]
mod tests;
