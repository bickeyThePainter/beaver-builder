# Tool Design & Execution Architecture

Deep-dive into how tools are defined, registered, dispatched, sandboxed, approved,
and executed in Codex — from schema to output serialization.

---

## 1. Tool Definition Layer

### Tool Spec Types

Tools are defined using several spec formats:

```rust
// codex-rs/protocol/src/dynamic_tools.rs
pub struct DynamicToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,  // JSON Schema for tool inputs
}

// codex-rs/core/src/tools/spec.rs
pub enum ToolSpec {
    Function(ResponsesApiTool),      // Standard Responses API tools
    Freeform(FreeformTool),          // Custom format (apply_patch)
    LocalShell,                      // Built-in shell execution
    Custom { ... },                  // Arbitrary custom tools
}

pub struct ResponsesApiTool {
    pub name: String,
    pub description: String,
    pub parameters: JsonSchema,      // Full JSON Schema of parameters
    pub strict: bool,
}
```

### Tool Payload Types (post-dispatch)

```rust
// codex-rs/core/src/tools/context.rs
#[derive(Clone, Debug)]
pub enum ToolPayload {
    Function { arguments: String },              // JSON string
    Custom { input: String },                    // Raw freeform input
    LocalShell { params: ShellToolCallParams },  // Parsed shell params
    Mcp { server: String, tool: String, raw_arguments: String },
}
```

---

## 2. Tool Registration & Routing

### Build Pipeline

```rust
// codex-rs/core/src/tools/spec.rs
pub(crate) fn build_specs(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, rmcp::model::Tool>>,
    app_tools: Option<HashMap<String, ToolInfo>>,
    dynamic_tools: &[DynamicToolSpec],
) -> ToolRegistryBuilder {
    let mut builder = ToolRegistryBuilder::new();
    builder.push_spec(spec);
    builder.register_handler("tool_name", Arc::new(HandlerImpl));
    builder.build()  // Returns (specs, registry)
}
```

### Tool Router

```rust
// codex-rs/core/src/tools/router.rs
pub struct ToolRouter {
    registry: ToolRegistry,          // Name → Handler mapping
    specs: Vec<ConfiguredToolSpec>,  // Available tool specs + capabilities
}

pub async fn dispatch_tool_call(
    &self, session: Arc<Session>, turn: Arc<TurnContext>,
    tracker: SharedTurnDiffTracker, call: ToolCall, source: ToolCallSource,
) -> Result<ResponseInputItem, FunctionCallError> {
    // Routes through ToolRegistry.dispatch()
}
```

### Full Dispatch Chain

```
Model Output
    → Router.build_tool_call()     → ToolCall
    → ToolRouter.dispatch_tool_call() → ToolRegistry.dispatch()
    → ToolHandler.handle()         → ToolOutput
    → ToolOutput.into_response()   → ResponseInputItem (fed back to model)
```

---

## 3. ToolHandler Trait

The core handler interface every tool implements:

```rust
// codex-rs/core/src/tools/registry.rs
pub trait ToolHandler: Send + Sync {
    fn kind(&self) -> ToolKind;

    fn matches_kind(&self, payload: &ToolPayload) -> bool;

    /// Returns true if tool may mutate environment
    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        false
    }

    /// Main execution method
    async fn handle(&self, invocation: ToolInvocation)
        -> Result<ToolOutput, FunctionCallError>;
}

pub enum ToolKind {
    Function,  // Standard function-style tool
    Mcp,       // Model Context Protocol tool
}
```

### Registry Dispatch (with mutation gating)

```rust
pub async fn dispatch(&self, invocation: ToolInvocation)
    -> Result<ResponseInputItem, FunctionCallError>
{
    let handler = self.handler(&invocation.tool_name)?;

    if !handler.matches_kind(&invocation.payload) {
        return Err(FunctionCallError::Fatal("incompatible payload".into()));
    }

    let is_mutating = handler.is_mutating(&invocation).await;

    // Serialize mutating tool calls via gate
    if is_mutating {
        invocation.turn.tool_call_gate.wait_ready().await;
    }

    let output = handler.handle(invocation).await?;

    // Post-execution hook
    dispatch_after_tool_use_hook(...).await?;

    output.into_response(&call_id, &payload)
}
```

---

