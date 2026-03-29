# Model Output Parsing & Streaming — Codex Deep Research

## Why This Matters

The streaming pipeline is where the agent "comes alive" for the user. Lag, glitches,
or crashes here destroy the user experience regardless of how smart the model is. More
importantly, the parsing layer is where model hallucinations are caught or propagated —
a malformed tool call that slips through here can corrupt the entire conversation state.

---

## SSE Streaming Foundation

**File**: `codex-api/src/sse/responses.rs`

### Architecture

```
OpenAI Responses API → SSE byte stream → event parsing → ResponseEvent channel
                                                              ↓
                                                      Agent loop polls
                                                              ↓
                                                      Tool dispatch / TUI render
```

### Streaming Setup

```rust
spawn_response_stream() {
    let (tx, rx) = mpsc::channel(1600);  // 1600 item buffer
    tokio::spawn(process_sse(stream, tx));
    return rx;
}
```

- Channel capacity: 1600 items
- Idle timeout: configurable per provider
- Spawned as independent async task

### SSE Event Types (Responses API)

| SSE Event | Parsed To |
|-----------|-----------|
| `response.created` | `ResponseEvent::Created` |
| `response.output_item.added` | `ResponseEvent::OutputItemAdded(ResponseItem)` |
| `response.output_item.done` | `ResponseEvent::OutputItemDone(ResponseItem)` |
| `response.output_text.delta` | `ResponseEvent::OutputTextDelta(delta)` |
| `response.reasoning_summary_text.delta` | `ResponseEvent::ReasoningSummaryDelta` |
| `response.reasoning_text.delta` | `ResponseEvent::ReasoningContentDelta` |
| `response.completed` | `ResponseEvent::Completed { response_id, token_usage }` |
| `response.failed` | `Err(ApiError::*)` |
| `response.incomplete` | `Err(ApiError::*)` |

---

## ResponseItem: The Universal Output Type

**File**: `protocol/src/models.rs:248-371`

Every model output is typed as `ResponseItem`:

```rust
pub enum ResponseItem {
    Message { role, content: Vec<ContentItem>, phase, end_turn },
    Reasoning { id, summary, content, encrypted_content },
    FunctionCall { id, name, arguments: String, call_id },
    LocalShellCall { id, call_id, status, action },
    CustomToolCall { id, call_id, name, input },
    WebSearchCall { id, status, action },
    ImageGenerationCall { id, status, revised_prompt, result },
    FunctionCallOutput { call_id, output },
    CustomToolCallOutput { call_id, output },
    GhostSnapshot { ghost_commit },
    Compaction { encrypted_content },
    Other,  // catch-all for unknown types
}
```

