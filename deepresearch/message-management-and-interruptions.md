# Message Management & Interruptions

Deep-dive into how Codex manages conversation history, handles user messages during
agent execution, implements context compression, and deals with turn interruptions.

---

## 1. The Central Question: What Happens When User Messages Arrive Mid-Execution?

### Answer: Input Steering + Pending Buffer

When a user sends `Op::UserInput` while the agent is actively executing a turn:

```rust
fn steer_input(input) -> SteerResult {
    if let Some(active_turn) = &self.active_turn {
        // Queue in buffer — NEVER lost
        active_turn.turn_state.pending_input.push(input);
        SteerResult::Steered
    } else {
        // No active turn — caller will spawn new task
        SteerResult::NoActiveTurn(input)
    }
}
```

**Flow**:

```
User sends message while agent is working
        │
steer_input() checks for active turn
        │
    ┌───┴───┐
    │ YES   │ NO
    │       │
    ▼       ▼
Queue in    Return NoActiveTurn(input)
pending_input    → handler spawns new task
    │
Turn loop drains buffer at
start of each iteration
    │
Messages processed in order
```

**Key guarantee**: User messages are **never lost**. They're buffered and
processed either in the current turn or in a newly spawned one.

---

## 2. Operation Queue (Submission System)

### Op Enum — All User Operations

```rust
// codex-rs/protocol/src/protocol.rs
pub enum Op {
    UserInput { items: Vec<ResponseInputItem>, ... },
    UserTurn { items: Vec<ResponseInputItem>, ... },
    Interrupt,
    Shutdown { ... },
    // ~174 variants total
}
```

### Event Enum — All Agent Responses

```rust
pub enum EventMsg {
    TurnStarted { ... },
    TurnComplete { last_agent_message: Option<String> },
    TurnAborted { reason: TurnAbortReason },
    AgentMessageDelta { ... },
    ExecApprovalRequest { ... },
    ApplyPatchApprovalRequest { ... },
    TokenCount { ... },
    ContextCompacted,
    ThreadRolledBack,
    McpStartupComplete { ... },
    Error { ... },
    ShutdownComplete,
    // ...many more
}
```

---

## 3. Conversation History Storage

### In-Memory: ContextManager

```rust
// codex-rs/core/src/context_manager/history.rs
pub struct ContextManager {
    history: Vec<ResponseItem>,  // Ordered oldest → newest
    // ...
}
```

### Persistence: Rollout Storage

Events are also written to persistent rollout storage:

```
Event → RolloutItem persisted → AgentStatus updated → Event sent to client
```

This ensures durability — even if the process crashes, history can be recovered.

### History Item Types

```rust
pub enum ResponseItem {
    AgentMessage { ... },           // Model's text output
    FunctionCall { ... },           // Tool invocation
    FunctionCallOutput { ... },     // Tool result
    CustomToolCallOutput { ... },
    McpToolCallOutput { ... },
    UserMessage { ... },            // User input
    DeveloperMessage { ... },       // System/config injections
    // ...
}
```

---

## 4. Interruption & Abort Handling

### Full Interruption Flow

```
User sends Op::Interrupt
        │
    ┌───▼───────────────────────────────┐
    │ 1. Cancel CancellationToken       │
    │    (signals all async operations)  │
    └───┬───────────────────────────────┘
        │
    ┌───▼───────────────────────────────┐
    │ 2. Wait 100ms for graceful stop   │
    │    tokio::select! {               │
    │        done.notified() => {},     │
    │        sleep(100ms) => {},        │
    │    }                              │
    └───┬───────────────────────────────┘
        │
    ┌───▼───────────────────────────────┐
    │ 3. Force-abort via AbortHandle    │
    │    if still running               │
    └───┬───────────────────────────────┘
        │
    ┌───▼───────────────────────────────┐
    │ 4. Call task.abort() hook         │
    │    (cleanup: close connections)   │
    └───┬───────────────────────────────┘
        │
    ┌───▼───────────────────────────────┐
    │ 5. Record abort marker in history │
    │    "<turn_aborted>User           │
    │     interrupted...</turn_aborted>"│
    └───┬───────────────────────────────┘
        │
    ┌───▼───────────────────────────────┐
    │ 6. Emit TurnAborted event         │
    └───┬───────────────────────────────┘
        │
    ┌───▼───────────────────────────────┐
    │ 7. Clear active_turn              │
    │    (allows next turn to start)    │
    └───────────────────────────────────┘
```

