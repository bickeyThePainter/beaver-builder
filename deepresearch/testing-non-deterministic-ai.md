# Testing Non-Deterministic AI Agents — Codex Deep Research

## Why This Matters

AI agents are the hardest software to test. The core computation (model inference) is
non-deterministic. Tool execution has side effects. Multi-turn conversations create
exponential state spaces. Most agent projects punt on testing entirely — and it shows
in their reliability.

Codex's testing philosophy: **"comprehensive mocking with selective reality."**
Mock the non-deterministic parts (model API). Run the deterministic parts (tool
execution, policy evaluation) against real implementations in isolated environments.

---

## Making Non-Determinism Deterministic

### Test Mode Initialization

**File**: `core/tests/common/lib.rs:26-30`

```rust
#[ctor]
fn init() {
    codex_core::test_support::set_thread_manager_test_mode(true);
    codex_core::test_support::set_deterministic_process_ids(true);
}
```

Two mechanisms force determinism:

1. **Thread Manager Test Mode** — deterministic thread scheduling and management
2. **Deterministic Process IDs** — consistent PID allocation instead of real OS values

These are set via `#[ctor]` (runs before `main`) so every test in the suite operates
in deterministic mode.

---

## WireMock-Based API Mocking

### ResponseMock

**File**: `core/tests/common/responses.rs`

Custom wrapper around WireMock that captures and inspects model API requests:

```rust
pub struct ResponseMock {
    mock: Mock,
    captured_requests: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl ResponseMock {
    // Inspect what prompts were sent to the model
    pub fn body_json(&self) -> Value
    pub fn message_input_texts(&self, role: &str) -> Vec<String>

    // Verify tool call handling
    pub fn saw_function_call(&self, call_id: &str) -> bool
    pub fn function_call_output_text(&self, call_id: &str) -> Option<String>
}
```

**Decompression**: Automatically handles zstd-encoded request bodies (production uses
compression).

### SSE Fixture Loading

```rust
// Convert JSON array → SSE format
load_sse_fixture("fixtures/tool_call_response.json") → SSE byte stream
```

Fixtures stored in `core/tests/fixtures/` as JSON arrays. Each element becomes an SSE
event. This makes fixtures human-readable while producing production-format streams.

### Mount Helpers

| Helper | Purpose |
|--------|---------|
| `mount_sse_once()` | Single streaming response |
| `mount_sse_sequence()` | Multiple sequential responses |
| `mount_response_sequence()` | Non-streaming responses |
| `mount_compact_json_once()` | Context compaction responses |

---

## TestCodex Harness

**File**: `core/tests/common/test_codex.rs`

### Builder Pattern

```rust
TestCodexBuilder::new()
    .with_config_mutation(|config| {
        config.approval_policy = AskForApproval::Never;
        config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    })
    .with_pre_build_hook(|dirs| {
        // Create test files in temp directory
        std::fs::write(dirs.cwd.join("test.py"), "print('hello')")?;
        Ok(())
    })
    .build()
    .await
```

### What It Provides

- **Isolated temp directories** for home and cwd
- **WireMock server** pre-configured as the model API endpoint
- **Config mutation** for per-test customization
- **Pre-build hooks** for filesystem setup
- **Event polling** with predicates and timeouts

### Test Flow Structure

```
1. Build harness with mock server
2. Mount SSE response sequences (deterministic model output)
3. Submit user turn: test.submit(Op::UserTurn("fix the bug"))
4. Poll for events: test.wait_for_event(|e| matches!(e, TurnComplete))
5. Verify: mock.saw_function_call("apply_patch"), mock.body_json()...
```

---

## Event Polling

**File**: `core/tests/common/lib.rs`

### Polling Helpers

```rust
// Wait for any event matching predicate
wait_for_event(harness, |event| {
    matches!(event, EventMsg::TurnComplete { .. })
})

// Wait and extract typed value
let approval = wait_for_event_match(harness, |event| {
    if let EventMsg::AskForApproval(req) = event {
        Some(req.clone())
    } else {
        None
    }
})

// With custom timeout
wait_for_event_with_timeout(harness, Duration::from_secs(30), predicate)
```

### File System Waiting

```rust
// Wait for file to appear (e.g., after apply_patch)
fs_wait::wait_for_path_exists(&path).await

// Search for file matching pattern
fs_wait::wait_for_matching_file(&dir, |name| name.ends_with(".py")).await
```

Uses the `notify` crate for filesystem events — no polling loops.

---

## Approval Flow Testing

**File**: `core/tests/suite/approvals.rs`

### Test Pattern

```
1. Define action (e.g., WriteFile, FetchUrl, RunCommand)
2. Mock model response that produces that action
3. Submit turn
4. Wait for AskForApproval event
5. Verify approval details match action
6. Send approval decision (accept/deny)
7. Verify action was executed or denied
```

### ActionKind Coverage

| Kind | What's Tested |
|------|--------------|
| `WriteFile` | File modification approval prompts |
| `FetchUrl` | Network access with mock endpoints |
| `FetchUrlNoProxy` | Proxy bypass scenarios |
| `RunCommand` | Shell command approval |
| `RunUnifiedExecCommand` | Escalated unified exec |
| `ApplyPatchFunction` | Patch approval (function call path) |
| `ApplyPatchShell` | Patch approval (shell path) |

---

## Snapshot Testing

### Context Snapshots

**File**: `core/tests/common/context_snapshot.rs`

