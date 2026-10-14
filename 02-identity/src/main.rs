//! Step 2: a stable identity.
//!
//! Step 1's endpoint, but the secret key is generated once and kept in NVS, so
//! the endpoint id survives restarts and reflashing.

use iroh::protocol::Router;
use log::info;

#[esp32::main]
async fn main() {
    let board = esp32::init();
    let _wifi = esp32::connect_wifi(board.modem, board.sys_loop, board.nvs.clone());
    // New in this step: the same key on every start.
    let secret_key = esp32::load_or_create_secret_key(board.nvs);

    let endpoint = iroh::Endpoint::builder(esp32::N0)
        .secret_key(secret_key)
        .bind()
        .await
        .expect("Failed to bind the endpoint");
    info!("Hello from iroh! My endpoint id is {}", endpoint.id());

    // Still no protocols: the router turns everyone away.
    let _router = Router::builder(endpoint).spawn();

    // Keep running, so the endpoint stays up.
    std::future::pending::<()>().await;
}
