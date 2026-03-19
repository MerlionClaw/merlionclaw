//! Anthropic Messages API client.

use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize;
use tracing::{debug, warn};

use super::{
    CompletionRequest, CompletionResponse, ContentBlock, LlmProvider, StreamEvent, Usage,
};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_RETRIES: u32 = 3;

/// Anthropic Messages API provider.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
        }
    }

    /// Create from an environment variable name.
    pub fn from_env(env_var: &str) -> anyhow::Result<Self> {
        let api_key = std::env::var(env_var)
            .map_err(|_| anyhow::anyhow!("{env_var} not set"))?;
        Ok(Self::new(api_key))
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).expect("valid api key"),
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        headers
    }

    fn build_body(
        &self,
        request: &CompletionRequest,
        stream: bool,
    ) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "system": request.system,
            "messages": request.messages,
        });

        if !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tools);
        }

        if stream {
            body["stream"] = serde_json::Value::Bool(true);
        }

        body
    }

    async fn send_with_retry(
        &self,
        body: &serde_json::Value,
        stream: bool,
    ) -> anyhow::Result<reqwest::Response> {
        let mut last_err = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt));
                warn!(attempt, ?delay, "retrying Anthropic API call");
                tokio::time::sleep(delay).await;
            }

            let resp = self
                .client
                .post(API_URL)
                .headers(self.headers())
                .json(body)
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() || (stream && status.as_u16() == 200) {
                        return Ok(r);
                    }

                    // Retryable status codes
                    if matches!(status.as_u16(), 429 | 529 | 500) {
                        let body_text = r.text().await.unwrap_or_default();
                        warn!(status = %status, body = %body_text, "retryable Anthropic API error");
                        last_err = Some(anyhow::anyhow!(
                            "Anthropic API error {status}: {body_text}"
                        ));
                        continue;
                    }

                    // Non-retryable errors
                    let body_text = r.text().await.unwrap_or_default();
                    return Err(anyhow::anyhow!(
                        "Anthropic API error {status}: {body_text}"
                    ));
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("request failed: {e}"));
                    continue;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("max retries exceeded")))
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = self.build_body(&request, false);
        debug!("sending completion request to Anthropic");

        let resp = self.send_with_retry(&body, false).await?;
        let api_resp: ApiResponse = resp.json().await?;

        let content = api_resp
            .content
            .into_iter()
            .map(|b| b.into_content_block())
            .collect();

        Ok(CompletionResponse {
            content,
            stop_reason: api_resp.stop_reason.unwrap_or_default(),
            usage: Usage {
                input_tokens: api_resp.usage.input_tokens,
                output_tokens: api_resp.usage.output_tokens,
            },
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamEvent>> + Send>>> {
        let body = self.build_body(&request, true);
        debug!("sending streaming request to Anthropic");

        let resp = self.send_with_retry(&body, true).await?;
        let byte_stream = resp.bytes_stream();

        Ok(Box::pin(SseStream::new(byte_stream)))
    }
}

/// Server-Sent Events stream parser.
struct SseStream<S> {
    inner: S,
    buffer: String,
    done: bool,
}

impl<S> SseStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            done: false,
        }
    }
}

impl<S> Stream for SseStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin + Send,
{
    type Item = anyhow::Result<StreamEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }

        // Try to parse a complete event from the buffer first
        if let Some(event) = try_parse_event(&mut this.buffer) {
            match event {
                ParsedSseEvent::StreamEvent(se) => return Poll::Ready(Some(Ok(se))),
                ParsedSseEvent::Done => {
                    this.done = true;
                    return Poll::Ready(None);
                }
                ParsedSseEvent::Skip => {
                    // Continue to poll for more data
                }
            }
        }

        // Poll the inner stream for more bytes
        let inner = Pin::new(&mut this.inner);
        match inner.poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let text = String::from_utf8_lossy(&bytes);
                this.buffer.push_str(&text);

                // Try to extract events
                if let Some(event) = try_parse_event(&mut this.buffer) {
                    match event {
                        ParsedSseEvent::StreamEvent(se) => Poll::Ready(Some(Ok(se))),
                        ParsedSseEvent::Done => {
                            this.done = true;
                            Poll::Ready(None)
                        }
                        ParsedSseEvent::Skip => {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    }
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(anyhow::anyhow!("stream error: {e}")))),
            Poll::Ready(None) => {
                this.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

enum ParsedSseEvent {
    StreamEvent(StreamEvent),
    Done,
    Skip,
}

/// Try to parse a complete SSE event from the buffer.
/// If a complete event is found, it is consumed from the buffer.
fn try_parse_event(buffer: &mut String) -> Option<ParsedSseEvent> {
    // SSE events are separated by double newline
    let separator = if buffer.contains("\n\n") {
        "\n\n"
    } else {
        return None;
    };

    let idx = buffer.find(separator)?;
    let event_text = buffer[..idx].to_string();
    buffer.drain(..idx + separator.len());

    let mut event_type = None;
    let mut data = None;

    for line in event_text.lines() {
        if let Some(val) = line.strip_prefix("event: ") {
            event_type = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("data: ") {
            data = Some(val.to_string());
        }
    }

    let event_type = event_type?;
    let data = data.unwrap_or_default();

    match event_type.as_str() {
        "content_block_delta" => {
            if let Ok(delta) = serde_json::from_str::<SseDelta>(&data) {
                match delta.delta {
                    SseDeltaContent::TextDelta { text } => {
                        Some(ParsedSseEvent::StreamEvent(StreamEvent::TextDelta(text)))
                    }
                    SseDeltaContent::InputJsonDelta { partial_json } => {
                        Some(ParsedSseEvent::StreamEvent(StreamEvent::ToolUseInputDelta(
                            partial_json,
                        )))
                    }
                }
            } else {
                Some(ParsedSseEvent::Skip)
            }
        }
        "content_block_start" => {
            if let Ok(start) = serde_json::from_str::<SseContentBlockStart>(&data) {
                if start.content_block.block_type == "tool_use" {
                    Some(ParsedSseEvent::StreamEvent(StreamEvent::ToolUseStart {
                        id: start.content_block.id.unwrap_or_default(),
                        name: start.content_block.name.unwrap_or_default(),
                    }))
                } else {
                    Some(ParsedSseEvent::Skip)
                }
            } else {
                Some(ParsedSseEvent::Skip)
            }
        }
        "content_block_stop" => Some(ParsedSseEvent::StreamEvent(StreamEvent::ToolUseEnd)),
        "message_delta" => {
            if let Ok(delta) = serde_json::from_str::<SseMessageDelta>(&data) {
                Some(ParsedSseEvent::StreamEvent(StreamEvent::MessageEnd {
                    stop_reason: delta.delta.stop_reason.unwrap_or_default(),
                }))
            } else {
                Some(ParsedSseEvent::Skip)
            }
        }
        "message_stop" => Some(ParsedSseEvent::Done),
        "message_start" | "ping" => Some(ParsedSseEvent::Skip),
        _ => Some(ParsedSseEvent::Skip),
    }
}

// --- Anthropic API response types ---

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ApiContentBlock>,
    stop_reason: Option<String>,
    usage: ApiUsage,
}

#[derive(Debug, Deserialize)]
struct ApiContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
}

