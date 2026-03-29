//! LLM API client -- infrastructure adapter for calling language models.
//! Supports OpenAI-compatible APIs with retries and streaming.

use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A message in the LLM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

/// Configuration for an LLM API call.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub tools: Option<serde_json::Value>,
}

/// Response from the LLM API.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Streaming chunk from the LLM API.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta: String,
    pub is_final: bool,
}

/// LLM client with retry logic and streaming support.
pub struct LlmClient {
    pub base_url: String,
    pub api_key: String,
    http: reqwest::Client,
    max_retries: u32,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<LlmMessage>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCall>,
}

#[derive(Debug, Deserialize)]
struct ApiToolCall {
    function: ApiFunction,
}

#[derive(Debug, Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

/// Streaming response types
#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

impl LlmClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
            max_retries: 3,
        }
    }

    /// Create client from environment variables.
    ///
    /// Configuration:
    /// - `LLM_BASE_URL`: Base URL for the OpenAI-compatible API (default: `http://localhost:11434/v1` for Ollama)
    /// - `LLM_API_KEY`: Bearer token for authentication (default: empty, which is fine for local Ollama)
    ///
    /// This client uses the OpenAI-compatible `/v1/chat/completions` endpoint format.
    /// It works with Ollama, vLLM, LiteLLM, OpenAI, or any OpenAI-compatible proxy.
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "http://localhost:11434/v1".into());
        let api_key = std::env::var("LLM_API_KEY").unwrap_or_default();
        Self::new(base_url, api_key)
    }

    /// Non-streaming chat completion with retry logic.
    pub async fn chat(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = ChatCompletionRequest {
            model: request.model,
            messages: request.messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: request.tools,
            stream: None,
        };

        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tracing::warn!("LLM request retry #{attempt}, waiting {backoff:?}");
                tokio::time::sleep(backoff).await;
            }

            match self.do_chat_request(&body).await {
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_retryable() => {
                    tracing::warn!("LLM request failed (retryable): {e}");
                    last_err = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or(LlmError::MaxRetriesExceeded))
    }

    /// Streaming chat completion. Returns a channel receiver for chunks.
    pub async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>, LlmError> {
        let body = ChatCompletionRequest {
            model: request.model,
            messages: request.messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: request.tools,
            stream: Some(true),
        };

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.api_key.is_empty() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                    .map_err(|_| LlmError::InvalidApiKey)?,
            );
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(LlmError::Http)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_text = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status,
                body: body_text,
            });
        }

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let mut byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();
            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        // Process SSE lines
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    let _ = tx
                                        .send(StreamChunk {
                                            delta: String::new(),
                                            is_final: true,
                                        })
                                        .await;
                                    return;
                                }
                                if let Ok(resp) = serde_json::from_str::<StreamResponse>(data) {
                                    if let Some(choice) = resp.choices.first() {
                                        if let Some(content) = &choice.delta.content {
                                            let _ = tx
                                                .send(StreamChunk {
                                                    delta: content.clone(),
                                                    is_final: choice.finish_reason.is_some(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Stream error: {e}");
                        break;
                    }
                }
            }
            // Send final chunk if stream ended without [DONE]
            let _ = tx
                .send(StreamChunk {
                    delta: String::new(),
                    is_final: true,
                })
                .await;
        });

        Ok(rx)
    }

    async fn do_chat_request(
        &self,
        body: &ChatCompletionRequest,
    ) -> Result<LlmResponse, LlmError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.api_key.is_empty() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                    .map_err(|_| LlmError::InvalidApiKey)?,
            );
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(LlmError::Http)?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let api_response: ChatCompletionResponse =
            response.json().await.map_err(LlmError::Http)?;

        let choice = api_response
            .choices
            .into_iter()
            .next()
            .ok_or(LlmError::EmptyResponse)?;

        let tool_calls = choice
            .message
            .tool_calls
            .into_iter()
            .map(|tc| ToolCall {
                name: tc.function.name,
                arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
            })
            .collect();

        Ok(LlmResponse {
            content: choice.message.content.unwrap_or_default(),
            tool_calls,
            finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".into()),
        })
    }
}

/// Errors from the LLM client.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(reqwest::Error),

    #[error("API error (status {status}): {body}")]
    ApiError { status: u16, body: String },

    #[error("Empty response from LLM")]
    EmptyResponse,

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,
}

impl LlmError {
    fn is_retryable(&self) -> bool {
        match self {
            LlmError::Http(e) => e.is_timeout() || e.is_connect(),
            LlmError::ApiError { status, .. } => {
                *status == 429 || *status >= 500
            }
            _ => false,
        }
    }
}
