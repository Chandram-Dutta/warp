use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::executor::block_on;
use reqwest::StatusCode;
use serde_json::json;

use super::*;

struct MockTransport {
    requests: Mutex<Vec<ProviderHttpRequest>>,
    response: Mutex<Option<ProviderHttpResponse>>,
}

impl MockTransport {
    fn new(payload: Value) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            response: Mutex::new(Some(ProviderHttpResponse {
                status: StatusCode::OK,
                payload,
            })),
        }
    }
}

#[async_trait]
impl ProviderTransport for MockTransport {
    async fn send(
        &self,
        request: ProviderHttpRequest,
    ) -> Result<ProviderHttpResponse, LocalAIError> {
        self.requests.lock().unwrap().push(request);
        Ok(self.response.lock().unwrap().take().unwrap())
    }
}

fn context() -> NextCommandContext {
    NextCommandContext::new(
        "zsh",
        Some("5.9".to_owned()),
        Some("/tmp/project".to_owned()),
        "git ",
        vec![RecentCommand {
            command: "git status".to_owned(),
            working_directory: Some("/tmp/project".to_owned()),
            exit_code: 0,
        }],
    )
}

#[test]
fn config_is_local_non_secret_data() {
    let config = LocalAINextCommandConfig::default();
    let serialized = serde_json::to_string(&config).unwrap();

    assert_eq!(config.active_provider, LocalAIProvider::Ollama);
    assert_eq!(config.ollama.endpoint, "http://localhost:11434");
    assert!(config.ollama.model.is_empty());
    assert!(!serialized.contains("api_key"));
    assert!(!serialized.contains("credential"));
}

