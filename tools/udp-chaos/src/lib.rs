//! Deterministic-ish UDP fault injection proxy for Fluxy transport tests.
//!
//! The proxy binds one downstream UDP socket and creates one upstream socket
//! per client address. Every datagram can be dropped, delayed, duplicated, or
//! reordered. An outage switch lets a test simulate a temporary network cut.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};

const MAX_DATAGRAM: usize = 65_535;
const MAX_FLOWS: usize = 64;

#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Fraction of datagrams dropped, in 0.0..=1.0.
    pub loss: f64,
    /// Base delay applied to every forwarded datagram.
    pub delay: Duration,
    /// Maximum additional random delay.
    pub jitter: Duration,
    /// Fraction of datagrams sent twice.
    pub duplicate: f64,
    /// Fraction of datagrams given an extra delay to induce reordering.
    pub reorder: f64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            loss: 0.0,
            delay: Duration::ZERO,
            jitter: Duration::ZERO,
            duplicate: 0.0,
            reorder: 0.0,
        }
    }
}

impl ChaosConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("loss", self.loss),
            ("duplicate", self.duplicate),
            ("reorder", self.reorder),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!("{name} must be between 0 and 1"));
            }
        }
        if self.delay > Duration::from_secs(60) || self.jitter > Duration::from_secs(60) {
            return Err("delay and jitter must be at most 60 seconds".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChaosStatsSnapshot {
    pub received: u64,
    pub forwarded: u64,
    pub dropped: u64,
    pub duplicated: u64,
    pub delayed: u64,
    pub reordered: u64,
}

#[derive(Default)]
struct Stats {
    received: AtomicU64,
    forwarded: AtomicU64,
    dropped: AtomicU64,
    duplicated: AtomicU64,
    delayed: AtomicU64,
    reordered: AtomicU64,
}

impl Stats {
    fn snapshot(&self) -> ChaosStatsSnapshot {
        ChaosStatsSnapshot {
            received: self.received.load(Ordering::Relaxed),
            forwarded: self.forwarded.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            duplicated: self.duplicated.load(Ordering::Relaxed),
            delayed: self.delayed.load(Ordering::Relaxed),
            reordered: self.reordered.load(Ordering::Relaxed),
        }
    }
}

struct Shared {
    shutdown: watch::Sender<bool>,
    _keepalive: watch::Receiver<bool>,
}

impl Shared {
    fn new() -> Self {
        let (shutdown, keepalive) = watch::channel(false);
        Self {
            shutdown,
            _keepalive: keepalive,
        }
    }

    fn stop(&self) {
        let _ = self.shutdown.send(true);
    }

    fn subscribe(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }
}

/// Handle used by a test to toggle an outage or inspect counters.
#[derive(Clone)]
pub struct ChaosHandle {
    shared: Arc<Shared>,
    outage: Arc<AtomicBool>,
    stats: Arc<Stats>,
}

impl ChaosHandle {
    pub fn set_outage(&self, enabled: bool) {
        self.outage.store(enabled, Ordering::SeqCst);
    }

    pub fn outage(&self) -> bool {
        self.outage.load(Ordering::SeqCst)
    }

    pub fn stats(&self) -> ChaosStatsSnapshot {
        self.stats.snapshot()
    }

    pub async fn shutdown(&self) {
        self.shared.stop();
    }
}

pub struct UdpChaosProxy {
    pub addr: SocketAddr,
    pub handle: ChaosHandle,
}

impl UdpChaosProxy {
    pub async fn shutdown(&self) {
        self.handle.shutdown().await;
    }
}

impl Drop for UdpChaosProxy {
    fn drop(&mut self) {
        self.handle.shared.stop();
    }
}

/// Bind the downstream socket and start forwarding to the target.
pub async fn start(
    target: SocketAddr,
    bind: SocketAddr,
    config: ChaosConfig,
) -> io::Result<UdpChaosProxy> {
    config
        .validate()
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let downstream = Arc::new(UdpSocket::bind(bind).await?);
    let addr = downstream.local_addr()?;
    let shared = Arc::new(Shared::new());
    let outage = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Stats::default());
    let handle = ChaosHandle {
        shared: shared.clone(),
        outage: outage.clone(),
        stats: stats.clone(),
    };
    let config = Arc::new(config);
    let task_shared = shared.clone();
    tokio::spawn(async move {
        run_downstream(downstream, target, config, task_shared, outage, stats).await;
    });
    Ok(UdpChaosProxy { addr, handle })
}

async fn run_downstream(
    downstream: Arc<UdpSocket>,
    target: SocketAddr,
    config: Arc<ChaosConfig>,
    shared: Arc<Shared>,
    outage: Arc<AtomicBool>,
    stats: Arc<Stats>,
) {
    let mut flows: HashMap<SocketAddr, mpsc::Sender<Vec<u8>>> = HashMap::new();
    let mut buffer = vec![0_u8; MAX_DATAGRAM];
    let mut shutdown = shared.subscribe();
    loop {
        let received = tokio::select! {
            _ = shutdown.wait_for(|value| *value) => break,
            result = downstream.recv_from(&mut buffer) => result,
        };
        let (length, client) = match received {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("udp-chaos downstream receive failed: {error}");
                break;
            }
        };
        stats.received.fetch_add(1, Ordering::Relaxed);
        let sender = match flows.get(&client) {
            Some(sender) => sender.clone(),
            None => {
                if flows.len() >= MAX_FLOWS {
                    stats.dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let sender = spawn_flow(
                    client,
                    target,
                    downstream.clone(),
                    config.clone(),
                    shared.clone(),
                    outage.clone(),
                    stats.clone(),
                );
                flows.insert(client, sender.clone());
                sender
            }
        };
        if sender.send(buffer[..length].to_vec()).await.is_err() {
            flows.remove(&client);
        }
    }
}

fn spawn_flow(
    client: SocketAddr,
    target: SocketAddr,
    downstream: Arc<UdpSocket>,
    config: Arc<ChaosConfig>,
    shared: Arc<Shared>,
    outage: Arc<AtomicBool>,
    stats: Arc<Stats>,
) -> mpsc::Sender<Vec<u8>> {
    let (sender, receiver) = mpsc::channel(128);
    let task_shared = shared.clone();
    tokio::spawn(async move {
        let bind_addr = if target.is_ipv4() {
            SocketAddr::from(([0, 0, 0, 0], 0))
        } else {
            SocketAddr::from(([0_u16; 8], 0))
        };
        let upstream = match UdpSocket::bind(bind_addr).await {
            Ok(socket) => Arc::new(socket),
            Err(error) => {
                eprintln!("udp-chaos upstream bind failed: {error}");
                return;
            }
        };
        if let Err(error) = upstream.connect(target).await {
            eprintln!("udp-chaos upstream connect failed: {error}");
            return;
        }
        let seed = (u64::from(client.port()) << 32)
            ^ u64::from(target.port())
            ^ SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.subsec_nanos() as u64)
                .unwrap_or(0);
        let mut shutdown = task_shared.subscribe();
        let context = FlowContext {
            config,
            outage,
            stats,
            shutdown: shutdown.clone(),
        };
        let mut to_target = tokio::spawn(forward_to_target(
            receiver,
            upstream.clone(),
            context.clone(),
            seed,
        ));
        let mut to_client = tokio::spawn(forward_to_client(
            upstream,
            downstream,
            client,
            context,
            seed ^ 0x9e37_79b9_7f4a_7c15,
        ));
        tokio::select! {
            _ = shutdown.wait_for(|value| *value) => {}
            _ = &mut to_target => {}
            _ = &mut to_client => {}
        }
        to_target.abort();
        to_client.abort();
    });
    sender
}

