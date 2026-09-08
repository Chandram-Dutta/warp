use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_compat::CompatExt as _;
use async_trait::async_trait;
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::{Host, Url};

const MAX_COMMAND_CONTEXTS: usize = 5;
const MAX_COMMAND_CHARS: usize = 2_000;
const MAX_DIRECTORY_CHARS: usize = 1_024;
const MAX_PREFIX_CHARS: usize = 2_000;
const MAX_SHELL_FIELD_CHARS: usize = 100;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    JsonSchema,
    PartialEq,
    Serialize,
    settings_value::SettingsValue,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum LocalAIProvider {
    OpenAICompatible,
    OpenRouter,
    #[default]
    Ollama,
}

impl LocalAIProvider {
    pub const ALL: [Self; 3] = [Self::OpenAICompatible, Self::OpenRouter, Self::Ollama];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAICompatible => "OpenAI-compatible",
            Self::OpenRouter => "OpenRouter",
            Self::Ollama => "Ollama-compatible",
        }
    }

    pub fn requires_api_key(self) -> bool {
        matches!(self, Self::OpenRouter)
    }
}

#[derive(
    Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, settings_value::SettingsValue,
)]
pub struct ProviderConfig {
    pub endpoint: String,
    pub model: String,
}

impl ProviderConfig {
    fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            model: String::new(),
        }
    }
}

#[derive(
    Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, settings_value::SettingsValue,
)]
pub struct LocalAINextCommandConfig {
    pub enabled: bool,
    pub active_provider: LocalAIProvider,
    pub openai_compatible: ProviderConfig,
    pub open_router: ProviderConfig,
    pub ollama: ProviderConfig,
}

impl Default for LocalAINextCommandConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            active_provider: LocalAIProvider::Ollama,
            openai_compatible: ProviderConfig::new("https://api.openai.com/v1"),
            open_router: ProviderConfig::new("https://openrouter.ai/api/v1"),
            ollama: ProviderConfig::new("http://localhost:11434"),
        }
    }
}

impl LocalAINextCommandConfig {
    pub fn provider(&self, provider: LocalAIProvider) -> &ProviderConfig {
        match provider {
            LocalAIProvider::OpenAICompatible => &self.openai_compatible,
            LocalAIProvider::OpenRouter => &self.open_router,
            LocalAIProvider::Ollama => &self.ollama,
        }
    }

    pub fn provider_mut(&mut self, provider: LocalAIProvider) -> &mut ProviderConfig {
        match provider {
            LocalAIProvider::OpenAICompatible => &mut self.openai_compatible,
            LocalAIProvider::OpenRouter => &mut self.open_router,
            LocalAIProvider::Ollama => &mut self.ollama,
        }
    }
}

#[derive(Clone)]
pub struct ProviderCredential(String);

impl ProviderCredential {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty()).then_some(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProviderCredential([REDACTED])")
    }
}

#[derive(Clone, Debug)]
pub struct ProviderConnection {
    pub provider: LocalAIProvider,
    pub endpoint: String,
    pub model: String,
    pub credential: Option<ProviderCredential>,
}

