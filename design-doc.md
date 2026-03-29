# Beaver Builder -- Design Document

## 1. System Overview

Beaver Builder is a coding agent harness that orchestrates AI agents through a structured pipeline. Each task progresses through well-defined stages -- from intent clarification to deployment -- with human oversight at key checkpoints.

### Goals

- **Structured autonomy**: Agents operate independently within bounded stages, but humans control transitions
- **Observable progress**: Every agent action produces events visible in real time via WebSocket
- **Review-loop quality**: The Coder/Reviewer loop catches defects before human review
- **Workspace isolation**: Each task operates in its own git worktree, preventing cross-contamination

### Non-Goals (for v1)

- Multi-tenant / multi-user (single operator assumed)
- Parallel pipeline execution within a single task
- Custom stage definitions (pipeline shape is fixed)

---

## 2. Domain Model

### 2.1 Bounded Contexts

```
+---------------------------+       +---------------------+
|   Pipeline Orchestration  |       |  Workspace Mgmt     |
|   (Core Domain)           |<----->|  (Supporting)        |
|                           |       |                     |
|  - Task                   |       |  - Workspace        |
|  - Pipeline (state machine)|      |  - Worktree         |
|  - Stage, Transition      |       |  - FileArtifact     |
+---------------------------+       +---------------------+
            |                                |
            v                                v
+---------------------------+       +---------------------+
|   Agent Execution         |       |  Protocol / Comms   |
|   (Supporting)            |       |  (Infrastructure)   |
|                           |       |                     |
|  - Agent config           |       |  - Ops (commands)   |
|  - Tool registry          |       |  - Events (facts)   |
|  - LLM interaction        |       |  - WS transport     |
+---------------------------+       +---------------------+
```

### 2.2 Aggregates & Entities

**Task** (Aggregate Root)
- Identity: `TaskId` (ULID)
- Contains: title, spec (from Intent Clarifier), priority, created_at
- References: `WorkspaceId`, `PipelineId`
- Invariant: A task always has exactly one pipeline

**Pipeline** (Aggregate Root)
- Identity: `PipelineId` (ULID)
- Contains: current `Stage`, transition history, review iteration count
- Invariant: Transitions must follow the state machine rules
- Invariant: Coder/Reviewer loop capped at `MAX_REVIEW_ITERATIONS` (default: 3)

**Workspace** (Aggregate Root)
- Identity: `WorkspaceId` (ULID)
- Contains: name, linked repos, swimlane config
- Has many: `Worktree` entities

**Worktree** (Entity, child of Workspace)
- Identity: `WorktreeId`
- Contains: branch name, status, file manifest

### 2.3 Value Objects

- `Stage` -- enum of pipeline stages
- `Transition` -- (from_stage, to_stage, timestamp, reason)
- `AgentConfig` -- model name, temperature, system prompt template, max tokens
- `ToolDef` -- tool name, description, parameter schema
- `FileArtifact` -- name, type, size, authoring agent
- `ReviewVerdict` -- Approved | RequestChanges(reason)
- `Priority` -- Low | Medium | High | Critical

### 2.4 Domain Events

- `TaskCreated { task_id, workspace_id }`
- `PipelineAdvanced { pipeline_id, from, to }`
- `PipelineReverted { pipeline_id, from, to, reason }`
- `AgentOutputProduced { pipeline_id, stage, content }`
- `ToolExecuted { pipeline_id, tool_name, result }`
- `ReviewSubmitted { pipeline_id, verdict }`
- `ApprovalRequired { pipeline_id, task_id }`
- `DeployCompleted { pipeline_id, environment }`
- `PipelineCompleted { pipeline_id, task_id }`

---

## 3. Pipeline State Machine

```
                    +-----------+
                    |  Created  |
                    +-----+-----+
                          |
                          v
                +------------------+
                | IntentClarifier  |<--- user dialog
                +--------+---------+
                         |  spec finalized
                         v
                  +--------------+
                  |  InitAgent   |
                  +------+-------+
                         |  scaffold done
                         v
                  +--------------+
                  |   Planner    |
                  +------+-------+
                         |  plan approved
                         v
                  +--------------+
              +-->|    Coder     |
              |   +------+-------+
              |          |  impl done
              |          v
              |   +--------------+
              +---|   Reviewer   |  (iteration < MAX)
              ^   +------+-------+
              |          |  approved
              |          v
   revert     |   +--------------+
   (rejected) +---|  HumanReview |
                  +------+-------+
                         |  approved
                         v
                  +--------------+
                  |    Deploy    |
                  +------+-------+
                         |  deployed
                         v
                  +--------------+
                  |     Push     |
                  +------+-------+
                         |
                         v
                  +--------------+
                  |   Completed  |
                  +--------------+
```

