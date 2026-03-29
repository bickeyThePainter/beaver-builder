# Agent Loop & Turn Mechanics

Deep-dive into how Codex orchestrates its core agent loop — from thread creation through
turn execution to completion, including interruption and backpressure handling.

---

## 1. Architecture: Queue-Based Asynchronous System

Codex operates on a **submission/event channel pattern**:

- **`submission_loop()`** processes all operations (`Op::UserInput`, `Op::Interrupt`, `Op::Shutdown`, etc.) in background
- Each operation is dispatched to a handler (fire-and-forget, except shutdown)
- Events flow back through `tx_event` channel to clients
- Agent status broadcast via `watch::Sender<AgentStatus>`

```
 User / UI                         Core Engine
 ────────                         ────────────
    │                                  │
    │  Op::UserInput ──────────►  rx_sub (bounded:8)
    │                                  │
    │                          submission_loop()
    │                                  │
    │                          handlers::user_input_or_turn()
    │                                  │
    │                          spawn_task(RegularTask)
    │                                  │
    │  ◄────── Event ──────────  tx_event (unbounded)
    │                                  │
    │  ◄── AgentStatus ────────  watch::Sender<AgentStatus>
```

---

## 2. Core Lifecycle: Thread → Turn → Task → Completion

### 2.1 Thread Creation (`ThreadManager::start_thread`)

- Creates `Session` with shared state: `Arc<Session>`
- Spawns background `submission_loop` task
- Returns `Codex` interface with channels

```rust
pub struct Session {
    pub(crate) tx_sub: Sender<Submission>,      // Bounded(8)
    pub(crate) rx_event: Receiver<Event>,       // Unbounded
    pub(crate) agent_status: watch::Receiver<AgentStatus>,
}

const SUBMISSION_CHANNEL_CAPACITY: usize = 8;
```

### 2.2 Turn Initiation (on `Op::UserInput`)

`handlers::user_input_or_turn()` calls `new_turn_with_sub_id()`:

1. Creates `TurnContext` — an **immutable snapshot** of config for this turn
2. Tries `steer_input()` to inject into active task
3. If no active task: calls `spawn_task()` with `RegularTask`

**Key design**: `TurnContext` is `Arc<_>` — it cannot change mid-turn. This prevents
configuration drift during execution.

### 2.3 Turn Execution (`run_turn()`)

The main agent loop:

```
loop {
    ┌─────────────────────────────────────────┐
    │  1. Build prompt (history + tools + ctx) │
    ├─────────────────────────────────────────┤
    │  2. Call model API (streaming response)  │
    ├─────────────────────────────────────────┤
    │  3. Parse response items:                │
    │     - AgentMessage → emit to UI          │
    │     - FunctionCall(A) → execute tool A   │
    │     - FunctionCall(B) → execute tool B   │
    ├─────────────────────────────────────────┤
    │  4. Collect tool outputs                 │
    ├─────────────────────────────────────────┤
    │  5. Check: needs_follow_up?              │
    │     YES → loop again with tool outputs   │
    │     NO  → turn complete                  │
    ├─────────────────────────────────────────┤
    │  6. Auto-compact if token limit hit      │
    │     → compact history, retry sampling    │
    └─────────────────────────────────────────┘
```

**Sampling Request**: builds prompt, calls model API with retry/fallback logic.
**Tool Execution**: runs function calls, collects outputs.
**Loop Decision**: `needs_follow_up=true` → loop again; `false` → turn complete.
**Auto-Compaction**: if token limit hit mid-turn, compacts history and retries.

### 2.4 Turn Completion (`on_task_finished()`)

1. Removes task from `ActiveTurn`
2. Drains buffered `pending_input` for re-processing
3. Emits `TurnComplete` event with optional last message
4. Clears `active_turn` to allow next turn

---

## 3. Multi-Tool Execution Pattern

The model can return **multiple tool calls in one response**:

```
Model response: [FunctionCall(A), FunctionCall(B), AgentMessage]
                        │
        Both A and B executed (parallel or serial depending on tool type)
                        │
        SamplingRequestResult { needs_follow_up: true }
                        │
        Loop continues with tool outputs in context
                        │
        Model gets A's output + B's output, can call more tools
```

This continues until model returns **only** text (no tool calls).

**Parallelism control** (from `tools/parallel.rs`):

