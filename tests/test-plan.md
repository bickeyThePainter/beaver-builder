# Beaver Builder -- Test Plan

## 1. Unit Tests

### 1.1 Pipeline State Machine (`backend/src/domain/pipeline.rs`)

**Valid transitions (happy path):**
- `Created -> IntentClarifier` via `advance()`
- `IntentClarifier -> InitAgent` via `advance()`
- `InitAgent -> Planner` via `advance()`
- `Planner -> Coder` via `advance()`
- `Coder -> Reviewer` via `advance()`
- `Reviewer -> HumanReview` via `advance()`
- `HumanReview -> Deploy` via `advance()`
- `Deploy -> Push` via `advance()`
- `Push -> Completed` via `advance()`

**Valid transitions (loops and reversions):**
- `Reviewer -> Coder` via `revert_to_coder()` (review rejection, iteration < MAX)
- `HumanReview -> Coder` via `revert_to_coder()` (human rejection, resets counter)
- `Reviewer -> HumanReview` via `force_human_review()` (loop exhausted)
- `{any non-terminal} -> Failed` via `fail()`

**Invalid transitions:**
- `advance()` from `Completed` returns `TerminalState` error
- `advance()` from `Failed` returns `TerminalState` error
- `revert_to_coder()` from any stage other than `Reviewer` or `HumanReview` returns `InvalidTransition`
- `force_human_review()` from any stage other than `Reviewer` returns `InvalidTransition`
- `fail()` from `Completed` returns `TerminalState`
- `fail()` from `Failed` returns `TerminalState`

**Review loop cap:**
- After `MAX_REVIEW_ITERATIONS` (3) reversions from Reviewer to Coder, `revert_to_coder()` returns `ReviewLoopExhausted`
- `review_iterations` counter increments on each `Reviewer -> Coder` reversion
- `review_iterations` resets to 0 on `HumanReview -> Coder` reversion

**Auto-escalation:**
- When `review_iterations >= MAX_REVIEW_ITERATIONS`, calling `revert_to_coder()` fails
- Orchestrator should then call `force_human_review()` to escalate

**Transition history:**
- Each transition appends to `pipeline.transitions` vec
- Transition records include `from`, `to`, `timestamp`, `reason`

### 1.2 Task Aggregate (`backend/src/domain/task.rs`)

- `Task::new()` creates with empty spec, no pipeline, Medium priority
- `set_spec()` updates the spec field
- `attach_pipeline()` sets `pipeline_id` to `Some(id)`
- Priority enum serializes to snake_case: `low`, `medium`, `high`, `critical`

### 1.3 Workspace (`backend/src/domain/workspace.rs`)

- `Workspace::new()` creates with empty repos, empty swimlane, no worktrees
- `add_worktree()` appends a Worktree
- `find_worktree()` returns `Some` for existing ID, `None` for unknown
- `WorktreeStatus` serializes to `active`, `processing`, `idle`
- `FileArtifact` has `name`, `file_type`, `size`, `author`

### 1.4 Protocol Serialization (`backend/src/protocol/`)

**Op round-trip:**
- Each `Op` variant serializes to `{ "type": "VariantName", "payload": { ... } }` and deserializes back
- All 9 variants: `UserMessage`, `StartPipeline`, `AdvanceStage`, `RevertStage`, `ApproveHumanReview`, `RejectHumanReview`, `Deploy`, `Push`, `InterruptPipeline`

**Event round-trip:**
- Each `Event` variant serializes to `{ "type": "VariantName", "payload": { ... } }` and deserializes back
- All 10 variants: `PipelineCreated`, `StageTransition`, `AgentOutput`, `ToolExecution`, `ApprovalRequired`, `ReviewSubmitted`, `DeployStatus`, `PushComplete`, `Error`, `Warning`

**WsMessage round-trip:**
- `WsMessage::Op { payload }` serializes to `{ "kind": "op", "payload": { "type": ..., "payload": ... } }`
- `WsMessage::Event { payload }` serializes to `{ "kind": "event", "payload": { "type": ..., "payload": ... } }`

