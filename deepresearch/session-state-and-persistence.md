# Session Management, State & Persistence

Deep-dive into how Codex manages sessions, persists state, implements checkpoints
via rollout files, and enables session resumption and recovery.

---

## 1. Dual-Layer Architecture

Codex uses a **two-tier state management** system:

```
┌─────────────────────────────────────────────────────────────┐
│                    IN-MEMORY (Runtime)                        │
│                                                              │
│  Session ─── SessionState ─── ContextManager (history)       │
│     │                              │                         │
│  ActiveTurn ─── TurnState          │                         │
│     │          (pending_approvals, │                         │
│     │           pending_input)     │                         │
│     │                              │                         │
│  TurnContext (immutable snapshot)   │                         │
│                                    │                         │
├────────────────────────────────────┼─────────────────────────┤
│                    ON-DISK (Persistent)                       │
│                                    │                         │
│  Rollout JSONL ◄───────────────────┘ (event source of truth) │
│  ~/.codex/sessions/YYYY/MM/DD/rollout-{ts}-{thread_id}.jsonl │
│                                                              │
│  SQLite State DB ◄──── indexed metadata for fast lookup      │
│  ~/.codex/state_5.sqlite  (threads, jobs, memories)          │
│                                                              │
│  SQLite Logs DB ◄──── application logs                       │
│  ~/.codex/logs_1.sqlite   (structured tracing)               │
│                                                              │
│  Config Files ◄──── user/project settings                    │
│  ~/.codex/config.toml, .codex/config.toml                    │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Rollout System (Event Source of Truth)

### File Location & Naming

```
~/.codex/sessions/
    └── 2026/
        └── 03/
            └── 08/
                └── rollout-2026-03-08T14-30-00-5973b6c0-94b8-487b-a530-2aeb6098ae0e.jsonl
