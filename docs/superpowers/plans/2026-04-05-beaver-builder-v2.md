# Beaver Builder v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an AI coding agent pipeline harness with 7-stage state machine, Rust backend, React frontend, and provider-agnostic LLM integration.

**Architecture:** Single-writer SQ/EQ pattern. Orchestrator reads Ops from mpsc channel, mutates Pipeline aggregate, calls `Arc<dyn LlmProvider>`, broadcasts Events. Frontend connects via WebSocket. 4 views: Pipeline Dashboard, Planner Chat, Review, Workspaces.

**Tech Stack:** Rust (axum, tokio, serde, chrono, uuid, tower-http, reqwest, async-trait, thiserror), React 19, Vite, TailwindCSS, Zustand, lucide-react, vitest.

**Agent Team Assignments:**

| Task | Agent | Parallel? |
|------|-------|-----------|
| 1-3 | Designer | First (sequential) |
| 4-8 | Coder | After Designer |
| 9 | Tester | Parallel with Coder (plan + fixtures only) |
| 10 | Reviewer | After Coder |
| 11 | Coder (fix) | After Reviewer (if needed) |
| 12-13 | Tester | After Coder done (real tests + E2E) |

---

### Task 1: Project Skeleton — Cargo.toml + package.json

**Agent:** Designer
**Files:**
- Create: `backend/Cargo.toml`
- Create: `backend/src/main.rs`
- Create: `backend/src/lib.rs`
- Create: `frontend/package.json`
- Create: `frontend/index.html`
- Create: `frontend/vite.config.ts`
- Create: `frontend/tsconfig.json`
- Create: `frontend/tailwind.config.js`
- Create: `frontend/postcss.config.js`
- Create: `frontend/src/main.tsx`
- Create: `frontend/src/index.css`
- Create: `frontend/src/vite-env.d.ts`

- [ ] **Step 1: Create backend/Cargo.toml**

```toml
[package]
name = "beaver-builder"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
tower-http = { version = "0.6", features = ["cors"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
async-trait = "0.1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
futures = "0.3"

[dev-dependencies]
tokio-tungstenite = "0.26"
```

- [ ] **Step 2: Create backend/src/main.rs (minimal, compiles)**

```rust
mod domain;
mod application;
mod llm;
mod infrastructure;
mod protocol;

fn main() {
    println!("beaver-builder");
}
```

- [ ] **Step 3: Create backend/src/lib.rs**

```rust
pub mod domain;
pub mod application;
pub mod llm;
pub mod infrastructure;
pub mod protocol;
```

- [ ] **Step 4: Create all mod.rs stubs**

Create empty `mod.rs` in each module dir (domain, application, llm, infrastructure, protocol) so `cargo check` passes.

- [ ] **Step 5: Create frontend/package.json**

```json
{
  "name": "beaver-builder-frontend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "test": "vitest run"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "zustand": "^5.0.0",
    "lucide-react": "^0.500.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.4.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.5.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.7.0",
    "vite": "^6.0.0",
    "vitest": "^3.0.0"
  }
}
```

- [ ] **Step 6: Create frontend config files**

`vite.config.ts`:
```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: { '/ws': { target: 'http://localhost:3001', ws: true } },
  },
});
```

`tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "jsx": "react-jsx",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "outDir": "./dist"
  },
  "include": ["src"]
}
```

`tailwind.config.js`:
```javascript
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: { extend: {} },
  plugins: [],
};
```

`postcss.config.js`:
```javascript
export default {
  plugins: { tailwindcss: {}, autoprefixer: {} },
};
```

`index.html`, `src/main.tsx`, `src/index.css`, `src/vite-env.d.ts` — minimal React + Tailwind boilerplate.

- [ ] **Step 7: Verify both build**

