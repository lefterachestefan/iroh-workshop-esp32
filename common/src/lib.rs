//! The boilerplate every workshop step needs, so the steps themselves stay
//! short: bring up the board, join WiFi, and build an iroh endpoint that fits
//! on an ESP32.
//!
//! ```ignore
//! let board = common::init();
//! let _wifi = common::connect_wifi(board.modem, board.sys_loop, board.nvs);
//! common::block_on(async {
//!     let endpoint = common::endpoint_builder().bind().await.unwrap();
//! });
//! ```

mod insecure_verifier;
mod quic_crypto_provider;
mod std_dns_resolver;

use std::net::Ipv4Addr;
use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::Pins;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use iroh::tls::CaTlsConfig;
use log::{info, warn};

/// `SSID:PASSWORD`, set when building: `WIFI_CONFIG='SSID:PASSWORD' cargo run --release`.
const WIFI_CONFIG: &str = match option_env!("WIFI_CONFIG") {
    Some(value) => value,
    None => {
        panic!("WIFI_CONFIG is not set. Build with WIFI_CONFIG='SSID:PASSWORD' cargo run --release")
    }
};

/// The parts of the board the steps use.
pub struct Board {
    pub modem: Modem<'static>,
    pub pins: Pins,
    pub sys_loop: EspSystemEventLoop,
    pub nvs: EspDefaultNvsPartition,
}

/// Set up logging and the runtime patches, and take the board's peripherals.
/// Call once, first thing in `main`.
pub fn init() -> Board {
    // Some patches to the runtime implemented by esp-idf-sys don't link without
    // this. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // eventfd, which tokio's I/O driver (mio) needs.
    let eventfd_config = esp_idf_svc::sys::esp_vfs_eventfd_config_t { max_fds: 5 };
    unsafe { esp_idf_svc::sys::esp_vfs_eventfd_register(&eventfd_config) };

    let peripherals = Peripherals::take().expect("Failed to take peripherals");
    Board {
        modem: peripherals.modem,
        pins: peripherals.pins,
        sys_loop: EspSystemEventLoop::take().expect("Failed to take event loop"),
        nvs: EspDefaultNvsPartition::take().expect("Failed to take NVS partition"),
    }
}

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
    let (ssid, password) = WIFI_CONFIG
        .split_once(':')
        .expect("WIFI_CONFIG must be SSID:PASSWORD");
    info!("Connecting to WiFi network {ssid:?}");
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs)).expect("Failed to create WiFi driver"),
        sys_loop,
    )
    .expect("Failed to create WiFi driver");
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().expect("SSID too long"),
        password: password.try_into().expect("Password too long"),
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

    // The clock starts at 1970; relay and discovery need the real time.
    let sntp = EspSntp::new_default().expect("Failed to start SNTP");
    for _ in 0..30 {
        if sntp.get_sync_status() == SyncStatus::Completed {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if sntp.get_sync_status() != SyncStatus::Completed {
        warn!("SNTP sync timed out, continuing anyway");
    }

    Wifi {
        _wifi: wifi,
        _sntp: sntp,
        ip: Ipv4Addr::from(ip.octets()),
    }
}

/// The endpoint's secret key, generated on first boot and kept in NVS, so the
/// endpoint id stays the same across restarts. Call after [`connect_wifi`]: the
/// hardware RNG is only truly random with the radio on.
pub fn load_or_create_secret_key(nvs: EspDefaultNvsPartition) -> iroh::SecretKey {
    let nvs = EspNvs::new(nvs, "workshop", true).expect("Failed to open NVS");
    let mut bytes = [0u8; 32];
    match nvs.get_blob("secret", &mut bytes) {
        Ok(Some(stored)) if stored.len() == 32 => return iroh::SecretKey::from_bytes(&bytes),
        Ok(_) => info!("No secret key yet, generating one"),
        // Don't paper over this with a new key: that would change the endpoint id.
        Err(e) => panic!("Failed to read the secret key: {e}"),
    }
    let key = iroh::SecretKey::generate();
    nvs.set_blob("secret", &key.to_bytes())
        .expect("Failed to store the secret key");
    key
}

/// An endpoint builder set up for the ESP32: pure-Rust crypto, small QUIC
/// buffers, n0's relays for NAT traversal, and pkarr discovery so peers can
/// dial it by endpoint id alone. Without `.secret_key(..)`, every start gets a
/// new random identity.
pub fn endpoint_builder() -> iroh::endpoint::Builder {
    use iroh::endpoint::{QuicTransportConfig, VarInt};

    // QUIC's defaults size flow control for MB-scale throughput; a few KiB is
    // plenty here and keeps memory small.
    let transport_config = QuicTransportConfig::builder()
        .max_concurrent_bidi_streams(VarInt::from_u32(4))
        .max_concurrent_uni_streams(VarInt::from_u32(0))
        .stream_receive_window(VarInt::from_u32(4 * 1024))
        .receive_window(VarInt::from_u32(8 * 1024))
        .send_window(8 * 1024)
        .datagram_receive_buffer_size(None)
        .build();

    iroh::Endpoint::builder(iroh::endpoint::presets::Empty)
        .crypto_provider(std::sync::Arc::new(quic_crypto_provider::provider()))
        // The minimal crypto provider can't verify the relay's RSA certificate.
        // Peers are still authenticated by their ed25519 keys at the QUIC layer.
        .ca_tls_config(CaTlsConfig::custom_server_cert_verifier(
            insecure_verifier::skip_verify(),
        ))
        .dns_resolver(iroh::dns::DnsResolver::custom(
            std_dns_resolver::StdDnsResolver,
        ))
        .transport_config(transport_config)
        .max_tls_tickets(0)
        .relay_mode(iroh::RelayMode::Default)
        .address_lookup(iroh::address_lookup::PkarrPublisher::n0_dns())
        .address_lookup(iroh::address_lookup::PkarrResolver::n0_dns())
        // No HTTPS probes or captive portal checks: they need real certificate
        // verification. UDP probes still measure relay latency.
        .net_report_config({
            let mut c = iroh::NetReportConfig::default();
            c.https_probes = false;
            c.captive_portal_check = false;
            c
        })
}

/// Run `future` to completion on a single-threaded tokio runtime.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .thread_stack_size(4096)
        .build()
        .expect("Failed to create tokio runtime")
        .block_on(future)
}
