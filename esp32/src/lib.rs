//! The boilerplate every workshop step needs, so the steps themselves stay
//! short: bring up the board, join WiFi, and build an iroh endpoint that fits
//! on an ESP32.
//!
//! ```ignore
//! #[esp32::main]
//! async fn main() {
//!     let board = esp32::init();
//!     let _wifi = esp32::connect_wifi(board.modem, board.sys_loop, board.nvs);
//!     let endpoint = iroh::Endpoint::bind(esp32::N0).await.unwrap();
//! }
//! ```

mod quic_crypto_provider;
#[cfg(feature = "wifi-config")]
mod wifi;

#[cfg(feature = "wifi-config")]
pub use wifi::{connect_wifi, Wifi};

use std::sync::Arc;
use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::adc::ADC1;
use esp_idf_svc::hal::gpio::Pins;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::sntp::EspSntp;
use iroh::address_lookup::{DnsAddressLookup, PkarrPublisher};
use iroh::endpoint::{presets::Preset, Builder, QuicTransportConfig, VarInt};
use log::{info, warn};

/// The parts of the board the steps use.
pub struct Board {
    pub modem: Modem<'static>,
    pub adc1: ADC1<'static>,
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
    let logger = esp_idf_svc::log::init_from_esp_idf();
    // With no tracing subscriber, the tracing->log bridge logs every span
    // creation at INFO. Quiet the noisy targets, but keep their errors.
    for target in [
        "iroh::socket::transports",               // poll_send (per packet)
        "iroh::net_report",                       // QADv4 relay probes
        "tracing::span",                          // span lifecycle (reportgen-actor, tx, …)
        "iroh::socket::remote_map::remote_state", // per-connection path events
    ] {
        let _ = logger.filter().set_target_level(target, log::LevelFilter::Error);
    }

    // Running out of PSRAM reboots; see `on_alloc_failed`.
    unsafe { esp_idf_svc::sys::heap_caps_register_failed_alloc_callback(Some(on_alloc_failed)) };

    info!("Started after {}", reset_reason());

    let peripherals = Peripherals::take().expect("Failed to take peripherals");
    Board {
        modem: peripherals.modem,
        adc1: peripherals.adc1,
        pins: peripherals.pins,
        sys_loop: EspSystemEventLoop::take().expect("Failed to take event loop"),
        nvs: EspDefaultNvsPartition::take().expect("Failed to take NVS partition"),
    }
}

/// Why the chip (re)started. After a crash whose message didn't make it to the
/// serial console, this is the only clue.
fn reset_reason() -> &'static str {
    use esp_idf_svc::sys::*;
    #[allow(non_upper_case_globals)]
    match unsafe { esp_reset_reason() } {
        esp_reset_reason_t_ESP_RST_POWERON => "power-on",
        esp_reset_reason_t_ESP_RST_EXT => "external reset",
        esp_reset_reason_t_ESP_RST_SW => "software reset",
        esp_reset_reason_t_ESP_RST_PANIC => "a panic",
        esp_reset_reason_t_ESP_RST_INT_WDT => "the interrupt watchdog",
        esp_reset_reason_t_ESP_RST_TASK_WDT => "the task watchdog",
        esp_reset_reason_t_ESP_RST_WDT => "a watchdog",
        esp_reset_reason_t_ESP_RST_DEEPSLEEP => "deep sleep",
        esp_reset_reason_t_ESP_RST_BROWNOUT => "a brownout",
        esp_reset_reason_t_ESP_RST_SDIO => "an SDIO reset",
        esp_reset_reason_t_ESP_RST_USB => "a USB reset",
        esp_reset_reason_t_ESP_RST_JTAG => "a JTAG reset",
        esp_reset_reason_t_ESP_RST_EFUSE => "an eFuse error",
        esp_reset_reason_t_ESP_RST_PWR_GLITCH => "a power glitch",
        esp_reset_reason_t_ESP_RST_CPU_LOCKUP => "a CPU lockup",
        _ => "an unknown reset",
    }
}

/// Called by ESP-IDF when an allocation has failed, after all fallbacks.
///
/// Plain `malloc`, which Rust and most C code use, takes PSRAM and falls back
/// to internal RAM, so when it fails, memory is gone for good: reboot, and
/// come back with a clean heap. Requests only internal RAM can serve (WiFi
/// buffers, DMA, FreeRTOS objects) fail on bursts and recover; the code that
/// makes them handles the error, so leave them be.
///
/// In IRAM: it may run while the flash cache is off. It mustn't allocate.
#[link_section = ".iram1.on_alloc_failed"]
unsafe extern "C" fn on_alloc_failed(size: usize, caps: u32, function: *const core::ffi::c_char) {
    use esp_idf_svc::sys::*;
    const INTERNAL_ONLY: u32 = MALLOC_CAP_INTERNAL
        | MALLOC_CAP_DMA
        | MALLOC_CAP_EXEC
        | MALLOC_CAP_IRAM_8BIT
        | MALLOC_CAP_RTCRAM
        | MALLOC_CAP_RETENTION
        | MALLOC_CAP_TCM
        | MALLOC_CAP_DMA_DESC_AHB
        | MALLOC_CAP_DMA_DESC_AXI;
    if caps & INTERNAL_ONLY == 0 {
        esp_rom_printf(
            c"Out of memory: %s(%u bytes, caps 0x%x)\n".as_ptr(),
            function,
            size as u32,
            caps,
        );
        esp_system_abort(c"Out of memory".as_ptr());
    }
}