### Transition Rules

| From            | To              | Trigger                        | Guard                           |
|-----------------|-----------------|--------------------------------|---------------------------------|
| Created         | IntentClarifier | StartPipeline op               | Task has workspace              |
| IntentClarifier | InitAgent       | Spec finalized (agent event)   | Spec is non-empty               |
| InitAgent       | Planner         | Scaffold complete              | Required files exist            |
| Planner         | Coder           | Plan approved                  | Design doc produced             |
| Coder           | Reviewer        | Implementation complete        | At least one file changed       |
| Reviewer        | Coder           | RequestChanges verdict         | iteration < MAX_REVIEW_ITERS    |
| Reviewer        | HumanReview     | Approved verdict               | --                              |
| HumanReview     | Coder           | Human rejects                  | --                              |
| HumanReview     | Deploy          | Human approves                 | --                              |
| Deploy          | Push            | Deploy succeeds                | Health check passes             |
| Push            | Completed       | Push succeeds                  | Remote confirms                 |
| *any*           | Failed          | Unrecoverable error            | --                              |

### Coder/Reviewer Loop

The loop tracks `review_iteration: u8`. Each Coder->Reviewer transition increments it. When the Reviewer approves, the count resets. If `review_iteration >= MAX_REVIEW_ITERATIONS`, the pipeline auto-transitions to HumanReview with a warning that the review loop was exhausted.

---

## 4. Backend Architecture (Rust)

### 4.1 Module Structure

```
backend/src/
  main.rs                    -- tokio entry, server bootstrap
  domain/
    mod.rs                   -- re-exports
    pipeline.rs              -- Pipeline aggregate, Stage enum, transition logic
    task.rs                  -- Task aggregate
    workspace.rs             -- Workspace aggregate, Worktree entity
    agent.rs                 -- AgentConfig, AgentRole
    tool.rs                  -- ToolDef, ToolResult value objects
  application/
    mod.rs                   -- re-exports
    orchestrator.rs          -- PipelineOrchestrator: drives the state machine
    handlers.rs              -- Op -> side-effects -> Event mapping
  infrastructure/
    mod.rs                   -- re-exports
    ws_server.rs             -- axum WebSocket server, connection management
    llm_client.rs            -- HTTP client for LLM APIs (OpenAI-compatible)
    git_ops.rs               -- git worktree, branch, commit, push operations
    fs_ops.rs                -- sandboxed file read/write
  protocol/
    mod.rs                   -- re-exports
    ops.rs                   -- Op enum (inbound commands)
    events.rs                -- Event enum (outbound facts)
    messages.rs              -- WebSocket frame types
```

### 4.2 Key Traits

```rust
/// Core domain port -- the orchestrator depends on this, not on concrete LLM clients
trait AgentExecutor: Send + Sync {
    async fn execute(&self, config: &AgentConfig, context: AgentContext) -> AgentOutput;
}

/// Tool execution -- each tool implements this
trait ToolHandler: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, params: serde_json::Value) -> ToolResult;
}

/// Repository pattern for persistence (in-memory for v1)
trait TaskRepository: Send + Sync {
    async fn save(&self, task: &Task) -> Result<()>;
    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn list_active(&self) -> Result<Vec<Task>>;
}
```

### 4.3 Submission Queue / Event Queue Pattern

Inspired by Codex architecture:

```
   Client (WS)          Backend
   -----------          -------
       |                   |
       |--- Op (JSON) ---->|  Submission Queue (mpsc channel)
       |                   |
       |                   |  Orchestrator reads from SQ
       |                   |  Validates + executes
       |                   |  Produces Events
       |                   |
       |<-- Event (JSON) --|  Event Queue (broadcast channel)
       |                   |
```