**Edge cases:**
- Optional fields (`pipeline_id` in Error/Warning can be null)
- `ToolExecution` params/result are `serde_json::Value` (arbitrary JSON)
- Stage names use snake_case in serialization

### 1.5 Orchestrator (`backend/src/application/orchestrator.rs`)

- Receives `Op` from submission queue, validates against current state
- `StartPipeline` creates a new Pipeline and emits `PipelineCreated`
- `AdvanceStage` calls `pipeline.advance()` and emits `StageTransition`
- `RevertStage` calls `pipeline.revert_to_coder()` and emits `StageTransition`
- `ApproveHumanReview` advances from HumanReview to Deploy
- `RejectHumanReview` calls `pipeline.revert_to_coder()` with reason
- Invalid ops produce `Error` events (not panics)
- Ops referencing non-existent pipeline/task produce `Error` events

---

## 2. Integration Tests

### 2.1 WebSocket Connection Lifecycle

- **Connect**: Client opens WS to `ws://localhost:3001/ws`, receives no error
- **Send Op**: Client sends valid `WsMessage::Op`, server processes without disconnect
- **Receive Event**: After sending a valid Op, client receives corresponding `WsMessage::Event`
- **Disconnect**: Client closes connection cleanly; server removes from broadcast list
- **Reconnect**: After disconnect, client can reconnect and resume receiving events
- **Heartbeat**: Client sends ping every 30s, server responds with pong; no timeout disconnect

### 2.2 Pipeline End-to-End

Full sequence: `StartPipeline` -> advance through all stages -> `PushComplete`

1. Send `StartPipeline { task_id, workspace_id }`
2. Expect `PipelineCreated { pipeline_id, task_id, stage: "created" }`
3. Expect `StageTransition` for each stage progression
4. At `HumanReview`, send `ApproveHumanReview`
5. At `Deploy`, send `Deploy { environment: "staging" }`
6. Expect `DeployStatus`
7. At `Push`, send `Push { remote, branch }`
8. Expect `PushComplete { sha }`

### 2.3 Coder/Reviewer Loop

1. Advance pipeline to Reviewer stage
2. Simulate reviewer rejection: orchestrator calls `revert_to_coder()`
3. Expect `StageTransition { from: "reviewer", to: "coder" }` and `ReviewSubmitted { verdict: "request_changes", iteration: 1 }`
4. Advance back to Reviewer
5. Simulate second rejection (iteration 2)
6. Advance back to Reviewer
7. Simulate third rejection -> `ReviewLoopExhausted` error
8. Orchestrator calls `force_human_review()`
9. Expect `StageTransition { from: "reviewer", to: "human_review" }` and `Warning` about exhausted loop

### 2.4 Human Review Flows

**Approve flow:**
1. Pipeline at `HumanReview`
2. Send `ApproveHumanReview`
3. Expect `StageTransition { from: "human_review", to: "deploy" }`

**Reject flow:**
1. Pipeline at `HumanReview`
2. Send `RejectHumanReview { reason }`
3. Expect `StageTransition { from: "human_review", to: "coder" }`
4. Assert `review_iterations` reset to 0

### 2.5 Error Handling

- **Invalid op for current state**: Send `ApproveHumanReview` when pipeline is at `Coder` -> `Error` event
- **Malformed JSON**: Send `{ "kind": "op", "payload": "garbage" }` -> `Error` event with parse error code
- **Unknown pipeline_id**: Send `AdvanceStage { pipeline_id: "nonexistent" }` -> `Error` event
- **Connection drop mid-operation**: Agent execution continues; client reconnects and receives queued events (or catchup)
- **Double advance**: Send two rapid `AdvanceStage` ops -> first succeeds, second fails with invalid transition

---

## 3. E2E Test Scenarios

### Scenario 1: Happy Path

**Preconditions:**
- Backend server running on `:3001`
- Frontend connected via WebSocket
- One workspace exists with id `ws_01` and an active worktree

**Steps:**

