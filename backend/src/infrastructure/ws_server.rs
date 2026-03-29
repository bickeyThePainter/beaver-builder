use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::CorsLayer;

use crate::protocol::events::Event;
use crate::protocol::messages::WsMessage;
use crate::protocol::ops::Op;

#[derive(Clone)]
struct AppState {
    sq_tx: mpsc::Sender<Op>,
    eq_tx: broadcast::Sender<Event>,
}

/// Build the Axum router. Useful for testing with a pre-bound listener.
pub fn build_router(sq_tx: mpsc::Sender<Op>, eq_tx: broadcast::Sender<Event>) -> Router {
    let state = AppState { sq_tx, eq_tx };
    Router::new()
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(addr: &str, sq_tx: mpsc::Sender<Op>, eq_tx: broadcast::Sender<Event>) {
    let app = build_router(sq_tx, eq_tx);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind TCP listener");
    axum::serve(listener, app)
        .await
        .expect("Axum server error");
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut eq_rx = state.eq_tx.subscribe();

    // Forward events from EQ to WebSocket client
    let send_task = tokio::spawn(async move {
        while let Ok(event) = eq_rx.recv().await {
            let msg = WsMessage::Event { payload: event };
            if let Ok(json) = serde_json::to_string(&msg) {
                if ws_tx.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Forward Ops from WebSocket client to SQ
    let sq_tx = state.sq_tx;
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let Message::Text(text) = msg {
                match serde_json::from_str::<WsMessage>(&text) {
                    Ok(WsMessage::Op { payload }) => {
                        if sq_tx.send(payload).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {
                        tracing::warn!("Client sent an Event frame (ignored)");
                    }
                    Err(e) => {
                        tracing::warn!("Invalid WS message: {e}");
                    }
                }
            }
        }
    });

    // Wait for either task to finish (client disconnect or server shutdown)
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::debug!("WebSocket connection closed");
}
