use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::provider::{LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, StreamChunk, Usage};

/// OpenAI-compatible chat completion provider.
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

// ── OpenAI API request/response shapes ───────────────────────────────

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<LlmMessage>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    model: String,
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

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

// ── Implementation ───────────────────────────────────────────────────

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 500;

impl OpenAiProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        let client = Client::new();
        Self {
            client,
            api_key,
            base_url,
        }
    }

    /// Create a provider from environment variables.
    /// Reads OPENAI_API_KEY (fallback LLM_API_KEY). Base URL defaults to https://api.openai.com.
    pub fn from_env() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY")
            .or_else(|_| std::env::var("LLM_API_KEY"))
            .unwrap_or_default();
        let base_url =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".into());
        Self::new(base_url, api_key)
    }

    /// Build the completions URL: {base_url}/v1/chat/completions (no double /v1).
    fn completions_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        }
    }

    /// Determine if an HTTP status code is retryable (429, 5xx).
    fn is_retryable(status: u16) -> bool {
        status == 429 || (500..600).contains(&status)
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let url = self.completions_url();
        let body = ChatCompletionRequest {
            model: request.model.clone(),
            messages: request.messages.clone(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let mut last_err: Option<LlmError> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = BASE_BACKOFF_MS * 2u64.pow(attempt - 1);
                debug!(attempt, backoff_ms = backoff, "retrying after backoff");
                tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            }

            let result = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            let resp = match result {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        warn!(attempt, "request timed out, will retry");
                        last_err = Some(LlmError::RequestFailed {
                            message: "request timed out".into(),
                            status: None,
                        });
                        continue;
                    }
                    return Err(LlmError::RequestFailed {
                        message: e.to_string(),
                        status: None,
                    });
                }
            };

            let status = resp.status().as_u16();
            if !resp.status().is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                if Self::is_retryable(status) {
                    warn!(attempt, status, "retryable error");
                    last_err = Some(LlmError::RequestFailed {
                        message: body_text,
                        status: Some(status),
                    });
                    continue;
                }
                return Err(LlmError::RequestFailed {
                    message: body_text,
                    status: Some(status),
                });
            }

            let api_resp: ChatCompletionResponse = resp
                .json()
                .await
                .map_err(|e| LlmError::ParseError(e.to_string()))?;

            let content = api_resp
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .unwrap_or_default();

            let usage = api_resp.usage.map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            });

            return Ok(LlmResponse {
                content,
                model: api_resp.model,
                usage,
            });
        }

        Err(last_err.unwrap_or_else(|| LlmError::RequestFailed {
            message: "max retries exceeded".into(),
            status: None,
        }))
    }

    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, LlmError> {
        let url = self.completions_url();
        let body = ChatCompletionRequest {
            model: request.model.clone(),
            messages: request.messages.clone(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                message: e.to_string(),
                status: None,
            })?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::RequestFailed {
                message: body_text,
                status: Some(status),
            });
        }

        let (tx, rx) = mpsc::channel::<StreamChunk>(128);

        // Spawn a task to read SSE lines and forward chunks
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut byte_stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("stream read error: {e}");
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" {
                            let _ = tx
                                .send(StreamChunk {
                                    delta: String::new(),
                                    is_final: true,
                                })
                                .await;
                            return;
                        }

                        if let Ok(parsed) = serde_json::from_str::<StreamResponse>(data) {
                            if let Some(choice) = parsed.choices.first() {
                                let delta_text =
                                    choice.delta.content.clone().unwrap_or_default();
                                let is_final = choice.finish_reason.is_some();

                                if !delta_text.is_empty() || is_final {
                                    let _ = tx
                                        .send(StreamChunk {
                                            delta: delta_text,
                                            is_final,
                                        })
                                        .await;
                                }

                                if is_final {
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            // Stream ended without [DONE] — send final marker
            let _ = tx
                .send(StreamChunk {
                    delta: String::new(),
                    is_final: true,
                })
                .await;
        });

        Ok(rx)
    }
}
