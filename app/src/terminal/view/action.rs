use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

use command_corrections::Correction;
use pathfinder_geometry::vector::Vector2F;
use session_sharing_protocol::common::Role;
use warp_util::user_input::UserInput;
use warpui::EntityId;
use warpui::elements::HyperlinkUrl;
use warpui::event::ModifiersState;
use warpui::units::Lines;

use super::inline_banner::{
    AwsBedrockLoginBannerAction, AwsCliNotInstalledBannerAction, VimModeBannerAction,
};
use super::{
    AliasExpansionBannerAction, ContextMenuAction, GridHighlightedLink, InputContextMenuAction,
    NotificationsDiscoveryBannerAction, NotificationsErrorBannerAction, RichContentLink,
    TerminalEditor,
};
use crate::ai::agent::AIAgentExchangeId;
use crate::ai::agent::conversation::AIConversationId;
use crate::server::ids::SyncId;
use crate::server::telemetry::{AgentModeRewindEntrypoint, ToggleBlockFilterSource};
use crate::terminal::available_shells::AvailableShell;
use crate::terminal::block_list_element::{
    BlockHoverAction, BlockListMenuSource, BlockSelectAction, BlockTextSelectAction,
};
use crate::terminal::block_list_viewport::OverhangingBlock;
use crate::terminal::model::SecretHandle;
use crate::terminal::model::completions::ShellCompletion;
use crate::terminal::model::index::Point;
use crate::terminal::model::mouse::MouseState;
use crate::terminal::model::selection::{SelectAction, SelectionDirection};
use crate::terminal::model::terminal_model::{BlockIndex, WithinModel};
use crate::terminal::shared_session::SharedSessionActionSource;
use crate::terminal::view::RichContentSecretTooltipInfo;
use crate::terminal::view::inline_banner::AgentModeSetupSpeedbumpBannerAction;
use crate::workflows::workflow::Workflow;

/// This represents whether entering a subshell for a particular command should become automatic in
/// the future, or to ask again.
#[derive(Clone, Debug)]
pub enum RememberForWarpification {
    /// If yes, need to transmit the command itself so it can be persisted to user-defaults
    RememberSubshellCommand(String),
    RememberSSHHost(String),
    DoNotRememberSubshellCommand,
    DoNotRememberSSHHost,
}

impl RememberForWarpification {
    pub fn as_bool(&self) -> bool {
        match self {
            RememberForWarpification::RememberSubshellCommand(_) => true,
            RememberForWarpification::RememberSSHHost(_) => true,
            RememberForWarpification::DoNotRememberSubshellCommand => false,
            RememberForWarpification::DoNotRememberSSHHost => false,
        }
    }

    pub fn is_ssh(&self) -> bool {
        match self {
            RememberForWarpification::RememberSSHHost(_) => true,
            RememberForWarpification::DoNotRememberSSHHost => true,
            RememberForWarpification::RememberSubshellCommand(_) => false,
            RememberForWarpification::DoNotRememberSubshellCommand => false,
        }
    }
}

#[derive(Clone)]
pub enum TerminalAction {
    Scroll {
        delta: Lines,
    },
    AltScroll {
        delta: i32,
        point: Point,
    },
    SharedSessionViewerAltScroll {
        new_scroll_top: Lines,
    },
    ScrollToTopOfBlock {
        topmost_block: BlockIndex,
    },
    BlockTextSelect(BlockTextSelectAction),
    BlockSelect {
        action: BlockSelectAction,
        should_redetermine_focus: bool,
    },
    BlockHover(BlockHoverAction),
    BlockSnackbarHover {
        is_hovered: bool,
    },
    BlockNearSnackbarHover {
        is_hovered: bool,
    },

