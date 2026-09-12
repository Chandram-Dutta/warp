use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp_core::features::FeatureFlag;

use super::{Availability, SlashCommandKind};
use crate::search::slash_command_menu::StaticCommand;
use crate::search::slash_command_menu::static_commands::Argument;
use crate::ui_components::color_dot;

pub static AGENT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/agent",
    description: "Start a new conversation",
    kind: SlashCommandKind::Agent,
    icon_path: "bundled/svg/warp-3.svg",
    availability: Availability::AI_ENABLED.union(Availability::NOT_CLOUD_AGENT),
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

pub static CLOUD_AGENT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/cloud-agent",
    description: "Start a new cloud agent conversation",
    kind: SlashCommandKind::CloudAgent,
    icon_path: "bundled/svg/warp-3.svg",
    availability: Availability::AI_ENABLED.union(Availability::NOT_CLOUD_AGENT),
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

pub static CREATE_ENVIRONMENT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/create-environment",
    description: "Create an Oz environment (Docker image + repos) via guided setup",
    kind: SlashCommandKind::CreateEnvironment,
    icon_path: "bundled/svg/dataflow.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: Some(
        Argument::optional()
            .with_hint_text("<optional repo paths or GitHub URLs>")
            .with_execute_on_selection(),
    ),
});

pub const CREATE_DOCKER_SANDBOX: StaticCommand = StaticCommand {
    name: "/docker-sandbox",
    description: "Create a new docker sandbox terminal session",
    kind: SlashCommandKind::CreateDockerSandbox,
    icon_path: "bundled/svg/docker.svg",
    availability: Availability::LOCAL.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: false,
    argument: None,
};

pub static CREATE_NEW_PROJECT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/create-new-project",
    description: "Have Oz walk you through creating a new coding project",
    kind: SlashCommandKind::CreateNewProject,
    icon_path: "bundled/svg/plus.svg",
    availability: Availability::LOCAL | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::required().with_hint_text("<describe what you want to build>")),
});

pub const ADD_RULE: StaticCommand = StaticCommand {
    name: "/add-rule",
    description: "Add a new global rule for the agent",
    kind: SlashCommandKind::AddRule,
    icon_path: "bundled/svg/book-open.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
};

pub static RENAME_TAB: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/rename-tab",
    description: "Rename the current tab",
    kind: SlashCommandKind::RenameTab,
    icon_path: "bundled/svg/pencil-line.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text("<tab name>")),
});

pub static RENAME_CONVERSATION: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/rename-conversation",
    description: "Rename the current conversation",
    kind: SlashCommandKind::RenameConversation,
    icon_path: "bundled/svg/pencil-line.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text("<new title>")),
});

static SET_TAB_COLOR_HINT: LazyLock<String> = LazyLock::new(|| {
    let mut hint = String::from("<");
    for color in color_dot::TAB_COLOR_OPTIONS {
        hint.push_str(&color.to_string().to_ascii_lowercase());
        hint.push('|');
    }
    hint.push_str("none>");
    hint
});

pub static SET_TAB_COLOR: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/set-tab-color",
    description: "Set the color of the current tab",
    kind: SlashCommandKind::SetTabColor,
    icon_path: "bundled/svg/ellipse.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text(SET_TAB_COLOR_HINT.as_str())),
});

pub static FORK: LazyLock<StaticCommand> = LazyLock::new(|| {
    let hint_text = "<optional prompt to send in forked conversation>";
    StaticCommand {
        name: "/fork",
        description: "Fork the current conversation",
        kind: SlashCommandKind::Fork,
        icon_path: "bundled/svg/arrow-split.svg",
        availability: Availability::AGENT_VIEW
            | Availability::ACTIVE_CONVERSATION
            | Availability::NO_LRC_CONTROL
            | Availability::AI_ENABLED,
        auto_enter_ai_mode: true,
        argument: Some(Argument::optional().with_hint_text(hint_text)),
    }
});

