use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

/// Role of a message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: Role,
    pub content: String,
}

/// Request payload for an LLM chat completion.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub stream: bool,
}

/// Response from an LLM chat completion.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<Usage>,
}

/// Token usage statistics.
#[derive(Debug, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A chunk of streamed LLM output.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta: String,
    pub is_final: bool,
}

/// Errors that can occur when calling an LLM provider.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("request failed: {message}")]
    RequestFailed { message: String, status: Option<u16> },

    #[error("rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("response parse error: {0}")]
    ParseError(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("provider not configured: {0}")]
    NotConfigured(String),
}

/// Provider-agnostic LLM trait. The orchestrator depends on this trait,
/// never on a concrete provider.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request and get a full response.
    async fn chat(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;

    /// Send a streaming chat completion request.
    /// Returns a channel receiver that yields chunks as they arrive.
    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, LlmError>;
}
