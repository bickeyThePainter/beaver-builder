# Observability & Debugging — Codex Deep Research

## Why This Matters

You can't improve what you can't observe. AI agents are notoriously hard to debug —
they're non-deterministic, multi-step, and failures often manifest far from their root
cause (a bad tool call 5 turns ago causes a wrong answer now). Codex addresses this
with a multi-layered observability system that captures events at every layer.

---

## Event System Architecture

### Three Observation Layers

```
Layer 1: Protocol Events (EventMsg)
  → User-facing: streamed to CLI/TUI in real-time
  → Persistent: written to rollout JSONL file

Layer 2: OpenTelemetry (SessionTelemetry)
  → Metrics: counters, histograms, durations
  → Structured traces: distributed tracing with W3C context

Layer 3: SQLite Log Database (LogDbLayer)
  → All tracing events batched and persisted
  → 10-day retention with automatic cleanup
  → Queryable via logs_client CLI
```

---

## Layer 1: Protocol Events (EventMsg)

**File**: `protocol/src/protocol.rs`

The `EventMsg` enum covers the full agent lifecycle:

| Event | Key Data |
|-------|----------|
| `TurnStarted` | model, turn_id, metadata |
| `TurnComplete` | turn_id, exit_code/reason, token_usage |
| `ExecCommandBegin` | command, cwd, parsed_cmd, source, process_id |
| `ExecCommandEnd` | stdout, stderr, exit_code, duration, status |
| `PatchApplyBegin` | changes (file → FileChange map), auto_approved |
| `PatchApplyEnd` | status (Completed/Failed/Declined) |
| `ItemStarted` | TurnItem (message, reasoning, tool call) |
| `ItemCompleted` | TurnItem with result |
| `TokenCount` | input, cached_input, output, reasoning_output, total |
| `ErrorEvent` | message, CodexErrorInfo (structured error type) |
| `WarningEvent` | message |

Events are wrapped in `Event { id: turn_id, msg: EventMsg }` and flow through:
```
Tool/Agent → session.send_event() → event channel → CLI/TUI + rollout file
```

### CodexErrorInfo: Structured Error Classification

```rust
pub enum CodexErrorInfo {
    ContextWindowExceeded,
    UsageLimitExceeded,
    ServerOverloaded,
    HttpConnectionFailed { http_status_code },
    ResponseStreamConnectionFailed { http_status_code },
    InternalServerError,
    Unauthorized,
    BadRequest,
    SandboxError,
    ResponseStreamDisconnected { http_status_code },
    ResponseTooManyFailedAttempts { attempt_count },
}
```

Each variant has an `affects_turn_status()` method that determines whether the error
should mark the turn as failed vs. allowing retry.

---

## Layer 2: OpenTelemetry Integration

**File**: `otel/src/events/session_telemetry.rs`

### Structured Logging Macros

```rust
log_event!(telemetry, "tool_call_started", { tool_name: name });
trace_event!(telemetry, "model_response_received", { tokens: count });
log_and_trace_event!(telemetry, "approval_granted", { command: cmd });
```

### Metrics Collected

| Metric | Type | Purpose |
|--------|------|---------|
| `API_CALL_COUNT_METRIC` | Counter | Total API calls |
| `API_CALL_DURATION_METRIC` | Histogram | API response latency |
| `SSE_EVENT_COUNT_METRIC` | Counter | SSE events received |
| `SSE_EVENT_DURATION_METRIC` | Histogram | SSE event processing time |
| `WEBSOCKET_EVENT_COUNT_METRIC` | Counter | WebSocket events |
| `WEBSOCKET_REQUEST_DURATION_METRIC` | Histogram | WS request latency |
| `TOOL_CALL_COUNT_METRIC` | Counter | Tool invocations |
| `TOOL_CALL_DURATION_METRIC` | Histogram | Tool execution time |
| `RESPONSES_API_ENGINE_IAPI_TTFT_DURATION_METRIC` | Histogram | Time-to-first-token |

### Session Tagging

All telemetry events are automatically tagged with:
- `auth_mode` — authentication method
- `session_source` — how the session was created
- `originator` — who initiated the action
- `model` — active model
- `app_version` — Codex version

### Distributed Tracing

W3C Trace Context propagation (`W3cTraceContext`) across async handoffs:
- `traceparent` / `tracestate` headers
- Spans across model API calls, tool execution, approval flows

---

## Layer 3: SQLite Log Database

**File**: `state/src/log_db.rs`

### Architecture

A `tracing_subscriber::Layer` captures all tracing events and batches them into SQLite:

```sql
CREATE TABLE logs (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    target TEXT,
    message TEXT,
    thread_id TEXT,
    process_uuid TEXT,
    module_path TEXT,
    file TEXT,
    line INTEGER
);
```

### Batching & Retention

- Events are buffered and batch-flushed periodically
- Background task runs 10-day retention policy
- Configurable batch size for performance tuning

### Query Tool