pub static MOVE_TO_CLOUD: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/handoff",
    description: "Hand off this conversation to a cloud agent",
    kind: SlashCommandKind::MoveToCloud,
    icon_path: "bundled/svg/upload-cloud-01.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::AI_ENABLED
        | Availability::NOT_CLOUD_AGENT,
    auto_enter_ai_mode: false,
    argument: Some(
        Argument::optional()
            .with_hint_text("<optional follow-up prompt>")
            .with_execute_on_selection(),
    ),
});

pub const INIT: StaticCommand = StaticCommand {
    name: "/init",
    description: "Index this codebase and generate an AGENTS.md file",
    kind: SlashCommandKind::Init,
    icon_path: "bundled/svg/warp-2.svg",
    availability: Availability::REPOSITORY
        .union(Availability::AGENT_VIEW)
        .union(Availability::AI_ENABLED),
    auto_enter_ai_mode: true,
    argument: None,
};

pub const OPEN_SETTINGS_FILE: StaticCommand = StaticCommand {
    name: "/open-settings-file",
    description: "Open settings file (TOML)",
    kind: SlashCommandKind::OpenSettingsFile,
    icon_path: "bundled/svg/file-code-02.svg",
    availability: Availability::LOCAL,
    auto_enter_ai_mode: false,
    argument: None,
};

pub const CHANGELOG: StaticCommand = StaticCommand {
    name: "/changelog",
    description: "Open the latest changelog",
    kind: SlashCommandKind::Changelog,
    icon_path: "bundled/svg/book-open.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
};

// Accepts an optional argument so that buffers like `/feedback some text` still parse to
// this command (the trailing text is ignored on execution). Without this, typing any
// argument after `/feedback` would fall through and be treated as plain input.
pub static FEEDBACK: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/feedback",
    description: "Send feedback",
    kind: SlashCommandKind::Feedback,
    icon_path: "bundled/svg/feedback.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

pub const OPEN_RULES: StaticCommand = StaticCommand {
    name: "/open-rules",
    description: "View all of your global and project rules",
    kind: SlashCommandKind::OpenRules,
    icon_path: "bundled/svg/book-open.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
};

pub static NEW: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/new",
    description: "Start a new conversation (alias for /agent)",
    kind: SlashCommandKind::New,
    icon_path: "bundled/svg/new-conversation.svg",
    availability: Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED
        | Availability::NOT_CLOUD_AGENT,
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

pub const PLAN_NAME: &str = "/plan";

pub static PLAN: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: PLAN_NAME,
    description: "Prompt the agent to do some research and create a plan for a task",
    kind: SlashCommandKind::Plan,
    icon_path: "bundled/svg/file-06.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text("<describe your task>")),
});

pub const ORCHESTRATE_NAME: &str = "/orchestrate";

pub static ORCHESTRATE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: ORCHESTRATE_NAME,
    description: "Break a task into subtasks and run them in parallel with multiple agents",
    kind: SlashCommandKind::Orchestrate,
    icon_path: "bundled/svg/warp-3.svg",
    availability: Availability::LOCAL | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text("<describe your task>")),
});

/// If `query` starts with the given command `name` followed by a space,
/// returns the remainder of the query. Otherwise returns `None`.
pub fn strip_command_prefix(query: &str, name: &str) -> Option<String> {
    query
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(' '))
        .map(|rest| rest.to_string())
}

pub static COMPACT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/compact",
    description: "Free up context by summarizing convo history",
    kind: SlashCommandKind::Compact,
    icon_path: "bundled/svg/collapse_content.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED
        | Availability::NOT_CLOUD_AGENT,
    auto_enter_ai_mode: true,
    argument: Some(
        Argument::optional().with_hint_text("<optional custom summarization instructions>"),
    ),
});

pub static COMPACT_AND: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/compact-and",
    description: "Compact conversation and then send a follow-up prompt",
    kind: SlashCommandKind::CompactAnd,
    icon_path: "bundled/svg/collapse_content.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED
        | Availability::NOT_CLOUD_AGENT,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text("<prompt to send after compaction>")),
});

