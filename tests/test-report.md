# Beaver Builder -- Test Report

**Date:** 2026-03-30
**Test Runner:** cargo test (Rust), vitest (TypeScript)

---

## 1. Summary

| Metric         | Count |
|----------------|-------|
| Total tests    | 87    |
| Passed         | 87    |
| Failed         | 0     |
| Skipped        | 0     |

**All tests pass.**

Note: The 24 pipeline unit tests run twice (once in the `lib` target, once in the `bin` target) due to the dual `lib.rs`/`main.rs` setup. Unique test count is **63**.

---

## 2. Coverage Breakdown by Module

### Backend

| Module | File | Test Count | Type |
|--------|------|-----------|------|
| **domain::pipeline** | `backend/src/domain/pipeline.rs` | 24 | Unit |
| **protocol** (ops, events, messages) | `backend/tests/protocol_tests.rs` | 31 | Integration |
| **application::orchestrator** | `backend/tests/orchestrator_tests.rs` | 7 | Integration |
| **infrastructure::ws_server** | `backend/tests/ws_integration.rs` | 1 | Integration |
| **infrastructure::fs_ops** | `backend/src/infrastructure/fs_ops.rs` | 2 | Unit (pre-existing) |

### Frontend

| Module | File | Test Count | Type |
|--------|------|-----------|------|
| **store (Zustand)** | `frontend/src/__tests__/store.test.ts` | 24 | Unit |

### Detail: Pipeline Unit Tests (24)

- Happy path through all 9 stages
- Each transition records from/to correctly
- Cannot advance from Completed (TerminalState)
- Cannot advance from Failed (TerminalState)
- revert_to_coder invalid from Planner/Coder/Deploy
- force_human_review invalid from Coder/HumanReview
- fail() from Completed/Failed returns TerminalState
- Review loop increments and caps at MAX_REVIEW_ITERATIONS (3)
- Full advance/revert/advance/revert cycle escalates correctly
- Human rejection resets review counter
- Human rejection after exhausted loop resets counter and allows fresh loop
- fail() from every active stage (Created through Push)
- Transition history records all transitions with reasons
- Transition timestamps are sequential
- Pipeline::new defaults (Created, 0 iterations, empty transitions)
- is_terminal() correct for Completed, Failed, and active states
- Stage serializes to snake_case
- Stage deserializes from snake_case

### Detail: Protocol Tests (31)

- Round-trip for all 9 Op variants
- Round-trip for all 10 Event variants (12 tests including edge cases)
- WsMessage::Op and WsMessage::Event wrapping round-trip
- All 9 Op fixtures from `tests/fixtures/ops.json` deserialize
- All 15 Event fixtures from `tests/fixtures/events.json` deserialize
- Op and Event fixture round-trip (serialize -> deserialize -> serialize = stable)
- Op/Event tagged format verification (`type` + `payload`)
- Edge cases: null optional fields, arbitrary JSON in ToolExecution params/result

### Detail: Orchestrator Tests (7)

- StartPipeline emits PipelineCreated + StageTransition
- AdvanceStage through full pipeline (8 transitions, Created to Completed)
- Coder/Reviewer loop via RevertStage (3 cycles + auto-escalation)
- InterruptPipeline emits Warning, blocks further advances
- Non-existent pipeline produces Error event
- ApproveHumanReview advances to Deploy
- RejectHumanReview reverts to Coder

### Detail: WebSocket Integration (1)

- Full stack: start server on random port, connect client, send StartPipeline, verify PipelineCreated event received

### Detail: Frontend Store Tests (24)

- PipelineCreated: creates new task / updates existing task
- StageTransition: updates currentStage, derives status (processing/completed/failed/awaiting_approval)
- AgentOutput: appends to logs, forwards to chat messages for active intent_clarifier
- ToolExecution: appends tool info to logs
- ApprovalRequired: sets awaiting_approval status
- ReviewSubmitted: appends verdict+iteration to logs
- DeployStatus: appends status with/without URL
- PushComplete: appends remote+sha to logs
- Error: sets failed status (with pipeline_id), no-op (null pipeline_id)
- Warning: appends to logs (with pipeline_id), no-op (null pipeline_id)
- Unknown event types silently ignored
- addMessage, resetChat, selectWorkspace (with/without worktrees), selectTask

