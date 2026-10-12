//! Switch the light, and ask whether it's on. Steps 2 and 3.

use irpc::{channel::oneshot, rpc_requests};
use serde::{Deserialize, Serialize};

/// Identifies the protocol when connecting, like a port number.
pub const ALPN: &[u8] = b"iroh-esp32-workshop/light/1";

#[rpc_requests(message = LightMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum LightProtocol {
    /// Switch the light on or off. Answers with the new state.
    #[rpc(tx = oneshot::Sender<bool>)]
    Set(Set),
    /// Is the light on?
    #[rpc(tx = oneshot::Sender<bool>)]
    Get(Get),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Set {
    pub on: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Get;
