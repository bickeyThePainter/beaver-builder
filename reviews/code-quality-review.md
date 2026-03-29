# Beaver Builder -- Code Quality Review

**Date:** 2026-03-29
**Verdict:** NEEDS_WORK

---

## Summary

The codebase shows strong architectural design with a clean separation between protocol, domain, application, and infrastructure layers. The Rust backend has well-modeled domain types with proper error handling in the pipeline state machine. The React frontend is cleanly structured with Zustand for state management and proper TypeScript typing. However, the project is in early scaffold stage with most infrastructure modules unused, minimal test coverage, and several code quality issues that need attention.

---

## Build & Test Results

| Check | Status |
|-------|--------|
| `cargo test` | 6/6 passed |
| `cargo clippy` | 41 dead_code warnings (expected for scaffold), 0 lint errors after fixes |
| `bun run build` (tsc + vite) | Clean build, no warnings |

---

## Test Coverage Assessment

**Current: ~15%** | **Target: 60%+**

### What IS Tested
- `domain::pipeline` -- 4 tests covering happy path, review loop, human rejection, and fail transitions
- `infrastructure::fs_ops` -- 2 tests covering scaffold creation and path traversal prevention

### What is NOT Tested (ordered by priority)
1. **`application::orchestrator`** -- Zero tests. This is the most critical module (single-writer loop). Needs tests for:
   - `StartPipeline` op creates pipeline and task correctly
   - `AdvanceStage` op validates pipeline exists and transitions
   - `RevertStage` with auto-escalation to HumanReview
   - `ApproveHumanReview` / `RejectHumanReview` flow
   - `InterruptPipeline` sets failed state
   - `UserMessage` with missing task (should handle gracefully)
   - Error event emission on invalid ops
   - ID generation uniqueness
2. **`infrastructure::ws_server`** -- No integration tests. Needs at minimum:
   - WebSocket connect/disconnect lifecycle
   - Op serialization/deserialization round-trip
   - Event broadcast to multiple clients
   - Malformed message handling
3. **`infrastructure::llm_client`** -- No tests. Needs:
   - Retry logic (mock HTTP responses for 429, 500)
   - SSE stream parsing
   - Tool call argument parsing (currently uses `unwrap_or_default` -- test malformed JSON)
4. **`domain::workspace`** -- No tests for `add_worktree` / `find_worktree`
5. **Frontend** -- Zero component tests. No test framework configured (no vitest/jest in package.json)

---

## Findings

### Critical (0 remaining -- fixed)

All critical issues have been fixed directly in code.

### Major (5 findings)

#### M1. `unwrap()` in production server startup
**Location:** `infrastructure/ws_server.rs:31-32` (WAS)
**Status:** FIXED -- replaced with `.expect()` with descriptive messages.

#### M2. Unused imports causing compiler warnings
**Location:** `main.rs:6`, `ws_server.rs:9`, `orchestrator.rs:4`
**Status:** FIXED -- removed `std::sync::Arc` imports and unused `Stage`/`MAX_REVIEW_ITERATIONS`.

#### M3. Clippy lint: useless `.into()` conversion
**Location:** `ws_server.rs:51`
**Status:** FIXED -- removed `json.into()`, using `json` directly.

#### M4. Clippy lint: manual string prefix stripping
**Location:** `llm_client.rs:222-223`
**Status:** FIXED -- replaced `starts_with` + slice with `strip_prefix`.

#### M5. No orchestrator tests -- highest-risk untested code
**Location:** `application/orchestrator.rs`
**Issue:** The orchestrator is the single-writer mutation point for all domain state. It handles 9 different op variants with branching error paths. Zero test coverage.
**Recommendation:** Add at minimum 6-8 unit tests using a mock `broadcast::channel` and `mpsc::channel`. The orchestrator's `handle_op` method is async but does not do I/O, so tests are straightforward:
```rust
#[tokio::test]
async fn start_pipeline_creates_task_and_pipeline() {
    let (sq_tx, sq_rx) = mpsc::channel(16);
    let (eq_tx, mut eq_rx) = broadcast::channel(16);
    let mut orch = PipelineOrchestrator::new(sq_rx, eq_tx);
    // Send op, verify events emitted
}
```

### Minor (8 findings)

#### m1. Unused variable in Op::Push handler
**Location:** `orchestrator.rs:173`
**Status:** FIXED -- changed `branch` to `branch: _`.

#### m2. Dead code warnings for scaffold modules
**Location:** `domain/workspace.rs`, `domain/agent.rs`, `domain/tool.rs`, `infrastructure/llm_client.rs`, `infrastructure/git_ops.rs`
**Issue:** ~38 dead_code warnings because infrastructure/domain types are defined but not yet wired into the orchestrator.
**Recommendation:** This is expected for an early-stage project. Consider adding `#[allow(dead_code)]` module-level attributes to scaffold modules, or better, wire them into the orchestrator as you implement each stage's agent execution.

#### m3. `Op::Deploy` and `Op::Push` emit events without actual work
**Location:** `orchestrator.rs:164-179`
**Issue:** Deploy emits a status event but does no deployment. Push emits `PushComplete` with `sha: "placeholder"`. There's no pipeline state transition for either.
**Recommendation:** Either validate pipeline state and transition, or mark these as unimplemented with `todo!()` or `tracing::warn!`.

