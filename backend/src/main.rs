mod domain;
mod application;
mod infrastructure;
mod protocol;

use tokio::sync::{broadcast, mpsc};
use tracing_subscriber::EnvFilter;

use crate::application::orchestrator::PipelineOrchestrator;
use crate::infrastructure::ws_server;
use crate::protocol::ops::Op;
use crate::protocol::events::Event;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("beaver_builder=debug".parse().unwrap()))
        .init();

    // Submission Queue: clients submit Ops here
    let (sq_tx, sq_rx) = mpsc::channel::<Op>(256);

    // Event Queue: orchestrator broadcasts Events here
    let (eq_tx, _eq_rx) = broadcast::channel::<Event>(1024);

    // Start the orchestrator (single-writer loop)
    let orchestrator = PipelineOrchestrator::new(sq_rx, eq_tx.clone());
    tokio::spawn(orchestrator.run());

    // Start WebSocket server
    let addr = "0.0.0.0:3001";
    tracing::info!("Beaver Builder listening on {addr}");
    ws_server::serve(addr, sq_tx, eq_tx).await;
}
