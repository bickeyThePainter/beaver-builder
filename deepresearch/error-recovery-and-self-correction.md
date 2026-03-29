# Error Recovery & Self-Correction — Codex Deep Research

## Why This Matters

The difference between an agent that "just works" and one that frustrates users is almost
entirely error recovery. Happy paths are easy. The question is: what happens when a tool
fails, the model hallucinates, the sandbox blocks a command, the API rate-limits you,
or the connection drops mid-stream?

Codex implements a **5-layer recovery architecture** where each layer handles a different
failure class, and critically, feeds actionable error information back to the model for
self-correction.

---

## Layer 1: Stream Retry with Exponential Backoff

**File**: `core/src/codex.rs:5823-5913`

The outermost retry loop operates at the sampling request level:

```
loop {
    result = try_run_sampling_request()

    match result {
        Ok(output) → return
        Err(ContextWindowExceeded) → non-retryable, fail immediately
        Err(UsageLimitReached) → non-retryable, update rate limits, fail
        Err(retryable) → apply backoff and retry
    }

    if retries >= max_retries && has_fallback_transport:
        switch transport (WebSocket → HTTPS)
        reset retry counter

    if retries < max_retries:
        sleep(backoff(retries))
        notify user: "Reconnecting... {retries}/{max_retries}"
}
```

### Backoff Strategy (`core/src/util.rs:40-45`)

- **Initial delay**: 200ms
- **Factor**: 2.0 (doubles each attempt)
- **Jitter**: 10% randomization (0.9–1.1x) to prevent thundering herd
- **Formula**: `base = 200ms * 2^(attempt-1) * random(0.9, 1.1)`

### Retryability Classification (`core/src/error.rs:195-230`)

**Retryable** (transient):
- Stream disconnections (before completion)
- Timeouts, connection failures, I/O errors
- JSON parse errors (corrupted stream)
- Task join errors
- Unexpected HTTP status codes

**Non-retryable** (terminal):
- Context window exceeded
- Usage/rate limits, quota exceeded
- Turn aborted by user
- Invalid requests, malformed data
- Unauthorized (token refresh failed)
- Sandbox errors
- Unsupported operations

### Transport Fallback

If retries exhaust on one transport, the system can **switch from WebSocket to HTTPS**
(or vice versa). This handles cases where one transport path is down but the other works.
Retry counter resets on transport switch.

---

## Layer 2: Sandbox Escalation Flow

**File**: `core/src/tools/orchestrator.rs:102-341`

When a sandboxed command fails, the orchestrator implements a two-stage strategy:

### Stage 1: Try in Sandbox

```
initial_sandbox = SandboxManager.select_initial(
    file_system_policy, network_policy, tool_preference, ...
)
result = run_attempt(tool, request, initial_sandbox)
```

### Stage 2: Escalate on Denial

When `SandboxErr::Denied` is received:

1. **Check escalation eligibility**: `tool.escalate_on_failure()` (default: true)
2. **Check approval policy**: Only if `wants_no_sandbox_approval()` or network context exists
3. **Generate retry reason**: Either network denial detail or generic message
4. **Request approval**: Via Guardian (if enabled) or user prompt
5. **Retry without sandbox**: Same command, `SandboxType::None`

```
On SandboxErr::Denied:
    if !tool.escalate_on_failure() → return error to model

    retry_reason = if network_denial:
        "Network access to '{host}' is blocked by policy."
    else:
        "command failed; retry without sandbox?"

    decision = request_approval(retry_reason)

    if approved:
        result = run_attempt(tool, request, SandboxType::None)
    else:
        return denial to model
```

### Approval Caching Prevents Re-Prompting

If a command was already `ApprovedForSession`, the escalated retry runs **immediately**
without bothering the user again. This is critical for UX — users should never see the
same approval prompt twice.

---

## Layer 3: Tool Error → Model Feedback Pipeline

This is the most important recovery mechanism: **the model sees what went wrong and can
self-correct**.

### Two Error Variants

```rust
pub enum FunctionCallError {
    RespondToModel(String),   // Error message becomes tool output
    Fatal(String),            // Propagated up, terminates turn
}
```

`RespondToModel` is the workhorse. When a tool fails, the error message is formatted as
a tool output — from the model's perspective, it's as if the tool returned an error string.
The model can then adjust its approach.

### Error Message Construction (`core/src/error.rs:621-655`)

For sandbox failures, the error message is built from (in priority order):
1. Aggregated output (if both stdout and stderr present)
2. Stderr alone (if only stderr has content)
3. Stdout alone (if only stdout has content)
4. Exit code fallback (if no output captured)

This ensures the model gets the *most actionable* error information available.

### Routing (`core/src/tools/router.rs`)

