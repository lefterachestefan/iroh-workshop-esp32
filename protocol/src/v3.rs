//! A lamp with a light sensor: [`v2`](crate::v2), plus automatic mode, where the
//! lamp switches itself on when it gets dark. Step 5.
//!
//! Every answer is the whole [`LightState`]: the light, the sensor reading and
//! all [`Settings`]. A client takes from it what it shows. The settings change
//! together, with one [`Configure`](LightProtocol::Configure).

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
    /// Replace all settings at once. To change one, take the others from the
    /// last [`LightState`].
    #[rpc(tx = oneshot::Sender<LightState>)]
    Configure(Settings),
    #[rpc(tx = oneshot::Sender<LightState>)]
    Get(Get),
    /// Get the current state, then every change (including the sensor
    /// reading and the settings), until you hang up.
    #[rpc(tx = mpsc::Sender<LightState>)]
    Watch(Watch),
}

/// How the lamp behaves. Kept across restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Does the lamp switch itself, following the sensor?
    pub auto: bool,
    /// In automatic mode, the light goes on below this level and off above it,
    /// in percent.
    pub threshold: u8,
    /// In automatic mode, how long it has to stay darker (or brighter) than the
    /// threshold before the lamp switches, in milliseconds. Longer means a
    /// passing shadow doesn't switch it; shorter means it reacts faster.
    pub delay_ms: u16,
}

/// Everything about the lamp, in every answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightState {
    /// Is the light on?
    pub on: bool,
    /// How bright it is around the lamp, 0 (dark) to 100 (bright).
    pub level: u8,
    pub settings: Settings,
}
