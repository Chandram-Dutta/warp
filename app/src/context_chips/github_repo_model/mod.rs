mod local;
pub use local::LocalGitHubRepoModel as GitHubRepoModel;

#[derive(Debug)]
pub enum GitHubRepoEvent {
    PrInfoChanged,
}
