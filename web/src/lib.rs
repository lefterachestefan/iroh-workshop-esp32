//! iroh in the browser, for the workshop pages. One small API per step; the
//! protocols come from `../protocol`, the same crate the devices use.
//!
//! Browsers can't send UDP, so this endpoint reaches the devices over a relay.

use std::cell::RefCell;
use std::rc::Rc;

use iroh::{endpoint::presets, EndpointId};
use iroh_provision::client::Session;
use iroh_provision::{Network, PairingTicket, Status};
use iroh_provision_ble::web::WebTransport;
use light_protocol::{v1, v2, v3};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// The browser's iroh endpoint.
#[wasm_bindgen]
pub struct Node {
    endpoint: iroh::Endpoint,
}

#[wasm_bindgen]
impl Node {
    pub async fn spawn() -> Result<Node, JsError> {
        let endpoint = iroh::Endpoint::bind(presets::N0).await.map_err(js_err)?;
        Ok(Node { endpoint })
    }

    /// Steps 1 and 2: is the device there? It has no protocols yet, so it turns the
    /// connection away, and that answer is what we're after.
    pub async fn hello(&self, endpoint_id: String) -> Result<String, JsError> {
        let id = parse_id(&endpoint_id)?;
        match self
            .endpoint
            .connect(id, b"iroh-esp32-workshop/hello")
            .await
        {
            Ok(conn) => {
                conn.close(0u32.into(), b"bye");
                Ok("connected".into())
            }
            Err(err)
                if err
                    .to_string()
                    .contains("doesn't support any known protocol") =>
            {
                Ok("the device answered (it has no protocols yet)".into())
            }
            Err(err) => Err(js_err(err)),
        }
    }

    /// Step 3: the light switch, protocol version 1.
    pub fn light_v1(&self, endpoint_id: String) -> Result<LightV1, JsError> {
        let id = parse_id(&endpoint_id)?;
        let client = irpc_iroh::client(self.endpoint.clone(), id, v1::ALPN);
        Ok(LightV1 { client })
    }

    /// Step 4: the light switch with a button, protocol version 2.
    pub fn light_v2(&self, endpoint_id: String) -> Result<LightV2, JsError> {
        let id = parse_id(&endpoint_id)?;
        let client = irpc_iroh::client(self.endpoint.clone(), id, v2::ALPN);
        Ok(LightV2 { client })
    }

    /// Step 5: the lamp, protocol version 3. Takes an endpoint id, or the
    /// `pair…` ticket from provisioning.
    pub fn light_v3(&self, endpoint_id: String) -> Result<LightV3, JsError> {
        let id = parse_id(&endpoint_id)?;
        let client = irpc_iroh::client(self.endpoint.clone(), id, v3::ALPN);
        Ok(LightV3 { client })
    }
}

#[wasm_bindgen]
pub struct LightV1 {
    client: irpc::Client<v1::LightProtocol>,
}

#[wasm_bindgen]
impl LightV1 {
    /// Switch the light; resolves with the new state.
    pub async fn set(&self, on: bool) -> Result<bool, JsError> {
        self.client.rpc(v1::Set { on }).await.map_err(js_err)
    }

    pub async fn get(&self) -> Result<bool, JsError> {
        self.client.rpc(v1::Get).await.map_err(js_err)
    }
}

#[wasm_bindgen]
pub struct LightV2 {
    client: irpc::Client<v2::LightProtocol>,
}

#[wasm_bindgen]
impl LightV2 {
    /// Switch the light; resolves with the new state.
    pub async fn set(&self, on: bool) -> Result<bool, JsError> {
        self.client.rpc(v2::Set { on }).await.map_err(js_err)
    }

    pub async fn get(&self) -> Result<bool, JsError> {
        self.client.rpc(v2::Get).await.map_err(js_err)
    }

    /// Call `on_change(on)` with the current state, then with every change,
    /// e.g. when someone presses the button. Resolves when the device hangs up.
    pub async fn watch(&self, on_change: js_sys::Function) -> Result<(), JsError> {
        let mut changes = self
            .client
            .server_streaming(v2::Watch, 4)
            .await
            .map_err(js_err)?;
        while let Some(on) = changes.recv().await.map_err(js_err)? {
            let _ = on_change.call1(&JsValue::NULL, &on.into());
        }
        Ok(())
    }
}

#[wasm_bindgen]
pub struct LightV3 {
    client: irpc::Client<v3::LightProtocol>,
}

/// The lamp's state, as the device reports it.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct LampState {
    /// Is the light on?
    pub on: bool,
    /// Does the lamp follow the light sensor?
    pub auto: bool,
    /// Brightness around the lamp, 0 (dark) to 100.
    pub level: u8,
    /// In automatic mode, the light goes on below this level.
    pub threshold: u8,
}

