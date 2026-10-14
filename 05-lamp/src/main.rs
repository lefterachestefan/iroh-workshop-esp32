//! Step 5: the whole thing, a lamp you'd put on a shelf.
//!
//! - No `WIFI_CONFIG`: on first boot the device waits for WiFi credentials
//!   over Bluetooth, sealed to its endpoint id so only it can read them (see
//!   iroh-provision). Hold BOOT for 5 seconds to forget them and start over.
//! - A real lamp on a pin, and a push button: press to switch, hold for a
//!   second to switch automatic mode.
//! - A photoresistor: in automatic mode the lamp goes on when it gets dark.
//!
//! Version 3 of the light protocol reports all of that: on/off, the mode, how
//! bright it is, and the threshold.
//!
//! Wiring (XIAO ESP32-C5):
//!
//! | Pin | GPIO | Connect to |
//! |---|---|---|
//! | D1 | 0 | photoresistor (D1 is its 3.3 V supply) |
//! | D0 | 1 | photoresistor's other leg (ADC, internal pull-down) |
//! | D9 | 9 | push button to GND |
//! | D10 | 10 | LED + resistor to GND, or a MOSFET for a bigger lamp |

mod sensor;

use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::{AnyIOPin, AnyOutputPin, Output, OutputPin, PinDriver, Pull};
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use iroh::protocol::Router;
use iroh_provision::{device, DeviceInfo};
use iroh_provision_esp_idf::{
    fill_random, join_or_provision, keep_connected, watch_reset_button, BlinkingLed, NvsStorage,
    StationWifi,
};
use irpc::channel::mpsc;
use irpc_iroh::IrohProtocol;
use light_protocol::v3::{LightMessage, LightProtocol, LightState, Settings, ALPN};
use log::{info, warn};
use sensor::LightSensor;

/// What this device is: the start of its Bluetooth name (`lamp-1a2b3c4d5e`),
/// and the default name the provisioning page gives it (`lamp-1`, ...).
const KIND: &str = "lamp";

/// Everything (button, sensor, automatic mode) runs on a 50 ms tick.
const TICK: Duration = Duration::from_millis(50);
/// Hold the button this long to switch automatic mode, instead of the light.
const LONG_PRESS_TICKS: u32 = 20;
/// In automatic mode, the lamp goes on below `threshold - MARGIN` and off above
/// `threshold + MARGIN`, so a reading right at the threshold doesn't flicker.
const MARGIN: f32 = 3.0;
/// ... and only once it's been that dark (or bright) for `delay_ms`, a setting.
/// The default, with the sensor's smoothing, reacts in about half a second, and
/// a hand passing by doesn't switch it.
const DEFAULT_DELAY_MS: u16 = 300;
/// The longest delay a client can set.
const MAX_DELAY_MS: u16 = 10_000;
/// Watchers hear about the brightness when it changed by this much.
const REPORT_STEP: u8 = 2;

