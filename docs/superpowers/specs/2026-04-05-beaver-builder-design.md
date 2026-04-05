# Beaver Builder — Design Document

## Overview

Beaver Builder is an AI coding agent pipeline harness. It orchestrates multiple LLM agents through a 7-stage workflow with a back-and-forth review loop. The Planner stage drives an interactive brainstorm dialog with the user to produce a design-doc before any code is written.

**Tech stack**: Rust backend (axum, tokio, serde) + React frontend (Vite, TailwindCSS, Zustand).
**LLM provider**: OpenAI (gpt-5.x models).
**Architecture**: Single-writer Submission Queue / Event Queue pattern, inspired by OpenAI Codex.

---

## Pipeline State Machine

### 7 Stages

| # | Stage | Model | Responsibility |
|---|-------|-------|---------------|
| 1 | Planner | gpt-5.4 | Interactive brainstorm dialog → design-doc (blocks until user approves) |
| 2 | Init Agent | gpt-5.3-codex | Scaffold README, CHANGELOG, specs/, worktree |
| 3 | Coder | gpt-5.3-codex | Implement the plan |
| 4 | Reviewer | gpt-5.4 | Architecture + code quality review (can reject → Coder) |
| 5 | Human Review | N/A | Manual approval gate with diff view |
| 6 | Deploy | gpt-5.4 | Deploy to test environment |
| 7 | Push | gpt-5.4 | Push to remote, task complete |

### Transition Rules

- **Happy path**: Created → Planner → Init → Coder → Reviewer → HumanReview → Deploy → Push → Completed
- **Review reject**: Reviewer → Coder (increments counter, max 3 iterations)
- **Auto-escalate**: After 3 rejections, Reviewer → HumanReview + Warning event
- **Human reject**: HumanReview → Coder (resets review counter to 0)
- **Fail**: Any non-terminal stage → Failed
- **Planner blocks**: Pipeline waits at Planner until user approves design-doc via interactive dialog

### Stage Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Created,
    Planner,
    InitAgent,
    Coder,
    Reviewer,
    HumanReview,
    Deploy,
    Push,
    Completed,
    Failed,
}
```

---

## Domain Model (DDD)

### Pipeline (Aggregate Root)

The single source of truth for pipeline state. All transitions are validated within this aggregate.

```
Pipeline {
    id: String,
    task_id: String,
    current_stage: Stage,
    review_iterations: u8,       // caps at 3
    transitions: Vec<Transition>,
    created_at: DateTime<Utc>,
}
```

Methods: `advance()`, `revert_to_coder(reason)`, `force_human_review()`, `fail(reason)`, `current_stage()`.

### Task

Links a user request to a pipeline.

```
Task {
    id: String,
    title: String,
    spec: String,
    workspace_id: String,
    pipeline_id: Option<String>,
}
```

### AgentConfig

Per-stage LLM configuration. `AgentConfig::for_stage(stage)` returns model, system prompt, temperature, max tokens, and available tools.

---

## Protocol

### Op Enum (Client → Server) — 8 variants

| Op | Payload |
|----|---------|
| UserMessage | task_id, content |
| StartPipeline | task_id, workspace_id |
| AdvanceStage | pipeline_id |
| RevertStage | pipeline_id, reason |
| ApproveHumanReview | pipeline_id |
| RejectHumanReview | pipeline_id, reason |
| Deploy | pipeline_id, environment |
| InterruptPipeline | pipeline_id |

### Event Enum (Server → Client) — 10 variants

| Event | Payload |
|-------|---------|
| PipelineCreated | pipeline_id, task_id, stage |
| StageTransition | pipeline_id, from: Stage, to: Stage, timestamp |
| AgentOutput | pipeline_id, stage: Stage, delta, is_final |
| ToolExecution | pipeline_id, tool, params, result, duration_ms |
| ApprovalRequired | pipeline_id, task_id, summary |
| ReviewSubmitted | pipeline_id, verdict, iteration |
| DeployStatus | pipeline_id, status, url |
| PushComplete | pipeline_id, remote, sha |
| Error | pipeline_id, code, message |
| Warning | pipeline_id, message |

### WebSocket Envelope

```json
{ "kind": "op" | "event", "payload": { "type": "VariantName", "payload": { ... } } }
```

All Stage fields use serde `snake_case` serialization. No `format!("{:?}")`.

---

## Backend Architecture

### Single-Writer SQ/EQ

```
Frontend → WebSocket Server → mpsc (SQ) → Orchestrator → broadcast (EQ) → WebSocket Server → Frontend
```

- **Submission Queue**: `mpsc::channel<Op>` — clients submit ops, orchestrator is the sole consumer.
- **Event Queue**: `broadcast::channel<Event>` — orchestrator publishes, all WS connections subscribe.
- **Orchestrator**: Single async loop. Reads ops, validates transitions via Pipeline aggregate, calls `Arc<dyn LlmProvider>`, emits events. No locks, no races. Provider-agnostic — never imports a concrete LLM client.

### Module Structure

```
backend/src/
  domain/
    mod.rs
    pipeline.rs     // Pipeline aggregate root + state machine + tests
    task.rs          // Task aggregate
    agent.rs         // AgentConfig::for_stage()
  application/
    mod.rs
    orchestrator.rs  // Single-writer SQ/EQ loop
  llm/
    mod.rs
    provider.rs      // LlmProvider trait, LlmRequest, LlmResponse, LlmError
    openai.rs        // OpenAiProvider (OPENAI_API_KEY, retries, streaming)
    factory.rs       // LlmProviderFactory::from_env()
  infrastructure/
    mod.rs
    ws_server.rs     // axum WebSocket handler
    git_ops.rs       // git CLI wrapper (init, commit, push, worktree)
    fs_ops.rs        // Sandboxed file operations (scaffold, read, write)
  protocol/
    mod.rs
    ops.rs           // Op enum
    events.rs        // Event enum
    messages.rs      // WsMessage envelope
  main.rs            // Tracing, orchestrator, axum server
  lib.rs             // Expose modules for integration tests
