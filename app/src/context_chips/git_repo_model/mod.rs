mod local;
pub use local::LocalGitRepoStatusModel as GitRepoStatusModel;

pub use super::git_repo_models::GitRepoModels;
use crate::context_chips::display_chip::GitBranchTrackingStatus;
use crate::util::git_status::DiffStats;

#[derive(Debug, Clone)]
pub struct GitStatusMetadata {
    pub current_branch_name: String,
    pub main_branch_name: String,
    pub stats_against_head: DiffStats,
    pub branch_tracking_status: GitBranchTrackingStatus,
}

#[derive(Debug)]
pub enum GitRepoStatusEvent {
    MetadataChanged,
}