impl ProviderConnection {
    pub fn new(
        provider: LocalAIProvider,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        credential: Option<ProviderCredential>,
    ) -> Self {
        Self {
            provider,
            endpoint: endpoint.into(),
            model: model.into(),
            credential,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShellContext {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentCommand {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    pub exit_code: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NextCommandContext {
    pub shell: ShellContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    pub command_prefix: String,
    pub recent_commands: Vec<RecentCommand>,
}

impl NextCommandContext {
    pub fn new(
        shell_name: impl Into<String>,
        shell_version: Option<String>,
        working_directory: Option<String>,
        command_prefix: impl Into<String>,
        recent_commands: Vec<RecentCommand>,
    ) -> Self {
        Self {
            shell: ShellContext {
                name: truncate(shell_name.into(), MAX_SHELL_FIELD_CHARS),
                version: shell_version.map(|value| truncate(value, MAX_SHELL_FIELD_CHARS)),
            },
            working_directory: working_directory.map(|value| truncate(value, MAX_DIRECTORY_CHARS)),
            command_prefix: truncate(command_prefix.into(), MAX_PREFIX_CHARS),
            recent_commands: recent_commands
                .into_iter()
                .rev()
                .take(MAX_COMMAND_CONTEXTS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(|command| RecentCommand {
                    command: truncate(command.command, MAX_COMMAND_CHARS),
                    working_directory: command
                        .working_directory
                        .map(|value| truncate(value, MAX_DIRECTORY_CHARS)),
                    exit_code: command.exit_code,
                })
                .collect(),
        }
    }
}

fn truncate(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    value.chars().take(max_chars).collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NextCommandSuggestion {
    DisplayOnly { command: String },
}

impl NextCommandSuggestion {
    pub fn display_only_command(&self) -> &str {
        match self {
            Self::DisplayOnly { command } => command,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAIError {
    #[error("the selected provider endpoint is invalid")]
    InvalidEndpoint,
    #[error("the selected provider model is not configured")]
    MissingModel,
    #[error("the selected provider requires an API key")]
    MissingCredential,
    #[error("the provider request failed")]
    Transport(#[source] reqwest::Error),
    #[error("the provider returned HTTP status {0}")]
    HttpStatus(StatusCode),
    #[error("the provider response did not contain a command suggestion")]
    InvalidResponse,
}

pub struct ProviderHttpRequest {
    pub url: Url,
    pub bearer_token: Option<ProviderCredential>,
    pub payload: Value,
}

impl fmt::Debug for ProviderHttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderHttpRequest")
            .field("url", &self.url)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("payload", &self.payload)
            .finish()
    }
}

pub struct ProviderHttpResponse {
    pub status: StatusCode,
    pub payload: Value,
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait ProviderTransport: Send + Sync {
    async fn send(
        &self,
        request: ProviderHttpRequest,
    ) -> Result<ProviderHttpResponse, LocalAIError>;
}

pub struct DirectHttpTransport {
    client: OnceLock<reqwest::Client>,
}

impl Default for DirectHttpTransport {
    fn default() -> Self {
        Self {
            client: OnceLock::new(),
        }
    }
}

impl DirectHttpTransport {
    fn client(&self) -> &reqwest::Client {
        self.client.get_or_init(|| {
            #[cfg(not(target_family = "wasm"))]
            let builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
            #[cfg(target_family = "wasm")]
            let builder = reqwest::Client::builder();

            builder
                .build()
                .expect("should not fail to create local AI client")
        })
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl ProviderTransport for DirectHttpTransport {
    async fn send(
        &self,
        request: ProviderHttpRequest,
    ) -> Result<ProviderHttpResponse, LocalAIError> {
        let mut builder = self
            .client()
            .post(request.url)
            .timeout(REQUEST_TIMEOUT)
            .json(&request.payload);
        if let Some(token) = request.bearer_token {
            builder = builder.bearer_auth(token.expose());
        }

        #[cfg(not(target_family = "wasm"))]
        let response = builder
            .send()
            .compat()
            .await
            .map_err(LocalAIError::Transport)?;
        #[cfg(target_family = "wasm")]
        let response = builder.send().await.map_err(LocalAIError::Transport)?;

        let status = response.status();
        if !status.is_success() {
            return Err(LocalAIError::HttpStatus(status));
        }

        #[cfg(not(target_family = "wasm"))]
        let payload = response
            .json()
            .compat()
            .await
            .map_err(LocalAIError::Transport)?;
        #[cfg(target_family = "wasm")]
        let payload = response.json().await.map_err(LocalAIError::Transport)?;

        Ok(ProviderHttpResponse { status, payload })
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait NextCommandProvider: Send + Sync {
    async fn suggest(
        &self,
        connection: ProviderConnection,
        context: NextCommandContext,
    ) -> Result<NextCommandSuggestion, LocalAIError>;
}

pub struct DirectNextCommandProvider {
    transport: Arc<dyn ProviderTransport>,
}

impl Default for DirectNextCommandProvider {
    fn default() -> Self {
        Self::new(Arc::new(DirectHttpTransport::default()))
    }
}

impl DirectNextCommandProvider {
    pub fn new(transport: Arc<dyn ProviderTransport>) -> Self {
        Self { transport }
    }

    fn request(
        connection: &ProviderConnection,
        context: NextCommandContext,
    ) -> Result<ProviderHttpRequest, LocalAIError> {
        if connection.model.trim().is_empty() {
            return Err(LocalAIError::MissingModel);
        }
        if connection.provider.requires_api_key() && connection.credential.is_none() {
            return Err(LocalAIError::MissingCredential);
        }

        let (url, payload) = match connection.provider {
            LocalAIProvider::OpenAICompatible | LocalAIProvider::OpenRouter => (
                provider_url(&connection.endpoint, "chat/completions")?,
                openai_payload(&connection.model, context),
            ),
            LocalAIProvider::Ollama => (
                provider_url(&connection.endpoint, "api/chat")?,
                ollama_payload(&connection.model, context),
            ),
        };

        Ok(ProviderHttpRequest {
            url,
            bearer_token: connection.credential.clone(),
            payload,
        })
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl NextCommandProvider for DirectNextCommandProvider {
    async fn suggest(
        &self,
        connection: ProviderConnection,
        context: NextCommandContext,
    ) -> Result<NextCommandSuggestion, LocalAIError> {
        let provider = connection.provider;
        let response = self
            .transport
            .send(Self::request(&connection, context)?)
            .await?;
        if !response.status.is_success() {
            return Err(LocalAIError::HttpStatus(response.status));
        }

        let command = match provider {
            LocalAIProvider::OpenAICompatible | LocalAIProvider::OpenRouter => response
                .payload
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            LocalAIProvider::Ollama => response
                .payload
                .pointer("/message/content")
                .and_then(Value::as_str),
        }
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or(LocalAIError::InvalidResponse)?;

        Ok(NextCommandSuggestion::DisplayOnly {
            command: command.to_owned(),
        })
    }
}

fn provider_url(endpoint: &str, suffix: &str) -> Result<Url, LocalAIError> {
    let mut url = Url::parse(endpoint.trim()).map_err(|_| LocalAIError::InvalidEndpoint)?;
    let secure_transport = url.scheme() == "https"
        || (url.scheme() == "http"
            && match url.host() {
                Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(Host::Ipv4(address)) => address.is_loopback(),
                Some(Host::Ipv6(address)) => address.is_loopback(),
                None => false,
            });
    if !secure_transport
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(LocalAIError::InvalidEndpoint);
    }

    let path = url.path().trim_end_matches('/');
    if !path.ends_with(suffix) {
        url.set_path(&format!("{path}/{suffix}"));
    }
    Ok(url)
}

fn context_message(context: NextCommandContext) -> String {
    serde_json::to_string(&context).expect("Next Command context is serializable")
}

fn openai_payload(model: &str, context: NextCommandContext) -> Value {
    json!({
        "model": model.trim(),
        "messages": [
            {
                "role": "system",
                "content": "Predict the next shell command. Return exactly one command and no explanation. Never execute commands."
            },
            {
                "role": "user",
                "content": context_message(context)
            }
        ],
        "stream": false,
        "temperature": 0.2,
        "max_tokens": 200
    })
}

fn ollama_payload(model: &str, context: NextCommandContext) -> Value {
    json!({
        "model": model.trim(),
        "messages": [
            {
                "role": "system",
                "content": "Predict the next shell command. Return exactly one command and no explanation. Never execute commands."
            },
            {
                "role": "user",
                "content": context_message(context)
            }
        ],
        "stream": false,
        "options": {
            "temperature": 0.2,
            "num_predict": 200
        }
    })
}

#[cfg(test)]
mod tests;