## 4. Built-In Tool Handlers

| Handler | Kind | Mutating | Purpose |
|---|---|---|---|
| `ShellHandler` | Function | Yes | Execute shell commands |
| `ApplyPatchHandler` | Function/Freeform | Yes | Apply file patches |
| `UnifiedExecHandler` | Function | Yes | Unified exec (ConPTY) |
| `DynamicToolHandler` | Function | Yes | External/dynamic tools |
| `McpHandler` | MCP | Varies | Route to MCP servers |
| `ReadFileHandler` | Function | No | Read file contents |
| `GrepFilesHandler` | Function | No | Search files (BM25) |
| `ListDirHandler` | Function | No | List directories |
| `JsReplHandler` | Function | Yes | JavaScript REPL |
| `ViewImageHandler` | Function | No | Image viewing |
| `PlanHandler` | Function | No | Update turn plan |
| `RequestUserInputHandler` | Function | No | Request user input |

---

## 5. ToolOrchestrator: Approval → Sandbox → Retry

The orchestrator coordinates the full lifecycle for mutating tools:

```rust
// codex-rs/core/src/tools/orchestrator.rs
pub(crate) struct ToolOrchestrator {
    sandbox: SandboxManager,
}
```

### Orchestration Phases

```
PHASE 1: DETERMINE APPROVAL REQUIREMENT
    │
    ├── Skip { bypass_sandbox }     → proceed directly
    ├── Forbidden { reason }        → return Err(Rejected)
    └── NeedsApproval { reason }    → go to Phase 2
                │
PHASE 2: ASK FOR APPROVAL
    │
    ├── Denied / Abort              → return Err(Rejected)
    └── Approved                    → go to Phase 3
                │
PHASE 3: SANDBOX ATTEMPT
    │
    Select sandbox type → build SandboxAttempt → run tool
                │
PHASE 4: ESCALATE ON FAILURE (RETRY)
    │
    If tool.escalate_on_failure() and got Rejected:
        Escalate sandbox → retry with relaxed policy
```

### Approval Requirement Types

```rust
pub(crate) enum ExecApprovalRequirement {
    Skip {
        bypass_sandbox: bool,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    NeedsApproval {
        reason: Option<String>,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    Forbidden { reason: String },
}
```

---

## 6. ToolRuntime Trait (Execution Abstraction)

Every executable tool implements this runtime-agnostic interface:

```rust
// codex-rs/core/src/tools/sandboxing.rs
pub trait ToolRuntime<Req, Out>: Approvable<Req> + Sandboxable {
    fn network_approval_spec(
        &self, _req: &Req, _ctx: &ToolCtx
    ) -> Option<NetworkApprovalSpec> { None }

    async fn run(
        &mut self, req: &Req,
        attempt: &SandboxAttempt<'_>, ctx: &ToolCtx,
    ) -> Result<Out, ToolError>;
}
```

This allows the orchestrator to work with any tool — `ShellRuntime`,
`ApplyPatchRuntime`, etc. — through the same phases.

---

## 7. Shell Execution (The Primary Tool)

### ShellHandler → ShellRuntime

```rust
impl ToolHandler for ShellHandler {
    async fn handle(&self, invocation: ToolInvocation)
        -> Result<ToolOutput, FunctionCallError>
    {
        // 1. Parse ShellToolCallParams from payload
        // 2. Create ExecApprovalRequirement via exec_policy_manager
        // 3. Build ShellRequest
        // 4. Run through ToolOrchestrator (approval → sandbox → exec)
        // 5. Format output for model
    }
}
```

### ShellRuntime::run()

```rust
impl ToolRuntime<ShellRequest, ExecToolCallOutput> for ShellRuntime {
    async fn run(&mut self, req: &ShellRequest,
        attempt: &SandboxAttempt<'_>, ctx: &ToolCtx)
        -> Result<ExecToolCallOutput, ToolError>
    {
        // 1. Wrap command (snapshot, UTF8)
        // 2. Try zsh-fork backend if configured (fast path)
        // 3. Build CommandSpec (command, cwd, env, timeout, permissions)
        // 4. Transform for sandbox via SandboxAttempt
        // 5. Execute under sandbox → ExecToolCallOutput
    }
}
```

### Output Collection & Serialization