**File**: `state/src/bin/logs_client.rs`

CLI tool to query the log database:
```bash
codex-logs query --level error --since "2h ago" --target "core::tools"
```

---

## Rollout: Session Replay

### What's Captured

**File**: `protocol/src/protocol.rs`

Every session action is persisted as a `RolloutItem` in JSONL format:

```rust
pub enum RolloutItem {
    EventMsg(EventMsg),           // User-facing events
    ResponseItem(ResponseItem),    // Model conversation items
    TurnContext(TurnContextItem),  // Metadata snapshots
    Compacted(CompactedContextItem), // Compressed history markers
    SessionMeta(SessionMetadata),  // Session-level metadata
}
```

### Session Reconstruction

**File**: `core/src/codex/rollout_reconstruction.rs`

`RolloutReconstruction` parses the rollout file and rebuilds full session state:
- Tracks context changes across turns
- Handles rollbacks (thread forks)
- Recreates the exact conversation state at any point

### Why This Matters for Debugging

The rollout file is the **ground truth** for what happened in a session. When a user
reports a bug:

1. Load their rollout file
2. Replay the session to the point of failure
3. Inspect exactly what the model saw, what it produced, what tools ran, what errors occurred

This is equivalent to having a flight recorder for every agent session.

---

## Bifurcated Error Surfacing

Codex deliberately separates how errors appear to users vs. models:

### User-Facing Errors

Sent as `EventMsg::Error(ErrorEvent)`:
- Human-readable message
- Structured `CodexErrorInfo` for programmatic handling
- Examples: "Context window exceeded", "Usage limit reached"

### Model-Facing Errors

Sent as tool output (`FunctionCallOutput`):
- Actionable error details (stderr, exit codes, stack traces)
- Formatted for model self-correction
- Examples: `"apply_patch verification failed: line 42 does not match"`

### Internal Logging

Via `tracing::error!` → SQLite:
- Full error chains with context
- Function, module, line information
- Searchable by thread_id, timestamp, level

```rust
// From tools/events.rs
ToolEventFailure::Rejected(msg) => {
    // Normalize rejection for user display
    let event = ToolEventStage::Failure(ToolEventFailure::Rejected(normalized));
    self.emit(ctx, event).await;
    // User gets normalized message, model sees detailed error
}
```

---

## Token Usage Tracking

### Per-Turn Tracking

```rust
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}
```

### Cumulative Session Tracking

```rust
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,      // Cumulative for entire session
    pub last_token_usage: TokenUsage,       // From last API response
    pub model_context_window: Option<i64>,  // Model's token limit
}
```

### Flow

1. Token usage from Responses API SSE stream captured in `ResponseEvent::Completed`
2. `TotalTokenUsageBreakdown` tracks running estimates between API calls
3. `TokenCountEvent` emitted to clients for display
4. Rate limit headers captured and surfaced

### What's NOT Tracked

No per-model pricing or billing aggregates. Codex tracks token counts, not costs.
Billing is handled upstream.

---

## Debugging Tools

| Tool | Purpose | Location |
|------|---------|----------|
| `md-events` | Parse markdown event output | `tui/src/bin/md-events.rs` |
| `logs_client` | Query SQLite log DB | `state/src/bin/logs_client.rs` |
| `debug-config` | List session flags | CLI command |
| Rollout file | Full session replay | Session cache directory |
| `log_user_prompts` | Enable user input logging | SessionTelemetry config |

---

## Design Insights

1. **Three layers serve three audiences.** Protocol events are for the user (real-time).
   OpenTelemetry is for the operator (monitoring). SQLite logs are for the developer
   (post-mortem debugging). Each layer is independently useful.

2. **Rollout files are the killer debugging feature.** They capture everything — not
   just what the model said, but what it saw, what tools ran, what errors occurred.
   This is the difference between "it didn't work" and "here's exactly why."

3. **Bifurcated error surfacing prevents information overload.** Users don't need stderr.
   Models don't need "please try again later." Each audience gets errors formatted for
   their needs.

4. **Token tracking is approximate by design.** The 4-byte heuristic is good enough for
   context management. Only the API's actual token count matters for billing. Trying to
   match the tokenizer exactly is a maintenance trap.

5. **10-day log retention is a pragmatic choice.** Long enough to debug reported issues,
   short enough to not fill disks. The rollout files provide permanent session records
   when needed.

---

## Key Files

| Component | Path |
|-----------|------|
| Event protocol | `protocol/src/protocol.rs` |
| Event emission | `core/src/tools/events.rs` |
| Session telemetry | `otel/src/events/session_telemetry.rs` |
| SQLite log DB | `state/src/log_db.rs` |
| Log query tool | `state/src/bin/logs_client.rs` |
| Rollout replay | `core/src/codex/rollout_reconstruction.rs` |
| Token tracking | `core/src/context_manager/history.rs` |
| Error classification | `protocol/src/protocol.rs` (CodexErrorInfo) |