```bash
cd backend && cargo check
cd ../frontend && bun install && bun run build
```

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "scaffold project skeleton with cargo + vite"
```

---

### Task 2: Domain Layer — Pipeline State Machine

**Agent:** Designer
**Files:**
- Create: `backend/src/domain/mod.rs`
- Create: `backend/src/domain/pipeline.rs`
- Create: `backend/src/domain/task.rs`
- Create: `backend/src/domain/agent.rs`

- [ ] **Step 1: Write pipeline state machine tests**

In `backend/src/domain/pipeline.rs`, write tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_advances_through_all_stages() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        let stages = [
            Stage::Planner, Stage::InitAgent, Stage::Coder,
            Stage::Reviewer, Stage::HumanReview,
            Stage::Deploy, Stage::Push, Stage::Completed,
        ];
        for expected in stages {
            let t = p.advance().unwrap();
            assert_eq!(t.to, expected);
        }
    }

    #[test]
    fn review_loop_caps_at_3() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);
        for i in 0..3 {
            p.revert_to_coder(format!("fix #{}", i + 1)).unwrap();
            p.advance().unwrap(); // back to Reviewer
        }
        let err = p.revert_to_coder("one more".into()).unwrap_err();
        assert!(matches!(err, TransitionError::ReviewLoopExhausted { .. }));
    }

    #[test]
    fn human_rejection_resets_counter() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::HumanReview);
        p.revert_to_coder("rejected".into()).unwrap();
        assert_eq!(p.review_iterations, 0);
        assert_eq!(p.current_stage, Stage::Coder);
    }

    #[test]
    fn cannot_advance_from_completed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);
        assert!(matches!(p.advance().unwrap_err(), TransitionError::TerminalState(_)));
    }

    #[test]
    fn fail_from_any_active_stage() {
        for stage in [Stage::Planner, Stage::Coder, Stage::Reviewer, Stage::Deploy] {
            let mut p = Pipeline::new("p1".into(), "t1".into());
            advance_to(&mut p, stage);
            p.fail("broke".into()).unwrap();
            assert_eq!(p.current_stage, Stage::Failed);
        }
    }

    #[test]
    fn stage_serializes_to_snake_case() {
        assert_eq!(serde_json::to_string(&Stage::HumanReview).unwrap(), "\"human_review\"");
        assert_eq!(serde_json::to_string(&Stage::InitAgent).unwrap(), "\"init_agent\"");
    }

    fn advance_to(p: &mut Pipeline, target: Stage) {
        while p.current_stage != target {
            p.advance().expect("advance_to failed");
        }
    }
}
```

- [ ] **Step 2: Run tests — they should fail (no implementation yet)**

```bash
cd backend && cargo test -- domain::pipeline 2>&1 | tail -5
```

- [ ] **Step 3: Implement Pipeline, Stage, Transition, TransitionError**

Full implementation of:
- `Stage` enum (10 variants, `serde rename_all = "snake_case"`)
- `Stage::happy_next()` — returns the next stage on the happy path (7-stage: Created→Planner→Init→Coder→Reviewer→HumanReview→Deploy→Push→Completed)
- `Pipeline::new()`, `advance()`, `revert_to_coder()`, `force_human_review()`, `fail()`, `current_stage()`
- `Transition` struct (from, to, timestamp, reason)
- `TransitionError` enum (InvalidTransition, ReviewLoopExhausted, TerminalState)
- `MAX_REVIEW_ITERATIONS = 3`

- [ ] **Step 4: Run tests — all should pass**

```bash
cd backend && cargo test -- domain::pipeline
```
Expected: 6+ tests pass.

- [ ] **Step 5: Implement Task and AgentConfig**

`task.rs`: Task struct with `new()`, `attach_pipeline()`.

`agent.rs`: AgentConfig with `for_stage()` mapping:
- Planner → gpt-5.4, temp 0.4
- InitAgent → gpt-5.3-codex, temp 0.2
- Coder → gpt-5.3-codex, temp 0.3
- Reviewer → gpt-5.4, temp 0.2
- Default → gpt-5.4, temp 0.0

Include system prompt constants for each stage.

- [ ] **Step 6: Wire domain/mod.rs**

```rust
pub mod pipeline;
pub mod task;
pub mod agent;
```

- [ ] **Step 7: cargo test — all domain tests pass**

```bash
cd backend && cargo test
```

- [ ] **Step 8: Commit**

```bash
git add backend/src/domain/ && git commit -m "implement domain layer: pipeline state machine, task, agent config"
```

---

### Task 3: Protocol + LLM Provider Trait

**Agent:** Designer
**Files:**
- Create: `backend/src/protocol/mod.rs`
- Create: `backend/src/protocol/ops.rs`
- Create: `backend/src/protocol/events.rs`
- Create: `backend/src/protocol/messages.rs`
- Create: `backend/src/llm/mod.rs`
- Create: `backend/src/llm/provider.rs`