```

Pattern: `rollout-{YYYY-MM-DDThh-mm-ss}-{thread_id}.jsonl`

Archived sessions move to `~/.codex/archived_sessions/`.

### Format: JSONL (Newline-Delimited JSON)

Each line is a self-contained JSON object with a timestamp:

```rust
pub struct RolloutLine {
    pub timestamp: String,       // RFC3339: "2026-03-08T14:30:00.123Z"
    #[serde(flatten)]
    pub item: RolloutItem,       // One of the variants below
}
```

### RolloutItem Variants

```rust
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RolloutItem {
    SessionMeta(SessionMetaLine),    // First item — session identity
    ResponseItem(ResponseItem),      // Messages, tool calls, outputs
    Compacted(CompactedItem),        // Checkpoint with replacement history
    TurnContext(TurnContextItem),    // Turn-level config snapshot
    EventMsg(EventMsg),              // Stream events (turns, tokens, etc.)
}
```

### Example JSONL Content

```json
{"timestamp":"2026-03-08T14:30:00.123Z","type":"session_meta","payload":{"id":"5973b6c0-...","cwd":"/home/user/project","originator":"cli","cli_version":"0.15.0","source":"cli","model_provider":"openai"}}
{"timestamp":"2026-03-08T14:30:01.234Z","type":"turn_context","payload":{"turn_id":"turn-1","cwd":"/home/user/project","model":"gpt-5","sandbox_policy":"workspace-write","approval_policy":"on-request"}}
{"timestamp":"2026-03-08T14:30:02.345Z","type":"event_msg","payload":{"type":"user_message","message":"Fix the bug in main.rs"}}
{"timestamp":"2026-03-08T14:30:05.456Z","type":"response_item","payload":{"type":"agent_message","content":"I'll look into main.rs..."}}
{"timestamp":"2026-03-08T14:30:10.567Z","type":"response_item","payload":{"type":"local_shell_call","command":"cat main.rs"}}
{"timestamp":"2026-03-08T14:30:15.678Z","type":"compacted","payload":{"message":"Context compacted","replacement_history":[...]}}
{"timestamp":"2026-03-08T14:30:20.789Z","type":"event_msg","payload":{"type":"turn_complete"}}
```

---

## 3. SessionMeta: Session Identity

The first item in every rollout file:

```rust
pub struct SessionMeta {
    pub id: ThreadId,                       // UUID v7
    pub forked_from_id: Option<ThreadId>,   // If forked from another session
    pub timestamp: String,                  // Session start time (RFC3339)
    pub cwd: PathBuf,                       // Working directory
    pub originator: String,                 // App that created the session
    pub cli_version: String,                // Version of codex CLI
    pub source: SessionSource,              // Cli, VSCode, Unknown, SubAgent
    pub agent_nickname: Option<String>,     // For multi-agent orchestration
    pub agent_role: Option<String>,
    pub model_provider: Option<String>,     // openai, anthropic, etc.
    pub base_instructions: Option<BaseInstructions>,  // System prompt
    pub dynamic_tools: Option<Vec<DynamicToolSpec>>,  // Custom tools
    pub memory_mode: Option<String>,        // "enabled" or "disabled"
}
```

Also wrapped in `SessionMetaLine` which includes optional `GitInfo`
(sha, branch, origin_url).

---

## 4. Checkpoints: CompactedItem

The key to efficient session resumption:

```rust
pub struct CompactedItem {
    pub message: String,                            // Human-readable summary
    pub replacement_history: Option<Vec<ResponseItem>>,  // FULL HISTORY CHECKPOINT
}
```

When `replacement_history` is present, it's a **complete replacement** of the
conversation history up to that point. This allows recovery without replaying
the entire rollout from the beginning.

### When Checkpoints Are Written

- **Auto-compaction**: When token limit is hit mid-turn
- **Manual compaction**: User-triggered context compression
- **After compaction**: A new `TurnContextItem` is also emitted to re-establish baseline

---

## 5. TurnContextItem: Turn Snapshots

Persisted once per real user turn (and again after compaction):

```rust
pub struct TurnContextItem {
    pub turn_id: Option<String>,
    pub trace_id: Option<String>,
    pub cwd: PathBuf,
    pub current_date: Option<String>,
    pub timezone: Option<String>,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub network: Option<TurnContextNetworkItem>,
    pub model: String,
    pub personality: Option<Personality>,
    pub collaboration_mode: Option<CollaborationMode>,
    pub realtime_active: Option<bool>,
    pub effort: Option<ReasoningEffortConfig>,
    pub summary: ReasoningSummaryConfig,
    pub user_instructions: Option<String>,
    pub developer_instructions: Option<String>,
    pub final_output_json_schema: Option<Value>,
    pub truncation_policy: Option<TruncationPolicy>,
}
```

These snapshots capture **everything the model saw** at each turn boundary,
enabling exact reconstruction of model-visible context.

---

## 6. Event Persistence Policy

Two modes control what gets written to rollout:

### Limited Mode (Default)

Persists only what's needed for reconstruction:
- UserMessage, AgentMessage, AgentReasoning
- TokenCount, ContextCompacted
- Turn lifecycle (TurnStarted, TurnComplete, TurnAborted)
- ThreadRolledBack, UndoCompleted
- ResponseItems (Message, FunctionCall, FunctionCallOutput, etc.)

### Extended Mode

Adds detailed execution data (with size limits):
- ExecCommandEnd (aggregated output capped at **10KB**)
- PatchApplyEnd, McpToolCallEnd
- Error events, web search results
- Collaboration agent spawn/interaction events

Sanitization: `ExecCommandEnd` has stdout/stderr cleared; only aggregated_output
is kept (max 10,000 bytes) to prevent unbounded rollout growth.

---

## 7. RolloutRecorder: Write Pipeline

### Architecture: Async Actor Pattern

```rust
pub struct RolloutRecorder {
    tx: Sender<RolloutCmd>,               // Channel to writer task
    pub(crate) rollout_path: PathBuf,     // File path
    state_db: Option<StateDbHandle>,      // SQLite handle
    event_persistence_mode: EventPersistenceMode,
}

enum RolloutCmd {
    AddItems(Vec<RolloutItem>),
    Persist { ack: oneshot::Sender<()> },
    Flush { ack: oneshot::Sender<()> },
    Shutdown { ack: oneshot::Sender<()> },
}
```

### Write Flow

```
record_items(&[RolloutItem])
    │
    ▼
Filter by EventPersistenceMode (Limited/Extended)
    │
    ▼
Sanitize large outputs (10KB cap)
    │
    ▼
Send RolloutCmd::AddItems to writer task (bounded channel: 256)
    │
    ▼
Writer task: serde_json::to_string() + "\n"
    │
    ▼
tokio::fs::File::write_all() + flush()   ← durable per-item
    │
    ▼