    // TODO: we should eventually use a Modifiers struct here instead of using
    // an aggregated is_selecting_blocks when we need better granularity.
    // This refactor will need to start from the Events themselves.
    ClickOnGrid {
        position: WithinModel<Point>,
        modifiers: ModifiersState,
    },
    MiddleClickOnGrid {
        /// `None` here means that the click was on the Block List but not on a particular blockgrid.
        position: Option<WithinModel<Point>>,
    },
    MiddleClickOnInput,
    MaybeLinkHover {
        position: Option<WithinModel<Point>>,
        from_editor: TerminalEditor,
    },
    MaybeHoverSecret {
        secret_handle: Option<SecretHandle>,
    },
    MaybeDismissToolTip {
        from_keybinding: bool,
    },
    AltScreenContextMenu {
        position: Vector2F,
    },
    AltSelect(SelectAction<Point>),
    MaybeClearAltSelect,
    AltMouseAction(MouseState),
    InsertCommandCorrection {
        correction: Correction,
    },
    BlockListContextMenu(BlockListMenuSource),
    CloseContextMenu,
    Paste,
    Copy,
    CopyOutputs,
    CopyCommands,
    CopyGitBranch,
    ReinputCommands,
    ReinputCommandsWithSudo,
    ClearBuffer,
    Focus,
    FocusInputAndClearSelection,
    ShowFindBar,
    SelectPriorBlock,
    SelectBookmarkDown,
    SelectBookmarkUp,
    JumpToLatestAgentMessage,
    BookmarkSelectedBlock,
    ScrollToBottomOfSelectedBlocks,
    ScrollToTopOfSelectedBlocks,
    ScrollToBottomOfOverhangingBlock(OverhangingBlock),
    SelectNextBlock,
    Up,
    OpenBlockListContextMenu,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    KeyboardSelectText(SelectionDirection),
    UserInputSequence(Vec<u8>),
    ControlSequence(Vec<u8>),
    RunNativeShellCompletions {
        buffer_text: String,
        results_tx: async_channel::Sender<Vec<ShellCompletion>>,
    },
    KeyDown(String),
    TypedCharacters(String),
    ContextMenu(ContextMenuAction),
    // IMPORTANT: Do not add a binding for ctrl_d, as we don't want this behavior to leak out to
    // parts of the terminal unrelated to the block list
    CtrlD,
    CtrlC,
    ClearSelectionsWhenShellMode,
    Close,
    ToggleMaximizePane,
    SplitRight(Option<AvailableShell>),
    SplitLeft(Option<AvailableShell>),
    SplitDown(Option<AvailableShell>),
    SplitUp(Option<AvailableShell>),
    /// The context menu that's used for the prompt directly above input editor
    PromptContextMenu {
        position_offset_from_prompt: Vector2F,
    },
    OpenInputContextMenu {
        position: Vector2F,
    },
    InputContextMenuItem(InputContextMenuAction),
    /// Open the menu on the specified [`crate::ai::blocklist::AIBlock`] that lists the blocks that
    /// were attached to the query in the specified [`crate::ai::blocklist::AIAgentExchange`] which
    /// is part of the specified [`crate::ai::blocklist::AIConversation`].
    OpenAIBlockAttachedBlocksMenu {
        ai_block_view_id: EntityId,
        exchange_id: AIAgentExchangeId,
        conversation_id: AIConversationId,
    },
    /// Open the overflow context menu for an AI block with copy options
    OpenAIBlockOverflowMenu {
        ai_block_view_id: EntityId,
        exchange_id: AIAgentExchangeId,
        conversation_id: AIConversationId,
        is_restored: bool,
    },
    /// Show the confirmation dialog before rewinding an AI conversation
    RewindAIConversation {
        ai_block_view_id: EntityId,
        exchange_id: AIAgentExchangeId,
        conversation_id: AIConversationId,
        /// The entrypoint from which this action was triggered (for telemetry).
        entrypoint: AgentModeRewindEntrypoint,
    },
    /// Actually execute the rewind (called after user confirms in the dialog)
    ExecuteRewindAIConversation {
        ai_block_view_id: EntityId,
        exchange_id: AIAgentExchangeId,
        conversation_id: AIConversationId,
    },
    SelectAllBlocks,
    ExpandBlockSelectionAbove,
    ExpandBlockSelectionBelow,
    NotificationsDiscoveryBanner(NotificationsDiscoveryBannerAction),
    BookmarkBlock(BlockIndex),
    NotificationsErrorBanner(NotificationsErrorBannerAction),
    JumpToBookmark(BlockIndex),
    OpenGridLink(GridHighlightedLink),
    OpenRichContentLink(RichContentLink),
    ToggleGridSecret {
        handle: WithinModel<SecretHandle>,
        show_secret: bool,
    },
    CopyGridSecret(WithinModel<SecretHandle>),
    ToggleRichContentSecret {
        rich_content_tooltip_info: RichContentSecretTooltipInfo,
        show_secret: bool,
    },
    CopyRichContentSecret(RichContentSecretTooltipInfo),
    ShowInFileExplorer(PathBuf),
    OpenWorkflowModal,
    OpenWorkflowModalForAIWorkflow(Workflow),
    OpenWorkflowModalForBlock(BlockIndex),
    OpenWorkflowModalWithCloudWorkflow(SyncId),
    /// Starts a subshell in the active session.
    TriggerSubshellBootstrap,
    /// If the user says "no" to Warpification, possibly requesting not to be asked again
    DismissWarpifyBanner(RememberForWarpification),
    /// Triggers the banner asking to turn the running block into a subshell. The String is the
    /// command that the user entered.
    ShowSubshellBanner(String),
    InsertMostRecentCommandCorrection,
    AliasExpansionBanner(AliasExpansionBannerAction),
    OpenBlockFilterEditor(BlockIndex),
    ImportSettings,
    OpenSharedSessionOnDesktop {
        source: SharedSessionActionSource,
    },
    ToggleBlockFilterOnSelectedOrLastBlock(ToggleBlockFilterSource),
    CopySharedSessionLink {
        source: SharedSessionActionSource,
    },
    VimModeBanner(VimModeBannerAction),
    ToggleSnackbarInActivePane,
    OpenSharedSessionViewerRoleMenu,
    RequestSharedSessionRole(Role),
    /// User selected a block inside an AI block's attached block menu so we jump to it and select
    /// it if possible.
    SelectAIAttachedBlock(BlockIndex),
    DragAndDropFiles(Vec<String>),
    HyperlinkClick(HyperlinkUrl),

