# Subagent Orchestration & Inter-Agent Communication

Deep-dive into how Codex spawns subagents, manages MCP server connections,
routes approval flows between agents, and handles inter-component communication.

---

## 1. Architecture: No Traditional Subagents — Thread-Level Delegation

Codex does **not** have a traditional "subagent spawning" model with a pool of
workers. Instead it uses **thread-level delegation**: each agent (main, Guardian, etc.)
runs as a separate `Codex` instance with its own `Session`, sharing services
through `Arc<>`.

```
┌─────────────────────────────────────────────────────┐
│                    SHARED SERVICES                    │
│  ┌────────────┐ ┌────────────┐ ┌─────────────────┐ │
│  │ AuthManager │ │ModelManager│ │ McpConnManager  │ │
│  └─────┬──────┘ └─────┬──────┘ └───────┬─────────┘ │
│        │              │                │            │
├────────┼──────────────┼────────────────┼────────────┤
│        │              │                │            │
│  ┌─────▼──────────────▼────────────────▼──────────┐ │
│  │              MAIN AGENT (Session)               │ │
│  │  Config: workspace-write, on-request            │ │
│  │  Tools: shell, apply_patch, mcp, ...            │ │
│  └──────────────────┬─────────────────────────────┘ │
│                     │                                │
│           spawn via run_codex_thread_interactive     │
│                     │                                │
│  ┌──────────────────▼─────────────────────────────┐ │
│  │            GUARDIAN AGENT (Session)              │ │
│  │  Config: read-only, never (approval=never)      │ │
│  │  Tools: NONE (locked down)                      │ │
│  │  Purpose: Risk assessment only                  │ │
│  └────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

### SessionSource identifies agent type

```rust
pub enum SessionSource {
    Session,                             // Main agent
    SubAgent(SubAgentSource),            // Subagent
}

