pub mod buffer_model;
mod classic;
mod common;
pub mod decorations;
pub mod inline_history;
pub mod inline_menu;
pub mod message_bar;
pub mod slash_commands;
mod suggestions_mode_menu;
pub mod suggestions_mode_model;
mod terminal_message_bar;

use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use async_channel::Sender;
use base64::Engine as _;
#[cfg(feature = "local_fs")]
use diesel::SqliteConnection;
use futures::FutureExt as _;
use futures::stream::AbortHandle;
use itertools::Itertools;
use lazy_static::lazy_static;
use ordered_float::Float;
use parking_lot::FairMutex;
#[cfg(feature = "local_fs")]
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use session_sharing_protocol::common::{ParticipantId, ServerConversationToken};
use settings::{Setting as _, ToggleableSetting};
use string_offset::{ByteOffset, CharOffset};
use vec1::Vec1;
use vim::vim::{VimHandler, VimMode};
use warp_cli::agent::Harness;
use warp_completer::completer::{
    self, CompleterOptions, CompletionContext, CompletionsFallbackStrategy, Description,
    ExplicitTabCompletion, MatchStrategy, MatchType, PathSeparators, PreparedSuggestion,
    SuggestionResults,
};
use warp_completer::meta::{HasSpan, Spanned};
use warp_completer::parsers::LiteCommand;
use warp_completer::parsers::simple::command_at_cursor_position;
use warp_completer::signatures::CommandRegistry;
use warp_completer::util::parse_current_commands_and_tokens;
use warp_core::r#async::debounce;
use warp_core::context_flag::ContextFlag;
use warp_core::input::NavigationKey;
use warp_core::ui::theme::AnsiColorIdentifier;
use warp_core::ui::theme::color::internal_colors;
use warp_core::user_preferences::GetUserPreferences as _;
use warp_errors::{report_error, report_if_error};
use warp_util::path::ShellFamily;
pub use warpui::WindowId;
use warpui::accessibility::{AccessibilityContent, ActionAccessibilityContent, WarpA11yRole};
#[cfg(all(feature = "local_fs", not(target_family = "wasm")))]
use warpui::r#async::FutureExt as _;
use warpui::r#async::SpawnedFutureHandle;
use warpui::clipboard::ClipboardContent;
use warpui::color::ColorU;
use warpui::elements::{
    Align, AnchorPair, ChildAnchor, Clipped, ConstrainedBox, Container, CornerRadius,
    CrossAxisAlignment, DispatchEventResult, DropTargetData, Element, EventHandler, Flex,
    MainAxisAlignment, MainAxisSize, MouseStateHandle, OffsetPositioning, OffsetType, ParentAnchor,
    ParentElement, PositionedElementOffsetBounds, PositioningAxis, Radius, ResizableStateHandle,
    SavePosition, SelectionHandle, Text, Wrap, XAxisAnchor, YAxisAnchor, resizable_state_handle,
};
pub use warpui::elements::{ParentElement as _, Stack};
pub use warpui::geometry::vector::{Vector2F, vec2f};
use warpui::keymap::{BindingDescription, EditableBinding, FixedBinding, Keystroke};
use warpui::platform::OperatingSystem;
use warpui::presenter::ChildView;
use warpui::text_layout::TextStyle;
use warpui::ui_components::chip::Chip;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::units::IntoPixels;
use warpui::{
    AppContext, Entity, EntityId, FocusContext, ModelAsRef, ModelHandle, SingletonEntity,
    TypedActionView, View, ViewContext, ViewHandle, WeakViewHandle, end_trace, start_trace,
};

use self::decorations::InputBackgroundJobOptions;
use super::alias::is_expandable_alias;
use super::block_list_viewport::InputMode;
use super::event::{BlockCompletedEvent, BlockType, UserBlockCompleted};
use super::ligature_settings::LigatureSettings;
use super::model::block::{
    AgentInteractionMetadata, BlockId, BlockMetadata, BlocklistEnvVarMetadata,
};
use super::model::session::{Session, SessionId, SessionType, Sessions};
use super::prompt_render_helper::{
    PromptRenderHelper, SameLinePromptElements, should_render_prompt_on_same_line,
};
use super::safe_mode_settings::{
    SafeModeSettings, SafeModeSettingsChangedEvent, get_secret_obfuscation_mode,
};
use super::session_settings::{SessionSettings, SessionSettingsChangedEvent};
use super::settings::{SpacingMode, TerminalSettings, TerminalSettingsChangedEvent};
use super::shared_session::SharedSessionStatus;
use super::shared_session::presence_manager::PresenceManager;
use super::shared_session::viewer::history_model::SharedSessionHistoryModel;
use super::shell::ShellType;
use super::view::{
    ExecuteCommandEvent, PADDING_LEFT as TERMINAL_VIEW_PADDING_LEFT, SyncInputType, TerminalAction,
};
use super::warpify::SubshellSource;
use super::{History, HistoryEntry, SizeInfo, TerminalModel, prompt};
#[allow(unused_imports)]
use crate::ASSETS;
use crate::ai::AIRequestUsageModel;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{AIAgentContext, AIAgentExchangeId, ImageContext};
use crate::ai::attachment_utils::MAX_ATTACHMENT_SIZE_BYTES;
use crate::ai::block_context::BlockContext;
use crate::ai::blocklist::agent_view::{
    AgentViewController, AgentViewEntryOrigin, EphemeralMessageModel, is_in_cloud_context,
};
use crate::ai::blocklist::telemetry_banner::should_collect_ai_ugc_telemetry;
use crate::ai::blocklist::{
    BLOCK_CONTEXT_ATTACHMENT_REGEX, BlocklistAIContextEvent, BlocklistAIContextModel,
    BlocklistAIControllerEvent, BlocklistAIHistoryEvent, BlocklistAIHistoryModel,
    BlocklistAIInputEvent, BlocklistAIInputModel, DIFF_HUNK_ATTACHMENT_REGEX,
    DRIVE_OBJECT_ATTACHMENT_REGEX, InputConfig, InputType, InputTypeAutoDetectionSource,
    PendingAttachment, PendingFile,
};
use crate::ai::cloud_environments::CloudAmbientAgentEnvironment;
use crate::ai::document::ai_document_model::{AIDocumentId, AIDocumentVersion};
use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::ai::llms::LLMPreferences;
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::appearance::{Appearance, AppearanceEvent};
use crate::channel::{Channel, ChannelState};
use crate::cloud_object::model::actions::ObjectActionType;
use crate::cloud_object::model::persistence::CloudModel;
use crate::cloud_object::model::view::CloudViewModel;
use crate::cloud_object::{CloudObject, Space};
use crate::completer::SessionContext;
use crate::context_chips::display::{PromptDisplay, PromptDisplayEvent};
use crate::context_chips::display_chip::{DisplayChipConfig, PromptChipShellCommand};
use crate::context_chips::prompt_type::PromptType;
use crate::context_chips::spacing;
use crate::editor::{
    AutosuggestionLocation, AutosuggestionType, BaselinePositionComputationMethod,
    CommandXRayAnchor, CrdtOperation, CursorColors, DisplayPoint, EditOrigin, EditorAction,
    EditorDecoratorElements, EditorOptions, EditorSnapshot, EditorView, Event as EditorEvent,
    InteractionState, PathTransformerFn, PlainTextEditorViewAction, Point as BufferPoint,
    PropagateAndNoOpEscapeKey, PropagateAndNoOpNavigationKeys, PropagateHorizontalNavigationKeys,
    ReplicaId, TextColors, TextRun, default_cursor_colors, position_id_for_cached_point,
    position_id_for_cursor, position_id_for_first_cursor,
};
use crate::env_vars::EnvVarCollectionExt;
use crate::features::FeatureFlag;
use crate::input_suggestions::{
    Event as InputSuggestionsEvent, HistoryInputSuggestion, InputSuggestions,
    TabCompletionsPreselectOption,
};
use crate::network::NetworkStatus;
use crate::pane_group::PaneGroupAction;
use crate::pane_group::focus_state::PaneFocusHandle;
#[cfg(feature = "local_fs")]
use crate::persistence::{database_file_path_for_current_scope, establish_ro_connection};
use crate::prefix::longest_common_prefix;
use crate::prompt::editor_modal::OpenSource as PromptEditorOpenSource;
use crate::resource_center::{
    Tip, TipAction, TipHint, TipsCompleted, mark_feature_used_and_write_to_user_defaults,
};
use crate::search::QueryFilter;
use crate::server::cloud_objects::update_manager::UpdateManager;
use crate::server::ids::SyncId;
#[cfg(all(feature = "local_fs", not(target_family = "wasm")))]
use crate::server::server_api::ai::AttachmentInput;
use crate::server::telemetry::{
    AgentModeAutoDetectionFalsePositivePayload, AgentModeAutoDetectionSettingOrigin,
    AnonymousUserSignupEntrypoint, CommandXRayTrigger, EnvVarTelemetryMetadata,
    SlashCommandAcceptedDetails, SlashMenuSource, TelemetryEvent, WorkflowTelemetryMetadata,
};
use crate::session_management::SessionNavigationPromptElements;
use crate::settings::{
    AISettings, AISettingsChangedEvent, AliasExpansionSettings, AppEditorSettings,
    AppEditorSettingsChangedEvent, InputModeSettings, InputSettings, InputSettingsChangedEvent,
    MAX_TIMES_TO_SHOW_AUTOSUGGESTION_HINT, PrivacySettings,
};
use crate::settings_view::{SettingsSection, flags};
use crate::suggestions::ignored_suggestions_model::{
    IgnoredSuggestionsModel, IgnoredSuggestionsModelEvent, SuggestionType,
};
use crate::terminal::CLIAgent;
use crate::terminal::cli_agent::CLIAgentRuntimeExt as _;
use crate::terminal::input::buffer_model::InputBufferModel;
use crate::terminal::input::inline_history::InlineHistoryMenuView;
use crate::terminal::input::inline_menu::InlineMenuPositioner;
use crate::terminal::input::suggestions_mode_model::{
    InputSuggestionsModeEvent, InputSuggestionsModeModel,
};
use crate::terminal::input::terminal_message_bar::TerminalInputMessageBar;
use crate::terminal::model::session::active_session::ActiveSession;
use crate::terminal::model::session::shell_quote_arg;
use crate::terminal::next_command_model::{
    NextCommandModel, NextCommandModelEvent, NextCommandSuggestionState, is_command_valid,
    is_next_command_enabled,
};
use crate::ui_components::blended_colors;
use crate::ui_components::icons::Icon;
use crate::user_config::WarpConfig;
use crate::util::bindings::{self, CustomAction, keybinding_name_to_normalized_string};
use crate::util::truncation::truncate_from_end;
use crate::view_components::{DismissibleToast, ToastFlavor};
use crate::voltron::{
    Voltron, VoltronEvent, VoltronFeatureView, VoltronFeatureViewHandle, VoltronFeatureViewMeta,
    VoltronItem, VoltronMetadata,
};
use crate::workflows::command_parser::{
    WorkflowArgumentIndex, WorkflowDisplayData, compute_workflow_display_data,
    compute_workflow_display_data_for_history_command,
    compute_workflow_display_data_with_overrides,
};
use crate::workflows::info_box::{
    WORKFLOW_PARAMETER_HIGHLIGHT_COLOR, WorkflowsInfoBoxViewEvent, WorkflowsMoreInfoView,
};
use crate::workflows::local_workflows::LocalWorkflows;
use crate::workflows::workflow_enum::EnumVariants;
use crate::workflows::{self, WorkflowSelectionSource, WorkflowSource, WorkflowType};
use crate::workspace::sync_inputs::SyncedInputState;
use crate::workspace::{
    CommandSearchOptions, ForkedConversationDestination, InitContent, ToastStack, WorkspaceAction,
};
use crate::workspaces::user_workspaces::{UserWorkspaces, UserWorkspacesEvent};
#[allow(unused_imports)]
use crate::{AgentModeEntrypoint, ServerApiProvider, cmd_or_ctrl_shift, send_telemetry_from_ctx};

/// Drop target data for dropping content on the [`Input`].
#[derive(Debug, Clone)]
pub struct InputDropTargetData {
    pub input_view: WeakViewHandle<Input>,
}

impl InputDropTargetData {
    fn new(input_view: WeakViewHandle<Input>) -> Self {
        Self { input_view }
    }

    pub fn weak_view_handle(&self) -> WeakViewHandle<Input> {
        self.input_view.clone()
    }
}

impl DropTargetData for InputDropTargetData {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub const DEBOUNCE_INPUT_DECORATION_PERIOD: Duration = Duration::from_millis(10);

const SHORT_CIRCUIT_HIGHLIGHTING_ACTIONS: [Option<PlainTextEditorViewAction>; 7] = [
    Some(PlainTextEditorViewAction::Space),
    Some(PlainTextEditorViewAction::NonExpandingSpace),
    Some(PlainTextEditorViewAction::Paste),
    Some(PlainTextEditorViewAction::Tab),
    Some(PlainTextEditorViewAction::AcceptCompletionSuggestion),
    Some(PlainTextEditorViewAction::CursorChanged),
    Some(PlainTextEditorViewAction::NewLine),
];

/// Border width for the line at the top of the input box in pixels
pub fn get_input_box_top_border_width() -> f32 {
    if FeatureFlag::MinimalistUI.is_enabled() {
        0.0
    } else {
        1.0
    }
}

pub const COMPLETIONS_MENU_WIDTH: f32 = 330.;
pub const OPEN_COMPLETIONS_KEYBINDING_NAME: &str = "input:open_completion_suggestions";
pub const INPUT_A11Y_LABEL: &str = "Command Input.";
pub const INPUT_A11Y_HELPER: &str = "Input your shell command, press enter to execute. Press cmd-up to navigate to output of previously executed commands. Press cmd-l to re-focus command input.";
pub const AI_COMMAND_SEARCH_HINT_TEXT: &str = "Type '#' for AI command suggestions";

/// Action name for setting input mode to agent mode
pub const SET_INPUT_MODE_AGENT_ACTION_NAME: &str = "input:set_mode_agent";

/// Action name for setting input mode to terminal mode
pub const SET_INPUT_MODE_TERMINAL_ACTION_NAME: &str = "input:set_mode_terminal";

/// Action name for setting input mode to unlocked agent mode (with natural language detection)
pub const SET_INPUT_MODE_UNLOCKED_AGENT_ACTION_NAME: &str = "input:set_mode_unlocked_agent";

/// Action name for setting input mode to unlocked terminal mode (with natural language detection)
pub const SET_INPUT_MODE_UNLOCKED_TERMINAL_ACTION_NAME: &str = "input:set_mode_unlocked_terminal";

/// The position ID used to identify the start of the replacement span for completions.
const COMPLETIONS_START_OF_REPLACEMENT_SPAN_POSITION_ID: &str =
    "start_of_completions_replacement_span";

const HISTORY_DETAILS_VIEW_WIDTH_REQUIREMENT: f32 = 1100.;

const MIN_BUFFER_LEN_TO_SHOW_COMPLETIONS_WHILE_TYPING: usize = 2;

/// If the editor buffer matches this prefix, AI input is enabled.
const AI_INPUT_PREFIX: &str = "* ";

/// If the editor buffer matches this prefix, terminal input is enabled and locked.
const TERMINAL_INPUT_PREFIX: &str = "!";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputPrefixMode {
    None,
    Shell,
}

const VIM_STATUS_BAR_BOTTOM_PADDING: f32 = 20.;

const DYNAMIC_ENUM_GENERATE_MESSAGE: &str = "Run the following command to generate variants:";
const DYNAMIC_ENUM_RUN_MESSAGE: &str = "Run command";
const DYNAMIC_ENUM_PENDING_MESSAGE: &str = "Command pending...";
const DYNAMIC_ENUM_FAILURE_MESSAGE: &str = "Command failed";
const DYNAMIC_ENUM_NO_RESULTS_MESSAGE: &str = "Command returned no results";
const DYNAMIC_ENUM_MENU_PADDING: f32 = 10.;
const DYNAMIC_ENUM_MENU_HEIGHT_OFFSET: f32 = 25.;
const DYNAMIC_ENUM_HORIZONTAL_TEXT_PADDING: f32 = 5.;

cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        const CMD_ENTER_KEYBINDING: &str = "cmd-enter";
    } else {
        // On linux and windows, the CmdEnter EditorAction is bound to ctrl-shift-enter.
        const CMD_ENTER_KEYBINDING: &str =  "ctrl-shift-enter";
    }
}

lazy_static! {
    static ref RUN_DYNAMIC_ENUM_COMMAND_KEYSTROKE: Keystroke = if OperatingSystem::get().is_mac() {
        Keystroke {
            cmd: true,
            key: "enter".to_owned(),
            ..Default::default()
        }
    } else {
        Keystroke {
            ctrl: true,
            shift: true,
            key: "enter".to_owned(),
            ..Default::default()
        }
    };
}