### Abort Reasons

```rust
enum TurnAbortReason {
    Interrupted,   // User sent Op::Interrupt
    Replaced,      // New task spawned, replacing old
    ReviewEnded,   // Code review session concluded
}
```

### Critical Invariant

`spawn_task()` always calls `abort_all_tasks(Replaced)` first:

```rust
pub async fn spawn_task<T: SessionTask>(...) {
    // Abort any existing task BEFORE spawning new one
    self.abort_all_tasks(TurnAbortReason::Replaced).await;
    // Now spawn the new task
    // ...
}
```

Only one task runs per turn. No racing between turns.

### Abort Marker in History

When a turn is aborted, a marker is written to conversation history:

```xml
<turn_aborted>User interrupted the current turn</turn_aborted>
```

This ensures the model sees the interruption context on the next turn
and can adapt its behavior accordingly.

---

## 5. Context Window Management

### Token Estimation

```rust
auto_compact_limit = model.auto_compact_token_limit()

// In the turn loop:
loop {
    sampling_request()
    total_tokens = get_total_token_usage()
    if total_tokens >= auto_compact_limit && needs_follow_up {
        run_auto_compact(...)  // Compact history
        continue               // Retry sampling with compacted context
    }
}
```

### Token Counting Strategy

- Uses **byte-based estimation** (not an accurate tokenizer)
- Reserves 70-80% of context window to avoid overflow
- `estimate_token_count()` sums: `base_instructions + history_items`
- Model-specific limits via `model_context_window()`

### Overflow Handling

When token limit is hit mid-turn:
1. Run auto-compaction on history
2. Clear `reference_context_item` to force full reinjection
3. Retry the sampling request with compacted context
4. If still over limit → `ContextWindowExceeded` terminal error

---

## 6. Context Compression (Memory System)

### Two-Phase Architecture

```
Phase 1: EXTRACTION
  Model: gpt-5.1-codex-mini
  Input: Full conversation rollout (up to 150K tokens)
  Output: Raw extracted memories

Phase 2: CONSOLIDATION
  Model: gpt-5.3-codex
  Input: Phase 1 raw memories
  Output: Consolidated, deduplicated memories
```

### Phase 1 — Memory Extraction

```rust
// codex-rs/core/src/memories/prompts.rs
pub(super) fn build_stage_one_input_message(
    model_info: &ModelInfo,
    rollout_path: &Path,
    rollout_cwd: &Path,
    rollout_contents: &str,
) -> anyhow::Result<String> {
    // Uses template: templates/memories/stage_one_input.md
    // Truncates rollout to 70% of model's context window
}
```

### Phase 2 — Consolidation

```rust
pub(super) fn build_consolidation_prompt(
    memory_root: &Path,
    selection: &Phase2InputSelection,
) -> String {
    // Uses template: templates/memories/consolidation.md
    // Merges and deduplicates across sessions
}
```

### Manual Compaction

When triggered (auto or explicit), history is replaced:

```
[initial_context] + [recent_messages] + [compaction_summary]
```

- Clears `reference_context_item` to force full reinjection of system context
- Emits `ContextCompacted` event to notify UI

---

## 7. Tool Call Results in History

### Representation

```rust
ResponseItem::FunctionCallOutput {
    id: String,
    name: String,
    output: FunctionCallOutputPayload { body, success },
}
```

### Pairing Invariant

**Critical constraint**: Tool call / output pairs must be kept consistent:

- If a tool call item is removed from history → its output MUST also be removed
- If an output is removed → the call can remain (shows tool was executed but result dropped)
- This is enforced during context compaction and rollback