- [ ] **Step 1: Implement Op enum (8 variants)**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Op {
    UserMessage { task_id: String, content: String },
    StartPipeline { task_id: String, workspace_id: String },
    AdvanceStage { pipeline_id: String },
    RevertStage { pipeline_id: String, reason: String },
    ApproveHumanReview { pipeline_id: String },
    RejectHumanReview { pipeline_id: String, reason: String },
    Deploy { pipeline_id: String, environment: String },
    InterruptPipeline { pipeline_id: String },
}
```

- [ ] **Step 2: Implement Event enum (10 variants)**

All `Stage` fields use the `Stage` type (not String). Include `stage: Stage` in PipelineCreated and AgentOutput, `from: Stage, to: Stage` in StageTransition.

- [ ] **Step 3: Implement WsMessage envelope**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum WsMessage {
    #[serde(rename = "op")]
    Op { payload: Op },
    #[serde(rename = "event")]
    Event { payload: Event },
}
```

- [ ] **Step 4: Write serialization round-trip tests**

Test every Op and Event variant serializes → deserializes correctly.

- [ ] **Step 5: Implement LlmProvider trait**

`backend/src/llm/provider.rs`:
```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
    async fn chat_stream(&self, request: LlmRequest) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>, LlmError>;
}
```

Plus `LlmRequest`, `LlmResponse`, `LlmMessage`, `StreamChunk`, `LlmError` types.

- [ ] **Step 6: Wire mod.rs files**

`protocol/mod.rs`, `llm/mod.rs`.

- [ ] **Step 7: cargo build + cargo test**

```bash
cd backend && cargo build && cargo test
```

- [ ] **Step 8: Commit**

```bash
git add backend/src/protocol/ backend/src/llm/ && git commit -m "implement protocol ops/events and llm provider trait"
```

---

### Task 4: OpenAI Provider Implementation

**Agent:** Coder
**Files:**
- Create: `backend/src/llm/openai.rs`
- Create: `backend/src/llm/factory.rs`
- Modify: `backend/src/llm/mod.rs`

- [ ] **Step 1: Implement OpenAiProvider**

`openai.rs`:
- `OpenAiProvider::new(base_url, api_key)` and `from_env()` (reads `OPENAI_API_KEY`, base URL defaults to `https://api.openai.com`)
- `impl LlmProvider for OpenAiProvider` — `chat()` with retry (3 attempts, exponential backoff, retryable on 429/5xx/timeout)
- `chat_stream()` — SSE parsing, returns mpsc::Receiver<StreamChunk>
- URL construction: `{base_url}/v1/chat/completions` (no double `/v1`)
- Proper error types via `LlmError`

- [ ] **Step 2: Implement LlmProviderFactory**

`factory.rs`:
```rust
pub struct LlmProviderFactory;

impl LlmProviderFactory {
    pub fn from_env() -> Arc<dyn LlmProvider> {
        let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".into());
        match provider.as_str() {
            "openai" => Arc::new(OpenAiProvider::from_env()),
            other => panic!("Unknown LLM provider: {other}. Supported: openai"),
        }
    }
}
```

- [ ] **Step 3: Update llm/mod.rs**

```rust
pub mod provider;
pub mod openai;
pub mod factory;
```

- [ ] **Step 4: cargo build**

```bash
cd backend && cargo build
```

- [ ] **Step 5: Commit**

```bash
git add backend/src/llm/ && git commit -m "implement openai provider with retry and streaming"
```

---

### Task 5: Infrastructure — WebSocket Server + Git/FS Ops

**Agent:** Coder
**Files:**
- Create: `backend/src/infrastructure/mod.rs`
- Create: `backend/src/infrastructure/ws_server.rs`
- Create: `backend/src/infrastructure/git_ops.rs`
- Create: `backend/src/infrastructure/fs_ops.rs`

- [ ] **Step 1: Implement ws_server.rs**

axum router with `/ws` route, `CorsLayer::permissive()`. WebSocket handler:
- Forward client Op messages to SQ (mpsc sender)
- Subscribe to EQ (broadcast receiver), forward Events to client
- Use `tokio::select!` for bidirectional, clean disconnect handling

Expose `build_router()` for tests and `serve()` for main.

- [ ] **Step 2: Implement git_ops.rs**

`GitOps` struct with static methods using `std::process::Command`:
- `init(path)`, `create_worktree(repo, branch)`, `commit(path, message)`, `push(path, remote, branch)`, `current_branch(path)`, `diff(path)`
- `GitError` enum with `CommandFailed` and `Io` variants