#[derive(PartialEq, Eq, Copy, Clone, Serialize)]
pub enum TelemetryInputSuggestionsMode {
    HistoryFuzzySearch,
    CompletionSuggestions,
    HistoryUp,
    NaturalLanguageCommandSearch,
    StaticWorkflowEnumSuggestions,
    DynamicWorkflowEnumSuggestions,
    InlineHistoryMenu,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HistorySearchMode {
    /// Prefix match commands.
    Prefix,
    /// Fuzzy match commands.
    Fuzzy,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum TabCompletionsMenuPosition {
    /// The menu should be positioned at the last cursor.
    AtLastCursor,
    /// The menu should be positioned at the first cursor.
    AtFirstCursor,
    /// The menu should be positioned at the given position.
    AtStartOfReplacementSpan,
}

impl TabCompletionsMenuPosition {
    fn to_position_id(self, editor_view_id: EntityId) -> String {
        match self {
            Self::AtLastCursor => position_id_for_cursor(editor_view_id),
            Self::AtFirstCursor => position_id_for_first_cursor(editor_view_id),
            Self::AtStartOfReplacementSpan => position_id_for_cached_point(
                editor_view_id,
                COMPLETIONS_START_OF_REPLACEMENT_SPAN_POSITION_ID,
            ),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct BufferState {
    buffer: String,
    cursor_point: Option<BufferPoint>,
}

impl BufferState {
    pub fn new(buffer: String, cursor_point: Option<BufferPoint>) -> Self {
        Self {
            buffer,
            cursor_point,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum InputSuggestionsMode {
    /// Mode used when arrow-up is pressed.
    HistoryUp {
        /// Text in the buffer when arrow-up is pressed (possibly empty).
        original_buffer: String,
        /// Cursor point when arrow-up is pressed.
        /// This is None when there are > 1 active selections when HistoryUp is invoked.
        /// TODO: eventually, we should support saving/resetting _many_ cursors rather than a single one.
        original_cursor_point: Option<BufferPoint>,
        search_mode: HistorySearchMode,
        /// The AI input mode when arrow-up is pressed.
        original_input_type: InputType,
        /// The AI input's lock status when the arrow-up is pressed.
        original_input_was_locked: bool,
    },
    CompletionSuggestions {
        /// Stores the byte index of the beginning of the text we are replacing
        replacement_start: usize,

        /// Stores the original buffer text before the user pressed TAB.
        /// Used to close the suggestions menu if the buffer_text_original is no longer in the input buffer.
        buffer_text_original: String,

        /// Stores the suggestions for the original buffer_text_original.
        /// Used to filter down results during prefix search.
        completion_results: SuggestionResults,

        /// Stores the original trigger of the completions, so that we can track whether the menu
        /// was opened automatically (AsYouType) or manually (with Tab)
        trigger: CompletionsTrigger,

        /// Where the menu should be positioned.
        menu_position: TabCompletionsMenuPosition,
    },

    StaticWorkflowEnumSuggestions {
        /// The suggested values for the workflow argument.
        suggestions: Vec<String>,

        /// Where the menu should be positioned.
        menu_position: TabCompletionsMenuPosition,

        /// The selected ranges for every instance of the argument.
        selected_ranges: Vec<Range<ByteOffset>>,

        /// Store the cursor point of the end of the first selected argument.
        cursor_point: BufferPoint,
    },

    DynamicWorkflowEnumSuggestions {
        /// The suggested values for the workflow argument.
        suggestions: Vec<String>,

        /// Where the menu should be positioned.
        menu_position: TabCompletionsMenuPosition,

        /// The selected ranges for every instance of the argument.
        selected_ranges: Vec<Range<ByteOffset>>,

        /// Store the cursor point of the end of the first selected argument.
        cursor_point: BufferPoint,

        /// Store the current state of the dynamic enum suggestions menu.
        dynamic_enum_status: DynamicEnumSuggestionStatus,

        /// The command associated with the dynamic enum.
        command: String,
    },

    /// Inline history menu mode for selecting commands and conversations from history.
    InlineHistoryMenu {
        original_input_config: Option<InputConfig>,
    },

    /// Mode indicating that no suggestion UI is being shown.
    Closed,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DynamicEnumSuggestionStatus {
    /// When the command has not yet been approved to run on the users laptop
    Unapproved,
    /// The command is running asynchronously, but has not yet finished so we do not have suggestions to display
    Pending,
    /// The command succeeded; display suggested variants
    Success,
    /// The command failed
    Failure,
}

impl InputSuggestionsMode {
    pub fn is_visible(&self) -> bool {
        *self != InputSuggestionsMode::Closed
    }

    pub fn is_inline_menu(&self) -> bool {
        matches!(self, Self::InlineHistoryMenu { .. })
    }

    /// Whether this mode should snapshot the input buffer on open and restore it on dismiss.
    fn should_snapshot_and_restore_buffer(&self) -> bool {
        // For now this just delegates to whether the current mode is an inline menu,
        // but in the future we might build this out/add more detail here.
        self.is_inline_menu()
    }

    fn input_config_to_restore(&self) -> Option<InputConfig> {
        match self {
            Self::InlineHistoryMenu {
                original_input_config,
            } => *original_input_config,
            _ => None,
        }
    }

    fn to_telemetry_mode(&self) -> TelemetryInputSuggestionsMode {
        match *self {
            InputSuggestionsMode::HistoryUp {
                search_mode: HistorySearchMode::Prefix,
                ..
            } => TelemetryInputSuggestionsMode::HistoryUp,
            InputSuggestionsMode::HistoryUp {
                search_mode: HistorySearchMode::Fuzzy,
                ..
            } => TelemetryInputSuggestionsMode::HistoryFuzzySearch,
            InputSuggestionsMode::CompletionSuggestions { .. } => {
                TelemetryInputSuggestionsMode::CompletionSuggestions
            }
            InputSuggestionsMode::StaticWorkflowEnumSuggestions { .. } => {
                TelemetryInputSuggestionsMode::StaticWorkflowEnumSuggestions
            }
            InputSuggestionsMode::DynamicWorkflowEnumSuggestions { .. } => {
                TelemetryInputSuggestionsMode::DynamicWorkflowEnumSuggestions
            }

            InputSuggestionsMode::InlineHistoryMenu { .. } => {
                TelemetryInputSuggestionsMode::InlineHistoryMenu
            }
            InputSuggestionsMode::Closed => unreachable!(),
        }
    }
}

struct SharedSessionInputState {
    /// History model for viewers in a shared session.
    // TODO: With this current approach, the shared session history crosses
    // subshell boundaries, we'll need to make it work with our current history model
    // to ensure we show the right shell history.
    history_model: ModelHandle<SharedSessionHistoryModel>,

    // Is [`Some`] iff a command execution was requested by a shared session executor.
    pending_command_execution_request: Option<ViewerCommandExecutionRequest>,
}

struct ViewerCommandExecutionRequest {
    /// Text in buffer when command execution was requested.
    original_buffer: String,
}

/// Where a command execution request originates from.
#[derive(Clone)]
pub enum CommandExecutionSource {
    /// A non-shared command execution request from Warp AI++.
    /// Shared commands use the SharedSession variant instead.
    AI {
        /// Metadata associated with the execution.
        metadata: AgentInteractionMetadata,
    },

    /// A command execution request in a shared session (by a viewer or sharer).
    ///
    /// For a sharer, this will be processed similar to [`CommandExecutionSource::User`]
    /// except the resulting block will be annotated with the participant ID.
    ///
    /// For a viewer, this will be handled by sending the request to the sharer.
    SharedSession {
        /// The participant ID of the
        participant_id: ParticipantId,
        /// The block ID associated to the active block when
        /// the request was fired.
        block_id: BlockId,
        /// Optional AI metadata if this command was requested by the AI agent
        /// in a shared session. This is used to associate the resulting command block
        /// with the original agent command.
        ai_metadata: Option<AgentInteractionMetadata>,
    },

    /// A normal command execution request.
    User,

    EnvVarCollection {
        metadata: BlocklistEnvVarMetadata,
    },
}

impl CommandExecutionSource {
    /// Whether this command execution originates from an AI command.
    pub fn is_ai_command(&self) -> bool {
        // TODO: at some point we will want to couple both of these cases
        // into one source variant, as they are both AI sources.
        matches!(
            self,
            CommandExecutionSource::AI { .. }
                | CommandExecutionSource::SharedSession {
                    ai_metadata: Some(_),
                    ..
                }
        )
    }
}

fn render_prompt_chip_shell_command(
    command: &PromptChipShellCommand,
    shell_type: ShellType,
) -> String {
    match command {
        PromptChipShellCommand::GitCheckout { branch_name } => {
            format!("git checkout {}", shell_quote_arg(branch_name, shell_type))
        }
        PromptChipShellCommand::GitCreateAndCheckoutBranch { branch_name } => {
            format!(
                "git checkout -b {} --",
                shell_quote_arg(branch_name, shell_type)
            )
        }
        PromptChipShellCommand::ChangeDirectory { dir_name } => {
            format!("cd {}", shell_quote_arg(dir_name, shell_type))
        }
        PromptChipShellCommand::NvmUse { version } => {
            format!("nvm use {}", shell_quote_arg(version, shell_type))
        }
        PromptChipShellCommand::NvmInstallLatestNode => "nvm install node".to_string(),
        PromptChipShellCommand::Echo { message } => {
            format!("echo {}", shell_quote_arg(message, shell_type))
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum HistoryUpMode {
    // Show prefixed results.
    Prefixed,
    // Show all results with no query.
    RegularNoQuery,
    // Show all results with query.
    RegularWithQuery,
    // Used for ConfirmSuggestion event.
    NotApplicable,
}

impl HistoryUpMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            HistoryUpMode::Prefixed => "prefixed history up",
            HistoryUpMode::RegularNoQuery => "regular history up (no query)",
            HistoryUpMode::RegularWithQuery => "regular history up (with query)",
            HistoryUpMode::NotApplicable => "history up",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEmptyStateChangeReason {
    /// The buffer transitioned between empty and non-empty due to a regular edit.
    Edited,
    /// The buffer was cleared because a user-executed command completed and we reinitialized the
    /// buffer for the next command.
    UserCommandCompleted,
}

pub enum Event {
    AutosuggestionAccepted,
    ClearSelectedBlock,
    PageUp,
    PageDown,
    SelectRecentBlocks {
        /// Select the `count` most recent blocks.
        count: usize,
    },
    Copy,
    UnhandledModifierKeyOnEditor(Arc<String>),
    ClearSelectionsWhenShellMode,
    InputStateChanged(InputState),
    /// Emitted when the input text transitions between empty and non-empty states
    InputEmptyStateChanged {
        is_empty: bool,
        reason: InputEmptyStateChangeReason,
    },
    Escape,
    /// note: Terminal Inputs should only emit the variant
    /// SyncInputType::InputEditorContentsChanged.
    SyncInput(SyncInputType),
    ShowCommandSearch(CommandSearchOptions),
    CtrlD,
    CtrlC,
    ExecuteCommand(Box<ExecuteCommandEvent>),
    ExecuteAIQuery,
    EmacsBindingUsed,
    /// The input editor was locally edited and
    /// peers should be notified, if applicable.
    EditorUpdated {
        /// The block ID associated to the buffer that
        /// these operations were made in.
        block_id: BlockId,

        /// The CRDT-compliant operations.
        operations: Rc<Vec<CrdtOperation>>,
    },
    InputFocusedFromMiddleClick,
    EditorFocused,

    OpenSettings(SettingsSection),
    OpenFileInEditor(PathBuf),
    OpenEnvironmentManagementPane,
    ToggleAIDocumentPane {
        document_id: AIDocumentId,
        document_version: AIDocumentVersion,
    },
    OpenAIDocumentPane {
        document_id: AIDocumentId,
        document_version: AIDocumentVersion,
    },
    ShowToast {
        message: String,
        flavor: ToastFlavor,
    },

    CreateDockerSandbox,
    /// Exit cloud mode (ambient agent) and start a new *local* agent conversation in the root terminal.
    ///
    /// If `initial_prompt` is `Some`, it should prefill the local agent prompt but not auto-send.
    ExitCloudModeAndStartLocalAgent {
        initial_prompt: Option<String>,
    },
    ScrollToExchange {
        exchange_id: AIAgentExchangeId,
    },
    /// Trigger environment setup flow with optional repository arguments
    TriggerEnvironmentSetup {
        repos: Vec<String>,
    },
}

impl Event {
    pub fn is_available_in_product(&self) -> bool {
        #[cfg(feature = "local_only")]
        {
            if let Self::OpenSettings(section) = self {
                return section.is_available();
            }

            !matches!(
                self,
                Self::ExecuteAIQuery
                    | Self::OpenEnvironmentManagementPane
                    | Self::ToggleAIDocumentPane { .. }
                    | Self::OpenAIDocumentPane { .. }
                    | Self::CreateDockerSandbox
                    | Self::ExitCloudModeAndStartLocalAgent { .. }
                    | Self::ScrollToExchange { .. }
                    | Self::TriggerEnvironmentSetup { .. }
            )
        }
    }
}

pub enum InputState {
    Enabled,
    Disabled,
}

#[derive(Clone, Debug)]
pub enum InputAction {
    FocusInputBox,
    CtrlR,
    CtrlD,
    Up,
    PageUp,
    PageDown,
    ClearScreen,
    SelectAndRefreshVoltron(VoltronItem),
    /// Open the completions menu if the cursor is in a valid position to generate completion
    /// suggestions.
    MaybeOpenCompletionSuggestions,
    HideWorkflowInfoCard,

    /// If the command originates from a workflow but doesn't match the workflow template,
    /// this action resets the command to its original workflow state.
    ResetWorkflowState,

    ToggleClassicCompletionsMode,

    /// This is for toggling whether autodetection is enabled/disabled at the app-level,
    /// not for whether its enabled/disabled for the current input
    ToggleInputAutoDetection,

    /// Generate a new Next Command suggestion.
    CycleNextCommandSuggestion,

    /// Persist the completions menu width when the user resizes it.
    UpdateCompletionsMenuWidth(f32),

    /// Persist the completions menu height when the user resizes it.
    UpdateCompletionsMenuHeight(f32),

    /// Opens the inline history menu for cycling through past commands and conversations.
    OpenInlineHistoryMenu,

    /// Clears attached blocks and text selection context.
    ClearAttachedContext,
}

impl InputAction {
    pub fn is_available_in_product(&self) -> bool {
        #[cfg(feature = "local_only")]
        !matches!(
            self,
            Self::SelectAndRefreshVoltron(VoltronItem::AiCommands)
                | Self::ToggleInputAutoDetection
                | Self::ClearAttachedContext
        )
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum MenuPositioning {
    /// Position floating input menus above the input box -- corresponds
    /// to the regular blocklist.
    #[default]
    AboveInputBox,

    /// Position floating input menus below the input box -- corresponds
    /// to the inverted blocklist.
    BelowInputBox,
}

impl MenuPositioning {
    fn completion_suggestions_y_anchor(&self) -> AnchorPair<YAxisAnchor> {
        self.y_anchor()
    }

    fn history_y_anchor(&self) -> AnchorPair<YAxisAnchor> {
        self.y_anchor()
    }

    fn history_y_offset(&self) -> OffsetType {
        match *self {
            MenuPositioning::AboveInputBox => OffsetType::Pixel(0.),
            MenuPositioning::BelowInputBox => OffsetType::Pixel(-11.),
        }
    }

    fn command_xray_y_anchor(&self) -> AnchorPair<YAxisAnchor> {
        self.y_anchor()
    }

    fn workflows_info_y_anchor(&self) -> AnchorPair<YAxisAnchor> {
        self.y_anchor()
    }

    fn voltron_parent_anchor(&self) -> ParentAnchor {
        match *self {
            MenuPositioning::AboveInputBox => ParentAnchor::BottomLeft,
            MenuPositioning::BelowInputBox => ParentAnchor::TopLeft,
        }
    }

    fn voltron_child_anchor(&self) -> ChildAnchor {
        match *self {
            MenuPositioning::AboveInputBox => ChildAnchor::BottomLeft,
            MenuPositioning::BelowInputBox => ChildAnchor::TopLeft,
        }
    }

    fn voltron_offset(&self) -> Vector2F {
        match *self {
            MenuPositioning::AboveInputBox => vec2f(11., -11.),
            MenuPositioning::BelowInputBox => vec2f(11., -66.),
        }
    }

    fn y_anchor(&self) -> AnchorPair<YAxisAnchor> {
        match *self {
            MenuPositioning::AboveInputBox => {
                AnchorPair::new(YAxisAnchor::Top, YAxisAnchor::Bottom)
            }
            MenuPositioning::BelowInputBox => {
                AnchorPair::new(YAxisAnchor::Bottom, YAxisAnchor::Top)
            }
        }
    }
}

impl MenuPositioningProvider for MenuPositioning {
    fn menu_position(&self, _app: &AppContext) -> MenuPositioning {
        *self
    }
}

struct WorkflowsState {
    selected_workflow_state: Option<SelectedWorkflowState>,
}

struct EnvVarCollectionState {
    selected_env_vars: Option<SyncId>,
}

/// State when a workflow is selected.
#[derive(Clone)]
struct SelectedWorkflowState {
    /// A handle to the WorkflowsMoreInfoView shown for the selected workflow.
    ///
    /// Note that this is unconditionally constructed, even when `should_show_more_info_view` is
    /// `false`, because the `WorkflowsMoreInfoView` itself contains business logic for the state
    /// of the input editor when editing workflow arguments with the shift-tab UX. This isn't
    /// ideal, and more of a symptom of retrofitting a `WorkflowsMoreInfoView`-less version of the
    /// shift-tab UX specifically for up-arrow history.
    more_info_view: ViewHandle<WorkflowsMoreInfoView>,

    /// Map of arguments to the corresponding index of highlights. This is necessary so that we can
    /// select all instances of an argument when a user changes the selected argument.
    argument_index_to_highlight_index: HashMap<WorkflowArgumentIndex, Vec<usize>>,

    /// Map of arguments with enum variants to those variants, which are used as suggested inputs to the argument.
    argument_index_to_enum_variants: HashMap<WorkflowArgumentIndex, EnumVariants>,

    workflow_source: WorkflowSource,
    workflow_type: WorkflowType,
    workflow_selection_source: WorkflowSelectionSource,

    /// `true` if the WorkflowsMoreInfoView should be shown for the selected workflow. This is true
    /// in all cases except when a workflow-linked history command is selected from up-arrow
    /// history.
    should_show_more_info_view: bool,
}

/// Helper struct for differentiating the cases when the command is able to be
/// parsed into the workflow it originates from versus when it's been edited to
/// the point of us not being able to determine where the arguments are.
pub enum CommandMatchesWorkflowTemplate {
    Yes(WorkflowDisplayData),
    No,
}

/// Helper struct for performing alias expansion.
struct ExpansionInfo {
    /// The expanded text to replace the alias with.
    alias_value: String,
    /// The buffer text to replace the alias in.
    buffer_text: String,
    /// The byte indices that should be replaced with the alias_value.
    byte_range: Range<usize>,
}

/// For inserting last word of last command in history - by default, this is the last command but consecutive
/// inserts fetch further in history. Represents reverse index of history command to reference.
/// (insert_command_from_history_index=0 for most recent, 1 for command before it, etc.) See self.update_last_word_insertion_state()
struct LastWordInsertion {
    insert_command_from_history_index: usize,
    is_latest_editor_event: bool,
}

/// Data pertaining to the session state and history is bundled together, making
/// it accessible to other objects coupled with the same terminal session, such as a notebook.
#[derive(Clone)]
pub struct CompleterData {
    pub sessions: ModelHandle<Sessions>,
    pub active_block_metadata: Option<BlockMetadata>,
    command_registry: Arc<CommandRegistry>,
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    last_user_block_completed: Option<UserBlockCompleted>,
}

impl CompleterData {
    pub fn new(
        sessions: ModelHandle<Sessions>,
        active_block_metadata: Option<BlockMetadata>,
        command_registry: Arc<CommandRegistry>,
        last_user_block_completed: Option<UserBlockCompleted>,
    ) -> Self {
        Self {
            sessions,
            active_block_metadata,
            command_registry,
            last_user_block_completed,
        }
    }

    pub fn active_block_session_id(&self) -> Option<SessionId> {
        self.active_block_metadata
            .as_ref()
            .and_then(BlockMetadata::session_id)
    }

    pub fn completion_session_context(&self, app: &AppContext) -> Option<SessionContext> {
        let active_block_session_id = self.active_block_session_id()?;
        let current_session = self.sessions.as_ref(app).get(active_block_session_id);
        let pwd = self
            .active_block_metadata
            .as_ref()
            .and_then(BlockMetadata::current_working_directory)
            .map(str::to_owned);

        current_session.zip(pwd).map(|(current_session, pwd)| {
            // TODO(abhishek): Ideally, BlockMetadata::current_working_directory should directly
            // return a TypedPathBuf. This shouldn't happen here in the view.
            let current_working_directory =
                current_session.convert_directory_to_typed_path_buf(pwd);

            SessionContext::new(
                current_session,
                self.command_registry.clone(),
                current_working_directory,
                app,
            )
        })
    }
}

/// Autosuggestion result returned by the generator.
pub struct AutoSuggestionResult {
    /// Text in the editor buffer.
    pub buffer_text: String,
    /// Generated autosuggestion result.
    pub autosuggestion_result: Option<String>,
}

/// Views that call into the autosuggestion generation logic must implement the Autosuggester
/// trait. This requires a callback on_autosuggestion_result and functions to set and abort
/// the latest future that's been spawned for autosuggestions.
pub trait Autosuggester {
    fn on_autosuggestion_result(
        &mut self,
        _result: AutoSuggestionResult,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    fn abort_latest_autosuggestion_future(&mut self);

    fn set_autosuggestion_future(&mut self, abort_handle: AbortHandle);
}

/// Implement this trait to provide whether menus like autocomplete, voltron, etc
/// should be positionined above or below the input.
pub trait MenuPositioningProvider {
    fn menu_position(&self, app: &AppContext) -> MenuPositioning;

    fn inline_menu_position(&self, _inline_menu_height: f32, _app: &AppContext) -> MenuPositioning {
        MenuPositioning::AboveInputBox
    }
}

/// Stores state referenced by the Input view and PromptRenderHelper.
/// Note that this is largely a workaround to avoid having to pass/upgrade
/// a weak view handle from `Input` to `PromptRenderHelper` for this state.
pub struct InputRenderStateModel {
    editor_modified_since_block_finished: bool,
    // For future: we should explore reading this directly off TerminalModel.
    size_info: SizeInfo,
}

impl InputRenderStateModel {
    pub fn new(editor_modified_since_block_finished: bool, size_info: SizeInfo) -> Self {
        Self {
            editor_modified_since_block_finished,
            size_info,
        }
    }

    pub fn editor_modified_since_block_finished(&self) -> bool {
        self.editor_modified_since_block_finished
    }

    pub fn size_info(&self) -> SizeInfo {
        self.size_info
    }

    pub fn set_editor_modified_since_block_finished(
        &mut self,
        editor_modified_since_block_finished: bool,
    ) {
        self.editor_modified_since_block_finished = editor_modified_since_block_finished;
    }

    pub fn set_size_info(&mut self, size_info: SizeInfo) {
        self.size_info = size_info;
    }
}

impl Entity for InputRenderStateModel {
    type Event = ();
}

lazy_static! {
    /// Define the regex patterns that we show completions-as-you-type in AI input on.
    /// We only show file completions - as such, we match on the following patterns:
    /// 1. "/": The last word starts with a slash
    /// 2. "./": The last word starts with "./"
    /// 3. "../": The last word starts with "../"
    /// 4. "{text}/": The last word contains a slash after some text
    /// We combine all the regex patterns for performance reasons (one string scan).
    /// NOTE: this assumes Unix-style paths. When we expand to Windows, we'll want to update this!
    static ref FILEPATH_PATTERN: Regex = Regex::new(
        r"^(?:/|\.\/|\.\./|[^/]+/)"
    ).expect("Expect regex to be valid");
}

/// Returns boolean indicating whether completions-as-you-type should pop up, while in AI input.
/// This is primarily based on the last word in the buffer text, and whether it makes sense to show
/// filepath completions.
fn should_show_completions_in_ai_input(buffer_text: &str) -> bool {
    if buffer_text.ends_with(char::is_whitespace) {
        return false;
    }

    let last_word = buffer_text.split_whitespace().last();

    if let Some(last_word) = last_word {
        FILEPATH_PATTERN.is_match(last_word)
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DenyExecutionReason {
    /// Can't execute command because shell bootstrapping is still underway; shell isn't ready to
    /// execute user-supplied commands yet.
    NotBootstrapped,

    /// Can't execute command because there's an active command in control of the pty.
    ExistingActiveCommand,

    /// With the exception of shared sessions, we should only execute commands if they can be
    /// recorded in history.
    ///
    /// Gonna be honest, I (zach b) have the least amount of context on this one, don't really know
    /// why this is the case.
    ///
    /// This is not returned as a `CancellationReason::No` for shared sessions even if it may be
    /// true; we do not record shared sessions in the History model thus they are default not-
    /// appendable.
    HistoryNotAppendable,
}

impl DenyExecutionReason {
    pub fn is_existing_active_command(&self) -> bool {
        matches!(self, DenyExecutionReason::ExistingActiveCommand)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanExecuteCommand {
    Yes,
    No(DenyExecutionReason),
}

impl CanExecuteCommand {
    pub fn is_no(&self) -> bool {
        matches!(self, CanExecuteCommand::No(_))
    }
}

pub struct Input {
    model: Arc<FairMutex<TerminalModel>>,
    menu_positioning_provider: Arc<dyn MenuPositioningProvider>,
    tips_completed: ModelHandle<TipsCompleted>,
    editor: ViewHandle<EditorView>,
    input_suggestions: ViewHandle<InputSuggestions>,
    suggestions_mode_model: ModelHandle<InputSuggestionsModeModel>,
    completions_menu_resizable_width: ResizableStateHandle,
    completions_menu_resizable_height: ResizableStateHandle,
    sessions: ModelHandle<Sessions>,
    focus_handle: Option<PaneFocusHandle>,
    active_block_metadata: Option<BlockMetadata>,
    /// The [`EntityId`] of the terminal view that this input view is inside.
    terminal_view_id: EntityId,
    view_id: EntityId,
    input_render_state_model_handle: ModelHandle<InputRenderStateModel>,
    workflows_state: WorkflowsState,
    env_var_collection_state: EnvVarCollectionState,
    voltron_view: ViewHandle<Voltron>,
    is_voltron_open: bool,
    command_x_ray_description: Option<Arc<Description>>,
    last_parsed_tokens: Option<decorations::ParsedTokensSnapshot>,
    debounce_input_background_tx: Sender<InputBackgroundJobOptions>,
    /// If true, will submit the command in the editor to the shell upon receiving the
    /// precmd message.
    has_pending_command: bool,
    last_word_insertion: LastWordInsertion,

    ai_context_model: ModelHandle<BlocklistAIContextModel>,
    ai_input_model: ModelHandle<BlocklistAIInputModel>,

    /// To ensure we only have one run of completions-as-you-type at any given time,
    /// we keep an abort handle of the current run. If we have reason to start a new run
    /// (e.g. new input), we simply abort the existing run. The same applies to the
    /// syntax highlighting and autosuggestions features (all which use the completer).
    completions_abort_handle: Option<AbortHandle>,
    decorations_future_handle: Option<SpawnedFutureHandle>,
    autosuggestions_abort_handle: Option<AbortHandle>,

    pub prompt_render_helper: PromptRenderHelper,
    prompt_type: ModelHandle<PromptType>,
    // A cached copy of enable_autosuggestions from settings (to avoid
    // a settings read on every typed character).
    enable_autosuggestions_setting: bool,

    /// Manages the input state for a shared session.
    /// Is [`Some`] iff this is a viewer in a shared session.
    shared_session_input_state: Option<SharedSessionInputState>,

    /// Manages presence state for shared session.
    ///
    /// Only [`Some`] if this is a shared session.
    shared_session_presence_manager: Option<ModelHandle<PresenceManager>>,

    /// A cache of the local buffer operations for the latest instance
    /// of the input buffer. Specifically, these only include operations
    /// resulting from local changes to the buffer (not remote changes / operations).
    /// Note that the input buffer is reinstantiated every time a command is executed,
    /// while ultimately clears this set.
    ///
    /// Today, we only expect to use this with when starting
    /// a shared session.
    ///
    /// TODO (suraj): technically, we don't need the full
    /// history for _selections_; we just need the latest.
    latest_buffer_operations: Vec<CrdtOperation>,

    /// Incoming remote edits that are not yet applied
    /// because the block ID they were meant for was
    /// not active when these operations were received.
    ///
    /// When the buffer is reinstantiated, we check
    /// if any of these pending remote edits can be flushed.
    ///
    /// Today, we only expect to use this for shared session viewers.
    deferred_remote_operations: DeferredRemoteOperations,

    next_command_model: ModelHandle<NextCommandModel>,

    /// The last block that the user ran. This is used for generating autosuggestions.
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    last_user_block_completed: Option<UserBlockCompleted>,

    hoverable_handle: MouseStateHandle,

    #[cfg(feature = "local_fs")]
    conn: Option<Arc<Mutex<SqliteConnection>>>,

    terminal_input_message_bar: ViewHandle<TerminalInputMessageBar>,

    /// Inline history menu for up-arrow with conversations and commands.
    inline_history_menu_view: ViewHandle<InlineHistoryMenuView>,

    inline_terminal_menu_positioner: ModelHandle<InlineMenuPositioner>,

    /// Cached flag indicating whether the editor buffer is empty, used to track changes between
    /// empty and non-empty states.
    ///
    /// If simply looking for if the editor contents empty, check the editor view directly instead
    /// of using this flag.
    is_editor_empty_on_last_edit: bool,

    /// Weak handle to this input view for drop target data
    weak_view_handle: WeakViewHandle<Input>,

    agent_view_controller: ModelHandle<AgentViewController>,
    ephemeral_message_model: ModelHandle<EphemeralMessageModel>,

    /// When a command is executed from a prompt chip (e.g. `cd` from the directory dropdown),
    /// we snapshot the current input contents here so we can restore them after the command
    /// completes and the buffer would normally be cleared.
    input_contents_before_prompt_chip_command: Option<String>,
}

/// A map of remote buffer operations that were deferred because
/// the corresponding block ID was not active when these operations
/// were received.
struct DeferredRemoteOperations {
    /// The latest block ID that we flushed for.
    latest_block_id: BlockId,

    /// The deferred operations.
    deferred_ops: HashMap<BlockId, Vec<CrdtOperation>>,
}

impl DeferredRemoteOperations {
    fn new(latest_block_id: BlockId) -> Self {
        Self {
            latest_block_id,
            deferred_ops: HashMap::new(),
        }
    }

    /// Defers the `operations` corresponding to the `block_id`.
    fn defer(&mut self, block_id: BlockId, operations: Vec<CrdtOperation>) {
        self.deferred_ops
            .entry(block_id)
            .or_default()
            .extend(operations);
    }

    /// Removes and returns the deferred operations for the latest block ID, if any.
    fn flush(&mut self) -> Option<Vec<CrdtOperation>> {
        self.deferred_ops.remove(&self.latest_block_id)
    }
}

pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;

    if cfg!(feature = "integration_tests") {
        app.register_fixed_bindings([
            // Hack: Add explicit ctrl-r binding for integration tests, since the tests' injected
            // keypresses won't trigger Mac menu items. Unfortunately we can't use
            // cfg[test] because we are a separate process!
            FixedBinding::new(
                "ctrl-r",
                WorkspaceAction::ShowCommandSearch(Default::default()),
                id!("Input") & !id!("VoltronActive"),
            ),
        ]);
    }

    app.register_fixed_bindings(vec![
        FixedBinding::new("ctrl-d", InputAction::CtrlD, id!("Input")),
        FixedBinding::custom(
            CustomAction::History,
            InputAction::Up,
            "Show History",
            // We need to ensure the workflow info box is not open as the "up" arrow
            // key is used to navigate the environment variables dropdown.
            id!("Input")
                & !id!("IMEOpen")
                & !id!("VoltronActive")
                & !id!("WorkflowInfoBox")
                & !id!("PromptChipMenuOpen"),
        ),
    ]);

    app.register_editable_bindings([EditableBinding::new(
        "input:clear_screen",
        "Clear screen",
        InputAction::ClearScreen,
    )
    .with_context_predicate(id!("Input"))
    .with_key_binding("ctrl-l")]);

    app.register_editable_bindings([
        EditableBinding::new(
            "terminal:scroll_up_one_page",
            "Scroll terminal output up one page",
            InputAction::PageUp,
        )
        .with_context_predicate(id!("Input") & !id!("IMEOpen"))
        .with_key_binding("pageup"),
        EditableBinding::new(
            "terminal:scroll_down_one_page",
            "Scroll terminal output down one page",
            InputAction::PageDown,
        )
        .with_context_predicate(id!("Input") & !id!("IMEOpen"))
        .with_key_binding("pagedown"),
    ]);

    app.register_editable_bindings([EditableBinding::new(
        "workspace:edit_prompt",
        BindingDescription::new("Edit Prompt")
            .with_custom_description(bindings::MAC_MENUS_CONTEXT, "Edit Prompt"),
        WorkspaceAction::OpenPromptEditor {
            open_source: PromptEditorOpenSource::CommandPalette,
        },
    )
    .with_group(bindings::BindingGroup::Settings.as_str())
    .with_context_predicate(
        id!("Input")
            & id!(SharedSessionStatus::NotShared.as_keymap_context())
            & !id!("LongRunningCommand")
            & !id!(flags::ACTIVE_AGENT_VIEW)
            & !id!(flags::ACTIVE_INLINE_AGENT_VIEW),
    )]);

    if FeatureFlag::ClassicCompletions.is_enabled()
        && !FeatureFlag::ForceClassicCompletions.is_enabled()
    {
        app.register_editable_bindings([EditableBinding::new(
            "input:toggle_classic_completions_mode",
            "(Experimental) Toggle classic completions mode",
            InputAction::ToggleClassicCompletionsMode,
        )
        .with_context_predicate(id!("Input"))]);
    }

    // Register editable bindings relating to Command Search.
    app.register_editable_bindings([
        EditableBinding::new(
            "workspace:show_command_search",
            "Command Search",
            WorkspaceAction::ShowCommandSearch(Default::default()),
        )
        // Only show command search if none of the input-related panels are open, and if we aren't
        // in Vim normal mode. Command Search is ctrl-r by default, and so is Redo in Vim (in
        // normal mode). So, the child should be allowed to handle this action first. Child views
        // normally do get first precedence to handle keybindings, but this is _not_ the case when
        // a parent view binds a CustomAction, which is what is happening here in the Input view.
        // Therefore, this binding is guarded with !id!("VimNormalMode"). Note that although there
        // is usually a conflict between these, that isn't always the case if the user has
        // re-mapped CommandSearch to something else. However, we don't account for that here.
        .with_context_predicate(id!("Input") & !id!("VoltronActive") & !id!("VimNormalMode"))
        .with_custom_action(CustomAction::CommandSearch),
        EditableBinding::new(
            "input:search_command_history",
            "History Search",
            WorkspaceAction::ShowCommandSearch(CommandSearchOptions {
                filter: Some(QueryFilter::History),
                init_content: Default::default(),
            }),
        )
        .with_context_predicate(id!("Input") & !id!("VoltronActive"))
        .with_custom_action(CustomAction::HistorySearch),
        EditableBinding::new(
            OPEN_COMPLETIONS_KEYBINDING_NAME,
            "Open completions menu",
            InputAction::MaybeOpenCompletionSuggestions,
        )
        .with_context_predicate(id!("Input"))
        .with_key_binding("tab"),
    ]);

    if let Some(custom_action) = workflows::CategoriesView::custom_action() {
        app.register_editable_bindings([EditableBinding::new(
            "input:toggle_workflows",
            "Workflows",
            InputAction::SelectAndRefreshVoltron(VoltronItem::Workflows),
        )
        .with_context_predicate(id!("Input"))
        .with_custom_action(custom_action)]);
    }

    if ChannelState::channel() == Channel::Integration {
        app.register_fixed_bindings([
            // Hack: Add explicit bindings for the tests, since the tests' injected
            // keypresses won't trigger Mac menu items. Unfortunately we can't use
            // cfg[test] because we are a separate process!
            FixedBinding::new(
                "ctrl-shift-R",
                InputAction::SelectAndRefreshVoltron(VoltronItem::Workflows),
                id!("Input"),
            ),
        ]);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompletionsTrigger {
    Keybinding,
    AsYouType,
}

/// Represents whether the input editor should render the subshell flag.
#[derive(Clone, Debug)]
enum SubshellRenderState {
    /// Contains the subshell-spawning command for the flag. Render the flag
    /// and extend the flag into the input editor.
    Flag(SubshellSource),
    /// The input is inside a subshell, extend the flag into the input editor,
    /// but do not render the actual flag.
    Flagpole,
}

/// Represents whether a command is currently being executed.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Executing {
    Yes,
    No,
}

impl Input {
    pub fn send_input_buffer_to_terminal_editor(
        &mut self,
        buffer_contents: Arc<String>,
        ctx: &mut ViewContext<Self>,
    ) {
        self.editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text_for_syncing_inputs(buffer_contents, ctx);
        });
    }

    pub fn run_command_in_synced_terminal_input(&mut self, ctx: &mut ViewContext<Self>) {
        self.has_pending_command = true;
        self.execute_pending_command(ctx);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        model: Arc<FairMutex<TerminalModel>>,
        tips_completed: ModelHandle<TipsCompleted>,
        sessions: ModelHandle<Sessions>,
        size_info: SizeInfo,
        menu_positioning_provider: Arc<dyn MenuPositioningProvider>,
        current_prompt: ModelHandle<PromptType>,
        ai_context_model: ModelHandle<BlocklistAIContextModel>,
        ai_input_model: ModelHandle<BlocklistAIInputModel>,
        terminal_view_id: EntityId,
        current_repo_path: Option<PathBuf>,
        model_events: ModelHandle<crate::terminal::model_events::ModelEventDispatcher>,
        agent_view_controller: ModelHandle<AgentViewController>,
        active_session: ModelHandle<ActiveSession>,
        ephemeral_message_model: ModelHandle<EphemeralMessageModel>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        let initial_session_context = {
            let completer_data = CompleterData::new(
                sessions.clone(),
                None, // active_block_metadata will be set later when blocks are available
                CommandRegistry::global_instance(),
                None, // last_user_block_completed will be set later
            );
            completer_data.completion_session_context(ctx)
        };

        let is_shared_session_viewer = model.lock().shared_session_status().is_viewer();

        let prompt_view = ctx.add_typed_action_view(|ctx| {
            PromptDisplay::new(
                current_prompt.clone(),
                terminal_view_id,
                menu_positioning_provider.clone(),
                initial_session_context.clone(),
                current_repo_path.clone(),
                model_events.clone(),
                is_shared_session_viewer,
                ctx,
            )
        });
        ctx.subscribe_to_view(&prompt_view, |me, _, event, ctx| {
            me.handle_prompt_event(event, ctx);
        });
        ctx.subscribe_to_model(&Appearance::handle(ctx), move |me, _, event, ctx| {
            if let AppearanceEvent::ThemeChanged = event {
                me.handle_theme_change(ctx);
            }
        });
        ctx.subscribe_to_model(&TerminalSettings::handle(ctx), move |_, _, event, ctx| {
            if let TerminalSettingsChangedEvent::Spacing { .. } = event {
                ctx.notify();
            }
        });
        ctx.subscribe_to_model(&agent_view_controller, |me, _, event, ctx| {
            use crate::ai::blocklist::agent_view::AgentViewControllerEvent;
            if let AgentViewControllerEvent::EnteredAgentView { origin, .. } = event {
                me.close_suggestion_modes_for_new_conversation(ctx);
                // Entering Agent View can remove multiline same-line prompt decorator content in a
                // single render pass. Reset shrink-delay so we don't hold onto stale input height
                // for one frame (which shows up as extra bottom padding/jitter).
                me.editor.update(ctx, |editor, ctx| {
                    editor.reset_height_shrink_delay(ctx);
                });

                if *origin == AgentViewEntryOrigin::CloudAgent {
                    // By default, shared session viewers cannot edit the input - override that for composing ambient agent queries.
                    me.editor.update(ctx, |editor, ctx| {
                        editor.set_interaction_state(InteractionState::Editable, ctx);
                    });
                    me.set_zero_state_hint_text(ctx);
                }
            }
            ctx.notify();
        });

        let prompt_selection_state_handle = SelectionHandle::default();

        let view_id = ctx.view_id();

        let input_render_state_model_handle: ModelHandle<InputRenderStateModel> =
            ctx.add_model(|_| InputRenderStateModel::new(false, size_info));

        let prompt_render_helper = PromptRenderHelper::new(
            sessions.clone(),
            prompt_view,
            prompt_selection_state_handle,
            view_id,
            input_render_state_model_handle.clone(),
        );

        let next_command_model =
            ctx.add_model(|_| NextCommandModel::new(sessions.clone(), model.clone()));
        ctx.subscribe_to_model(&next_command_model, |me, _, event, ctx| {
            me.handle_next_command_model_event(event, ctx);
        });

        let editor = {
            // Clones used in render_decorator_elements closure below.
            let prompt_render_helper_clone = prompt_render_helper.clone();
            let model_clone = model.clone();
            let input_render_state_model_handle_clone = input_render_state_model_handle.clone();

            let ai_input_model = ai_input_model.clone();

            ctx.subscribe_to_model(&ai_input_model, |me, _, _, ctx| {
                #[cfg(feature = "voice_input")]
                me.update_voice_transcription_options(ctx);
                me.update_editor_input_mode(ctx);
            });

            ctx.add_typed_action_view(|ctx| {
                let options = EditorOptions {
                    autogrow: true,
                    autocomplete_symbols: true,
                    propagate_and_no_op_vertical_navigation_keys:
                        PropagateAndNoOpNavigationKeys::Always,
                    propagate_horizontal_navigation_keys:
                        PropagateHorizontalNavigationKeys::AtBoundary,
                    propagate_and_no_op_escape_key: PropagateAndNoOpEscapeKey::PropagateFirst,
                    soft_wrap: true,
                    supports_vim_mode: true,
                    use_settings_line_height_ratio: true,
                    render_decorator_elements: Some(Box::new(
                        move |app| -> EditorDecoratorElements {
                            let terminal_model = model_clone.lock();
                            let active_block = terminal_model.block_list().active_block();

                            let mut editor_decorator_elements = EditorDecoratorElements::default();

                            if should_render_prompt_on_same_line(&terminal_model, app) {
                                let SameLinePromptElements {
                                    lprompt_top,
                                    lprompt_bottom,
                                    rprompt,
                                } = prompt_render_helper_clone.render_same_line_prompt_areas(
                                    &terminal_model,
                                    Appearance::as_ref(app),
                                    app,
                                );

                                editor_decorator_elements.top_section = lprompt_top;
                                editor_decorator_elements.left_notch = lprompt_bottom;
                                editor_decorator_elements.right_notch = rprompt;
                                editor_decorator_elements.right_notch_offset_px = Some(
                                    active_block.rprompt_render_offset(
                                        &input_render_state_model_handle_clone
                                            .as_ref(app)
                                            .size_info,
                                    ),
                                )
                            }

                            editor_decorator_elements
                        },
                    )),
                    cursor_colors_fn: Box::new(default_cursor_colors),
                    baseline_position_computation_method: BaselinePositionComputationMethod::Grid,
                    // We implement middle-click paste at the [`TerminalView`] level,
                    // and we don't want to double-paste.
                    middle_click_paste: false,
                    allow_user_cursor_preference: true,
                    delegate_paste_handling: true,
                    keymap_context_modifier: Some(Box::new(|context, _| {
                        context
                            .set
                            .insert(flags::TERMINAL_INPUT_PAGE_KEYS_HANDLED_BY_INPUT);
                    })),
                    ..Default::default()
                };
                EditorView::new(options, ctx).with_next_command_model(next_command_model.clone())
            })
        };

        let buffer_model = ctx.add_model(|ctx| InputBufferModel::new(&editor, ctx));
        let suggestions_mode_model =
            ctx.add_model(|_| InputSuggestionsModeModel::new(buffer_model.clone()));

        let terminal_content_element_position_id =
            format!("terminal_content_element_{terminal_view_id}");
        let input_save_position_id = format!("status_free_input_{}", ctx.view_id());
        let window_id = ctx.window_id();
        let inline_terminal_menu_positioner = ctx.add_model(|ctx| {
            InlineMenuPositioner::new(
                &suggestions_mode_model,
                terminal_content_element_position_id,
                input_save_position_id,
                size_info,
                window_id,
                ctx,
            )
        });

        let inline_history_menu_view = ctx.add_view({
            let active_session = active_session.clone();
            let buffer_model = buffer_model.clone();
            |ctx| {
                inline_history::InlineHistoryMenuView::new(
                    active_session,
                    &suggestions_mode_model,
                    &inline_terminal_menu_positioner,
                    buffer_model,
                    ctx,
                )
            }
        });
        if FeatureFlag::InlineHistoryMenu.is_enabled() {
            ctx.subscribe_to_view(&inline_history_menu_view, |me, _, event, ctx| {
                me.handle_inline_history_menu_event(event, ctx);
            });
        }
        let inline_history_model = inline_history_menu_view.as_ref(ctx).model().clone();

        let terminal_input_message_bar = ctx.add_view(|ctx| {
            TerminalInputMessageBar::new(suggestions_mode_model.clone(), inline_history_model, ctx)
        });

        current_prompt.update(ctx, |prompt_type, ctx| {
            if let PromptType::Dynamic { prompt } = prompt_type {
                prompt.update(ctx, |current_prompt, ctx| {
                    current_prompt.subscribe_to_input_editor(editor.clone(), ctx);
                });
            }
        });

        ctx.subscribe_to_view(&editor, move |me, _, event, ctx| {
            me.handle_editor_event(event, ctx);
        });

        let input_suggestions = ctx.add_typed_action_view(InputSuggestions::new);
        ctx.subscribe_to_view(&input_suggestions, move |me, _, event, ctx| {
            me.handle_suggestions_event(event, ctx);
        });

        let app_workflows = LocalWorkflows::as_ref(ctx)
            .app_workflows()
            .cloned()
            .collect_vec();
        let local_user_workflows = WarpConfig::as_ref(ctx).local_user_workflows().clone();

        let workflows_search_view = ctx.add_typed_action_view(|ctx| {
            workflows::CategoriesView::new(local_user_workflows, app_workflows, ctx)
        });
        ctx.subscribe_to_view(&workflows_search_view, move |me, _, event, ctx| {
            me.handle_workflows_event(event, ctx);
        });

        let safe_mode_settings = SafeModeSettings::handle(ctx);
        ctx.subscribe_to_model(&safe_mode_settings, |me, _, event, ctx| {
            me.handle_safe_mode_settings_changed_event(event, ctx)
        });

        ctx.subscribe_to_model(&InputModeSettings::handle(ctx), |_, _, _, ctx| {
            ctx.notify();
        });

        let (debounce_input_background_tx, debounce_input_background_rx) =
            async_channel::unbounded();
        let _ = ctx.spawn_stream_local(
            debounce(
                DEBOUNCE_INPUT_DECORATION_PERIOD,
                debounce_input_background_rx,
            ),
            |me, mode, ctx| me.run_input_background_jobs(mode, ctx),
            |_me, _ctx| {},
        );

        let voltron_features = Vec1::new(VoltronFeatureView::new(
            VoltronItem::Workflows,
            VoltronFeatureViewHandle::Workflows(workflows_search_view.clone()),
        ));
        let voltron_view = { ctx.add_typed_action_view(|ctx| Voltron::new(voltron_features, ctx)) };
        ctx.subscribe_to_view(&voltron_view, move |me, _, event, ctx| {
            me.handle_voltron_event(event, ctx);
        });

        ctx.subscribe_to_model(&SessionSettings::handle(ctx), move |me, _, evt, ctx| {
            me.handle_session_settings_event(evt, ctx);
        });

        let editor_settings_handle = &AppEditorSettings::handle(ctx);
        ctx.subscribe_to_model(
            editor_settings_handle,
            Self::handle_app_editor_settings_event,
        );

        ctx.subscribe_to_model(&LigatureSettings::handle(ctx), |_, _, _, ctx| ctx.notify());

        let workflows_state = WorkflowsState {
            selected_workflow_state: None,
        };

        let env_var_collection_state = EnvVarCollectionState {
            selected_env_vars: None,
        };

        let last_word_insertion = LastWordInsertion {
            insert_command_from_history_index: 0,
            is_latest_editor_event: false,
        };

        ctx.subscribe_to_model(
            &InputSettings::handle(ctx),
            Self::handle_input_settings_event,
        );

        ctx.subscribe_to_model(&suggestions_mode_model, |me, _, event, ctx| {
            let InputSuggestionsModeEvent::ModeChanged {
                buffer_to_restore,
                input_config_to_restore,
            } = event;
            if let Some(buffer_state) = buffer_to_restore {
                me.restore_buffer_state(buffer_state, ctx);
            }
            if let Some(input_config) = input_config_to_restore {
                let is_buffer_empty = me.editor.as_ref(ctx).buffer_text(ctx).is_empty();
                me.ai_input_model.update(ctx, |ai_input_model, ctx| {
                    ai_input_model.set_input_config(
                        *input_config,
                        is_buffer_empty,
                        Some(InputTypeAutoDetectionSource::RestoreSavedConfig),
                        ctx,
                    );
                });
            }

            me.set_zero_state_hint_text(ctx);
            ctx.notify();
        });

        ctx.subscribe_to_model(&ai_input_model, |me, _, event, ctx| {
            let _ = me
                .debounce_input_background_tx
                .try_send(InputBackgroundJobOptions::default().with_command_decoration());

            let config = event.updated_config();
            if config.is_locked && me.suggestions_mode_model.as_ref(ctx).is_visible() {
                // Preserve certain menus when input type changes - they handle their own
                // input type transitions during navigation.
                let should_preserve_menu = me
                    .suggestions_mode_model
                    .as_ref(ctx)
                    .is_inline_history_menu();

                if !should_preserve_menu {
                    // If switching to any locked mode (not autodetect), close suggestions
                    me.close_input_suggestions(/*should_focus_input=*/ false, ctx);
                }
            }

            if config.input_type.is_ai() && !config.is_locked {
                // If command x-ray is open, hide it -- it shouldn't show in AI input mode.
                me.hide_x_ray(ctx);
            }

            me.editor.update(ctx, |editor, ctx| {
                editor.maybe_populate_intelligent_autosuggestion(ctx);
            });
            me.set_zero_state_hint_text(ctx);
            ctx.notify();
        });
        // Re-render the input's placeholder/hint text when any history event that could
        // change `selected_conversation_status_for_hint` fires — the selected conversation
        // swapping, its status changing, its title being set/unset, it becoming non-empty,
        // or its being cleared. Mirrors the set used by
        // `agent_conversation_event_affects_vertical_tabs` in `workspace/view.rs` that keeps
        // vertical tab progress indicators in sync.
        ctx.subscribe_to_model(
            &BlocklistAIHistoryModel::handle(ctx),
            move |me, _, event, ctx| {
                let affects_hint = matches!(
                    event,
                    BlocklistAIHistoryEvent::UpdatedConversationStatus { .. }
                        | BlocklistAIHistoryEvent::SetActiveConversation { .. }
                        | BlocklistAIHistoryEvent::ClearedActiveConversation { .. }
                        | BlocklistAIHistoryEvent::ClearedConversationsForTerminalSurface { .. }
                        | BlocklistAIHistoryEvent::StartedNewConversation { .. }
                        | BlocklistAIHistoryEvent::SplitConversation { .. }
                        | BlocklistAIHistoryEvent::AppendedExchange { .. }
                        | BlocklistAIHistoryEvent::UpdatedStreamingExchange { .. }
                        | BlocklistAIHistoryEvent::UpdatedConversationTitle { .. }
                        | BlocklistAIHistoryEvent::UpdatedConversationMetadata { .. }
                        | BlocklistAIHistoryEvent::RestoredConversations { .. }
                );
                if !affects_hint {
                    return;
                }
                if event.terminal_surface_id() != Some(terminal_view_id) {
                    return;
                }
                me.set_zero_state_hint_text(ctx);
                ctx.notify();
            },
        );

        ctx.subscribe_to_model(&AISettings::handle(ctx), |me, _, event, ctx| {
            me.handle_ai_settings_changed_event(event, ctx)
        });

        ctx.subscribe_to_model(
            &IgnoredSuggestionsModel::handle(ctx),
            |me, _, event, ctx| {
                me.handle_ignored_suggestions_event(event, ctx);
            },
        );

        let ai_req_usage_model = AIRequestUsageModel::handle(ctx);
        ctx.subscribe_to_model(&ai_req_usage_model, |_, _, _, ctx| {
            ctx.notify();
        });
        ctx.observe(&ai_req_usage_model, |_, _, ctx| {
            ctx.notify();
        });

        let deferred_remote_operations =
            DeferredRemoteOperations::new(model.lock().block_list().active_block_id().clone());

        // Use persisted menu sizes from settings, or fall back to defaults
        let input_settings = InputSettings::as_ref(ctx);
        let completions_menu_width = *input_settings.completions_menu_width.value();
        let completions_menu_height = *input_settings.completions_menu_height.value();

        let is_editor_empty = editor.as_ref(ctx).is_empty(ctx);
        let mut input = Self {
            input_suggestions,
            suggestions_mode_model,
            completions_menu_resizable_width: resizable_state_handle(completions_menu_width),
            completions_menu_resizable_height: resizable_state_handle(completions_menu_height),
            tips_completed,
            editor,
            model,
            sessions,
            focus_handle: None,
            active_block_metadata: None,
            view_id,
            input_render_state_model_handle,
            workflows_state,
            env_var_collection_state,
            voltron_view,
            is_voltron_open: false,
            command_x_ray_description: None,
            last_parsed_tokens: None,
            debounce_input_background_tx,
            has_pending_command: false,
            last_word_insertion,
            decorations_future_handle: None,
            autosuggestions_abort_handle: None,
            completions_abort_handle: None,
            menu_positioning_provider,
            terminal_input_message_bar,
            prompt_render_helper,
            prompt_type: current_prompt,
            ai_context_model,
            ai_input_model,
            enable_autosuggestions_setting: *editor_settings_handle
                .as_ref(ctx)
                .enable_autosuggestions,
            latest_buffer_operations: Vec::new(),
            deferred_remote_operations,
            shared_session_input_state: None,
            shared_session_presence_manager: None,
            next_command_model,
            last_user_block_completed: None,
            hoverable_handle: Default::default(),
            terminal_view_id,
            #[cfg(feature = "local_fs")]
            conn: None,
            inline_history_menu_view,
            inline_terminal_menu_positioner,

            is_editor_empty_on_last_edit: is_editor_empty,
            weak_view_handle: ctx.handle(),
            agent_view_controller,
            ephemeral_message_model,
            input_contents_before_prompt_chip_command: None,
        };

        #[cfg(feature = "local_fs")]
        if let Some(db_url) = database_file_path_for_current_scope().to_str()
            && let Ok(conn) = establish_ro_connection(db_url)
        {
            input.conn = Some(Arc::new(Mutex::new(conn)));
        }

        if input.model.lock().shared_session_status().is_viewer() {
            input.editor.update(ctx, |editor, ctx| {
                editor.set_interaction_state(InteractionState::Selectable, ctx);
            });
        } else {
            input.set_zero_state_hint_text(ctx);
        }

        #[cfg(feature = "voice_input")]
        input.update_voice_transcription_options(ctx);
        input.update_editor_input_mode(ctx);
        input
    }

    #[cfg(feature = "voice_input")]
    fn update_voice_transcription_options(&mut self, ctx: &mut ViewContext<Self>) {
        let ai_input_model = self.ai_input_model.as_ref(ctx);
        let ai_settings = AISettings::as_ref(ctx);

        let voice_transcription_options = match (
            ai_input_model.input_type(),
            ai_settings.is_voice_input_enabled(ctx),
        ) {
            (InputType::AI, true) => crate::editor::VoiceTranscriptionOptions::Enabled {
                // If UDI is enabled, we show the button below the text input
                show_button: !self.should_show_universal_developer_input(ctx)
                    && !FeatureFlag::AgentView.is_enabled(),
            },
            (InputType::Shell, true) => {
                crate::editor::VoiceTranscriptionOptions::Enabled { show_button: false }
            }
            (_, false) => crate::editor::VoiceTranscriptionOptions::Disabled,
        };

        self.editor.update(ctx, move |editor, ctx| {
            editor.update_voice_transcription_options(voice_transcription_options, ctx);
            ctx.notify();
        });
    }

    fn update_editor_input_mode(&mut self, ctx: &mut ViewContext<Self>) {
        let ai_input_model = self.ai_input_model.as_ref(ctx);
        let is_ai_input = ai_input_model.input_type().is_ai();
        self.editor.update(ctx, move |editor, ctx| {
            editor.set_is_ai_input(is_ai_input, ctx);
            ctx.notify();
        });
    }

    fn prefix_mode(&self, ctx: &AppContext) -> InputPrefixMode {
        let ai_input_model = self.ai_input_model.as_ref(ctx);
        let is_shell_active =
            ai_input_model.is_input_type_locked() && !ai_input_model.input_type().is_ai();

        if is_shell_active {
            InputPrefixMode::Shell
        } else {
            InputPrefixMode::None
        }
    }

    fn handle_inline_history_menu_event(
        &mut self,
        event: &inline_history::InlineHistoryMenuEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            inline_history::InlineHistoryMenuEvent::AcceptCommand { command, .. } => {
                if self
                    .suggestions_mode_model
                    .as_ref(ctx)
                    .is_inline_history_menu()
                {
                    self.suggestions_mode_model.update(ctx, |model, ctx| {
                        model.set_mode(InputSuggestionsMode::Closed, ctx);
                    });
                    ctx.notify();
                }
                self.editor.update(ctx, |editor, ctx| {
                    editor.set_buffer_text(command, ctx);
                });
                self.input_enter(ctx);
            }
            inline_history::InlineHistoryMenuEvent::SelectCommand {
                command,
                linked_workflow_data,
            } => {
                if let Some((workflow_type, workflow_source)) = linked_workflow_data
                    .as_ref()
                    .and_then(|linked_workflow_data| linked_workflow_data.linked_workflow(ctx))
                {
                    // TODO(ben): We should include the chosen env vars in the history
                    // entry.
                    let env_vars = workflow_type.as_workflow().default_env_vars();
                    self.insert_workflow_into_input(
                        workflow_type,
                        workflow_source,
                        WorkflowSelectionSource::UpArrowHistory,
                        None,
                        Some(command),
                        env_vars,
                        /*should_show_more_info_view=*/ false,
                        ctx,
                    );
                } else {
                    self.editor.update(ctx, |editor, ctx| {
                        editor.set_buffer_text_ignoring_undo(command, ctx);
                    });
                }
            }
            inline_history::InlineHistoryMenuEvent::Close => {
                if self
                    .suggestions_mode_model
                    .as_ref(ctx)
                    .is_inline_history_menu()
                {
                    self.suggestions_mode_model.update(ctx, |model, ctx| {
                        model.close_and_restore_buffer(ctx);
                    });
                    ctx.notify();
                }
            }
        }
    }

    fn restore_buffer_state(&mut self, buffer_state: &BufferState, ctx: &mut ViewContext<Self>) {
        self.editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text_ignoring_undo(&buffer_state.buffer, ctx);
            if let Some(original_cursor_point) = &buffer_state.cursor_point {
                editor.reset_selections_to_point(original_cursor_point, ctx);
            }
        });
        ctx.notify();
    }

    fn open_inline_history_menu(&mut self, ctx: &mut ViewContext<Self>) {
        if !FeatureFlag::InlineHistoryMenu.is_enabled() {
            return;
        }

        if self.prompt_render_helper.has_open_chip_menu(ctx) {
            return;
        }

        let original_input_config = self.ai_input_model.as_ref(ctx).input_config();
        self.suggestions_mode_model.update(ctx, |m, ctx| {
            m.set_mode(
                InputSuggestionsMode::InlineHistoryMenu {
                    original_input_config: Some(original_input_config),
                },
                ctx,
            );
        });

        ctx.notify();
    }

    pub fn set_shared_session_presence_manager(
        &mut self,
        presence_manager: ModelHandle<PresenceManager>,
    ) {
        self.shared_session_presence_manager = Some(presence_manager);
    }

    pub fn clear_attached_context(&mut self, ctx: &mut ViewContext<Self>) {
        self.ai_context_model.update(ctx, |model, ctx| {
            model.reset_context_to_default(ctx);
        });
        ctx.emit(Event::ClearSelectionsWhenShellMode);
    }

    pub fn ai_input_model(&self) -> &ModelHandle<BlocklistAIInputModel> {
        &self.ai_input_model
    }

    fn handle_prompt_event(&mut self, event: &PromptDisplayEvent, ctx: &mut ViewContext<Self>) {
        match event {
            PromptDisplayEvent::OpenFile(file_name) => {
                // Insert the filename into the terminal input
                self.editor.update(ctx, |editor, ctx| {
                    editor.set_buffer_text(file_name, ctx);
                });
                ctx.notify();
            }
            PromptDisplayEvent::OpenTextFileInEditor(file_name) => {
                let result = self.open_file_in_editor(file_name, ctx);
                if let Err(e) = result {
                    log::warn!("Failed to open file in editor: {e}");
                }
            }
            PromptDisplayEvent::ToggleMenu { open } => {
                if *open {
                    // Close any open input suggestion menus (history, Ctrl+R, etc.) when chip menus
                    // are opened to prevent overlapping menus in UDI
                    self.close_overlays(false, ctx);
                    ctx.notify();
                } else {
                    self.focus_input_box(ctx);
                }
            }
            PromptDisplayEvent::TryExecuteCommand(command) => {
                let Some(shell_type) = self
                    .active_session(ctx)
                    .map(|session| session.shell().shell_type())
                else {
                    log::warn!("Tried to execute prompt chip command without an active session");
                    return;
                };
                let command = render_prompt_chip_shell_command(command, shell_type);
                // Snapshot the current input so we can restore it after the command completes.
                let current_input = self.buffer_text(ctx);
                if self.try_execute_command_from_source(&command, CommandExecutionSource::User, ctx)
                {
                    if !current_input.is_empty() {
                        self.input_contents_before_prompt_chip_command = Some(current_input);
                    }
                }
            }
        }
    }

    fn open_file_in_editor(
        &mut self,
        file_name: &str,
        ctx: &mut ViewContext<Self>,
    ) -> Result<(), String> {
        let Some(session_id) = self.active_block_session_id() else {
            return Err("Tried to open file without a session id".to_string());
        };

        let Some(session) = self.sessions.as_ref(ctx).get(session_id) else {
            return Err("Tried to open file without a session".to_string());
        };

        if !session.is_local() {
            return Err("Cannot open a remote session file in a local editor".to_string());
        }

        let current_dir = self
            .active_block_metadata
            .as_ref()
            .and_then(|metadata| metadata.current_working_directory())
            .map(PathBuf::from)
            .ok_or("Failed to get current working directory".to_string())?;
        ctx.emit(Event::OpenFileInEditor(current_dir.join(file_name)));

        Ok(())
    }

    fn handle_theme_change(&mut self, ctx: &mut ViewContext<Self>) {
        if self.should_apply_decorations(ctx) {
            self.run_input_background_jobs(
                InputBackgroundJobOptions::default().with_command_decoration(),
                ctx,
            );
        }
        let text_colors = TextColors::from_appearance(Appearance::as_ref(ctx));
        self.editor.update(ctx, |editor, ctx| {
            editor.set_text_colors(text_colors, ctx);
        });
    }

    pub fn sessions<'a, A: ModelAsRef>(&self, ctx: &'a A) -> &'a Sessions {
        self.sessions.as_ref(ctx)
    }

    pub fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, ctx: &mut ViewContext<Self>) {
        self.focus_handle = Some(focus_handle.clone());
        let focus_model = focus_handle.focus_state_handle().clone();
        ctx.subscribe_to_model(&focus_model, move |me, _, event, ctx| {
            if !focus_handle.is_affected(event) {
                return;
            }

            let is_focused = focus_handle.is_focused(ctx);

            me.prompt_render_helper
                .prompt_view()
                .update(ctx, |prompt_view, ctx| {
                    prompt_view.on_pane_focus_changed(is_focused, ctx);
                });

            me.set_zero_state_hint_text(ctx);

            // Update the universal developer input button bar blurred state when focus changes
        });
    }

    fn is_pane_focused(&self, app: &AppContext) -> bool {
        // If the focus handle hasn't been set yet, assume we're not in a split pane and therefore focused.
        self.focus_handle.as_ref().is_none_or(|h| h.is_focused(app))
    }

    fn is_active_session(&self, app: &AppContext) -> bool {
        self.focus_handle
            .as_ref()
            .is_some_and(|h| h.is_active_session(app))
    }

    pub fn menu_positioning(&self, app: &AppContext) -> MenuPositioning {
        self.menu_positioning_provider.menu_position(app)
    }

    fn size_info(&self, ctx: &AppContext) -> SizeInfo {
        ctx.model(&self.input_render_state_model_handle).size_info()
    }

    pub fn set_size_info(&mut self, size_info: SizeInfo, ctx: &mut AppContext) {
        self.input_render_state_model_handle
            .update(ctx, |input_render_state_model, _| {
                input_render_state_model.set_size_info(size_info);
            });
    }

    pub fn editor(&self) -> &ViewHandle<EditorView> {
        &self.editor
    }

    pub(crate) fn ai_context_model(&self) -> &ModelHandle<BlocklistAIContextModel> {
        &self.ai_context_model
    }

    pub fn buffer_text(&self, ctx: &AppContext) -> String {
        self.editor.as_ref(ctx).buffer_text(ctx)
    }

    pub fn buffer_text_number_of_lines(&self, ctx: &AppContext) -> usize {
        self.buffer_text(ctx).lines().count()
    }

    #[cfg(feature = "integration_tests")]
    pub fn input_suggestions(&self) -> &ViewHandle<InputSuggestions> {
        &self.input_suggestions
    }

    pub fn suggestions_mode_model(&self) -> &ModelHandle<InputSuggestionsModeModel> {
        &self.suggestions_mode_model
    }

    pub fn inline_terminal_menu_positioner(&self) -> &ModelHandle<InlineMenuPositioner> {
        &self.inline_terminal_menu_positioner
    }

    pub fn completer_data(&self) -> CompleterData {
        CompleterData::new(
            self.sessions.clone(),
            self.active_block_metadata.clone(),
            CommandRegistry::global_instance(),
            self.last_user_block_completed.clone(),
        )
    }

    fn start_byte_index_of_first_selection(&self, ctx: &ViewContext<Self>) -> ByteOffset {
        self.editor
            .as_ref(ctx)
            .start_byte_index_of_first_selection(ctx)
    }

    fn handle_input_settings_event(
        &mut self,
        input_settings: ModelHandle<InputSettings>,
        event: &InputSettingsChangedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            InputSettingsChangedEvent::ShowHintText { .. } => {
                self.set_zero_state_hint_text(ctx);
                ctx.notify();
            }
            InputSettingsChangedEvent::SyntaxHighlighting { .. } => {
                if !*input_settings.as_ref(ctx).syntax_highlighting.value() {
                    self.clear_decorations(ctx);
                }
                self.run_input_background_jobs(
                    InputBackgroundJobOptions::default().with_command_decoration(),
                    ctx,
                );
            }
            InputSettingsChangedEvent::ErrorUnderliningEnabled { .. } => {
                if !*input_settings.as_ref(ctx).error_underlining.value() {
                    self.clear_decorations(ctx);
                }
                self.run_input_background_jobs(
                    InputBackgroundJobOptions::default().with_command_decoration(),
                    ctx,
                );
            }
            InputSettingsChangedEvent::InputBoxTypeSetting { .. } => {
                // Force a re-render when switching between Universal and Classic input modes
                // to ensure all UI elements update in real-time
                self.set_zero_state_hint_text(ctx);
                ctx.notify();
            }
            InputSettingsChangedEvent::CompletionsMenuWidth { .. } => {
                let new_value = *input_settings.as_ref(ctx).completions_menu_width.value();
                if let Ok(mut guard) = self.completions_menu_resizable_width.lock() {
                    guard.set_size(new_value);
                }
                ctx.notify();
            }
            InputSettingsChangedEvent::CompletionsMenuHeight { .. } => {
                let new_value = *input_settings.as_ref(ctx).completions_menu_height.value();
                if let Ok(mut guard) = self.completions_menu_resizable_height.lock() {
                    guard.set_size(new_value);
                }
                ctx.notify();
            }
            _ => {}
        }
    }

    fn handle_next_command_model_event(
        &mut self,
        event: &NextCommandModelEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            NextCommandModelEvent::NextCommandSuggestionReady => {
                let NextCommandSuggestionState::Ready { .. } =
                    self.next_command_model.as_ref(ctx).get_state()
                else {
                    return;
                };

                // If there is already an autosuggestion for some reason, don't replace it to avoid flickering.
                let editor = self.editor.as_ref(ctx);
                if editor.active_autosuggestion() {
                    return;
                }

                self.editor.update(ctx, |editor, ctx| {
                    editor.maybe_populate_intelligent_autosuggestion(ctx);
                });
            }
        }
    }

    #[cfg(feature = "voice_input")]
    pub(super) fn toggle_voice_input(
        &mut self,
        from: &voice_input::VoiceInputToggledFrom,
        ctx: &mut ViewContext<Self>,
    ) {
        self.enter_ai_mode(Some(InputTypeAutoDetectionSource::VoiceInputToggle), ctx);
        let did_start_listening = self
            .editor
            .update(ctx, |editor, ctx| editor.toggle_voice_input(from, ctx));
        if did_start_listening {
            self.focus_input_box(ctx);
        }
    }

    /// Switches to AI mode but preserves current lock state.
    fn enter_ai_mode(
        &mut self,
        decision_source: Option<InputTypeAutoDetectionSource>,
        ctx: &mut ViewContext<Self>,
    ) {
        let is_input_buffer_empty = self.editor.as_ref(ctx).buffer_text(ctx).is_empty();
        self.ai_input_model.update(ctx, |input_model, ctx| {
            let new_config = input_model.input_config().with_input_type(InputType::AI);
            input_model.set_input_config(new_config, is_input_buffer_empty, decision_source, ctx);
        });
    }

    /// Helper function to ensure agent mode when needed, using the same logic as SelectFile.
    /// This handles the transition from shell mode to agent mode while preserving lock semantics.
    /// Only forces agent mode if the user hasn't explicitly locked the mode to Shell.
    ///
    /// Pass `decision_source` to attribute the resulting input type change in NLD telemetry;
    /// callers without a meaningful source may pass `None`.
    pub fn ensure_agent_mode_for_ai_features(
        &mut self,
        should_override_shell_lock: bool,
        decision_source: Option<InputTypeAutoDetectionSource>,
        ctx: &mut ViewContext<Self>,
    ) {
        let ai_input_model = self.ai_input_model.as_ref(ctx);
        let config = ai_input_model.input_config();

        // Don't force agent mode if user has explicitly locked to Shell mode
        if (!should_override_shell_lock || FeatureFlag::AgentView.is_enabled())
            && config.is_locked
            && !config.input_type.is_ai()
        {
            return;
        }
        self.enter_ai_mode(decision_source, ctx);
    }

    fn cycle_next_command_suggestion(&mut self, ctx: &mut ViewContext<Self>) {
        self.next_command_model.update(ctx, |model, _| {
            model.cycle_next_command_suggestion();
        });
        self.editor.update(ctx, |editor, ctx| {
            editor.clear_autosuggestion(ctx);
        });
    }

    /// Predicts the next action using an AI model and past context on blocks within Warp.
    /// Populates the autosuggestion with the predicted action, if any. Otherwise, falls back to
    /// existing autosuggestion logic.
    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    fn maybe_predict_next_action_ai(
        &mut self,
        block_completed: UserBlockCompleted,
        ctx: &mut ViewContext<Self>,
    ) {
        if !is_next_command_enabled(ctx) {
            return;
        }

        // If the last block was empty, don't create any suggestions.
        // Also don't create suggestions for requested commands part of an agent mode conversation.
        if block_completed
            .command
            .get_with(|compute| {
                let model = self.model.lock();
                compute(model.block_list())
            })
            .is_empty()
            || block_completed.was_part_of_agent_interaction
        {
            return;
        }

        // If we already have an active autosuggestion (e.g. from command corrections), don't regenerate.
        let editor = self.editor.as_ref(ctx);
        if editor.active_autosuggestion() {
            return;
        }

        // We only have intelligent autosuggestions on empty buffer for now.
        if !self.buffer_text(ctx).is_empty() {
            return;
        }

        let Some(session) = self.active_session(ctx) else {
            return;
        };
        let shell_name = session.shell().shell_type().name().to_owned();
        let shell_version = session.shell().version().clone();
        let completer_data = self.completer_data();
        self.next_command_model.update(ctx, |model, ctx| {
            model.generate_next_command_suggestion(
                block_completed,
                shell_name,
                shell_version,
                completer_data,
                ctx,
            );
        });
    }

    pub fn set_zero_state_hint_text(&mut self, ctx: &mut ViewContext<Self>) {
        self.editor.update(ctx, |editor, ctx| {
            editor.clear_placeholder_text(ctx);
        });
    }

    /// Finds the start byte of the token under the given hovered point
    fn start_byte_index_at_point(
        &self,
        point: &DisplayPoint,
        ctx: &AppContext,
    ) -> Option<ByteOffset> {
        self.editor.read(ctx, |editor, ctx| {
            editor.start_byte_offset_at_point(point, ctx)
        })
    }

    fn handle_safe_mode_settings_changed_event(
        &mut self,
        event: &SafeModeSettingsChangedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            SafeModeSettingsChangedEvent::SafeModeEnabled { .. }
            | SafeModeSettingsChangedEvent::HideSecretsInBlockList { .. }
            | SafeModeSettingsChangedEvent::SecretDisplayModeSetting { .. } => {
                self.model
                    .lock()
                    .set_obfuscate_secrets(get_secret_obfuscation_mode(ctx));
            }
        }
    }

    fn handle_ai_settings_changed_event(
        &mut self,
        event: &AISettingsChangedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            AISettingsChangedEvent::AgentModeQuerySuggestionsEnabled { .. }
            | AISettingsChangedEvent::IsAnyAIEnabled { .. }
            | AISettingsChangedEvent::IsActiveAIEnabled { .. } => {
                let ai_settings = AISettings::handle(ctx);
                if !ai_settings
                    .as_ref(ctx)
                    .is_intelligent_autosuggestions_enabled(ctx)
                    && matches!(
                        self.editor.as_ref(ctx).active_autosuggestion_type(),
                        Some(AutosuggestionType::Command {
                            was_intelligent_autosuggestion: true
                        })
                    )
                {
                    self.editor.update(ctx, |editor, ctx| {
                        editor.clear_autosuggestion(ctx);
                    });
                    self.next_command_model.update(ctx, |model, _| {
                        model.clear_state();
                    });
                }
                self.set_zero_state_hint_text(ctx);

                if let AISettingsChangedEvent::IsAnyAIEnabled { .. } = event {
                    let is_input_buffer_empty = self.editor.as_ref(ctx).buffer_text(ctx).is_empty();
                    // If there is no AI enabled, ensure input is locked in command mode.
                    if !ai_settings.as_ref(ctx).is_any_ai_enabled(ctx) {
                        self.ai_input_model.update(ctx, |input_model, ctx| {
                            input_model.set_input_config(
                                InputConfig {
                                    input_type: InputType::Shell,
                                    is_locked: true,
                                },
                                is_input_buffer_empty,
                                None,
                                ctx,
                            );
                        });
                    }
                }

                ctx.notify();
            }
            AISettingsChangedEvent::AIAutoDetectionEnabled { .. }
            | AISettingsChangedEvent::NLDInTerminalEnabled { .. } => {
                // The input model handles updating the lock state via its own subscription.
                // If NLD is now enabled for the current context and the buffer is non-empty,
                // trigger autodetection on the current buffer contents.
                if self
                    .ai_input_model
                    .as_ref(ctx)
                    .should_run_input_autodetection(ctx)
                    && !self.editor.as_ref(ctx).buffer_text(ctx).is_empty()
                {
                    self.run_input_background_jobs(
                        InputBackgroundJobOptions::default().with_ai_input_detection(),
                        ctx,
                    );
                }
            }
            #[cfg(feature = "voice_input")]
            AISettingsChangedEvent::VoiceInputEnabled { .. } => {
                self.update_voice_transcription_options(ctx);
            }
            _ => {}
        }
    }

    fn handle_ignored_suggestions_event(
        &mut self,
        event: &IgnoredSuggestionsModelEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            IgnoredSuggestionsModelEvent::SuggestionIgnored => {
                // We may need to regenerate the autosuggestion if the suggestion just ignored
                // was the one suggested in the input.
                self.editor.update(ctx, |editor, ctx| {
                    editor.clear_autosuggestion(ctx);
                });
                self.maybe_generate_autosuggestion(ctx);
            }
        }
    }

    /// Returns `true` if we can query the [`History`] model for the active session.
    fn can_query_history(&self, ctx: &AppContext) -> bool {
        let model = self.model.lock();
        let Some(session_id) = model.block_list().active_block().session_id() else {
            return false;
        };

        let is_bootstrapped = model.block_list().is_bootstrapped();
        let is_history_queryable = History::as_ref(ctx).is_queryable(&session_id);

        // TODO: we should investigate why we need to check for bootstrapped here.
        // It's confusing and might actually be implied
        // (session history is only queryable if the session is bootstrapped).

        // We also return true for shared session executors since they're able to view the history
        // of a shared session without yet being hooked up to the history model.
        is_bootstrapped && (is_history_queryable || model.shared_session_status().is_executor())
    }

    /// Returns enum indicating if we can execute a command in the active session.
    ///
    /// We can only execute a command if:
    /// 1. the session is bootstrapped, because we don't want to interfere
    ///    with the PTY while bootstrapping is in progress
    /// 2. there isn't an active, long-running command (in-band commands are okay)
    /// 3. if the history for the session is appendable, because we want to
    ///    acknowledge the command in the session's history. Except when viewing
    ///    a shared session, since those sessions aren't registered in the [`History`]
    ///    model.
    fn can_execute_command(&self, ctx: &AppContext) -> CanExecuteCommand {
        let model = self.model.lock();
        let active_block = model.block_list().active_block();

        if !model.block_list().is_bootstrapped() {
            CanExecuteCommand::No(DenyExecutionReason::NotBootstrapped)
        } else if active_block.is_active_and_long_running()
            && !active_block.is_in_band_command_block()
        {
            CanExecuteCommand::No(DenyExecutionReason::ExistingActiveCommand)
        } else if !model.shared_session_status().is_executor()
            && active_block
                .session_id()
                .is_none_or(|session_id| !History::as_ref(ctx).is_appendable(&session_id))
        {
            CanExecuteCommand::No(DenyExecutionReason::HistoryNotAppendable)
        } else {
            CanExecuteCommand::Yes
        }
    }

    pub fn execute_pending_command(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.has_pending_command {
            return;
        }

        let command = self.get_command(ctx);
        if self.can_execute_command(ctx).is_no() {
            return;
        }

        self.try_execute_command(&command, ctx);
        self.has_pending_command = false;

        self.editor.update(ctx, |editor, ctx| {
            editor.set_interaction_state(InteractionState::Editable, ctx);
        });
    }

    /// Try to execute a command in the local session that was
    /// requested by a shared session participant (sharer or viewer).
    ///
    /// Returns `true` if the command was executed, `false` otherwise.
    pub fn try_execute_command_on_behalf_of_shared_session_participant(
        &mut self,
        command: &str,
        participant_id: ParticipantId,

        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let block_id = self.model.lock().block_list().active_block_id().clone();
        self.try_execute_command_from_source(
            command,
            CommandExecutionSource::SharedSession {
                participant_id,
                block_id,
                ai_metadata: None,
            },
            ctx,
        )
    }

    /// Freeze the editor and put it in a loading state.
    pub fn freeze_input_in_loading_state(&mut self, ctx: &mut ViewContext<Self>) -> String {
        let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);
        self.freeze_input_in_loading_state_with_text(&buffer_text, ctx);
        buffer_text
    }

    /// Freeze the editor and render `"{display_text} ◌"` as the loading indicator.
    fn freeze_input_in_loading_state_with_text(
        &mut self,
        buffer_text: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.editor.update(ctx, |editor, ctx| {
            // Use an ephemeral edit to show the loading state
            // and disallow edits.
            // TODO: the ◌ treatment is a stop-gap to rendering an svg
            // to the right of the buffer text.
            editor.set_buffer_text_ignoring_undo(&format!("{buffer_text} ◌"), ctx);
            editor.set_interaction_state(InteractionState::Selectable, ctx);

            // We manually set the text color to appear disabled.
            // We could use the [`InteractionState::Disabled`] interaction state
            // but that disallows text selection.
            let appearance = Appearance::as_ref(ctx);
            editor.set_text_colors(TextColors::all_hint_color(appearance), ctx);
        });
    }

    pub fn try_execute_command(&mut self, command: &str, ctx: &mut ViewContext<Self>) -> bool {
        let shared_session_status = self.model.lock().shared_session_status().clone();
        if shared_session_status.is_viewer() {
            // If this is a viewer who isn't also an executor, they should not
            // be allowed to execute commands.
            if shared_session_status.is_reader() {
                // TODO: consider showing a toast in this scenario. It should be unlikely
                // that a viewer can get here without being an executor because the main
                // caller of this API is the `enter` handler.
                log::warn!("Viewer tried to execute a command as a reader");
                return false;
            } else if shared_session_status.is_executor() {
                let original_buffer = self.freeze_input_in_loading_state(ctx);

                if let Some(shared_session_input_state) = self.shared_session_input_state.as_mut() {
                    shared_session_input_state.pending_command_execution_request =
                        Some(ViewerCommandExecutionRequest { original_buffer });
                }
            }

            // Get our own shared session participant ID.
            let Some(participant_id) = self
                .shared_session_presence_manager
                .as_ref()
                .map(|m| m.as_ref(ctx).id())
            else {
                return false;
            };
            self.try_execute_command_on_behalf_of_shared_session_participant(
                command,
                participant_id,
                ctx,
            )
        } else {
            self.try_execute_command_from_source(command, CommandExecutionSource::User, ctx)
        }
    }

    /// Executes the given command if the terminal session is in a valid state to accept and
    /// execute a command. Afterwards, ensures the workflows info menu and input suggestions menu
    /// are both closed.
    ///
    /// This will _not_ execute a command if any of the following are true:
    ///     1. The history list and/or blocklist are not yet bootstrapped.
    ///     2. The active blocklist has not yet received the precmd payload.
    ///     3. There is an active, long-running command.
    ///
    /// Returns `true` if the command was executed, `false` otherwise.
    fn try_execute_command_from_source(
        &mut self,
        command: &str,
        source: CommandExecutionSource,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if let CanExecuteCommand::No(reason) = self.can_execute_command(ctx) {
            if reason.is_existing_active_command() {
                const MAX_COMMAND_LENGTH: usize = 43;
                let truncated_command = truncate_from_end(command, MAX_COMMAND_LENGTH);

                // Block user submissions while a requested command is actively running
                let window_id = ctx.window_id();
                ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                    toast_stack.add_ephemeral_toast(
                        DismissibleToast::error(format!(
                            "Cannot run `{truncated_command}` (command already running)."
                        )),
                        window_id,
                        ctx,
                    );
                });
            }

            log::warn!("Tried to execute command but can_execute_command was false: {reason:?}");
            return false;
        }

        // Clear the auto-suggestion in the editor, so the height of
        // the input box is not inaccurate for its contents. Since we
        // we adjust the height of the long running block to be the same
        // as the height of the input box, we don't want the long
        // running block to have a lot of extra space for the frames
        // before it has any output or if it's a command that doesn't
        // have any output.
        //
        // Note that we do not clear the input box here (we do it in
        // `TerminalView` when we handle the `BlockCompleted` message
        // instead) for a similar reason. Specifically, we don't want
        // multi-line commands to have the height of the empty input
        // box because we don't want its contents to be cut off.
        //
        // If we had a zero-state autosuggestion and the user created an empty block,
        // keep the zero-state autosuggestion.
        if !command.is_empty() {
            self.editor.update(ctx, |editor, ctx| {
                editor.clear_autosuggestion(ctx);
                editor.clear_all_placeholder_text();
                ctx.notify();
            });
            self.next_command_model.update(ctx, |model, _| {
                model.clear_state();
            });
        }

        let home_dir = prompt::home_dir_for_block(
            self.model.lock().block_list().active_block(),
            self.sessions.as_ref(ctx),
        );
        self.model
            .lock()
            .block_list_mut()
            .active_block_mut()
            .set_home_dir(home_dir);

        let env_var_collection_id = self.env_var_collection_state.selected_env_vars;
        self.model
            .lock()
            .block_list_mut()
            .active_block_mut()
            .set_cloud_env_var_state(env_var_collection_id);

        // Record whether NLD was overridden (input type manually locked) at submission time.
        let nld_overridden = self.ai_input_model.as_ref(ctx).is_input_type_locked();
        self.model
            .lock()
            .block_list_mut()
            .active_block_mut()
            .set_nld_overridden(nld_overridden);

        let did_execute: bool;
        if self
            .model
            .lock()
            .block_list()
            .active_block()
            .has_received_precmd()
        {
            self.tips_completed.update(ctx, |tips, ctx| {
                mark_feature_used_and_write_to_user_defaults(
                    Tip::Hint(TipHint::CreateBlock),
                    tips,
                    ctx,
                );
                ctx.notify();
            });

            if !command.is_empty() {
                IgnoredSuggestionsModel::handle(ctx).update(ctx, |model, ctx| {
                    model.remove_ignored_suggestion(
                        command.to_string(),
                        SuggestionType::ShellCommand,
                        ctx,
                    );
                });
            }

            self.start_block_and_write_command_to_pty(command, source, ctx);
            did_execute = true;
        } else {
            // We don't want to submit the command if precmd has not
            // been received. Instead, we want the user to be aware
            // that the prompt might not be up to date.
            send_telemetry_from_ctx!(TelemetryEvent::TriedToExecuteBeforePrecmd, ctx);
            did_execute = false;
        }

        // Close the workflows info box if it was open.
        self.clear_selected_workflow(ctx);

        // Close the input suggestions menu if it was open.
        self.close_input_suggestions(/*should_focus_input=*/ false, ctx);
        did_execute
    }

    /// We locked the viewer's input when they attempted to execute a command.
    /// On failure, we must restore the editor to its original state before the attempt.
    pub fn on_execute_command_for_shared_session_participant_failure(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(shared_session_input_state) = self.shared_session_input_state.as_mut() else {
            return;
        };
        let Some(ViewerCommandExecutionRequest { original_buffer }) = shared_session_input_state
            .pending_command_execution_request
            .as_ref()
        else {
            return;
        };

        // Unfreeze the editor
        if let SharedSessionStatus::ActiveViewer { role } =
            self.model.lock().shared_session_status()
        {
            self.editor.update(ctx, |editor, ctx| {
                // Restore the original buffer and interaction state based on the viewer's role.
                editor.set_buffer_text(original_buffer, ctx);
                editor.set_interaction_state(role.into(), ctx);

                // Shared-session pending-command and cloud-followup flows can swap the editor into
                // a frozen/pending color treatment, so restore the normal palette alongside the
                // buffer + interaction state reset.
                let appearance: &Appearance = Appearance::as_ref(ctx);
                editor.set_text_colors(TextColors::from_appearance(appearance), ctx);
            });
        }
        shared_session_input_state.pending_command_execution_request = None;
    }

    /// Restores the frozen/loading visual state of the agent input for both the sharer
    /// and viewer without touching the CRDT buffer contents.
    ///
    /// Does NOT clear or reinitialize the buffer. Buffer clearing for agent prompts is
    /// handled by the sharer emitting CRDT delete operations via `system_clear_buffer`
    /// (triggered when `BlocklistAIControllerEvent::SentRequest` fires). Viewers receive
    /// those delete ops through `InputUpdated` and apply them via the normal CRDT path.
    ///
    /// For viewers, this exits the ephemeral loading state created by
    /// `freeze_input_in_loading_state`. When `is_shared_session_viewer_prompt_inflight` is true,
    /// we optimistically clear the buffer using a display-only empty ephemeral
    /// so the viewer sees an empty buffer immediately before crdt operations for actually clearing
    /// the real buffer are received from the sharer.
    ///
    /// The display-only ephemeral is safe for CRDT: when the viewer next makes an edit
    /// (materializing the ephemeral), its empty content is **discarded** — no delete ops
    /// are generated for the regular buffer's contents. The edit proceeds directly on
    /// the regular buffer (which the sharer's delete ops will have cleared by then).
    pub fn unfreeze_agent_input(
        &mut self,
        is_shared_session_viewer_prompt_inflight: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        if matches!(
            self.model.lock().shared_session_status(),
            SharedSessionStatus::ActiveViewer { .. }
        ) {
            self.editor.update(ctx, |editor, ctx| {
                if let SharedSessionStatus::ActiveViewer { role } =
                    self.model.lock().shared_session_status()
                {
                    // reinstate role for viewers
                    editor.set_interaction_state(role.into(), ctx);
                    // Exit the ephemeral loading state so the regular CRDT buffer is
                    // accessible. The sharer's delete ops (arriving via InputUpdated)
                    // will clear the regular buffer.
                    editor.exit_ephemeral_loading_state(ctx);
                    if is_shared_session_viewer_prompt_inflight {
                        // Create a display-only empty ephemeral for immediate visual
                        // feedback. This is an optimistic clear for UI purposes, without
                        // affecting the real buffer synced by crdt operations.
                        // Unlike a regular ephemeral, materializing this one
                        // discards its content instead of restoring it to the regular
                        // buffer, so no spurious CRDT delete ops are generated.
                        editor.show_display_only_empty_buffer(ctx);
                    }
                }

                let appearance: &Appearance = Appearance::as_ref(ctx);
                editor.set_text_colors(TextColors::from_appearance(appearance), ctx);
            });
        }
    }

    /// Restores a VM-down cloud follow-up after an attachment upload fails. Unlike
    /// [`Self::unfreeze_agent_input`], this path runs on a disconnected cloud pane rather than an
    /// active shared-session viewer, so it must restore the visible prompt and editable state
    /// directly.
    fn restore_cloud_followup_input_after_upload_failure(
        &mut self,
        prompt: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(prompt, ctx);
            editor.set_interaction_state(InteractionState::Editable, ctx);
            let appearance: &Appearance = Appearance::as_ref(ctx);
            editor.set_text_colors(TextColors::from_appearance(appearance), ctx);
        });
    }

    pub fn reset_after_cloud_followup_submission(&mut self, ctx: &mut ViewContext<Self>) {
        self.editor.update(ctx, |editor, ctx| {
            editor.set_interaction_state(InteractionState::Editable, ctx);
            editor.clear_buffer_and_reset_undo_stack(ctx);

            let appearance: &Appearance = Appearance::as_ref(ctx);
            editor.set_text_colors(TextColors::from_appearance(appearance), ctx);
        });
    }

    fn clear_selected_env_var_collection(&mut self) {
        self.env_var_collection_state.selected_env_vars = None;
    }

    /// Closes the workflows panel.
    fn clear_selected_workflow(&mut self, ctx: &mut ViewContext<Self>) {
        // Clear the env var state if we had one.
        self.clear_selected_env_var_collection();

        // `take()` closes the Workflows panel because the panel is only
        // rendered if `selected_workflow_state` is Some(..).
        if let Some(state) = self.workflows_state.selected_workflow_state.take() {
            self.update_workflows_info_box_expanded_setting(ctx, &state);
        }
        ctx.notify();
    }

    /// Hides the workflows panel, persisting the shift-tab UX.
    fn hide_workflows_info_box(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(state) = &mut self.workflows_state.selected_workflow_state {
            state.should_show_more_info_view = false;
        }
        if let Some(state) = self.workflows_state.selected_workflow_state.clone() {
            self.update_workflows_info_box_expanded_setting(ctx, &state);
        }
        ctx.notify();
    }

    /// Returns the starting byte index position of the last selection.
    fn start_byte_index_of_last_selection(&self, ctx: &ViewContext<Self>) -> ByteOffset {
        self.editor
            .as_ref(ctx)
            .start_byte_index_of_last_selection(ctx)
    }

    fn handle_session_settings_event(
        &mut self,
        evt: &SessionSettingsChangedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match evt {
            SessionSettingsChangedEvent::HonorPS1 { .. } => {
                let mut model = self.model.lock();
                model.set_honor_ps1(*SessionSettings::as_ref(ctx).honor_ps1);
                ctx.notify();
            }
            SessionSettingsChangedEvent::SavedPrompt { .. } => {
                self.notify_and_notify_children(ctx);
            }
            _ => {}
        }
    }

    fn handle_app_editor_settings_event(
        &mut self,
        settings: ModelHandle<AppEditorSettings>,
        evt: &AppEditorSettingsChangedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        if let AppEditorSettingsChangedEvent::EnableAutosuggestions { .. } = evt {
            let next_enable_autosuggestions_setting =
                *AppEditorSettings::as_ref(ctx).enable_autosuggestions;
            if self.enable_autosuggestions_setting && !next_enable_autosuggestions_setting {
                // Clear the active autosuggestion if autosuggestions was turned off.
                self.editor.update(ctx, |view, ctx| {
                    view.clear_autosuggestion(ctx);
                });
                ctx.notify();
            }
            // Ensure our cached copy of the enabled_autosuggestions setting
            // is up-to-date.
            self.enable_autosuggestions_setting = next_enable_autosuggestions_setting;
        }

        // The cursor and status bar may change appearance when vim mode is enabled or disabled.
        if let AppEditorSettingsChangedEvent::VimModeEnabled { .. } = evt {
            ctx.notify();
        }

        if let AppEditorSettingsChangedEvent::CursorDisplayState { .. } = evt {
            ctx.notify();
        }

        // The vim status bar should be shown and hidden immediately upon toggling.
        if settings.as_ref(ctx).vim_mode_enabled()
            && let AppEditorSettingsChangedEvent::VimStatusBar { .. } = evt
        {
            ctx.notify();
        }
    }

    pub fn set_autosuggestion(
        &mut self,
        autosuggestion: impl Into<String>,
        autosuggestion_type: AutosuggestionType,
        ctx: &mut ViewContext<Self>,
    ) {
        self.editor.update(ctx, |editor, ctx| {
            editor.set_autosuggestion(
                autosuggestion,
                AutosuggestionLocation::EndOfBuffer,
                autosuggestion_type,
                ctx,
            );
        })
    }

    fn handle_workflows_event(
        &mut self,
        event: &workflows::CategoriesViewEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            workflows::CategoriesViewEvent::Close => {
                self.focus_input_box(ctx);
                self.close_voltron(ctx);
            }
            workflows::CategoriesViewEvent::WorkflowSelected {
                workflow,
                workflow_source,
            } => {
                let workflow_id = workflow.server_id();
                let workflow_source = *workflow_source;
                let space = workflow_id.and_then(|id| {
                    CloudViewModel::as_ref(ctx)
                        .object_space(&id.to_string(), ctx)
                        .map(Into::into)
                });

                send_telemetry_from_ctx!(
                    TelemetryEvent::WorkflowSelected(WorkflowTelemetryMetadata {
                        workflow_source,
                        workflow_categories: workflow.as_workflow().tags().cloned(),
                        workflow_selection_source: WorkflowSelectionSource::Voltron,
                        workflow_id,
                        workflow_space: space,
                        enum_ids: workflow.as_workflow().get_server_enum_ids()
                    }),
                    ctx
                );

                self.show_workflows_info_box_on_workflow_selection(
                    *workflow.clone(),
                    workflow_source,
                    WorkflowSelectionSource::Voltron,
                    None,
                    ctx,
                );
                self.close_voltron(ctx);
            }
        }
    }