```rust
pub struct ExecToolCallOutput {
    pub exit_code: i32,
    pub duration: Duration,
    pub timed_out: bool,
    pub aggregated_output: OutputContent,  // Combined stdout/stderr
}

// Serialized as JSON for model consumption:
// { "exit_code": 0, "duration_seconds": 1.2, "output": "..." }
```

---

## 8. Execution Policy Engine (Starlark DSL)

Commands are evaluated against Starlark rules before execution:

```python
# Example policy rules
prefix_rule(
    pattern=["rm", ["-rf", "-r"]],
    decision="forbidden",
    justification="Recursive delete not allowed"
)

prefix_rule(
    pattern=["cargo", "test"],
    decision="allow",
    justification="Unit tests are safe"
)

host_executable(
    name="python",
    paths=["/usr/bin/python3", "/usr/local/bin/python3"]
)
```

### ExecPolicyManager

```rust
// codex-rs/core/src/exec_policy.rs
pub struct ExecPolicyManager {
    policy: ArcSwap<Policy>,  // Hot-swappable Starlark rules
}

pub async fn create_exec_approval_requirement_for_command(
    &self, req: ExecApprovalRequest<'_>,
) -> ExecApprovalRequirement {
    let evaluation = exec_policy.check_multiple_with_options(
        commands.iter(), &fallback, &match_options
    );
    match evaluation.decision {
        Decision::Forbidden => Forbidden { reason },
        Decision::Prompt   => NeedsApproval { reason },
        Decision::Allow    => Skip { bypass_sandbox },
    }
}
```

---

## 9. Sandbox Attempt Structure

```rust
pub struct SandboxAttempt<'a> {
    pub sandbox: SandboxType,         // None | MacosSeatbelt | LinuxSeccomp | WindowsRestrictedToken
    pub policy: &'a SandboxPolicy,    // ReadOnly | WorkspaceWrite | DangerFullAccess
    pub file_system_policy: &'a FileSystemSandboxPolicy,
    pub network_policy: NetworkSandboxPolicy,
    pub enforce_managed_network: bool,
    pub manager: &'a SandboxManager,
    pub sandbox_cwd: &'a Path,
    pub codex_linux_sandbox_exe: Option<&'a PathBuf>,
    pub use_linux_sandbox_bwrap: bool,
    pub windows_sandbox_level: WindowsSandboxLevel,
}
```

---

## 10. Tool Output & Result Serialization

```rust
pub enum ToolOutput {
    Function {
        body: FunctionCallOutputBody,    // Text or ContentItems
        success: Option<bool>,
    },
    Mcp {
        result: Result<CallToolResult, String>,
    },
}

pub enum FunctionCallOutputBody {
    Text(String),                    // Plain text output
    ContentItems(Vec<ContentItem>),  // Structured content (images, etc.)
}

impl ToolOutput {
    pub fn into_response(self, call_id: &str, payload: &ToolPayload)
        -> ResponseInputItem
    {
        match self {
            ToolOutput::Function { body, success } => {
                if matches!(payload, ToolPayload::Custom { .. }) {
                    ResponseInputItem::CustomToolCallOutput { call_id, output }
                } else {
                    ResponseInputItem::FunctionCallOutput { call_id, output }
                }
            }
            ToolOutput::Mcp { result } => {
                ResponseInputItem::McpToolCallOutput { call_id, result }
            }
        }
    }
}
```

---

## 11. Parallel Tool Execution

```rust
// codex-rs/core/src/tools/parallel.rs
pub fn handle_tool_call(self, call: ToolCall, cancellation_token: CancellationToken)
    -> impl Future<Output = Result<ResponseInputItem, CodexErr>>
{
    let supports_parallel = self.router.tool_supports_parallel(&call.tool_name);
    let lock = Arc::clone(&self.parallel_execution);

    async move {
        let _guard = if supports_parallel {
            Either::Left(lock.read().await)   // Read lock — parallel OK
        } else {
            Either::Right(lock.write().await) // Write lock — serial only
        };

        router.dispatch_tool_call(...).await
    }
}
```

---

## 12. MCP Tool Handler

