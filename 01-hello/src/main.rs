//! Step 1: hello iroh.
//!
//! Join WiFi, start an iroh endpoint, and print its endpoint id: the public key
//! anyone can dial it with. There's no stored key yet, so every start gets a
//! new random identity.

use iroh::protocol::Router;
use log::info;

#[esp32::main]
async fn main() {
    let board = esp32::init();
    let _wifi = esp32::connect_wifi(board.modem, board.sys_loop, board.nvs);

    let endpoint = iroh::Endpoint::bind(esp32::N0)
        .await
        .expect("Failed to bind the endpoint");
    info!("Hello from iroh! My endpoint id is {}", endpoint.id());

    // A router answers incoming connections. It has no protocols yet, so it
    // turns everyone away, but that already shows the device is reachable.
    let _router = Router::builder(endpoint).spawn();

    // Keep running, so the endpoint stays up.
    std::future::pending::<()>().await;
}