#[esp32::main]
async fn main() {
    let board = esp32::init();

    let secret_key = provision_wifi(
        board.modem,
        board.sys_loop,
        board.nvs.clone(),
        board.pins.gpio28.into(),
        board.pins.gpio27.into(),
    );
    let _sntp = esp32::sync_clock();

    let mut lamp = Lamp::new(board.pins.gpio10, board.nvs);
    let button = PinDriver::input(board.pins.gpio9, Pull::Up).expect("Failed to set up the button");
    // D1, right next to D0, powers the photoresistor, so it needs only short
    // wires. It draws well under a milliamp.
    let mut sensor_supply =
        PinDriver::output(board.pins.gpio0).expect("Failed to set up the sensor supply");
    sensor_supply.set_high().expect("Failed to switch the sensor supply on");
    let mut sensor =
        LightSensor::new(board.adc1, board.pins.gpio1).expect("Failed to set up the sensor");

    let endpoint = iroh::Endpoint::builder(esp32::N0)
        .secret_key(secret_key)
        .bind()
        .await
        .expect("Failed to bind the endpoint");
    info!("Lamp ready, endpoint id {}", endpoint.id());

    let (sender, mut requests) = tokio::sync::mpsc::channel(4);
    let _router = Router::builder(endpoint)
        .accept(ALPN, IrohProtocol::<LightProtocol>::with_sender(sender))
        .spawn();

    let mut watchers: Vec<mpsc::Sender<LightState>> = Vec::new();
    let mut tick = tokio::time::interval(TICK);
    // How many ticks the button has been held, 0 when it's up.
    let mut held = 0u32;
    // How many ticks automatic mode has wanted to switch the lamp.
    let mut unsettled = 0u32;

    loop {
        let before = lamp.state;
        tokio::select! {
            Some(request) = requests.recv() => match request {
                LightMessage::Set(request) => {
                    lamp.leave_auto();
                    lamp.switch(request.on);
                    request.tx.send(lamp.state).await.ok();
                }
                LightMessage::Configure(request) => {
                    lamp.configure(*request);
                    request.tx.send(lamp.state).await.ok();
                }
                LightMessage::Get(request) => {
                    request.tx.send(lamp.state).await.ok();
                }
                LightMessage::Watch(request) => {
                    // The current state first, then every change.
                    if request.tx.send(lamp.state).await.is_ok() {
                        watchers.push(request.tx);
                    }
                }
            },
            _ = tick.tick() => {
                // The button: a short press switches the light by hand, holding
                // it switches automatic mode (once, while still held).
                if button.is_low() {
                    held += 1;
                    if held == LONG_PRESS_TICKS {
                        info!("Button held");
                        let settings = lamp.state.settings;
                        lamp.configure(Settings {
                            auto: !settings.auto,
                            ..settings
                        });
                    }
                } else {
                    if held > 0 && held < LONG_PRESS_TICKS {
                        info!("Button pressed");
                        lamp.leave_auto();
                        lamp.switch(!lamp.state.on);
                    }
                    held = 0;
                }

                // The sensor, and automatic mode.
                match sensor.read() {
                    Ok(level) => {
                        lamp.report_level(level);
                        let settings = lamp.state.settings;
                        let threshold = f32::from(settings.threshold);
                        let wants_switch = settings.auto
                            && if lamp.state.on {
                                level > threshold + MARGIN
                            } else {
                                level < threshold - MARGIN
                            };
                        unsettled = if wants_switch { unsettled + 1 } else { 0 };
                        if unsettled >= settle_ticks(settings.delay_ms) {
                            unsettled = 0;
                            lamp.switch(!lamp.state.on);
                        }
                    }
                    Err(e) => warn!("Failed to read the sensor: {e}"),
                }
            }
        }

        if lamp.state != before {
            // Tell every watcher. `try_send` doesn't wait: a watcher that can't
            // take the update right now has hung up or stopped reading, and
            // waiting for it would stall everything else. Forget it; dropping
            // its sender ends its stream, so it notices and can watch again.
            let mut still_watching = Vec::new();
            for watcher in watchers.drain(..) {
                if let Ok(true) = watcher.try_send(lamp.state).await {
                    still_watching.push(watcher);
                }
            }
            watchers = still_watching;
        }
    }
}

/// Join WiFi, and return the device's identity (made on first boot, kept in
/// NVS). With stored networks, join the strongest one in range; without, wait
/// for credentials over Bluetooth first. From then on, rejoin whichever stored
/// network is around when WiFi drops.
///
/// Holding `boot_button` for 5 seconds forgets the networks. `status_led`
/// blinks slowly while waiting for credentials, fast while joining, and is off
/// once joined.
fn provision_wifi(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    boot_button: AnyIOPin<'static>,
    status_led: AnyOutputPin<'static>,
) -> iroh::SecretKey {
    watch_reset_button(boot_button, nvs.clone());
    let mut wifi = StationWifi::start(modem, sys_loop, nvs.clone()).expect("Failed to start WiFi");
    let mut storage = NvsStorage::new(nvs).expect("Failed to open NVS");
    // After starting WiFi: the RNG is only truly random with the radio on.
    let secret_key = device::load_or_create_identity(&mut storage, fill_random)
        .expect("Failed to load the identity");
    // Compare this with what the provisioning page shows.
    info!("Endpoint id {}", secret_key.public());
    let mut status_led = BlinkingLed::new(status_led, true).expect("Failed to set up the LED");
    let info = DeviceInfo { kind: KIND.into() };
    let joined = join_or_provision(&mut storage, &mut wifi, &secret_key, &info, &mut status_led)
        .expect("Failed to provision");
    info!("Joined {:?}, IP {}", joined.ssid, joined.ip);
    keep_connected(wifi, &mut storage, status_led);
    secret_key
}

