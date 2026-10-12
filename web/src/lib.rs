//! iroh in the browser, for the workshop pages. One small API per step; the
//! protocols come from `../protocol`, the same crate the devices use.
//!
//! Browsers can't send UDP, so this endpoint reaches the devices over a relay.

use iroh::{endpoint::presets, EndpointId};
use light_protocol::{v1, v2};
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

    /// Step 1: is the device there? It has no protocols yet, so it turns the
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

    /// Steps 2 and 3: the light switch, protocol version 1.
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

fn parse_id(s: &str) -> Result<EndpointId, JsError> {
    s.trim().parse().map_err(js_err)
}

fn js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&format!("{err:#}"))
}
