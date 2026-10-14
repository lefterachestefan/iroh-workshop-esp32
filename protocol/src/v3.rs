//! A lamp with a light sensor: [`v2`](crate::v2), plus automatic mode, where the
//! lamp switches itself on when it gets dark. Step 5.
//!
//! Every answer is the whole [`LightState`], not just on/off, so a page always
//! shows the sensor reading and the mode along with the light.

use irpc::{
    channel::{mpsc, oneshot},
    rpc_requests,
};
use serde::{Deserialize, Serialize};

pub use crate::v1::{Get, Set};
pub use crate::v2::Watch;

/// A new version, a new ALPN.
pub const ALPN: &[u8] = b"iroh-esp32-workshop/light/3";

#[rpc_requests(message = LightMessage)]
#[derive(Debug, Serialize, Deserialize)]
pub enum LightProtocol {
    /// Switch the light on or off by hand. Leaves automatic mode.
    #[rpc(tx = oneshot::Sender<LightState>)]
    Set(Set),
    /// Switch automatic mode on or off.
    #[rpc(tx = oneshot::Sender<LightState>)]
    SetAuto(SetAuto),
    /// How dark it has to get, in percent of full brightness, before the lamp
    /// switches on in automatic mode.
    #[rpc(tx = oneshot::Sender<LightState>)]
    SetThreshold(SetThreshold),
    #[rpc(tx = oneshot::Sender<LightState>)]
    Get(Get),
    /// Get the current state, then every change (including the sensor
    /// reading), until you hang up.
    #[rpc(tx = mpsc::Sender<LightState>)]
    Watch(Watch),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetAuto {
    pub auto: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetThreshold {
    pub threshold: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightState {
    /// Is the light on?
    pub on: bool,
    /// Does the lamp switch itself, following the sensor?
    pub auto: bool,
    /// How bright it is around the lamp, 0 (dark) to 100 (bright).
    pub level: u8,
    /// In automatic mode, the light goes on below this level and off above it.
    pub threshold: u8,
}
