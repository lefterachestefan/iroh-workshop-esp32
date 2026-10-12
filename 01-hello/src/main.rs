//! Step 1: hello iroh.
//!
//! Join WiFi, start an iroh endpoint, and print its endpoint id: the public key
//! anyone can dial it with. There's no stored key yet, so every start gets a
//! new random identity.

use log::info;

fn main() {
    let board = common::init();
    let _wifi = common::connect_wifi(board.modem, board.sys_loop, board.nvs);

    common::block_on(async {
        let endpoint = common::endpoint_builder()
            .bind()
            .await
            .expect("Failed to bind the endpoint");
        info!("Hello from iroh! My endpoint id is {}", endpoint.id());

        // Keep running, so the endpoint stays up.
        std::future::pending::<()>().await;
    });
}
