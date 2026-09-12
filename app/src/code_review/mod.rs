pub(crate) mod comment_rendering;
pub mod comments;

/// The keystroke that submits in the code review panel. Meant to mirror the keystroke for
/// [`EditorViewEvent::CmdEnter`].
pub const CODE_REVIEW_SUBMIT_KEYSTROKE: &str = "cmdorctrl-enter";