```rust
pub struct ToolCallRuntime {
    parallel_execution: Arc<RwLock<()>>,  // Gate for non-parallel tools
}

// Tools marked supports_parallel=true → read lock (concurrent)
// Tools marked supports_parallel=false → write lock (exclusive)
```

---

## 4. Interruption & Abort Semantics

When user sends `Op::Interrupt`:

```
1. Cancel the CancellationToken
        │
2. Wait 100ms for graceful shutdown (select!)
        │
3. Force-abort Tokio task via AbortHandle if still running
        │
4. Call task's abort() hook for cleanup
        │
5. Record abort marker in history:
   "<turn_aborted>User interrupted...</turn_aborted>"
        │
6. Emit TurnAborted event with reason
        │
7. Clear active_turn to allow next turn
```

**Critical invariant**: `spawn_task()` always calls `abort_all_tasks(Replaced)` first,
so only one task runs per turn.

### Abort Reasons

```rust
enum TurnAbortReason {
    Interrupted,   // User sent Op::Interrupt
    Replaced,      // New task spawned, replacing old one
    ReviewEnded,   // Code review session concluded
}
```

---

## 5. Backpressure & Input Steering

During a running turn, if user sends more input:

```rust
fn steer_input(input) -> SteerResult {
    if active_turn.exists() {
        // Queue in buffer — never lost
        turn_state.pending_input.push(input);
        SteerResult::Steered
    } else {
        SteerResult::NoActiveTurn(input)  // Caller spawns new task
    }
}
```

- `steer_input()` queues it in `TurnState::pending_input` buffer
- Turn loop drains buffer at start of each iteration
- If no active turn: returns `NoActiveTurn(input)` → handler spawns new task
- Multiple user messages queue sequentially in buffer

---

## 6. Context Window Management

```rust
auto_compact_limit = model.auto_compact_token_limit()

loop {
    sampling_request()
    total_tokens = get_total_token_usage()
    if total_tokens >= auto_compact_limit && needs_follow_up {
        run_auto_compact(...)  // Compact history
        continue               // Retry sampling
    }
}
```

**Truncation Policy**: Uses model-specific truncation rules to keep conversation
within budget. When exceeded mid-turn, runs auto-compaction and retries rather
than failing.

---

## 7. Error Handling

### Terminal Errors (return immediately)

| Error | Meaning |
|---|---|
| `ContextWindowExceeded` | Conversation too long even after compaction |
| `UsageLimitReached` | Rate limit / quota exhausted |
| `InvalidImageRequest` | Bad image in tool response |
| `TurnAborted` | User interrupted (handled separately) |

### Retryable Errors (exponential backoff with fallback)

- Network disconnections
- Stream errors
- Can switch from WebSocket → HTTPS fallback automatically

---

## 8. AgentStatus Tracking

Events drive status transitions via `watch::Sender`:

```
TurnStarted    → Running
TurnComplete   → Completed(last_message)
TurnAborted    → Errored(reason)
Error          → Errored(message)
ShutdownComplete → Shutdown
```

---

## 9. Lock Ordering (Prevents Deadlock)

Strict acquisition order:

```
1. Session.state           (outermost)
2. Session.active_turn
3. ActiveTurn.turn_state   (innermost)
```

---

## 10. Key Design Patterns

| Pattern | Implementation | Purpose |
|---|---|---|
| **Shared ownership** | `Arc<Session>`, `Arc<TurnContext>`, `Arc<dyn SessionTask>` | Safe cross-task sharing |
| **Cancellation** | `CancellationToken` with child tokens | Hierarchical cancellation |
| **Event persistence** | Event → RolloutItem persisted → Status updated → Event sent | Durability before notification |
| **Immutable turns** | `TurnContext` is `Arc<_>` snapshot | No config drift mid-turn |
| **Graceful shutdown** | Select between `Notify` completion and timeout | Clean resource teardown |
| **Input steering** | `pending_input: Vec<ResponseInputItem>` | Never lose user messages |

---

## Key Files

| File | Purpose |
|---|---|
| `codex-rs/core/src/codex.rs` | Main Codex struct, agent loop (~6200 lines) |
| `codex-rs/core/src/thread_manager.rs` | Thread lifecycle management |
| `codex-rs/core/src/state/mod.rs` | State structures |
| `codex-rs/core/src/state/turn.rs` | `ActiveTurn`, `TurnState` |
| `codex-rs/core/src/tasks/mod.rs` | Task spawning / abortion |
| `codex-rs/core/src/agent/` | Status, guards, control |