    StartFileDropTarget,
    StopFileDropTarget,
    SetMarkedText {
        marked_text: UserInput<String>,
        selected_range: Range<usize>,
    },
    ClearMarkedText,
    HideTelemetryBannerPermanently,
    ShowInitializationBlock,
    GenerateCodebaseIndex,
    ShowWarpifySettings,
    /// Removes a pending attachment (image or file) by index in the unified list.
    DeleteAttachment {
        index: usize,
    },
    /// Opens a pending input attachment image in the workspace lightbox before
    /// the attachment has been submitted with a user query.
    OpenAttachmentLightbox {
        index: usize,
    },
    WriteCodebaseIndex,
    ToggleAutoexecuteMode,

    AgentModeSetupSpeedbumpBanner(AgentModeSetupSpeedbumpBannerAction),
    ResumeConversation,
    ToggleTodoPopup,
    CloseTodoPopup,
    InitProject,
    SummarizeConversation,
    AddProjectAtCurrentDirectory,
    OpenAddRulePane,
    OpenRulesPane,
    OpenBillingAndUsagePane,
    PickRepoToOpen,
    DismissCodeToolbeltTooltip,
    /// Start the guided Warp Environment setup flow (inserts the inline setup block).
    SetupCloudEnvironment(Vec<String>),
    /// Start the guided Warp Environment setup flow immediately (no inline setup block).
    SetupCloudEnvironmentAndStart(Vec<String>),
    /// Show the environment setup mode selector to choose between remote GitHub or local agent flow.
    TriggerEnvironmentSetupSelection(Vec<String>),
    /// Open the Environment Management pane.
    OpenEnvironmentManagementPane,
    ToggleLongRunningCommandControl,
    ToggleHideCliResponses,
    ExitAgentView,
    /// Cancel the ambient agent task while it's loading
    CancelAmbientAgentTask,
    OpenInlineHistoryMenu,
    AwsBedrockLoginBanner(AwsBedrockLoginBannerAction),
    AwsCliNotInstalledBanner(AwsCliNotInstalledBannerAction),
    /// Toggle the usage footer on the last AI block in the active conversation.
    ToggleUsageFooter,
    /// Toggle PTY recording for this session.
    ToggleSessionRecording,