impl From<v3::LightState> for LampState {
    fn from(s: v3::LightState) -> Self {
        LampState {
            on: s.on,
            auto: s.auto,
            level: s.level,
            threshold: s.threshold,
        }
    }
}

#[wasm_bindgen]
impl LightV3 {
    /// Switch the light by hand (leaves automatic mode).
    pub async fn set(&self, on: bool) -> Result<LampState, JsError> {
        Ok(self.client.rpc(v3::Set { on }).await.map_err(js_err)?.into())
    }

    pub async fn set_auto(&self, auto: bool) -> Result<LampState, JsError> {
        Ok(self.client.rpc(v3::SetAuto { auto }).await.map_err(js_err)?.into())
    }

    pub async fn set_threshold(&self, threshold: u8) -> Result<LampState, JsError> {
        let request = v3::SetThreshold { threshold };
        Ok(self.client.rpc(request).await.map_err(js_err)?.into())
    }

    pub async fn get(&self) -> Result<LampState, JsError> {
        Ok(self.client.rpc(v3::Get).await.map_err(js_err)?.into())
    }

    /// Call `on_change(state)` with the current state, then with every change.
    /// Resolves when the device hangs up.
    pub async fn watch(&self, on_change: js_sys::Function) -> Result<(), JsError> {
        let mut changes = self
            .client
            .server_streaming(v3::Watch, 4)
            .await
            .map_err(js_err)?;
        while let Some(state) = changes.recv().await.map_err(js_err)? {
            let state = LampState::from(state);
            let _ = on_change.call1(&JsValue::NULL, &state.into());
        }
        Ok(())
    }
}

/// Step 5: show the browser's Bluetooth picker and connect to a device waiting
/// for WiFi credentials. Call from a click handler.
#[wasm_bindgen]
pub async fn find_device() -> Result<Device, JsError> {
    let transport = WebTransport::request().await.map_err(js_err)?;
    let browser_device = transport.device().clone();
    let session = Session::connect(transport).await.map_err(js_err)?;
    Ok(Device {
        endpoint_id: session.endpoint().to_string(),
        kind: session.info().kind.clone(),
        browser_device,
        session: Rc::new(RefCell::new(Some(session))),
    })
}

/// A device waiting for WiFi credentials, connected over Bluetooth.
#[wasm_bindgen]
pub struct Device {
    endpoint_id: String,
    kind: String,
    browser_device: web_sys::BluetoothDevice,
    /// Taken out while `provision` runs.
    session: Rc<RefCell<Option<Session<WebTransport>>>>,
}

#[wasm_bindgen]
impl Device {
    /// Compare with the endpoint id the device prints.
    #[wasm_bindgen(getter)]
    pub fn endpoint_id(&self) -> String {
        self.endpoint_id.clone()
    }

    /// What it is, e.g. `lamp`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.clone()
    }

    /// Send the network and a name, sealed to the device's endpoint id; resolve
    /// with the `pair…` ticket once it joined, reject with the reason
    /// otherwise. `on_status(detail)` hears the steps in between. Can be called
    /// again after a failure, e.g. with the right password.
    pub async fn provision(
        &self,
        ssid: String,
        password: String,
        name: String,
        on_status: js_sys::Function,
    ) -> Result<String, JsError> {
        let networks = [Network { ssid, password }];
        let mut session = self
            .session
            .borrow_mut()
            .take()
            .ok_or_else(|| JsError::new("already provisioning"))?;
        let result = session
            .provision_with(&networks, &name, None, |status| {
                let detail = describe(status);
                let _ = on_status.call1(&JsValue::NULL, &detail.into());
            })
            .await;
        *self.session.borrow_mut() = Some(session);
        Ok(result.map_err(js_err)?.ticket.to_string())
    }

    pub fn disconnect(&self) {
        if let Some(gatt) = self.browser_device.gatt() {
            gatt.disconnect();
        }
    }
}

fn describe(status: &Status) -> String {
    match status {
        Status::Unprovisioned => "waiting for credentials".into(),
        Status::Connecting => "joining the network…".into(),
        Status::Connected { ip, .. } => {
            let [a, b, c, d] = ip;
            format!("joined, IP {a}.{b}.{c}.{d}")
        }
        Status::Failed(failure) => failure.describe().into(),
    }
}

/// An endpoint id, or a `pair…` ticket from provisioning (which holds one).
fn parse_id(s: &str) -> Result<EndpointId, JsError> {
    let s = s.trim();
    if s.starts_with("pair") {
        let ticket: PairingTicket = s.parse().map_err(js_err)?;
        return Ok(ticket.endpoint);
    }
    s.parse().map_err(js_err)
}

fn js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&format!("{err:#}"))
}
