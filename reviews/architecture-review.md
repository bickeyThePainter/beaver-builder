# Beaver Builder -- Architecture Review

**Date**: 2026-03-29
**Reviewer**: Architecture Review Agent
**Scope**: Full codebase (design-doc.md, backend/src/, frontend/src/)

---

## Summary Verdict: NEEDS_WORK

The architecture is fundamentally sound. The DDD layering is clean, the pipeline state machine is well-implemented with proper invariant enforcement, and the SQ/EQ pattern provides a solid concurrency model. However, there are several issues ranging from missing domain logic that leaked into the application layer, to incomplete protocol handling, to a frontend that bypasses the backend entirely for task creation. None are fatal, but they need attention before this becomes harder to refactor.

---

## Critical Findings

### C-1: Frontend creates tasks without backend involvement

**Location**: `frontend/src/components/Chat/IntentChat.tsx:54-69`

The `handleDeploy` function creates a `Task` object entirely client-side with a random ID (`TSK-${Math.floor(Math.random() * 900) + 100}`) and adds it directly to the Zustand store. It never sends an Op to the backend. This means:

- The backend has no knowledge of the task
- The `pipeline_id` is null and never gets set via a `StartPipeline` Op
- The task ID format (`TSK-xxx`) doesn't match the backend's sequential ID generation (`pl_1`, etc.)
- No `PipelineCreated` or `StageTransition` events will ever fire for these tasks

**Impact**: The entire pipeline flow is broken end-to-end. The frontend is essentially a static mockup.

**Fix**: `handleDeploy` must send a `StartPipeline` Op via WebSocket. The backend should respond with `PipelineCreated` + `StageTransition` events, and the store's `handleEvent` should create the task from those events. Remove client-side task creation entirely.

### C-2: IntentChat uses hardcoded simulated responses instead of WebSocket

**Location**: `frontend/src/components/Chat/IntentChat.tsx:33-51`

The `handleSend` function uses `setTimeout` to simulate agent responses instead of sending a `UserMessage` Op to the backend. This bypasses the entire agent execution pipeline described in the design doc.

**Fix**: Send `UserMessage` Op via `sendOp` from the `useWebSocket` hook. Listen for `AgentOutput` events in the store's `handleEvent` to append agent messages to the chat.

---

## Major Findings

### M-1: Orchestrator contains domain logic that belongs in Pipeline aggregate

**Location**: `backend/src/application/orchestrator.rs:94-127`

The `RevertStage` handler catches `ReviewLoopExhausted`, then calls `force_human_review()` and emits `ApprovalRequired`. This escalation policy (when loop exhausts, auto-escalate to human review) is domain logic -- it's a state machine rule, not orchestration.

**Fix**: Move this into `Pipeline::revert_to_coder()`. When the loop is exhausted, the method should auto-transition to `HumanReview` and return a result variant indicating escalation occurred (e.g., `TransitionResult::Escalated(Transition)`), rather than returning an error that the application layer must interpret.

### M-2: Event stage/from/to fields use ad-hoc string formatting instead of Stage serialization

**Location**: `backend/src/application/orchestrator.rs:88-89`

```rust
from: format!("{:?}", transition.from).to_lowercase(),
to: format!("{:?}", transition.to).to_lowercase(),
```

This converts `Stage::IntentClarifier` to `"intentclarifier"` (no underscore), but the frontend expects `"intent_clarifier"` (with underscore, matching `serde(rename_all = "snake_case")`). The `PipelineCreated` event at line 72 hardcodes `"intent_clarifier"`, so there's an inconsistency between how different events serialize stage names.

**Impact**: Stage transition events won't match frontend `PIPELINE_STAGES` values, breaking the `StageIndicator` component.

**Fix**: Use serde serialization consistently. Either serialize the `Stage` enum to JSON and extract the string, or add a `Stage::as_str()` method that returns the snake_case name matching the serde annotation. Replace all `format!("{:?}", ...)` with this method.

### M-3: Event enum uses String where it should use Stage enum

**Location**: `backend/src/protocol/events.rs:16-25`

The `StageTransition` event uses `from: String` and `to: String` instead of `Stage`. Same for `PipelineCreated.stage`. This discards type safety -- any arbitrary string can be emitted as a stage name.

**Fix**: Change these fields to use the `Stage` enum directly. Serde will serialize them correctly as snake_case strings thanks to `#[serde(rename_all = "snake_case")]`.

### M-4: Pipeline and Task IDs use plain String instead of typed IDs

**Location**: `backend/src/domain/pipeline.rs:79-80`, `backend/src/domain/task.rs:15`

The design doc specifies `TaskId(ULID)` and `PipelineId(ULID)`, and `ulid` is in Cargo.toml. But the implementation uses `String` everywhere, and the orchestrator generates IDs with a sequential counter (`pl_1`, `pl_2`).

