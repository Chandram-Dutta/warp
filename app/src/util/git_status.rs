//! Working-tree status and line counts.
//!
//! Portions adapted from GitHub Desktop, Copyright (c) GitHub, Inc., under the MIT license.
//! See GITHUB-DESKTOP-LICENSE in this directory.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek};
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use warp_util::git::run_git_command;

use super::git::FileChangeEntry;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GitFileStatus {
    New,
    Modified,
    Deleted,
    Renamed { old_path: String },
    Copied { old_path: String },
    Untracked,
    Conflicted,
}

impl GitFileStatus {
    pub fn is_renamed(&self) -> bool {
        matches!(self, Self::Renamed { .. })
    }

    pub fn is_new_file(&self) -> bool {
        matches!(self, Self::New | Self::Untracked)
    }
}

impl TryFrom<&str> for GitFileStatus {
    type Error = anyhow::Error;

    fn try_from(status_code: &str) -> Result<Self> {
        match status_code {
            ".M" | "M." | "MM" => Ok(Self::Modified),
            ".A" | "A." | "AM" => Ok(Self::New),
            ".D" | "D." | "AD" => Ok(Self::Deleted),
            _ => Ok(Self::Modified),
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct DiffStats {
    pub files_changed: usize,
    pub total_additions: usize,
    pub total_deletions: usize,
}

impl DiffStats {
    pub(crate) fn has_no_changes(&self) -> bool {
        self.files_changed == 0
    }
}

#[derive(Clone, Default, Debug)]
pub struct DiffMetadataAgainstBase {
    pub aggregate_stats: DiffStats,
    pub files: Vec<FileChangeEntry>,
}

impl DiffMetadataAgainstBase {
    pub fn is_dirty(&self) -> bool {
        !self.aggregate_stats.has_no_changes()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GitNumStatMetadata {
    pub lines_added: usize,
    pub lines_removed: usize,
    pub is_binary_file: bool,
}

/// Parses NUL-separated porcelain v2 status, preserving whitespace in paths.
pub(crate) fn parse_git_status(status_output: &str) -> Result<Vec<(String, GitFileStatus)>> {
    let mut files = Vec::new();
    let tokens: Vec<&str> = status_output.split('\0').collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        if token.is_empty() || token.starts_with("# ") {
            i += 1;
            continue;
        }
        match token.chars().next().unwrap_or('?') {
            '1' => {
                let parts: Vec<&str> = token.splitn(9, ' ').collect();
                if parts.len() >= 9 {
                    files.push((parts[8].to_string(), GitFileStatus::try_from(parts[1])?));
                }
            }
            '2' => {
                let parts: Vec<&str> = token.splitn(10, ' ').collect();
                if parts.len() >= 10 {
                    let old_path = tokens.get(i + 1).copied().unwrap_or_default().to_string();
                    let status = if parts[1].starts_with('R') {
                        GitFileStatus::Renamed { old_path }
                    } else if parts[1].starts_with('C') {
                        GitFileStatus::Copied { old_path }
                    } else {
                        GitFileStatus::try_from(parts[1])?
                    };
                    files.push((parts[9].to_string(), status));
                    i += 1;
                }
            }
            'u' => {
                let parts: Vec<&str> = token.splitn(11, ' ').collect();
                if parts.len() >= 11 {
                    files.push((parts[10].to_string(), GitFileStatus::Conflicted));
                }
            }
            '?' => {
                if token.len() > 2 {
                    files.push((token[2..].to_string(), GitFileStatus::Untracked));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(files)
}

/// Returns `None` for binary files or files larger than 20 MB.
pub(crate) async fn num_lines_in_file_if_non_binary(file_path: &Path) -> Result<Option<usize>> {
    const MAX_FILE_SIZE_TO_READ_LINES: u64 = 20 * 1000 * 1000;
    let mut file = File::open(file_path)?;
    let mut buffer = vec![0; 1024];
    let n = file.read(&mut buffer)?;
    buffer.truncate(n);
    if warp_util::file_type::is_buffer_binary(&buffer) {
        return Ok(None);
    }
    file.rewind()?;
    if file.metadata()?.len() > MAX_FILE_SIZE_TO_READ_LINES {
        return Ok(None);
    }
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut count = 0;
    loop {
        let len = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            count += bytecount::count(buf, b'\n');
            buf.len()
        };
        reader.consume(len);
        // Allow cancellation between chunks without loading the entire file into memory.
        futures_lite::future::yield_now().await;
    }
    Ok(Some(count))
}

pub(crate) async fn get_diff_metadata_using_numstat(
    repo_path: &Path,
    commit: &str,
) -> Result<HashMap<String, GitNumStatMetadata>> {
    let numstat_output = match run_git_command(repo_path, &["diff", "--numstat", commit]).await {
        Ok(output) => output,
        Err(_) => return Ok(HashMap::new()),
    };
    let mut diff_metadata = HashMap::new();
    for line in numstat_output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            diff_metadata.insert(
                parts[2].to_string(),
                GitNumStatMetadata {
                    lines_added: parts[0].parse().unwrap_or(0),
                    lines_removed: parts[1].parse().unwrap_or(0),
                    is_binary_file: parts[0] == "-" && parts[1] == "-",
                },
            );
        }
    }
    Ok(diff_metadata)
}

/// Counts uncommitted changes against HEAD, including untracked text files.
pub(crate) async fn diff_metadata_against_head(
    repo_path: &Path,
) -> Result<DiffMetadataAgainstBase> {
    let status_output = run_git_command(
        repo_path,
        &[
            "--no-optional-locks",
            "status",
            "--untracked-files=all",
            "--branch",
            "--porcelain=2",
            "-z",
        ],
    )
    .await?;
    let changed_files = parse_git_status(&status_output)?;
    let num_stat_metadata = get_diff_metadata_using_numstat(repo_path, "HEAD").await?;
    let mut total_additions = 0;
    let mut total_deletions = 0;
    let mut files = Vec::with_capacity(changed_files.len());
    for (file_path, status) in &changed_files {
        let (additions, deletions) = if let Some(metadata) = num_stat_metadata.get(file_path) {
            (metadata.lines_added, metadata.lines_removed)
        } else if matches!(status, GitFileStatus::Untracked) {
            // An unreadable entry or nested repository must not hide the remaining changes.
            let num_lines = num_lines_in_file_if_non_binary(&repo_path.join(file_path))
                .await
                .unwrap_or_else(|err| {
                    log::debug!("Could not count lines for untracked entry {file_path}: {err}");
                    None
                });
            (num_lines.unwrap_or(0), 0)
        } else {
            (0, 0)
        };
        total_additions += additions;
        total_deletions += deletions;
        files.push(FileChangeEntry {
            path: file_path.clone(),
            additions,
            deletions,
        });
    }
    Ok(DiffMetadataAgainstBase {
        aggregate_stats: DiffStats {
            files_changed: changed_files.len(),
            total_additions,
            total_deletions,
        },
        files,
    })
}

#[cfg(test)]
#[path = "git_status_tests.rs"]
mod tests;