    fn handle_voltron_event(&mut self, event: &VoltronEvent, ctx: &mut ViewContext<Self>) {
        match event {
            VoltronEvent::Close => {
                self.close_voltron(ctx);
            }
        }
    }

    // Whether a workflow info box is open or not
    pub fn is_workflows_info_box_open(&self) -> bool {
        self.workflows_state.selected_workflow_state.is_some()
    }

    pub fn workflows_info_box_open_workflow_cloud_id(&self) -> Option<SyncId> {
        if let Some(state) = &self.workflows_state.selected_workflow_state {
            match &state.workflow_type {
                WorkflowType::Cloud(workflow) => Some(workflow.id),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn show_workflows_info_box_on_workflow_selection(
        &mut self,
        workflow_type: WorkflowType,
        workflow_source: WorkflowSource,
        workflow_selection_source: WorkflowSelectionSource,
        argument_override: Option<HashMap<String, String>>,
        ctx: &mut ViewContext<Input>,
    ) {
        // Should not show workflows info box for read-only viewers
        let should_show_more_info_view = !self.model.lock().shared_session_status().is_reader();
        let env_vars = workflow_type.as_workflow().default_env_vars();
        self.insert_workflow_into_input(
            workflow_type,
            workflow_source,
            workflow_selection_source,
            argument_override,
            None,
            env_vars,
            should_show_more_info_view,
            ctx,
        );
    }

    pub fn show_workflow_info_box_for_history_command(
        &mut self,
        history_command: &str,
        workflow_type: WorkflowType,
        workflow_source: WorkflowSource,
        workflow_selection_source: WorkflowSelectionSource,
        ctx: &mut ViewContext<Input>,
    ) {
        // Should not show workflows info box for read-only viewers
        let should_show_more_info_view = !self.model.lock().shared_session_status().is_reader();
        let env_vars = workflow_type.as_workflow().default_env_vars();
        self.insert_workflow_into_input(
            workflow_type,
            workflow_source,
            workflow_selection_source,
            None,
            Some(history_command),
            env_vars,
            should_show_more_info_view,
            ctx,
        );
    }

    /// Helper function to see if the selected history command matches the template of the workflow.
    fn command_matches_workflow_template(
        &self,
        history_command: &str,
        workflow_type: WorkflowType,
    ) -> CommandMatchesWorkflowTemplate {
        // if let Some(history_command) = history_command {
        if let Some(display_data) = compute_workflow_display_data_for_history_command(
            history_command,
            workflow_type.as_workflow(),
        ) {
            CommandMatchesWorkflowTemplate::Yes(display_data)
        } else {
            // In this case, the workflow comes from a history command but the command has been edited so
            // it no longer matches the original workflow template (e.g., a flag was added). We want
            // to treat this command as a workflow but without the argument parsing and shift-tab UX.
            CommandMatchesWorkflowTemplate::No
        }
    }

    /// Inserts the given workflow into the input editor and initiates the shift-tab workflow
    /// parameter editing "mode".
    ///
    /// If `should_show_more_info_view`, the `WorkflowsMoreInfoView` for the selected workflow is
    /// displayed above the input.
    ///
    /// If `history_command` is `Some()` _and_ matches the contained workflow in `workflow_type`,
    /// `history_command` is inserted into the input instead, with its parameters highlighted and
    /// made editable via the shift-tab UX.
    #[allow(clippy::too_many_arguments)]
    fn insert_workflow_into_input(
        &mut self,
        workflow_type: WorkflowType,
        workflow_source: WorkflowSource,
        workflow_selection_source: WorkflowSelectionSource,
        argument_overrides: Option<HashMap<String, String>>,
        history_command: Option<&str>,
        selected_env_vars: Option<SyncId>,
        should_show_more_info_view: bool,
        ctx: &mut ViewContext<Input>,
    ) {
        let input_type = if workflow_type.as_workflow().is_agent_mode_workflow() {
            InputType::AI
        } else {
            InputType::Shell
        };

        // Set input type based on whether or not this is a shell or AI workflow.
        self.ai_input_model.update(ctx, |input_model, ctx| {
            input_model.set_input_type(
                input_type,
                Some(InputTypeAutoDetectionSource::WorkflowInsertion),
                ctx,
            );
        });

        // As the first step, clear the existing buffer so that selecting a workflow
        // is effectively a buffer replacement (not append).
        self.editor.update(ctx, |editor, ctx| {
            editor.clear_buffer(ctx);
        });

        if let Some(env_vars_command) = selected_env_vars
            .as_ref()
            .and_then(|id| self.env_vars_command_prefix(id, ctx))
        {
            self.editor.update(ctx, |editor, ctx| {
                editor.system_insert(
                    &env_vars_command,
                    PlainTextEditorViewAction::SystemInsert,
                    ctx,
                )
            });
        }

        // The workflow may or may not come from a history command. If it does, the history command may or may not match
        // the template of the original workflow. If it does match, we have extra display data to show (such as the indices in
        // the command to highlight as arguments). If it doesn't match, there's no additional display data to show. Then, in the
        // default case where there is no history command, there is additional display data.
        let (command_to_insert, display_data) = match history_command {
            Some(history_command) => {
                match self.command_matches_workflow_template(history_command, workflow_type.clone())
                {
                    CommandMatchesWorkflowTemplate::Yes(workflow_display_data) => (
                        workflow_display_data
                            .command_with_replaced_arguments
                            .clone(),
                        Some(workflow_display_data),
                    ),
                    CommandMatchesWorkflowTemplate::No => (history_command.to_string(), None),
                }
            }
            None => {
                let data = if let Some(arguments_to_override) = argument_overrides {
                    compute_workflow_display_data_with_overrides(
                        workflow_type.as_workflow(),
                        arguments_to_override,
                    )
                } else {
                    compute_workflow_display_data(workflow_type.as_workflow())
                };
                (data.command_with_replaced_arguments.clone(), Some(data))
            }
        };

        match display_data {
            Some(WorkflowDisplayData {
                command_with_replaced_arguments,
                replaced_ranges,
                argument_index_to_highlight_index_map,
                argument_index_to_object_id_map,
                ..
            }) => {
                let text_style_ranges = replaced_ranges
                    .into_iter()
                    .map(|range| {
                        (
                            range,
                            TextStyle::new().with_background_color(ColorU::from_u32(
                                WORKFLOW_PARAMETER_HIGHLIGHT_COLOR,
                            )),
                        )
                    })
                    .collect_vec();

                self.editor.update(ctx, |editor, ctx| {
                    editor.insert_with_styles(
                        &command_with_replaced_arguments,
                        &text_style_ranges,
                        PlainTextEditorViewAction::SystemInsert,
                        ctx,
                    );
                });

                // Get enum variants
                let cloud_model = CloudModel::as_ref(ctx);
                let enum_variants_map = argument_index_to_object_id_map
                    .iter()
                    .filter_map(|(index, object_id)| {
                        cloud_model
                            .get_workflow_enum(object_id)
                            .map(|workflow_enum| {
                                workflow_enum.model().string_model.variants.clone()
                            })
                            .map(|variants| (*index, variants))
                    })
                    .collect();

                self.workflows_state.selected_workflow_state = Some(SelectedWorkflowState {
                    more_info_view: self.create_workflows_info_view(
                        workflow_type.clone(),
                        true,
                        ctx,
                    ),
                    argument_index_to_highlight_index: argument_index_to_highlight_index_map,
                    argument_index_to_enum_variants: enum_variants_map,
                    workflow_source,
                    workflow_type,
                    workflow_selection_source,
                    should_show_more_info_view,
                });
            }
            None => {
                self.editor.update(ctx, |editor, ctx| {
                    editor.user_initiated_insert(
                        &command_to_insert,
                        PlainTextEditorViewAction::SystemInsert,
                        ctx,
                    )
                });

                self.workflows_state.selected_workflow_state = Some(SelectedWorkflowState {
                    more_info_view: self.create_workflows_info_view(
                        workflow_type.clone(),
                        false,
                        ctx,
                    ),
                    argument_index_to_highlight_index: HashMap::new(),
                    argument_index_to_enum_variants: HashMap::new(),
                    workflow_source,
                    workflow_type,
                    workflow_selection_source,
                    should_show_more_info_view,
                });
            }
        };

        self.env_var_collection_state.selected_env_vars = selected_env_vars;

        // Ensure the env var selector dropdown is consistent with the selected env vars.
        if let Some(more_info_view) = self
            .workflows_state
            .selected_workflow_state
            .as_ref()
            .map(|state| &state.more_info_view)
        {
            more_info_view.update(ctx, |info_view, ctx| {
                info_view.set_environment_variables_selection(selected_env_vars, ctx);
            })
        }

        // Emit the a11y content as the last step so that it overwrites any of the a11y content
        // emitted by the editor (if multiple `AccessibilityContent`s are emitted within the same
        // event loop, the last one wins).
        let mut accessibility_text = format!("Workflow command {} inserted.", &command_to_insert);
        if let Some(a11y_content) = self.selected_workflow_a11y_text(ctx) {
            let _ = write!(accessibility_text, " {a11y_content}");
        }
        ctx.emit_a11y_content(AccessibilityContent::new(
            accessibility_text,
            "Press shift-tab to select the next workflow argument",
            WarpA11yRole::UserAction,
        ));

        // Only highlight an argument and show enum suggestions if history suggestions are not active
        if !matches!(
            self.suggestions_mode_model.as_ref(ctx).mode(),
            InputSuggestionsMode::HistoryUp { .. } | InputSuggestionsMode::InlineHistoryMenu { .. },
        ) {
            self.highlight_selected_workflow_argument(
                self.get_text_style_ranges_for_workflow(ctx),
                ctx,
            );
        }
        self.focus_input_box(ctx);
    }

    /// Builds a prefix for applying env vars to a command in the current session.
    fn env_vars_command_prefix(&self, env_vars_id: &SyncId, ctx: &AppContext) -> Option<String> {
        let shell_type = self.active_session(ctx)?.shell().shell_type();
        let env_vars = &CloudModel::as_ref(ctx)
            .get_env_var_collection(env_vars_id)?
            .model()
            .string_model;

        if shell_type == ShellType::Fish {
            // Warp currently doesn't support newlines in Fish, just prepend the vars
            let mut command = env_vars.export_variables_for_shell(ShellType::Fish);
            command.push(' ');
            Some(command)
        } else {
            // Add newlines at the end to separate the vars from the comment/command
            Some(format!(
                "# Environment variables\n{}\n\n",
                env_vars.export_variables(" ", shell_type.into())
            ))
        }
    }

    fn create_workflows_info_view(
        &mut self,
        workflow: WorkflowType,
        show_shift_tab_treatment: bool,
        ctx: &mut ViewContext<Input>,
    ) -> ViewHandle<WorkflowsMoreInfoView> {
        let workflow_more_info_view = ctx.add_typed_action_view(|ctx| {
            WorkflowsMoreInfoView::new(
                *InputSettings::as_ref(ctx).workflows_box_expanded.value(),
                workflow,
                show_shift_tab_treatment,
                ctx,
            )
        });

        ctx.subscribe_to_view(&workflow_more_info_view, move |me, _, event, ctx| {
            me.handle_workflow_more_info_event(event, ctx);
        });

        workflow_more_info_view
    }

    fn handle_workflow_more_info_event(
        &mut self,
        event: &WorkflowsInfoBoxViewEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            WorkflowsInfoBoxViewEvent::PrefixCommandWithEnvironmentVariables(env_vars) => {
                self.reset_workflow_state(*env_vars, ctx);

                // The ID may be `None` if the user is *clearing* environment variables.
                if let Some(env_vars_id) = env_vars {
                    let env_vars_object =
                        CloudModel::as_ref(ctx).get_env_var_collection(env_vars_id);
                    let telemetry_metadata = EnvVarTelemetryMetadata {
                        object_id: env_vars_id.into_server().map(Into::into),
                        team_uid: env_vars_object
                            .and_then(|object| object.permissions.owner.into()),
                        space: env_vars_object
                            .map_or(Space::Personal, |object| object.space(ctx))
                            .into(),
                    };
                    send_telemetry_from_ctx!(
                        TelemetryEvent::EnvVarWorkflowParameterization(telemetry_metadata),
                        ctx
                    );
                }
            }
        }
    }

    /// Returns the a11y text for a workflow that is selected. `None`, if there is no workflow
    /// selected.
    fn selected_workflow_a11y_text(&self, ctx: &mut ViewContext<Self>) -> Option<String> {
        self.workflows_state
            .selected_workflow_state
            .as_ref()
            .and_then(|selected_workflow_state| {
                selected_workflow_state.more_info_view.read(ctx, |view, _| {
                    view.selected_argument()
                        .map(|argument| format!("Selected Workflow argument {}", argument.name()))
                })
            })
    }

    fn workflow_arg_was_deleted(
        &self,
        text_style_run_count: usize,
        argument_index_to_highlight_index: &HashMap<WorkflowArgumentIndex, Vec<usize>>,
    ) -> bool {
        let expected_run_count: usize = argument_index_to_highlight_index
            .values()
            .map(|indices| indices.len())
            .sum();
        text_style_run_count != expected_run_count
    }

    fn get_text_style_ranges_for_workflow(
        &self,
        ctx: &ViewContext<Self>,
    ) -> Vec<Range<ByteOffset>> {
        let text_style_runs: Vec<_> = self
            .editor
            .as_ref(ctx)
            .text_style_runs(ctx)
            .filter(|style_run| style_run.text_style().background_color.is_some())
            .collect();
        self.build_text_run_ranges_for_workflows(&text_style_runs)
    }

    /// We are currently using the styling of text runs in the input as a way of tracking
    /// where our workflow arguments are.
    /// This doesn't work in 2 cases:
    ///
    /// 1. When part of a workflow argument is subject to syntax highlighting, it breaks
    ///    a run into one or more runs. Example: "--env JOB_EXECUTION_MODE=REAL" will wind
    ///    up with syntax highlighting on `--env`, resulting in 2 runs.
    /// 2. When workflow arguments directly follow each other with no spacing, they will
    ///    both be covered by a single run.. Example: {a}{b}{c} will only get a single
    ///    run covering "abc"
    ///
    /// This helper acts as a quick hack to address the first issue:
    /// if two background-highlighted runs are contiguous, they are merged into a single run.
    /// This is a short-term fix and should be addressed in a more comprehensive way that does
    /// not rely on the styling of the input.
    ///
    /// See [CLD-997](https://linear.app/warpdotdev/issue/CLD-997)
    fn build_text_run_ranges_for_workflows(
        &self,
        text_style_runs: &[TextRun],
    ) -> Vec<Range<ByteOffset>> {
        let mut ranges = text_style_runs
            .iter()
            .map(|style_run| style_run.byte_range().clone())
            .collect::<Vec<_>>();
        ranges.sort_by(|a, b| a.start.cmp(&b.start));

        let capacity = ranges.len();

        ranges.into_iter().fold(
            Vec::<Range<ByteOffset>>::with_capacity(capacity),
            |mut acc: Vec<Range<ByteOffset>>, next| -> Vec<Range<ByteOffset>> {
                match acc.last() {
                    Some(current) if current.end >= next.start => {
                        let new_range = std::cmp::min(current.start, next.start)
                            ..std::cmp::max(current.end, next.end);
                        acc.pop();
                        acc.push(new_range);
                    }
                    _ => {
                        acc.push(next);
                    }
                };
                acc
            },
        )
    }

    /// Highlight the currently selected workflow argument and open the enum suggestions menu if applicable.
    /// Takes in `text_style_ranges`, which contains ByteOffset Ranges of arguments in the input editor.
    fn highlight_selected_workflow_argument(
        &mut self,
        text_style_ranges: Vec<Range<ByteOffset>>,
        ctx: &mut ViewContext<Self>,
    ) {
        let mut variants = None;
        let mut selected_ranges = Vec::new();

        if let Some(active_workflow_state) = self.workflows_state.selected_workflow_state.as_ref() {
            active_workflow_state
                .more_info_view
                .update(ctx, |workflows_info_view, ctx| {
                    let selected_workflow_state = &mut workflows_info_view.selected_workflow_state;
                    // Update the editor given what the currently selected argument index is
                    self.editor.update(ctx, |editor, ctx| {
                        // If an argument has been completely deleted - pause the shift-tab cycling
                        if self.workflow_arg_was_deleted(
                            text_style_ranges.len(),
                            &active_workflow_state.argument_index_to_highlight_index,
                        ) {
                            selected_workflow_state.set_argument_cycling_enabled(false);
                        } else {
                            variants = active_workflow_state
                                .argument_index_to_enum_variants
                                .get(&selected_workflow_state.currently_selected_argument());

                            selected_workflow_state.set_argument_cycling_enabled(true);
                            // Get all of the highlighted ranges for the currently selected argument.
                            let byte_ranges = active_workflow_state
                                .argument_index_to_highlight_index
                                .get(&selected_workflow_state.currently_selected_argument())
                                .map(|indices| {
                                    indices
                                        .iter()
                                        .filter_map(|index| text_style_ranges.get(*index).cloned())
                                });

                            if let Some(byte_ranges) = byte_ranges {
                                selected_ranges = byte_ranges.clone().collect();
                                editor.select_ranges_by_byte_offset(byte_ranges, ctx);
                            }
                        }
                    });
                });
        }

        if let Some(enum_variants) = variants {
            self.populate_enum_suggestions_menu(enum_variants.clone(), selected_ranges, ctx);
        } else {
            self.suggestions_mode_model.update(ctx, |m, ctx| {
                m.set_mode(InputSuggestionsMode::Closed, ctx);
            });
        }
        ctx.notify();
    }

    fn populate_enum_suggestions_menu(
        &mut self,
        enum_variants: EnumVariants,
        selected_ranges: Vec<Range<ByteOffset>>,
        ctx: &mut ViewContext<Self>,
    ) {
        // If the newly highlighted argument has enum variants, populate the suggestions menu
        let position = self.editor.as_ref(ctx).first_selection_end_to_point(ctx);

        self.editor.update(ctx, |editor, ctx| {
            editor.cache_buffer_point(
                position,
                COMPLETIONS_START_OF_REPLACEMENT_SPAN_POSITION_ID,
                ctx,
            );
        });

        let variants = match enum_variants {
            EnumVariants::Static(variants) => {
                self.suggestions_mode_model.update(ctx, |m, ctx| {
                    m.set_mode(
                        InputSuggestionsMode::StaticWorkflowEnumSuggestions {
                            suggestions: variants.clone(),
                            menu_position: TabCompletionsMenuPosition::AtFirstCursor,
                            selected_ranges,
                            cursor_point: position,
                        },
                        ctx,
                    );
                });
                variants
            }
            EnumVariants::Dynamic(command) => {
                if FeatureFlag::DynamicWorkflowEnums.is_enabled() {
                    self.suggestions_mode_model.update(ctx, |m, ctx| {
                        m.set_mode(
                            InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
                                suggestions: vec![],
                                menu_position: TabCompletionsMenuPosition::AtFirstCursor,
                                selected_ranges,
                                cursor_point: position,
                                dynamic_enum_status: DynamicEnumSuggestionStatus::Unapproved,
                                command,
                            },
                            ctx,
                        );
                    });
                }
                vec![]
            }
        };

        self.input_suggestions.update(ctx, |input, ctx| {
            input.set_enum_variants(variants, ctx);
        });

        ctx.notify();
    }

    fn handle_suggestions_event(
        &mut self,
        event: &InputSuggestionsEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        if !self.suggestions_mode_model.as_ref(ctx).is_visible() {
            return;
        }

        match event {
            InputSuggestionsEvent::ConfirmSuggestion {
                suggestion,
                match_type,
            } => {
                if !self.confirm_suggestion(suggestion, ctx) {
                    return;
                }

                send_telemetry_from_ctx!(
                    TelemetryEvent::ConfirmSuggestion {
                        mode: self
                            .suggestions_mode_model
                            .as_ref(ctx)
                            .mode()
                            .to_telemetry_mode(),
                        match_type: *match_type,
                    },
                    ctx
                );
                self.close_input_suggestions(/*should_focus_input=*/ true, ctx);
            }
            InputSuggestionsEvent::ConfirmAndExecuteSuggestion {
                suggestion,
                match_type,
            } => {
                if !self.confirm_and_execute_suggestion(suggestion, ctx) {
                    return;
                }

                send_telemetry_from_ctx!(
                    TelemetryEvent::ConfirmSuggestion {
                        mode: self
                            .suggestions_mode_model
                            .as_ref(ctx)
                            .mode()
                            .to_telemetry_mode(),
                        match_type: *match_type,
                    },
                    ctx
                );

                self.close_input_suggestions(/*should_focus_input=*/ true, ctx);

                let command = self.get_command(ctx);
                self.try_execute_command(&command, ctx);

                ctx.emit_a11y_content(AccessibilityContent::new_without_help(
                    format!("Executed: {command}"),
                    WarpA11yRole::UserAction,
                ));
            }
            InputSuggestionsEvent::CloseSuggestion {
                should_restore_buffer_before_history_up,
            } => {
                self.close_input_suggestions_and_restore_buffer(
                    true,
                    *should_restore_buffer_before_history_up,
                    ctx,
                );
            }
            InputSuggestionsEvent::Select(selected_item) => {
                let mode = self.suggestions_mode_model.as_ref(ctx).mode().clone();
                match &mode {
                    InputSuggestionsMode::HistoryUp { .. } => {
                        if let Some((workflow_type, workflow_source)) = selected_item
                            .linked_workflow_data()
                            .and_then(|linked_workflow_data| {
                                linked_workflow_data.linked_workflow(ctx)
                            })
                        {
                            // TODO(ben): We should include the chosen env vars in the history
                            // entry.
                            let env_vars = workflow_type.as_workflow().default_env_vars();
                            self.insert_workflow_into_input(
                                workflow_type,
                                workflow_source,
                                WorkflowSelectionSource::UpArrowHistory,
                                None,
                                Some(selected_item.text()),
                                env_vars,
                                /*should_show_more_info_view=*/ false,
                                ctx,
                            );
                        } else {
                            self.editor.update(ctx, |editor, ctx| {
                                editor.set_buffer_text_ignoring_undo(selected_item.text(), ctx);
                            });
                        }

                        self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                            let input_type = if selected_item.is_ai_query() {
                                InputType::AI
                            } else {
                                InputType::Shell
                            };
                            ai_input_model.set_input_type(
                                input_type,
                                Some(InputTypeAutoDetectionSource::HistorySelection),
                                ctx,
                            );
                        });
                    }
                    InputSuggestionsMode::CompletionSuggestions {
                        replacement_start, ..
                    } => {
                        let replacement_start = *replacement_start;
                        if self.is_classic_completions_enabled(ctx) {
                            self.editor.update(ctx, |editor, ctx| {
                                let cursor_end_offset =
                                    editor.end_byte_index_of_last_selection(ctx);
                                editor.select_and_replace(
                                    selected_item.text(),
                                    [ByteOffset::from(replacement_start)..cursor_end_offset],
                                    PlainTextEditorViewAction::CycleCompletionSuggestion,
                                    ctx,
                                );
                                ctx.notify();
                            });
                            ctx.notify();
                        }
                    }
                    InputSuggestionsMode::StaticWorkflowEnumSuggestions { .. }
                    | InputSuggestionsMode::DynamicWorkflowEnumSuggestions { .. } => {
                        // If in the future we want to replace the selected arguments with suggestion options as we cycle, this is where we do it
                    }

                    InputSuggestionsMode::InlineHistoryMenu { .. } => {
                        // Inline history menu selection is handled separately
                        // This shouldn't be reached since inline history menu doesn't use InputSuggestions
                    }
                    InputSuggestionsMode::Closed => {
                        log::warn!("Got a InputSuggestionsEvent::Select when the mode was Closed!");
                    }
                }
            }
            InputSuggestionsEvent::IgnoreItem { item } => {
                let command_text = item.text();
                let suggestion_type = if item.is_ai_query() {
                    SuggestionType::AIQuery
                } else {
                    SuggestionType::ShellCommand
                };

                IgnoredSuggestionsModel::handle(ctx).update(ctx, |model, ctx| {
                    model.add_ignored_suggestion(command_text.to_string(), suggestion_type, ctx);
                });

                // Refresh the history suggestions menu and keep it open
                if matches!(
                    self.suggestions_mode_model.as_ref(ctx).mode(),
                    InputSuggestionsMode::HistoryUp { .. }
                ) {
                    let history = if self.model.lock().shared_session_status().is_executor() {
                        self.shared_session_history(ctx)
                    } else {
                        self.command_history(ctx)
                    };
                    let original_buffer = if let InputSuggestionsMode::HistoryUp {
                        original_buffer,
                        ..
                    } = self.suggestions_mode_model.as_ref(ctx).mode()
                    {
                        original_buffer.clone()
                    } else {
                        String::new()
                    };

                    let matches =
                        InputSuggestions::history_prefix_search(&original_buffer, history);
                    self.input_suggestions
                        .update(ctx, move |input_suggestions, ctx| {
                            input_suggestions.set_history_matches(matches, ctx);
                        });
                }
            }
        }
    }

    /// Resets the SelectedWorkflowState back to the original workflow, with its original arguments. This
    /// is useful when the command does not match the original workflow.
    fn reset_workflow_state(&mut self, env_vars: Option<SyncId>, ctx: &mut ViewContext<Input>) {
        // We want to also initially clear the stored selected env var.
        self.clear_selected_env_var_collection();

        if let Some(state) = self.workflows_state.selected_workflow_state.take() {
            self.insert_workflow_into_input(
                state.workflow_type,
                state.workflow_source,
                state.workflow_selection_source,
                None,
                None,
                env_vars,
                true,
                ctx,
            )
        }

        ctx.notify();
    }

    fn confirm_suggestion(&mut self, suggestion: &str, ctx: &mut ViewContext<Input>) -> bool {
        self.confirm_suggestion_internal(suggestion, Executing::No, ctx)
    }

    fn confirm_and_execute_suggestion(
        &mut self,
        suggestion: &str,
        ctx: &mut ViewContext<Input>,
    ) -> bool {
        self.confirm_suggestion_internal(suggestion, Executing::Yes, ctx)
    }

    /// Handles suggestion confirmation behaviour in editor and returns true if suggestions menu should be closed
    /// For CompletionSuggestions, inserts suggestion into editor. For HistoryUp, no action since "select" populates buffer.
    /// Closed branch should never be executed (does not use the input suggestions panel).
    fn confirm_suggestion_internal(
        &mut self,
        suggestion: &str,
        executing: Executing,
        ctx: &mut ViewContext<Input>,
    ) -> bool {
        match self.suggestions_mode_model.as_ref(ctx).mode() {
            InputSuggestionsMode::Closed => false,
            InputSuggestionsMode::HistoryUp { .. } => true,
            InputSuggestionsMode::CompletionSuggestions {
                replacement_start, ..
            } => {
                self.insert_completion_result_into_editor(
                    suggestion,
                    *replacement_start,
                    executing,
                    ctx,
                );
                true
            }
            InputSuggestionsMode::StaticWorkflowEnumSuggestions {
                selected_ranges, ..
            }
            | InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
                selected_ranges, ..
            } => {
                let selected_ranges = selected_ranges.clone();
                self.editor.update(ctx, |editor, ctx| {
                    editor.select_and_replace(
                        suggestion,
                        selected_ranges.iter().cloned(),
                        PlainTextEditorViewAction::AcceptCompletionSuggestion,
                        ctx,
                    );
                });
                true
            }

            InputSuggestionsMode::InlineHistoryMenu { .. } => {
                // Inline history menu selection is handled separately
                false
            }
        }
    }

    pub fn close_input_suggestions_and_restore_buffer(
        &mut self,
        should_focus_input: bool,
        should_restore_buffer_before_history_up: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        if should_restore_buffer_before_history_up
            && let InputSuggestionsMode::HistoryUp {
                original_buffer,
                original_cursor_point,
                original_input_was_locked,
                original_input_type,
                ..
            } = self.suggestions_mode_model.as_ref(ctx).mode()
        {
            let original_buffer = original_buffer.clone();
            let original_cursor_point = *original_cursor_point;
            let original_input_was_locked = *original_input_was_locked;
            let original_input_type = *original_input_type;
            // If the user closes the input suggestions menu, we want to reset the AI input mode
            // to the exact same state it was originally, which includes the mode itself and
            // whether it was locked to that mode.
            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                ai_input_model.set_input_config(
                    InputConfig {
                        input_type: original_input_type,
                        is_locked: original_input_was_locked,
                    },
                    original_buffer.is_empty(),
                    Some(InputTypeAutoDetectionSource::RestoreSavedConfig),
                    ctx,
                );
            });

            self.editor.update(ctx, |editor, ctx| {
                editor.set_buffer_text_ignoring_undo(&original_buffer, ctx);
                if let Some(original_cursor_point) = original_cursor_point {
                    editor.reset_selections_to_point(&original_cursor_point, ctx);
                }
            });
        }
        self.close_input_suggestions(/*should_focus_input=*/ should_focus_input, ctx);
    }

    pub fn close_input_suggestions(
        &mut self,
        should_focus_input: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        // If the input suggestions view is already closed, don't refocus the input box.
        if !self.suggestions_mode_model.as_ref(ctx).is_closed() {
            let was_inline_menu_open = self
                .suggestions_mode_model
                .as_ref(ctx)
                .is_inline_menu_open();

            self.suggestions_mode_model.update(ctx, |m, ctx| {
                m.set_mode(InputSuggestionsMode::Closed, ctx);
            });

            // If we're closing an inline menu, trigger autodetection on the buffer contents
            if was_inline_menu_open {
                self.run_input_background_jobs(
                    InputBackgroundJobOptions::default().with_ai_input_detection(),
                    ctx,
                );
            }

            if should_focus_input {
                self.focus_input_box(ctx);
                self.maybe_generate_autosuggestion(ctx);
            } else {
                ctx.notify();
            }
        }
    }

    pub fn clear_buffer_and_reset_undo_stack(&mut self, ctx: &mut ViewContext<Self>) {
        self.editor.update(ctx, |view, ctx| {
            view.clear_buffer_and_reset_undo_stack(ctx);
        });
    }

    pub fn replace_buffer_content(&mut self, content: &str, ctx: &mut ViewContext<Self>) {
        self.editor.update(ctx, |view, ctx| {
            view.set_buffer_text(content, ctx);
        });
    }

    // Fill the input buffer with the provided text and auto-select all of the text
    // (so that it's easy to delete).
    pub fn prefill_buffer_and_select_all(&mut self, content: &str, ctx: &mut ViewContext<Self>) {
        let content = content.trim();
        if content.is_empty() {
            return;
        }

        self.editor.update(ctx, |editor, ctx| {
            editor.clear_autosuggestion(ctx);
            editor.set_buffer_text_ignoring_undo(content, ctx);
            editor.handle_action(&EditorAction::SelectAll, ctx);
        });
    }

    /// Appends text to the current buffer at the cursor position, preserving existing buffer content.
    pub fn append_to_buffer(&mut self, content: &str, ctx: &mut ViewContext<Self>) {
        self.system_insert(content, ctx);
    }

    pub fn insert_typeahead_text(
        &mut self,
        num_typeahead_chars_inserted: CharOffset,
        typeahead: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.editor.update(ctx, |view, ctx| {
            view.replace_first_n_characters(num_typeahead_chars_inserted, typeahead, ctx);
            view.move_to_buffer_end(ctx);
        });
    }

    pub fn focus_input_box(&self, ctx: &mut ViewContext<Self>) {
        ctx.focus_self();
    }

    pub fn input_type(&self, app: &AppContext) -> InputType {
        self.ai_input_model.as_ref(app).input_type()
    }

    /// Close all overlays managed by the input view. Does not change what is focused.
    /// If should_restore_buffer_before_history_up is true, the buffer will be restored to the state it was in before the history up menu was opened.
    pub fn close_overlays(
        &mut self,
        should_restore_buffer_before_history_up: bool,
        ctx: &mut ViewContext<Input>,
    ) {
        self.close_voltron(ctx);
        self.close_input_suggestions_and_restore_buffer(
            false,
            should_restore_buffer_before_history_up,
            ctx,
        );
        self.clear_selected_workflow(ctx);
    }

    /// Closes any active suggestion mode UI when starting a new conversation.
    ///
    /// This is intentionally narrower than `close_overlays`: it does not close Voltron, workflow
    /// info overlays, etc.
    fn close_suggestion_modes_for_new_conversation(&mut self, ctx: &mut ViewContext<Self>) {
        self.suggestions_mode_model.update(ctx, |model, ctx| {
            model.set_mode(InputSuggestionsMode::Closed, ctx);
        });
    }

    fn close_voltron(&mut self, ctx: &mut ViewContext<Input>) {
        self.is_voltron_open = false;
        ctx.notify();
    }

    fn editor_up(&mut self, ctx: &mut ViewContext<Self>) {
        // History and input suggestions are not available for
        // read-only viewers in a shared session
        if self.model.lock().shared_session_status().is_reader() {
            return;
        }

        // For some input suggestion modes, the menu handles its own actions.
        let handled = match self.suggestions_mode_model.as_ref(ctx).mode() {
            InputSuggestionsMode::InlineHistoryMenu { .. } => {
                self.inline_history_menu_view.update(ctx, |view, ctx| {
                    view.select_up(ctx);
                });
                true
            }
            InputSuggestionsMode::HistoryUp { .. }
            | InputSuggestionsMode::CompletionSuggestions { .. }
            | InputSuggestionsMode::StaticWorkflowEnumSuggestions { .. }
            | InputSuggestionsMode::DynamicWorkflowEnumSuggestions { .. }
            | InputSuggestionsMode::Closed => false,
        };

        if handled {
            return;
        }

        // If the input suggestions menu is open, always cycle to the next option.
        if self.suggestions_mode_model.as_ref(ctx).is_visible() && self.can_query_history(ctx) {
            self.input_suggestions.update(ctx, |suggestions, ctx| {
                suggestions.select_prev(ctx);
            });
            return;
        }

        // Otherwise, check if the cursor is on the first row and open the
        // history up menu.
        let editor = self.editor.as_ref(ctx);
        if editor.single_cursor_on_first_row(ctx) {
            if FeatureFlag::InlineHistoryMenu.is_enabled()
                && self.suggestions_mode_model.as_ref(ctx).is_closed()
            {
                self.open_inline_history_menu(ctx);
                return;
            }

            let history = if self.model.lock().shared_session_status().is_executor() {
                self.shared_session_history(ctx)
            } else {
                self.command_history(ctx)
            };
            let original_buffer = self.editor.as_ref(ctx).buffer_text(ctx);

            let matches = InputSuggestions::history_prefix_search(&original_buffer, history);
            self.input_suggestions
                .update(ctx, move |input_suggestions, ctx| {
                    input_suggestions.set_history_matches(matches, ctx);
                });

            let original_cursor_point = self.editor.as_ref(ctx).single_cursor_to_point(ctx);
            let original_input_type = self.ai_input_model.as_ref(ctx).input_type();
            let original_input_was_locked = self.ai_input_model.as_ref(ctx).is_input_type_locked();
            self.suggestions_mode_model.update(ctx, |m, ctx| {
                m.set_mode(
                    InputSuggestionsMode::HistoryUp {
                        original_buffer,
                        original_cursor_point,
                        search_mode: HistorySearchMode::Prefix,
                        original_input_type,
                        original_input_was_locked,
                    },
                    ctx,
                );
            });

            ctx.notify();
            return;
        }
        // Finally, if we're neither scrolling through an existing suggestion
        // list nor entering the history mode, we move the cursor up.
        self.editor.update(ctx, |input, ctx| input.move_up(ctx));
    }

    // TODO - Implement PageUp functionality for input suggestions menu
    fn editor_page_up(&mut self, ctx: &mut ViewContext<Self>) {
        let event = self.editor.read(ctx, |editor, ctx| {
            TelemetryEvent::PageUpDownInEditorPressed {
                is_empty_editor: editor.is_empty(ctx),
                is_down: false,
            }
        });
        send_telemetry_from_ctx!(event, ctx);
        if self.suggestions_mode_model.as_ref(ctx).is_visible() {
            self.editor
                .update(ctx, |input, ctx| input.move_page_up(ctx));
        } else {
            ctx.emit(Event::PageUp);
        }
    }

    /// Asks the currently active inline menu whether the buffer should be restored on dismiss
    /// (defaulting to true for any inline menus that don't have specific behavior requirements for this decision).
    fn should_restore_buffer_on_inline_menu_dismiss(&self, ctx: &ViewContext<Self>) -> bool {
        match self.suggestions_mode_model.as_ref(ctx).mode() {
            // If the input is not being used as a search on the model menu
            // we should not restore/revert the changes to the input on-dismiss,
            _ => true,
        }
    }

    fn editor_escape(&mut self, ctx: &mut ViewContext<Self>) {
        let vim_mode = self.editor.as_ref(ctx).vim_mode(ctx);
        let has_attached_context = {
            let context_model = self.ai_context_model.as_ref(ctx);
            !context_model.pending_context_block_ids().is_empty()
                || context_model.pending_context_selected_text().is_some()
        };
        let should_escape_vim_before_dismissing = vim_mode == Some(VimMode::Insert)
            && (self.suggestions_mode_model.as_ref(ctx).is_history_up()
                || self
                    .suggestions_mode_model
                    .as_ref(ctx)
                    .is_inline_history_menu());

        if should_escape_vim_before_dismissing {
            self.editor.update(ctx, |editor, editor_ctx| {
                editor.handle_action(&EditorAction::VimEscape, editor_ctx);
            });
        } else if self
            .suggestions_mode_model
            .as_ref(ctx)
            .is_inline_menu_open()
        {
            if self.should_restore_buffer_on_inline_menu_dismiss(ctx) {
                self.suggestions_mode_model.update(ctx, |model, ctx| {
                    model.close_and_restore_buffer(ctx);
                });
            } else {
                self.suggestions_mode_model.update(ctx, |model, ctx| {
                    model.set_mode(InputSuggestionsMode::Closed, ctx);
                });
            }
            ctx.notify();
        } else if self.suggestions_mode_model.as_ref(ctx).is_visible() {
            self.input_suggestions
                .update(ctx, |input_suggestions, ctx| {
                    input_suggestions.exit(true, ctx);
                });
        } else if self.workflows_state.selected_workflow_state.is_some() {
            self.clear_current_workflow(ctx);
        } else if !matches!(vim_mode, None | Some(VimMode::Normal)) {
            self.editor.update(ctx, |editor, editor_ctx| {
                editor.handle_action(&EditorAction::VimEscape, editor_ctx);
            });
        } else if FeatureFlag::AgentView.is_enabled()
            && self.agent_view_controller.as_ref(ctx).is_active()
            && has_attached_context
        {
            self.clear_attached_context(ctx);
        } else {
            if FeatureFlag::AgentView.is_enabled()
                && !self.agent_view_controller.as_ref(ctx).is_fullscreen()
            {
                if self.ai_input_model.as_ref(ctx).is_ai_input_enabled() {
                    // This implies the contents of the terminal input are autodetected as an agent
                    // prompt; overrides the autodetection by explicitly setting input mode back to
                    // terminal.
                    self.set_input_mode_terminal(false, ctx);
                }
            } else {
                self.set_input_mode_natural_language_detection(ctx);
            }
            ctx.emit(Event::Escape);
        }
    }

    /// Emits an `AgentModeAutodetectionFalsePositive` telemetry event if the current input text has
    /// been autodetected as AI input and the user manually toggled to shell.
    /// Also emits `AgentModeChangedInputType` if the user is part of the analytics experiment.
    ///
    /// This is intended to be called whenever the user manually toggles the input to new_input_type. Because the user is manually toggling
    /// back to shell mode after input has been autodetected as natural language, we infer that the
    /// current input text may not have been correctly classified as natural language.
    /// For users opted in to the analytics experiment, we collect the input buffer text whenever the input type is toggled
    /// in either direction.
    fn maybe_send_autodetection_telemetry_on_manual_toggle(
        &self,
        new_input_type: InputType,
        ctx: &mut ViewContext<Self>,
    ) {
        let input_buffer_text = self.buffer_text(ctx);
        let buffer_length = input_buffer_text.len();
        let input =
            should_collect_ai_ugc_telemetry(ctx, PrivacySettings::as_ref(ctx).is_telemetry_enabled)
                .then_some(input_buffer_text);
        let is_udi_enabled = InputSettings::as_ref(ctx).is_universal_developer_input_enabled(ctx);
        send_telemetry_from_ctx!(
            TelemetryEvent::AgentModeChangedInputType {
                input,
                buffer_length,
                is_manually_changed: true,
                new_input_type,
                active_block_id: self.model.lock().block_list().active_block_id().clone(),
                is_udi_enabled,
            },
            ctx
        );

        let ai_input_model = self.ai_input_model.as_ref(ctx);
        if matches!(new_input_type, InputType::Shell) && !ai_input_model.is_input_type_locked() {
            let current_input_text = self.buffer_text(ctx);
            if !current_input_text.is_empty() {
                let event_payload = if ChannelState::channel().is_dogfood() {
                    AgentModeAutoDetectionFalsePositivePayload::InternalDogfoodUsers {
                        input_text: current_input_text,
                    }
                } else {
                    AgentModeAutoDetectionFalsePositivePayload::ExternalUsers
                };
                send_telemetry_from_ctx!(
                    TelemetryEvent::AgentModePotentialAutoDetectionFalsePositive(event_payload),
                    ctx
                );
            }
        }
    }

    /// Takes the current collpased/expanded state of the info box and saves it to the user's settings so that last value can be
    /// reused the next time the user opens a workflow.
    fn update_workflows_info_box_expanded_setting(
        &mut self,
        ctx: &mut ViewContext<Self>,
        selected_workflow_state: &SelectedWorkflowState,
    ) {
        let info_box_expanded = selected_workflow_state
            .more_info_view
            .as_ref(ctx)
            .info_box_expanded;

        InputSettings::handle(ctx).update(ctx, |input_settings, ctx| {
            report_if_error!(
                input_settings
                    .workflows_box_expanded
                    .set_value(info_box_expanded, ctx)
            );
        });
    }

    fn clear_current_workflow(&mut self, ctx: &mut ViewContext<Input>) {
        // Whenever we clear the workflow we also want to clear the env vars
        self.clear_selected_env_var_collection();

        if let Some(state) = self.workflows_state.selected_workflow_state.take() {
            self.update_workflows_info_box_expanded_setting(ctx, &state);
        }
        self.editor
            .update(ctx, |editor, ctx| editor.clear_text_style_runs(ctx));
        ctx.notify();
    }

    fn editor_down(&mut self, ctx: &mut ViewContext<Self>) {
        // For some input suggestion modes, the menu handles its own actions.
        let handled = match self.suggestions_mode_model.as_ref(ctx).mode() {
            InputSuggestionsMode::HistoryUp { .. }
            | InputSuggestionsMode::CompletionSuggestions { .. }
            | InputSuggestionsMode::StaticWorkflowEnumSuggestions { .. }
            | InputSuggestionsMode::DynamicWorkflowEnumSuggestions { .. }
            | InputSuggestionsMode::InlineHistoryMenu { .. }
            | InputSuggestionsMode::Closed => false,
        };

        if handled {
            return;
        } else if self
            .suggestions_mode_model
            .as_ref(ctx)
            .is_inline_history_menu()
        {
            self.inline_history_menu_view.update(ctx, |view, ctx| {
                view.select_down(ctx);
            });
            return;
        }

        if self.suggestions_mode_model.as_ref(ctx).is_visible() {
            if self.input_suggestions.as_ref(ctx).is_empty() {
                // arrow down on an empty suggestions means we should close it.
                self.close_input_suggestions_and_restore_buffer(true, true, ctx);
            } else {
                self.input_suggestions.update(ctx, |suggestions, ctx| {
                    suggestions.select_next(ctx);
                });
            }
        } else if FeatureFlag::CycleNextCommandSuggestion.is_enabled()
            && self.editor.as_ref(ctx).is_empty(ctx)
        {
            self.cycle_next_command_suggestion(ctx);
        } else {
            self.editor.update(ctx, |editor, ctx| editor.move_down(ctx));
        }
    }

    // TODO - Implement PageDown functionality for input suggestions menu
    fn editor_page_down(&mut self, ctx: &mut ViewContext<Self>) {
        let event = self.editor.read(ctx, |editor, ctx| {
            TelemetryEvent::PageUpDownInEditorPressed {
                is_empty_editor: editor.is_empty(ctx),
                is_down: true,
            }
        });
        send_telemetry_from_ctx!(event, ctx);
        if self.suggestions_mode_model.as_ref(ctx).is_visible() {
            self.editor
                .update(ctx, |input, ctx| input.move_page_down(ctx));
        } else {
            ctx.emit(Event::PageDown);
        }
    }

    fn maybe_generate_autosuggestion(&mut self, ctx: &mut ViewContext<Self>) {
        let editor = self.editor.as_ref(ctx);

        let should_generate_autosuggestion = !editor.active_autosuggestion()
            && self.enable_autosuggestions_setting
            && !self.ai_input_model.as_ref(ctx).is_ai_input_enabled();

        if should_generate_autosuggestion {
            let buffer_text = editor.buffer_text(ctx);
            self.generate_autosuggestion_async(buffer_text, self.completer_data(), ctx)
        }
    }

    /// Asynchronously generate an autosuggestion to be inserted into the editor. First, reverse
    /// search the user's history to find a possible command that starts with the buffer text. If
    /// no commands are found, run the completer in a background thread to generate a result.
    pub fn generate_autosuggestion_async(
        &mut self,
        buffer_text: String,
        completer_data: CompleterData,
        ctx: &mut ViewContext<Self>,
    ) {
        if buffer_text.is_empty() {
            return;
        }

        let Some(session_id) = completer_data.active_block_session_id() else {
            return;
        };
        self.abort_latest_autosuggestion_future();

        if FeatureFlag::PartialNextCommandSuggestions.is_enabled() && is_next_command_enabled(ctx) {
            let Some(session) = self.active_session(ctx) else {
                return;
            };
            if let Some(last_user_block_completed) =
                completer_data.last_user_block_completed.clone()
            {
                self.next_command_model.update(ctx, |model, ctx| {
                    let shell_name = session.shell().shell_type().name().to_owned();
                    let shell_version = session.shell().version().clone();
                    model.generate_next_command_suggestion_with_prefix(
                        Some(buffer_text),
                        last_user_block_completed,
                        shell_name,
                        shell_version,
                        completer_data,
                        ctx,
                    );
                });
                return;
            }
        }

        let completion_context = completer_data.completion_session_context(ctx);
        let completion_session = completion_context
            .as_ref()
            .map(|completion_context| completion_context.session.clone());

        let reverse_chronological_potential_autosuggestions =
            NextCommandModel::get_reverse_chronological_potential_autosuggestions(
                &buffer_text,
                &completer_data,
                ctx,
            );

        let session_env_vars = self.sessions.read(ctx, |sessions, _| {
            sessions.get_env_vars_for_session(session_id)
        });
        // Get current ignored shell commands to filter during generation
        let ignored_suggestions = IgnoredSuggestionsModel::as_ref(ctx)
            .get_ignored_suggestions_for_type(SuggestionType::ShellCommand);
        #[cfg(feature = "local_fs")]
        let conn = self.conn.clone();
        // Resolve the last completed block's lazily-computed fields now, synchronously, since the
        // spawned future below doesn't have access to the terminal model to resolve them later.
        #[cfg(feature = "local_fs")]
        let last_user_block_completed_data =
            completer_data
                .last_user_block_completed
                .as_ref()
                .map(|block| {
                    (
                        block
                            .command
                            .get_with(|compute| {
                                let model = self.model.lock();
                                compute(model.block_list())
                            })
                            .to_owned(),
                        block
                            .serialized_block
                            .get_with(|compute| {
                                let model = self.model.lock();
                                compute(model.block_list())
                            })
                            .clone(),
                    )
                });
        let abort_handle = ctx
            .spawn_abortable(
                async move {
                    #[cfg(feature = "local_fs")]
                    // First, use rich history to find commands with a matching prefix that were run
                    // in a similar context, taking into account the most recent block run.
                    if let Some(conn) = conn
                        && let Some((last_command, last_serialized_block)) =
                            &last_user_block_completed_data
                    {
                        let similar_history_contexts = {
                            let mut conn = conn.lock();
                            NextCommandModel::get_similar_history_context(
                                &mut conn,
                                last_command,
                                &last_serialized_block.pwd,
                                last_serialized_block.exit_code,
                                last_serialized_block.shell_host.as_ref(),
                                0,
                            )
                        };
                        if !similar_history_contexts.is_empty() {
                            let mut history_next_command_counts = counter::Counter::<String>::new();
                            // Find the most likely next command after a similar context, out of those that have a matching prefix and aren't ignored.
                            for history_context in &similar_history_contexts {
                                if history_context
                                    .next_command
                                    .command
                                    .starts_with(&buffer_text)
                                    && !ignored_suggestions
                                        .contains(&history_context.next_command.command)
                                {
                                    history_next_command_counts
                                        [&history_context.next_command.command] += 1;
                                }
                            }

                            for (most_likely_next_command, _) in
                                history_next_command_counts.k_most_common_ordered(5)
                            {
                                if is_command_valid(
                                    &most_likely_next_command,
                                    completion_context.as_ref(),
                                    session_env_vars.as_ref(),
                                )
                                .await
                                {
                                    return AutoSuggestionResult {
                                        buffer_text,
                                        autosuggestion_result: Some(
                                            most_likely_next_command.clone(),
                                        ),
                                    };
                                }
                            }
                        }
                    }

                    // If we have no suggestion from similar historical context, fallback to the most recent
                    // command with a matching prefix run in the same pwd (if exists, otherwise just most recent command anywhere with matching prefix).
                    for reverse_chronological_command in
                        reverse_chronological_potential_autosuggestions.unwrap_or_default()
                    {
                        if !ignored_suggestions.contains(&reverse_chronological_command.command)
                            && is_command_valid(
                                &reverse_chronological_command.command,
                                completion_context.as_ref(),
                                session_env_vars.as_ref(),
                            )
                            .await
                        {
                            return AutoSuggestionResult {
                                buffer_text,
                                autosuggestion_result: Some(reverse_chronological_command.command),
                            };
                        }
                    }

                    // If we have no command anywhere in history with a matching prefix, fallback to the first completer result.
                    let Some(completion_context) = completion_context else {
                        return AutoSuggestionResult {
                            buffer_text,
                            autosuggestion_result: None,
                        };
                    };
                    let completion_result = completer::suggestions(
                        buffer_text.as_str(),
                        buffer_text.len(),
                        session_env_vars.as_ref(),
                        CompleterOptions {
                            match_strategy: MatchStrategy::CaseSensitive,
                            fallback_strategy: CompletionsFallbackStrategy::FilePaths,
                            suggest_file_path_completions_only: false,
                            parse_quotes_as_literals: false,
                        },
                        &completion_context,
                    )
                    .await;

                    let autosuggestion = completion_result.and_then(|result| {
                        let replacement_span = result.replacement_span;
                        result
                            .suggestions
                            .into_iter()
                            .map(|s| {
                                // Reproduce the final buffer text with the autosuggestion since the
                                // completer only gives the replacement span of the suggestion.
                                format!(
                                    "{}{}",
                                    &buffer_text[..replacement_span.start()],
                                    s.replacement()
                                )
                            })
                            .find(|suggestion| !ignored_suggestions.contains(suggestion))
                    });

                    AutoSuggestionResult {
                        buffer_text,
                        autosuggestion_result: autosuggestion,
                    }
                },
                Self::on_autosuggestion_result,
                move |_, _| {
                    if let Some(session) = completion_session {
                        session.cancel_active_commands();
                    }
                },
            )
            .abort_handle();

        self.set_autosuggestion_future(abort_handle);
    }

    fn is_potential_expansion(
        token: &Spanned<String>,
        cursor_pos: usize,
        executing: Executing,
    ) -> bool {
        match executing {
            // Expansion was triggered by user entering the command to be executed.
            // To expand, cursor must be exactly at the end of the token.
            Executing::Yes => token.span().end() == cursor_pos,
            // Expansion was triggered by user pressing Space at the end of a token.
            // To expand, cursor must be one index after the end of the token.
            Executing::No => token.span().end() + 1 == cursor_pos,
        }
    }

    /// Gets the abbreviation and abbreviation value, or alias and alias value, given
    /// a command, if they exist. Will return None if the conditions for alias
    /// expansion are not met.
    fn get_valid_abbreviation_or_alias_for_expansion<'a>(
        &self,
        command: Option<&'a LiteCommand>,
        cursor_pos: usize,
        executing: Executing,
        session_context: &'a SessionContext,
        ctx: &mut ViewContext<Self>,
    ) -> Option<(&'a Spanned<String>, &'a str)> {
        // An alias must be the first token of a command
        let first_token = command?.parts.first()?;

        if !Self::is_potential_expansion(first_token, cursor_pos, executing) {
            return None;
        }

        // If there is an abbreviation, we expand it as long as we aren't executing.
        // In fish, an alias formatted like `ls=echo Hello && ls` would get expanded
        // twice if we also performed expansion on enter.
        if matches!(executing, Executing::No)
            && let Some(abbr_value) = session_context
                .session
                .abbreviation_value(&first_token.item)
        {
            return Some((first_token, abbr_value));
        }

        // We only expand aliases if the user has turned the setting on.
        if self.should_expand_aliases(ctx) {
            let alias_value = session_context.session.alias_value(&first_token.item)?;
            if !is_expandable_alias(&first_token.item, alias_value) {
                return None;
            }

            return Some((first_token, alias_value));
        }
        None
    }

    /// Function to check whether the previous token was a valid command abbreviation
    /// or alias and handle expansion. This should only be called after the user has
    /// entered a space into the input editor.
    fn run_expansion_on_space(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(expansion_info) = self.run_expansion_internal(Executing::No, ctx) {
            self.expand_alias(expansion_info.byte_range, &expansion_info.alias_value, ctx);
        }
    }

    /// Function that checks whether the current token was a valid command abbreviation
    /// or alias, and returns a String representing the input buffer with the expanded
    /// text. This should be called after the user has pressed Enter to execute the
    /// command.
    fn get_expanded_command_on_execute(&mut self, ctx: &mut ViewContext<Self>) -> Option<String> {
        self.run_expansion_internal(Executing::Yes, ctx)
            .and_then(|expansion_info| {
                let mut text = expansion_info.buffer_text;
                let is_valid_byte_range = text.is_char_boundary(expansion_info.byte_range.start)
                    && text.is_char_boundary(expansion_info.byte_range.end);
                is_valid_byte_range.then(|| {
                    text.replace_range(expansion_info.byte_range, &expansion_info.alias_value);
                    text
                })
            })
    }

    /// Helper function that handles whether there is a valid expansion based on
    /// the current input buffer and cursor position. Returns info needed to
    /// perform the expansion.
    fn run_expansion_internal(
        &mut self,
        executing: Executing,
        ctx: &mut ViewContext<Self>,
    ) -> Option<ExpansionInfo> {
        let session_context = self.completion_session_context(ctx)?;
        let editor = self.editor.as_ref(ctx);
        editor.single_cursor_to_point(ctx)?;
        let buffer_text = editor.buffer_text(ctx);
        let cursor_pos = editor.end_byte_index_of_last_selection(ctx);
        let command = command_at_cursor_position(
            buffer_text.as_str(),
            session_context.escape_char(),
            cursor_pos,
        );

        self.get_valid_abbreviation_or_alias_for_expansion(
            command.as_ref(),
            cursor_pos.as_usize(),
            executing,
            &session_context,
            ctx,
        )
        .map(|(alias, alias_value)| ExpansionInfo {
            alias_value: alias_value.into(),
            buffer_text,
            byte_range: alias.span().start()..cursor_pos.as_usize(),
        })
    }

    fn expand_alias(
        &mut self,
        replacement_range: Range<usize>,
        alias_value: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        let alias_value_with_space = format!("{alias_value} ");
        self.editor.update(ctx, |input, ctx| {
            input.select_and_replace(
                &alias_value_with_space,
                [ByteOffset::from(replacement_range.start)
                    ..ByteOffset::from(replacement_range.end)],
                PlainTextEditorViewAction::ExpandAlias,
                ctx,
            );
        });
    }

    /// If at least one input is being synced, emit an event that other
    /// terminal views can decide to process based on their sync state.
    fn send_input_sync_event(&self, edit_origin: &EditOrigin, ctx: &mut ViewContext<Self>) {
        let is_syncing_inputs =
            SyncedInputState::as_ref(ctx).is_syncing_any_inputs(ctx.window_id());

        if is_syncing_inputs
                    // If the edit we're applying in `handle_editor_event`
                    //came from another synced terminal,
                    // don't emit a new event which would create a cycle
                    && *edit_origin != EditOrigin::SyncedTerminalInput
                    // Similarly, only emit an event from the session the user is typing in
                    && self.focus_handle.as_ref().is_none_or(|h| h.is_focused(ctx))
        {
            let buffer = self.editor.as_ref(ctx).buffer_text(ctx);
            ctx.emit(Event::SyncInput(
                SyncInputType::InputEditorContentsChanged {
                    contents: Arc::new(buffer),
                },
            ));
        }
    }

    fn handle_editor_event(&mut self, event: &EditorEvent, ctx: &mut ViewContext<Self>) {
        // We want to clear the token description hover on any editor action
        self.hide_x_ray(ctx);

        if !matches!(event, EditorEvent::InsertLastWordPrevCommand) {
            self.update_last_word_insertion_state();
        }

        let is_ai_input_enabled = self.ai_input_model.as_ref(ctx).is_ai_input_enabled();

        match event {
            EditorEvent::Edited(edit_origin) => {
                // We should ideally be handling all `Edited` events, not just those that are
                // marked EditOrigin. However, we receive the notification that the block has
                // completed, in the same event we clear the input box per-command. Due to how
                // events are dispatched in the UI framework, we would receive an Edited event
                // immediately from clearing the input box. But we don't want that.
                // Only processing the user typed events should be good enough here.

                if matches!(
                    edit_origin,
                    EditOrigin::UserTyped | EditOrigin::UserInitiated
                ) {
                    self.model.lock().set_is_input_dirty(true);
                }

                if *edit_origin == EditOrigin::UserTyped
                    && !ctx
                        .model(&self.input_render_state_model_handle)
                        .editor_modified_since_block_finished()
                {
                    self.input_render_state_model_handle.update(
                        ctx,
                        |input_render_state_model, _| {
                            input_render_state_model.set_editor_modified_since_block_finished(true);
                        },
                    );
                    ctx.notify();
                }

                let is_editor_empty = self.editor.as_ref(ctx).is_empty(ctx);
                if is_editor_empty != self.is_editor_empty_on_last_edit {
                    self.is_editor_empty_on_last_edit = is_editor_empty;
                    ctx.emit(Event::InputEmptyStateChanged {
                        is_empty: is_editor_empty,
                        reason: InputEmptyStateChangeReason::Edited,
                    });
                }

                let is_ai_input_enabled = self.ai_input_model.as_ref(ctx).is_ai_input_enabled();

                let mut short_circuit_highlighting = false;
                let mut check_alias_expansion = false;

                self.editor.read(ctx, |editor, editor_ctx| {
                    let last_action = editor.get_last_action(editor_ctx);
                    if Some(PlainTextEditorViewAction::Space) == last_action
                        && *edit_origin == EditOrigin::UserTyped
                    {
                        check_alias_expansion = true;
                    }

                    if SHORT_CIRCUIT_HIGHLIGHTING_ACTIONS.contains(&last_action) {
                        short_circuit_highlighting = true;
                    }
                });

                // Force AI mode if buffer contains any attachment patterns (blocks, drive objects, diffs)
                if AISettings::as_ref(ctx).is_any_ai_enabled(ctx) && edit_origin.is_user() {
                    let buffer_text = self.buffer_text(ctx);
                    if Self::buffer_contains_attachment_patterns(&buffer_text) {
                        self.ensure_agent_mode_for_ai_features(
                            false,
                            Some(InputTypeAutoDetectionSource::AttachmentForcedAi),
                            ctx,
                        );
                    }
                }

                if check_alias_expansion {
                    self.run_expansion_on_space(ctx);
                }

                // Don't run NLD autodetection when an inline menu is open (slash commands,
                // conversation menu, model selector), as the buffer contents are being used as
                // a search query for the menu rather than as a command/prompt.
                let is_inline_menu_open = self
                    .suggestions_mode_model
                    .as_ref(ctx)
                    .is_inline_menu_open();

                let should_run_ai_input_detection = match edit_origin {
                    // Edits made by the local user should trigger autodetection, if
                    // it is enabled.
                    EditOrigin::UserInitiated
                    | EditOrigin::UserTyped
                    | EditOrigin::SyncedTerminalInput => {
                        !is_inline_menu_open
                            && self
                                .ai_input_model
                                .as_ref(ctx)
                                .should_run_input_autodetection(ctx)
                    }
                    // System edits should never trigger autodetection.
                    EditOrigin::RemoteEdit | EditOrigin::SystemEdit => false,
                };

                // Abort any autodetection work on the old buffer state.
                self.ai_input_model.update(ctx, |controller, _| {
                    controller.abort_in_progress_detection();
                });
                // Abort any inflight request to generate a Next Command suggestion.
                self.next_command_model.update(ctx, |model, _| {
                    model.abort_inflight_request();
                });

                if self.should_apply_decorations(ctx)
                    || should_run_ai_input_detection
                    || is_ai_input_enabled
                {
                    let mut mode = InputBackgroundJobOptions::default();

                    if self.should_apply_decorations(ctx) {
                        mode = mode.with_command_decoration();
                    }

                    if should_run_ai_input_detection {
                        mode = mode.with_ai_input_detection();
                    }

                    if short_circuit_highlighting {
                        self.run_input_background_jobs(mode, ctx);
                    } else {
                        let _ = self.debounce_input_background_tx.try_send(mode);
                    }
                }

                let is_input_mode_locked = self.ai_input_model.as_ref(ctx).is_input_type_locked();
                let buffer_text = self.buffer_text(ctx);

                // If the last buffer didn't start with the AI input prefix and the current buffer does, then enable AI input.
                if FeatureFlag::AgentMode.is_enabled()
                    && !FeatureFlag::AgentView.is_enabled()
                    && AISettings::as_ref(ctx).is_any_ai_enabled(ctx)
                    && (!is_ai_input_enabled || !is_input_mode_locked)
                {
                    if buffer_text.starts_with(AI_INPUT_PREFIX)
                        && *edit_origin == EditOrigin::UserTyped
                    {
                        let last_buffer_text = self.editor.as_ref(ctx).last_buffer_text(ctx);

                        if !last_buffer_text.starts_with(AI_INPUT_PREFIX) {
                            // Remove the prefix from the editor contents.
                            let is_input_buffer_empty =
                                self.editor.update(ctx, |editor_view, ctx| {
                                    if let Some(query) =
                                        editor_view.buffer_text(ctx).strip_prefix(AI_INPUT_PREFIX)
                                    {
                                        editor_view.set_buffer_text(query, ctx);
                                    }
                                    editor_view.buffer_text(ctx).is_empty()
                                });

                            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                                ai_input_model.set_input_config(
                                    InputConfig {
                                        input_type: InputType::AI,
                                        is_locked: true,
                                    },
                                    is_input_buffer_empty,
                                    Some(InputTypeAutoDetectionSource::AgentModePrefix),
                                    ctx,
                                );
                            });
                        }
                    } else if buffer_text.is_empty() && is_input_mode_locked {
                        self.ai_input_model.update(ctx, |input_model, ctx| {
                            input_model.set_input_config_for_classic_mode(
                                input_model
                                    .input_config()
                                    .unlocked_if_autodetection_enabled(false, ctx),
                                ctx,
                            );
                        });
                    }

                    ctx.notify();
                }

                let ai_settings = AISettings::as_ref(ctx);
                if FeatureFlag::AgentView.is_enabled() && buffer_text.is_empty() {
                    let last_buffer_text = self.editor.as_ref(ctx).last_buffer_text(ctx);
                    let was_shell_mode_prefix_stripped =
                        last_buffer_text == TERMINAL_INPUT_PREFIX && buffer_text.is_empty();

                    let is_fullscreen_agent_view_active =
                        self.agent_view_controller.as_ref(ctx).is_fullscreen();
                    let current_input_config = self.ai_input_model.as_ref(ctx).input_config();

                    // We should re-enable autodetection if the user overrode an autodetection
                    // result:
                    // * In agent view, this means the user overrode a mis-classified shell command
                    //   to be an agent prompt.
                    // * In terminal view, this eans the user overrode a mis-classified agent prompt
                    //   to a terminal command.
                    let should_reenable_autodetection = (ai_settings
                        .is_ai_autodetection_enabled(ctx)
                        && is_fullscreen_agent_view_active
                        && current_input_config.is_ai()
                        && current_input_config.is_locked
                        && !was_shell_mode_prefix_stripped)
                        || (ai_settings.is_nld_in_terminal_enabled(ctx)
                            && !self.agent_view_controller.as_ref(ctx).is_active()
                            && current_input_config.is_shell()
                            && current_input_config.is_locked);
                    if should_reenable_autodetection {
                        self.ai_input_model.update(ctx, |input_model, ctx| {
                            input_model.enable_autodetection(input_model.input_type(), ctx);
                        });
                        ctx.notify();
                    }
                }

                // If the last buffer didn't start with the terminal input prefix and the current buffer does, then enable terminal input and lock it.
                let is_locked_shell_mode = !is_ai_input_enabled && is_input_mode_locked;
                let is_agent_view_active = self.agent_view_controller.as_ref(ctx).is_active();
                let is_agent_in_control_or_tagged_in = self
                    .model
                    .lock()
                    .block_list()
                    .active_block()
                    .is_agent_in_control_or_tagged_in();
                if FeatureFlag::AgentMode.is_enabled()
                    && !is_locked_shell_mode
                    && (!FeatureFlag::AgentView.is_enabled() || is_agent_view_active)
                    && !is_agent_in_control_or_tagged_in
                {
                    let buffer_text = self.buffer_text(ctx);
                    if buffer_text.starts_with(TERMINAL_INPUT_PREFIX)
                        && *edit_origin == EditOrigin::UserTyped
                    {
                        let last_buffer_text = self.editor.as_ref(ctx).last_buffer_text(ctx);

                        if !last_buffer_text.starts_with(TERMINAL_INPUT_PREFIX) {
                            // Remove the prefix from the editor contents.
                            let is_input_buffer_empty =
                                self.editor.update(ctx, |editor_view, ctx| {
                                    if let Some(command) = editor_view
                                        .buffer_text(ctx)
                                        .strip_prefix(TERMINAL_INPUT_PREFIX)
                                    {
                                        editor_view.set_buffer_text(command, ctx);
                                    }
                                    editor_view.buffer_text(ctx).is_empty()
                                });

                            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                                ai_input_model.set_input_config(
                                    InputConfig {
                                        input_type: InputType::Shell,
                                        is_locked: true,
                                    },
                                    is_input_buffer_empty,
                                    Some(InputTypeAutoDetectionSource::ShellPrefix),
                                    ctx,
                                );
                            });
                        }
                    }
                    ctx.notify();
                }

                // We only sync on EditorEvent::Edited events because we're only
                // syncing terminal input editor contents, not the full
                // functionality of the terminal input in each blocklist
                // e.g., we don't want to sync EditorEvent::CmdUpOnFirstRow.
                self.send_input_sync_event(edit_origin, ctx);

                // Distinguish edits the completion system applied itself (accepting or
                // cycling through a candidate) from edits the user made (typing,
                // backspacing, pasting). System-applied edits are allowed to diverge from
                // the original completion query so that classic cycling keeps working;
                // user edits still have to be revalidated against that query.
                let is_user_edit = !matches!(
                    self.editor.as_ref(ctx).get_last_action(ctx),
                    Some(
                        PlainTextEditorViewAction::AcceptCompletionSuggestion
                            | PlainTextEditorViewAction::CycleCompletionSuggestion
                    )
                );

                let mode = self.suggestions_mode_model.as_ref(ctx).mode().clone();
                match &mode {
                    InputSuggestionsMode::CompletionSuggestions {
                        replacement_start,
                        buffer_text_original,
                        completion_results,
                        trigger,
                        ..
                    } => {
                        let replacement_start = *replacement_start;
                        let editor_text = self.buffer_text(ctx);
                        let cursor_position = self.start_byte_index_of_last_selection(ctx);
                        let current_word =
                            editor_text.get(replacement_start..cursor_position.as_usize());
                        let current_selected_item =
                            self.input_suggestions.as_ref(ctx).get_selected_item_text();
                        let selected_item_differs_from_current_word = current_selected_item
                            .zip(current_word)
                            .map(|(selected_item, current_word)| selected_item != current_word)
                            .unwrap_or(true);

                        // To support completions-as-you-type x classic completions,
                        // we need to make sure we don't recompute the completion results
                        // as the user cycles (which inserts into buffer and thus is treated
                        // as an edit). Thus, when using the two features together, we only
                        // recompute the result set if the selected item doesn't match the
                        // current word span.
                        let old_buffer_text_original = buffer_text_original.clone();
                        if *trigger == CompletionsTrigger::AsYouType
                            && (!self.is_classic_completions_enabled(ctx)
                                || (self.is_classic_completions_enabled(ctx)
                                    && selected_item_differs_from_current_word))
                        {
                            // For as-you-type completions, we recalculate suggestions rather than
                            // filtering, since typing could involve moving to a new parameter
                            // within a given command, rather than being a strict subset as is the
                            // case with manual tab completions.
                            self.open_completion_suggestions(CompletionsTrigger::AsYouType, ctx);
                            self.maybe_generate_autosuggestion(ctx);

                            // Since tab completions are async, we should close the
                            // menu if it's been some time and the menu still hasn't updated,
                            // otherwise the user will see an old completions menu even while
                            // the buffer text has changed. We wait with a delay so that way
                            // the menu doesn't close right away and open away right after if
                            // the completions finish quickly, since that causes a jittery UX.
                            let _ = ctx.spawn(
                                async move {
                                    warpui::r#async::Timer::after(Duration::from_millis(750)).await;
                                    old_buffer_text_original
                                },
                                move |input, old_buffer_text_original, ctx| {
                                    if let InputSuggestionsMode::CompletionSuggestions {
                                        buffer_text_original,
                                        ..
                                    } = input.suggestions_mode_model.as_ref(ctx).mode()
                                    {
                                        // The menu hasn't changed since last time so
                                        // close it for now. If the menu is truly delayed,
                                        // the completions callback will eventually open it.
                                        if old_buffer_text_original == *buffer_text_original {
                                            input.close_input_suggestions(true, ctx);
                                        }
                                    }
                                },
                            );
                        } else {
                            let buffer_text_original = buffer_text_original.clone();
                            let completion_results = completion_results.clone();
                            let should_close = self.update_tab_completion_menu(
                                replacement_start,
                                buffer_text_original.as_str(),
                                &completion_results,
                                is_user_edit,
                                ctx,
                            );
                            if should_close {
                                self.close_input_suggestions(
                                    /*should_focus_input=*/ true, ctx,
                                );
                            }
                        }
                    }
                    InputSuggestionsMode::StaticWorkflowEnumSuggestions {
                        cursor_point, ..
                    }
                    | InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
                        cursor_point, ..
                    } => {
                        let cursor_point = *cursor_point;
                        let point = self.editor.as_ref(ctx).first_selection_end_to_point(ctx);
                        let should_close = point != cursor_point;

                        if should_close {
                            self.close_input_suggestions(/*should_focus_input=*/ true, ctx);
                        }
                    }
                    InputSuggestionsMode::HistoryUp { .. } => {
                        // In HistoryUp mode, we replace the buffer as options
                        // are selected.
                        // We also dismiss the suggestion menu if the buffer
                        // is edited such that it doesn't exactly match
                        // the selected suggestion.

                        if let Some(selected_text) =
                            self.input_suggestions.as_ref(ctx).get_selected_item_text()
                        {
                            if *selected_text.to_string()
                                == self.editor.as_ref(ctx).buffer_text(ctx)
                            {
                                return;
                            }

                            let has_active_ai_block =
                                self.model.lock().block_list().has_active_ai_block(ctx);
                            // We only focus the input if there is no active AI
                            // block. Otherwise, the input is incorrectly focused
                            // when executing an AI query from the history menu.
                            self.close_input_suggestions(
                                !has_active_ai_block, /*should_focus_input=*/
                                ctx,
                            );
                        }
                    }
                    InputSuggestionsMode::Closed => {
                        if !self.can_query_history(ctx) {
                            return;
                        }

                        let editor = self.editor.as_ref(ctx);
                        let buffer_text = editor.buffer_text(ctx);

                        self.maybe_generate_autosuggestion(ctx);

                        if buffer_text.is_empty()
                            && self.workflows_state.selected_workflow_state.is_some()
                        {
                            self.clear_current_workflow(ctx);
                        }

                        if self.should_show_completions_while_typing(ctx)
                            && matches!(edit_origin, EditOrigin::UserTyped)
                        {
                            self.open_completion_suggestions(CompletionsTrigger::AsYouType, ctx);
                        }
                    }

                    InputSuggestionsMode::InlineHistoryMenu { .. } => {
                        let mismatched = self
                            .inline_history_menu_view
                            .as_ref(ctx)
                            .model()
                            .as_ref(ctx)
                            .selected_item()
                            .is_some_and(|selected_item_text| {
                                selected_item_text.command
                                    != self.editor.as_ref(ctx).buffer_text(ctx)
                            });
                        if mismatched {
                            self.suggestions_mode_model.update(ctx, |model, ctx| {
                                model.set_mode(InputSuggestionsMode::Closed, ctx);
                            });
                            ctx.notify();
                        }
                    }
                }
            }
            EditorEvent::BufferReplaced => {
                let ai_input_model = self.ai_input_model.as_ref(ctx);
                if FeatureFlag::AgentMode.is_enabled()
                    && AISettings::as_ref(ctx).is_any_ai_enabled(ctx)
                    && !ai_input_model.is_ai_input_enabled()
                    && ai_input_model.is_input_type_locked()
                {
                    // If this edit effectively emptied the buffer and we're in shell mode,
                    // unlock the input so autodetection can kick in.
                    self.ai_input_model.update(ctx, |input_model, ctx| {
                        input_model.set_input_config_for_classic_mode(
                            input_model
                                .input_config()
                                .unlocked_if_autodetection_enabled(false, ctx),
                            ctx,
                        );
                    });
                    ctx.notify();
                }
            }
            EditorEvent::SelectionChanged => {
                let mode = self.suggestions_mode_model.as_ref(ctx).mode().clone();
                let is_completion_suggestions =
                    matches!(mode, InputSuggestionsMode::CompletionSuggestions { .. });
                if is_completion_suggestions && !self.cursor_positioned_for_completion(ctx) {
                    self.close_input_suggestions(/*should_focus_input=*/ true, ctx);
                } else {
                    match &mode {
                        InputSuggestionsMode::HistoryUp { .. } | InputSuggestionsMode::Closed => {}
                        InputSuggestionsMode::CompletionSuggestions {
                            replacement_start,
                            buffer_text_original,
                            completion_results,
                            ..
                        } => {
                            let replacement_start = *replacement_start;
                            let buffer_text_original = buffer_text_original.clone();
                            let completion_results = completion_results.clone();
                            // A selection change is a cursor move, not a buffer edit, so it
                            // never counts as a user edit for invalidation purposes.
                            let should_close = self.update_tab_completion_menu(
                                replacement_start,
                                buffer_text_original.as_str(),
                                &completion_results,
                                /*is_user_edit=*/ false,
                                ctx,
                            );

                            if should_close {
                                self.close_input_suggestions(
                                    /*should_focus_input=*/ true, ctx,
                                );
                            }
                        }
                        InputSuggestionsMode::StaticWorkflowEnumSuggestions {
                            cursor_point,
                            ..
                        }
                        | InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
                            cursor_point,
                            ..
                        } => {
                            let cursor_point = *cursor_point;
                            let point = self.editor.as_ref(ctx).first_selection_end_to_point(ctx);
                            let should_close = point != cursor_point;

                            if should_close {
                                self.close_input_suggestions(
                                    /*should_focus_input=*/ true, ctx,
                                );
                            }
                        }

                        InputSuggestionsMode::InlineHistoryMenu { .. } => {
                            // Inline history menu handles its own selection state
                        }
                    }
                }
            }
            EditorEvent::AutosuggestionAccepted {
                autosuggestion_type,
                ..
            } => {
                ctx.emit(Event::AutosuggestionAccepted);

                self.input_suggestions
                    .update(ctx, |input_suggestions, ctx| {
                        // We should not restore the buffer to the old state since we're accepting an autosuggestion from the new state.
                        input_suggestions.exit(false, ctx);
                    });
                match autosuggestion_type {
                    AutosuggestionType::Command {
                        was_intelligent_autosuggestion,
                    } => {
                        // Switch to shell input mode but preserve current lock state when accepting a command autosuggestion.
                        self.ai_input_model.update(ctx, |input_model, ctx| {
                            input_model.set_input_type(
                                InputType::Shell,
                                Some(InputTypeAutoDetectionSource::CommandAutosuggestionAccepted),
                                ctx,
                            );
                        });
                        if !was_intelligent_autosuggestion {
                            // This accepted autosuggestion count is used to determine whether to show the right arrow to accept icon
                            // when there's an autosuggestion while the input buffer is not empty.
                            // So it should only be incremented when an autosuggestion is accepted while the buffer is not empty (is NOT intelligent/zero-state).
                            InputSettings::handle(ctx).update(ctx, |input_settings, ctx| {
                                let current_count =
                                    *input_settings.autosuggestion_accepted_count.value();
                                if current_count < MAX_TIMES_TO_SHOW_AUTOSUGGESTION_HINT {
                                    let new_count = if current_count < 0 {
                                        // Note: there was a bug in the previous implementation of this method which would
                                        // cause it to overflow the i8 value to a negative value. In that case, we know
                                        // that the user has definitely accepted at _least_ 128 autosuggestions, so we can
                                        // set it to the maximum relevant value: MAX_TIMES_TO_SHOW_AUTOSUGGESTION_HINT
                                        MAX_TIMES_TO_SHOW_AUTOSUGGESTION_HINT
                                    } else {
                                        current_count + 1
                                    };

                                    report_if_error!(
                                        input_settings
                                            .autosuggestion_accepted_count
                                            .set_value(new_count, ctx)
                                    )
                                }
                            })
                        }
                    }
                };
            }
            EditorEvent::Navigate(NavigationKey::Up) => {
                self.editor_up(ctx);
            }
            EditorEvent::Navigate(NavigationKey::Down) => {
                self.editor_down(ctx);
            }
            EditorEvent::Navigate(NavigationKey::PageUp) => {
                self.editor_page_up(ctx);
            }
            EditorEvent::Navigate(NavigationKey::PageDown) => {
                self.editor_page_down(ctx);
            }
            EditorEvent::Navigate(NavigationKey::Tab) => {
                self.input_tab(ctx);
            }
            EditorEvent::Navigate(NavigationKey::ShiftTab) => {
                self.input_shift_tab(ctx);
            }
            EditorEvent::Enter => self.input_enter(ctx),
            EditorEvent::CmdEnter => self.input_cmd_enter(ctx),
            EditorEvent::CtrlEnter => {}
            EditorEvent::Escape => self.editor_escape(ctx),
            EditorEvent::CtrlC { .. } => {
                self.close_input_suggestions(/*should_focus_input=*/ true, ctx);

                self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                    ai_input_model.set_input_config_for_classic_mode(
                        InputConfig {
                            input_type: InputType::Shell,
                            is_locked: true,
                        }
                        .unlocked_if_autodetection_enabled(false, ctx),
                        ctx,
                    );
                });
                ctx.emit(Event::CtrlC);
            }
            EditorEvent::DeleteAllLeft => {
                if self.ai_input_model.as_ref(ctx).is_ai_input_enabled() {
                    let new_input_type = InputType::Shell;
                    self.maybe_send_autodetection_telemetry_on_manual_toggle(new_input_type, ctx);
                    self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                        ai_input_model.set_input_config_for_classic_mode(
                            InputConfig {
                                input_type: new_input_type,
                                is_locked: true,
                            }
                            .unlocked_if_autodetection_enabled(false, ctx),
                            ctx,
                        );
                    });
                } else if self.ai_input_model.as_ref(ctx).is_input_type_locked() {
                    let is_agent_view_fullscreen =
                        self.agent_view_controller.as_ref(ctx).is_fullscreen();
                    if is_agent_view_fullscreen {
                        self.exit_shell_mode_to_ai(ctx);
                    }
                }
            }
            EditorEvent::CmdUpOnFirstRow => ctx.emit(Event::SelectRecentBlocks { count: 1 }),
            EditorEvent::Copy => ctx.emit(Event::Copy),
            EditorEvent::UnhandledModifierKeyOnEditor(keystroke) => {
                ctx.emit(Event::UnhandledModifierKeyOnEditor(keystroke.clone()))
            }
            EditorEvent::ClearParentSelections => {
                ctx.emit(Event::ClearSelectionsWhenShellMode);
            }
            EditorEvent::HideXRay => {
                self.hide_x_ray(ctx);
            }
            EditorEvent::TryToShowXRay(token_at) => {
                if self.ai_input_model.as_ref(ctx).is_ai_input_enabled() {
                    // Don't show command x-ray for AI queries.
                    return;
                }

                match token_at {
                    CommandXRayAnchor::Cursor => {
                        let pos = self.start_byte_index_of_first_selection(ctx);
                        self.start_xray_at_offset(pos, CommandXRayTrigger::Keystroke, ctx);
                    }
                    CommandXRayAnchor::Hover(mouse_position) => {
                        if let Some(offset) = self.start_byte_index_at_point(mouse_position, ctx)
                            && !self.suggestions_mode_model.as_ref(ctx).is_visible()
                        {
                            self.start_xray_at_offset(offset, CommandXRayTrigger::Hover, ctx);
                        }
                    }
                }
            }
            EditorEvent::InsertLastWordPrevCommand => self.insert_last_word_previous_command(ctx),
            // For this particular view, the terminal Input, we ignore search direction because in
            // this context, search means search through History which isn't actually sensitive to
            // left/right direction.
            EditorEvent::Search { term, .. } => {
                ctx.emit(Event::ShowCommandSearch(CommandSearchOptions {
                    filter: Some(QueryFilter::History),
                    init_content: InitContent::Custom(term.clone().unwrap_or("".to_owned())),
                }));
            }
            // For this view, the terminal Input, we do not support ex-commands. The closest
            // analogy we have in this view would be workflows. So, open command search with the
            // workflows filter to handle this event.
            EditorEvent::ExCommand => ctx.emit(Event::ShowCommandSearch(CommandSearchOptions {
                filter: Some(QueryFilter::Workflows),
                init_content: InitContent::Custom("".to_owned()),
            })),
            EditorEvent::VimStatusUpdate => ctx.notify(),
            EditorEvent::BackspaceOnEmptyBuffer | EditorEvent::BackspaceAtBeginningOfBuffer => {
                self.handle_backspace_at_buffer_boundary(ctx);
            }
            EditorEvent::EmacsBindingUsed => {
                ctx.emit(Event::EmacsBindingUsed);
            }
            EditorEvent::UpdatePeers { operations } => {
                self.latest_buffer_operations.extend(operations.to_vec());

                // TODO (suraj): we might want to push down the buffer ID to the buffer
                // and have it returned as part of the event. That way, we aren't subject
                // to any skew of the block ID from the time the event is emitted (when the edit
                // is processed) to the time when we query the block ID (now).
                ctx.emit(Event::EditorUpdated {
                    block_id: self.model.lock().block_list().active_block_id().clone(),
                    operations: operations.clone(),
                })
            }
            EditorEvent::MiddleClickPaste => {
                ctx.emit(Event::InputFocusedFromMiddleClick);
            }
            EditorEvent::Focused => ctx.emit(Event::EditorFocused),

            EditorEvent::VoiceStateUpdated {
                is_listening,
                is_transcribing,
            } => {
                if *is_listening || *is_transcribing {
                    // Show voice status as placeholder when the buffer is empty.
                    if self.editor.as_ref(ctx).is_empty(ctx) {
                        let placeholder = if *is_listening {
                            "Listening..."
                        } else {
                            "Transcribing..."
                        };
                        self.editor.update(ctx, |editor, ctx| {
                            editor.set_placeholder_text(placeholder, ctx);
                        });
                    }
                } else {
                    self.set_zero_state_hint_text(ctx);
                }
            }
            EditorEvent::Paste => {
                self.process_paste_event(ctx);
            }

            EditorEvent::IgnoreAutosuggestion { suggestion } => {
                IgnoredSuggestionsModel::handle(ctx).update(ctx, |model, ctx| {
                    model.add_ignored_suggestion(
                        suggestion.clone(),
                        SuggestionType::ShellCommand,
                        ctx,
                    );
                });

                self.editor.update(ctx, |editor, ctx| {
                    editor.clear_autosuggestion(ctx);
                });
            }
            _ => {}
        }
    }

    fn process_paste_event(&mut self, ctx: &mut ViewContext<Self>) {
        let content = ctx.clipboard().read();
        self.insert_clipboard_text_content(ctx, content);
    }

    /// Insert clipboard text content (paths / plaintext)
    fn insert_clipboard_text_content(
        &self,
        ctx: &mut ViewContext<Self>,
        content: ClipboardContent,
    ) {
        let clipboard_content_str = self
            .editor
            .read(ctx, |editor, _| editor.clipboard_text_content(content));
        self.editor.update(ctx, |editor, ctx| {
            editor.user_initiated_insert(
                &clipboard_content_str,
                PlainTextEditorViewAction::Paste,
                ctx,
            );
        });
    }

    /// Handles backspace at the buffer boundary (empty buffer or cursor at
    /// position 0). Covers shell-prefix exit and legacy
    /// classic-mode AI icon toggling in a single function.
    fn handle_backspace_at_buffer_boundary(&mut self, ctx: &mut ViewContext<Self>) {
        match self.prefix_mode(ctx) {
            InputPrefixMode::Shell => {
                if self.agent_view_controller.as_ref(ctx).is_fullscreen() {
                    self.exit_shell_mode_to_ai(ctx);
                    ctx.notify();
                }
                return;
            }
            InputPrefixMode::None => {}
        }

        // Legacy classic-mode AI icon toggling (only reachable when Agent View
        // is inactive and no prefix mode is set).
        if !self.ai_input_model.as_ref(ctx).is_ai_input_enabled() {
            return;
        }

        let is_udi_enabled = InputSettings::as_ref(ctx).is_universal_developer_input_enabled(ctx);
        if is_udi_enabled {
            return;
        }

        // When the agent view is active, the classic-mode AI icon toggling and follow-up clearing
        // logic below does not apply.
        if FeatureFlag::AgentView.is_enabled() && self.agent_view_controller.as_ref(ctx).is_active()
        {
            return;
        }

        // If we have an AI follow up icon, backspace should clear that icon.
        if self
            .ai_context_model
            .as_ref(ctx)
            .is_targeting_existing_conversation(ctx)
        {
            self.ai_context_model.update(ctx, |ai_context_model, ctx| {
                ai_context_model.set_pending_query_state_for_new_conversation(
                    // This origin is unused in this codepath, which doesn't get called when
                    // AgentView is enabled.
                    AgentViewEntryOrigin::Input {
                        was_prompt_autodetected: false,
                    },
                    ctx,
                );
            });
        } else {
            // Otherwise backspace away the AI icon.
            let new_input_type = self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                let new_input_config = ai_input_model.input_config().with_toggled_type().locked();
                let new_input_type = new_input_config.input_type;
                ai_input_model.set_input_config_for_classic_mode(new_input_config, ctx);
                new_input_type
            });
            self.maybe_send_autodetection_telemetry_on_manual_toggle(new_input_type, ctx);
        }
    }

    /// Updates the tab completion menu given the current text of the editor and location of the
    /// cursor. Returns whether the input suggestions should be closed.
    ///
    /// If the original text is still within the buffer up to where the cursor is, we filter the
    /// suggestions to only show the suggestions that match the current word. If the original text
    /// is _not_ within the buffer up to the cursor, we close the input suggestions.
    fn update_tab_completion_menu(
        &self,
        replacement_start: usize,
        buffer_text_original: &str,
        completion_results: &SuggestionResults,
        is_user_edit: bool,
        ctx: &mut ViewContext<Input>,
    ) -> bool {
        let editor_text = self.editor.as_ref(ctx).buffer_text(ctx);
        let cursor_position = self.start_byte_index_of_last_selection(ctx);
        let text_up_to_cursor = &editor_text[0..cursor_position.as_usize()];

        // If the cursor position is before the start of the replacement span,
        // then we should definitely close the menu.
        if cursor_position.as_usize() < replacement_start {
            return true;
        }

        // If the buffer no longer starts with the original buffer text,
        // then we should close the completion menu because the result set
        // was based on a different query.
        //
        // Classic completions get an exemption from this check, but only for
        // system-applied edits: when the completion system cycles through fuzzy
        // matches it rewrites the buffer to each candidate, and the text up to the
        // cursor may no longer start with the original buffer text. Keeping the
        // result set alive in that case is what lets cycling work.
        //
        // A user edit (typing, backspacing, pasting) that diverges from the original
        // query must still invalidate the result set. Otherwise a Tab followed by
        // Backspace past the replacement boundary would leave stale suggestions on
        // screen (and an empty prefix would re-show the entire original result set).
        if !text_up_to_cursor.starts_with(buffer_text_original)
            && (!self.is_classic_completions_enabled(ctx) || is_user_edit)
        {
            // Close the input suggestions since the buffer was edited to no longer
            // contain the text that triggered tab completion.
            true
        } else {
            // The current word is everything from the start of the replacement to the
            // cursor
            let current_word = &editor_text[replacement_start..cursor_position.as_usize()];

            if self.is_classic_completions_enabled(ctx) {
                let current_selected_item =
                    self.input_suggestions.as_ref(ctx).get_selected_item_text();
                if current_selected_item.is_some_and(|selected| selected == current_word) {
                    // If we're in classic completion mode and the selected item is equal
                    // to the current word, then we should keep the menu open; the user is cycling.
                    // We early-return because we don't want to filter the menu based on the
                    // selected item.
                    return false;
                }
            }

            // If the user continues to type with the tab suggestions open, we perform a
            // prefix search on the original results to filter the suggestions.
            let should_close = self.input_suggestions.update(ctx, |suggestions, ctx| {
                suggestions.prefix_search_for_tab_completion(
                    current_word,
                    completion_results,
                    TabCompletionsPreselectOption::Unchanged,
                    ctx,
                );

                // We should close the menu if there aren't any results
                // after filtering.
                suggestions.items().is_empty()
            });

            ctx.notify();
            should_close
        }
    }

    fn clear_screen(&mut self, ctx: &mut ViewContext<Self>) {
        self.model.lock().clear_visible_screen();
        ctx.notify();
    }

    /// Attempts to write the EOT (End-of-Transmission) char to the PTY, which is canonically mapped
    /// to Ctrl-D. If successful, the session is terminated.
    fn ctrl_d(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::CtrlD);
    }

    fn ctrl_r(&mut self, ctx: &mut ViewContext<Self>) {
        if self.suggestions_mode_model.as_ref(ctx).is_history_up() {
            // Iterate through menu if we're already in history substring mode and
            // the user hits ctrl-r.
            self.input_suggestions
                .update(ctx, |input_suggestions, ctx| {
                    input_suggestions.select_prev(ctx);
                });
        } else if !self.ai_input_model.as_ref(ctx).is_ai_input_enabled() {
            self.fuzzy_history_search(ctx);
        }
    }

    fn fuzzy_history_search(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.can_query_history(ctx) {
            return;
        }

        self.focus_input_box(ctx);

        let editor = self.editor.as_ref(ctx);

        let original_cursor_point = editor.single_cursor_to_point(ctx);

        // Although we don't use suggestions_mode_model when using Voltron,
        // we still close the input suggestion menu before opening the Voltron modal,
        // which involves resetting the cursor point.
        let original_buffer = editor.buffer_text(ctx);
        let original_input_type = self.ai_input_model.as_ref(ctx).input_type();
        let original_input_was_locked = self.ai_input_model.as_ref(ctx).is_input_type_locked();
        self.suggestions_mode_model.update(ctx, |m, ctx| {
            m.set_mode(
                InputSuggestionsMode::HistoryUp {
                    original_buffer,
                    original_cursor_point,
                    search_mode: HistorySearchMode::Fuzzy,
                    original_input_type,
                    original_input_was_locked,
                },
                ctx,
            );
        });

        self.select_and_refresh_voltron(VoltronItem::History, ctx);

        ctx.notify();
    }

    pub fn on_session_share_joined(
        &mut self,
        replica_id: ReplicaId,
        presence_manager: ModelHandle<PresenceManager>,
        ctx: &mut ViewContext<Self>,
    ) {
        // Shared session history model should only be set if we are a viewer
        debug_assert!(self.model.lock().shared_session_status().is_viewer());
        self.set_shared_session_presence_manager(presence_manager);

        // Set the history model which is only available for a shared session viewer.
        let history_model = ctx.add_model(|_| SharedSessionHistoryModel::new());
        self.shared_session_input_state = Some(SharedSessionInputState {
            history_model,
            pending_command_execution_request: None,
        });

        self.editor().update(ctx, |editor, ctx| {
            editor.reinitialize_buffer(Some(replica_id), ctx);
        });
    }

    /// Returns a collection of history entries that are shell commands from
    /// the shared session (run on the sharer's machine).
    fn shared_session_history<'b>(
        &'b self,
        ctx: &'b ViewContext<Self>,
    ) -> Vec<HistoryInputSuggestion<'b>> {
        let Some(history_model) = self
            .shared_session_input_state
            .as_ref()
            .map(|state| state.history_model.clone())
        else {
            return Vec::new();
        };

        // TODO: append viewer's local shell history
        history_model
            .as_ref(ctx)
            .entries()
            .map(|entry| HistoryInputSuggestion { entry })
            .collect()
    }

    /// Returns shell commands ordered for up-arrow history.
    fn command_history<'a>(
        &'a self,
        ctx: &'a ViewContext<Self>,
    ) -> Vec<HistoryInputSuggestion<'a>> {
        History::as_ref(ctx).up_arrow_suggestions(self.active_block_session_id(), ctx)
    }

    fn update_last_word_insertion_state(&mut self) {
        // If an `InsertLastWordPrevCommand` action is received, its handler method will set
        // `is_latest_editor_event` on `self.last_word_insertion` to true, marking the following
        // EditorEvent (buffer edited) received is from this insertion.
        //
        // Any other editor event means the following "last word" insert is not consecutive, so
        // index is reset - the following insert will insert last word from most recent command
        // in history, index 0 (After that, a consecutive insertion would increment to index 1,
        // last word of second last command in history).
        //
        // If the last event was a last word insertion, we increment the
        // `insert_command_from_history_index` on `self.last_word_insertion` to indicate
        // consecutive inserts may be made (if so, insert from next earlier command in history).
        // We then set `is_latest_editor_event` to false for the following editor event; if another
        // last word insertion occurs, it is responsible for re-setting this boolean to true.
        if self.last_word_insertion.is_latest_editor_event {
            self.last_word_insertion.insert_command_from_history_index += 1;
            self.last_word_insertion.is_latest_editor_event = false;
        } else {
            self.last_word_insertion.insert_command_from_history_index = 0;
        }
    }

    fn history_commands<'b>(&self, ctx: &'b ViewContext<Input>) -> Vec<&'b HistoryEntry> {
        self.active_block_session_id()
            .map_or_else(Vec::new, |session_id| {
                History::as_ref(ctx)
                    .commands(session_id)
                    .unwrap_or_default()
            })
    }

    fn insert_last_word_previous_command(&mut self, ctx: &mut ViewContext<Input>) {
        if let Some(word_to_insert) = self.get_last_word_of_command_in_history(
            self.last_word_insertion.insert_command_from_history_index,
            ctx,
        ) {
            self.editor.update(ctx, |editor, ctx| {
                editor.insert_selected_text_to_buffer_ignoring_undo(&word_to_insert, ctx);
            });

            self.last_word_insertion.is_latest_editor_event = true;
        }
    }

    fn get_last_word_of_command_in_history(
        &mut self,
        command_history_index: usize,
        ctx: &mut ViewContext<Input>,
    ) -> Option<String> {
        let commands = self.history_commands(ctx);
        if commands.is_empty() {
            return None;
        }

        let view_command_idx = commands.len().saturating_sub(1 + command_history_index);
        let view_command = commands[view_command_idx];

        let last_word = view_command
            .command
            .rsplit_once(' ')
            .map(|(_, last_word)| last_word)
            .unwrap_or(&view_command.command);

        Some(last_word.to_string())
    }

    /// We only want to show the completions while typing menu when the cursor is
    /// positioned at the end of the buffer text
    fn is_cursor_in_valid_position_for_completions_while_typing(
        &self,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let editor = self.editor.as_ref(ctx);
        editor.single_cursor_at_buffer_end(false /* respect_line_cap */, ctx)
    }

    fn should_show_completions_while_typing(&self, ctx: &mut ViewContext<Self>) -> bool {
        let editor = self.editor.as_ref(ctx);
        let buffer_text = editor.buffer_text(ctx);

        self.is_completions_while_typing_turned_on(ctx)
            && (!self.ai_input_model.as_ref(ctx).is_ai_input_enabled()
                || should_show_completions_in_ai_input(&buffer_text))
            && buffer_text.len() >= MIN_BUFFER_LEN_TO_SHOW_COMPLETIONS_WHILE_TYPING
            && self.is_cursor_in_valid_position_for_completions_while_typing(ctx)
    }

    fn is_completions_while_typing_turned_on(&self, app: &AppContext) -> bool {
        *InputSettings::as_ref(app)
            .completions_open_while_typing
            .value()
    }

    fn is_classic_completions_enabled(&self, ctx: &AppContext) -> bool {
        (FeatureFlag::ClassicCompletions.is_enabled()
            && *InputSettings::as_ref(ctx).classic_completions_mode)
            || FeatureFlag::ForceClassicCompletions.is_enabled()
    }

    fn should_expand_aliases(&self, ctx: &mut ViewContext<Self>) -> bool {
        // Never expand aliases when in AI input mode, regardless of the setting.
        if self.ai_input_model.as_ref(ctx).input_type().is_ai() {
            return false;
        }
        *AliasExpansionSettings::as_ref(ctx)
            .alias_expansion_enabled
            .value()
    }

    fn open_completion_suggestions(
        &mut self,
        completions_trigger: CompletionsTrigger,
        ctx: &mut ViewContext<Self>,
    ) {
        let editor = self.editor.as_ref(ctx);
        let buffer_text = editor.buffer_text(ctx);

        let is_command_grid_active = {
            let model = self.model.lock();
            !model.is_alt_screen_active()
                && model.block_list().active_block().is_command_grid_active()
        };

        // If the cursor is in a valid completion position, go into CompletionSuggestions mode
        if is_command_grid_active && self.can_query_history(ctx) {
            let matcher = MatchStrategy::Fuzzy;

            if let Some(completion_context) = self.completion_session_context(ctx) {
                let cursor_position = self.start_byte_index_of_last_selection(ctx);
                let before_cursor_text = buffer_text[..cursor_position.as_usize()].to_owned();
                let editor_model = self.editor.read(ctx, |view, ctx| view.snapshot_model(ctx));

                self.run_completions_async(
                    before_cursor_text,
                    matcher,
                    completions_trigger,
                    editor_model,
                    cursor_position,
                    completion_context,
                    ctx,
                );
            }
        }
    }

    /// _Asynchronously_ generates completions by calling into the completer.
    #[allow(clippy::too_many_arguments)]
    fn run_completions_async(
        &mut self,
        before_cursor_text: String,
        matcher: MatchStrategy,
        completions_trigger: CompletionsTrigger,
        editor_snapshot: EditorSnapshot,
        cursor_position: ByteOffset,
        completion_context: SessionContext,
        ctx: &mut ViewContext<'_, Input>,
    ) {
        let buffer_text = self.buffer_text(ctx);

        // The 'ForceNativeShellCompletions' user pref can be used to unconditionally
        // generate and show native shell completion results (i.e. regardless of whether or
        // not we have completion results via completion specs).
        let force_native_shell_completions = ctx
            .private_user_preferences()
            .read_value("ForceNativeShellCompletions")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(false);

        let use_native_shell_completions = (FeatureFlag::NativeShellCompletions.is_enabled() || force_native_shell_completions)
            && completion_context
                .session
                .shell()
                .supports_native_shell_completions()
            // For now, don't use native shell completions for multi-line commands.
            && !buffer_text.contains('\n');

        let fallback_strategy = match completions_trigger {
            CompletionsTrigger::Keybinding if !use_native_shell_completions => {
                CompletionsFallbackStrategy::FilePaths
            }
            _ => CompletionsFallbackStrategy::None,
        };

        if self.is_completions_while_typing_turned_on(ctx)
            && let Some(last_abort_handle) = self.completions_abort_handle.take()
        {
            last_abort_handle.abort();
        }

        let input_type = self.ai_input_model.as_ref(ctx).input_type();

        // Don't trigger completions if the last character typed is whitespace, in AI input mode.
        // The user is likely typing in a natural language word at this point, not a filepath.
        if input_type.is_ai()
            && completions_trigger == CompletionsTrigger::AsYouType
            && before_cursor_text.ends_with(char::is_whitespace)
        {
            return;
        }

        let Some(session_id) = self.completer_data().active_block_session_id() else {
            return;
        };
        let session_env_vars = self.sessions.read(ctx, |sessions, _| {
            sessions.get_env_vars_for_session(session_id)
        });

        let cursor_position = cursor_position.as_usize();
        let native_results_fut = if use_native_shell_completions {
            // If we're using native shell completions, construct a future that
            // will be resolved with any completions data provided by the shell.
            let (results_tx, results_rx) = async_channel::unbounded();
            ctx.dispatch_typed_action(&TerminalAction::RunNativeShellCompletions {
                buffer_text: buffer_text[0..cursor_position].to_owned(),
                results_tx,
            });
            async move { results_rx.recv().await.ok() }.boxed()
        } else {
            // If not, we can immediately say that there are no completion
            // results from the shell.
            futures::future::ready(None).boxed()
        };

        let completion_session = completion_context.session.clone();

        let abort_handle = ctx
            .spawn_abortable(
                async move {
                    let suggestions = completer::suggestions(
                        before_cursor_text.as_str(),
                        cursor_position,
                        session_env_vars.as_ref(),
                        CompleterOptions {
                            match_strategy: matcher,
                            fallback_strategy,
                            suggest_file_path_completions_only: input_type.is_ai(),
                            parse_quotes_as_literals: input_type.is_ai(),
                        },
                        &completion_context,
                    )
                    .await;

                    let suggestions = match suggestions {
                        Some(s) if !s.suggestions.is_empty() && !force_native_shell_completions => {
                            Some(s)
                        }
                        _ => native_results_fut.await.map(|results| {
                            let suggestions = results.into_iter().map(Into::into).collect_vec();

                            let token_end = cursor_position;
                            // Within the section of the buffer from the start
                            // to the end of this token...
                            let token_start = buffer_text[0..token_end]
                                // Find the last whitespace char before the token end.
                                .rfind(char::is_whitespace)
                                // If we find one, the token start is the next char.
                                .map(|pos| pos + 1)
                                // Otherwise, the start is the beginning of the buffer.
                                .unwrap_or_default();

                            SuggestionResults {
                                replacement_span: (token_start, token_end).into(),
                                suggestions,
                                match_strategy: MatchStrategy::Fuzzy,
                            }
                        }),
                    };

                    (suggestions, completions_trigger, editor_snapshot)
                },
                |input, (suggestions, completions_trigger, editor_model), ctx| {
                    input.handle_completion_suggestions_results(
                        suggestions,
                        completions_trigger,
                        editor_model,
                        ctx,
                    )
                },
                move |_, _| {
                    completion_session.cancel_active_commands();
                },
            )
            .abort_handle();

        self.completions_abort_handle = Some(abort_handle);
    }

    /// Asynchronously generates dynamic enum suggestions.
    fn get_enum_suggestions_async(
        &mut self,
        command: String,
        editor_snapshot: EditorSnapshot,
        ctx: &mut ViewContext<'_, Input>,
    ) {
        if let Some(completion_context) = self.completion_session_context(ctx) {
            self.suggestions_mode_model.update(ctx, |m, ctx| {
                m.set_dynamic_enum_status(DynamicEnumSuggestionStatus::Pending, ctx);
            });
            let abort_handle = ctx
                .spawn(
                    async move {
                        let variants = super::dynamic_enum_suggestions::run_dynamic_enum_command(
                            command.as_str(),
                            &completion_context,
                        )
                        .await;

                        (variants, editor_snapshot)
                    },
                    move |input, (variants, editor_model), ctx| {
                        input.handle_enum_completion_results(variants, editor_model, ctx);
                    },
                )
                .abort_handle();

            self.completions_abort_handle = Some(abort_handle);
            ctx.notify();
        }
    }

    /// When the command finishes running, update the input suggestions menu with the suggestions.
    fn handle_enum_completion_results(
        &mut self,
        results: anyhow::Result<Vec<String>>,
        editor_snapshot_when_completer_was_ran: EditorSnapshot,
        ctx: &mut ViewContext<Self>,
    ) {
        let current_editor_model = self
            .editor
            .read(ctx, |editor, ctx| editor.snapshot_model(ctx));

        let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);
        // If the editor has changed since the completions trigger was hit-- noop since the
        // suggestions are no longer valid. Note that we purposely ignore attributes such as text
        // styles for the purposes of this check (we only care about the buffer text content and
        // the cursor selections state).
        if buffer_text != editor_snapshot_when_completer_was_ran.text()
            || current_editor_model.selections()
                != editor_snapshot_when_completer_was_ran.selections()
        {
            return;
        }

        let (variants, status) = match results {
            Ok(variants) => (variants, DynamicEnumSuggestionStatus::Success),
            Err(e) => {
                log::warn!("Failed to generate dynamic enum suggestions: {e:?}");
                (vec![], DynamicEnumSuggestionStatus::Failure)
            }
        };

        self.input_suggestions.update(ctx, |input, ctx| {
            input.set_enum_variants(variants.clone(), ctx);
        });

        if let InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
            menu_position,
            selected_ranges,
            cursor_point,
            command,
            ..
        } = self.suggestions_mode_model.as_ref(ctx).mode()
        {
            let updated_mode = InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
                dynamic_enum_status: status,
                suggestions: variants,
                menu_position: *menu_position,
                selected_ranges: selected_ranges.clone(),
                cursor_point: *cursor_point,
                command: command.clone(),
            };
            self.suggestions_mode_model.update(ctx, |model, ctx| {
                model.set_mode(updated_mode, ctx);
            });
        }

        ctx.notify();
    }

    fn path_separators(&self, ctx: &AppContext) -> PathSeparators {
        self.active_session(ctx)
            .map(|session| session.path_separators())
            .unwrap_or(PathSeparators::for_os())
    }

    /// Returns the buffer point that the tab completion menu should be positioned relative to.
    /// If None, the menu should be positioned relative to the cursor.
    ///
    /// In regular completions mode, we want to dock the completions menu at the cursor.
    ///
    /// In classic completions mode, we want to dock the completions menu at the start of
    /// the replacement span*. This ensures that the menu doesn't jump around as the cursor
    /// moves when the user cycles through items in the menu.
    /// * The one edge case is when we're completing a file path. In this case, the menu
    ///   should be docked at the end of the last directory in the replacement span.
    ///   This is because the replacement span will include the entire file path.
    ///   For example, if the user types "cd app/D" and one of the completion display result is
    ///   "Documents", then the replacement span will be for "app/D" and the replacement will
    ///   be "app/Documents".
    fn tab_completions_menu_position(
        &self,
        results: &SuggestionResults,
        buffer_text_original: &str,
        ctx: &AppContext,
    ) -> Option<BufferPoint> {
        // In regular mode, the menu should be positioned at the cursor.
        if !self.is_classic_completions_enabled(ctx) {
            return None;
        }

        // Note: the replacement span is in terms of byte offsets.
        // But these byte offsets should correspond to valid char offsets.
        let start = results.replacement_span.start();
        let end = results.replacement_span.end();

        let all_results_are_file_completions = results
            .suggestions
            .iter()
            .all(|s| s.suggestion.file_type.is_some());

        let offset = if all_results_are_file_completions {
            // If all the results are file completions, let's find the last slash in the replacement
            // span and dock the completions menu right after it. We do this because the replacement
            // span of file path completions is relative to the beginning of the file path. For
            // example, if the user types "cd app/D" and one of the completion display result is
            // "Documents", then the replacement span will be for "app/D" and the replacement will
            // be "app/Documents".
            buffer_text_original
                .get(0..end)
                .and_then(|s| s.rfind(self.path_separators(ctx).all))
                .map(|i| i + 1)
                .unwrap_or(start)
        } else {
            start
        };

        let point = self
            .editor
            .as_ref(ctx)
            .point_for_offset(ByteOffset::from(offset), ctx);
        point.ok()
    }

    fn handle_completion_suggestions_results(
        &mut self,
        results: Option<SuggestionResults>,
        completions_trigger: CompletionsTrigger,
        editor_snapshot_when_completer_was_ran: EditorSnapshot,
        ctx: &mut ViewContext<Self>,
    ) {
        let current_editor_model = self
            .editor
            .read(ctx, |editor, ctx| editor.snapshot_model(ctx));

        let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);
        // If the editor has changed since the completions trigger was hit-- noop since the
        // suggestions are no longer valid. Note that we purposely ignore attributes such as text
        // styles for the purposes of this check (we only care about the buffer text content and
        // the cursor selections state).
        if buffer_text != editor_snapshot_when_completer_was_ran.text()
            || current_editor_model.selections()
                != editor_snapshot_when_completer_was_ran.selections()
        {
            return;
        }

        match results {
            None => {
                // It's necessary to specifically set to closed in the case where we first
                // opened the tab menu and then keep typing
                self.suggestions_mode_model.update(ctx, |m, ctx| {
                    m.set_mode(InputSuggestionsMode::Closed, ctx);
                });
            }
            Some(results) if results.suggestions.is_empty() => {
                self.suggestions_mode_model.update(ctx, |m, ctx| {
                    m.set_mode(InputSuggestionsMode::Closed, ctx);
                });
            }
            Some(results) => {
                let query = results.replacement_span.slice(&buffer_text);
                let buffer_text_original = buffer_text
                    [0..self.start_byte_index_of_last_selection(ctx).as_usize()]
                    .to_string();
                let decision = if completions_trigger == CompletionsTrigger::Keybinding {
                    results.explicit_tab_completion(query, self.path_separators(ctx).all)
                } else {
                    ExplicitTabCompletion::Open {
                        suggestions: results
                            .prepare_for_query(query, self.path_separators(ctx).all),
                        replacement_span: results.replacement_span,
                    }
                };
                let prepared_suggestions: Vec<PreparedSuggestion> = match decision {
                    ExplicitTabCompletion::NoAction => {
                        self.suggestions_mode_model.update(ctx, |model, ctx| {
                            model.set_mode(InputSuggestionsMode::Closed, ctx);
                        });
                        ctx.notify();
                        return;
                    }
                    ExplicitTabCompletion::InsertSingle {
                        suggestion,
                        replacement_span,
                    } => {
                        self.insert_completion_result_into_editor(
                            &suggestion.suggestion.replacement,
                            replacement_span.start(),
                            Executing::No,
                            ctx,
                        );
                        ctx.notify();
                        return;
                    }
                    ExplicitTabCompletion::InsertCommonPrefixAndOpen {
                        common_prefix,
                        suggestions,
                        replacement_span,
                    } => {
                        self.insert_completion_prefix_into_editor(
                            ctx,
                            &common_prefix,
                            replacement_span.start(),
                        );
                        suggestions
                    }
                    ExplicitTabCompletion::Open { suggestions, .. } => suggestions,
                };

                // If not using completions as you type, then
                // clear any autosuggestions when tab completions are open.
                // The autosuggestion will be repopulated when the menu is closed.
                // We don't do this for completions as you type because the user would
                // otherwise hardly see autosuggestons.
                if FeatureFlag::RemoveAutosuggestionDuringTabCompletions.is_enabled()
                    && !self.is_completions_while_typing_turned_on(ctx)
                {
                    self.editor.update(ctx, |view, ctx| {
                        view.clear_autosuggestion(ctx);
                    });
                }

                // Decide where to render the tab completion menu.
                // If we're rendering it at a specific position, let's make sure
                // that position exists in the position cache.
                let position =
                    self.tab_completions_menu_position(&results, &buffer_text_original, ctx);
                let menu_position = if let Some(position) = position {
                    self.editor.update(ctx, |editor, ctx| {
                        editor.cache_buffer_point(
                            position,
                            COMPLETIONS_START_OF_REPLACEMENT_SPAN_POSITION_ID,
                            ctx,
                        );
                    });
                    TabCompletionsMenuPosition::AtStartOfReplacementSpan
                } else {
                    TabCompletionsMenuPosition::AtLastCursor
                };

                self.suggestions_mode_model.update(ctx, |model, ctx| {
                    model.set_mode(
                        InputSuggestionsMode::CompletionSuggestions {
                            replacement_start: results.replacement_span.start(),
                            buffer_text_original,
                            completion_results: results.clone(),
                            trigger: completions_trigger,
                            menu_position,
                        },
                        ctx,
                    );
                });

                let preselect_option = if self.is_classic_completions_enabled(ctx) {
                    TabCompletionsPreselectOption::Unselected
                } else {
                    TabCompletionsPreselectOption::First
                };

                self.input_suggestions
                    .update(ctx, |input_suggestions, ctx| {
                        input_suggestions.set_prepared_tab_completions(
                            prepared_suggestions,
                            preselect_option,
                            ctx,
                        );
                    });
            }
        }
        ctx.notify();
    }

    /// Replace the replacement with the common completion prefix. Note that completion prefix
    /// itself is not the completion result so we don't add a space.
    fn insert_completion_prefix_into_editor(
        &mut self,
        ctx: &mut ViewContext<Input>,
        completion_prefix: &str,
        replacement_start: usize,
    ) {
        self.editor.update(ctx, |input, ctx| {
            let cursor_end_offset = input.end_byte_index_of_last_selection(ctx);
            input.select_and_replace(
                completion_prefix,
                [ByteOffset::from(replacement_start)..cursor_end_offset],
                PlainTextEditorViewAction::AcceptCompletionSuggestion,
                ctx,
            );
        });
    }

    /// Replace the replacement with the completion result and potentially add a space after.
    fn insert_completion_result_into_editor(
        &mut self,
        completion_result: &str,
        replacement_start: usize,
        executing: Executing,
        ctx: &mut ViewContext<Input>,
    ) {
        let is_completions_as_you_type_enabled = self.is_completions_while_typing_turned_on(ctx);
        self.editor.update(ctx, |input, ctx| {
            let cursor_end_offset = input.end_byte_index_of_last_selection(ctx);

            // Add a space to the end if the end of the selection/replacement
            // is at the end of the buffer and the completion result doesn't end with a slash.
            // If completions as you type is turned on and classic completions is off, then
            // _don't_ add a space.
            let is_classic_completions_enabled = self.is_classic_completions_enabled(ctx);
            let replacement: Cow<str> = if (!is_completions_as_you_type_enabled
                || is_classic_completions_enabled)
                && cursor_end_offset.as_usize() == input.buffer_text(ctx).len()
                && !completion_result.ends_with(self.path_separators(ctx).main)
                && executing == Executing::No
            {
                format!("{completion_result} ").into()
            } else {
                completion_result.into()
            };

            input.select_and_replace(
                &replacement,
                [ByteOffset::from(replacement_start)..cursor_end_offset],
                PlainTextEditorViewAction::AcceptCompletionSuggestion,
                ctx,
            );
        });
    }

    /// Whether the editor is in a state where we should tab complete instead of indenting text
    /// within the editor.
    /// The editor is considered in a state where we should tab complete if:
    ///     1) The buffer text is not empty.
    ///     2) The user is not actively selecting.
    ///     3) There is only a single selection and that selection does not take up the entire
    ///        buffer.
    fn cursor_positioned_for_completion(&self, ctx: &mut ViewContext<Self>) -> bool {
        let input = self.editor.as_ref(ctx);
        let buffer_text = input.buffer_text(ctx);

        // We can show the completion menu when there is a single cursor selection
        // and we aren't actively selecting.
        !buffer_text.trim_start().is_empty()
            && !input.is_selecting(ctx)
            && input.num_selections(ctx) == 1
            && !input.any_selections_span_entire_buffer(ctx)
    }

    /// Returns the index of the argument our cursor is currently on, if there is one,
    /// as well as any style runs computed for reuse in `highlight_selected_workflow_argument`
    fn get_current_argument(
        &self,
        ctx: &ViewContext<Self>,
    ) -> (Option<WorkflowArgumentIndex>, Vec<Range<ByteOffset>>) {
        // If we aren't in a workflow, return
        let Some(workflow_state) = &self.workflows_state.selected_workflow_state else {
            report_error!(anyhow::anyhow!(
                "Tried to get the current argument when no workflow is loaded into the input",
            ));
            return (None, Vec::new());
        };

        let cursor_position = self
            .editor
            .as_ref(ctx)
            .end_byte_index_of_last_selection(ctx);

        // Get the highlighted text style ranges, which are used to determine where the workflow arguments are
        let text_style_ranges = self.get_text_style_ranges_for_workflow(ctx);

        // Find a text range that contains the cursor position
        let highlight_index = text_style_ranges
            .iter()
            .position(|range| range.contains(&cursor_position));

        // Find the argument that corresponds with this highlight index
        let arg_index = highlight_index.and_then(|index| {
            workflow_state
                .argument_index_to_highlight_index
                .iter()
                .find(|(_, highlight)| highlight.contains(&index))
                .map(|(arg_index, _)| *arg_index)
        });

        (arg_index, text_style_ranges)
    }

    fn input_shift_tab(&mut self, ctx: &mut ViewContext<Self>) {
        match self.suggestions_mode_model.as_ref(ctx).mode() {
            // If the model selector is open and has multiple tabs,

            // If we're in CompletionSuggestions mode, shift tab moves to the previous selection.
            InputSuggestionsMode::CompletionSuggestions { .. } => {
                self.input_suggestions.update(ctx, |suggestions, ctx| {
                    suggestions.select_prev(ctx);
                });
                return;
            }
            _ => {}
        }

        if let Some(workflows_info_view) = &self
            .workflows_state
            .selected_workflow_state
            .as_ref()
            .map(|state| &state.more_info_view)
        {
            // Get the index of the argument we are currently selecting, if it exists
            let (current_argument, text_style_ranges) = self.get_current_argument(ctx);

            workflows_info_view.update(ctx, |info_view, ctx| {
                // If we are selecting an argument, open that one
                if let Some(index) = current_argument {
                    info_view.selected_workflow_state.set_argument_index(index);
                }
                // If we were in history suggestion mode, select the first argument
                else if matches!(
                    self.suggestions_mode_model.as_ref(ctx).mode(),
                    InputSuggestionsMode::HistoryUp { .. }
                ) {
                    info_view
                        .selected_workflow_state
                        .set_argument_index(0.into());
                }
                // Otherwise, continue to cycle arguments
                else {
                    info_view.selected_workflow_state.increment_argument_index();
                }

                ctx.notify();
            });

            self.highlight_selected_workflow_argument(text_style_ranges, ctx);

            if let Some(a11y_text) = self.selected_workflow_a11y_text(ctx) {
                ctx.emit_a11y_content(AccessibilityContent::new_without_help(
                    a11y_text,
                    WarpA11yRole::UserAction,
                ));
            }
        } else {
            self.editor.update(ctx, |input, ctx| input.unindent(ctx));
        }
    }

    pub fn completion_session_context(&self, ctx: &AppContext) -> Option<SessionContext> {
        self.active_block_session_id()
            .and_then(|active_block_session_id| {
                let current_session = self.sessions.as_ref(ctx).get(active_block_session_id);
                let pwd = self
                    .active_block_metadata
                    .as_ref()
                    .and_then(BlockMetadata::current_working_directory)
                    .map(str::to_owned);

                current_session.zip(pwd).map(|(current_session, pwd)| {
                    // TODO(abhishek): Ideally, BlockMetadata::current_working_directory should directly
                    // return a TypedPathBuf. This shouldn't happen here in the view.
                    let current_working_directory =
                        current_session.convert_directory_to_typed_path_buf(pwd);
                    SessionContext::new(
                        current_session,
                        CommandRegistry::global_instance(),
                        current_working_directory,
                        ctx,
                    )
                })
            })
    }

    pub fn active_session(&self, ctx: &AppContext) -> Option<Arc<Session>> {
        self.active_block_session_id()
            .and_then(|active_block_session_id| {
                self.sessions.as_ref(ctx).get(active_block_session_id)
            })
    }

    fn hide_x_ray(&mut self, ctx: &mut ViewContext<Self>) {
        if self.command_x_ray_description.take().is_some() {
            self.editor.update(ctx, |editor, ctx| {
                editor.clear_command_x_ray();
                ctx.notify();
            });
            ctx.notify();
        }
    }

    fn start_xray_at_offset(
        &mut self,
        pos: ByteOffset,
        trigger: CommandXRayTrigger,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(completion_context) = self.completion_session_context(ctx) {
            let buffer_text = self.buffer_text(ctx);
            let _ =
                ctx.spawn(
                    async move {
                        completer::describe(buffer_text.as_str(), pos, &completion_context).await
                    },
                    |input, description, ctx| {
                        input.show_xray(description, trigger, ctx);
                    },
                );
        }
    }

    fn show_xray(
        &mut self,
        description: Option<Description>,
        trigger: CommandXRayTrigger,
        ctx: &mut ViewContext<'_, Self>,
    ) {
        let description = description.map(Arc::new);
        self.command_x_ray_description.clone_from(&description);
        if let Some(description) = description {
            if trigger == CommandXRayTrigger::Keystroke {
                ctx.emit_a11y_content(AccessibilityContent::new_without_help(
                    description.a11y_text(),
                    WarpA11yRole::UserAction,
                ));
            }
            ctx.notify();
            self.editor.update(ctx, move |editor, ctx| {
                editor.set_command_x_ray(description);
                ctx.notify();
            });
        }
        ctx.notify();
    }

    fn active_block_session_id(&self) -> Option<SessionId> {
        self.active_block_metadata
            .as_ref()
            .and_then(BlockMetadata::session_id)
    }

    /// Handles a tab keypress from the editor.
    ///
    /// "Tab" is the default trigger to open the completion suggestions menu, but this may be
    /// overridden in settings. If the completion suggestions menu is already open, tab and
    /// shift-tab are used to select the next and previous suggestion, respectively -- this is not
    /// overridable; note that even if "open completion suggestions menu" is rebound to a non-tab
    /// key, tab and shift-tab are still used to navigate within the menu once it is open.
    ///
    /// If tab is not bound to "open completion suggestions menu" nor is the suggestions menu
    /// already open, inserts a tab char into the input editor.
    fn input_tab(&mut self, ctx: &mut ViewContext<Self>) {
        // We have to manually check if "tab" is bound to
        // `InputAction::MaybeOpenCompletionSuggestions` here because the child `EditorView`
        // handles the actual tab keypress event -- the handler method attached to the
        // `EditableBinding` for `MaybeOpenCompletionSuggestions` is not called when the
        // binding is tab because the UI framework dictates that only one View may receive a
        // keypress event.
        let is_tab_bound_to_open_completions =
            bindings::keybinding_name_to_keystroke(OPEN_COMPLETIONS_KEYBINDING_NAME, ctx)
                .map(|keystroke| keystroke.key == "tab")
                .unwrap_or_default();

        let replacement_start_opt = if let InputSuggestionsMode::CompletionSuggestions {
            replacement_start,
            ..
        } = self.suggestions_mode_model.as_ref(ctx).mode()
        {
            Some(*replacement_start)
        } else {
            None
        };
        if let Some(replacement_start) = replacement_start_opt {
            // The completions menu is already open, in which there are two cases.
            // Case 1: There is a common prefix amongst filtered suggestions that we could fill; so
            //         we fill it in buffer.
            // Case 2: Else, tab should move to next option.
            let (common_prefix_of_filtered_suggestions, is_single_prefix_suggestion) =
                self.input_suggestions.read(ctx, |suggestions, _| {
                    // Ignore fuzzy matches when calculating longest common
                    // prefix of suggestions. So even if there are fuzzy
                    // matches, we can find a common prefix and try to insert it.
                    let suggestion_texts = suggestions
                        .items()
                        .iter()
                        .filter(|item| {
                            matches!(
                                item.match_type(),
                                MatchType::Prefix {
                                    is_case_sensitive: true
                                } | MatchType::Exact {
                                    is_case_sensitive: true
                                }
                            )
                        })
                        .map(|item| item.text())
                        .collect_vec();
                    let num_suggestions = suggestion_texts.len();
                    (
                        longest_common_prefix(suggestion_texts).map(|x| x.to_owned()),
                        num_suggestions == 1,
                    )
                });
            if let Some(common_prefix) = common_prefix_of_filtered_suggestions {
                let input_text = self.editor.as_ref(ctx).buffer_text(ctx);
                // Determine the current word in the editor that will be replaced by the tab
                // completion. We use the start index of the selection since the completer only sees
                // the text up to the start of the selection when generating completion results.
                let current_word = &input_text
                    [replacement_start..self.start_byte_index_of_last_selection(ctx).as_usize()];

                // Insert the common prefix if it is longer than what the user has currently typed
                // This check is necessary because the suggestions are case-insensitive, while the
                // common prefix logic is necessarily case-sensitive. That can lead to the common
                // prefix being shorter, causing confusing behavior where the input is shortened.
                // Also, we check if the replacement
                if common_prefix.len() > current_word.len()
                    && common_prefix.starts_with(current_word)
                {
                    self.insert_completion_prefix_into_editor(
                        ctx,
                        &common_prefix,
                        replacement_start,
                    );
                    // If there was only a single completion remaining and we just inserted it into the editor,
                    // close the completions menu.
                    if is_single_prefix_suggestion {
                        self.close_input_suggestions(true, ctx)
                    }
                    return;
                }
            }
            self.input_suggestions.update(ctx, |suggestions, ctx| {
                suggestions.select_next(ctx);
            });
        } else if matches!(
            self.suggestions_mode_model.as_ref(ctx).mode(),
            InputSuggestionsMode::StaticWorkflowEnumSuggestions { .. }
                | InputSuggestionsMode::DynamicWorkflowEnumSuggestions { .. }
        ) {
            self.input_suggestions.update(ctx, |suggestions, ctx| {
                suggestions.select_next(ctx);
            });
        } else if is_tab_bound_to_open_completions && self.cursor_positioned_for_completion(ctx) {
            self.open_completion_suggestions(CompletionsTrigger::Keybinding, ctx);
        } else {
            // Otherwise, pass the tab down to the editor
            self.editor.update(ctx, |input, ctx| input.handle_tab(ctx));
        }
    }

    /// Opens the completion suggestions menu if the cursor is in a valid position to generate
    /// suggestions and the menu is not already open.
    ///
    /// This is called when [`InputAction::MaybeOpenCompletionSuggestions`] is bound to a non-tab
    /// key; tab is the default binding. This is _not_ called when the binding is set to the
    /// default ("tab") because the tab keypress event is actually handled by the child
    /// [`Editor`] view, so the tab event is never actually propagated to this input view. Instead,
    /// the logic to open the completions menu when tab bound is implemented in
    /// [`Self::input_tab()`], which is called when the editor emits an
    /// `EditorEvent::Navigate(NavigationKey::Tab)`.
    ///
    /// Ultimately this weirdness is due to limitations in the UI framework preventing multiple
    /// `View`s from handling/responding to the same `Event`.
    fn maybe_open_completion_suggestions(&mut self, ctx: &mut ViewContext<Self>) {
        if !matches!(
            self.suggestions_mode_model.as_ref(ctx).mode(),
            InputSuggestionsMode::CompletionSuggestions { .. },
        ) && self.cursor_positioned_for_completion(ctx)
        {
            self.open_completion_suggestions(CompletionsTrigger::Keybinding, ctx);
        }
    }

    #[cfg(test)]
    fn user_insert(&mut self, text: &str, ctx: &mut ViewContext<Self>) -> bool {
        self.insert_internal(text, EditOrigin::UserTyped, ctx)
    }

    pub fn user_replace_editor_text(&mut self, text: &str, ctx: &mut ViewContext<Self>) -> bool {
        self.editor.update(ctx, |editor, ctx| {
            editor.select_all(ctx);
        });
        self.insert_internal(text, EditOrigin::UserTyped, ctx)
    }

    // It's the responsibility of the caller to ensure that the text submitted here
    // should be inputted into the input area (i.e. arrow keys should not be
    // included in the string).
    pub fn system_insert(&mut self, text: &str, ctx: &mut ViewContext<Self>) -> bool {
        self.insert_internal(text, EditOrigin::UserInitiated, ctx)
    }

    pub fn has_pending_command(&self) -> bool {
        self.has_pending_command
    }

    pub fn set_pending_command(&mut self, exec: &str, ctx: &mut ViewContext<Self>) {
        self.has_pending_command = true;
        self.system_insert(exec, ctx);
    }

    fn should_enter_accept_completion_suggestion(&self, app: &AppContext) -> bool {
        let InputSuggestionsMode::CompletionSuggestions {
            replacement_start, ..
        } = self.suggestions_mode_model.as_ref(app).mode()
        else {
            return false;
        };
        let completions_while_typing = self.is_completions_while_typing_turned_on(app);
        let selected_item = self.input_suggestions.as_ref(app).get_selected_item_text();

        // If classic completions is enabled, accept the suggestion if an item is selected.
        if self.is_classic_completions_enabled(app) {
            return self
                .input_suggestions
                .as_ref(app)
                .get_selected_item()
                .is_some();
        }
        // If completions as you type is disabled, accept the suggestion if an item is selected.
        if !completions_while_typing {
            return selected_item.is_some();
        }

        let path_separators = self.path_separators(app).all;

        // At this point, we know completions as you type is enabled and classic completions
        // is disabled. Accept the completion unless the buffer already matches the selected item
        // (in which case, just execute the command).
        let current_buffer_text = self.editor.as_ref(app).buffer_text(app);
        selected_item.is_none_or(|selected_item| {
            let Some(replacement) = &current_buffer_text.get(*replacement_start..) else {
                report_error!("Failed to get replacement range in current buffer text");
                return true;
            };
            if replacement == &selected_item {
                return false;
            }
            let Some(no_slash) = selected_item.strip_suffix(path_separators) else {
                return true;
            };
            replacement != &no_slash
        })
    }

    /// Determines whether to insert a newline in the buffer instead of executing a command
    /// when enter is pressed.
    fn should_insert_newline_on_enter(&self, ctx: &AppContext) -> bool {
        let editor = self.editor.as_ref(ctx);
        let shell_family = editor.shell_family();
        editor.chars_preceding_selections(ctx).any(|chars| {
            let mut preceding_chars = chars.rev();
            while let Some(c) = preceding_chars.next() {
                match shell_family {
                    Some(ShellFamily::PowerShell) => {
                        if c == '`' {
                            // Kind of a quirk, but PowerShell only inserts a
                            // newline after a backtick if the character preceding
                            // the backtick is whitespace.
                            if let Some(c) = preceding_chars.next()
                                && !c.is_ascii_whitespace()
                            {
                                return false;
                            }
                            return true;
                        }
                    }
                    Some(ShellFamily::Posix) | None => {
                        if c == '\\' {
                            // Continue if there are more \ characters
                            if let Some(c) = preceding_chars.next()
                                && c == '\\'
                            {
                                continue;
                            }
                            // Odd number of \ characters
                            return true;
                        }
                    }
                }
                return false;
            }
            false
        })
    }

    /// Handles the user's 'Enter' keypress.
    ///
    /// Depending on input state, this method may either execute a command, accept an input
    /// suggestion, or add a newline to the input buffer contents.  If there is an active and long
    /// running command, exits early and does nothing. This method should not be callable if there
    /// is an active and long running command; in such a state, the enter keypress should be
    /// handled by the ongoing process corresponding to the active/long running command.
    pub(crate) fn input_enter(&mut self, ctx: &mut ViewContext<Self>) {
        if self.should_insert_newline_on_enter(ctx) {
            self.editor.update(ctx, |editor, ctx| {
                editor.user_initiated_insert("\n", PlainTextEditorViewAction::NewLine, ctx)
            });
            return;
        }

        if self
            .suggestions_mode_model
            .as_ref(ctx)
            .is_inline_history_menu()
            && self
                .inline_history_menu_view
                .as_ref(ctx)
                .model()
                .as_ref(ctx)
                .selected_item()
                .is_some()
        {
            self.inline_history_menu_view
                .update(ctx, |view, ctx| view.accept_selected_item(ctx));
            return;
        }

        if matches!(
            self.suggestions_mode_model.as_ref(ctx).mode(),
            InputSuggestionsMode::CompletionSuggestions { .. }
        ) && self.should_enter_accept_completion_suggestion(ctx)
        {
            self.input_suggestions.update(ctx, |suggestions, ctx| {
                suggestions.confirm(ctx);
            });
            return;
        }

        if matches!(
            self.suggestions_mode_model.as_ref(ctx).mode(),
            InputSuggestionsMode::StaticWorkflowEnumSuggestions { .. }
                | InputSuggestionsMode::DynamicWorkflowEnumSuggestions { .. }
        ) {
            self.input_suggestions.update(ctx, |suggestions, ctx| {
                suggestions.confirm(ctx);
            });
            return;
        }

        let command = self.get_command(ctx);
        if !self.try_execute_command(&command, ctx) {
            return;
        }
        self.emit_input_buffer_submitted_telemetry(ctx);

        if SyncedInputState::as_ref(ctx).is_syncing_any_inputs(ctx.window_id()) {
            ctx.emit(Event::SyncInput(SyncInputType::RanCommand));
        }

        self.model.lock().set_is_input_dirty(false);
    }

    fn input_cmd_enter(&mut self, ctx: &mut ViewContext<Self>) {
        // NaturalLanguageCommandSearch has its own `cmd+enter` behaviour, not expected to execute here
        let mode = self.suggestions_mode_model.as_ref(ctx).mode().clone();
        match &mode {
            InputSuggestionsMode::CompletionSuggestions { .. }
            | InputSuggestionsMode::HistoryUp { .. } => {
                self.input_suggestions.update(ctx, |suggestions, ctx| {
                    suggestions.confirm_and_execute(ctx);
                });
            }
            InputSuggestionsMode::DynamicWorkflowEnumSuggestions {
                dynamic_enum_status: DynamicEnumSuggestionStatus::Unapproved,
                command,
                ..
            } => {
                let editor_model = self.editor.read(ctx, |view, ctx| view.snapshot_model(ctx));
                self.get_enum_suggestions_async(command.clone(), editor_model, ctx);
            }
            _ => {}
        }
    }

    fn emit_input_buffer_submitted_telemetry(&self, ctx: &mut ViewContext<Self>) {
        let input_model = self.ai_input_model.as_ref(ctx);
        let block_id = self.model.lock().active_block_id().clone();
        send_telemetry_from_ctx!(
            TelemetryEvent::InputBufferSubmitted {
                input_type: input_model.input_type(),
                is_locked: input_model.is_input_type_locked(),
                input_type_decision_source: input_model.last_ai_autodetection_source(),
                was_lock_set_with_empty_buffer: input_model.was_lock_set_with_empty_buffer(),
                block_id,
            },
            ctx
        );
    }

    /// Returns true if toggling the input mode is disabled.
    fn is_input_mode_toggle_disabled(&self) -> bool {
        // Don't allow input mode changes for:
        // - read-only viewers in shared sessions.
        // - long-running commands with an agent tagged in or in control.
        let terminal_model = self.model.lock();
        let active_block = terminal_model.block_list().active_block();
        terminal_model.shared_session_status().is_reader()
            || active_block.is_agent_in_control_or_tagged_in()
    }

    /// Set input mode to natural language detection (auto-detection)
    pub fn set_input_mode_natural_language_detection(&mut self, ctx: &mut ViewContext<Self>) {
        if self.is_input_mode_toggle_disabled() {
            return;
        }

        let is_autodetection_enabled = AISettings::as_ref(ctx).is_ai_autodetection_enabled(ctx);

        if !is_autodetection_enabled {
            return;
        }

        let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);

        self.ai_input_model.update(ctx, |ai_input_model, ctx| {
            // If we're already configured to do autodetection, there's nothing to do here.
            if ai_input_model.should_run_input_autodetection(ctx) {
                return;
            }

            // Update the input mode to remove any locks and re-enable autodetection.
            // If the buffer is empty, this returns the input mode to the default.
            let input_type = if buffer_text.is_empty() {
                InputType::default()
            } else {
                ai_input_model.input_config().input_type
            };
            ai_input_model.enable_autodetection(input_type, ctx);
        });

        // If the buffer is non-empty, we should kick off the autodetection process, in case the
        // classification doesn't match the previous locked mode.
        if !buffer_text.is_empty()
            && let Some(completion_context) = self.completion_session_context(ctx)
        {
            let ai_input_model = self.ai_input_model.clone();

            ctx.spawn(
                async move {
                    (
                        parse_current_commands_and_tokens(buffer_text, &completion_context).await,
                        completion_context,
                    )
                },
                move |_input, (parsed_tokens, completion_context), ctx| {
                    let session_id = completion_context.session.id();
                    ai_input_model.update(ctx, |model, ctx| {
                        model.detect_and_set_input_type(
                            parsed_tokens,
                            completion_context,
                            Some(session_id),
                            ctx,
                        );
                    });
                },
            );
        }
    }

    /// Set input mode to Agent Mode (AI input)
    pub fn set_input_mode_agent(
        &mut self,
        ensure_input_is_focused: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        // Don't allow input mode changes for read-only viewers in shared sessions
        if self.model.lock().shared_session_status().is_reader() {
            return;
        }

        let is_input_buffer_empty = self.editor.as_ref(ctx).buffer_text(ctx).is_empty();

        // When AgentView is enabled, reverting to AI mode in an active agent view with an empty
        // buffer should unlock (re-enable autodetection) - semantically like clearing the "!".
        //
        // If there is a pending image / file attachment, do NOT unlock. The user's intent is
        // unambiguously "talk to the agent"; letting the classifier flip the input back to
        // shell mode would be a bug.
        let has_locking_attachment = self.ai_context_model.as_ref(ctx).has_locking_attachment();
        let should_unlock = FeatureFlag::AgentView.is_enabled()
            && self.agent_view_controller.as_ref(ctx).is_fullscreen()
            && is_input_buffer_empty
            && AISettings::as_ref(ctx).is_ai_autodetection_enabled(ctx)
            && !has_locking_attachment;

        if should_unlock {
            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                ai_input_model.enable_autodetection(InputType::AI, ctx);
            });
        } else {
            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                let new_config = InputConfig {
                    input_type: InputType::AI,
                    is_locked: true,
                };
                let decision_source = if has_locking_attachment {
                    InputTypeAutoDetectionSource::AttachmentForcedAi
                } else {
                    InputTypeAutoDetectionSource::ManualToggle
                };
                ai_input_model.set_input_config(
                    new_config,
                    is_input_buffer_empty,
                    Some(decision_source),
                    ctx,
                );
            });
        }

        if ensure_input_is_focused {
            self.focus_input_box(ctx);
        }
    }

    /// Set input mode to Terminal Mode (shell command input)
    pub fn set_input_mode_terminal(&mut self, steal_focus: bool, ctx: &mut ViewContext<Self>) {
        if self.is_input_mode_toggle_disabled() {
            return;
        }

        let is_input_buffer_empty = self.editor.as_ref(ctx).buffer_text(ctx).is_empty();
        self.ai_input_model.update(ctx, |ai_input_model, ctx| {
            let new_config = InputConfig {
                input_type: InputType::Shell,
                is_locked: true,
            };
            ai_input_model.set_input_config(
                new_config,
                is_input_buffer_empty,
                Some(InputTypeAutoDetectionSource::ManualToggle),
                ctx,
            );
        });

        if steal_focus {
            self.focus_input_box(ctx);
        }
    }

    /// Applies an input config update from an external source (e.g., session sharing).
    pub fn apply_external_input_config_update(
        &mut self,
        config: InputConfig,
        ctx: &mut ViewContext<Self>,
    ) {
        // do nothing if the config is the same as the current config
        if config == self.ai_input_model.as_ref(ctx).input_config() {
            return;
        }

        let is_input_buffer_empty = self.editor.as_ref(ctx).buffer_text(ctx).is_empty();
        self.ai_input_model.update(ctx, |model, ctx| {
            model.set_input_config(
                config,
                is_input_buffer_empty,
                Some(InputTypeAutoDetectionSource::SessionSharingApply),
                ctx,
            );
        });
    }

    /// Returns true if the input is locked in shell mode
    fn is_locked_in_shell_mode(&self, ctx: &ViewContext<Self>) -> bool {
        let ai_input_model = self.ai_input_model.as_ref(ctx);
        ai_input_model.is_input_type_locked() && !ai_input_model.input_type().is_ai()
    }

    /// Exits `!` shell mode by switching back to AI mode, respecting the autodetection setting.
    fn exit_shell_mode_to_ai(&mut self, ctx: &mut ViewContext<Self>) {
        let new_config = InputConfig {
            input_type: InputType::AI,
            is_locked: true,
        }
        .unlocked_if_autodetection_enabled(true, ctx);
        self.ai_input_model.update(ctx, |ai_input_model, ctx| {
            ai_input_model.set_input_config(
                new_config,
                true,
                Some(InputTypeAutoDetectionSource::ShellPrefix),
                ctx,
            );
        });
    }

    fn get_command(&mut self, ctx: &mut ViewContext<Self>) -> String {
        // Expand valid abbreviations or aliases, if any
        if let Some(expanded_command) = self.get_expanded_command_on_execute(ctx) {
            return expanded_command;
        }
        self.editor.as_ref(ctx).buffer_text(ctx)
    }

    /// Inserts the given text into the input buffer. Note that this requires a TerminalModel lock
    /// because when not in Agent Mode, we clear all active selections when inserting text into the
    /// editor! Any upstream caller should NOT be holding a lock on the TerminalModel when calling
    /// this method, to avoid a deadlock.
    fn insert_internal(
        &mut self,
        text: &str,
        edit_origin: EditOrigin,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if matches!(edit_origin, EditOrigin::UserTyped) {
            self.model.lock().set_is_input_dirty(true);
        }
        // If not in Agent Mode, clear any active text selections in the blocklist when inserting
        // new text. Note that the TerminalModel lock is instantly dropped after this expression,
        // since it's stored in a temporary variable.
        //
        // When `FeatureFlag::AgentView` is enabled, blocks are attachable as AI context in terminal
        // mode. Selections are preserved so they can be attached to the query when entering the
        // agent view.
        if !self.ai_input_model.as_ref(ctx).is_ai_input_enabled()
            && !FeatureFlag::AgentView.is_enabled()
        {
            self.model.lock().block_list_mut().clear_selection();
        }

        ctx.focus(&self.editor);
        self.editor.update(ctx, |editor, ctx| match edit_origin {
            EditOrigin::UserTyped => editor.user_insert(text, ctx),
            EditOrigin::UserInitiated => {
                editor.user_initiated_insert(text, PlainTextEditorViewAction::SystemInsert, ctx)
            }
            EditOrigin::SystemEdit => {
                editor.system_insert(text, PlainTextEditorViewAction::SystemInsert, ctx)
            }
            EditOrigin::SyncedTerminalInput | EditOrigin::RemoteEdit => (),
        });
        ctx.notify();
        true
    }

    /// Returns the operations for any edits made to the latest buffer.
    pub fn latest_buffer_operations(&self) -> impl Iterator<Item = &CrdtOperation> {
        self.latest_buffer_operations.iter()
    }

    /// Applies the `operations` if the block ID of this buffer
    /// is equal to `block_id`. Otherwise, queues up these operations
    /// to be processed eventually when the block IDs are equal.
    pub fn process_remote_edits(
        &mut self,
        block_id: &BlockId,
        operations: Vec<CrdtOperation>,
        ctx: &mut ViewContext<Self>,
    ) {
        // We check the `block_id` against the cached latest block ID
        // rather than the latest terminal model state because the terminal
        // model can be updated off of the main thread. This can cause
        // scenarios where the terminal model has a new active block ID but
        // we haven't processed block completed events yet.
        //
        // Although we're checking against a potentially old block ID here,
        // we'll flush the right ops when we handle the block completed events.
        if block_id == &self.deferred_remote_operations.latest_block_id {
            self.editor.update(ctx, |editor, ctx| {
                editor.apply_remote_operations(operations, ctx);
            });
        } else {
            self.deferred_remote_operations
                .defer(block_id.clone(), operations);
        }
    }

    /// Updates the latest block ID to be equal to the latest block ID known to the terminal model
    /// and flushes any previously-deferred operations for this new block ID.
    pub fn refresh_deferred_remote_operations(&mut self, ctx: &mut ViewContext<Self>) {
        let latest_block_id = self.model.lock().block_list().active_block_id().clone();
        self.deferred_remote_operations.latest_block_id = latest_block_id;
        self.flush_deferred_remote_operations(ctx);
    }

    /// Flushes any deferred remote operations for the latest known block ID.
    fn flush_deferred_remote_operations(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(operations) = self.deferred_remote_operations.flush() {
            self.editor.update(ctx, |editor, ctx| {
                editor.apply_remote_operations(operations, ctx);
            });
        }
    }

    /// Resets state in the input box that depends on the block lifecycle.
    /// This is on a performance-sensitive path.
    ///
    /// If the newly created block is for an executed user command, the input buffer is cleared.
    pub fn handle_block_completed_event(
        &mut self,
        block_completed_event: BlockCompletedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        // We clear the input box after executing a command here instead of where we
        // execute a command to avoid the input box flashing when its contents are
        // cleared. For the multiline input box case, this also caused contents to go
        // off the screen because we were forcing the long running command to be the same
        // size of the cleared input box.
        if let BlockType::User(user_block) = &block_completed_event.block_type {
            // Only clear the input buffer for user-executed commands, not agent-executed ones.
            let should_clear_buffer = !user_block.was_part_of_agent_interaction;
            let latest_block_id = self.model.lock().block_list().active_block_id().clone();
            let input_contents_before_prompt_chip_command =
                self.input_contents_before_prompt_chip_command.take();

            if should_clear_buffer {
                // We want to reinitialize the buffer whenever a command is completed so that
                // state does not leak from buffer to buffer (e.g. edit history).
                if self.deferred_remote_operations.latest_block_id != latest_block_id {
                    self.deferred_remote_operations.latest_block_id = latest_block_id;
                    self.editor
                        .update(ctx, |editor, ctx| editor.reinitialize_buffer(None, ctx));
                    self.latest_buffer_operations = Vec::new();

                    // If we have a pending input restore (from a prompt chip command like cd),
                    // restore the input contents instead of leaving the buffer empty.
                    if let Some(restore_text) = input_contents_before_prompt_chip_command {
                        self.editor.update(ctx, |editor, ctx| {
                            editor.set_buffer_text(&restore_text, ctx);
                        });
                        self.is_editor_empty_on_last_edit = false;
                    } else {
                        // This is the one place where buffer contents can change without an `Edit`
                        // -- this is because the buffer semantically isn't being edited, a new one is
                        // being constructed. We can guarantee in this case that the buffer was previously
                        // non-empty and should emit this event, because this code path is executed upon block
                        // completion in response to an executed command, though this guarantee is not explicitly
                        // enforced by the code.
                        self.is_editor_empty_on_last_edit = true;
                        ctx.emit(Event::InputEmptyStateChanged {
                            is_empty: true,
                            reason: InputEmptyStateChangeReason::UserCommandCompleted,
                        });
                    }
                }
            } else {
                // For agent-executed commands, still update the latest block ID but don't clear the buffer
                if self.deferred_remote_operations.latest_block_id != latest_block_id {
                    self.deferred_remote_operations.latest_block_id = latest_block_id;
                }
            }

            // Make sure the viewer's interaction state is correct based on their role.
            // We may have locked up their input if they tried to execute a command.
            if let SharedSessionStatus::ActiveViewer { role } =
                self.model.lock().shared_session_status()
            {
                self.editor.update(ctx, |editor, ctx| {
                    editor.set_interaction_state(role.into(), ctx);

                    // Also need to set the text colors back to normal.
                    let appearance: &Appearance = Appearance::as_ref(ctx);
                    editor.set_text_colors(TextColors::from_appearance(appearance), ctx);
                });

                if let Some(shared_session_input_state) = self.shared_session_input_state.as_mut() {
                    shared_session_input_state.pending_command_execution_request = None;
                };
            }

            // Generate autosuggestion if the input is not empty (user had type-ahead).
            self.maybe_generate_autosuggestion(ctx);
        }

        self.input_render_state_model_handle
            .update(ctx, |input_render_state_model, _| {
                input_render_state_model.set_editor_modified_since_block_finished(false);
            });

        // Re-render for anything that depends on the block list (e.g. zero state AM chips).
        ctx.notify();
    }

    /// Performs any post-block completion processing that's relevant to the input.
    ///
    /// This is triggered after [`Self::handle_block_completed_event`] as
    /// the handling of the main block completed event is a sensitive path.
    pub fn handle_after_block_completed_event(
        &mut self,
        block: BlockType,
        ctx: &mut ViewContext<Self>,
    ) {
        if let BlockType::User(block_completed) = block {
            self.last_user_block_completed = Some(block_completed.clone());

            let is_in_fullscreen_agent_view =
                self.agent_view_controller.as_ref(ctx).is_fullscreen();
            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                // If the user has autodetection enabled, unlock the input mode.
                // Otherwise, keep it locked in the current mode.
                let new_config = ai_input_model
                    .input_config()
                    .unlocked_if_autodetection_enabled(is_in_fullscreen_agent_view, ctx);
                ai_input_model.set_input_config(new_config, false, None, ctx);
            });

            let viewing_shared_session = self.model.lock().shared_session_status().is_viewer();
            if viewing_shared_session {
                // As we switch to the new block ID, if there were any remote
                // edits that were pending for that block ID, we should flush them.
                // Today, we only expect this to be the case with session-sharing viewers.
                self.flush_deferred_remote_operations(ctx);

                // Update shared session history model
                match self
                    .shared_session_input_state
                    .as_ref()
                    .map(|state| state.history_model.clone())
                {
                    Some(shared_session_history_model) => {
                        let command = block_completed
                            .command
                            .get_with(|compute| {
                                let model = self.model.lock();
                                compute(model.block_list())
                            })
                            .to_owned();
                        let serialized_block =
                            block_completed.serialized_block.get_with(|compute| {
                                let model = self.model.lock();
                                compute(model.block_list())
                            });
                        shared_session_history_model.update(ctx, move |history_model, _ctx| {
                            history_model
                                .push(HistoryEntry::for_completed_block(command, serialized_block))
                        })
                    }
                    _ => {
                        log::warn!("Tried to access non-existent shared session history model")
                    }
                }
            } else if is_next_command_enabled(ctx) {
                self.maybe_predict_next_action_ai(block_completed, ctx);
            }

            ctx.emit(Event::InputStateChanged(InputState::Enabled));
        } else if block.is_bootstrap_block()
            && self
                .model
                .lock()
                .block_list()
                .is_bootstrapping_precmd_done()
        {
            // When a bootstrap block is completed and the session is now
            // post-bootstrap, post-precmd, we know that the active block ID
            // is the block ID that we want to key input updates off of
            // (the block IDs during bootstrap are meaningless).
            self.refresh_deferred_remote_operations(ctx);

            // If the user typed ahead during bootstrap, the autosuggestion and
            // completions-as-you-type requests were silently skipped (history
            // wasn't queryable, session ID was absent). Now that bootstrap is
            // done, retry them so ghost text appears without the user having to
            // re-type.
            if !self.buffer_text(ctx).is_empty() {
                self.maybe_generate_autosuggestion(ctx);

                if self.should_show_completions_while_typing(ctx) {
                    self.open_completion_suggestions(CompletionsTrigger::AsYouType, ctx);
                }
            }
        }
    }

    /// 'Starts' the active block and sends its command bytes to the pty.
    ///
    /// Additionally, the executed command is recorded to history if appropriate.
    fn start_block_and_write_command_to_pty(
        &mut self,
        command: &str,
        source: CommandExecutionSource,
        ctx: &mut ViewContext<Self>,
    ) {
        start_trace!("command_execution:start");

        // Abort running completions since we're about to execute a command.
        if let Some(abort_handle) = self.completions_abort_handle.take() {
            abort_handle.abort();
        }
        self.abort_latest_autosuggestion_future();

        if let Some(future_handle) = self.decorations_future_handle.take() {
            future_handle.abort_handle().abort();
        }

        let session_id = self
            .active_block_session_id()
            .expect("session_id should be set (via bootstrap) before executing command");

        // If the SelectedWorkflowState is populated with a workflow, we count this as a workflow execution.
        let (workflow_id, workflow_command) = {
            match self.workflows_state.selected_workflow_state.as_ref() {
                Some(selected_workflow_state) => {
                    send_telemetry_from_ctx!(
                        TelemetryEvent::WorkflowExecuted(WorkflowTelemetryMetadata {
                            workflow_source: selected_workflow_state.workflow_source,
                            workflow_categories: selected_workflow_state
                                .workflow_type
                                .as_workflow()
                                .tags()
                                .cloned(),
                            workflow_selection_source: selected_workflow_state
                                .workflow_selection_source,
                            // This is only `Some()` for WarpDrive workflows; we don't track
                            // ID for execution of local workflows because they have no such
                            // unique ID.
                            workflow_id: selected_workflow_state.workflow_type.server_id(),
                            workflow_space: match &selected_workflow_state.workflow_type {
                                WorkflowType::Cloud(workflow) => Some(workflow.space(ctx).into()),
                                _ => None,
                            },
                            enum_ids: selected_workflow_state
                                .workflow_type
                                .as_workflow()
                                .get_server_enum_ids()
                        }),
                        ctx
                    );

                    let workflow_type = &selected_workflow_state.workflow_type;
                    let workflow_id = match workflow_type {
                        WorkflowType::Cloud(workflow) => Some(workflow.id),
                        _ => None,
                    };

                    // If the SelectedWorkflowState is populated, then we're always able to return the workflow command.
                    // The case where workflow_id = None but workflow_command = Some() is when it's a local workflow, which
                    // don't have ids and are tracked just by persisting the workflow contents. This is a little janky and would
                    // be fixed if we could identify all workflows under a unified id system, not just cloud ones.
                    (
                        workflow_id,
                        workflow_type
                            .as_workflow()
                            .command()
                            .map(|command| command.to_owned()),
                    )
                }
                None => (None, None),
            }
        };

        ctx.emit(Event::ExecuteCommand(Box::new(ExecuteCommandEvent {
            command: command.to_string(),
            workflow_id,
            session_id,
            workflow_command,
            should_add_command_to_history: true,
            source,
        })));
        end_trace!();
    }

    pub fn notify_and_notify_children(&self, ctx: &mut ViewContext<Self>) {
        ctx.notify();
        // The left notch may have been updated due to the prompt updating, in the case of
        // same-line prompt!
        self.editor.update(ctx, |_editor, ctx| {
            ctx.notify();
        });
    }

    /// Returns a tuple (prompt_text, rprompt_text).
    pub fn prompt_and_rprompt_text(&self, app: &AppContext) -> (String, Option<String>) {
        let model = self.model.lock();
        let appearance = Appearance::as_ref(app);
        let (lprompt_top, lprompt_bottom, rprompt) = self
            .prompt_render_helper
            .render_prompt(&model, appearance, app);
        // Separate this into a helper (follow-up PR?)

        let lprompt_top_text = lprompt_top.map(|rendered| rendered.element.text(app));
        let lprompt_bottom_text = lprompt_bottom.map(|rendered| rendered.element.text(app));
        let rprompt_text = rprompt.map(|rendered| rendered.element.text(app));
        if should_render_prompt_on_same_line(&model, app) {
            if let Some(lprompt_top_text) = lprompt_top_text {
                (
                    lprompt_top_text + "\n" + &lprompt_bottom_text.unwrap_or_default(),
                    rprompt_text,
                )
            } else {
                (lprompt_bottom_text.unwrap_or_default(), rprompt_text)
            }
        } else {
            (lprompt_top_text.unwrap_or_default(), rprompt_text)
        }
    }

    pub fn create_prompt_elements(&self, app: &AppContext) -> SessionNavigationPromptElements {
        let model = self.model.lock();
        let block = self.prompt_render_helper.prompt_block(&model);
        let mut prompt_elements = SessionNavigationPromptElements {
            ps1_prompt_grid: None,
            prompt_chip_snapshot: None,
        };

        if let Some(block) = block
            && block.honor_ps1()
            && model.block_list().is_bootstrapped()
        {
            // PS1 mode: capture the raw prompt grid so the command palette
            // can render it with full fidelity (CORE-1683).
            prompt_elements.ps1_prompt_grid = Some(block.prompt_grid().clone());
        }

        // Always capture a chip snapshot as the fallback prompt representation.
        // This covers both UDI mode and any edge cases where PS1 is not available
        // (e.g. not yet bootstrapped, block-level honor_ps1 mismatch).
        if prompt_elements.ps1_prompt_grid.is_none() {
            prompt_elements.prompt_chip_snapshot = Some(self.prompt_type.as_ref(app).snapshot(app));
        }
        prompt_elements
    }

    /// This function determines if the subshell flag should be in the input editor. The flag
    /// should show here if there are no blocks in the block list for this subshell session, which
    /// will be the case if no non-hidden blocks have been executed yet or the block list was
    /// cleared.
    fn get_subshell_flag_render_state(
        &self,
        model: &TerminalModel,
        spacing_is_compact: bool,
        app: &AppContext,
    ) -> Option<SubshellRenderState> {
        if spacing_is_compact {
            return None;
        }
        let session_id = self.active_block_session_id()?;
        let should_render = self
            .sessions
            .as_ref(app)
            .get(session_id)
            .and_then(|session| {
                session.subshell_info().as_ref().map(|info| {
                    if let Some(env_var_collection_name) = &info.env_var_collection_name {
                        Some(SubshellRenderState::Flag(SubshellSource::EnvVarCollection(
                            env_var_collection_name.to_owned(),
                        )))
                    } else {
                        info.spawning_command.split_whitespace().next().map(|exec| {
                            SubshellRenderState::Flag(SubshellSource::Command(exec.to_owned()))
                        })
                    }
                })
            })?;

        let block_list = model.block_list();
        let block_before_active_block = block_list
            .prev_non_hidden_block_from_index(block_list.active_block_index())
            .and_then(|index| block_list.block_at(index));

        match block_before_active_block {
            // If there is a block before the editor, and it belongs to this same subshell session,
            // the flag will be in the block list, and hence doesn't need to be in the editor.
            // Only extend the flag into the editor.
            Some(block) if block.session_id() == Some(session_id) => {
                Some(SubshellRenderState::Flagpole)
            }
            // Otherwise, this editor (the active block) is the first in this subshell session, and
            // we should show the flag here.
            _ => should_render,
        }
    }

    pub fn set_active_block_metadata(
        &mut self,
        active_block_metadata: BlockMetadata,
        is_after_in_band_command: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let active_session = active_block_metadata
            .session_id()
            .and_then(|session_id| self.sessions.as_ref(ctx).get(session_id));
        if let Some(session) = active_session {
            let transformer: Option<PathTransformerFn> = session
                .windows_path_converter()
                .map(|convert| Box::new(convert) as PathTransformerFn);
            self.editor.update(ctx, |editor, _| {
                editor.set_shell_family(session.shell().shell_type().into());
                editor.set_drag_drop_path_transformer(transformer);
            });
            self.input_suggestions.update(ctx, |input_suggestions, _| {
                input_suggestions.set_path_separators(session.path_separators());
            });
        }
        self.active_block_metadata = Some(active_block_metadata);

        // If needed, update the prompt display with the now-available session
        // context. In-band commands don't meaningfully change block metadata,
        // so only update prompt display chips if the previous block was not an
        // in-band command (i.e.: was probably a user-executed block).
        //
        // If we update the prompt display chips here, we can get into infinite
        // loops where we run an in-band command to compute an updated value for
        // a chip (e.g.: listing the files in the current directory), which
        // triggers another in-band command, etc. etc.
        if !is_after_in_band_command {
            self.update_prompt_display_chips(ctx);
        }
    }

    pub fn update_prompt_display_chips(&mut self, ctx: &mut ViewContext<Self>) {
        let session_context = self.completion_session_context(ctx);

        self.prompt_render_helper
            .prompt_view()
            .update(ctx, |prompt, prompt_ctx| {
                prompt.update_session_context(session_context.clone(), prompt_ctx);
            });
    }

    pub fn update_repo_path(&mut self, repo_path: Option<PathBuf>, ctx: &mut ViewContext<Self>) {
        self.prompt_render_helper
            .prompt_view()
            .update(ctx, |prompt, prompt_ctx| {
                prompt.update_repo_path(repo_path.clone(), prompt_ctx);
            });
    }

    fn active_session_path_if_local(&self, ctx: &ViewContext<Self>) -> Option<&Path> {
        self.active_block_session_id().and_then(|session_id| {
            let current_session = self.sessions.as_ref(ctx).get(session_id)?;
            if current_session.is_local() {
                self.active_block_metadata
                    .as_ref()
                    .and_then(BlockMetadata::current_working_directory)
                    .map(Path::new)
            } else {
                None
            }
        })
    }

    fn render_input_box(
        &self,
        show_vim_status: bool,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        // Set editor height to be half of the terminal view height
        let editor_height = self.size_info(app).pane_height_px() / 2.0.into_pixels();

        // Round down editor height to be divisible by line height so we do not see partial lines
        let line_height = self
            .editor
            .as_ref(app)
            .line_height(app.font_cache(), appearance)
            .into_pixels();
        let editor_height_rounded_down =
            (editor_height / line_height).round().max(1.0.into_pixels()) * line_height;

        let terminal_settings = TerminalSettings::as_ref(app);
        let terminal_spacing =
            terminal_settings.terminal_input_spacing(appearance.line_height_ratio(), app);
        let mut bottom_padding = terminal_spacing.editor_bottom_padding;

        // When `FeatureFlag::AgentView` is enabled, always render with UDI-style spacing values,
        // regardless of terminal/agent mode or prompt setting.
        let is_udi_style_spacing = !cfg!(feature = "local_only")
            && (self.should_show_universal_developer_input(app)
                || FeatureFlag::AgentView.is_enabled());

        let is_compact_mode =
            matches!(terminal_settings.spacing_mode.value(), SpacingMode::Compact)
                && !is_udi_style_spacing;

        // In compact mode, allocate some extra padding for the Vim status bar.
        if is_compact_mode && show_vim_status {
            bottom_padding = VIM_STATUS_BAR_BOTTOM_PADDING;
        }

        if is_udi_style_spacing {
            bottom_padding = terminal_spacing.editor_bottom_padding - 4.;
        }

        let input_box = Container::new(
            ConstrainedBox::new(Clipped::new(ChildView::new(&self.editor).finish()).finish())
                .with_max_height(editor_height_rounded_down.as_f32())
                .finish(),
        )
        .with_padding_right(*TERMINAL_VIEW_PADDING_LEFT)
        .with_padding_bottom(bottom_padding)
        .finish();

        let input_editor_save_position_id = self.editor_save_position_id();
        SavePosition::new(
            EventHandler::new(input_box)
                .on_right_mouse_down(move |ctx, _, position| {
                    let input_rect = ctx
                        .element_position_by_id(input_editor_save_position_id.clone())
                        .expect("input editor position id should be saved");
                    let offset_position = position - input_rect.origin();
                    ctx.dispatch_typed_action(TerminalAction::OpenInputContextMenu {
                        position: offset_position,
                    });
                    DispatchEventResult::StopPropagation
                })
                .finish(),
            &self.editor_save_position_id(),
        )
        .finish()
    }

    // TODO remove voltron from the code given we are not using it anymore, and we have universal search instead.
    fn select_and_refresh_voltron(
        &mut self,
        feature_item: VoltronItem,
        ctx: &mut ViewContext<Input>,
    ) {
        // View-only sessions should not show workflows menu
        if self.model.lock().shared_session_status().is_reader() {
            return;
        }

        let welcome_tip_feature = match feature_item {
            VoltronItem::AiCommands => Some(Tip::Action(TipAction::AiCommandSearch)),
            VoltronItem::History => Some(Tip::Action(TipAction::HistorySearch)),
            VoltronItem::Workflows => None,
        };

        if let Some(welcome_tip_feature) = welcome_tip_feature {
            self.tips_completed.update(ctx, |tips_completed, ctx| {
                mark_feature_used_and_write_to_user_defaults(
                    welcome_tip_feature,
                    tips_completed,
                    ctx,
                );
                ctx.notify();
            });
        }
        // If input suggestions are opened we should close them when opening voltron
        if self.suggestions_mode_model.as_ref(ctx).is_visible() {
            self.close_input_suggestions_and_restore_buffer(true, true, ctx);
        }
        let active_session_path_if_local = self.active_session_path_if_local(ctx);
        let menu_positioning = self.menu_positioning(ctx);
        let metadata = VoltronMetadata {
            active_session_path_if_local: active_session_path_if_local.map(|path| path.into()),
            starting_editor_text: Some(self.editor.as_ref(ctx).buffer_text(ctx)),
            keymap_context: Self::keymap_context(self, ctx),
            menu_positioning,
        };

        self.voltron_view.update(ctx, |voltron, ctx| {
            voltron.select_and_refresh_by_name(feature_item, metadata, ctx);
            self.is_voltron_open = true;
        });
        ctx.notify();
    }

    /// Returns whether the buffer contains any attachment patterns (blocks, drive objects, or diffs).
    /// These patterns indicate the user is referencing context that requires AI mode.
    fn buffer_contains_attachment_patterns(buffer_text: &str) -> bool {
        BLOCK_CONTEXT_ATTACHMENT_REGEX.is_match(buffer_text)
            || DRIVE_OBJECT_ATTACHMENT_REGEX.is_match(buffer_text)
            || DIFF_HUNK_ATTACHMENT_REGEX.is_match(buffer_text)
    }

    /// Returns the SavePosition ID for the input.
    ///
    /// This may be used by parent views to position UI elements relative to the input.
    pub fn save_position_id(&self) -> String {
        format!("input_{}", self.view_id)
    }

    /// Returns the position ID for the input editor
    pub fn editor_save_position_id(&self) -> String {
        format!("input_editor_{}", self.view_id)
    }

    /// Returns the position ID for the (left) prompt.
    pub fn prompt_save_position_id(&self) -> String {
        format!("prompt_area_{}", self.view_id)
    }

    /// A save position for the bordered input alone,
    /// not including the status bar.
    pub fn status_free_input_save_position_id(&self) -> String {
        format!("status_free_input_{}", self.view_id)
    }

    pub fn should_show_universal_developer_input(&self, app: &AppContext) -> bool {
        !cfg!(feature = "local_only")
            && InputSettings::as_ref(app).is_universal_developer_input_enabled(app)
    }

    /// Returns whether the input box is currently pinned to the top of the screen.
    fn is_input_at_top(&self, model: &TerminalModel, ctx: &AppContext) -> bool {
        match InputModeSettings::as_ref(ctx).input_mode.value() {
            InputMode::PinnedToBottom => false,
            InputMode::PinnedToTop => true,
            InputMode::Waterfall => model.is_block_list_empty(),
        }
    }
}

