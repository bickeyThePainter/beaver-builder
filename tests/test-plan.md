# Beaver Builder v2 — Test Plan

## 1. Unit Tests

### 1.1 Pipeline State Machine (`backend/src/domain/pipeline.rs`)

| # | Test Case | Setup | Action | Expected |
|---|-----------|-------|--------|----------|
| U1 | Happy path 7-stage | `Pipeline::new()` at Created | `advance()` x8 | Created → Planner → InitAgent → Coder → Reviewer → HumanReview → Deploy → Push → Completed |
| U2 | Cannot advance past Completed | Pipeline at Completed | `advance()` | `Err(TransitionError::TerminalState)` |
| U3 | Cannot advance past Failed | Pipeline at Failed | `advance()` | `Err(TransitionError::TerminalState)` |
| U4 | Review revert: Reviewer → Coder | Pipeline at Reviewer | `revert_to_coder("reason")` | Stage becomes Coder, review_iterations increments by 1 |
| U5 | Review loop cap at 3 | Pipeline at Reviewer, 3 prior reverts | `revert_to_coder()` | `Err(TransitionError::ReviewLoopExhausted)` |
| U6 | Auto-escalation after exhaustion | Orchestrator handles ReviewLoopExhausted | Orchestrator calls `force_human_review()` | Stage → HumanReview, Warning event emitted |
| U7 | Human rejection resets counter | Pipeline at HumanReview | `revert_to_coder("rejected")` | Stage → Coder, `review_iterations` reset to 0 |
| U8 | Fail from any active stage | Pipeline at each of Planner, InitAgent, Coder, Reviewer, HumanReview, Deploy, Push | `fail("reason")` | Stage → Failed |
| U9 | Cannot fail from Completed | Pipeline at Completed | `fail("reason")` | `Err(TransitionError::TerminalState)` |
| U10 | Cannot revert from non-Reviewer/HumanReview | Pipeline at Coder | `revert_to_coder("reason")` | `Err(TransitionError::InvalidTransition)` |
| U11 | Transition history recorded | Pipeline through 3 advances | Check `transitions` vec | 3 entries with correct from/to/timestamp/reason |
| U12 | Timestamps are monotonically increasing | Pipeline through 5 advances | Compare transition timestamps | Each timestamp >= previous |
| U13 | Stage serializes to snake_case | All Stage variants | `serde_json::to_string()` | `"planner"`, `"init_agent"`, `"coder"`, `"reviewer"`, `"human_review"`, `"deploy"`, `"push"`, `"created"`, `"completed"`, `"failed"` |
| U14 | Stage deserializes from snake_case | Snake_case strings | `serde_json::from_str()` | Correct Stage variants |
| U15 | Invalid stage string rejected | `"not_a_stage"` | `serde_json::from_str()` | Deserialization error |

### 1.2 Protocol Serialization (`backend/src/protocol/`)

| # | Test Case | Action | Expected |
|---|-----------|--------|----------|
| P1 | Op round-trip — all 8 variants | Serialize each Op to JSON, deserialize back | Original == deserialized |
| P2 | Event round-trip — all 10 variants | Serialize each Event to JSON, deserialize back | Original == deserialized |
| P3 | Op tagged enum format | Serialize `Op::UserMessage` | `{ "type": "UserMessage", "payload": { "task_id": "...", "content": "..." } }` |
| P4 | Event tagged enum format | Serialize `Event::StageTransition` | `{ "type": "StageTransition", "payload": { "pipeline_id": "...", "from": "coder", "to": "reviewer", ... } }` |
| P5 | WsMessage envelope — op | Serialize WsMessage::Op | `{ "kind": "op", "payload": { "type": "...", "payload": { ... } } }` |
| P6 | WsMessage envelope — event | Serialize WsMessage::Event | `{ "kind": "event", "payload": { "type": "...", "payload": { ... } } }` |
| P7 | Fixture round-trip — ops.json | Load `tests/fixtures/ops.json`, deserialize all entries, re-serialize | JSON structure matches |
| P8 | Fixture round-trip — events.json | Load `tests/fixtures/events.json`, deserialize all entries, re-serialize | JSON structure matches |
| P9 | Stage fields in events are snake_case | Serialize StageTransition with HumanReview | `"from": "human_review"` not `"HumanReview"` |

---

## 2. Integration Tests

### 2.1 Orchestrator (`backend/tests/orchestrator_tests.rs`)