- [ ] **Step 3: Implement fs_ops.rs**

`SandboxedFs` with path traversal prevention:
- `new(root)`, `read_file(relative)`, `write_file(relative, content)`, `create_dir(relative)`, `list_dir(relative)`, `scaffold_project(title, spec)`
- Resolve symlinks before checking path prefix (macOS /tmp → /private/tmp)

- [ ] **Step 4: cargo build**

```bash
cd backend && cargo build
```

- [ ] **Step 5: Commit**

```bash
git add backend/src/infrastructure/ && git commit -m "implement ws server, git ops, and sandboxed fs"
```

---

### Task 6: Orchestrator — Wire Everything Together

**Agent:** Coder
**Files:**
- Create: `backend/src/application/mod.rs`
- Create: `backend/src/application/orchestrator.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Implement PipelineOrchestrator**

```rust
pub struct PipelineOrchestrator {
    sq_rx: mpsc::Receiver<Op>,
    eq_tx: broadcast::Sender<Event>,
    llm: Arc<dyn LlmProvider>,
    pipelines: HashMap<String, Pipeline>,
    tasks: HashMap<String, Task>,
    conversations: HashMap<String, Vec<LlmMessage>>,
    next_id: u64,
}
```

`run()` loop: `while let Some(op) = self.sq_rx.recv().await { self.handle_op(op).await }`.

`handle_op()` matches all 8 Op variants:
- `StartPipeline`: create Pipeline, advance to Planner, emit PipelineCreated + StageTransition
- `UserMessage`: get AgentConfig for current stage, build conversation, call `self.llm.chat()`, emit AgentOutput
- `AdvanceStage`: call `pipeline.advance()`, emit StageTransition
- `RevertStage`: call `pipeline.revert_to_coder()`, handle ReviewLoopExhausted (auto-escalate + Warning + ApprovalRequired)
- `ApproveHumanReview` / `RejectHumanReview`: advance or revert
- `Deploy`: emit DeployStatus
- `InterruptPipeline`: call `pipeline.fail()`, emit Warning

- [ ] **Step 2: Wire main.rs**

```rust
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive("beaver_builder=debug".parse().unwrap()))
        .init();

    let (sq_tx, sq_rx) = mpsc::channel::<Op>(256);
    let (eq_tx, _) = broadcast::channel::<Event>(1024);

    let llm = LlmProviderFactory::from_env();
    let orchestrator = PipelineOrchestrator::new(sq_rx, eq_tx.clone(), llm);
    tokio::spawn(orchestrator.run());

    let addr = "0.0.0.0:3001";
    tracing::info!("Beaver Builder listening on {addr}");
    ws_server::serve(addr, sq_tx, eq_tx).await;
}
```

- [ ] **Step 3: cargo build + cargo test**

```bash
cd backend && cargo build && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add backend/src/application/ backend/src/main.rs && git commit -m "implement orchestrator and wire main.rs"
```

---

### Task 7: Frontend — Types, Store, Hooks, Components

**Agent:** Coder
**Files:**
- Create: `frontend/src/types/index.ts`
- Create: `frontend/src/store/index.ts`
- Create: `frontend/src/hooks/useWebSocket.ts`
- Create: `frontend/src/hooks/usePipeline.ts`
- Create: `frontend/src/App.tsx`
- Create: all component files in `frontend/src/components/`

- [ ] **Step 1: Implement types/index.ts**

Mirror Rust protocol exactly. All Op variants, Event variants, WsMessage envelope, Stage type, Task/Workspace view models.

- [ ] **Step 2: Implement store/index.ts**

Zustand store with:
- `connected`, `setConnected`
- `sendOp`, `setSendOp`
- `currentView` (4 views: dashboard, workspaces, planner-chat, review)
- `tasks`, `selectedTaskId`, `selectTask`, `addTask`
- `workspaces`, `selectedWorkspaceId`, `activeWorktreeId`
- `messages`, `generatedSpec`, `activeTaskId`, `addMessage`, `resetChat`
- `handleEvent()` — switch on ALL 10 event types. No silent drops.

- [ ] **Step 3: Implement hooks**

`useWebSocket.ts`: Connect to `ws://localhost:3001/ws` (dev mode direct, not through proxy). Auto-reconnect with exponential backoff. Register `sendOp` in store. Parse incoming WsMessage, call `handleEvent`.