impl Entity for Input {
    type Event = Event;
}

impl TypedActionView for Input {
    type Action = InputAction;

    fn action_accessibility_contents(
        &mut self,
        action: &InputAction,
        _: &mut ViewContext<Self>,
    ) -> ActionAccessibilityContent {
        match action {
            InputAction::FocusInputBox => {
                ActionAccessibilityContent::Custom(AccessibilityContent::new(
                    INPUT_A11Y_LABEL,
                    // TODO (a11y) use bindings from user settings
                    INPUT_A11Y_HELPER,
                    WarpA11yRole::TextareaRole,
                ))
            }
            _ => ActionAccessibilityContent::Empty,
        }
    }

    fn handle_action(&mut self, action: &InputAction, ctx: &mut ViewContext<Self>) {
        if !action.is_available_in_product() {
            return;
        }

        match action {
            InputAction::FocusInputBox => self.focus_input_box(ctx),
            InputAction::Up => self.editor_up(ctx),
            InputAction::PageUp => self.editor_page_up(ctx),
            InputAction::PageDown => self.editor_page_down(ctx),
            InputAction::CtrlD => self.ctrl_d(ctx),
            InputAction::CtrlR => self.ctrl_r(ctx),
            InputAction::ClearScreen => self.clear_screen(ctx),
            InputAction::SelectAndRefreshVoltron(feature_name) => {
                self.select_and_refresh_voltron(*feature_name, ctx);
            }
            InputAction::MaybeOpenCompletionSuggestions => {
                self.maybe_open_completion_suggestions(ctx);
            }
            InputAction::HideWorkflowInfoCard => self.hide_workflows_info_box(ctx),
            InputAction::ResetWorkflowState => self.reset_workflow_state(None, ctx),
            InputAction::ToggleClassicCompletionsMode => {
                InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                    if let Err(e) = settings.classic_completions_mode.toggle_and_save_value(ctx) {
                        log::warn!(
                            "Failed to toggle and save classic completions mode setting: {e}."
                        )
                    }
                });
            }
            InputAction::ToggleInputAutoDetection => {
                if let Ok(new_value) =
                    AISettings::handle(ctx).update(ctx, |ai_settings, model_ctx| {
                        ai_settings
                            .ai_autodetection_enabled_internal
                            .toggle_and_save_value(model_ctx)
                    })
                {
                    send_telemetry_from_ctx!(
                        TelemetryEvent::AgentModeToggleAutoDetectionSetting {
                            is_autodetection_enabled: new_value,
                            origin: AgentModeAutoDetectionSettingOrigin::Banner
                        },
                        ctx
                    );
                }
            }
            InputAction::CycleNextCommandSuggestion => {
                self.cycle_next_command_suggestion(ctx);
            }

