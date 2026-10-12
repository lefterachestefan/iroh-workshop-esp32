//! Step 3: a stable identity.
//!
//! Step 2's light switch, but the secret key is generated once and kept in NVS,
//! so the endpoint id survives restarts and reflashing. Same protocol as step 2,
//! so the same page works.

use esp_idf_svc::hal::gpio::PinDriver;
use iroh::protocol::Router;
use irpc_iroh::IrohProtocol;
use light_protocol::v1::{LightMessage, LightProtocol, ALPN};
use log::info;

fn main() {
    let board = common::init();
    let _wifi = common::connect_wifi(board.modem, board.sys_loop, board.nvs.clone());
    // New in this step: the same key on every start.
    let secret_key = common::load_or_create_secret_key(board.nvs);

    // The XIAO ESP32-C5's user LED is on GPIO27. It's wired active low: it
    // lights when the pin is low.
    let mut led = PinDriver::output(board.pins.gpio27).expect("Failed to set up the LED");
    led.set_high().expect("Failed to switch the LED off");

    common::block_on(async {
        let endpoint = common::endpoint_builder()
            .secret_key(secret_key)
            .bind()
            .await
            .expect("Failed to bind the endpoint");
        info!("Light switch ready, endpoint id {}", endpoint.id());

        // Incoming requests for ALPN end up in `requests`.
        let (sender, mut requests) = tokio::sync::mpsc::channel(4);
        let _router = Router::builder(endpoint)
            .accept(ALPN, IrohProtocol::<LightProtocol>::with_sender(sender))
            .spawn();

        let mut on = false;
        while let Some(request) = requests.recv().await {
            match request {
                LightMessage::Set(request) => {
                    on = request.on;
                    info!("Switching the light {}", if on { "on" } else { "off" });
                    if on {
                        led.set_low().ok();
                    } else {
                        led.set_high().ok();
                    }
                    request.tx.send(on).await.ok();
                }
                LightMessage::Get(request) => {
                    request.tx.send(on).await.ok();
                }
            }
        }
    });
}