`usePipeline.ts`: Derived state helper — stage index, progress percentage.

- [ ] **Step 4: Implement all components**

Reference `gemini-web-page.html` for styling. Dark theme `bg-[#07080a]`, slate/indigo.

Pipeline: `PipelineCard.tsx` (7-stage bar), `StageIndicator.tsx`.
Chat: `PlannerChat.tsx` (sends UserMessage + StartPipeline ops via sendOp — NOT static mockup), `MessageBubble.tsx`, `SpecCard.tsx`.
Review: `ReviewPanel.tsx`, `DiffView.tsx`, `ApprovalActions.tsx` (sends ApproveHumanReview / RejectHumanReview ops).
Workspace: `WorkspaceList.tsx`, `WorkspaceDetail.tsx`, `WorktreeExplorer.tsx`.
Layout: `Navbar.tsx` (4 view tabs), `StatusBar.tsx`.

- [ ] **Step 5: Implement App.tsx**

Root component with 4 views, useWebSocket hook, Navbar + StatusBar.

- [ ] **Step 6: bun run build**

```bash
cd frontend && bun run build
```

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ && git commit -m "implement frontend: types, store, hooks, 4 views, all components"
```

---

### Task 8: Integration Tests — Orchestrator + WebSocket

**Agent:** Coder
**Files:**
- Create: `backend/tests/orchestrator_tests.rs`
- Create: `backend/tests/ws_integration.rs`

- [ ] **Step 1: Orchestrator integration tests**

Test via real mpsc/broadcast channels:
- StartPipeline → PipelineCreated + StageTransition events
- Full pipeline advancement through all 7 stages
- RevertStage × 3 → ReviewLoopExhausted → auto-escalate
- ApproveHumanReview / RejectHumanReview
- InterruptPipeline

- [ ] **Step 2: WebSocket integration test**

Start server on random port, connect via tokio-tungstenite:
- Send StartPipeline Op → receive PipelineCreated Event
- Verify JSON envelope format

- [ ] **Step 3: cargo test — all pass**

```bash
cd backend && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add backend/tests/ && git commit -m "add orchestrator and websocket integration tests"
```

---

### Task 9: Test Plan + Fixtures (Parallel with Coder)

**Agent:** Tester (runs parallel with Tasks 4-8)
**Files:**
- Create: `tests/test-plan.md`
- Create: `tests/fixtures/ops.json`
- Create: `tests/fixtures/events.json`
- Create: `tests/fixtures/scenarios/happy-path.json`
- Create: `tests/fixtures/scenarios/review-loop.json`
- Create: `tests/fixtures/scenarios/human-rejection.json`
- Create: `tests/fixtures/workspaces.json`
- Create: `tests/fixtures/tasks.json`

- [ ] **Step 1: Write test plan**

Cover: unit tests (pipeline, protocol), integration tests (orchestrator, WS, LLM mock), frontend tests (store), E2E scenarios (3 browser scenarios using Chrome DevTools MCP).

- [ ] **Step 2: Write fixture data**

All JSON matching Rust serde tagged enum format: `{ "type": "Variant", "payload": { ... } }`. All Stage values in snake_case.

- [ ] **Step 3: Write scenario sequences**

Each scenario: step-by-step ops, expected events, state_after assertions.

- [ ] **Step 4: Commit**

```bash
git add tests/ && git commit -m "add test plan and fixture data"
```

---

### Task 10: Combined Architecture + Code Quality Review

**Agent:** Reviewer (single agent, two sections)
**Files:**
- Create: `reviews/review.md`

- [ ] **Step 1: Read all source files**

Read design-doc, CONTEXT.md, all backend/src/ and frontend/src/ files.

- [ ] **Step 2: Architecture review**

Check: DDD compliance, aggregate boundaries, SQ/EQ pattern, LlmProvider trait usage (orchestrator must NOT import openai.rs), protocol completeness, frontend-backend type alignment.

- [ ] **Step 3: Code quality review**

Run `cargo build`, `cargo test`, `cargo clippy`, `bun run build`. Check: no unwrap() in prod, proper error types, no static mockups in frontend, store handles all 10 events, test coverage assessment.

- [ ] **Step 4: Write review.md**

Verdict (PASS/NEEDS_WORK/FAIL), findings by severity, specific file:line references. If NEEDS_WORK, write actionable fix list to `reviews/feedback-for-coder.md`.

- [ ] **Step 5: Commit**

```bash
git add reviews/ && git commit -m "add combined architecture and code quality review"
```

---

### Task 11: Fix Review Findings (if needed)

**Agent:** Coder (fix round)
**Files:**
- Modify: files identified in `reviews/feedback-for-coder.md`

- [ ] **Step 1: Read reviews/feedback-for-coder.md**

- [ ] **Step 2: Fix all Critical and Major issues**

- [ ] **Step 3: cargo build + cargo test + cargo clippy + bun run build**

All must pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "fix review findings"
```