pub enum SubAgentSource {
    Other(String),  // e.g., "guardian"
}
```

---

## 2. Subagent Spawning: `run_codex_thread_interactive`

The core function for creating subagents:

```rust
// codex-rs/core/src/codex_delegate.rs
pub(crate) async fn run_codex_thread_interactive(
    config: Config,                        // Isolated config for subagent
    auth_manager: Arc<AuthManager>,        // SHARED
    models_manager: Arc<ModelsManager>,    // SHARED
    parent_session: Arc<Session>,          // Parent reference
    parent_ctx: Arc<TurnContext>,          // Parent turn context
    cancel_token: CancellationToken,       // Cancellation scope
    subagent_source: SubAgentSource,       // Identity marker
    initial_history: Option<InitialHistory>,
) -> Result<Codex, CodexErr> {

    let codex = Codex::spawn(
        config,
        auth_manager,
        models_manager,
        Arc::clone(&parent_session.services.skills_manager),   // SHARED
        Arc::clone(&parent_session.services.plugins_manager),  // SHARED
        Arc::clone(&parent_session.services.mcp_manager),      // SHARED
        Arc::clone(&parent_session.services.file_watcher),     // SHARED
        initial_history,
        SessionSource::SubAgent(subagent_source),              // Mark as subagent
        parent_session.services.agent_control.clone(),
        Vec::new(),
        false, None, None,
    ).await?;

    // Spawn event forwarding and op forwarding tasks
    tokio::spawn(forward_events(codex, parent_session, parent_ctx, cancel_token));
    tokio::spawn(forward_ops(codex, rx_ops, cancel_token));

    Ok(codex_interface)
}
```

### What's Shared vs. Isolated

| Resource | Shared? | Notes |
|---|---|---|
| AuthManager | Shared | Same credentials |
| ModelsManager | Shared | Same model catalog |
| McpConnectionManager | Shared | Same MCP servers |
| SkillsManager | Shared | Same skills |
| PluginsManager | Shared | Same plugins |
| FileWatcher | Shared | Same file system events |
| Config (sandbox/approval) | **Isolated** | Subagent gets locked-down copy |
| Session state | **Isolated** | Separate history, context |
| ExecPolicy rules | **Isolated** | Subagent has own policy |
| Network approvals | **Copied** | Inherited but scoped |

---

## 3. Guardian Agent: The Primary Subagent

### Spawning Flow

```rust
// codex-rs/core/src/guardian.rs
async fn run_guardian_subagent(
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    prompt_items: Vec<UserInput>,
    schema: Value,                    // JSON schema for structured output
    cancel_token: CancellationToken,
) -> anyhow::Result<GuardianAssessment> {

    // 1. Build locked-down config
    let guardian_config = build_guardian_subagent_config(
        turn.config.as_ref(),
        live_network_config,
        guardian_model,
        guardian_reasoning_effort,
    )?;

    // 2. Spawn Guardian as one-shot Codex instance
    let child_cancel = cancel_token.child_token();
    let codex = run_codex_thread_interactive(
        guardian_config,
        session.services.auth_manager.clone(),
        session.services.models_manager.clone(),
        Arc::clone(&session),
        turn,
        child_cancel.clone(),
        SubAgentSource::Other("guardian".to_string()),
        None,
    ).await?;

    // 3. Seed inherited network approvals
    session.services.network_approval
        .copy_session_approved_hosts_to(&codex.session.services.network_approval)
        .await;

    // 4. Submit approval request as user input
    codex.submit(Op::UserInput {
        items: prompt_items,
        final_output_json_schema: Some(schema),
    }).await?;

    // 5. Await completion
    let mut last_agent_message = None;
    while let Ok(event) = codex.next_event().await {
        match event.msg {
            EventMsg::TurnComplete(event) => {
                last_agent_message = event.last_agent_message;
                break;
            }
            _ => {}
        }
    }

    // 6. Shutdown subagent
    let _ = codex.submit(Op::Shutdown {}).await;
    child_cancel.cancel();

    // 7. Parse structured JSON response
    parse_guardian_assessment(last_agent_message.as_deref())
}
```

### Guardian Configuration (Locked Down)

```rust
fn build_guardian_subagent_config(parent_config, ...) -> anyhow::Result<Config> {
    let mut config = parent_config.clone();

    // Specific model for assessment
    config.model = Some(active_model.to_string());
    config.developer_instructions = Some(guardian_policy_prompt());

    // LOCK DOWN: No tool execution
    config.permissions.approval_policy =
        Constrained::allow_only(AskForApproval::Never);

    // LOCK DOWN: Read-only sandbox
    config.permissions.sandbox_policy =
        Constrained::allow_only(SandboxPolicy::new_read_only_policy());

    // Disable non-essential features
    config.features.disable(Feature::Collab)?;
    config.features.disable(Feature::WebSearchRequest)?;
    config.features.disable(Feature::WebSearchCached)?;

    Ok(config)
}
```

### Guardian Risk Assessment Output

```rust
pub struct GuardianAssessment {
    pub risk_level: GuardianRiskLevel,  // Low / Medium / High
    pub risk_score: u32,                // 0-100
    pub rationale: String,              // Reasoning
    pub evidence: Vec<GuardianEvidence>,
}

// Fail closed on high risk
let approved = assessment.risk_score < GUARDIAN_APPROVAL_RISK_THRESHOLD;
```

### Timeout & Failure Handling

```rust
let review = tokio::select! {
    review = run_guardian_subagent(...) => Some(review),
    _ = tokio::time::sleep(GUARDIAN_REVIEW_TIMEOUT) => {
        cancel_token.cancel();
        None  // Timeout → fail closed
    }
};

