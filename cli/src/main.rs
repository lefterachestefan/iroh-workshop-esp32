//! Times light switch requests to a device (step 3, step 4 with `--v2`, or
//! step 5 with `--v3`), to
//! see where the time
//! goes: run it with `--relay-only` to take the same path as the browser, and
//! without to let iroh go direct over the LAN.
//!
//! ```sh
//! cargo run --release -- <endpoint-id>
//! cargo run --release -- <endpoint-id> --relay-only
//! ```

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use iroh::{endpoint::presets, Endpoint, EndpointId};
use irpc_iroh::IrohRemoteConnection;
use light_protocol::{v1, v2, v3};

#[derive(Parser)]
struct Args {
    /// The device's endpoint id, from its serial console.
    endpoint_id: EndpointId,
    /// How many requests to time.
    #[arg(short, long, default_value_t = 20)]
    count: usize,
    /// Pause between requests, in ms.
    #[arg(short, long, default_value_t = 200)]
    interval: u64,
    /// No UDP: reach the device only over the relay, like the browser does.
    #[arg(long)]
    relay_only: bool,
    /// Speak protocol version 2, for step 4.
    #[arg(long)]
    v2: bool,
    /// Speak protocol version 3, for step 5.
    #[arg(long, conflicts_with = "v2")]
    v3: bool,
    /// Don't switch: print the lamp's state, then every change (step 5).
    #[arg(long, requires = "v3")]
    watch: bool,
    /// Don't switch: switch automatic mode on or off (step 5).
    #[arg(long, requires = "v3", value_parser = parse_on_off)]
    auto: Option<bool>,
    /// Don't switch: set how dark it has to get, in percent, before the lamp
    /// switches on in automatic mode (step 5).
    #[arg(long, requires = "v3", value_parser = clap::value_parser!(u8).range(0..=100))]
    threshold: Option<u8>,
    /// Don't switch: set how long, in ms, it has to stay past the threshold
    /// before the lamp switches in automatic mode (step 5).
    #[arg(long, requires = "v3")]
    delay: Option<u16>,
}

fn parse_on_off(s: &str) -> Result<bool, String> {
    match s {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("on or off".into()),
    }
}

/// Version 1, 2 or 3 of the light protocol; `Set` is the same in all of them.
enum Light {
    V1(irpc::Client<v1::LightProtocol>),
    V2(irpc::Client<v2::LightProtocol>),
    V3(irpc::Client<v3::LightProtocol>),
}

impl Light {
    async fn set(&self, on: bool) -> Result<bool> {
        Ok(match self {
            Light::V1(client) => client.rpc(v1::Set { on }).await?,
            Light::V2(client) => client.rpc(v2::Set { on }).await?,
            Light::V3(client) => client.rpc(v3::Set { on }).await?.on,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut builder = Endpoint::builder(presets::N0);
    if args.relay_only {
        builder = builder.clear_ip_transports();
    }
    let endpoint = builder.bind().await?;

    let start = Instant::now();
    let alpn = if args.v3 {
        v3::ALPN
    } else if args.v2 {
        v2::ALPN
    } else {
        v1::ALPN
    };
    let conn = endpoint.connect(args.endpoint_id, alpn).await?;
    println!("{} connected in {:?}", now(), start.elapsed());
    let remote = IrohRemoteConnection::new(conn.clone());
    let light = if args.v3 {
        Light::V3(irpc::Client::boxed(remote))
    } else if args.v2 {
        Light::V2(irpc::Client::boxed(remote))
    } else {
        Light::V1(irpc::Client::boxed(remote))
    };

    if args.auto.is_some() || args.threshold.is_some() || args.delay.is_some() {
        let Light::V3(client) = &light else {
            unreachable!("the settings require --v3")
        };
        // All settings go together: change the given ones, keep the rest.
        let mut settings = client.rpc(v3::Get).await?.settings;
        if let Some(auto) = args.auto {
            settings.auto = auto;
        }
        if let Some(threshold) = args.threshold {
            settings.threshold = threshold;
        }
        if let Some(delay) = args.delay {
            settings.delay_ms = delay;
        }
        print_state(&client.rpc(settings).await?);
        return Ok(());
    }

    if args.watch {
        let Light::V3(client) = &light else {
            unreachable!("--watch requires --v3")
        };
        let mut states = client.server_streaming(v3::Watch, 4).await?;
        while let Some(state) = states.recv().await? {
            print_state(&state);
        }
        return Ok(());
    }

    let mut times = Vec::with_capacity(args.count);
    for i in 0..args.count {
        let on = i % 2 == 0;
        let start = Instant::now();
        light.set(on).await?;
        let elapsed = start.elapsed();
        times.push(elapsed);
        println!("{} set {:<3} {:>7.1} ms  via {}", now(), if on { "on" } else { "off" }, ms(elapsed), path(&conn));
        tokio::time::sleep(Duration::from_millis(args.interval)).await;
    }

    times.sort();
    println!(
        "min {:.1} ms, median {:.1} ms, max {:.1} ms",
        ms(times[0]),
        ms(times[times.len() / 2]),
        ms(times[times.len() - 1]),
    );
    conn.close(0u32.into(), b"bye");
    endpoint.close().await;
    Ok(())
}

/// The path the connection uses right now, with QUIC's RTT estimate for it.
fn path(conn: &iroh::endpoint::Connection) -> String {
    let paths = conn.paths();
    let Some(path) = paths.iter().find(|p| p.is_selected()) else {
        return "?".into();
    };
    let kind = if path.is_relay() { "relay" } else { "direct" };
    format!("{kind} (rtt {:.1} ms)", ms(path.rtt()))
}

fn print_state(state: &v3::LightState) {
    let settings = state.settings;
    println!(
        "{} light {:<3}  level {:>3}%  auto {:<3}  threshold {:>3}%  delay {} ms",
        now(),
        on_off(state.on),
        state.level,
        on_off(settings.auto),
        settings.threshold,
        settings.delay_ms,
    );
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Wall-clock time, UTC, like the device's log.
fn now() -> String {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let s = t.as_secs() % 86400;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600, s / 60 % 60, s % 60, t.subsec_millis())
}