**Impact**: No collision safety, no temporal ordering (which ULID provides), type confusion between task IDs and pipeline IDs (both are `String`).

**Fix**: Introduce newtype wrappers: `struct TaskId(Ulid)` and `struct PipelineId(Ulid)`. Use `Ulid::new()` in the orchestrator.

### M-5: `useWebSocket` hook is initialized but `sendOp` is never wired to any UI action

**Location**: `frontend/src/hooks/useWebSocket.ts`, `frontend/src/App.tsx:13`

`useWebSocket()` is called in `App` but `sendOp` is discarded. No component actually sends Ops to the backend. The IntentChat creates tasks client-side (C-1), and there's no UI for `AdvanceStage`, `ApproveHumanReview`, `RejectHumanReview`, etc.

**Fix**: Pass `sendOp` down to components that need it (or expose it via the Zustand store). Wire up the pipeline control Ops.

### M-6: `handlers.rs` is empty -- no separation of cross-cutting concerns

**Location**: `backend/src/application/handlers.rs`

The file is a placeholder comment. All Op handling is inline in the orchestrator's `handle_op` match arms. This is acceptable for v1 size, but the orchestrator is already 200+ lines and growing.

**Fix (suggestion for v2)**: Extract each Op handler into a function in `handlers.rs` that takes `&mut PipelineOrchestrator` (or the relevant state slices) and returns `Vec<Event>`. This keeps the orchestrator as a thin dispatch loop.

### M-7: Store `handleEvent` doesn't handle all event types

**Location**: `frontend/src/store/index.ts:74-134`

Only `PipelineCreated`, `StageTransition`, `AgentOutput`, and `Error` are handled. Missing handlers for:
- `ToolExecution` -- should appear in task logs
- `ApprovalRequired` -- should set task status to `awaiting_approval`
- `ReviewSubmitted` -- should show verdict in logs
- `DeployStatus` -- should update logs with deploy progress
- `PushComplete` -- should mark task as completed
- `Warning` -- should appear in logs

**Fix**: Add handlers for all event types in the switch statement.

---

## Minor Findings

### m-1: Task aggregate is anemic

**Location**: `backend/src/domain/task.rs`

Task has no invariant enforcement. `set_spec` accepts any string (empty or not). `attach_pipeline` doesn't check if a pipeline is already attached (the design doc says "a task always has exactly one pipeline"). There's no validation in `new()` either.

**Fix**: Add guards: `set_spec` should reject empty specs. `attach_pipeline` should return `Result` and reject if `pipeline_id` is already `Some`.

### m-2: Workspace aggregate has no invariant enforcement

**Location**: `backend/src/domain/workspace.rs:56-58`

`add_worktree` doesn't check for duplicate IDs. `find_worktree` is the only query. There's no way to remove a worktree or update its status.

**Fix**: Add duplicate-ID check in `add_worktree`. Add `remove_worktree` and `update_worktree_status` methods.

### m-3: `SandboxedFs::resolve` has a TOCTOU concern

**Location**: `backend/src/infrastructure/fs_ops.rs:23-55`

The path validation builds a logical path from components but doesn't canonicalize the final path. If a symlink exists inside the sandbox pointing outside it, a write to that symlink would escape the sandbox. The root is canonicalized, but the resolved path is not.

**Fix**: After building the resolved path, canonicalize it (if it exists) and verify it still starts with the canonical root. For writes to new paths, canonicalize the parent directory.

### m-4: `AgentConfig::for_stage` catches `Deploy`, `Push`, `Created`, `Completed`, `Failed` in a single wildcard arm

**Location**: `backend/src/domain/agent.rs:65-75`

The wildcard `_` arm gives Deploy and Push empty system prompts and no tools. The design doc specifies Deploy should have `exec_command` and `health_check`, and Push should have `git_push`.

**Fix**: Add explicit arms for `Stage::Deploy` and `Stage::Push` with their correct tool sets. Use the wildcard only for terminal states (`Created`, `Completed`, `Failed`).

### m-5: No heartbeat/ping handling on the backend

**Location**: `backend/src/infrastructure/ws_server.rs:61-77`

The frontend sends `{ kind: 'ping' }` heartbeats every 30s, but the backend's WS handler tries to parse it as `WsMessage`. Since `ping` is not a valid `WsMessage::kind`, it logs "Invalid WS message" on every heartbeat.

**Fix**: Either handle `Message::Ping` at the axum/WebSocket level, or add a `Ping`/`Pong` variant to `WsMessage` and respond accordingly.

### m-6: `LlmClient` uses OpenAI-compatible API format but targets Anthropic by default

**Location**: `backend/src/infrastructure/llm_client.rs:127-129`