            InputAction::UpdateCompletionsMenuWidth(width) => {
                InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.completions_menu_width.set_value(*width, ctx));
                });
            }
            InputAction::UpdateCompletionsMenuHeight(height) => {
                InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.completions_menu_height.set_value(*height, ctx));
                });
            }

            InputAction::OpenInlineHistoryMenu => {
                self.open_inline_history_menu(ctx);
            }

            InputAction::ClearAttachedContext => {
                self.clear_attached_context(ctx);
            }
        }
    }
}

impl View for Input {
    fn ui_name() -> &'static str {
        "Input"
    }

    fn accessibility_contents(&self, _: &AppContext) -> Option<AccessibilityContent> {
        Some(AccessibilityContent::new(
            INPUT_A11Y_LABEL,
            // TODO (a11y) use bindings from user settings
            INPUT_A11Y_HELPER,
            WarpA11yRole::TextareaRole,
        ))
    }

    fn on_focus(&mut self, focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        if focus_ctx.is_self_focused() {
            if self.is_voltron_open {
                ctx.focus(&self.voltron_view);
            } else if self.prompt_render_helper.has_open_chip_menu(ctx) {
                // Focus the PromptDisplay, which will in turn focus any open chip menu
                ctx.focus(self.prompt_render_helper.prompt_view());
            } else {
                self.close_voltron(ctx);
                ctx.focus(&self.editor);
                ctx.notify();
            }
            ctx.dispatch_typed_action(&PaneGroupAction::HandleFocusChange);
        }
    }

    fn keymap_context(&self, app: &AppContext) -> warpui::keymap::Context {
        let mut ctx = Self::default_keymap_context();

        #[cfg(feature = "local_only")]
        {
            ctx.set.insert(flags::TERMINAL_MODE_INPUT);
            if self.buffer_text(app).is_empty() {
                ctx.set.insert(flags::EMPTY_INPUT_BUFFER);
            }
            if AppEditorSettings::as_ref(app).vim_mode_enabled() {
                ctx.set.insert("VimModeEnabled");
            }
            if let Some(VimMode::Normal) = self.editor.as_ref(app).vim_mode(app) {
                ctx.set.insert("VimNormalMode");
            }

            let model_lock = self.model.lock();
            if model_lock
                .block_list()
                .active_block()
                .is_active_and_long_running()
            {
                ctx.set.insert("LongRunningCommand");
            }
            if model_lock.is_block_list_empty() {
                ctx.set.insert("TerminalView_EmptyBlockList");
            } else {
                ctx.set.insert("TerminalView_NonEmptyBlockList");
            }
            return ctx;
        }
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.render_classic_input(app)
    }
}

impl Autosuggester for Input {
    fn on_autosuggestion_result(
        &mut self,
        result: AutoSuggestionResult,
        ctx: &mut ViewContext<Self>,
    ) {
        let buffer_text = result.buffer_text;
        if self.editor.as_ref(ctx).buffer_text(ctx) != buffer_text {
            return;
        }

        let autosuggestion_result_substring = result
            .autosuggestion_result
            .as_ref()
            .and_then(|result| result.strip_prefix(buffer_text.as_str()));

        if let Some(autosuggestion) = autosuggestion_result_substring {
            self.set_autosuggestion(
                autosuggestion,
                AutosuggestionType::Command {
                    was_intelligent_autosuggestion: false,
                },
                ctx,
            );
        }
    }

    fn abort_latest_autosuggestion_future(&mut self) {
        if let Some(last_abort_handle) = self.autosuggestions_abort_handle.take() {
            last_abort_handle.abort();
        }
    }

    fn set_autosuggestion_future(&mut self, abort_handle: AbortHandle) {
        self.autosuggestions_abort_handle = Some(abort_handle);
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