/// The light, and its settings, which survive a restart.
struct Lamp {
    state: LightState,
    pin: PinDriver<'static, Output>,
    settings: EspNvs<NvsDefault>,
}

impl Lamp {
    fn new(pin: impl OutputPin + 'static, nvs: EspDefaultNvsPartition) -> Self {
        let mut pin = PinDriver::output(pin).expect("Failed to set up the lamp");
        pin.set_low().expect("Failed to switch the lamp off");
        let settings = EspNvs::new(nvs, "lamp", true).expect("Failed to open NVS");
        let auto = settings.get_u8("auto").ok().flatten().unwrap_or(0) != 0;
        let threshold = settings.get_u8("threshold").ok().flatten().unwrap_or(30);
        let delay_ms = settings
            .get_u16("delay_ms")
            .ok()
            .flatten()
            .unwrap_or(DEFAULT_DELAY_MS);
        info!(
            "Automatic mode {}, threshold {threshold}%, delay {delay_ms} ms",
            on_off(auto)
        );
        Self {
            state: LightState {
                on: false,
                level: 0,
                settings: Settings {
                    auto,
                    threshold,
                    delay_ms,
                },
            },
            pin,
            settings,
        }
    }

    fn switch(&mut self, on: bool) {
        if on == self.state.on {
            return;
        }
        self.state.on = on;
        info!("Light is {}", on_off(on));
        // Active high: the pin drives the LED, or the MOSFET's gate.
        if on {
            self.pin.set_high().ok();
        } else {
            self.pin.set_low().ok();
        }
    }

    /// Take new settings, and keep the ones that changed.
    fn configure(&mut self, new: Settings) {
        let new = Settings {
            threshold: new.threshold.min(100),
            delay_ms: new.delay_ms.min(MAX_DELAY_MS),
            ..new
        };
        let old = self.state.settings;
        self.state.settings = new;
        if new.auto != old.auto {
            info!("Automatic mode {}", on_off(new.auto));
            self.save_u8("auto", new.auto as u8);
            if new.auto {
                // Follow the sensor right away, rather than after the delay.
                self.switch(self.state.level < new.threshold);
            }
        }
        if new.threshold != old.threshold {
            info!("Threshold {}%", new.threshold);
            self.save_u8("threshold", new.threshold);
        }
        if new.delay_ms != old.delay_ms {
            info!("Delay {} ms", new.delay_ms);
            if let Err(e) = self.settings.set_u16("delay_ms", new.delay_ms) {
                warn!("Failed to save delay_ms: {e}");
            }
        }
    }

    /// Switching by hand leaves automatic mode.
    fn leave_auto(&mut self) {
        let settings = self.state.settings;
        self.configure(Settings {
            auto: false,
            ..settings
        });
    }

    /// Update the reported brightness, if it moved enough to be worth telling
    /// watchers about.
    fn report_level(&mut self, level: f32) {
        let level = level.round() as u8;
        if level.abs_diff(self.state.level) >= REPORT_STEP {
            self.state.level = level;
        }
    }

    fn save_u8(&mut self, key: &str, value: u8) {
        if let Err(e) = self.settings.set_u8(key, value) {
            warn!("Failed to save {key}: {e}");
        }
    }
}

/// How many sensor ticks make `delay_ms`, at least one.
fn settle_ticks(delay_ms: u16) -> u32 {
    (u32::from(delay_ms)).div_ceil(TICK.as_millis() as u32).max(1)
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}
