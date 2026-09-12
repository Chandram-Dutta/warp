use clap::{Args, Subcommand};

/// Model-related subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ModelCommand {
    /// List available models for the Warp Agent harness. For third party harnesses,
    /// consult third party harness docs for available models.
    List,
}

/// Shared CLI args for selecting a base model.
#[derive(Debug, Clone, Args, Default)]
pub struct ModelArgs {
    /// Override the base model used by this command.
    ///
    /// For the default Oz harness, use `oz model list` to see available model IDs.
    ///
    /// For third-party harnesses (`--harness claude` or `--harness codex`), this
    /// sets the harness-specific model. The value is passed directly to the harness.
    /// See third party harness docs for list of accepted values.
    #[arg(long = "model", value_name = "MODEL_ID")]
    pub model: Option<String>,
}
