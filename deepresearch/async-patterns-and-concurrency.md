# Async Patterns & Concurrency

Deep-dive into the concurrency model, channel architecture, cancellation patterns,
streaming, backpressure, and synchronization primitives in Codex.

---

## 1. Runtime Model

Codex uses **multi-threaded Tokio** with work-stealing scheduler:

```toml
# codex-rs/app-server/Cargo.toml
tokio = { features = ["io-std", "macros", "process", "rt-multi-thread", "signal"] }
```

- **Multi-threaded runtime** (`rt-multi-thread`) — not single-threaded
- **Process management** for spawning subprocesses
- **Signal handling** for graceful shutdown
- **Async I/O** for stdio/file operations

---

## 2. Three-Tier Channel Architecture

### Tier 1: Core Engine (async-channel)

```rust
// codex-rs/core/src/codex.rs
use async_channel::{Sender, Receiver};

const SUBMISSION_CHANNEL_CAPACITY: usize = 8;

let (tx_sub, rx_sub) = async_channel::bounded(SUBMISSION_CHANNEL_CAPACITY);
let (tx_event, rx_event) = async_channel::unbounded();
let (tx_status, rx_status) = watch::channel(AgentStatus::default());
```

**Why async-channel instead of tokio::sync::mpsc?**
- Runtime-agnostic (not Tokio-specific)
- Both bounded and unbounded variants
- Works alongside `watch::Receiver` for state subscriptions

### Tier 2: App-Server Transport (tokio mpsc)

```rust
// codex-rs/app-server/src/transport.rs
pub(crate) const CHANNEL_CAPACITY: usize = 128;

let (writer_tx, writer_rx) = mpsc::channel::<OutgoingMessage>(CHANNEL_CAPACITY);
let (writer_control_tx, writer_control_rx) = mpsc::channel::<WebSocketMessage>(CHANNEL_CAPACITY);
```

### Tier 3: Request/Response Callbacks (oneshot)

```rust
// codex-rs/app-server/src/outgoing_message.rs
let (tx_approve, rx_approve) = oneshot::channel();
```

---

## 3. Channel Types & Their Roles

| Primitive | Capacity | Use Case | File |
|---|---|---|---|
| `async_channel::bounded(8)` | 8 | Submissions (user ops) | `core/src/codex.rs` |
| `async_channel::unbounded` | ∞ | Events (agent output) | `core/src/codex.rs` |
| `watch::channel` | 1 (latest) | Agent status broadcast | `core/src/codex.rs` |
| `mpsc::channel(128)` | 128 | Transport messages | `app-server/transport.rs` |
| `oneshot::channel` | 1 | Request/response callback | `app-server/outgoing_message.rs` |
| `broadcast::channel` | N | Thread creation events | `app-server/lib.rs` |
| `mpsc::unbounded_channel` | ∞ | TUI app events | `tui/src/public_widgets/` |

---

## 4. Cancellation Architecture

### 4.1 `OrCancelExt` Trait (Utility)

```rust
// codex-rs/async-utils/src/lib.rs
#[async_trait]
pub trait OrCancelExt: Sized {
    type Output;
    async fn or_cancel(self, token: &CancellationToken) -> Result<Self::Output, CancelErr>;
}

#[async_trait]
impl<F: Future + Send> OrCancelExt for F {
    async fn or_cancel(self, token: &CancellationToken) -> Result<Self::Output, CancelErr> {
        tokio::select! {
            _ = token.cancelled() => Err(CancelErr::Cancelled),
            res = self => Ok(res),
        }
    }
}
```

Any future can be wrapped with `.or_cancel(&token)` to become cancellation-aware.

### 4.2 Hierarchical Child Tokens

