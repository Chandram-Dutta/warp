use std::path::Path;
use std::time::Duration;

use warp_core::send_telemetry_from_ctx;
use warp_errors::report_error;
use warpui::r#async::Timer;
use warpui::clipboard::{ClipboardContent, ImageData};
use warpui::{AppContext, ViewContext};

use super::TerminalView;
use crate::server::telemetry::{CLISubagentControlState, TelemetryEvent};
use crate::settings::CompiledCommandsForCodingAgentToolbar;
pub use crate::terminal::CLIAgent;
use crate::terminal::TerminalModel;
use crate::terminal::cli_agent::CLIAgentRuntimeExt as _;
use crate::util::image::{MAX_IMAGE_SIZE_BYTES_FOR_CLI_AGENT, MIME_SNIFF_BYTES, infer_mime_type};

/// Longer delay between clipboard image pastes (Ctrl+V) to CLI agents.
/// The CLI agent needs time to read from the system clipboard before
/// we overwrite it with the next image.
const CLI_AGENT_IMAGE_PASTE_DELAY: Duration = Duration::from_millis(300);

/// Bytes that simulate a "paste image from clipboard" keystroke for the
/// foreground CLI agent. `0x16` is `Ctrl+V` (SYN); on Windows Claude Code
/// listens for `Alt+V` (`ESC` + `'v'`) instead. Mirrored from the equivalent
/// branch in `TerminalView::paste`.
fn cli_agent_paste_keystroke_bytes() -> Vec<u8> {
    if cfg!(windows) {
        vec![0x1b, b'v']
    } else {
        vec![0x16]
    }
}

impl TerminalView {
    /// Returns the detected CLI agent for the active block's command, if any.
    ///
    /// This method resolves aliases before detecting the CLI agent. For example,
    /// if a user has aliased `foo` to `claude`, running `foo` will detect Claude.
    /// Falls back to user-configured toolbar command patterns, returning the
    /// assigned agent (or `CLIAgent::Unknown` for unassigned patterns).
    ///
    /// The second tuple element is the custom command prefix (the first word of
    /// the command), present only when the agent was resolved via a custom
    /// toolbar command pattern rather than native detection.
    pub(super) fn detect_cli_agent_from_model(
        &self,
        model: &TerminalModel,
        ctx: &AppContext,
    ) -> Option<(CLIAgent, Option<String>)> {
        let active_block = model.block_list().active_block();

        if !active_block.is_active_and_long_running() {
            return None;
        }

        let command = active_block.command_with_secrets_obfuscated(false);

        let detected = self.active_block_session_id().and_then(|session_id| {
            self.sessions.read(ctx, |sessions, _| {
                let session = sessions.get(session_id)?;
                CLIAgent::detect(
                    &command,
                    Some(session.shell_family().escape_char()),
                    Some(session.aliases()),
                    ctx,
                )
            })
        });

        if let Some(agent) = detected {
            return Some((agent, None));
        }

        CompiledCommandsForCodingAgentToolbar::matched_agent(ctx, &command).map(|agent| {
            let prefix = command.split_whitespace().next().map(str::to_owned);
            (agent, prefix)
        })
    }

    /// Returns control of an Agent-tagged command to the terminal input.
    pub(super) fn tag_out_agent_for_user_long_running_command(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        if !self
            .model
            .lock()
            .block_list()
            .active_block()
            .is_agent_tagged_in()
        {
            return;
        }

        self.model
            .lock()
            .block_list_mut()
            .active_block_mut()
            .set_is_agent_tagged_in(false);

        self.input.update(ctx, |input, ctx| {
            input.set_input_mode_terminal(false, ctx);
        });
        self.redetermine_terminal_focus(ctx);

        ctx.notify();

        let model = self.model.lock();
        let active_block = model.block_list().active_block();
        let conversation_id = active_block.ai_conversation_id();
        let block_id = active_block.id().clone();
        send_telemetry_from_ctx!(
            TelemetryEvent::CLISubagentControlStateChanged {
                conversation_id,
                block_id,
                control_state: CLISubagentControlState::AgentTaggedOut,
            },
            ctx
        );
    }

