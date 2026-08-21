use std::sync::Arc;

pub use ai::api_keys::AwsCredentials;
use ai::api_keys::{ApiKeyManager, AwsCredentialsState};
use futures::future::BoxFuture;
use parking_lot::FairMutex;
use warpui::{ModelContext, ModelHandle};

use crate::terminal::model::terminal_model::TerminalModel;
use crate::terminal::model_events::ModelEventDispatcher;

#[derive(Debug, Clone, thiserror::Error)]
#[error("AWS credentials are unavailable without Warp Agent support")]
pub struct LoadAwsCredentialsError;

pub async fn load_aws_credentials_from_sdk(
    profile: &str,
) -> Result<AwsCredentials, LoadAwsCredentialsError> {
    let _ = profile;
    Err(LoadAwsCredentialsError)
}

pub(crate) fn aws_role_session_name(run_id: &str) -> String {
    format!("Oz_Run_{run_id}")
}

pub trait AwsCredentialRefresher {
    fn register_model_event_dispatcher(
        &mut self,
        model_events: &ModelHandle<ModelEventDispatcher>,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        ctx: &mut ModelContext<Self>,
    ) where
        Self: Sized;

    fn subscribe_to_settings_changes(&mut self, ctx: &mut ModelContext<Self>)
    where
        Self: Sized;
}

impl AwsCredentialRefresher for ApiKeyManager {
    fn register_model_event_dispatcher(
        &mut self,
        model_events: &ModelHandle<ModelEventDispatcher>,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        ctx: &mut ModelContext<Self>,
    ) {
        let _ = (self, model_events, terminal_model, ctx);
    }

    fn subscribe_to_settings_changes(&mut self, ctx: &mut ModelContext<Self>) {
        let _ = (self, ctx);
    }
}

pub(crate) fn refresh_aws_credentials(
    manager: &mut ApiKeyManager,
    ctx: &mut ModelContext<ApiKeyManager>,
) -> BoxFuture<'static, Result<(), String>> {
    manager.set_aws_credentials_state(AwsCredentialsState::Disabled, ctx);
    Box::pin(async { Ok(()) })
}