```rust
// codex-rs/core/src/tasks/mod.rs
pub async fn spawn_task<T: SessionTask>(...) {
    let cancellation_token = CancellationToken::new();
    let task_cancellation_token = cancellation_token.child_token();  // Child!

    let handle = tokio::spawn(async move {
        task.run(session_ctx, ctx, input, task_cancellation_token.child_token()).await;
        // ...
    });

    let running_task = RunningTask {
        cancellation_token,  // Parent token — cancelling this cancels children
        handle: Arc::new(AbortOnDropHandle::new(handle)),
        // ...
    };
}
```

Cancelling a parent token cascades to all children. This enables hierarchical
teardown: cancel a turn → all its tool executions cancel too.

### 4.3 AbortOnDropHandle

```rust
// Used in tool execution
let handle: AbortOnDropHandle<Result<...>> =
    AbortOnDropHandle::new(tokio::spawn(async move { ... }));
```

Automatically aborts the spawned task when the handle is dropped. Ensures no
leaked tasks if the parent panics or is cancelled.

---

## 5. Tool Execution Concurrency

### Read/Write Lock Pattern

```rust
// codex-rs/core/src/tools/parallel.rs
pub struct ToolCallRuntime {
    parallel_execution: Arc<RwLock<()>>,
}

pub fn handle_tool_call(self, call: ToolCall, cancellation_token: CancellationToken)
    -> impl Future<...>
{
    let supports_parallel = self.router.tool_supports_parallel(&call.tool_name);

    async move {
        let _guard = if supports_parallel {
            Either::Left(lock.read().await)   // Read lock → parallel OK
        } else {
            Either::Right(lock.write().await) // Write lock → serial only
        };

        // Execute tool under guard
        router.dispatch_tool_call(...).await
    }
}
```

**Read-safe tools** (grep, read_file, list_dir) acquire read lock → run concurrently.
**Mutating tools** (shell, apply_patch) acquire write lock → exclusive access.

### Cancellation Racing

```rust
tokio::select! {
    _ = cancellation_token.cancelled() => {
        Ok(Self::aborted_response(&call, elapsed))
    },
    res = async {
        let _guard = /* acquire lock */;
        router.dispatch_tool_call(...).await
    } => res,
}
```

Tool execution races against cancellation. If cancelled mid-execution,
an aborted response is generated with elapsed time.

---

## 6. Streaming Response Processing

### WebSocket Session Management

```rust
// codex-rs/core/src/client.rs
pub struct ModelClientSession {
    client: ModelClient,
    websocket_session: WebsocketSession,
    turn_state: Arc<OnceLock<String>>,  // Sticky routing token
}

#[derive(Default)]
struct WebsocketSession {
    connection: Option<ApiWebSocketConnection>,
    last_request: Option<ResponsesApiRequest>,
    last_response_rx: Option<oneshot::Receiver<LastResponse>>,
}
```

- Sessions maintain sticky routing via `OnceLock<String>`
- WebSocket connections are reused across turns
- `oneshot::Receiver` tracks response completion

### Stream Parsing

```rust
// codex-rs/core/src/codex.rs
pub struct AssistantMessageStreamParsers {
    parsers_by_item: HashMap<String, AssistantTextStreamParser>,
}
```

Multiple concurrent response items are parsed via per-item stream parsers.

---

## 7. App-Server Main Loop (Select! Multiplexing)

```rust
// codex-rs/app-server/src/lib.rs
loop {
    tokio::select! {
        // Branch 1: Incoming transport events
        Some(event) = transport_event_rx.recv() => {
            match event {
                TransportEvent::ConnectionOpened { .. } => { /* register */ }
                TransportEvent::ConnectionClosed { .. } => { /* deregister */ }
                TransportEvent::IncomingMessage { .. } => {
                    match message {
                        JSONRPCMessage::Request(r)      => process_request(r),
                        JSONRPCMessage::Response(r)     => process_response(r),
                        JSONRPCMessage::Notification(n) => process_notification(n),
                    }
                }
            }
        }

        // Branch 2: Thread creation broadcasts
        created = thread_created_rx.recv(), if listen_for_threads => {
            match created {
                Ok(thread_id) => try_attach_thread_listener(thread_id),
                Err(RecvError::Lagged(_)) => warn!("receiver lagged"),
                Err(RecvError::Closed) => { listen_for_threads = false; }
            }
        }
    }
}
```

