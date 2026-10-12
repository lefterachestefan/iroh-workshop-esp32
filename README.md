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

   If that finishes, you're set.

## Steps

### 01-hello

Join WiFi, start an iroh endpoint, and print its endpoint id: the public key
anyone can dial it with.

```sh
cd 01-hello
WIFI_CONFIG='your-ssid:your-password' cargo run --release
```

`cargo run` flashes the board over USB and opens the serial monitor. After a few
seconds:

```text
I (…) hello: Hello from iroh! My endpoint id is 5a1f…
```

There's no stored key yet, so every start prints a different id.

The boilerplate (WiFi, an iroh endpoint sized for a microcontroller) is in
[`common/`](common/src/lib.rs), so each step's `main.rs` only shows what's new.

## Layout

All steps are one Cargo workspace, so ESP-IDF is built once for all of them.

| Path | What |
|---|---|
| `common/` | WiFi, time, and the iroh endpoint builder shared by all steps |
| `01-hello/` | step 1 |
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
[Seeed Studio XIAO ESP32-C5]: https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C5-p-6609.html