### Ordering Guarantee

Tool calls always appear **before** their outputs in the history vector.
This is an append-only invariant.

---

## 8. Context Updates Between Turns

### Reference Context Item

```rust
struct ReferenceContextItem {
    model: String,
    sandbox_policy: SandboxPolicy,
    approval_policy: AskForApproval,
    collaboration_mode: CollaborationMode,
    personality: Option<Personality>,
    // ...
}
```

At the start of each turn, the system compares the current state against
the reference context. If anything changed, it injects update messages:

- Model switch → inject new model instructions
- Permission change → inject new sandbox/approval instructions
- Collaboration mode change → inject `<collaboration_mode>` message
- Personality change → inject `<personality_spec>` message

---

## 9. History Rollback

```rust
pub fn drop_last_n_user_turns(n: usize) {
    // Remove last N user messages from history
    // Preserve initial context (system setup)
    // Emit ThreadRolledBack event
}
```

This enables the "undo" flow — user can roll back failed or unwanted turns
while preserving the conversation foundation.

---

## 10. Pending State Management

### TurnState (Active Turn Buffers)

```rust
pub struct TurnState {
    pub pending_input: Vec<ResponseInputItem>,    // Queued user messages
    pub pending_approvals: Vec<PendingApproval>,  // Waiting approval decisions
    pub pending_tool_calls: Vec<ToolCall>,         // Queued tool executions
    // ...
}
```

During an active turn, multiple types of work can be pending simultaneously:
- User input that arrived during execution
- Approval requests waiting for user/guardian response
- Tool calls awaiting execution

### Drain Pattern

```rust
// At start of each turn loop iteration:
let pending = std::mem::take(&mut turn_state.pending_input);
for input in pending {
    // Process each queued input
}
```

`std::mem::take` atomically swaps the buffer with an empty vec,
ensuring no race conditions with concurrent pushers.

---

## 11. Key Guarantees & Constraints

| Guarantee | Mechanism |
|---|---|
| Messages never lost | `pending_input` buffer + drain pattern |
| Chronological order | Append-only history vec |
| One task per turn | `abort_all_tasks(Replaced)` before spawn |
| Abort visibility | `<turn_aborted>` marker in history |
| Tool call pairing | Remove call → must remove output |
| Context fit | Auto-compaction on token limit hit |
| Durable history | Event → RolloutItem persistence |
| Rollback safety | `drop_last_n_user_turns` preserves foundation |

---

## 12. State Machine: Turn Lifecycle

```
                    ┌──────────────┐
                    │   IDLE       │
                    │ (no active   │
                    │  turn)       │
                    └──────┬───────┘
                           │ Op::UserInput
                           ▼
                    ┌──────────────┐
                    │  RUNNING     │◄──── steer_input() queues
                    │ (task active)│      additional messages
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
       ┌──────────┐ ┌──────────┐ ┌──────────┐
       │ COMPLETE │ │ ABORTED  │ │  ERROR   │
       │          │ │          │ │          │
       └──────┬───┘ └──────┬───┘ └──────┬───┘
              │            │            │
              └────────────┴────────────┘
                           │
                    ┌──────▼───────┐
                    │   IDLE       │ ← drain pending_input
                    │              │   spawn new task if needed
                    └──────────────┘
```

---

## Key Files

| File | Purpose |
|---|---|
| `core/src/codex.rs` | Main loop, steer_input, turn lifecycle |
| `core/src/state/turn.rs` | `ActiveTurn`, `TurnState`, pending buffers |
| `core/src/state/session.rs` | `SessionState`, history storage |
| `core/src/context_manager/history.rs` | `ContextManager`, token counting |
| `core/src/memories/` | Phase 1 & 2 memory system |
| `core/src/tasks/mod.rs` | Task spawning, abort handling |
| `protocol/src/protocol.rs` | `Op`, `EventMsg` definitions |
| `core/src/contextual_user_message.rs` | XML tag markers |