---

## 8. WebSocket Connection Lifecycle

### Inbound/Outbound Split

```rust
// codex-rs/app-server/src/transport.rs
async fn run_websocket_connection(connection_id, stream, transport_event_tx) {
    let (websocket_writer, websocket_reader) = websocket_stream.split();

    // Two independent tasks
    let mut outbound_task = tokio::spawn(run_websocket_outbound_loop(
        websocket_writer, writer_rx, writer_control_rx, disconnect_token.clone()
    ));

    let mut inbound_task = tokio::spawn(run_websocket_inbound_loop(
        websocket_reader, transport_event_tx, writer_tx, writer_control_tx,
        connection_id, disconnect_token.clone()
    ));

    // Wait for either to finish — then clean up both
    tokio::select! {
        _ = &mut outbound_task => {
            disconnect_token.cancel();
            inbound_task.abort();
        }
        _ = &mut inbound_task => {
            disconnect_token.cancel();
            outbound_task.abort();
        }
    }

    transport_event_tx.send(TransportEvent::ConnectionClosed { connection_id }).await;
}
```

### Outbound Loop (Multi-Channel Select)

```rust
async fn run_websocket_outbound_loop(
    websocket_writer, writer_rx, writer_control_rx, disconnect_token
) {
    loop {
        tokio::select! {
            _ = disconnect_token.cancelled() => break,
            message = writer_control_rx.recv() => { /* ping/pong */ },
            message = writer_rx.recv() => { /* data messages */ },
        }
    }
}
```

---

## 9. Backpressure & Overload Handling

### Bounded Channel Backpressure

Channel capacity of 128 means senders block when queue is full.

### Explicit Overload Detection

```rust
// codex-rs/app-server/src/transport.rs
async fn enqueue_incoming_message(transport_event_tx, writer, connection_id, message) -> bool {
    match transport_event_tx.try_send(event) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            // Queue full → send overload error to client
            let overload_error = OutgoingMessage::Error(OutgoingError {
                id: request.id,
                error: JSONRPCErrorError {
                    code: OVERLOADED_ERROR_CODE,
                    message: "Server overloaded; retry later.".to_string(),
                    data: None,
                },
            });
            writer.try_send(overload_error);
            true
        }
        Err(TrySendError::Closed(_)) => false,
    }
}
```

### Slow Connection Disconnection

```rust
async fn send_message_to_connection(connections, connection_id, message) -> bool {
    match writer.try_send(message) {
        Ok(()) => false,
        Err(TrySendError::Full(_)) => {
            warn!("disconnecting slow connection: {connection_id:?}");
            connection_state.request_disconnect();
            true
        }
    }
}
```

---

## 10. Synchronization Primitives

### Atomic for Lock-Free Counters

```rust
let connection_counter = Arc::new(AtomicU64::new(1));
let id = connection_counter.fetch_add(1, Ordering::Relaxed);
```

### Arc + RwLock for Shared Collections

```rust
pub(crate) struct ConnectionState {
    pub outbound_initialized: Arc<AtomicBool>,
    pub outbound_experimental_api_enabled: Arc<AtomicBool>,
    pub outbound_opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
}
```

- `Arc<AtomicBool>` for simple flags (cheapest)
- `Arc<RwLock<>>` for collections with occasional writes
- Avoids locking on every read

### Tokio Mutex for Async-Safe Locking

```rust
use tokio::sync::Mutex;

pub struct OutgoingMessageSender {
    request_id_to_callback: Mutex<HashMap<RequestId, PendingCallbackEntry>>,
}
```

Standard `std::sync::Mutex` cannot be held across `.await` — Tokio Mutex can.

### Memory Ordering Strategy