| Step | Actor  | Action                                                  | Expected Event(s)                                              | Assertion                                            |
|------|--------|---------------------------------------------------------|----------------------------------------------------------------|------------------------------------------------------|
| 1    | User   | Opens Intent Chat, types "Build a REST API for todos"   | Op: `UserMessage { task_id: "task_01", content: "..." }`       | Message appears in chat                              |
| 2    | Agent  | Intent Clarifier responds with spec                     | `AgentOutput { stage: "intent_clarifier", is_final: true }`    | SpecCard renders with title and description          |
| 3    | User   | Clicks "Start Pipeline" on SpecCard                     | Op: `StartPipeline { task_id: "task_01", workspace_id: "ws_01" }` | --                                                |
| 4    | Server | Pipeline created                                        | `PipelineCreated { pipeline_id: "pipe_01", stage: "created" }` | PipelineCard appears on dashboard                    |
| 5    | Server | Transitions: Created -> IntentClarifier -> InitAgent    | 2x `StageTransition`                                           | StageIndicator highlights InitAgent                  |
| 6    | Agent  | Init agent scaffolds project                            | `ToolExecution { tool: "create_file", ... }` x N               | Files listed in worktree explorer                    |
| 7    | Server | InitAgent -> Planner                                    | `StageTransition`                                              | StageIndicator highlights Planner                    |
| 8    | Agent  | Planner produces design doc                             | `AgentOutput { stage: "planner", is_final: true }`             | Design doc content in task detail logs               |
| 9    | Server | Planner -> Coder                                        | `StageTransition`                                              | StageIndicator highlights Coder                      |
| 10   | Agent  | Coder implements the API                                | Multiple `AgentOutput`, `ToolExecution` events                 | Code files appear in worktree                        |
| 11   | Server | Coder -> Reviewer                                       | `StageTransition`                                              | StageIndicator highlights Reviewer                   |
| 12   | Agent  | Reviewer approves                                       | `ReviewSubmitted { verdict: "approved", iteration: 0 }`        | --                                                   |
| 13   | Server | Reviewer -> HumanReview                                 | `StageTransition`, `ApprovalRequired { summary }`              | Approval banner appears, status = "awaiting_approval"|
| 14   | User   | Clicks "Approve"                                        | Op: `ApproveHumanReview { pipeline_id: "pipe_01" }`            | --                                                   |
| 15   | Server | HumanReview -> Deploy                                   | `StageTransition`                                              | StageIndicator highlights Deploy                     |
| 16   | Agent  | Deploy agent runs deployment                            | `DeployStatus { status: "success", url: "https://..." }`       | Deploy URL shown in task detail                      |
| 17   | Server | Deploy -> Push                                          | `StageTransition`                                              | StageIndicator highlights Push                       |
| 18   | Agent  | Push agent pushes to remote                             | `PushComplete { remote: "origin", sha: "abc123" }`             | SHA shown, status = "completed"                      |
| 19   | Server | Push -> Completed                                       | `StageTransition { to: "completed" }`                          | PipelineCard shows completed state                   |

**Final assertions:**
- Task status is `completed`
- `pipeline.transitions` has 9 entries (Created through Completed)
- `review_iterations` is 0
- All 8 working stages were visited exactly once

---

### Scenario 2: Review Loop

**Preconditions:**
- Pipeline exists and has reached `Reviewer` stage (iteration 0)
- Coder has produced implementation files

**Steps:**

