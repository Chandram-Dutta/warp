use std::any::Any;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use parking_lot::FairMutex;
#[cfg(windows)]
use warpui::ModelHandle;
use warpui::{AppContext, ViewHandle, WindowId};

use super::terminal_manager::{TerminalManager, TerminalSurfaceInit};
use crate::ai::blocklist::InputConfig;
use crate::context_chips::current_prompt::CurrentPrompt;
use crate::context_chips::prompt_type::PromptType;
use crate::pane_group::TerminalViewResources;
use crate::persistence::ModelEvent;
use crate::terminal::{TerminalManager as TerminalManagerTrait, TerminalModel, TerminalView};

/// Configuration for constructing the GUI terminal surface.
pub(crate) struct TerminalViewSurfaceConfig {
    pub(crate) resources: TerminalViewResources,
    pub(crate) model_event_sender: Option<SyncSender<ModelEvent>>,
    pub(crate) window_id: WindowId,
    pub(crate) initial_input_config: Option<InputConfig>,
}

/// Creates the GUI terminal surface.
pub(crate) fn create_terminal_view_surface(
    config: TerminalViewSurfaceConfig,
    surface_init: TerminalSurfaceInit,
    ctx: &mut AppContext,
) -> ViewHandle<TerminalView> {
    let TerminalSurfaceInit {
        wakeups_rx,
        model_events,
        model,
        sessions,
        size_info,
        colors,
        inactive_pty_reads_rx,
    } = surface_init;
    let TerminalViewSurfaceConfig {
        resources,
        model_event_sender,
        window_id,
        initial_input_config,
    } = config;
    let current_prompt = ctx.add_model(|ctx| {
        CurrentPrompt::new_with_model_events(
            sessions.clone(),
            Some((&model_events, model.clone())),
            ctx,
        )
    });
    let prompt_type = ctx.add_model(|ctx| PromptType::new_dynamic(current_prompt, ctx));
    ctx.add_typed_action_view(window_id, |ctx| {
        TerminalView::new(
            resources,
            wakeups_rx,
            model_events,
            model,
            sessions,
            size_info,
            colors,
            model_event_sender,
            prompt_type,
            initial_input_config,
            Some(inactive_pty_reads_rx),
            false,
            ctx,
        )
    })
}

impl TerminalManager<TerminalView> {
    /// Returns the PTY process id, for integration tests.
    #[cfg(feature = "integration_tests")]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}

impl TerminalManagerTrait for TerminalManager<TerminalView> {
    fn model(&self) -> Arc<FairMutex<TerminalModel>> {
        self.model.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Send a Shutdown event to each PTY's event loop and waits for the
/// event loop to terminate.
/// This is needed on Windows to ensure all OpenConsole processes are
/// cleaned up before the main thread exits.
#[cfg(windows)]
pub fn shutdown_all_pty_event_loops(ctx: &mut AppContext) {
    let terminal_managers: Vec<ModelHandle<Box<dyn TerminalManagerTrait>>> = ctx.models_of_type();
    terminal_managers.into_iter().for_each(|terminal_manager| {
        terminal_manager.update(ctx, |terminal_manager, _ctx| {
            if let Some(manager) = terminal_manager
                .as_any_mut()
                .downcast_mut::<TerminalManager<TerminalView>>()
            {
                manager.shutdown_event_loop();
            }
        })
    })
}
