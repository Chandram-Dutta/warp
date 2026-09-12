use std::collections::HashMap;
use std::path::{Path, PathBuf};

use repo_metadata::repositories::DetectedRepositories;
use warpui::{Entity, ModelContext, ModelHandle, SingletonEntity, WeakModelHandle};

use super::git_repo_model::GitRepoStatusModel;
use super::github_repo_model::GitHubRepoModel;

/// Shares local repository watchers and GitHub CLI metadata between terminals.
#[derive(Default)]
pub struct GitRepoModels {
    git_status_models: HashMap<PathBuf, WeakModelHandle<GitRepoStatusModel>>,
    github_repo_models: HashMap<PathBuf, WeakModelHandle<GitHubRepoModel>>,
}

impl GitRepoModels {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(
        &mut self,
        repo: &Path,
        ctx: &mut ModelContext<Self>,
    ) -> anyhow::Result<ModelHandle<GitRepoStatusModel>> {
        if let Some(handle) = self
            .git_status_models
            .get(repo)
            .and_then(|weak| weak.upgrade(ctx))
        {
            return Ok(handle);
        }
        let Some(repository) =
            DetectedRepositories::as_ref(ctx).get_local_watched_repo_for_path(repo, ctx)
        else {
            anyhow::bail!("No watched repository found for path: {}", repo.display());
        };
        let handle =
            ctx.add_model(|ctx| GitRepoStatusModel::new(repo.to_path_buf(), repository, ctx));
        self.git_status_models
            .insert(repo.to_path_buf(), handle.downgrade());
        Ok(handle)
    }

    pub fn subscribe_github_repo(
        &mut self,
        repo: &Path,
        ctx: &mut ModelContext<Self>,
    ) -> anyhow::Result<ModelHandle<GitHubRepoModel>> {
        if let Some(handle) = self
            .github_repo_models
            .get(repo)
            .and_then(|weak| weak.upgrade(ctx))
        {
            return Ok(handle);
        }
        let git_status = self.subscribe(repo, ctx)?;
        let handle = ctx.add_model(|ctx| GitHubRepoModel::new(repo.to_path_buf(), git_status, ctx));
        self.github_repo_models
            .insert(repo.to_path_buf(), handle.downgrade());
        Ok(handle)
    }
}

impl Entity for GitRepoModels {
    type Event = ();
}

impl SingletonEntity for GitRepoModels {}
