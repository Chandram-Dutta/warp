//! Command-only context for local Next Command suggestions.

use std::sync::Arc;

use parking_lot::FairMutex;
use warp_local_ai::{NextCommandContext, RecentCommand};

use crate::terminal::TerminalModel;
use crate::terminal::model::block::BlockState;

#[derive(Clone)]
pub struct HistoryContext {
    pub previous_commands: Vec<crate::persistence::model::Command>,
    pub next_command: crate::persistence::model::Command,
}

#[derive(Clone)]
pub struct CommandContext {
    pub shell_name: String,
    pub shell_version: Option<String>,
    pub history_contexts: Vec<HistoryContext>,
    pub recent_commands: Vec<RecentCommand>,
}

impl CommandContext {
    pub fn provider_context(&self, prefix: Option<&str>) -> NextCommandContext {
        let working_directory = self
            .recent_commands
            .last()
            .and_then(|command| command.working_directory.clone());
        NextCommandContext::new(
            self.shell_name.clone(),
            self.shell_version.clone(),
            working_directory,
            prefix.unwrap_or_default(),
            self.recent_commands.clone(),
        )
    }
}

pub fn get_recent_commands(
    terminal_model: Arc<FairMutex<TerminalModel>>,
    number_of_blocks: usize,
) -> Vec<RecentCommand> {
    let model = terminal_model.lock();
    let blocks = model.block_list().blocks();
    let first_block = blocks.len().saturating_sub(number_of_blocks);

    blocks[first_block..]
        .iter()
        .filter(|block| {
            block.state() == BlockState::DoneWithExecution && !block.is_in_band_command_block()
        })
        .map(|block| RecentCommand {
            command: block.command_with_secrets_obfuscated(false),
            working_directory: block.pwd().cloned(),
            exit_code: block.exit_code().value() as i64,
        })
        .collect()
}

#[cfg(test)]
#[path = "next_command_tests.rs"]
mod tests;