    /// Allow the blocked clipboard operation by adjusting the OSC 52 clipboard access setting.
    Osc52AllowBlockedClipboardOperation,
}

impl TerminalAction {
    pub fn is_available_in_product(&self) -> bool {
        #[cfg(not(feature = "local_only"))]
        return true;

        #[cfg(feature = "local_only")]
        {
            use TerminalAction::*;

            !matches!(
                self,
                JumpToLatestAgentMessage
                    | OpenAIBlockAttachedBlocksMenu { .. }
                    | OpenAIBlockOverflowMenu { .. }
                    | RewindAIConversation { .. }
                    | ExecuteRewindAIConversation { .. }
                    | OpenWorkflowModal
                    | OpenWorkflowModalForAIWorkflow(_)
                    | OpenWorkflowModalForBlock(_)
                    | OpenWorkflowModalWithCloudWorkflow(_)
                    | OpenSharedSessionOnDesktop { .. }
                    | CopySharedSessionLink { .. }
                    | OpenSharedSessionViewerRoleMenu
                    | RequestSharedSessionRole(_)
                    | SelectAIAttachedBlock(_)
                    | StartFileDropTarget
                    | StopFileDropTarget
                    | HideTelemetryBannerPermanently
                    | GenerateCodebaseIndex
                    | DeleteAttachment { .. }
                    | OpenAttachmentLightbox { .. }
                    | WriteCodebaseIndex
                    | ToggleAutoexecuteMode
                    | AgentModeSetupSpeedbumpBanner(_)
                    | ResumeConversation
                    | ToggleTodoPopup
                    | CloseTodoPopup
                    | InitProject
                    | SummarizeConversation
                    | AddProjectAtCurrentDirectory
                    | OpenAddRulePane
                    | OpenRulesPane
                    | OpenBillingAndUsagePane
                    | PickRepoToOpen
                    | DismissCodeToolbeltTooltip
                    | SetupCloudEnvironment(_)
                    | SetupCloudEnvironmentAndStart(_)
                    | TriggerEnvironmentSetupSelection(_)
                    | OpenEnvironmentManagementPane
                    | ToggleLongRunningCommandControl
                    | ToggleHideCliResponses
                    | ExitAgentView
                    | CancelAmbientAgentTask
                    | AwsBedrockLoginBanner(_)
                    | AwsCliNotInstalledBanner(_)
                    | ToggleUsageFooter
            ) && !matches!(self, ContextMenu(action) if !action.is_available_in_product())
                && !matches!(self, InputContextMenuItem(action) if !action.is_available_in_product())
        }
    }
}