- **Submission Queue**: `tokio::sync::mpsc` -- serializes all mutations through the orchestrator
- **Event Queue**: `tokio::sync::broadcast` -- fans out events to all connected WS clients
- The orchestrator is the **single writer** to domain state. This eliminates concurrency hazards on the state machine without locks.

### 4.4 Agent Execution Flow

When the pipeline enters a stage, the orchestrator:

1. Looks up the `AgentConfig` for that stage (model, system prompt, tools)
2. Builds an `AgentContext` (task spec, workspace path, prior stage outputs)
3. Calls `AgentExecutor::execute()` in a spawned task
4. The agent streams tokens -> `AgentOutput` events published to EQ
5. If the agent calls tools, `ToolHandler::execute()` runs in a sandbox
6. On completion, the orchestrator evaluates the output and determines the next transition

---

## 5. Frontend Architecture (React + Vite)

### 5.1 Component Tree

```
App
  Navbar                          -- nav links, "Initialize Session" button
  (route-based views)
    PipelineDashboard
      PipelineCard                -- per-task card with stage progress bar
      TaskDetail                  -- spec, live telemetry, logs
    WorkspaceView
      WorkspaceList               -- sidebar list of workspaces
      WorkspaceDetail             -- repos, swimlane, worktree selector
        WorktreeExplorer          -- file list for selected worktree
    IntentChat
      MessageBubble               -- user/agent message
      SpecCard                    -- generated spec with deploy button
  StatusBar                       -- fixed bottom bar, connection status
```

### 5.2 State Management (Zustand)

```typescript
interface BeaverStore {
  // Connection
  connected: boolean;

  // Pipeline
  tasks: Task[];
  selectedTaskId: string | null;

  // Workspace
  workspaces: Workspace[];
  selectedWorkspaceId: string | null;
  activeWorktreeId: string | null;

  // Chat
  messages: ChatMessage[];
  generatedSpec: Spec | null;

  // Actions
  handleEvent(event: Event): void;
  sendOp(op: Op): void;
}
```

The store has a single `handleEvent` dispatcher that pattern-matches on `Event.type` and updates the relevant slice. This keeps event handling centralized and testable.

### 5.3 WebSocket Protocol

- Transport: native WebSocket to `ws://localhost:3001/ws`
- Frames: JSON-encoded `{ type: "op" | "event", payload: Op | Event }`
- Reconnection: exponential backoff (1s, 2s, 4s, max 30s)
- Heartbeat: client sends ping every 30s, server responds with pong

### 5.4 Design System

Derived from the Gemini reference:

- Background: `#07080a` (near-black)
- Surface: `slate-900/40` with `border-slate-800`
- Primary accent: `indigo-500` / `indigo-600`
- Text: `slate-200` (body), `slate-400` (secondary), `slate-600` (muted)
- Active states: indigo glow (`ring-indigo-500/20`, `shadow-indigo-500/30`)
- Status colors: emerald (live), amber (processing), slate (idle)
- Typography: system sans-serif, monospace for IDs and code
- Radius: `rounded-xl` for cards, `rounded-2xl` for containers, `rounded-full` for pills

---

## 6. API Contracts

### 6.1 Ops (Client -> Server)

| Op                  | Payload                                    | Description                     |
|---------------------|--------------------------------------------|---------------------------------|
| `UserMessage`       | `{ task_id, content }`                     | Chat message to Intent Clarifier|
| `StartPipeline`     | `{ task_id, workspace_id }`                | Begin pipeline execution        |
| `AdvanceStage`      | `{ pipeline_id }`                          | Manual stage advance (admin)    |
| `RevertStage`       | `{ pipeline_id, reason }`                  | Send back to previous stage     |
| `ApproveHumanReview`| `{ pipeline_id }`                          | Human approves at gate          |
| `RejectHumanReview` | `{ pipeline_id, reason }`                  | Human rejects, loops to Coder   |
| `Deploy`            | `{ pipeline_id, environment }`             | Trigger deployment              |
| `Push`              | `{ pipeline_id, remote, branch }`          | Push to remote                  |
| `InterruptPipeline` | `{ pipeline_id }`                          | Halt current agent execution    |

### 6.2 Events (Server -> Client)

