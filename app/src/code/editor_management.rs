use std::path::{Path, PathBuf};

use ai::skills::SkillReference;
use serde::{Deserialize, Serialize};
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::path::LineAndColumnArg;

use crate::ai::agent::AIAgentActionId;
use crate::ai::skills::SkillOpenOrigin;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum CodeSource {
    /// Opened from file links.
    Link {
        path: PathBuf,
        range_start: Option<LineAndColumnArg>,
        range_end: Option<LineAndColumnArg>,
    },
    /// Opened from an active AI agent conversation.
    AIAction { id: AIAgentActionId },
    /// Opened from project rules (WARP.md) file.
    ProjectRules { location: LocalOrRemotePath },
    /// Opened from file tree (local or remote).
    FileTree { location: LocalOrRemotePath },
    /// Opened from command palette file search (local or remote).
    CommandPalette { location: LocalOrRemotePath },
    /// Opened from macOS Finder via "Open With".
    Finder { path: PathBuf },
    /// Opened from a skill.
    Skill {
        reference: SkillReference,
        location: LocalOrRemotePath,
        origin: SkillOpenOrigin,
    },
}

impl CodeSource {
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            Self::AIAction { .. } => None,
            Self::FileTree { location, .. } | Self::CommandPalette { location, .. } => {
                match location {
                    LocalOrRemotePath::Local(path) => Some(path.clone()),
                    LocalOrRemotePath::Remote(_) => None,
                }
            }
            Self::Link { path, .. } | Self::Finder { path } => Some(path.clone()),
            Self::ProjectRules { location } | Self::Skill { location, .. } => {
                location.to_local_path().map(Path::to_path_buf)
            }
        }
    }

    /// Returns the `LocalOrRemotePath` for file tree sources.
    pub fn file_location(&self) -> Option<&LocalOrRemotePath> {
        match self {
            Self::FileTree { location } | Self::CommandPalette { location } => Some(location),
            _ => None,
        }
    }

    /// Returns the `LocalOrRemotePath` for any source that has a backing file.
    ///
    /// Unlike `path()` (which only returns local paths) and `file_location()`
    /// (which only covers `FileTree`), this covers every variant that maps to
    /// a file — local or remote.
    pub fn location(&self) -> Option<LocalOrRemotePath> {
        match self {
            Self::AIAction { .. } => None,
            Self::FileTree { location } | Self::CommandPalette { location } => {
                Some(location.clone())
            }
            Self::Link { path, .. } | Self::Finder { path } => {
                Some(LocalOrRemotePath::Local(path.clone()))
            }
            Self::ProjectRules { location } | Self::Skill { location, .. } => {
                Some(location.clone())
            }
        }
    }

    /// Returns true if this is a bundled skill that should be read-only.
    pub fn is_bundled_skill(&self) -> bool {
        matches!(
            self,
            Self::Skill {
                reference: SkillReference::BundledSkillId(_),
                ..
            }
        )
    }

    pub fn omit_line_col(&self) -> CodeSource {
        if let CodeSource::Link { path, .. } = self {
            CodeSource::Link {
                path: path.clone(),
                range_start: None,
                range_end: None,
            }
        } else {
            self.clone()
        }
    }

    /// Returns the variant name as a string for telemetry purposes.
    pub fn telemetry_source_name(&self) -> &'static str {
        match self {
            Self::Link { .. } => "link",
            Self::AIAction { .. } => "ai_action",
            Self::ProjectRules { .. } => "project_rules",
            Self::FileTree {
                location: LocalOrRemotePath::Remote(_),
            } => "remote_file_tree",
            Self::FileTree { .. } => "file_tree",
            Self::CommandPalette {
                location: LocalOrRemotePath::Remote(_),
            } => "remote_command_palette",
            Self::CommandPalette { .. } => "command_palette",
            Self::Finder { .. } => "finder",
            Self::Skill { .. } => "skill",
        }
    }

    /// Returns `true` if this source should be restored across app restarts.
    ///
    /// `AIAction` is ephemeral (tied to a live conversation) and should not
    /// be restored.
    pub fn is_restorable(&self) -> bool {
        !matches!(
            self,
            Self::AIAction { .. }
                | Self::FileTree {
                    location: LocalOrRemotePath::Remote(_),
                }
                | Self::CommandPalette {
                    location: LocalOrRemotePath::Remote(_),
                }
                | Self::ProjectRules {
                    location: LocalOrRemotePath::Remote(_),
                }
                | Self::Skill {
                    location: LocalOrRemotePath::Remote(_),
                    ..
                }
        )
    }
}
