//! WebSocket integration test.
//!
//! Starts the server on a random port, connects a WebSocket client,
//! sends a StartPipeline Op, and verifies the PipelineCreated event.

use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::connect_async;
use futures::{SinkExt, StreamExt};

use beaver_builder::protocol::ops::Op;
use beaver_builder::protocol::events::Event;
use beaver_builder::protocol::messages::WsMessage;
use beaver_builder::application::orchestrator::PipelineOrchestrator;
use beaver_builder::infrastructure::ws_server;

/// Start the full server stack on a random available port and return the port.
async fn start_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (sq_tx, sq_rx) = mpsc::channel::<Op>(64);
    let (eq_tx, _eq_rx) = broadcast::channel::<Event>(256);

    let orchestrator = PipelineOrchestrator::new(sq_rx, eq_tx.clone());
    tokio::spawn(orchestrator.run());

    // Spawn the axum server using the pre-bound listener
    let sq_tx_clone = sq_tx.clone();
    let eq_tx_clone = eq_tx.clone();
    tokio::spawn(async move {
        let app = ws_server::build_router(sq_tx_clone, eq_tx_clone);
        axum::serve(listener, app).await.unwrap();
    });

    // Give server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    port
}

#[tokio::test]
async fn ws_connect_send_op_receive_event() {
    let port = start_server().await;
    let url = format!("ws://127.0.0.1:{port}/ws");

    let (ws_stream, _) = connect_async(&url).await.expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();

    // Send StartPipeline Op wrapped in WsMessage
    let op = WsMessage::Op {
        payload: Op::StartPipeline {
            task_id: "test_task".into(),
            workspace_id: "test_ws".into(),
        },
    };
    let json = serde_json::to_string(&op).unwrap();
    write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await.unwrap();

    // Read events - should get PipelineCreated and StageTransition
    let mut received_events: Vec<Event> = Vec::new();
    for _ in 0..2 {
        match tokio::time::timeout(std::time::Duration::from_secs(2), read.next()).await {
            Ok(Some(Ok(msg))) => {
                if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        if let WsMessage::Event { payload } = ws_msg {
                            received_events.push(payload);
                        }
                    }
                }
            }
            _ => break,
        }
    }

    assert!(received_events.len() >= 1, "Expected at least 1 event, got {}", received_events.len());

    // Verify PipelineCreated event
    let has_pipeline_created = received_events.iter().any(|e| matches!(e, Event::PipelineCreated { task_id, .. } if task_id == "test_task"));
    assert!(has_pipeline_created, "Expected PipelineCreated event for test_task");
}