| Event               | Payload                                         | Description                     |
|---------------------|--------------------------------------------------|---------------------------------|
| `PipelineCreated`   | `{ pipeline_id, task_id, stage: "Created" }`     | New pipeline registered         |
| `StageTransition`   | `{ pipeline_id, from, to, timestamp }`           | Stage changed                   |
| `AgentOutput`       | `{ pipeline_id, stage, delta, is_final }`        | Streaming agent text            |
| `ToolExecution`     | `{ pipeline_id, tool, params, result, duration }`| Tool was called                 |
| `ApprovalRequired`  | `{ pipeline_id, task_id, summary }`              | Human review gate reached       |
| `ReviewSubmitted`   | `{ pipeline_id, verdict, iteration }`            | Reviewer verdict                |
| `DeployStatus`      | `{ pipeline_id, status, url }`                   | Deploy progress/result          |
| `PushComplete`      | `{ pipeline_id, remote, sha }`                   | Push succeeded                  |
| `Error`             | `{ pipeline_id?, code, message }`                | Error occurred                  |
| `Warning`           | `{ pipeline_id?, message }`                      | Non-fatal warning               |

---

## 7. Tool System

### 7.1 ToolHandler Trait

Each pipeline stage has access to a curated set of tools:

| Stage           | Tools                                           |
|-----------------|--------------------------------------------------|
| IntentClarifier | (none -- pure dialog)                            |
| InitAgent       | `create_file`, `create_dir`, `git_init`          |
| Planner         | `read_file`, `list_dir`, `write_file`            |
| Coder           | `read_file`, `write_file`, `exec_command`, `grep`|
| Reviewer        | `read_file`, `list_dir`, `grep`, `git_diff`      |
| Deploy          | `exec_command`, `health_check`                   |
| Push            | `git_push`                                       |

### 7.2 Sandbox

All tool execution is scoped to the task's worktree directory. The `exec_command` tool runs in a restricted subprocess with:
- Working directory locked to worktree path
- Network access limited (no outbound except deploy targets)
- Timeout per execution (30s default)
- No access to parent directories

### 7.3 Tool Registry

```rust
struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    fn tools_for_stage(&self, stage: &Stage) -> Vec<Arc<dyn ToolHandler>> { ... }
}
```

---

## 8. Agent Abstraction

Each pipeline stage maps to an agent configuration:

```rust
struct AgentConfig {
    role: AgentRole,          // enum matching Stage
    model: String,            // e.g. "claude-sonnet-4-20250514", "gpt-4o-mini"
    system_prompt: String,    // stage-specific prompt template
    temperature: f32,
    max_tokens: u32,
    tools: Vec<String>,       // tool names this agent can use
}
```

### Model Assignment Strategy

| Stage           | Model Tier     | Rationale                                    |
|-----------------|--------------- |----------------------------------------------|
| IntentClarifier | Mid (Sonnet)   | Needs good dialog, not heavy reasoning       |
| InitAgent       | Cheap (Haiku)  | Mechanical scaffolding                       |
| Planner         | Strong (Opus)  | Architecture decisions need deep reasoning   |
| Coder           | Mid (Sonnet)   | Fast, capable, cost-effective for code gen   |
| Reviewer        | Strong (Opus)  | Catch subtle issues, architectural coherence |
| Deploy          | Cheap (Haiku)  | Scripted operations                          |
| Push            | Cheap (Haiku)  | Scripted operations                          |

### System Prompt Templates

Each agent's system prompt follows a structure:
1. Role declaration ("You are the Reviewer agent for Beaver Builder...")
2. Context injection (task spec, prior stage outputs, workspace state)
3. Behavioral constraints (what tools are available, what to produce)
4. Output format expectations (structured output for machine consumption)

---

## 9. Deployment & Development

### Dev Setup

```bash
# Backend
cd backend && cargo run

# Frontend
cd frontend && bun install && bun run dev
```

### Architecture Decision Records

- **ADR-001**: Single-writer orchestrator over actor model -- simpler to reason about, sufficient for single-operator use
- **ADR-002**: In-memory state for v1 -- SQLite persistence planned for v2
- **ADR-003**: axum over actix-web -- better tokio ecosystem integration, tower middleware
- **ADR-004**: Zustand over Redux -- less boilerplate, sufficient for our state shape
- **ADR-005**: JSON over protobuf for WS protocol -- simpler debugging, performance is not the bottleneck
