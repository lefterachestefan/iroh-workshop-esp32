//! Joining the network baked in at build time, for steps 1 to 4. (Step 5 gets
//! its networks over Bluetooth instead, so it builds without this.)

use std::net::Ipv4Addr;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::info;

use crate::sync_clock;

/// `SSID:PASSWORD`, set when building: `WIFI_CONFIG='SSID:PASSWORD' cargo run --release`.
/// For an open network, just `SSID`.
const WIFI_CONFIG: &str = match option_env!("WIFI_CONFIG") {
    Some(value) => value,
    None => {
        panic!("WIFI_CONFIG is not set. Build with WIFI_CONFIG='SSID:PASSWORD' cargo run --release")
    }
};

/// A joined WiFi network. Keep it alive: dropping it disconnects.
pub struct Wifi {
    _wifi: BlockingWifi<EspWifi<'static>>,
    _sntp: EspSntp<'static>,
    pub ip: Ipv4Addr,
}

/// Join the network from `WIFI_CONFIG` and set the clock via SNTP.
pub fn connect_wifi(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Wifi {
    let (ssid, password) = WIFI_CONFIG.split_once(':').unwrap_or((WIFI_CONFIG, ""));
    info!("Connecting to WiFi network {ssid:?}");
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs)).expect("Failed to create WiFi driver"),
        sys_loop,
    )
    .expect("Failed to create WiFi driver");
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().expect("SSID too long"),
        password: password.try_into().expect("Password too long"),
        // The default, WPA2, won't join an open network.
        auth_method: if password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    }))
    .expect("Failed to configure WiFi");
    wifi.start().expect("Failed to start WiFi");
    wifi.connect().expect("Failed to connect to WiFi");
    wifi.wait_netif_up().expect("Failed to get an IP address");
    let ip = wifi
        .wifi()
        .sta_netif()
        .get_ip_info()
        .expect("No IP info")
        .ip;
    info!("WiFi connected, IP {ip}");

    Wifi {
        _wifi: wifi,
        _sntp: sync_clock(),
        ip: Ipv4Addr::from(ip.octets()),
    }
}