#[derive(Clone)]
struct FlowContext {
    config: Arc<ChaosConfig>,
    outage: Arc<AtomicBool>,
    stats: Arc<Stats>,
    shutdown: watch::Receiver<bool>,
}

async fn forward_to_target(
    mut inbound: mpsc::Receiver<Vec<u8>>,
    upstream: Arc<UdpSocket>,
    mut context: FlowContext,
    seed: u64,
) {
    let mut rng = ChaosRng::new(seed);
    loop {
        let packet = tokio::select! {
            _ = context.shutdown.wait_for(|value| *value) => break,
            packet = inbound.recv() => packet,
        };
        let Some(packet) = packet else { break };
        if let Some(action) = plan(&context.config, &context.outage, &mut rng, &context.stats) {
            if !action.delay.is_zero() {
                tokio::time::sleep(action.delay).await;
            }
            for _ in 0..action.copies {
                if upstream.send(&packet).await.is_err() {
                    return;
                }
                context.stats.forwarded.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

async fn forward_to_client(
    upstream: Arc<UdpSocket>,
    downstream: Arc<UdpSocket>,
    client: SocketAddr,
    mut context: FlowContext,
    seed: u64,
) {
    let mut rng = ChaosRng::new(seed);
    let mut buffer = vec![0_u8; MAX_DATAGRAM];
    loop {
        let length = tokio::select! {
            _ = context.shutdown.wait_for(|value| *value) => break,
            result = upstream.recv(&mut buffer) => match result {
                Ok(length) => length,
                Err(_) => return,
            },
        };
        if let Some(action) = plan(&context.config, &context.outage, &mut rng, &context.stats) {
            if !action.delay.is_zero() {
                tokio::time::sleep(action.delay).await;
            }
            for _ in 0..action.copies {
                if downstream.send_to(&buffer[..length], client).await.is_err() {
                    return;
                }
                context.stats.forwarded.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

struct Action {
    delay: Duration,
    copies: u8,
}

fn plan(
    config: &ChaosConfig,
    outage: &AtomicBool,
    rng: &mut ChaosRng,
    stats: &Stats,
) -> Option<Action> {
    if outage.load(Ordering::SeqCst) || (config.loss > 0.0 && rng.unit() < config.loss) {
        stats.dropped.fetch_add(1, Ordering::Relaxed);
        return None;
    }
    let mut delay = config.delay;
    if !config.jitter.is_zero() {
        delay = delay.saturating_add(rng.duration(config.jitter));
    }
    if config.reorder > 0.0 && rng.unit() < config.reorder {
        stats.reordered.fetch_add(1, Ordering::Relaxed);
        delay = delay.saturating_add(config.delay.max(Duration::from_millis(5)));
    }
    if !delay.is_zero() {
        stats.delayed.fetch_add(1, Ordering::Relaxed);
    }
    let copies = if config.duplicate > 0.0 && rng.unit() < config.duplicate {
        stats.duplicated.fetch_add(1, Ordering::Relaxed);
        2
    } else {
        1
    };
    Some(Action { delay, copies })
}

struct ChaosRng {
    state: u64,
}

impl ChaosRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.max(0x9e37_79b9_7f4a_7c15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    fn duration(&mut self, maximum: Duration) -> Duration {
        if maximum.is_zero() {
            return Duration::ZERO;
        }
        Duration::from_nanos((self.unit() * maximum.as_nanos() as f64) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_invalid_probabilities() {
        let config = ChaosConfig {
            loss: 1.5,
            ..ChaosConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn outage_drops_every_packet() {
        let config = ChaosConfig::default();
        let outage = AtomicBool::new(true);
        let stats = Stats::default();
        let mut rng = ChaosRng::new(1);
        assert!(plan(&config, &outage, &mut rng, &stats).is_none());
        assert_eq!(stats.snapshot().dropped, 1);
    }

    #[test]
    fn duplicate_produces_two_copies() {
        let config = ChaosConfig {
            duplicate: 1.0,
            ..ChaosConfig::default()
        };
        let outage = AtomicBool::new(false);
        let stats = Stats::default();
        let mut rng = ChaosRng::new(1);
        let action = plan(&config, &outage, &mut rng, &stats).unwrap();
        assert_eq!(action.copies, 2);
        assert_eq!(stats.snapshot().duplicated, 1);
    }
}