#[test]
fn credential_debug_output_is_redacted() {
    let credential = ProviderCredential::new("secret-sentinel").unwrap();
    let connection = ProviderConnection::new(
        LocalAIProvider::OpenAICompatible,
        "https://example.com/v1",
        "model",
        Some(credential),
    );

    let debug = format!("{connection:?}");
    assert!(!debug.contains("secret-sentinel"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn openai_transport_receives_only_bounded_shell_and_command_context() {
    let output_sentinel = "terminal-output-must-not-be-sent";
    let codebase_sentinel = "codebase-context-must-not-be-sent";
    let transport = Arc::new(MockTransport::new(json!({
        "choices": [{"message": {"content": "git pull"}}]
    })));
    let provider = DirectNextCommandProvider::new(transport.clone());
    let credential = ProviderCredential::new("secret-sentinel");
    let connection = ProviderConnection::new(
        LocalAIProvider::OpenAICompatible,
        "https://example.com/v1/",
        "example-model",
        credential,
    );

    let suggestion = block_on(provider.suggest(connection, context())).unwrap();
    assert_eq!(suggestion.display_only_command(), "git pull");

    let requests = transport.requests.lock().unwrap();
    let request = requests.first().unwrap();
    assert_eq!(
        request.url.as_str(),
        "https://example.com/v1/chat/completions"
    );
    let payload = serde_json::to_string(&request.payload).unwrap();
    assert!(payload.contains("git status"));
    assert!(payload.contains("/tmp/project"));
    assert!(!payload.contains(output_sentinel));
    assert!(!payload.contains(codebase_sentinel));
    assert!(!payload.contains("git_branch"));
    assert!(!payload.contains("environment"));
    assert!(!payload.contains("secret-sentinel"));
    assert_eq!(
        request.bearer_token.as_ref().unwrap().expose(),
        "secret-sentinel"
    );
}

#[test]
fn openrouter_requires_a_secure_credential() {
    let transport = Arc::new(MockTransport::new(json!({})));
    let provider = DirectNextCommandProvider::new(transport.clone());
    let connection = ProviderConnection::new(
        LocalAIProvider::OpenRouter,
        "https://openrouter.ai/api/v1",
        "example/model",
        None,
    );

    let result = block_on(provider.suggest(connection, context()));
    assert!(matches!(result, Err(LocalAIError::MissingCredential)));
    assert!(transport.requests.lock().unwrap().is_empty());
}

#[test]
fn openrouter_uses_chat_completions_with_a_bearer_credential() {
    let transport = Arc::new(MockTransport::new(json!({
        "choices": [{"message": {"content": "git fetch --prune"}}]
    })));
    let provider = DirectNextCommandProvider::new(transport.clone());
    let connection = ProviderConnection::new(
        LocalAIProvider::OpenRouter,
        "https://openrouter.ai/api/v1",
        "example/model",
        ProviderCredential::new("secret-sentinel"),
    );

    let suggestion = block_on(provider.suggest(connection, context())).unwrap();
    assert_eq!(suggestion.display_only_command(), "git fetch --prune");
    let requests = transport.requests.lock().unwrap();
    assert_eq!(
        requests[0].url.as_str(),
        "https://openrouter.ai/api/v1/chat/completions"
    );
    assert_eq!(
        requests[0].bearer_token.as_ref().unwrap().expose(),
        "secret-sentinel"
    );
}

#[test]
fn ollama_uses_native_chat_api_and_returns_display_only_suggestion() {
    let transport = Arc::new(MockTransport::new(json!({
        "message": {"content": "cargo test -p warp_local_ai"}
    })));
    let provider = DirectNextCommandProvider::new(transport.clone());
    let connection = ProviderConnection::new(
        LocalAIProvider::Ollama,
        "http://localhost:11434",
        "qwen2.5-coder",
        None,
    );

    let suggestion = block_on(provider.suggest(connection, context())).unwrap();
    assert_eq!(
        suggestion,
        NextCommandSuggestion::DisplayOnly {
            command: "cargo test -p warp_local_ai".to_owned()
        }
    );
    assert_eq!(
        transport.requests.lock().unwrap()[0].url.as_str(),
        "http://localhost:11434/api/chat"
    );
}

#[test]
fn context_is_bounded_to_recent_commands_and_field_limits() {
    let recent_commands = (0..10)
        .map(|index| RecentCommand {
            command: format!("{index}-{}", "x".repeat(MAX_COMMAND_CHARS + 10)),
            working_directory: Some("p".repeat(MAX_DIRECTORY_CHARS + 10)),
            exit_code: index,
        })
        .collect();
    let context = NextCommandContext::new(
        "s".repeat(MAX_SHELL_FIELD_CHARS + 10),
        None,
        None,
        "p".repeat(MAX_PREFIX_CHARS + 10),
        recent_commands,
    );

    assert_eq!(context.recent_commands.len(), MAX_COMMAND_CONTEXTS);
    assert!(context.recent_commands[0].command.starts_with("5-"));
    assert_eq!(context.shell.name.chars().count(), MAX_SHELL_FIELD_CHARS);
    assert_eq!(context.command_prefix.chars().count(), MAX_PREFIX_CHARS);
    assert_eq!(
        context.recent_commands[0].command.chars().count(),
        MAX_COMMAND_CHARS
    );
    assert_eq!(
        context.recent_commands[0]
            .working_directory
            .as_ref()
            .unwrap()
            .chars()
            .count(),
        MAX_DIRECTORY_CHARS
    );
}

#[test]
fn endpoint_credentials_are_rejected() {
    let result = provider_url("https://user:secret@example.com/v1", "chat/completions");
    assert!(matches!(result, Err(LocalAIError::InvalidEndpoint)));

    let result = provider_url("https://example.com/v1?api_key=secret", "chat/completions");
    assert!(matches!(result, Err(LocalAIError::InvalidEndpoint)));
}

#[test]
fn insecure_remote_endpoints_are_rejected() {
    let result = provider_url("http://example.com/v1", "chat/completions");
    assert!(matches!(result, Err(LocalAIError::InvalidEndpoint)));

    assert!(provider_url("http://localhost:11434", "api/chat").is_ok());
    assert!(provider_url("http://127.0.0.1:11434", "api/chat").is_ok());
    assert!(provider_url("http://[::1]:11434", "api/chat").is_ok());
}