Custom (not Insta-based) snapshot system for context state:

```rust
pub enum ContextSnapshotRenderMode {
    RedactedText,          // Hide sensitive content
    FullText,              // Include everything
    KindOnly,              // Just item types
    KindWithTextPrefix,    // Type + preview (max N chars)
}
```

Used in compaction tests to verify context is correctly managed:

```rust
let snapshot = format_labeled_requests_snapshot(
    &[("before_compact", &before), ("after_compact", &after)],
    ContextSnapshotRenderMode::KindWithTextPrefix(200),
);
assert_snapshot!(snapshot);
```

### Insta Integration

```toml
insta = "1.46.3"  # In workspace dependencies
```

`INSTA_WORKSPACE_ROOT` configured for proper snapshot file location.
Less prominent than custom context snapshots in this codebase.

---

## Starlark Policy Testing

**File**: `execpolicy/tests/basic.rs`

Direct policy evaluation without subprocess:

```rust
let policy_src = r#"
    prefix_rule(
        pattern = ["git", "reset", "--hard"],
        decision = "forbidden",
        justification = "destructive operation",
    )
"#;

let mut parser = PolicyParser::new();
parser.parse("test.rules", policy_src)?;
let policy = parser.build();

let evaluation = policy.check(&["git", "reset", "--hard"], &options);
assert_eq!(evaluation.decision, Decision::Forbidden);
```

**Tests cover**: prefix matching, network rules, wildcard rejection, rule deduplication,
justification attachment, decision aggregation, host executable constraints.

---

## Gated Streaming Server

**File**: `core/tests/common/streaming_sse.rs`

For tests that need precise timing control over streaming:

```rust
pub struct GatedSseServer {
    chunks: Vec<(String, oneshot::Receiver<()>)>,
    captured_requests: Arc<Mutex<Vec<Request>>>,
}

// Each chunk waits for its gate to open
chunk_1 → gate_1.recv() → send chunk_1 → chunk_2 → gate_2.recv() → ...
```

This enables testing:
- What happens when streaming pauses mid-response
- User interrupts during streaming
- Timeout handling during slow responses
- Race conditions between tool completion and next model output

---

## CI/CD Pipeline

**File**: `.github/workflows/rust-ci.yml`

### Matrix Testing

| Platform | Architectures |
|----------|--------------|
| macOS | aarch64, x86_64 |
| Linux | musl, gnu, arm64 |
| Windows | x64, arm64 |

### Key CI Patterns

1. **Change detection**: Only runs if `codex-rs/` or `.github/` changed
2. **cargo nextest**: Parallel test execution with `ci-test` profile
3. **sccache**: Compilation caching across builds
4. **cargo-chef**: Dependency pre-warming for release builds
5. **bubblewrap setup**: User namespace enabling for sandbox tests on Linux
6. **AppArmor handling**: Ubuntu 24.04+ unprivileged namespace restrictions

### Environment

```yaml
RUST_BACKTRACE: 1
NEXTEST_STATUS_LEVEL: leak
```

---

## Platform-Specific Test Guards

### Skip Macros

```rust
skip_if_sandbox!()      // Skip when running inside a sandbox
skip_if_no_network!()   // Skip when network is unavailable
skip_if_windows!()      // Skip on Windows
```

Environment variable checks for seatbelt and network constraints enable tests
to self-skip when the environment can't support them.

---

## Design Insights

1. **Mock the model, not the tools.** The model is non-deterministic and expensive.
   Tools are deterministic and cheap. Mock the expensive non-deterministic part, run
   the cheap deterministic part for real. This gives you both speed and confidence.

2. **SSE fixtures as JSON arrays** is a brilliant format choice. JSON is human-readable
   and diff-friendly. The conversion to SSE format happens at test time. You can inspect
   and modify fixtures without understanding SSE encoding.

3. **Gated streaming servers** solve the timing problem. Most agent tests either ignore
   streaming entirely or test it with fixed delays (flaky). Gates give precise control
   over when each chunk arrives.

4. **Context snapshots > output snapshots.** Testing what the model *sees* (context
   state) is more valuable than testing what it *produces* (output text). If the context
   is wrong, the output will be wrong regardless of the model.

5. **Deterministic mode at the #[ctor] level** means every test in the suite gets
   determinism for free. No individual test setup needed.

6. **The approval test pattern** (mock → submit → wait → verify → decide → verify)
   captures the full async approval lifecycle. This is the kind of test that catches
   race conditions between approval events and tool execution.

7. **Multi-platform CI with change detection** prevents unnecessary builds while
   ensuring platform-specific bugs (sandbox, filesystem) are caught. The sandbox
   especially needs Linux-specific testing (bwrap, seccomp).

---

## Key Files

| Component | Path |
|-----------|------|
| Test mode init | `core/tests/common/lib.rs:26-30` |
| WireMock wrapper | `core/tests/common/responses.rs` |
| Test harness | `core/tests/common/test_codex.rs` |
| Event polling | `core/tests/common/lib.rs` |
| Approval tests | `core/tests/suite/approvals.rs` |
| Context snapshots | `core/tests/common/context_snapshot.rs` |
| Gated SSE server | `core/tests/common/streaming_sse.rs` |
| Policy tests | `execpolicy/tests/basic.rs` |
| CI pipeline | `.github/workflows/rust-ci.yml` |
| Process control | `core/tests/common/process.rs` |
| FS waiting | `core/tests/common/lib.rs` (fs_wait) |
