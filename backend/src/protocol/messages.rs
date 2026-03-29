use serde::{Deserialize, Serialize};
use super::{ops::Op, events::Event};

/// Top-level WebSocket frame. Every message over the wire is either
/// an Op (client -> server) or an Event (server -> client).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WsMessage {
    #[serde(rename = "op")]
    Op { payload: Op },

    #[serde(rename = "event")]
    Event { payload: Event },
}