`from_env()` defaults `base_url` to `https://api.anthropic.com`, but the request format (`/v1/chat/completions`, `Bearer` auth, OpenAI response schema) is OpenAI-compatible, not Anthropic Messages API-compatible. Anthropic uses `/v1/messages`, `x-api-key` header, and a different request/response schema.

**Fix**: Either target the OpenAI API by default (change default URL), or implement the Anthropic Messages API format. Given the agent configs use Claude model names, implementing the Anthropic adapter is the correct path.

### m-7: `Op::Deploy` and `Op::Push` handlers are stubs that emit events without doing work

**Location**: `backend/src/application/orchestrator.rs:164-179`

`Deploy` emits a `DeployStatus` event but doesn't actually deploy or advance the pipeline. `Push` emits `PushComplete` with a placeholder SHA but doesn't call `GitOps::push()` or advance the pipeline.

**Fix**: Wire these to infrastructure calls (`GitOps::push`, deployment scripts) and call `pipeline.advance()` on success.

### m-8: Frontend `PIPELINE_STAGES` omits `created`, `completed`, and `failed`

**Location**: `frontend/src/types/index.ts:42-51`

The `StageIndicator` only shows the 8 active stages, which is reasonable for the progress bar. But the `StageTransition` handler in the store sets `status: to === 'completed' ? 'completed' : ...`, relying on the string matching. If the backend sends `"completed"` (which it will, via `format!("{:?}", Stage::Completed).to_lowercase()`), the current code should work, but it's fragile since it depends on the format being lowercase-only.

---

## Suggestions

### S-1: Add a `TaskRepository` trait as specified in design doc

The design doc defines a `TaskRepository` trait (Section 4.2), but the implementation uses inline `HashMap` in the orchestrator. Extracting the repository trait now (even backed by HashMap) makes future persistence (SQLite for v2 per ADR-002) a clean swap.

### S-2: Consider making `Pipeline::transitions` private

**Location**: `backend/src/domain/pipeline.rs:84`

`transitions` is `pub`, allowing external code to push arbitrary transitions bypassing the state machine. Make it private with a `pub fn transitions(&self) -> &[Transition]` accessor.

### S-3: Add a `PipelineCompleted` event

The design doc lists `PipelineCompleted { pipeline_id, task_id }` as a domain event, but it's not in the `Event` enum. This event is important for the frontend to know a pipeline finished successfully (vs. just seeing `StageTransition` to `completed`).

### S-4: Frontend routing

The current view switching via `currentView` state works, but consider using a proper router (e.g., react-router or TanStack Router) so URLs are bookmarkable and browser back/forward work.

### S-5: Type the `ToolExecution` event params/result fields

**Location**: `frontend/src/types/index.ts:24`

`params: unknown` and `result: unknown` lose all type information. Consider at minimum `params: Record<string, unknown>`.

---

## Architecture Strengths

1. **Pipeline state machine** is well-encapsulated in the domain layer with proper invariant enforcement (review loop cap, terminal state checks, fail-from-any-state). The test suite covers the key transitions.

2. **SQ/EQ pattern** correctly implements single-writer semantics. The mpsc->orchestrator->broadcast flow eliminates race conditions on domain state without explicit locking.

3. **Clean layer separation** in the backend. Domain has zero infrastructure imports. Infrastructure depends on domain only through the protocol layer. The WebSocket server is a pure transport adapter.

4. **Sandboxed file operations** with proper path traversal prevention. The component-based normalization approach is solid.

5. **Frontend type mirroring** is thorough -- the Op/Event/WsMessage types in TypeScript match the Rust serde shapes closely.

6. **Design doc quality** is excellent -- clear bounded contexts, explicit transition rules with guards, and well-reasoned ADRs.

---

## Priority Action Items

| Priority | Item | Effort |
|----------|------|--------|
| P0 | C-1: Wire frontend task creation through WebSocket Ops | Medium |
| P0 | C-2: Replace simulated chat with real WebSocket Ops | Medium |
| P1 | M-2: Fix stage name serialization inconsistency | Small |
| P1 | M-3: Use Stage enum in Event fields | Small |
| P1 | m-6: Fix LLM client API format mismatch (Anthropic vs OpenAI) | Medium |
| P1 | M-7: Handle all event types in frontend store | Medium |
| P2 | M-1: Move escalation logic into Pipeline aggregate | Small |
| P2 | M-4: Introduce typed IDs (TaskId, PipelineId) | Medium |
| P2 | m-4: Add explicit Deploy/Push agent configs with correct tools | Small |
| P2 | m-5: Handle ping/pong properly in WS server | Small |
| P3 | m-1, m-2: Add invariant enforcement to Task and Workspace | Small |
| P3 | m-3: Fix TOCTOU in SandboxedFs | Small |
| P3 | S-1 through S-5: Quality improvements | Varies |
