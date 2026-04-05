mod domain;
mod application;
mod llm;
mod infrastructure;
mod protocol;

use application::orchestrator::PipelineOrchestrator;
use infrastructure::ws_server;
use llm::factory::LlmProviderFactory;
use protocol::events::Event;
use protocol::ops::Op;
use tokio::sync::{broadcast, mpsc};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("beaver_builder=debug".parse().expect("valid directive")),
        )
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