| Step | Actor    | Action                                             | Expected Event(s)                                                 | Assertion                                      |
|------|----------|----------------------------------------------------|-------------------------------------------------------------------|------------------------------------------------|
| 1    | Agent    | Reviewer rejects: "Missing error handling"         | `ReviewSubmitted { verdict: "request_changes", iteration: 1 }`    | review_iterations = 1                          |
| 2    | Server   | Reviewer -> Coder                                  | `StageTransition { from: "reviewer", to: "coder" }`              | StageIndicator shows Coder                     |
| 3    | Agent    | Coder fixes code                                   | `AgentOutput`, `ToolExecution` events                             | Files updated                                  |
| 4    | Server   | Coder -> Reviewer                                  | `StageTransition { from: "coder", to: "reviewer" }`              | StageIndicator shows Reviewer                  |
| 5    | Agent    | Reviewer rejects: "Tests not updated"              | `ReviewSubmitted { verdict: "request_changes", iteration: 2 }`    | review_iterations = 2                          |
| 6    | Server   | Reviewer -> Coder                                  | `StageTransition { from: "reviewer", to: "coder" }`              | StageIndicator shows Coder                     |
| 7    | Agent    | Coder fixes tests                                  | `AgentOutput`, `ToolExecution` events                             | Test files updated                             |
| 8    | Server   | Coder -> Reviewer                                  | `StageTransition { from: "coder", to: "reviewer" }`              | StageIndicator shows Reviewer                  |
| 9    | Agent    | Reviewer approves                                  | `ReviewSubmitted { verdict: "approved", iteration: 2 }`           | --                                             |
| 10   | Server   | Reviewer -> HumanReview                            | `StageTransition`, `ApprovalRequired`                             | Approval gate reached                          |

**Final assertions:**
- `review_iterations` is 2 (two rejections, then approved on third round)
- `pipeline.transitions` includes 4 extra entries from the loop (2x Reviewer->Coder, 2x Coder->Reviewer)
- All rejection reasons are recorded in transition history

---

### Scenario 3: Human Rejection

**Preconditions:**
- Pipeline at `HumanReview` stage
- Prior review loop used 2 iterations (review_iterations = 2)

**Steps:**

| Step | Actor  | Action                                                | Expected Event(s)                                                 | Assertion                                       |
|------|--------|-------------------------------------------------------|-------------------------------------------------------------------|-------------------------------------------------|
| 1    | User   | Clicks "Reject" with reason "API design doesn't match spec" | Op: `RejectHumanReview { pipeline_id, reason }`             | --                                              |
| 2    | Server | HumanReview -> Coder                                  | `StageTransition { from: "human_review", to: "coder" }`          | review_iterations reset to 0                    |
| 3    | Agent  | Coder rewrites API to match spec                      | `AgentOutput`, `ToolExecution` events                             | Files updated                                   |
| 4    | Server | Coder -> Reviewer                                     | `StageTransition`                                                 | StageIndicator shows Reviewer                   |
| 5    | Agent  | Reviewer approves                                     | `ReviewSubmitted { verdict: "approved", iteration: 0 }`           | review_iterations stays 0 (fresh loop)          |
| 6    | Server | Reviewer -> HumanReview                               | `StageTransition`, `ApprovalRequired`                             | Approval gate reached again                     |
| 7    | User   | Clicks "Approve"                                      | Op: `ApproveHumanReview { pipeline_id }`                          | --                                              |
| 8    | Server | HumanReview -> Deploy                                 | `StageTransition { from: "human_review", to: "deploy" }`         | --                                              |
| 9    | Agent  | Deploy succeeds                                       | `DeployStatus { status: "success", url }`                         | --                                              |
| 10   | Server | Deploy -> Push                                        | `StageTransition`                                                 | --                                              |
| 11   | Agent  | Push succeeds                                         | `PushComplete { remote: "origin", sha }`                          | --                                              |
| 12   | Server | Push -> Completed                                     | `StageTransition { to: "completed" }`                             | Task completed                                  |

**Final assertions:**
- `review_iterations` is 0 (was reset on human rejection)
- Transition history includes `HumanReview -> Coder` with rejection reason
- Pipeline reached `Completed` despite human rejection
- Total transitions > 9 (extra from rejection loop)

---

## 4. Frontend Tests

### 4.1 Component Rendering

**PipelineCard** (`components/Pipeline/PipelineCard.tsx`):
- Renders task title and current stage label
- Shows correct status color (emerald=completed, amber=processing, slate=idle)
- Displays priority badge
- Clicking selects the task (updates `selectedTaskId`)

**StageIndicator** (`components/Pipeline/StageIndicator.tsx`):
- Renders all 8 pipeline stages in order
- Highlights current stage with indigo accent
- Marks completed stages with checkmark / completed style
- Marks future stages as muted/inactive

