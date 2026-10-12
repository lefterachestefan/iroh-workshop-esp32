//! [`v1`](crate::v1), plus watching the light: the device tells you every time
//! it changes, e.g. when someone presses its button. Step 4.

use irpc::{
    channel::{mpsc, oneshot},
    rpc_requests,
};
use serde::{Deserialize, Serialize};

pub use crate::v1::{Get, Set};

/// A new version, a new ALPN.
pub const ALPN: &[u8] = b"iroh-esp32-workshop/light/2";

#[rpc_requests(message = LightMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum LightProtocol {
    /// Switch the light on or off. Answers with the new state.
    #[rpc(tx = oneshot::Sender<bool>)]
    Set(Set),
    /// Is the light on?
    #[rpc(tx = oneshot::Sender<bool>)]
    Get(Get),
    /// Get the current state, then every change, until you hang up.
    #[rpc(tx = mpsc::Sender<bool>)]
    Watch(Watch),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Watch;
