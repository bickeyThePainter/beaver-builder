use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::CorsLayer;
use tracing::{debug, info, warn};

use crate::protocol::{events::Event, messages::WsMessage, ops::Op};

/// Shared state passed to axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub sq_tx: mpsc::Sender<Op>,
    pub eq_tx: broadcast::Sender<Event>,
}

/// Build the axum router with /ws route and CORS.
pub fn build_router(sq_tx: mpsc::Sender<Op>, eq_tx: broadcast::Sender<Event>) -> Router {
    let state = AppState { sq_tx, eq_tx };
    Router::new()
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the WS server, binding to the given address.
pub async fn serve(addr: &str, sq_tx: mpsc::Sender<Op>, eq_tx: broadcast::Sender<Event>) {
    let app = build_router(sq_tx, eq_tx);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind address");
    info!("WebSocket server listening on {addr}");
    axum::serve(listener, app)
        .await
        .expect("server error");
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut eq_rx = state.eq_tx.subscribe();

    debug!("new WebSocket connection");

    loop {
        tokio::select! {
            // Client → Server: parse Op, forward to SQ
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsMessage>(&text) {
                            Ok(WsMessage::Op(op)) => {
                                debug!("received op from client");
                                if let Err(e) = state.sq_tx.send(op).await {
                                    warn!("failed to forward op to SQ: {e}");
                                    break;
                                }
                            }
                            Ok(WsMessage::Event(_)) => {
                                warn!("client sent an Event, ignoring");
                            }
                            Err(e) => {
                                warn!("failed to parse WsMessage: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("client disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("websocket read error: {e}");
                        break;
                    }
                    _ => {} // Ping/Pong/Binary — ignore
                }
            }

            // Server → Client: forward Events from EQ
            event = eq_rx.recv() => {
                match event {
                    Ok(ev) => {
                        let envelope = WsMessage::Event(ev);
                        match serde_json::to_string(&envelope) {
                            Ok(json) => {
                                if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                                    warn!("failed to send event to client: {e}");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("failed to serialize event: {e}");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("client lagged, dropped {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("event queue closed");
                        break;
                    }
                }
            }
        }
    }

    debug!("WebSocket handler exiting");
}