| # | Test Case | Op Sent | Expected Events |
|---|-----------|---------|-----------------|
| I1 | StartPipeline creates pipeline | `StartPipeline { task_id, workspace_id }` | `PipelineCreated` + `StageTransition(created → planner)` |
| I2 | AdvanceStage through all stages | `AdvanceStage` x7 (after StartPipeline) | 7 `StageTransition` events: planner→init_agent→coder→reviewer→human_review→deploy→push→completed |
| I3 | RevertStage from Reviewer | `RevertStage { pipeline_id, reason }` at Reviewer | `StageTransition(reviewer → coder)` + `ReviewSubmitted { verdict: "rejected", iteration: 1 }` |
| I4 | Review loop exhaustion → auto-escalate | `RevertStage` x3 (with advance between) then 4th RevertStage | `Warning { message: "..." }` + `StageTransition(reviewer → human_review)` + `ApprovalRequired` |
| I5 | ApproveHumanReview advances | `ApproveHumanReview` at HumanReview | `StageTransition(human_review → deploy)` |
| I6 | RejectHumanReview reverts to Coder | `RejectHumanReview` at HumanReview | `StageTransition(human_review → coder)`, review_iterations reset to 0 |
| I7 | InterruptPipeline fails pipeline | `InterruptPipeline` at any active stage | `Warning` + pipeline stage = Failed |
| I8 | Deploy emits DeployStatus | `Deploy { pipeline_id, environment }` | `DeployStatus { status: "...", url: "..." }` |
| I9 | Invalid op returns Error event | `AdvanceStage` on non-existent pipeline_id | `Error { code: "...", message: "..." }` |

### 2.2 WebSocket Lifecycle (`backend/tests/ws_integration.rs`)

| # | Test Case | Action | Expected |
|---|-----------|--------|----------|
| W1 | Connect to /ws | Open WebSocket to ws://localhost:{port}/ws | Connection established |
| W2 | Send Op, receive Event | Send StartPipeline via WS | Receive PipelineCreated + StageTransition as WsMessage envelopes |
| W3 | JSON envelope format | Receive event via WS | Matches `{ "kind": "event", "payload": { "type": "...", "payload": { ... } } }` |
| W4 | Multiple clients receive broadcasts | 2 WS clients connected, one sends Op | Both receive the resulting Events |
| W5 | Clean disconnect | Client disconnects | No server panic, other clients unaffected |
| W6 | Malformed JSON rejected | Send `{ "garbage": true }` | Error event or connection continues (no crash) |

### 2.3 LLM Mock Test (`backend/tests/`)

| # | Test Case | Setup | Action | Expected |
|---|-----------|-------|--------|----------|
| L1 | UserMessage → AgentOutput chain | Mock LlmProvider returning fixed response | Send `UserMessage { task_id, content }` | `AgentOutput { stage: "planner", delta: "...", is_final: true }` |
| L2 | Streaming UserMessage | Mock LlmProvider returning stream chunks | Send `UserMessage` | Multiple `AgentOutput` with `is_final: false`, then one with `is_final: true` |
| L3 | LLM error → Error event | Mock LlmProvider returning `LlmError` | Send `UserMessage` | `Error { code: "llm_error", message: "..." }` |

---

## 3. E2E Browser Scenarios (Chrome DevTools MCP)

All E2E tests use Chrome DevTools MCP tools against a running backend (port 3001) and frontend (port 5173).

### 3.1 Happy Path — Full Pipeline Completion

**Precondition:** Backend and frontend servers running.

| Step | Tool | Action | Assertion |
|------|------|--------|-----------|
| 1 | `navigate_page` | Go to `http://localhost:5173` | Page loads |
| 2 | `take_screenshot` | Capture initial state | → `01-initial-load.png` |
| 3 | `take_snapshot` | Get page a11y tree | Element UIDs available |
| 4 | `click` | Click Planner Chat nav tab | View switches to Planner Chat |
| 5 | `fill` | Type task description in chat input | Text appears in input |
| 6 | `click` | Click send button | Message appears in chat as user bubble |
| 7 | `wait_for` | Wait for agent response | AgentOutput text appears as agent bubble |
| 8 | `fill` + `click` | Send second message (e.g., "looks good, deploy") | Spec card appears |
| 9 | `click` | Click "DEPLOY PIPELINE" on spec card | Pipeline created, dashboard view shows task at Planner stage |
| 10 | `take_screenshot` | Capture pipeline at Planner | → `02-pipeline-created.png` |
| 11 | `evaluate_script` | Send `AdvanceStage` ops via WebSocket (x7) to advance through all stages | 7 StageTransition events received |
| 12 | `wait_for` | Wait for "COMPLETED" text on dashboard | Pipeline shows Completed status |
| 13 | `take_screenshot` | Capture final state | → `03-happy-path-completed.png` |
| 14 | `list_console_messages` | Check for JS errors | No error-level messages |

### 3.2 Review Loop — 3 Reverts + Auto-Escalation

**Precondition:** Backend and frontend servers running.

| Step | Tool | Action | Assertion |
|------|------|--------|-----------|
| 1 | `evaluate_script` | Send `StartPipeline` op via WebSocket | PipelineCreated event received |
| 2 | `evaluate_script` | Send `AdvanceStage` x4 (planner → init_agent → coder → reviewer) | Pipeline at Reviewer stage |
| 3 | `evaluate_script` | Send `RevertStage` + `AdvanceStage` (round 1: reviewer → coder → reviewer) | ReviewSubmitted iteration=1 |
| 4 | `evaluate_script` | Send `RevertStage` + `AdvanceStage` (round 2) | ReviewSubmitted iteration=2 |
| 5 | `evaluate_script` | Send `RevertStage` + `AdvanceStage` (round 3) | ReviewSubmitted iteration=3 |
| 6 | `evaluate_script` | Send 4th `RevertStage` | Warning event (auto-escalation), StageTransition to human_review, ApprovalRequired event |
| 7 | `wait_for` | Wait for approval UI | Human Review stage indicator active |
| 8 | `take_screenshot` | Capture auto-escalation state | → `04-review-loop-escalated.png` |
| 9 | `list_console_messages` | Check for errors | No error-level messages |

