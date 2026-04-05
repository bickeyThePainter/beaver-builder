use std::time::Duration;

use beaver_builder::infrastructure::ws_server::build_router;
use beaver_builder::protocol::events::Event;
use beaver_builder::protocol::messages::WsMessage;
use beaver_builder::protocol::ops::Op;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

async fn start_server() -> (
    String,
    mpsc::Receiver<Op>,
    broadcast::Sender<Event>,
) {
    let (sq_tx, sq_rx) = mpsc::channel::<Op>(64);
    let (eq_tx, _) = broadcast::channel::<Event>(256);

    let app = build_router(sq_tx, eq_tx.clone());

    // Bind to random port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("ws://127.0.0.1:{}/ws", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("server error");
    });

    // Brief delay to let server start
    tokio::time::sleep(Duration::from_millis(50)).await;

    (url, sq_rx, eq_tx)
}

#[tokio::test]
async fn ws_connect_send_op_receive_event() {
    let (url, mut sq_rx, eq_tx) = start_server().await;

    // Connect
    let (ws_stream, _) = connect_async(&url)
        .await
        .expect("connect");
    let (mut write, mut read) = ws_stream.split();

    // Send a StartPipeline Op
    let op = Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "ws1".into(),
    };
    let envelope = WsMessage::Op(op);
    let json = serde_json::to_string(&envelope).expect("serialize");
    write.send(Message::Text(json.into())).await.expect("send");

    // Verify op arrives in SQ
    let received = tokio::time::timeout(Duration::from_secs(2), sq_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    assert!(matches!(received, Op::StartPipeline { .. }));

    // Send an event through EQ
    let event = Event::PipelineCreated {
        pipeline_id: "p1".into(),
        task_id: "t1".into(),
        stage: beaver_builder::domain::pipeline::Stage::Created,
    };
    eq_tx.send(event).expect("broadcast");

    // Verify event arrives in WS
    let msg = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    match msg {
        Message::Text(text) => {
            let parsed: WsMessage = serde_json::from_str(&text).expect("parse");
            match parsed {
                WsMessage::Event(Event::PipelineCreated {
                    pipeline_id,
                    task_id,
                    ..
                }) => {
                    assert_eq!(pipeline_id, "p1");
                    assert_eq!(task_id, "t1");
                }
                other => panic!("unexpected WsMessage: {other:?}"),
            }
        }
        other => panic!("expected text message, got {other:?}"),
    }
}

#[tokio::test]
async fn ws_envelope_has_correct_kind_field() {
    let (url, _sq_rx, eq_tx) = start_server().await;

    let (_, mut read) = connect_async(&url)
        .await
        .expect("connect")
        .0
        .split();

    // Send event
    let event = Event::Warning {
        pipeline_id: "p1".into(),
        message: "test warning".into(),
    };
    eq_tx.send(event).expect("broadcast");

    let msg = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("read error");

    if let Message::Text(text) = msg {
        let value: serde_json::Value = serde_json::from_str(&text).expect("parse");
        assert_eq!(value["kind"], "event");
        assert_eq!(value["payload"]["type"], "Warning");
    } else {
        panic!("expected text");
    }
}