sync_thread_state_after_write()           ← update SQLite index
```

### Lazy File Materialization

New sessions **defer file creation** until first `persist()`:
- Pre-compute path at creation time
- Buffer items in memory
- On first `persist()`: create directories, open file, write SessionMeta, flush buffer

Resume sessions open the file immediately in append mode.

### Durability Guarantee

Every individual item write calls `file.flush().await` — no batch buffering.
Items are durable the instant `record_items()` completes.

---

## 8. History Reconstruction (Reverse Replay)

### Algorithm (`rollout_reconstruction.rs`)

Sessions are reconstructed by scanning rollout items **newest-to-oldest**:

```
Step 1: Scan backwards through rollout items
    │
    ├── Find newest Compacted with replacement_history → CHECKPOINT
    ├── Collect PreviousTurnSettings (model, realtime_active)
    ├── Collect reference TurnContextItem
    ├── Handle ThreadRolledBack events (skip dropped turns)
    │
Step 2: Build turn segments using turn_id boundaries
    │
Step 3: Replay surviving tail FORWARD from checkpoint
    │
Step 4: Return reconstructed history + metadata
```

```rust
pub(super) struct RolloutReconstruction {
    pub(super) history: Vec<ResponseItem>,
    pub(super) previous_turn_settings: Option<PreviousTurnSettings>,
    pub(super) reference_context_item: Option<TurnContextItem>,
}
```

### Why Reverse Scan?

- Avoids replaying the entire rollout (which could be millions of lines)
- Jumps to most recent checkpoint in O(n) where n = items since last checkpoint
- Handles rollback/undo events correctly by counting backwards

---

## 9. SQLite State Database

### Database Files

| File | Purpose | Journal |
|---|---|---|
| `state_5.sqlite` | Thread metadata, memories, jobs | WAL |
| `logs_1.sqlite` | Application logs | WAL |

Both use WAL (Write-Ahead Log) mode with Normal synchronous settings and
5-second busy timeout.

### Threads Table (Primary Index)

```sql
CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    rollout_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    source TEXT NOT NULL,              -- SessionSource enum
    model_provider TEXT NOT NULL,
    cwd TEXT NOT NULL,
    title TEXT NOT NULL,
    sandbox_policy TEXT NOT NULL,
    approval_mode TEXT NOT NULL,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    has_user_event INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    archived_at INTEGER,
    git_sha TEXT,
    git_branch TEXT,
    git_origin_url TEXT,
    agent_nickname TEXT,
    agent_role TEXT,
    cli_version TEXT,
    first_user_message TEXT,
    memory_mode TEXT NOT NULL DEFAULT 'enabled'
);

CREATE INDEX idx_threads_created_at ON threads(created_at DESC, id DESC);
CREATE INDEX idx_threads_updated_at ON threads(updated_at DESC, id DESC);
CREATE INDEX idx_threads_archived ON threads(archived);
CREATE INDEX idx_threads_source ON threads(source);
CREATE INDEX idx_threads_provider ON threads(model_provider);
```

### Stage1 Outputs (Memory Cache)

```sql
CREATE TABLE stage1_outputs (
    thread_id TEXT PRIMARY KEY,
    source_updated_at INTEGER NOT NULL,    -- Watermark for staleness
    raw_memory TEXT NOT NULL,              -- Extracted memory JSON
    rollout_summary TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    usage_count INTEGER,
    last_usage INTEGER,
    selected_for_phase2_source_updated_at INTEGER
);
```

### Jobs Table (Distributed Task Scheduling)

```sql
CREATE TABLE jobs (
    kind TEXT NOT NULL,            -- memory_stage1, memory_consolidate_global, etc.
    job_key TEXT NOT NULL,         -- Unique per kind
    status TEXT NOT NULL,          -- pending, running, completed, failed
    worker_id TEXT,
    ownership_token TEXT,
    started_at INTEGER,
    finished_at INTEGER,
    lease_until INTEGER,           -- Distributed lock expiration
    retry_at INTEGER,
    retry_remaining INTEGER NOT NULL,
    last_error TEXT,
    input_watermark INTEGER,
    last_success_watermark INTEGER,
    PRIMARY KEY (kind, job_key)
);
```

### Logs Table

```sql
CREATE TABLE logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,
    ts_nanos INTEGER NOT NULL,
    level TEXT NOT NULL,             -- ERROR, WARN, INFO, DEBUG, TRACE
    target TEXT NOT NULL,
    message TEXT,
    thread_id TEXT,
    process_uuid TEXT,
    module_path TEXT,
    file TEXT,
    line INTEGER,
    estimated_bytes INTEGER
);
```

**Retention**: Per-partition pruning (10 MiB + 1000-row caps per partition).

---

## 10. In-Memory State Structures

### Session (Top-Level)

```rust
pub(crate) struct Session {
    pub(crate) conversation_id: ThreadId,
    tx_event: Sender<Event>,
    agent_status: watch::Sender<AgentStatus>,
    state: Mutex<SessionState>,                    // Mutable runtime state
    pub(crate) active_turn: Mutex<Option<ActiveTurn>>,
    pub(crate) services: SessionServices,          // Shared service handles
    js_repl: Arc<JsReplHandle>,
    next_internal_sub_id: AtomicU64,
    // ...
}
```

### SessionState (Mutable Runtime)

```rust
pub(crate) struct SessionState {
    pub(crate) session_configuration: SessionConfiguration,
    pub(crate) history: ContextManager,                     // Full message history
    pub(crate) latest_rate_limits: Option<RateLimitSnapshot>,
    pub(crate) dependency_env: HashMap<String, String>,
    pub(crate) mcp_dependency_prompted: HashSet<String>,
    previous_turn_settings: Option<PreviousTurnSettings>,   // For resume
    pub(crate) active_mcp_tool_selection: Option<Vec<String>>,
    pub(crate) active_connector_selection: HashSet<String>,
    // ...
}
```

### ActiveTurn (Current Turn Only)

```rust
pub(crate) struct ActiveTurn {
    pub(crate) tasks: IndexMap<String, RunningTask>,     // Named subtasks
    pub(crate) turn_state: Arc<Mutex<TurnState>>,        // Mutable turn state
}