let assessment = match review {
    Some(Ok(a))  => a,
    Some(Err(e)) => GuardianAssessment {
        risk_level: High, risk_score: 100,
        rationale: format!("Guardian review failed: {e}"),
    },
    None => GuardianAssessment {
        risk_level: High, risk_score: 100,
        rationale: "Guardian review timed out".to_string(),
    },
};
```

**Fail-closed**: Any error or timeout → `risk_score=100` → rejection.

---

## 4. Event Forwarding (Parent ↔ Subagent Bridge)

### Forward Events: Subagent → Parent

```rust
// codex-rs/core/src/codex_delegate.rs
async fn forward_events(
    codex: Arc<Codex>,
    tx_sub: Sender<Event>,
    parent_session: Arc<Session>,
    parent_ctx: Arc<TurnContext>,
    cancel_token: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                shutdown_delegate(&codex).await;
                break;
            }
            event = codex.next_event() => {
                match event.msg {
                    // SKIP: noise events
                    EventMsg::AgentMessageDelta(_) |
                    EventMsg::TokenCount(_) |
                    EventMsg::SessionConfigured(_) => {}

                    // ROUTE: approval requests → parent session
                    EventMsg::ExecApprovalRequest(e) => {
                        handle_exec_approval(
                            &codex, id, &parent_session, &parent_ctx, e, &cancel_token
                        ).await;
                    }
                    EventMsg::ApplyPatchApprovalRequest(e) => {
                        handle_patch_approval(
                            &codex, id, &parent_session, &parent_ctx, e, &cancel_token
                        ).await;
                    }
                    EventMsg::RequestUserInput(e) => {
                        handle_request_user_input(
                            &codex, id, &parent_session, &parent_ctx, e, &cancel_token
                        ).await;
                    }

                    // FORWARD: everything else → caller
                    other => {
                        let _ = tx_sub.send(other).or_cancel(&cancel_token).await;
                    }
                }
            }
        }
    }
}
```

### Forward Ops: Parent → Subagent

```rust
async fn forward_ops(
    codex: Arc<Codex>,
    rx_ops: Receiver<Submission>,
    cancel_token: CancellationToken,
) {
    loop {
        let submission = rx_ops.recv().or_cancel(&cancel_token).await;
        let _ = codex.submit_with_id(submission).await;
    }
}
```

---

## 5. MCP Server Orchestration

### Startup: Parallel Initialization with JoinSet

```rust
// codex-rs/core/src/mcp_connection_manager.rs
pub async fn new(mcp_servers: &HashMap<String, McpServerConfig>, ...) -> (Self, CancellationToken) {
    let mut join_set = JoinSet::new();

    for (server_name, cfg) in mcp_servers.iter().filter(|(_, c)| c.enabled) {
        let cancel = cancel_token.child_token();

        // Emit startup status
        emit_update(&tx_event, McpStartupUpdateEvent {
            server: server_name.clone(),
            status: McpStartupStatus::Starting,
        }).await;

        // Spawn parallel startup task per server
        join_set.spawn(async move {
            let outcome = async_managed_client.client().await;

            if cancel.is_cancelled() {
                return (server_name, Err(StartupOutcomeError::Cancelled));
            }

            let status = match &outcome {
                Ok(_)    => McpStartupStatus::Ready,
                Err(err) => McpStartupStatus::Failed { error: display(err) },
            };

            emit_update(&tx_event, McpStartupUpdateEvent { server, status }).await;
            (server_name, outcome)
        });
    }

    // Await all and emit summary
    tokio::spawn(async move {
        let outcomes = join_set.join_all().await;
        let summary = build_summary(outcomes);
        tx_event.send(EventMsg::McpStartupComplete(summary)).await;
    });

    (manager, cancel_token)
}
```

### Resource Listing: Parallel with Pagination

```rust
pub async fn list_all_resources(&self) -> HashMap<String, Vec<Resource>> {
    let mut join_set = JoinSet::new();

    for (name, client) in &self.clients {
        join_set.spawn(async move {
            let mut collected = Vec::new();
            let mut cursor = None;
            loop {
                let response = client.list_resources(cursor).await?;
                collected.extend(response.resources);
                match response.next_cursor {
                    Some(next) if cursor.as_ref() != Some(&next) => cursor = Some(next),
                    _ => return Ok(collected),
                }
            }
        });
    }

    // Aggregate results from all servers
    let mut aggregated = HashMap::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((name, Ok(resources))) => { aggregated.insert(name, resources); }
            Ok((name, Err(err)))      => { warn!("Failed: {name}: {err:#}"); }
            Err(err)                  => { warn!("Task panic: {err:#}"); }
        }
    }
    aggregated
}
```

### Tool Call Flow

```
Model emits FunctionCall with MCP tool name
        │
ToolRouter recognizes as MCP tool
        │
McpHandler.handle(invocation)
        │
McpConnectionManager.call_tool(server, tool, arguments)
        │
RmcpClient (per-server) → JSON-RPC 2.0 call
        │
MCP Server processes & returns CallToolResult
        │
Result converted to ToolOutput::Mcp { result }
        │
Serialized back to ResponseInputItem for model
```

---

## 6. MCP Client Architecture

```rust
// codex-rs/rmcp-client/src/rmcp_client.rs
pub struct RmcpClient {
    state: Mutex<ClientState>,              // Connection state machine
    transport_recipe: TransportRecipe,      // How to connect
    initialize_context: Mutex<Option<InitializeContext>>,
    session_recovery_lock: Mutex<()>,
}

enum ClientState {
    Connecting { transport: Option<PendingTransport> },
    Ready {
        _process_group_guard: Option<ProcessGroupGuard>,
        service: Arc<RunningService<RoleClient, LoggingClientHandler>>,
        oauth: Option<OAuthPersistor>,
    },
}