pub static FORK_AND_COMPACT: LazyLock<StaticCommand> = LazyLock::new(|| {
    let hint_text = "<optional prompt to send after compaction>";
    StaticCommand {
        name: "/fork-and-compact",
        description: "Fork current conversation and compact it in the forked copy",
        kind: SlashCommandKind::ForkAndCompact,
        icon_path: "bundled/svg/fork_and_compact.svg",
        availability: Availability::AGENT_VIEW
            | Availability::ACTIVE_CONVERSATION
            | Availability::NO_LRC_CONTROL
            | Availability::AI_ENABLED
            | Availability::NOT_CLOUD_AGENT,
        auto_enter_ai_mode: true,
        argument: Some(Argument::optional().with_hint_text(hint_text)),
    }
});

pub static CONTINUE_LOCALLY: LazyLock<StaticCommand> = LazyLock::new(|| {
    let hint_text = "<optional prompt to send in local conversation>";
    StaticCommand {
        name: "/continue-locally",
        description: "Continue this cloud conversation locally",
        kind: SlashCommandKind::ContinueLocally,
        icon_path: "bundled/svg/arrow-split.svg",
        availability: Availability::AGENT_VIEW
            | Availability::ACTIVE_CONVERSATION
            | Availability::AI_ENABLED
            | Availability::CLOUD_AGENT,
        auto_enter_ai_mode: true,
        argument: Some(Argument::optional().with_hint_text(hint_text)),
    }
});

pub const USAGE: StaticCommand = StaticCommand {
    name: "/usage",
    description: "View account and credit usage",
    kind: SlashCommandKind::Usage,
    icon_path: "bundled/svg/bar-chart-04.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
};

pub const REMOTE_CONTROL: StaticCommand = StaticCommand {
    name: "/remote-control",
    description: "Start remote control for this session",
    kind: SlashCommandKind::RemoteControl,
    icon_path: "bundled/svg/phone-01.svg",
    availability: Availability::AI_ENABLED.union(Availability::NOT_CLOUD_AGENT),
    auto_enter_ai_mode: false,
    argument: None,
};

pub const COST: StaticCommand = StaticCommand {
    name: "/cost",
    description: "Toggle credit usage details",
    kind: SlashCommandKind::Cost,
    icon_path: "bundled/svg/bar-chart-04.svg",
    availability: Availability::AGENT_VIEW
        .union(Availability::AI_ENABLED)
        .union(Availability::NOT_CLOUD_AGENT),
    auto_enter_ai_mode: false,
    argument: None,
};

pub const EXPORT_TO_CLIPBOARD: StaticCommand = StaticCommand {
    name: "/export-to-clipboard",
    description: "Export current conversation to clipboard in markdown format",
    kind: SlashCommandKind::ExportToClipboard,
    icon_path: "bundled/svg/copy.svg",
    availability: Availability::AGENT_VIEW
        .union(Availability::AI_ENABLED)
        .union(Availability::NOT_CLOUD_AGENT),
    auto_enter_ai_mode: true,
    argument: None,
};

pub static EXPORT_TO_FILE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/export-to-file",
    description: "Export current conversation to a markdown file",
    kind: SlashCommandKind::ExportToFile,
    icon_path: "bundled/svg/download-01.svg",
    availability: Availability::AGENT_VIEW
        | Availability::AI_ENABLED
        | Availability::NOT_CLOUD_AGENT,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text("<optional filename>")),
});

pub const COPY_DEBUGGING_ID: StaticCommand = StaticCommand {
    name: "/copy-debugging-id",
    description: "Copy debugging information for this conversation",
    kind: SlashCommandKind::CopyDebuggingId,
    icon_path: "bundled/svg/copy.svg",
    availability: Availability::ACTIVE_CONVERSATION,
    auto_enter_ai_mode: false,
    argument: None,
};

pub static COMMAND_REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