pub(crate) struct TurnState {
    pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pending_user_input: HashMap<String, oneshot::Sender<RequestUserInputResponse>>,
    pending_elicitations: HashMap<(String, RequestId), oneshot::Sender<ElicitationResponse>>,
    pending_dynamic_tools: HashMap<String, oneshot::Sender<DynamicToolResponse>>,
    pending_input: Vec<ResponseInputItem>,    // Queued user messages
    pub(crate) tool_calls: u64,
    pub(crate) token_usage_at_turn_start: TokenUsage,
}
```

### ContextManager (History)

```rust
pub(crate) struct ContextManager {
    items: Vec<ResponseItem>,                          // Chronological history
    token_info: Option<TokenUsageInfo>,                // Token accounting
    reference_context_item: Option<TurnContextItem>,   // Latest baseline
}
```

---

## 11. Persistence Scope: What Gets Persisted vs. What Doesn't

| Data | Scope | Persisted? | Storage |
|---|---|---|---|
| ThreadId, SessionMeta | Session | Yes | Rollout + SQLite |
| Conversation history (ResponseItems) | Session | Yes | Rollout |
| Compacted checkpoints | Session | Yes | Rollout |
| TurnContext snapshots | Per-turn | Yes | Rollout |
| Event stream (turns, tokens) | Session | Yes (filtered) | Rollout |
| Thread metadata (title, git, etc.) | Session | Yes | SQLite |
| Memory extractions (stage1) | Session | Yes | SQLite |
| Job scheduling state | Global | Yes | SQLite |
| Application logs | Global | Yes | SQLite (logs DB) |
| Config (sandbox, model, MCP) | Process | Yes | TOML files |
| ActiveTurn state | Current turn | **No** | In-memory only |
| TurnState (pending approvals) | Current turn | **No** | In-memory only |
| Rate limit snapshot | Runtime | **No** | In-memory only |
| AgentStatus | Runtime | **No** | watch channel only |
| App-server ThreadState | Per-connection | **No** | In-memory only |

---

## 12. Session Lifecycle Flows

### New Session

```
1. ThreadManager.spawn()
2. RolloutRecorder::new(Create)
   └── Pre-compute path (defer file creation)
3. Session initialization
4. First turn submitted
5. RolloutRecorder::persist()
   ├── Create directories: ~/.codex/sessions/YYYY/MM/DD/
   ├── Open file (create + append)
   ├── Write SessionMeta (first line)
   └── StateRuntime.update_thread() (insert to SQLite)
6. Turn executes → items recorded to rollout
7. Turn completes → TurnContextItem persisted
```

### Resume Session

```
1. List available sessions
   ├── Option A: SQLite query (fast, indexed)
   └── Option B: Filesystem scan (fallback)
2. User selects session
3. RolloutRecorder::new(Resume)
   └── Open existing file in append mode
4. load_rollout_items(path)
   └── Read entire JSONL, parse line-by-line
5. reconstruct_history_from_rollout()
   ├── Reverse scan for Compacted checkpoint
   ├── Collect PreviousTurnSettings
   ├── Collect reference TurnContextItem
   ├── Handle rollbacks (skip dropped turns)
   └── Forward replay surviving tail