enum TransportRecipe {
    Stdio { program, args, env, cwd },
    StreamableHttp { server_name, url, bearer_token, http_headers },
}
```

### Transport Types

| Transport | Protocol | Use Case |
|---|---|---|
| `Stdio` | JSON-RPC over stdin/stdout | Local MCP servers |
| `StreamableHttp` | JSON-RPC over HTTP + SSE | Remote MCP servers |

---

## 7. Communication Protocol Summary

### Main Agent ↔ Subagent

```
async_channel::bounded(8)  →  Submissions (ops)
async_channel::unbounded   ←  Events (responses)
```

### Guardian ↔ User/Network

Approval requests from Guardian **bubble up to parent session**:

```
Guardian needs approval
    → Emits ExecApprovalRequest event
    → forward_events() intercepts
    → Routes to parent_session.handle_exec_approval()
    → Parent session asks user (or applies policy)
    → Response sent back to Guardian via Op
```

### MCP Client ↔ MCP Server

```
JSON-RPC 2.0 over:
  - stdio (local servers: stdin/stdout pipes)
  - HTTP + SSE (remote servers: streamable HTTP)
```

### App-Server ↔ Client (VS Code, etc.)

```
JSON-RPC 2.0 over:
  - stdio (default: newline-delimited JSON)
  - WebSocket (experimental)
```

---

## 8. Failure Handling

| Failure | Response | Principle |
|---|---|---|
| Guardian timeout (90s) | `risk_score=100`, rejection | Fail closed |
| Guardian error | `risk_score=100`, rejection | Fail closed |
| MCP server timeout | Tool call error returned to model | Model retries or adapts |
| MCP server crash | Startup marked "Failed" | Other servers unaffected |
| Subagent cancellation | Child token cascades | Graceful teardown |
| Network approval miss | Route to parent for decision | Escalate, don't assume |

### Guardian Fail-Closed Philosophy

```
Error / Timeout / Parsing Failure
        │
        ▼
GuardianAssessment {
    risk_level: High,
    risk_score: 100,
    rationale: "...",
}
        │
        ▼
risk_score >= THRESHOLD → REJECTED
```

Any uncertainty = rejection. The user/developer can override by changing
approval policy to something less restrictive.

---

## 9. Parallel Execution Patterns

| Context | Mechanism | Parallelism |
|---|---|---|
| MCP server startup | `JoinSet::spawn()` per server | All servers in parallel |
| MCP resource listing | `JoinSet::spawn()` per server | All servers in parallel |
| Tool calls (same turn) | `RwLock<()>` read/write guard | Parallel for read-only tools |
| Subagent spawning | `tokio::spawn()` | One subagent at a time |
| Event forwarding | `tokio::spawn()` per direction | Inbound + outbound parallel |

---

## 10. State Isolation Diagram

```
┌──────────────────────────────────────────────────┐
│              Arc<Services>  (SHARED)               │
│                                                    │
│  auth_manager ─── models_manager ─── mcp_manager  │
│  skills_manager ── plugins_manager ── file_watcher │
│  agent_control ── network_approval                 │
│                                                    │
├──────────────────┬───────────────────────────────┤
│  MAIN SESSION    │  GUARDIAN SESSION              │
│                  │                                │
│  config:         │  config:                       │
│   workspace-write│   read-only                    │
│   on-request     │   never (approval=never)       │
│                  │                                │
│  history:        │  history:                      │
│   [full conv]    │   [approval request only]      │
│                  │                                │
│  tools:          │  tools:                        │
│   shell, patch,  │   NONE (locked down)           │
│   mcp, read, ... │                                │
│                  │                                │
│  exec_policy:    │  exec_policy:                  │
│   [user rules]   │   [empty — no tool execution]  │
└──────────────────┴───────────────────────────────┘
```

---

## Key Files

| File | Purpose |
|---|---|
| `core/src/codex_delegate.rs` | Subagent spawning, event/op forwarding |
| `core/src/guardian.rs` | Guardian subagent lifecycle |
| `core/src/mcp_connection_manager.rs` | MCP server orchestration |
| `core/src/tools/handlers/mcp.rs` | MCP tool call handler |
| `rmcp-client/src/rmcp_client.rs` | MCP client (transport, state machine) |
| `app-server/src/lib.rs` | App-server request routing |
| `app-server/src/transport.rs` | WebSocket/stdio transport |
| `app-server/src/outgoing_message.rs` | Request/response callback pattern |
