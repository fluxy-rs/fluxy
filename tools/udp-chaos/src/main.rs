use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;
use udp_chaos::{ChaosConfig, start};

#[derive(Debug, Parser)]
#[command(
    name = "udp-chaos",
    version,
    about = "UDP fault injection proxy for Fluxy tests"
)]
struct Args {
    /// Address the proxy listens on for the client.
    #[arg(long, default_value = "127.0.0.1:42430")]
    listen: SocketAddr,
    /// Real Fluxy server address.
    #[arg(long)]
    target: SocketAddr,
    /// Fraction of datagrams to drop, 0.0..=1.0.
    #[arg(long, default_value_t = 0.0)]
    loss: f64,
    /// Base delay in milliseconds.
    #[arg(long, default_value_t = 0)]
    delay_ms: u64,
    /// Maximum random extra delay in milliseconds.
    #[arg(long, default_value_t = 0)]
    jitter_ms: u64,
    /// Fraction of datagrams to duplicate, 0.0..=1.0.
    #[arg(long, default_value_t = 0.0)]
    duplicate: f64,
    /// Fraction of datagrams to reorder, 0.0..=1.0.
    #[arg(long, default_value_t = 0.0)]
    reorder: f64,
    /// Drop all datagrams after this many milliseconds (0 disables).
    #[arg(long, default_value_t = 0)]
    outage_after_ms: u64,
    /// Duration of the one-shot outage in milliseconds (0 disables).
    #[arg(long, default_value_t = 0)]
    outage_duration_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let config = ChaosConfig {
        loss: args.loss,
        delay: Duration::from_millis(args.delay_ms),
        jitter: Duration::from_millis(args.jitter_ms),
        duplicate: args.duplicate,
        reorder: args.reorder,
    };
    if (args.outage_after_ms > 0) != (args.outage_duration_ms > 0) {
        return Err("--outage-after-ms and --outage-duration-ms must be used together".into());
    }
    let proxy = start(args.target, args.listen, config).await?;
    println!("listen={}", proxy.addr);
    println!("target={}", args.target);
    println!(
        "chaos loss={} delay_ms={} jitter_ms={} duplicate={} reorder={}",
        args.loss, args.delay_ms, args.jitter_ms, args.duplicate, args.reorder
    );
    if args.outage_after_ms > 0 {
        let handle = proxy.handle.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(args.outage_after_ms)).await;
            handle.set_outage(true);
            tokio::time::sleep(Duration::from_millis(args.outage_duration_ms)).await;
            handle.set_outage(false);
        });
    }
    tokio::signal::ctrl_c().await?;
    proxy.shutdown().await;
    Ok(())
}
