use std::fmt::Debug;
use std::ops::AddAssign;

use pathfinder_geometry::rect::RectF;
use warpui::AppContext;

pub mod diff_viewer;
pub mod editor;
pub mod editor_management;
pub mod inline_diff;

/// Trait to determine whether we should show the comment editor based on state held
/// by the parent of the [`CodeEditorView`].
pub trait ShowCommentEditorProvider: Debug + 'static {
    /// Returns whether the comment editor should be shown given the location of the line where
    /// the editor would be shown.
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn should_show_comment_editor(&self, editor_line_location: RectF, app: &AppContext) -> bool;
}

#[derive(Debug)]
struct NoopCommentEditorProvider;

impl ShowCommentEditorProvider for NoopCommentEditorProvider {
    fn should_show_comment_editor(&self, _editor_line_location: RectF, _app: &AppContext) -> bool {
        false
    }
}

/// The diff that results from editing a file.
#[derive(Debug, Default, Clone)]
pub struct DiffResult {
    /// The changes in unified diff format.
    pub unified_diff: String,
    /// Number of lines added.
    pub lines_added: usize,
    /// Number of lines removed.
    pub lines_removed: usize,
}

impl AddAssign<&DiffResult> for DiffResult {
    fn add_assign(&mut self, other: &DiffResult) {
        self.lines_added += other.lines_added;
        self.lines_removed += other.lines_removed;

        // There's not a standardized multi-file diff format, but concatenating the diffs is enough
        // for our needs: https://en.wikipedia.org/wiki/Diff#Extensions
        if !self.unified_diff.is_empty() && !other.unified_diff.is_empty() {
            self.unified_diff.push('\n');
        }
        self.unified_diff.push_str(&other.unified_diff);
    }
}