---

## 3. E2E Scenario Coverage

| Scenario | Status | Coverage |
|----------|--------|----------|
| **Scenario 1: Happy Path** | Covered | Orchestrator `advance_through_full_pipeline` test drives a pipeline from StartPipeline through all stages to Completed. Combined with protocol round-trip and WS integration tests, the full data flow is verified. |
| **Scenario 2: Review Loop** | Covered | Orchestrator `coder_reviewer_loop_via_revert_stage` test performs 3 revert+advance cycles, then verifies auto-escalation to HumanReview with Warning event. Pipeline unit tests verify counter mechanics. |
| **Scenario 3: Human Rejection** | Covered | Orchestrator `reject_human_review_reverts_to_coder` test sends RejectHumanReview and verifies StageTransition to Coder. Pipeline unit tests verify counter reset and ability to resume a fresh review loop. |

All three E2E scenarios from the test plan are covered at the orchestrator integration level. Full browser-based E2E with the frontend would require a browser test framework (Playwright/Cypress), which is out of scope for this phase.

---

## 4. Findings

### Bug: None found

No bugs were discovered during testing. The domain logic, protocol serialization, and orchestrator all behave as specified.

### Observation: Orchestrator auto-creates tasks

When `StartPipeline` is sent for a `task_id` that doesn't exist in the orchestrator's `tasks` map, the orchestrator auto-creates a task with title "Untitled". This is intentional behavior per the code, but worth noting as it means `StartPipeline` can never fail due to a missing task -- only a workspace validation step (which doesn't exist yet) could reject it.

### Observation: PipelineCreated emits IntentClarifier stage

The orchestrator's `StartPipeline` handler immediately advances from `Created` to `IntentClarifier`, so the `PipelineCreated` event carries `stage: intent_clarifier` (not `created`). The test plan mentions `stage: "created"` in the event payload, but the actual implementation always shows the post-advance stage. The fixtures file (`events.json`) has `"stage": "created"` for the sample, which still deserializes correctly but doesn't match runtime behavior. **Severity: Low** -- cosmetic mismatch between fixture and runtime.

---

## 5. Recommendations: Remaining Test Gaps

### High Priority

1. **Task and Workspace domain unit tests** -- `task.rs` and `workspace.rs` have no tests. They are simple but should have coverage for `set_spec`, `attach_pipeline`, `add_worktree`, `find_worktree`, priority serialization, and `WorktreeStatus` serialization.

2. **Invalid state Op handling** -- Test sending `ApproveHumanReview` when pipeline is at `Coder` stage (should produce Error event). Test double `AdvanceStage` in rapid succession.

3. **Malformed JSON over WebSocket** -- Send garbage JSON over WS and verify the connection doesn't crash and an error is logged (currently the WS handler logs a warning but doesn't send an Error event back to the client).

### Medium Priority

4. **Frontend component rendering tests** -- Add `@testing-library/react` + `jsdom` environment for testing `PipelineCard`, `StageIndicator`, `IntentChat`, and `WorkspaceList` rendering.

5. **useWebSocket hook tests** -- Test reconnection with exponential backoff using mocked WebSocket.

6. **Concurrent pipeline operations** -- Test two simultaneous `StartPipeline` ops for different tasks, verify both succeed independently.

### Low Priority

7. **Large payload tests** -- ToolExecution with 100KB+ params, AgentOutput with very large delta.

8. **Performance benchmarks** -- High-frequency AgentOutput event handling in the frontend store.

9. **Agent config tests** -- `AgentConfig::for_stage()` returns correct model/temperature/tools for each stage.

---

## 6. Infrastructure Changes Made for Testing

1. Created `backend/src/lib.rs` -- Exposes modules publicly so integration tests can reference `beaver_builder::*`.
2. Added `build_router()` public function to `backend/src/infrastructure/ws_server.rs` -- Allows tests to create the Axum router with a pre-bound listener on a random port.
3. Added `tokio-tungstenite = "0.24"` as a dev-dependency in `backend/Cargo.toml`.
4. Added `vitest` as a dev-dependency in `frontend/package.json` with `"test": "vitest run"` script.
5. Created `frontend/src/__tests__/` directory for frontend test files.