```

### LLM Abstraction (Provider-agnostic)

The LLM layer is a separate domain boundary — the orchestrator depends on a trait, not a concrete client. Switching from OpenAI to Anthropic (or any provider) should require only a new trait implementation, no orchestrator changes.

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
    async fn chat_stream(&self, request: LlmRequest) -> Result<Receiver<StreamChunk>, LlmError>;
}
```

- **`OpenAiProvider`**: Default implementation. Reads `OPENAI_API_KEY` from env, base URL defaults to `https://api.openai.com`.
- **`AnthropicProvider`**: Future implementation (same trait, different API mapping).
- **Provider selection**: `LlmProviderFactory::from_env()` checks `LLM_PROVIDER` env var (`openai` | `anthropic`), defaults to `openai`.
- **Orchestrator** holds `Arc<dyn LlmProvider>` — never imports a concrete provider.
- Per-pipeline conversation history maintained in `HashMap<String, Vec<LlmMessage>>`.
- Retry with exponential backoff (3 attempts, retryable on 429/5xx/timeout) — implemented in each provider.

---

## Frontend Architecture

### 4 Views

1. **Pipeline Dashboard** — Task cards with 7-stage progress bars, live telemetry panel. Click a task to see spec + agent logs.
2. **Planner Chat** — Brainstorm-style dialog with the Planner agent. Interactive: one question at a time. Produces spec card with "Deploy Pipeline" action.
3. **Review** (new) — Human review view. Shows agent review summary, files changed with diff stats, APPROVE/REJECT buttons. Wired to `ApproveHumanReview` / `RejectHumanReview` ops.
4. **Workspaces** — Sidebar workspace list, detail pane with repos/swimlane config, worktree file explorer.

### Component Tree

```
frontend/src/
  components/
    Pipeline/PipelineCard.tsx, StageIndicator.tsx
    Chat/PlannerChat.tsx, MessageBubble.tsx, SpecCard.tsx
    Review/ReviewPanel.tsx, DiffView.tsx, ApprovalActions.tsx
    Workspace/WorkspaceList.tsx, WorkspaceDetail.tsx, WorktreeExplorer.tsx
    Layout/Navbar.tsx, StatusBar.tsx
  hooks/useWebSocket.ts, usePipeline.ts
  store/index.ts       // Zustand — handles ALL 10 event types
  types/index.ts       // Mirrors Rust protocol exactly
```

### State Management

Zustand store with:
- `sendOp`: registered by `useWebSocket` hook, available to all components.
- `handleEvent`: switch on all 10 event types, updates tasks/logs/status.
- `connected`: WebSocket connection state.
- Per-view state: tasks, workspaces, messages, selectedTaskId, etc.

### Styling

- Dark theme: `bg-[#07080a]`, slate-900 panels, indigo-500 accents.
- TailwindCSS with custom config.
- lucide-react for icons.
- Reference: `gemini-web-page.html`.

---

## Testing Strategy

### Unit Tests (in backend)
- Pipeline state machine: all valid transitions, invalid transitions, review loop cap, auto-escalation, human rejection reset.
- Protocol serialization: Op/Event round-trip JSON with serde.

### Integration Tests (in backend/tests/)
- Orchestrator: StartPipeline, AdvanceStage, RevertStage, review loop, InterruptPipeline.
- WebSocket: connect, send Op, receive Event.
- LLM integration: mock server test for UserMessage → AgentOutput chain.

### Frontend Tests
- Zustand store: handleEvent for all 10 event types.
- Vitest as test runner.

### E2E Tests (browser)
Using Chrome DevTools MCP tools (navigate, click, fill, screenshot, evaluate_script, wait_for):

1. **Happy Path**: Open Planner Chat → send message → deploy pipeline → advance all stages → verify COMPLETED.
2. **Review Loop**: Advance to Reviewer → revert 3x → verify auto-escalation to HumanReview.
3. **Human Rejection**: At HumanReview → reject → coder fixes → reviewer approves → human approves → complete.

---

## Anti-Patterns (from v1 lessons)

- No static mockups — all UI interactions through WebSocket.
- No `format!("{:?}")` for serialization — use serde `snake_case`.
- No hardcoded model names — use `AgentConfig::for_stage()`.
- No `unwrap()` in production — proper Result types.
- No skipping LLM integration — UserMessage must call OpenAI.
- Tester owns test fixtures — Coder does not create test data.
- Zustand store handles ALL 10 event types — no silent drops.