**Critical design**: `FunctionCall.arguments` is a **raw JSON string**, not a parsed
object. This enables:
- Lazy parsing (only parse when dispatching)
- Better error messages (show raw input on parse failure)
- Forward compatibility (unknown argument structures don't crash)

---

## Tool Call Dispatch Pipeline

**File**: `core/src/tools/router.rs:67-136`

```
ResponseItem::FunctionCall/CustomToolCall/LocalShellCall
    │
    ▼
ToolRouter::build_tool_call()
    │  Determines tool type:
    │  - MCP tool (server.tool format) → ToolPayload::Mcp
    │  - Function call → ToolPayload::Function
    │  - Custom tool → ToolPayload::Custom
    │  - LocalShellCall → ToolPayload::LocalShell
    │
    ▼
ToolRouter::dispatch_tool_call()
    │  Executes with ToolRegistry
    │
    ▼
ResponseInputItem (output fed back to model)
```

---

## Output Item Processing in Agent Loop

**File**: `core/src/stream_events_utils.rs:158-276`

`handle_output_item_done()` is the main dispatcher:

### Path 1: Tool Call Detected

```
1. ToolRouter::build_tool_call() → ToolCall or error
2. Log tool invocation with payload preview
3. Record item to history (stay in sync with rollout)
4. Queue async tool execution future into in_flight queue
5. Set needs_follow_up = true (model needs to see tool output)
```

### Path 2: Non-Tool Response

```
1. handle_non_tool_response_item()
2. Parse into TurnItem
3. Strip hidden markup (citations, proposed_plan blocks)
4. For images: save base64 to filesystem
5. Emit turn_item_started → turn_item_completed events
6. Record to conversation history
```

### Path 3: Error

```
- MissingLocalShellCallId → FunctionCallOutput with error
- RespondToModel(msg) → Queue response back to model
- Fatal(msg) → Terminate turn
```

---

## Streaming Event Consumption

**File**: `core/src/codex.rs:6629-6850`

The agent loop polls events from the response channel:

```rust
while let Ok(Some(event)) = rx_event.try_recv() {
    match event {
        OutputItemDone(item) → {
            flush previous text segments
            handle plan-mode state transitions
            handle_output_item_done() → queue tools if needed
            update last_agent_message
        }

        OutputItemAdded(item) → {
            initialize new streaming item
            seed parser with raw text
            emit turn_item_started
        }

        OutputTextDelta(delta) → {
            parse through AssistantMessageStreamParsers
            emit AgentMessageContentDelta to frontend
        }

        ReasoningSummaryDelta { delta, index } → {
            emit ReasoningContentDeltaEvent
        }

        Completed { token_usage, .. } → {
            flush all remaining text
            drain in_flight tool futures
            return SamplingRequestResult
        }
    }
}
```

### Async Tool Execution During Streaming

```rust
// Tools are queued, not awaited inline
let tool_future = Box::pin(
    tool_runtime.handle_tool_call(call, cancellation_token)
);
in_flight.push_back(tool_future);

// ... continue processing stream events ...

// After response.completed: drain all queued tools
drain_in_flight(&mut in_flight, sess, turn_context).await?;
```

This allows **parallel tool execution** while the model continues producing output.
The model might produce multiple tool calls in one response — they all run concurrently.

---

## Three Layers of Error Handling

### Layer 1: SSE Parse Errors

**File**: `codex-api/src/sse/responses.rs:392-398`

```rust
let event = match serde_json::from_str(&sse.data) {
    Ok(event) => event,
    Err(e) => {
        debug!("Failed to parse SSE event: {e}");
        continue;  // Skip malformed event, continue streaming
    }
};
```

**Policy**: Malformed JSON is **logged and skipped**. One bad event doesn't crash the
stream. This is critical — SSE streams from production APIs occasionally contain
malformed events.

### Layer 2: Event Type Handling

```rust
match event.kind.as_str() {
    "response.output_item.done" => {
        if let Ok(item) = serde_json::from_value(item_val) {
            Ok(Some(ResponseEvent::OutputItemDone(item)))
        } else {
            debug!("failed to parse ResponseItem");
            Ok(None)  // Skip, don't crash
        }
    }
    unknown => {
        trace!("unhandled responses event: {}", unknown);
        Ok(None)  // Unknown events silently ignored
    }
}
```

**Policy**: Unknown event types and unparseable items are silently ignored. This provides
forward compatibility — new API event types don't break old clients.

### Layer 3: Tool Call Errors

```rust
pub enum FunctionCallError {
    RespondToModel(String),      // Send error back to model
    MissingLocalShellCallId,     // Guardrail violation
    Fatal(String),               // Terminate turn
}
```

**Policy**: Tool-level errors are routed back to the model as tool output, enabling
self-correction. Only `Fatal` errors terminate the turn.

---

## Rate Limiting & Header Extraction

**File**: `codex-api/src/sse/responses.rs:58-96`

Headers are emitted as **separate events** at stream start:

```rust
// Before content starts streaming:
tx.send(ResponseEvent::ServerModel(model)).await;       // What model is actually serving
tx.send(ResponseEvent::RateLimits(snapshot)).await;      // Current rate limit state
tx.send(ResponseEvent::ModelsEtag(etag)).await;         // Model catalog version
tx.send(ResponseEvent::ServerReasoningIncluded(true)).await;  // Reasoning enabled?
```

This provides **early visibility** into server state without blocking content delivery.

---

## TUI Streaming Display

**File**: `tui/src/markdown_stream.rs`

### Line-Gated Rendering

```rust
pub struct MarkdownStreamCollector {
    buffer: String,
    committed_line_count: usize,
    width: Option<usize>,
}

// Only emit fully complete lines
pub fn commit_complete_lines(&mut self) -> Vec<Line<'static>> {
    // Find last \n in buffer
    // Render everything before it as markdown
    // Keep partial line in buffer for next delta
}

// On stream end: emit remaining partial line
pub fn finalize_and_drain(&mut self) -> Vec<Line<'static>> {
    // Flush buffer, render final partial line
}
```

**Design**: Characters accumulate in a buffer. Only complete lines (ending with `\n`)
are rendered. This prevents the flickering/jumping that occurs when rendering partial
markdown (e.g., a half-formed code block).

---

## Design Insights

1. **Skip, don't crash.** At every parsing layer, malformed input is logged and skipped.
   A single corrupted SSE event, an unknown event type, or an unparseable ResponseItem
   never crashes the stream. This is the single most important property of a streaming
   parser.

2. **Lazy JSON parsing for tool arguments.** Storing `arguments` as a raw string means
   the parser never fails on unknown argument schemas. Parse only when dispatching.

3. **Parallel tool execution during streaming.** Tools are queued as futures and drained
   after the response completes. This maximizes throughput when the model produces
   multiple tool calls.

4. **Line-gated TUI rendering** prevents visual artifacts from partial markdown. The
   small latency cost (waiting for `\n`) is worth the visual stability.

5. **Headers before content.** Rate limits and model info arrive before the first token.
   This allows the UI to display model info and rate limit warnings immediately, not
   after the response completes.

6. **The `Other` variant in ResponseItem** is forward-compatibility insurance. New item
   types from the API are silently ignored rather than causing deserialization failures.

---

## Key Files

| Component | Path |
|-----------|------|
| SSE streaming | `codex-api/src/sse/responses.rs` |
| ResponseItem types | `protocol/src/models.rs:248-371` |
| Tool dispatch | `core/src/tools/router.rs:67-136` |
| Output item processing | `core/src/stream_events_utils.rs:158-276` |
| Agent loop consumption | `core/src/codex.rs:6629-6850` |
| TUI markdown streaming | `tui/src/markdown_stream.rs` |
| WebSocket variant | `codex-api/src/endpoint/responses_websocket.rs` |