```rust
fn failure_response(call_id, err: FunctionCallError) -> ResponseInputItem {
    let message = err.to_string();
    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: FunctionCallOutputBody::Text(message),  // Model sees this
    }
}
```

### Apply Patch: Structured Verification Errors

**File**: `core/src/tools/handlers/apply_patch.rs`

Patches are verified *before* execution with classified error types:

| Verification result | Error to model |
|---|---|
| `CorrectnessError(parse_error)` | `"apply_patch verification failed: {parse_error}"` |
| `ShellParseError(error)` | `"apply_patch handler received invalid patch input"` |
| `NotApplyPatch` | `"apply_patch handler received non-apply_patch input"` |

The model sees *why* its patch was wrong — not just "it failed" — enabling targeted
self-correction.

---

## Layer 4: Guardian Fail-Closed Recovery

**File**: `core/src/guardian.rs:164-236`

Guardian's error handling is a masterclass in fail-safe design:

```rust
const GUARDIAN_REVIEW_TIMEOUT: Duration = Duration::from_secs(90);

let assessment = match review {
    Some(Ok(assessment)) => assessment,           // Normal path
    Some(Err(err)) => risk_score: 100,           // Error → deny
    None => risk_score: 100,                      // Timeout → deny
};
```

**Every failure mode defaults to maximum risk**:
- Timeout (90s): risk_score 100 → denied
- Runtime error: risk_score 100 → denied
- JSON parse failure: risk_score 100 → denied

The rejection message explicitly **prevents workarounds**:
```
"The agent must not attempt to achieve the same outcome via workaround,
 indirect execution, or policy circumvention."
```

---

## Layer 5: User Interrupt Handling

**File**: `core/src/tools/parallel.rs`

User Ctrl-C during tool execution is gracefully captured:

```rust
tokio::select! {
    result = tool_execution => result,
    _ = cancellation_token.cancelled() => {
        let secs = started.elapsed().as_secs_f32().max(0.1);
        Ok(FunctionCallOutput {
            body: Text(format!("aborted by user after {secs:.1}s")),
        })
    }
}
```

The abort message is fed back to the model as tool output — the model knows the command
was interrupted, not that it failed.

---

## Error → Protocol Conversion

**File**: `core/src/error.rs:568-595`

Internal errors are converted to structured protocol types for consistent handling:

```rust
pub fn to_codex_protocol_error(&self) -> CodexErrorInfo {
    match self {
        ContextWindowExceeded → CodexErrorInfo::ContextWindowExceeded,
        UsageLimitReached(_) → CodexErrorInfo::UsageLimitExceeded,
        ServerOverloaded → CodexErrorInfo::ServerOverloaded,
        RetryLimit(_) → CodexErrorInfo::ResponseTooManyFailedAttempts { attempt_count },
        ConnectionFailed(_) → CodexErrorInfo::HttpConnectionFailed { status_code },
        Sandbox(_) → CodexErrorInfo::SandboxError,
        ...
    }
}
```

This structured error type flows to both the UI (for user display) and the event stream
(for logging/debugging).

---

## Design Insights

1. **Error information is a first-class signal, not an afterthought.** Every error path
   constructs a message designed to help the model self-correct. Generic "operation failed"
   messages are avoided — the model gets stderr, exit codes, and structured reasons.

2. **Retryability is a property of the error, not the caller.** The `is_retryable()` method
   lives on the error type itself. This prevents inconsistent retry logic scattered across
   call sites.

3. **Transport fallback is invisible to the model.** The WS→HTTPS switch happens below
   the retry loop — the model never knows its transport changed. This separation of concerns
   keeps the agent loop clean.

4. **Fail-closed beats fail-open for safety systems.** Guardian's design means the *absence*
   of information is treated as maximum risk. This is the correct default — availability
   failures should not become security failures.

5. **Approval caching is essential for the escalation UX.** Without it, a sandbox denial →
   user approval → retry cycle would re-prompt the user. Caching makes the experience
   feel like one continuous operation.

6. **Patch verification before execution** prevents the model from learning bad patterns.
   If a patch is malformed, the model gets a clear error *before* any file changes — so it
   can fix its patch format rather than trying to recover from a partially-applied change.

---

## Key Files

| Component | Path |
|-----------|------|
| Stream retry loop | `core/src/codex.rs:5823-5913` |
| Backoff strategy | `core/src/util.rs:40-45` |
| Error classification | `core/src/error.rs` |
| Sandbox escalation | `core/src/tools/orchestrator.rs:102-341` |
| Tool error routing | `core/src/tools/router.rs` |
| Patch verification | `core/src/tools/handlers/apply_patch.rs` |
| Guardian fail-closed | `core/src/guardian.rs:164-236` |
| Approval caching | `core/src/tools/sandboxing.rs:33-110` |
| User interrupt | `core/src/tools/parallel.rs` |