#### m4. `UserMessage` handler silently produces empty pipeline_id
**Location:** `orchestrator.rs:152-161`
**Issue:** If the task has no `pipeline_id`, `unwrap_or_default()` produces an empty string in the `AgentOutput` event. Downstream consumers may not handle empty pipeline IDs gracefully.
**Recommendation:** Return an error event if the task has no attached pipeline.

#### m5. Frontend `IntentChat` uses simulated responses
**Location:** `frontend/src/components/Chat/IntentChat.tsx:33-51`
**Issue:** `handleSend` uses `setTimeout` with hardcoded mock responses instead of sending ops over WebSocket. The `messages.length === 1` check is fragile.
**Recommendation:** Wire to WebSocket via `sendOp` from `useWebSocket` hook when backend Intent Clarifier agent is implemented.

#### m6. Frontend message list uses array index as key
**Location:** `frontend/src/components/Chat/IntentChat.tsx:93`
**Issue:** `key={i}` on `MessageBubble` list items. If messages are prepended or reordered, React will misidentify components.
**Recommendation:** Add a unique `id` field to `ChatMessage` (e.g., a nanoid or timestamp).

#### m7. No error boundary in React app
**Location:** `frontend/src/App.tsx`
**Issue:** No React error boundary wrapping the app. An unhandled exception in any component will crash the entire UI.
**Recommendation:** Add a top-level `ErrorBoundary` component.

#### m8. `handleEvent` in store doesn't handle `ToolExecution`, `ReviewSubmitted`, `DeployStatus`, `PushComplete`, `Warning`
**Location:** `frontend/src/store/index.ts:74-134`
**Issue:** The switch statement only handles `PipelineCreated`, `StageTransition`, `AgentOutput`, and `Error`. Five event types are silently dropped.
**Recommendation:** Add handlers for remaining events, at minimum logging them to the task's `logs` array.

### Suggestions (5)

#### S1. Consider using `ulid` crate for ID generation
The `ulid` dependency is declared in `Cargo.toml` but unused. The current ID generator (`pl_1`, `pl_2`...) is not globally unique and will conflict across server restarts. Switch to ULID for monotonic, sortable, unique IDs.

#### S2. Add `Display` impl for `Stage` instead of using `format!("{:?}", ...)`
**Location:** `orchestrator.rs:88-89`
Multiple places format stage names via `format!("{:?}", transition.from).to_lowercase()`. This couples event payloads to Rust debug formatting. Use serde's `rename_all` or a `Display` impl for consistency with the frontend's expected `snake_case` strings.

#### S3. Frontend: consider `React.memo` for `PipelineCard` and `MessageBubble`
These are rendered in lists and receive stable props. Memoizing them prevents unnecessary re-renders when sibling items change.

#### S4. WebSocket reconnect has no max retry limit
**Location:** `frontend/src/hooks/useWebSocket.ts:50-56`
The reconnect logic retries forever with exponential backoff capped at 30s. Consider adding a max retry count (e.g., 50) and showing a "Connection failed" UI state.

#### S5. `SandboxedFs::resolve` -- symlink escape not fully prevented
**Location:** `infrastructure/fs_ops.rs:23-56`
The `canonicalize` is called on the root but not on the final resolved path. A symlink inside the sandbox pointing outside could still escape. After building the resolved path, canonicalize it and verify it starts with the canonical root.

---

## Code Smells Summary

| Smell | Count | Locations |
|-------|-------|-----------|
| Dead code (expected scaffold) | ~38 items | workspace, agent, tool, llm_client, git_ops, fs_ops |
| Magic strings | 3 | `"placeholder"` sha, `"Untitled"` task title, `"op_failed"` error code |
| Placeholder logic | 3 | Deploy handler, Push handler, UserMessage handler |
| Missing error propagation | 2 | Deploy/Push don't validate pipeline state |
| Index-based React keys | 1 | IntentChat message list |

---

## Files Changed in This Review

| File | Changes |
|------|---------|
| `backend/src/main.rs` | Removed unused `Arc` import |
| `backend/src/application/orchestrator.rs` | Removed unused `Stage`, `MAX_REVIEW_ITERATIONS` imports; fixed unused `branch` variable |
| `backend/src/infrastructure/ws_server.rs` | Removed unused `Arc` import; replaced `unwrap()` with `expect()`; removed useless `.into()` |
| `backend/src/infrastructure/llm_client.rs` | Replaced manual prefix stripping with `strip_prefix` |

---

## Priority Action Items

1. **Add orchestrator unit tests** (M5) -- highest impact, most critical untested code
2. **Handle all event types in frontend store** (m8) -- 5 event types silently dropped
3. **Wire IntentChat to WebSocket** (m5) -- currently hardcoded mock responses
4. **Add React error boundary** (m7) -- prevents full UI crash on runtime errors
5. **Fix symlink escape in SandboxedFs** (S5) -- security-relevant
6. **Switch to ULID for ID generation** (S1) -- dependency already declared
7. **Add frontend test framework** -- no vitest/jest configured; 0% component test coverage