impl ApiContentBlock {
    fn into_content_block(self) -> ContentBlock {
        match self.block_type.as_str() {
            "text" => ContentBlock::Text {
                text: self.text.unwrap_or_default(),
            },
            "tool_use" => ContentBlock::ToolUse {
                id: self.id.unwrap_or_default(),
                name: self.name.unwrap_or_default(),
                input: self.input.unwrap_or(serde_json::Value::Object(Default::default())),
            },
            _ => ContentBlock::Text {
                text: format!("[unknown block type: {}]", self.block_type),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// --- SSE data types ---

#[derive(Debug, Deserialize)]
struct SseDelta {
    delta: SseDeltaContent,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum SseDeltaContent {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct SseContentBlockStart {
    content_block: SseContentBlock,
}

#[derive(Debug, Deserialize)]
struct SseContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseMessageDelta {
    delta: SseMessageDeltaInner,
}

#[derive(Debug, Deserialize)]
struct SseMessageDeltaInner {
    stop_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{super::ToolDefinition, *};

    #[test]
    fn test_parse_api_response() {
        let json = r#"{
            "content": [{"type": "text", "text": "Hello!"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.content[0].text.as_deref(), Some("Hello!"));
        assert_eq!(resp.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(resp.usage.input_tokens, 10);
    }

    #[test]
    fn test_parse_tool_use_response() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "tu_1", "name": "k8s_list_pods", "input": {"namespace": "default"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 15}
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 2);
        let block = resp.content.into_iter().nth(1).unwrap().into_content_block();
        match block {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tu_1");
                assert_eq!(name, "k8s_list_pods");
                assert_eq!(input["namespace"], "default");
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn test_parse_sse_text_delta() {
        let mut buffer = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n".to_string();
        let event = try_parse_event(&mut buffer).unwrap();
        match event {
            ParsedSseEvent::StreamEvent(StreamEvent::TextDelta(text)) => {
                assert_eq!(text, "Hello");
            }
            _ => panic!("expected TextDelta"),
        }
    }

    #[test]
    fn test_parse_sse_tool_use_start() {
        let mut buffer = "event: content_block_start\ndata: {\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"k8s_list_pods\"}}\n\n".to_string();
        let event = try_parse_event(&mut buffer).unwrap();
        match event {
            ParsedSseEvent::StreamEvent(StreamEvent::ToolUseStart { id, name }) => {
                assert_eq!(id, "tu_1");
                assert_eq!(name, "k8s_list_pods");
            }
            _ => panic!("expected ToolUseStart"),
        }
    }

    #[test]
    fn test_parse_sse_message_stop() {
        let mut buffer = "event: message_stop\ndata: {}\n\n".to_string();
        let event = try_parse_event(&mut buffer).unwrap();
        assert!(matches!(event, ParsedSseEvent::Done));
    }

    #[test]
    fn test_build_body_without_tools() {
        let provider = AnthropicProvider::new("test-key".to_string());
        let request = CompletionRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            system: "You are helpful.".to_string(),
            messages: vec![],
            tools: vec![],
            max_tokens: 1024,
        };
        let body = provider.build_body(&request, false);
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        assert!(body.get("tools").is_none());
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn test_build_body_with_tools_and_stream() {
        let provider = AnthropicProvider::new("test-key".to_string());
        let request = CompletionRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            system: "You are helpful.".to_string(),
            messages: vec![],
            tools: vec![ToolDefinition {
                name: "test".to_string(),
                description: "a test tool".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            max_tokens: 1024,
        };
        let body = provider.build_body(&request, true);
        assert_eq!(body["stream"], true);
        assert_eq!(body["tools"][0]["name"], "test");
    }
}