/// A unique identifier for a static slash command.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct SlashCommandId(Uuid);

impl SlashCommandId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SlashCommandId {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Registry {
    commands: HashMap<SlashCommandId, StaticCommand>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        let mut commands = HashMap::new();
        for command in all_commands() {
            debug_assert!(
                !command
                    .availability
                    .contains(Availability::TERMINAL_VIEW | Availability::AGENT_VIEW),
                "command `{}` sets both TERMINAL_VIEW and AGENT_VIEW, which is unsatisfiable",
                command.name,
            );
            commands.insert(SlashCommandId::new(), command);
        }
        Self { commands }
    }

    pub fn all_commands_by_id(&self) -> impl Iterator<Item = (SlashCommandId, &StaticCommand)> {
        self.commands.iter().map(|(id, cmd)| (*id, cmd))
    }

    pub fn all_commands(&self) -> impl Iterator<Item = &StaticCommand> {
        self.commands.values()
    }

    pub fn get_command(&self, id: &SlashCommandId) -> Option<&StaticCommand> {
        self.commands.get(id)
    }

    pub fn get_command_with_name(&self, name: &str) -> Option<&StaticCommand> {
        self.commands.values().find(|command| command.name == name)
    }

    #[cfg(test)]
    pub fn get_command_id_with_name(&self, name: &str) -> Option<&SlashCommandId> {
        self.commands
            .iter()
            .find(|(_, command)| command.name == name)
            .map(|(id, _)| id)
    }
}

fn all_commands() -> Vec<StaticCommand> {
    let mut commands = vec![
        ADD_RULE,
        COST,
        FEEDBACK.clone(),
        INIT,
        OPEN_RULES,
        AGENT.clone(),
        NEW.clone(),
        PLAN.clone(),
        RENAME_CONVERSATION.clone(),
        RENAME_TAB.clone(),
        SET_TAB_COLOR.clone(),
        USAGE,
        EXPORT_TO_CLIPBOARD,
        COPY_DEBUGGING_ID,
    ];

    if FeatureFlag::LocalDockerSandbox.is_enabled() {
        commands.push(CREATE_DOCKER_SANDBOX);
    }

    if FeatureFlag::CreatingSharedSessions.is_enabled()
        && FeatureFlag::HOARemoteControl.is_enabled()
    {
        commands.push(REMOTE_CONTROL);
    }

    if FeatureFlag::Changelog.is_enabled() {
        commands.push(CHANGELOG);
    }

    if FeatureFlag::CreateEnvironmentSlashCommand.is_enabled() {
        commands.push(CREATE_ENVIRONMENT.clone());
    }

    if FeatureFlag::CreateProjectFlow.is_enabled() {
        commands.push(CREATE_NEW_PROJECT.clone());
    }

    if FeatureFlag::SummarizationConversationCommand.is_enabled() {
        commands.push(COMPACT.clone());
        commands.push(COMPACT_AND.clone());
    }

    if !cfg!(target_family = "wasm") {
        commands.extend([
            FORK.clone(),
            FORK_AND_COMPACT.clone(),
            CONTINUE_LOCALLY.clone(),
        ]);
    }

    if !cfg!(target_family = "wasm") {
        commands.push(EXPORT_TO_FILE.clone());
    }

    if FeatureFlag::CloudMode.is_enabled() && FeatureFlag::CloudModeFromLocalSession.is_enabled() {
        commands.push(CLOUD_AGENT.clone());
    }

    if FeatureFlag::OzHandoff.is_enabled()
        && FeatureFlag::HandoffLocalCloud.is_enabled()
        && cfg!(all(feature = "local_fs", not(target_family = "wasm")))
    {
        commands.push(MOVE_TO_CLOUD.clone());
    }

    commands.push(ORCHESTRATE.clone());

    if FeatureFlag::SettingsFile.is_enabled() && cfg!(feature = "local_fs") {
        commands.push(OPEN_SETTINGS_FILE);
    }

    commands
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
