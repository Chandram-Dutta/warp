use anyhow::Result;
use settings::macros::define_settings_group;
use settings::{SupportedPlatforms, SyncToCloud};
use warp_local_ai::{
    LocalAINextCommandConfig, LocalAIProvider, ProviderConnection, ProviderCredential,
};
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};
use warpui_extras::secure_storage::{self, AppContextExt as _};

const OPENAI_CREDENTIAL_KEY: &str = "WarpLocalAINextCommandOpenAICompatibleCredential";
const OPENROUTER_CREDENTIAL_KEY: &str = "WarpLocalAINextCommandOpenRouterCredential";
const OLLAMA_CREDENTIAL_KEY: &str = "WarpLocalAINextCommandOllamaCredential";

define_settings_group!(LocalAISettings, settings: [
    next_command: LocalAINextCommandSetting {
        type: LocalAINextCommandConfig,
        default: LocalAINextCommandConfig::default(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        surface: settings::SettingSurfaces::GUI,
        private: false,
        storage_key: "LocalAINextCommandConfig",
        toml_path: "local_ai.next_command",
        max_table_depth: 3,
        description: "Local Next Command provider configuration.",
    },
]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalAICredentialsEvent {
    Changed(LocalAIProvider),
}

pub struct LocalAICredentials {
    openai_compatible: Option<String>,
    open_router: Option<String>,
    ollama: Option<String>,
}

impl LocalAICredentials {
    pub fn register(ctx: &mut AppContext) {
        let credentials = Self {
            openai_compatible: Self::read(LocalAIProvider::OpenAICompatible, ctx),
            open_router: Self::read(LocalAIProvider::OpenRouter, ctx),
            ollama: Self::read(LocalAIProvider::Ollama, ctx),
        };
        ctx.add_singleton_model(|_| credentials);
    }

    fn storage_key(provider: LocalAIProvider) -> &'static str {
        match provider {
            LocalAIProvider::OpenAICompatible => OPENAI_CREDENTIAL_KEY,
            LocalAIProvider::OpenRouter => OPENROUTER_CREDENTIAL_KEY,
            LocalAIProvider::Ollama => OLLAMA_CREDENTIAL_KEY,
        }
    }

    fn read(provider: LocalAIProvider, ctx: &AppContext) -> Option<String> {
        match ctx.secure_storage().read_value(Self::storage_key(provider)) {
            Ok(value) if !value.trim().is_empty() => Some(value),
            Ok(_) | Err(secure_storage::Error::NotFound) => None,
            Err(_) => {
                log::error!("Failed to read local AI provider credential from secure storage");
                None
            }
        }
    }

    pub fn credential(&self, provider: LocalAIProvider) -> Option<&str> {
        match provider {
            LocalAIProvider::OpenAICompatible => self.openai_compatible.as_deref(),
            LocalAIProvider::OpenRouter => self.open_router.as_deref(),
            LocalAIProvider::Ollama => self.ollama.as_deref(),
        }
    }

    pub fn set_credential(
        &mut self,
        provider: LocalAIProvider,
        credential: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        let credential = credential.and_then(|value| {
            let value = value.trim().to_owned();
            (!value.is_empty()).then_some(value)
        });
        match credential.as_deref() {
            Some(value) => ctx
                .secure_storage()
                .write_value_with_owner_only_fallback(Self::storage_key(provider), value)?,
            None => match ctx
                .secure_storage()
                .remove_value(Self::storage_key(provider))
            {
                Ok(()) | Err(secure_storage::Error::NotFound) => {}
                Err(error) => return Err(error.into()),
            },
        }

        *match provider {
            LocalAIProvider::OpenAICompatible => &mut self.openai_compatible,
            LocalAIProvider::OpenRouter => &mut self.open_router,
            LocalAIProvider::Ollama => &mut self.ollama,
        } = credential;
        ctx.emit(LocalAICredentialsEvent::Changed(provider));
        Ok(())
    }
}

impl Entity for LocalAICredentials {
    type Event = LocalAICredentialsEvent;
}

impl SingletonEntity for LocalAICredentials {}

impl LocalAISettings {
    pub fn is_next_command_enabled(&self, app: &AppContext) -> bool {
        self.provider_connection(app).is_some()
    }

    pub fn provider_connection(&self, app: &AppContext) -> Option<ProviderConnection> {
        if !self.next_command.enabled {
            return None;
        }
        let provider = self.next_command.active_provider;
        let provider_config = self.next_command.provider(provider);
        if provider_config.endpoint.trim().is_empty() || provider_config.model.trim().is_empty() {
            return None;
        }
        let credential = LocalAICredentials::as_ref(app)
            .credential(provider)
            .and_then(|value| ProviderCredential::new(value.to_owned()));
        if provider.requires_api_key() && credential.is_none() {
            return None;
        }
        Some(ProviderConnection::new(
            provider,
            provider_config.endpoint.clone(),
            provider_config.model.clone(),
            credential,
        ))
    }
}

#[cfg(test)]
#[path = "local_ai_tests.rs"]
mod tests;