---

### Task 12: Real Test Code (after Coder done)

**Agent:** Tester
**Files:**
- Create: `backend/tests/protocol_tests.rs`
- Create: `frontend/src/__tests__/store.test.ts`
- Modify: `backend/src/domain/pipeline.rs` (extend tests)

- [ ] **Step 1: Extend pipeline unit tests**

Add tests for: all invalid transitions, transition history recording, timestamp ordering, auto-escalation from Reviewer.

- [ ] **Step 2: Write protocol serialization tests**

Round-trip every Op and Event variant. Load from `tests/fixtures/ops.json` and `tests/fixtures/events.json`.

- [ ] **Step 3: Write frontend store tests**

Test `handleEvent` for all 10 event types. Test `addTask`, `selectTask`, `resetChat`. Use vitest.

- [ ] **Step 4: Run all tests**

```bash
cd backend && cargo test
cd ../frontend && bun run test
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "add comprehensive unit, protocol, and store tests"
```

---

### Task 13: Browser E2E Tests

**Agent:** Tester
**Prerequisite:** Tasks 4-8 (Coder) and Task 12 complete.

**Important:** You have Chrome DevTools MCP tools available. See CONTEXT.md for the full list. You MUST:
1. Start the backend server: `cd backend && cargo run &`
2. Start the frontend server: `cd frontend && bun run dev &`
3. Use Chrome DevTools MCP tools to interact with the real browser

**Files:**
- Create: `tests/e2e-screenshots/` (screenshots as evidence)
- Create: `tests/e2e-test-report.md`

- [ ] **Step 1: Start both servers**

```bash
cd backend && cargo run &
cd frontend && bun run dev &
```

Wait for both to be ready (port 3001 and 5173).

- [ ] **Step 2: E2E Scenario 1 — Happy Path**

1. `navigate_page` to http://localhost:5173
2. `take_screenshot` → `01-initial-load.png`
3. `take_snapshot` to get element UIDs
4. `click` "NEW TASK" button
5. `fill` the chat input with a task description
6. `click` send → verify message appears
7. Send second message → verify spec card appears
8. `click` "DEPLOY PIPELINE" → verify dashboard shows task at Planner stage
9. Use `evaluate_script` to send AdvanceStage ops through WebSocket
10. Verify all 7 transitions fire and final status is COMPLETED
11. `take_screenshot` → `02-happy-path-completed.png`

- [ ] **Step 3: E2E Scenario 2 — Review Loop**

1. Create new pipeline via `evaluate_script` WebSocket
2. Advance to Reviewer
3. Send 3 RevertStage ops, advance between each
4. Send 4th RevertStage → verify Warning event (auto-escalation)
5. Verify UI shows AWAITING_APPROVAL at Human Review stage
6. `take_screenshot` → `03-review-loop.png`

- [ ] **Step 4: E2E Scenario 3 — Human Rejection**

1. From Human Review stage, send RejectHumanReview op
2. Verify transition to Coder (counter reset)
3. Advance: Coder → Reviewer → Human Review
4. Send ApproveHumanReview → Deploy → Push → Completed
5. `take_screenshot` → `04-human-rejection.png`

- [ ] **Step 5: Check console errors and run Lighthouse**

```
list_console_messages(types: ["error"])
lighthouse_audit(mode: "snapshot", device: "desktop")
```

- [ ] **Step 6: Write test report**

`tests/e2e-test-report.md` with: summary, per-scenario results, screenshots, findings, Lighthouse scores.

- [ ] **Step 7: Stop servers and commit**

```bash
kill %1 %2
git add tests/ && git commit -m "add browser e2e tests with screenshots and report"
```

---

## Final: Push to Remote

After all tasks complete:

```bash
git add -A && git commit -m "beaver builder v2 complete" && git push
```
