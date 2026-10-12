# iroh on ESP32: workshop

Peer-to-peer networking with [iroh] on ESP32 microcontrollers, in small steps.
Each step is a directory with its own program; you can jump to any of them.

The board is the [Seeed Studio XIAO ESP32-C5]: RISC-V, 8 MB flash, 8 MB PSRAM,
dual-band WiFi 6, BLE.

## Before the workshop

The first build downloads and compiles ESP-IDF, Espressif's SDK: several GB and
a good while. **Do this at home**, not on the conference WiFi.

1. Install [Rust](https://rustup.rs). The right nightly installs itself the
   first time you build here (see `rust-toolchain.toml`). No `espup` needed: the
   C5 is RISC-V.
2. Install the flashing and linking tools:

   ```sh
   cargo install espflash ldproxy
   ```

   On Linux, `espflash` needs `libudev` (`sudo apt install libudev-dev` on
   Debian/Ubuntu), and you need to be in the `dialout` group to use the serial
   port.
3. Make sure you have `python3` (3.9 or newer) and `git`; the ESP-IDF build uses
   them.
4. Build the first step once:

   ```sh
   cd 01-hello
   WIFI_CONFIG='your-ssid:your-password' cargo build --release
   ```

   If that finishes, you're set. (The other steps share that build.)
5. For the web pages: `rustup target add wasm32-unknown-unknown` and
   `cargo install wasm-bindgen-cli --version 0.2.126`.

## The web pages

Each step has a page that talks to the device: iroh compiled to WebAssembly,
running in the browser and reaching the device over a relay (browsers can't send
UDP). Build and serve them once:

```sh
cd web
npm run build
npm run serve   # http://localhost:8080
```

Use Chrome, Firefox or Safari; any of them works for these pages.

## Steps

Run a step from its directory; `cargo run` flashes the board over USB and opens
the serial monitor:

```sh
cd 01-hello
WIFI_CONFIG='your-ssid:your-password' cargo run --release
```

Each step's `main.rs` is short; the boilerplate (WiFi, an iroh endpoint sized
for a microcontroller) is in [`common/`](common/src/lib.rs). Compare a step
with the previous one (`diff 02-light/src/main.rs 03-identity/src/main.rs`) to
see what's new.

### 01-hello: an endpoint

Join WiFi, start an iroh endpoint, and print its endpoint id: the public key
anyone can dial it with.

```text
I (…) hello: Hello from iroh! My endpoint id is 5a1f…
```

There's no stored key yet, so every start prints a different id. Paste it into
**page 1**: the device answers, but turns the connection away, since it doesn't
offer any protocol yet.

### 02-light: a protocol

The device offers a protocol: switch the LED, ask whether it's on. The protocol
is an enum in [`protocol/src/v1.rs`](protocol/src/v1.rs), shared by the device and
the page, so both speak the same Rust types; [irpc] takes care of the wire
format. Each protocol has an ALPN, a name both sides agree on when connecting.

Paste the endpoint id into **page 2** and switch the light.

### 03-identity: a stable endpoint id

The secret key is generated once and kept in flash (NVS), so the endpoint id
survives restarts and reflashing. Same protocol as step 2; **page 3**
remembers the id.

### 04-button: watching

The BOOT button toggles the light too, so its state can change without anyone
asking. Protocol version 2 ([`protocol/src/v2.rs`](protocol/src/v2.rs), with its
own ALPN) adds `Watch`: the device sends the current state, then every change.
On **page 4**, press the button and watch the light follow.

## Layout

All devices are one Cargo workspace, so ESP-IDF is built once for all of them.

| Path | What |
|---|---|
| `common/` | WiFi, time, the stored key, and the iroh endpoint builder |
| `protocol/` | the light protocols, version 1 (steps 2, 3) and 2 (step 4) |
| `01-hello/` … `04-button/` | the steps |
| `web/` | the pages, one per step (not part of the workspace: it builds for the browser) |
| `sdkconfig.defaults` | ESP-IDF settings for all steps (PSRAM, buffer sizes, ...) |
| `.cargo/config.toml` | target, chip (`MCU`), and the flash command |

## Troubleshooting

- **No serial port / `espflash` can't connect:** use a USB cable that carries
  data, not just power.
- **The board loops through resets and prints nothing useful:** unplug it and
  plug it back in.
- **`WIFI_CONFIG is not set`:** it's read when building, so it goes on the
  `cargo run` command line.

[iroh]: https://www.iroh.computer
[irpc]: https://docs.rs/irpc
[Seeed Studio XIAO ESP32-C5]: https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C5-p-6609.html