    /// Mirrors the CLI-agent Cmd+V image-paste path in `TerminalView::paste`
    /// for dropped image files: reads each file, writes its bytes to the
    /// system clipboard as image data, and sends the agent's paste keystroke
    /// to the PTY so the agent reads the image directly. This produces the
    /// same outcome as if the user had copied the image to their clipboard
    /// and pressed Cmd+V over the agent's TUI.
    pub(super) fn paste_dropped_images_to_cli_agent(
        &mut self,
        image_filepaths: Vec<String>,
        ctx: &mut ViewContext<Self>,
    ) {
        if image_filepaths.is_empty() {
            return;
        }
        let spawner = ctx.spawner();
        ctx.spawn(
            async move {
                for path_str in image_filepaths {
                    // Stat first so a multi-GB drop doesn't load into memory
                    // before we reject it. CLI agents handle their own
                    // compression, so the cap only exists to bound memory use.
                    match async_fs::metadata(&path_str).await {
                        Ok(meta) if (meta.len() as usize) > MAX_IMAGE_SIZE_BYTES_FOR_CLI_AGENT => {
                            let filename = Path::new(&path_str)
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path_str.clone());
                            let limit_mb = MAX_IMAGE_SIZE_BYTES_FOR_CLI_AGENT / 1_000_000;
                            let msg = format!(
                                "{filename} is too large to send to the agent (limit {limit_mb}MB)."
                            );
                            let _ = spawner
                                .spawn(move |me, ctx| {
                                    me.show_error_toast(msg, ctx);
                                })
                                .await;
                            continue;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            report_error!(
                                anyhow::Error::new(e).context("Failed to stat dropped image"),
                                extra: { "path" => %path_str }
                            );
                            continue;
                        }
                    }

                    let bytes = match async_fs::read(&path_str).await {
                        Ok(b) => b,
                        Err(e) => {
                            report_error!(
                                anyhow::Error::new(e).context("Failed to read dropped image"),
                                extra: { "path" => %path_str }
                            );
                            continue;
                        }
                    };
                    let path = Path::new(&path_str);
                    let filename = path.file_name().map(|n| n.to_string_lossy().into_owned());
                    let sniff_len = bytes.len().min(MIME_SNIFF_BYTES);
                    let mime_type = infer_mime_type(path, &bytes[..sniff_len]);

                    // Hop back to the view to write the clipboard + paste
                    // keystroke. Bail if the CLI agent session disappeared,
                    // OR if the agent's long-running block exited while we
                    // were reading off-thread — without that second check
                    // the paste byte would leak into the shell after the
                    // agent quit, since the session entry can outlive its
                    // foreground block.
                    let should_continue = spawner
                        .spawn(move |me, ctx| {
                            if !me.has_active_cli_agent_session(ctx) {
                                return false;
                            }
                            let still_long_running = me
                                .model
                                .lock()
                                .block_list()
                                .active_block()
                                .is_active_and_long_running();
                            if !still_long_running {
                                return false;
                            }
                            ctx.clipboard().write(ClipboardContent {
                                images: Some(vec![ImageData {
                                    data: bytes,
                                    mime_type,
                                    filename,
                                }]),
                                ..Default::default()
                            });
                            me.write_user_bytes_to_pty(cli_agent_paste_keystroke_bytes(), ctx);
                            true
                        })
                        .await;

                    if !matches!(should_continue, Ok(true)) {
                        return;
                    }

                    // Give the CLI agent time to read from the clipboard
                    // before we overwrite it with the next image.
                    Timer::after(CLI_AGENT_IMAGE_PASTE_DELAY).await;
                }
            },
            |_, _, _| {},
        );
    }
}
