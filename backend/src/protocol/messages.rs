use serde::{Deserialize, Serialize};

use super::events::Event;
use super::ops::Op;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum WsMessage {
    #[serde(rename = "op")]
    Op(Op),
    #[serde(rename = "event")]
    Event(Event),
}
