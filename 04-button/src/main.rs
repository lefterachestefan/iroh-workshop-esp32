//! Step 4: a button.
//!
//! The BOOT button toggles the light too, so the state can now change without
//! anyone asking. Version 2 of the light protocol adds `Watch`: the device sends
//! the current state, then every change, until the watcher hangs up.

use std::time::Duration;

use esp_idf_svc::hal::gpio::{PinDriver, Pull};
use iroh::protocol::Router;
use irpc::channel::mpsc;
use irpc_iroh::IrohProtocol;
use light_protocol::v2::{LightMessage, LightProtocol, ALPN};
use log::info;

#[esp32::main]
async fn main() {
    let board = esp32::init();
    let _wifi = esp32::connect_wifi(board.modem, board.sys_loop, board.nvs.clone());
    let secret_key = esp32::load_or_create_secret_key(board.nvs);

    // The XIAO ESP32-C5's user LED (GPIO27) and BOOT button (GPIO28) are both
    // active low: the LED lights when the pin is low, the button reads low
    // while pressed.
    let mut led = PinDriver::output(board.pins.gpio27).expect("Failed to set up the LED");
    led.set_high().expect("Failed to switch the LED off");
    let button =
        PinDriver::input(board.pins.gpio28, Pull::Up).expect("Failed to set up the button");

    let endpoint = iroh::Endpoint::builder(esp32::N0)
        .secret_key(secret_key)
        .bind()
        .await
        .expect("Failed to bind the endpoint");
    info!("Light switch ready, endpoint id {}", endpoint.id());

    let (sender, mut requests) = tokio::sync::mpsc::channel(4);
    let _router = Router::builder(endpoint)
        .accept(ALPN, IrohProtocol::<LightProtocol>::with_sender(sender))
        .spawn();

    let mut on = false;
    let mut watchers: Vec<mpsc::Sender<bool>> = Vec::new();
    // Checking the button every 50 ms also debounces it.
    let mut check_button = tokio::time::interval(Duration::from_millis(50));
    let mut was_pressed = false;

    loop {
        // Wait for whichever comes first: a request, or the next button check.
        let changed = tokio::select! {
            Some(request) = requests.recv() => match request {
                LightMessage::Set(request) => {
                    on = request.on;
                    request.tx.send(on).await.ok();
                    true
                }
                LightMessage::Get(request) => {
                    request.tx.send(on).await.ok();
                    false
                }
                LightMessage::Watch(request) => {
                    // The current state first, then every change.
                    if request.tx.send(on).await.is_ok() {
                        watchers.push(request.tx);
                    }
                    false
                }
            },
            _ = check_button.tick() => {
                let pressed = button.is_low();
                let just_pressed = pressed && !was_pressed;
                was_pressed = pressed;
                if just_pressed {
                    on = !on;
                    info!("Button pressed");
                }
                just_pressed
            }
        };

        if changed {
            info!("Light is {}", if on { "on" } else { "off" });
            if on {
                led.set_low().ok();
            } else {
                led.set_high().ok();
            }
            // Tell every watcher. `try_send` doesn't wait: a watcher that can't
            // take the update right now has hung up or stopped reading, and
            // waiting for it would stall everything else. Forget it; dropping
            // its sender ends its stream, so it notices and can watch again.
            let mut still_watching = Vec::new();
            for watcher in watchers.drain(..) {
                if let Ok(true) = watcher.try_send(on).await {
                    still_watching.push(watcher);
                }
            }
            watchers = still_watching;
        }
    }
}