### 3.3 Human Rejection — Reject, Fix, Complete

**Precondition:** Pipeline at HumanReview stage (continue from 3.2 or create fresh).

| Step | Tool | Action | Assertion |
|------|------|--------|-----------|
| 1 | `click` or `evaluate_script` | Send `RejectHumanReview` op | StageTransition human_review → coder, review_iterations reset to 0 |
| 2 | `wait_for` | Wait for Coder stage indicator | Pipeline at Coder stage |
| 3 | `evaluate_script` | Send `AdvanceStage` (coder → reviewer) | StageTransition to reviewer |
| 4 | `evaluate_script` | Send `AdvanceStage` (reviewer → human_review) | StageTransition to human_review, ApprovalRequired |
| 5 | `click` or `evaluate_script` | Send `ApproveHumanReview` | StageTransition human_review → deploy |
| 6 | `evaluate_script` | Send `AdvanceStage` (deploy → push) | StageTransition to push |
| 7 | `evaluate_script` | Send `AdvanceStage` (push → completed) | StageTransition to completed, PushComplete event |
| 8 | `wait_for` | Wait for "COMPLETED" | Pipeline shows Completed |
| 9 | `take_screenshot` | Capture final state | → `05-human-rejection-completed.png` |
| 10 | `list_console_messages` | Check for errors | No error-level messages |
| 11 | `lighthouse_audit` | Run accessibility audit | Score >= 80 |

---

## 4. Frontend Tests (Vitest)

### 4.1 Zustand Store — `handleEvent` (`frontend/src/__tests__/store.test.ts`)

| # | Test Case | Event Fired | Expected Store Mutation |
|---|-----------|-------------|------------------------|
| F1 | PipelineCreated | `{ type: "PipelineCreated", payload: { pipeline_id, task_id, stage: "planner" } }` | New task entry in `tasks` map with pipeline_id and stage |
| F2 | StageTransition | `{ type: "StageTransition", payload: { pipeline_id, from: "planner", to: "init_agent", timestamp } }` | Task's stage updated to "init_agent" |
| F3 | AgentOutput (streaming) | `{ type: "AgentOutput", payload: { pipeline_id, stage: "planner", delta: "chunk", is_final: false } }` | Message appended/updated in `messages` array |
| F4 | AgentOutput (final) | `{ type: "AgentOutput", payload: { ..., delta: "done", is_final: true } }` | Message marked as complete |
| F5 | ToolExecution | `{ type: "ToolExecution", payload: { pipeline_id, tool, params, result, duration_ms } }` | Tool execution logged in task's activity |
| F6 | ApprovalRequired | `{ type: "ApprovalRequired", payload: { pipeline_id, task_id, summary } }` | Task flagged for human review, summary stored |
| F7 | ReviewSubmitted | `{ type: "ReviewSubmitted", payload: { pipeline_id, verdict: "rejected", iteration: 2 } }` | Review iteration count updated |
| F8 | DeployStatus | `{ type: "DeployStatus", payload: { pipeline_id, status: "success", url: "https://..." } }` | Deploy status/url stored on task |
| F9 | PushComplete | `{ type: "PushComplete", payload: { pipeline_id, remote: "origin", sha: "abc123" } }` | Push info stored, task marked complete |
| F10 | Error | `{ type: "Error", payload: { pipeline_id, code: "invalid_op", message: "..." } }` | Error displayed/stored |
| F11 | Warning | `{ type: "Warning", payload: { pipeline_id, message: "Review loop exhausted" } }` | Warning displayed/stored |

### 4.2 Store Actions

| # | Test Case | Action | Expected |
|---|-----------|--------|----------|
| F12 | addTask | `addTask({ id, title, spec, workspace_id })` | Task added to `tasks` |
| F13 | selectTask | `selectTask("task-1")` | `selectedTaskId` updated |
| F14 | resetChat | `resetChat()` | `messages` cleared, `generatedSpec` cleared |
| F15 | setConnected | `setConnected(true)` | `connected` is true |
| F16 | setSendOp | `setSendOp(mockFn)` | `sendOp` calls mockFn |

---

## 5. Test Data

All fixture data lives in `tests/fixtures/`. See:
- `ops.json` — all 8 Op variants
- `events.json` — all 10 Event variants
- `scenarios/happy-path.json` — step-by-step op/event sequence
- `scenarios/review-loop.json` — 3 reverts + auto-escalation sequence
- `scenarios/human-rejection.json` — rejection, fix, completion sequence
- `workspaces.json` — sample workspace data
- `tasks.json` — sample tasks at various stages

All JSON follows the Rust `serde(tag = "type", content = "payload")` format with `snake_case` stage values.