**WorkspaceList** (`components/Workspace/WorkspaceList.tsx`):
- Renders list of workspace names
- Clicking a workspace calls `selectWorkspace(id)`
- Selected workspace has active styling

**IntentChat** (`components/Chat/IntentChat.tsx`):
- Renders initial agent greeting message
- User messages appear on right, agent messages on left
- Input field accepts text and sends on Enter
- SpecCard appears when `generatedSpec` is non-null

### 4.2 WebSocket Hook (`hooks/useWebSocket.ts`)

- **Connection**: Establishes WS to `ws://localhost:3001/ws` on mount
- **Message handling**: Incoming `WsMessage` with `kind: "event"` is dispatched to `handleEvent`
- **Send**: `sendOp()` wraps Op in `WsMessage` and sends as JSON
- **Reconnection**: On disconnect, retries with exponential backoff (1s, 2s, 4s, ... 30s max)
- **Connection state**: Updates `connected` in store on open/close

### 4.3 Store (`store/index.ts`)

**handleEvent dispatcher:**
- `PipelineCreated` -> updates matching task's `currentStage` and `status` to `"processing"`
- `StageTransition` -> updates matching task's `currentStage`, derives `status` from stage name, appends log
- `AgentOutput` -> appends `delta` to matching task's `logs`
- `Error` with `pipeline_id` -> sets matching task's `status` to `"failed"`, appends error log
- Unknown event types are silently ignored (no crash)

**Actions:**
- `addMessage` appends to messages array
- `resetChat` restores initial greeting
- `selectWorkspace` also sets `activeWorktreeId` to first worktree
- `selectTask` sets `selectedTaskId`

### 4.4 User Interactions

- **Send message**: Type in IntentChat input, press Enter -> `addMessage` called, Op sent via WS
- **Click stage**: Clicking a stage on StageIndicator (if interactive) shows stage details
- **Select workspace**: Click workspace in list -> `selectWorkspace` called, detail view updates
- **Approve/Reject**: Buttons in HumanReview view send `ApproveHumanReview` / `RejectHumanReview` ops

---

## 5. Performance & Edge Cases

### 5.1 Concurrent Pipeline Operations

- Two `StartPipeline` ops for different tasks submitted simultaneously: both should succeed independently
- Two `AdvanceStage` ops for the same pipeline in rapid succession: first succeeds, second returns error (serialized via submission queue)
- High-frequency `AgentOutput` events (simulating fast token streaming): frontend handles without lag or dropped frames

### 5.2 WebSocket Reconnection Under Load

- Disconnect and reconnect during active `AgentOutput` streaming: client resumes receiving events after reconnect (may miss some deltas)
- Server restart while client is connected: client detects disconnect, begins backoff, reconnects when server is back
- Multiple rapid disconnect/reconnect cycles: no zombie connections on server, no duplicate event delivery

### 5.3 Large Message Payloads

- `AgentOutput` with very large `delta` (100KB+): serialization/deserialization succeeds, no truncation
- `ToolExecution` with large `params`/`result` objects: round-trip preserves all data
- Pipeline with 50+ transitions (many review loops ending in human rejection): `transitions` vec stays serializable

### 5.4 Pipeline Stage Timeout Handling

- Agent execution exceeds timeout (30s default): agent is interrupted, `Error` event emitted
- Pipeline remains in current stage after timeout (not auto-advanced)
- Timed-out pipeline can be retried via `AdvanceStage` or failed via `InterruptPipeline`
- Deploy health check timeout: `DeployStatus { status: "timeout" }` event, pipeline does not advance

### 5.5 Additional Edge Cases

- `UserMessage` for a task that already has a running pipeline: message forwarded to current agent context
- `StartPipeline` for a task that already has a pipeline: error, one pipeline per task
- Empty `content` in `UserMessage`: should be rejected or handled gracefully
- `RejectHumanReview` with empty `reason`: should be rejected (reason is required context for Coder)
- Pipeline at `Completed` receiving any op: error, terminal state