// Manually implementing Debug to avoid leaking sensitive information in logs
impl fmt::Debug for TerminalAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use TerminalAction::*;

        match self {
            Scroll { delta } => write!(f, "Scroll {{ delta: {delta} }}"),
            AltScroll { delta, .. } => write!(f, "AltScroll {{ delta: {delta} }}"),
            SharedSessionViewerAltScroll { new_scroll_top } => write!(
                f,
                "SharedSessionViewerAltScroll {{ new_scroll_top: {new_scroll_top} }}"
            ),
            ScrollToTopOfBlock { topmost_block } => write!(
                f,
                "JumpToPreviousCommand {{ topmost_block: {topmost_block} }}"
            ),
            ScrollToTopOfSelectedBlocks => f.write_str("ScrollToTopOfSelectedBlocks"),
            ScrollToBottomOfSelectedBlocks => f.write_str("ScrollToBottomOfSelectedBlocks"),
            ScrollToBottomOfOverhangingBlock(overhanging_block) => {
                write!(f, "ScrollToBottomOfOverhangingBlock {overhanging_block:?}")
            }
            BlockTextSelect(action) => write!(f, "BlockTextSelect({action:?})"),
            BlockSelect { action, .. } => write!(f, "BlockSelect({action:?})"),
            BlockHover(action) => write!(f, "BlockHover({action:?})"),
            BlockSnackbarHover { is_hovered } => {
                write!(f, "BlockSnackbarHover{{ is_hovered {is_hovered} }}")
            }
            BlockNearSnackbarHover { is_hovered } => {
                write!(f, "BlockNearSnackbarHover{{ is_hovered {is_hovered} }}")
            }
            ClickOnGrid {
                position,
                modifiers,
            } => write!(
                f,
                "ClickOnGrid {{ position: {position:?}, modifiers: {modifiers:?} }}"
            ),
            MaybeLinkHover {
                position,
                from_editor,
            } => write!(
                f,
                "MaybeLinkHover {{ position: {position:?}, from_editor: {from_editor:?} }}"
            ),
            MaybeHoverSecret { secret_handle } => {
                write!(f, "MaybeHoverSecret {{ secret_handle: {secret_handle:?} }}")
            }
            MaybeDismissToolTip { from_keybinding } => write!(
                f,
                "MaybeDismissToolTip {{ from_keybinding: {from_keybinding:?}}}"
            ),
            AltSelect(action) => write!(f, "AltSelect({action:?})"),
            MaybeClearAltSelect => f.write_str("MaybeClearAltSelect"),
            AltMouseAction(action) => write!(f, "AltMouseAction({action:?})"),
            AltScreenContextMenu { position } => {
                write!(f, "AltScreenContextMenu {{ position: {position:?} }}")
            }
            BlockListContextMenu(menu) => write!(f, "BlockListContextMenu({menu:?})"),
            CloseContextMenu => f.write_str("CloseContextMenu"),
            Paste => f.write_str("Paste"),
            Copy => f.write_str("Copy"),
            CopyOutputs => f.write_str("CopyOutputs"),
            CopyCommands => f.write_str("CopyCommands"),
            CopyGitBranch => f.write_str("CopyGitBranch"),
            ReinputCommands => f.write_str("ReinputCommands"),
            ReinputCommandsWithSudo => f.write_str("ReinputCommandsWithSudo"),
            ClearBuffer => f.write_str("ClearBuffer"),
            SelectBookmarkUp => f.write_str("SelectBookmarkUp"),
            JumpToLatestAgentMessage => f.write_str("JumpToLatestAgentMessage"),
            SelectBookmarkDown => f.write_str("SelectBookmarkDown"),
            Focus => f.write_str("Focus"),
            FocusInputAndClearSelection => f.write_str("FocusInputAndClearSelection"),
            ShowFindBar => f.write_str("ShowFindBar"),
            SelectPriorBlock => f.write_str("SelectPriorBlock"),
            SelectNextBlock => f.write_str("SelectNextBlock"),
            BookmarkSelectedBlock => f.write_str("BookmarkSelectedBlock"),
            Up => f.write_str("Up"),
            Down => f.write_str("Down"),
            PageUp => f.write_str("PageUp"),
            PageDown => f.write_str("PageDown"),
            Home => f.write_str("Home"),
            End => f.write_str("End"),
            KeyboardSelectText(direction) => write!(f, "KeyboardSelectText({direction:?})"),
            ContextMenu(action) => write!(f, "ContextMenu({action:?})"),
            CtrlD => f.write_str("CtrlD"),
            CtrlC => f.write_str("CtrlC"),
            ClearSelectionsWhenShellMode => {
                f.write_str("ClearSelectionsWhenShellMode(TerminalAction)")
            }
            Close => f.write_str("Close"),
            SplitRight(_) => f.write_str("SplitRight"),
            SplitLeft(_) => f.write_str("SplitLeft"),
            SplitDown(_) => f.write_str("SplitDown"),
            SplitUp(_) => f.write_str("SplitUp"),
            ToggleMaximizePane => f.write_str("ToggleMaximizeActivePane"),
            PromptContextMenu {
                position_offset_from_prompt,
            } => write!(
                f,
                "PromptContextMenu {{ position_offset_from_prompt: {position_offset_from_prompt:?} }}"
            ),
            OpenInputContextMenu { position } => {
                write!(f, "OpenInputContextMenu {{ position: {position:?} }}")
            }
            InputContextMenuItem(action) => write!(f, "InputContextMenuItem({action:?})"),
            SelectAllBlocks => f.write_str("SelectAllBlocks"),
            ExpandBlockSelectionAbove => f.write_str("ExpandBlockSelectionAbove"),
            ExpandBlockSelectionBelow => f.write_str("ExpandBlockSelectionBelow"),
            UserInputSequence(_) => f.write_str("UserInputSequence"),
            ControlSequence(_) => f.write_str("ControlSequence"),
            KeyDown(_) => f.write_str("KeyDown"),
            TypedCharacters(_) => f.write_str("TypedCharacters"),
            NotificationsDiscoveryBanner(action) => {
                write!(f, "NotificationsDiscoveryBanner({action:?})")
            }
            BookmarkBlock(index) => {
                write!(f, "BookmarkBlock({index:?})")
            }
            NotificationsErrorBanner(action) => write!(f, "NotificationsErrorBanner({action:?})"),
            JumpToBookmark(index) => write!(f, "JumpToBookmark({index:?})"),
            InsertCommandCorrection { .. } => {
                write!(f, "InsertCommandCorrection",)
            }
            OpenGridLink(_) => f.write_str("OpenGridLink"),
            OpenRichContentLink(_) => f.write_str("OpenRichContentLink"),
            ToggleGridSecret { show_secret, .. } => write!(f, "ToggleGridSecret {show_secret:?}"),
            ToggleRichContentSecret { show_secret, .. } => {
                write!(f, "ToggleRichContentSecret {show_secret:?}")
            }
            CopyGridSecret(_) => f.write_str("CopyGridSecret"),
            CopyRichContentSecret(_) => f.write_str("CopyRichContentSecret"),
            ShowInFileExplorer(_) => f.write_str("ShowInFileExplorer"),
            OpenWorkflowModal => f.write_str("OpenWorkflowModal"),
            OpenWorkflowModalForAIWorkflow(_) => f.write_str("OpenWorkflowModalForAIWorkflow"),
            OpenWorkflowModalForBlock(block_index) => {
                write!(f, "OpenWorkflowModalForBlock({block_index:?})")
            }
            OpenWorkflowModalWithCloudWorkflow(_) => {
                f.write_str("OpenWorkflowModalWithCloudWorkflow")
            }
            OpenBlockListContextMenu => f.write_str("OpenBlockListContextMenu"),
            TriggerSubshellBootstrap => f.write_str("TriggerSubshellBootstrap"),
            DismissWarpifyBanner(remember) => write!(f, "DismissWarpifyBanner({remember:?})"),
            ShowSubshellBanner(_) => f.write_str("ShowSubshellBanner"),
            InsertMostRecentCommandCorrection => f.write_str("InsertMostRecentCommandCorrection"),
            AliasExpansionBanner(action) => write!(f, "AliasExpansionBanner({action:?}"),
            OpenBlockFilterEditor(block_index) => {
                write!(f, "OpenBlockFilterEditor({block_index:?})")
            }
            ImportSettings => write!(f, "ImportSettings"),
            OpenSharedSessionOnDesktop { source } => {
                write!(f, "OpenSharedSessionOnDesktop({source:?})")
            }
            ToggleBlockFilterOnSelectedOrLastBlock(_) => {
                f.write_str("ToggleBlockFilterOnSelectedOrLastBlock")
            }

            CopySharedSessionLink { .. } => f.write_str("CopySharedSessionLink"),
            VimModeBanner(action) => write!(f, "VimModeBanner({action:?})"),
            ToggleSnackbarInActivePane => write!(f, "ToggleSnackbarInActivePane"),
            OpenSharedSessionViewerRoleMenu => write!(f, "OpenSharedSessionViewerRoleMenu"),
            RequestSharedSessionRole(role) => write!(f, "RequestSharedSessionRole({role:?})"),
            MiddleClickOnGrid { position } => {
                write!(f, "MiddleClickonGrid {{ position: {position:?} }}")
            }
            MiddleClickOnInput => write!(f, "MiddleClickOnInput"),
            OpenAIBlockAttachedBlocksMenu { .. } => write!(f, "OpenAIBlockAttachedBlocksMenu"),
            OpenAIBlockOverflowMenu { .. } => write!(f, "OpenAIBlockOverflowMenu"),
            RewindAIConversation { .. } => write!(f, "RewindAIConversation"),
            ExecuteRewindAIConversation { .. } => write!(f, "ExecuteRewindAIConversation"),
            SelectAIAttachedBlock(_) => write!(f, "SelectAIAttachedBlock"),
            DragAndDropFiles(_) => write!(f, "DragAndDropFiles"),
            HyperlinkClick(hyperlink_url) => write!(f, "HyperlinkClick({hyperlink_url:?})"),

            StartFileDropTarget => write!(f, "StartFileDropTarget"),
            StopFileDropTarget => write!(f, "StopFileDropTarget"),
            RunNativeShellCompletions { buffer_text, .. } => {
                write!(f, "RunNativeShellCompletions({buffer_text:?})")
            }
            SetMarkedText {
                marked_text,
                selected_range,
            } => write!(f, "SetMarkedText {{{marked_text:?}, {selected_range:?}}}"),
            ClearMarkedText => write!(f, "ClearMarkedText"),
            HideTelemetryBannerPermanently => write!(f, "HideTelemetryBannerPermanently"),
            ShowInitializationBlock => write!(f, "ShowInitializationBlock"),
            GenerateCodebaseIndex => write!(f, "GenerateIndexForRepo"),
            ShowWarpifySettings => write!(f, "ShowWarpifySettings"),
            DeleteAttachment { index } => write!(f, "DeleteAttachment({index:?})"),
            OpenAttachmentLightbox { index } => {
                write!(f, "OpenAttachmentLightbox({index:?})")
            }
            WriteCodebaseIndex => write!(f, "PersistCodebaseIndex"),
            ToggleAutoexecuteMode => write!(f, "ToggleAutoexecuteMode"),

            AgentModeSetupSpeedbumpBanner(action) => {
                write!(f, "AgentModeSetupSpeedbumpBanner({action:?})")
            }
            ResumeConversation => write!(f, "ResumeConversation"),

            ToggleTodoPopup => write!(f, "ToggleTodoPopup"),
            CloseTodoPopup => write!(f, "CloseTodoPopup"),
            InitProject => write!(f, "InitProject"),
            AddProjectAtCurrentDirectory => write!(f, "AddProjectAtCurrentDirectory"),
            OpenAddRulePane => write!(f, "OpenAddRulePane"),
            OpenRulesPane => write!(f, "OpenRulesPane"),
            OpenBillingAndUsagePane => write!(f, "OpenBillingAndUsagePane"),
            PickRepoToOpen => write!(f, "PickRepoToOpen"),
            DismissCodeToolbeltTooltip => write!(f, "DismissCodeToolbeltTooltip"),
            SetupCloudEnvironment(_) => write!(f, "SetupCloudEnvironment"),
            SetupCloudEnvironmentAndStart(_) => write!(f, "SetupCloudEnvironmentAndStart"),
            TriggerEnvironmentSetupSelection(_) => write!(f, "TriggerEnvironmentSetupSelection"),
            OpenEnvironmentManagementPane => write!(f, "OpenEnvironmentManagementPane"),
            SummarizeConversation => write!(f, "SummarizeConversation"),
            ToggleLongRunningCommandControl => {
                write!(f, "TakeOverLongRunningCommandControlForUser")
            }
            ToggleHideCliResponses => write!(f, "ToggleHideCliResponses"),
            ExitAgentView => write!(f, "ExitAgentView"),

            CancelAmbientAgentTask => write!(f, "CancelAmbientAgentTask"),
            OpenInlineHistoryMenu => write!(f, "OpenInlineHistoryMenu"),

            AwsBedrockLoginBanner(action) => write!(f, "AwsBedrockLoginBanner({action:?})"),
            AwsCliNotInstalledBanner(action) => write!(f, "AwsCliNotInstalledBanner({action:?})"),
            ToggleUsageFooter => write!(f, "ToggleUsageFooter"),

            ToggleSessionRecording => write!(f, "ToggleSessionRecording"),
            Osc52AllowBlockedClipboardOperation => {
                write!(f, "Osc52AllowBlockedClipboardOperation")
            }
        }
    }
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