```rust
// Release: ensures all prior writes visible to readers
connection_state.outbound_experimental_api_enabled
    .store(value, Ordering::Release);

// Relaxed: sufficient for ID sequencing (no ordering constraints)
connection_counter.fetch_add(1, Ordering::Relaxed);
```

---

## 11. Graceful Shutdown

```rust
// codex-rs/app-server/src/lib.rs
struct ShutdownState {
    requested: bool,
    forced: bool,
}

impl ShutdownState {
    fn on_signal(&mut self, connection_count: usize, running_turn_count: usize) {
        if self.requested {
            self.forced = true;  // Second signal → force
            return;
        }
        self.requested = true;
        info!("entering graceful drain (connections={}, turns={})",
            connection_count, running_turn_count);
    }

    fn update(&mut self, running_turn_count: usize) -> ShutdownAction {
        if !self.requested { return ShutdownAction::Noop; }
        if self.forced || running_turn_count == 0 { return ShutdownAction::Finish; }
        ShutdownAction::Noop  // Wait for in-flight turns to complete
    }
}
```

First signal → drain gracefully. Second signal → force shutdown.

---

## 12. Request-Response Callback Pattern

```rust
// codex-rs/app-server/src/outgoing_message.rs
pub struct OutgoingMessageSender {
    next_server_request_id: AtomicI64,
    sender: mpsc::Sender<OutgoingEnvelope>,
    request_id_to_callback: Mutex<HashMap<RequestId, PendingCallbackEntry>>,
}

async fn send_request(...) -> (RequestId, oneshot::Receiver<ClientRequestResult>) {
    let id = self.next_request_id();  // Atomic increment
    let (tx, rx) = oneshot::channel();

    {
        let mut callbacks = self.request_id_to_callback.lock().await;
        callbacks.insert(id, PendingCallbackEntry { callback: tx, ... });
    }

    self.sender.send(OutgoingEnvelope::Broadcast { message }).await?;
    (id, rx)  // Caller awaits the oneshot receiver
}

pub async fn notify_client_response(&self, id: RequestId, result: Result) {
    let entry = self.take_request_callback(&id).await;
    if let Some((_, entry)) = entry {
        let _ = entry.callback.send(Ok(result));  // Wake waiting caller
    }
}
```

**Race condition prevention**:
- Atomic request ID generation (no duplicates)
- Mutex-protected lookup table
- Oneshot channel ensures response consumed exactly once

---

## 13. Select! Pattern Taxonomy

### Pattern 1: Cancellation Racing
```rust
tokio::select! {
    _ = token.cancelled() => Err(CancelErr),
    res = work_future => Ok(res),
}
```

### Pattern 2: Multi-Source Multiplexing
```rust
tokio::select! {
    _ = disconnect_token.cancelled() => break,
    msg = control_rx.recv() => handle_control(msg),
    msg = data_rx.recv() => handle_data(msg),
}
```

### Pattern 3: First-Finish Cleanup
```rust
tokio::select! {
    _ = &mut outbound_task => {
        disconnect_token.cancel();
        inbound_task.abort();
    }
    _ = &mut inbound_task => {
        disconnect_token.cancel();
        outbound_task.abort();
    }
}
```

---

## Summary

| Aspect | Approach |
|---|---|
| **Runtime** | Multi-threaded Tokio with work-stealing |
| **Message Passing** | 3-tier: async-channel (core), mpsc (transport), oneshot (callbacks) |
| **Status Broadcast** | `watch::channel` for latest-value subscriptions |
| **Cancellation** | Hierarchical `CancellationToken` with child tokens |
| **Tool Parallelism** | `RwLock<()>` — read lock = parallel, write lock = serial |
| **Task Lifecycle** | `AbortOnDropHandle` — auto-abort on drop |
| **Backpressure** | Bounded channels + `try_send` overload detection |
| **Slow Clients** | Disconnect on full outbound queue |
| **Shutdown** | Two-phase: graceful drain → forced exit |
| **Sync Primitives** | Atomics (counters), RwLock (collections), Tokio Mutex (async-safe) |