/// Set the clock via SNTP, once on a network. Keep the result alive: it keeps
/// the clock in sync.
///
/// The clock starts at 1970; relay and discovery need the real time, and TLS
/// certificate checks fail without it, so this waits until SNTP has set it.
pub fn sync_clock() -> EspSntp<'static> {
    // We check the clock itself: ESP-IDF reports `SyncStatus::Completed` only
    // once, then goes back to `Reset`.
    let sntp = EspSntp::new_default().expect("Failed to start SNTP");
    let mut waited = 0;
    while !clock_is_set() {
        if waited > 0 && waited % 10 == 0 {
            warn!("Still waiting for SNTP to set the clock ({waited}s)");
        }
        std::thread::sleep(Duration::from_secs(1));
        waited += 1;
    }
    info!("Clock set via SNTP");
    sntp
}

/// Whether the clock has been set: anything before 2025 is the 1970 boot time
/// plus uptime.
fn clock_is_set() -> bool {
    const JAN_1_2025: u64 = 1_735_689_600;
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .is_ok_and(|t| t.as_secs() > JAN_1_2025)
}

/// The endpoint's secret key, generated on first boot and kept in NVS, so the
/// endpoint id stays the same across restarts. Call after [`connect_wifi`]: the
/// hardware RNG is only truly random with the radio on.
pub fn load_or_create_secret_key(nvs: EspDefaultNvsPartition) -> iroh::SecretKey {
    let nvs = EspNvs::new(nvs, "workshop", true).expect("Failed to open NVS");
    let mut bytes = [0u8; 32];
    match nvs.get_blob("secret.key", &mut bytes) {
        Ok(Some(stored)) if stored.len() == 32 => return iroh::SecretKey::from_bytes(&bytes),
        Ok(_) => info!("No secret key yet, generating one"),
        // Don't paper over this with a new key: that would change the endpoint id.
        Err(e) => panic!("Failed to read the secret key: {e}"),
    }
    let key = iroh::SecretKey::generate();
    nvs.set_blob("secret.key", &key.to_bytes())
        .expect("Failed to store the secret key");
    key
}

/// An endpoint preset for the ESP32, used like iroh's own presets:
/// `Endpoint::builder(esp32::N0)`.
///
/// It does what `presets::N0` does (n0's relays, publish via pkarr over HTTPS,
/// look up via DNS), which iroh only compiles with ring or aws-lc. On top: a
/// pure-Rust crypto provider, none of the network checks that cost a TLS
/// handshake, and QUIC buffers sized for a microcontroller. DNS and certificate
/// roots are iroh's defaults.
///
/// Without `.secret_key(..)`, every start gets a new random identity.
pub struct N0;

impl Preset for N0 {
    fn apply(self, builder: Builder) -> Builder {
        builder
            .crypto_provider(Arc::new(quic_crypto_provider::provider()))
            .relay_mode(iroh::RelayMode::Default)
            .address_lookup(PkarrPublisher::n0_dns())
            .address_lookup(DnsAddressLookup::n0_dns())
            // UDP probes are enough to measure relay latency. (Port mapping is
            // already out: it's an optional iroh feature we don't enable.)
            .net_report_config({
                let mut c = iroh::NetReportConfig::default();
                c.https_probes = false;
                c.captive_portal_check = false;
                c
            })
            .transport_config(transport_config())
    }
}

/// QUIC flow control sized for small RPC messages, not bulk transfer.
///
/// The defaults are built for internet throughput: 1.25 MB per stream, no
/// limit per connection, 10 MB unacknowledged on the way out, 100 streams of
/// each kind. Those are limits, not allocations, but a peer that sends and
/// doesn't read could make us buffer that much for every connection, and 8 MB
/// of PSRAM is all we have. With these, a connection buffers at most
/// `receive_window + send_window`, 128 KB, so 4 MB of PSRAM covers about 30
/// clients. Our messages are a few bytes; the windows only limit throughput,
/// to about one window per round trip and stream.
fn transport_config() -> QuicTransportConfig {
    QuicTransportConfig::builder()
        // irpc opens a bidirectional stream per request; a `Watch` keeps its
        // stream open. 16 at once is plenty for one client.
        .max_concurrent_bidi_streams(VarInt::from_u32(16))
        .max_concurrent_uni_streams(VarInt::from_u32(0))
        .stream_receive_window(VarInt::from_u32(16 * 1024))
        .receive_window(VarInt::from_u32(64 * 1024))
        .send_window(64 * 1024)
        // No QUIC datagrams.
        .datagram_receive_buffer_size(None)
        .build()
}

/// Like `#[tokio::main]`, but on [`block_on`]'s runtime: `async fn main`
/// becomes a plain `fn main` that runs its body with [`block_on`].
///
/// The setup before the first `.await` (WiFi, SNTP) blocks, which is fine:
/// nothing else runs on the runtime yet.
pub use esp32_macros::main;

/// Run `future` to completion on a single-threaded tokio runtime, sized for the
/// ESP32. `#[tokio::main]` can't set the blocking pool's size or stack, hence
/// [`main`].
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    // eventfd, which tokio's I/O driver (mio) needs. Here rather than in
    // `init`: `#[esp32::main]` builds the runtime before `main` calls `init`.
    static EVENTFD: std::sync::Once = std::sync::Once::new();
    EVENTFD.call_once(|| {
        let config = esp_idf_svc::sys::esp_vfs_eventfd_config_t { max_fds: 5 };
        esp_idf_svc::sys::esp!(unsafe { esp_idf_svc::sys::esp_vfs_eventfd_register(&config) })
            .expect("Failed to register eventfd");
    });
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .thread_stack_size(4096)
        .build()
        .expect("Failed to create tokio runtime")
        .block_on(future)
}