6. SessionState restored
   ├── history ← reconstructed ResponseItems
   ├── previous_turn_settings ← from metadata
   └── reference_context_item ← latest TurnContextItem
7. New items append to existing rollout file
```

### Fork Session

```
1. Existing session running
2. User requests fork (with optional history rollback)
3. New ThreadId generated
4. RolloutRecorder::new(Create) with forked_from_id
5. SessionMeta written with forked_from_id reference
6. History reconstructed up to fork point
7. New rollout file created, SQLite updated
```

### Compaction (Mid-Turn Checkpoint)

```
1. Auto-compaction triggered (token limit hit)
2. Compute replacement_history (summarized context)
3. Emit Compacted(CompactedItem { message, replacement_history })
4. Persist to rollout file
5. SessionState.history.replace(replacement_history)
6. New TurnContextItem emitted (re-establish baseline)
7. Continue turn with smaller context
```

---

## 13. ThreadManager: Session Registry

```rust
pub(crate) struct ThreadManagerState {
    threads: Arc<RwLock<HashMap<ThreadId, Arc<CodexThread>>>>,
    thread_created_tx: broadcast::Sender<ThreadId>,
    // ... shared service handles
}
```

- Threads registered in `HashMap` keyed by `ThreadId`
- Creation broadcasted via `broadcast::Sender` to all listeners
- On resume: thread fetched, history reconstructed from rollout
- On close: thread removed (garbage collected)

---

## 14. Distributed Job Scheduling

The `jobs` table implements a distributed lock pattern for background tasks
(memory extraction, consolidation):

```rust
lease_until: INTEGER    // Current time + lease duration
retry_at: INTEGER       // Backoff for failed jobs
ownership_token: TEXT   // Unique worker identity
```

**Flow**:
1. Worker claims job: `UPDATE SET status='running', lease_until=now+60s WHERE lease_until < now`
2. Worker processes job
3. On success: `UPDATE SET status='completed', finished_at=now`
4. On failure: `UPDATE SET retry_at=now+backoff, retry_remaining=N-1, last_error=...`
5. If worker crashes: lease expires, another worker can claim

---

## 15. Recovery & Resilience

### Crash Recovery

| Scenario | Recovery Path |
|---|---|
| Process crash mid-turn | Rollout file exists → resume, rebuild from last checkpoint |
| SQLite corruption | Rollout files are source of truth → backfill SQLite from filesystem |
| Partial rollout write | JSONL is line-oriented → parse errors on last line are skipped |
| Stale job locks | `lease_until` expiration → another worker claims job |

### Guarantees

- **Per-item durability**: Every rollout write is flushed to disk
- **Append-only**: No in-place mutations (corrections are new events)
- **Graceful degradation**: Parse errors counted but don't block reconstruction
- **Event sourcing**: Full history can always be replayed from rollout alone

---

## 16. Key Design Principles

| Principle | Implementation |
|---|---|
| **Event Sourcing** | Rollout JSONL is the single source of truth |
| **Checkpoint Optimization** | Compacted items with replacement_history enable O(recent) reconstruction |
| **Dual Storage** | JSONL for completeness, SQLite for indexed queries |
| **Lazy Materialization** | Rollout files deferred until first persist |
| **Distributed Safety** | Job table with leasing for multi-process coordination |
| **Bounded Growth** | Extended mode caps output at 10KB; log retention prunes old entries |
| **Separation of Concerns** | State DB and Logs DB are separate SQLite files (independent WAL journals) |

---

## Key Files

| File | Purpose |
|---|---|
| `core/src/rollout/mod.rs` | Rollout module, path constants |
| `core/src/rollout/recorder.rs` | RolloutRecorder: write pipeline |
| `core/src/rollout/policy.rs` | Event persistence filtering |
| `core/src/codex/rollout_reconstruction.rs` | Reverse-replay reconstruction |
| `protocol/src/protocol.rs` | RolloutItem, SessionMeta, CompactedItem |
| `state/src/lib.rs` | StateRuntime: SQLite operations |
| `state/src/migrations/` | Database schema evolution |
| `core/src/state/mod.rs` | Session, SessionState |
| `core/src/state/turn.rs` | ActiveTurn, TurnState |
| `core/src/state/session.rs` | SessionConfiguration |
| `core/src/context_manager/history.rs` | ContextManager: in-memory history |
| `core/src/thread_manager.rs` | ThreadManager: session registry |
| `config/src/state.rs` | ConfigLayerStack: TOML persistence |