```rust
impl ToolHandler for McpHandler {
    fn kind(&self) -> ToolKind { ToolKind::Mcp }

    async fn handle(&self, invocation: ToolInvocation)
        -> Result<ToolOutput, FunctionCallError>
    {
        let (server, tool, raw_arguments) = /* extract from Mcp payload */;

        let response = handle_mcp_tool_call(
            session, &turn, call_id, server, tool, raw_arguments
        ).await;

        // Convert to ToolOutput::Mcp or ToolOutput::Function
    }
}
```

---

## 13. Dynamic Tool Handler (Client-Routed)

For tools defined externally and routed back to the calling client:

```rust
impl ToolHandler for DynamicToolHandler {
    async fn is_mutating(&self, _: &ToolInvocation) -> bool { true }

    async fn handle(&self, invocation: ToolInvocation)
        -> Result<ToolOutput, FunctionCallError>
    {
        // Route back to client via oneshot channel
        let response = request_dynamic_tool(
            &session, &turn, call_id, tool_name, parsed_args
        ).await?;

        let DynamicToolResponse { content_items, success } = response;
        Ok(ToolOutput::Function {
            body: FunctionCallOutputBody::ContentItems(content_items),
            success: Some(success),
        })
    }
}
```

---

## 14. Apply Patch Tool (Special Case)

```rust
impl ToolHandler for ApplyPatchHandler {
    async fn handle(&self, invocation: ToolInvocation)
        -> Result<ToolOutput, FunctionCallError>
    {
        // 1. Parse patch input (from Function or Custom payload)
        // 2. Verify patch via codex_apply_patch::maybe_parse_apply_patch_verified
        // 3. Build ApplyPatchRequest
        // 4. Run through ToolOrchestrator with ApplyPatchRuntime
        // 5. Return success/failure
    }
}
```

---

## 15. Error Handling Strategy

```
Handler.handle() → FunctionCallError
  ├── RespondToModel(msg)       → Surface to model (recoverable)
  ├── MissingLocalShellCallId   → Surface to model
  └── Fatal(msg)                → Abort turn (unrecoverable)

ToolRuntime.run() → ToolError
  ├── Rejected(reason)          → User declined approval
  └── Codex(err)                → System error

ExecPolicyManager → ExecApprovalRequirement
  ├── Skip { bypass_sandbox }
  ├── NeedsApproval { reason }
  └── Forbidden { reason }
```

---

## 16. Layered Architecture Summary

```
┌──────────────────────────────────────────────────────────┐
│  1. Definition Layer   — ToolSpec: schemas + descriptions │
├──────────────────────────────────────────────────────────┤
│  2. Registration Layer — ToolRegistry: name → handler map │
├──────────────────────────────────────────────────────────┤
│  3. Dispatch Layer     — ToolRouter: ResponseItem → call  │
├──────────────────────────────────────────────────────────┤
│  4. Handler Layer      — ToolHandler trait: per-tool logic│
├──────────────────────────────────────────────────────────┤
│  5. Orchestration Layer— ToolOrchestrator: approve+retry  │
├──────────────────────────────────────────────────────────┤
│  6. Execution Layer    — ToolRuntime: platform-specific   │
├──────────────────────────────────────────────────────────┤
│  7. Sandbox Layer      — exec.rs + sandboxing/: OS-level  │
└──────────────────────────────────────────────────────────┘
```

---

## Key Files

| File | Purpose |
|---|---|
| `codex-rs/core/src/tools/spec.rs` | Tool schema building (~3700 lines) |
| `codex-rs/core/src/tools/router.rs` | Dispatch routing |
| `codex-rs/core/src/tools/registry.rs` | Handler registry + ToolHandler trait |
| `codex-rs/core/src/tools/context.rs` | ToolInvocation, ToolPayload, ToolOutput |
| `codex-rs/core/src/tools/orchestrator.rs` | Approval → sandbox → retry pipeline |
| `codex-rs/core/src/tools/parallel.rs` | Parallel execution with RwLock gating |
| `codex-rs/core/src/tools/sandboxing.rs` | ToolRuntime trait, SandboxAttempt |
| `codex-rs/core/src/tools/handlers/` | All concrete handler implementations |
| `codex-rs/core/src/exec.rs` | Low-level command execution |
| `codex-rs/core/src/exec_policy.rs` | Starlark policy evaluation |
| `codex-rs/execpolicy/` | Starlark DSL implementation |
