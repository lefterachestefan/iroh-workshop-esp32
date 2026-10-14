//! Times light switch requests to a device (step 3, or step 4 with `--v2`), to
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
use light_protocol::{v1, v2};

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
}

/// Version 1 or 2 of the light protocol; `Set` is the same in both.
enum Light {
    V1(irpc::Client<v1::LightProtocol>),
    V2(irpc::Client<v2::LightProtocol>),
}

impl Light {
    async fn set(&self, on: bool) -> Result<bool> {
        Ok(match self {
            Light::V1(client) => client.rpc(v1::Set { on }).await?,
            Light::V2(client) => client.rpc(v2::Set { on }).await?,
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
    let alpn = if args.v2 { v2::ALPN } else { v1::ALPN };
    let conn = endpoint.connect(args.endpoint_id, alpn).await?;
    println!("{} connected in {:?}", now(), start.elapsed());
    let remote = IrohRemoteConnection::new(conn.clone());
    let light = if args.v2 {
        Light::V2(irpc::Client::boxed(remote))
    } else {
        Light::V1(irpc::Client::boxed(remote))
    };

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

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Wall-clock time, UTC, like the device's log.
fn now() -> String {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let s = t.as_secs() % 86400;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600, s / 60 % 60, s % 60, t.subsec_millis())
}
