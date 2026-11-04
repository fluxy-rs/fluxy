use std::collections::HashMap;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use clap::{Parser, Subcommand};
use serde_json::json;

use fluxy_app::{AppConfig, AppPaths, Engine, HeadlessTransfer, OfferView, SendRequest};
use fluxy_core::transfer::CacheStatus;
use fluxy_core::{IdentityStore, TrustStore};
use fluxy_net::interface::{
    InterfaceBinding, Selection, SelectionError, default_route_interface, enumerate, select_auto,
    select_manual,
};
use fluxy_net::{
    ClipboardShareOffer, ControlSession, DiscoveryConfig, DiscoveryEvent, MdnsDiscovery,
    PairingLimiter, PeerRecord, SERVICE_TYPE, SessionEvent, TlsIdentity, TransferConfig, accept,
    accept_session, client_endpoint, client_pair, connect, connect_trusted, server_endpoint,
    server_endpoint_bound,
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "fluxy", version, about = "Fluxy LAN transfer CLI")]
struct Cli {
    /// Override the application data directory.
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the stable local device id.
    Id,
    /// Print engine status as JSON.
    Status,
    /// Print the effective config as JSON.
    Config,
    /// Browse mDNS for Fluxy peers.
    Discover {
        /// How long to scan, in seconds.
        #[arg(long, default_value_t = 3)]
        timeout: u64,
        /// Restrict discovery to one network interface.
        #[arg(long)]
        interface: Option<String>,
        /// Include loopback peers for local development only.
        #[arg(long)]
        dev_loopback: bool,
        /// Print one JSON array instead of streaming lines.
        #[arg(long)]
        json: bool,
    },
    /// Listen for one QUIC peer and complete SAS pairing.
    Serve {
        /// Bind exactly this address; loopback requires --dev-loopback. Omit it
        /// to let the interface selection choose (--interface/--address, or the
        /// automatic LAN pick).
        #[arg(long)]
        listen: Option<SocketAddr>,
        /// Explicitly allow a loopback listener for local development.
        #[arg(long)]
        dev_loopback: bool,
        /// Exit after the first connection closes instead of waiting for a resume.
        #[arg(long)]
        once: bool,
        /// Bind and advertise on one network interface.
        #[arg(long)]
        interface: Option<String>,
        /// Address to bind on --interface. Omit it to use the interface's
        /// preferred address.
        #[arg(long)]
        address: Option<IpAddr>,
        /// Accept incoming files without a prompt. This is the receive side of a
        /// soak run or a scripted transfer; the desktop app asks the user instead.
        #[arg(long)]
        accept_files: bool,
    },
    /// List the local interfaces and the address automatic selection would use.
    Interfaces {
        /// Print one JSON object instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Pair with a peer and store its certificate pin.
    Pair {
        /// Peer QUIC address.
        addr: SocketAddr,
        /// 64-character hex BLAKE3 certificate fingerprint printed by serve.
        #[arg(long)]
        fingerprint: String,
        /// Explicitly allow a loopback target for local development.
        #[arg(long)]
        dev_loopback: bool,
    },
    /// Send chat to an already-paired peer.
    Chat {
        /// Peer QUIC address.
        addr: SocketAddr,
        /// Stable device id of the already-paired server.
        #[arg(long)]
        device: Uuid,
        /// Text to send and wait for an application ACK.
        #[arg(long)]
        message: String,
        /// Explicitly allow a loopback target for local development.
        #[arg(long)]
        dev_loopback: bool,
    },
    /// Offer the supplied text as a clipboard share to a paired peer.
    Clipboard {
        /// Peer QUIC address.
        addr: SocketAddr,
        /// Stable device id of the already-paired server.
        #[arg(long)]
        device: Uuid,
        /// Text to offer; the peer must accept before it is transmitted.
        #[arg(long)]
        text: String,
        /// Explicitly allow a loopback target for local development.
        #[arg(long)]
        dev_loopback: bool,
    },
    /// Send one file to an already-paired peer.
    TransferSend {
        /// Peer QUIC address.
        addr: SocketAddr,
        /// Stable device id of the already-paired peer.
        #[arg(long)]
        device: Uuid,
        /// File to send.
        #[arg(long)]
        file: PathBuf,
        /// Reuse a transfer id so a retry resumes the peer's checkpoint.
        #[arg(long)]
        transfer_id: Option<Uuid>,
        /// Block size in bytes.
        #[arg(long, default_value_t = 4 * 1024 * 1024)]
        block_size: u32,
        /// Hash the file and print the manifest without contacting the peer.
        #[arg(long)]
        prepare_only: bool,
        /// Explicitly allow a loopback target for local development.
        #[arg(long)]
        dev_loopback: bool,
    },
    /// Receive one file transfer from an already-paired peer.
    TransferRecv {
        /// Listen address; loopback requires --dev-loopback.
        #[arg(long, default_value = "127.0.0.1:0")]
        listen: SocketAddr,
        /// Explicitly allow a loopback listener for local development.
        #[arg(long)]
        dev_loopback: bool,
        /// Give up when no transfer completes within this many seconds.
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Legacy combined pair/chat command.
    Connect {
        /// Peer QUIC address.
        addr: SocketAddr,
        /// 64-character hex BLAKE3 certificate fingerprint printed by serve.
        #[arg(long)]
        fingerprint: Option<String>,
        /// Resume an already-paired server by device id instead of pairing.
        #[arg(long)]
        resume: Option<Uuid>,
        /// Explicitly allow a loopback target for local development.
        #[arg(long)]
        dev_loopback: bool,
        /// Optional text to send and wait for an application ACK.
        #[arg(long)]
        message: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let paths = match cli.data_dir {
        Some(root) => AppPaths::from_root(root),
        None => AppPaths::resolve()?,
    };
    fluxy_app::logging::init(&paths.log_dir)?;
    match cli.command {
        Command::Id => {
            let engine = Engine::load(paths)?;
            println!("{}", engine.identity().device_id());
        }
        Command::Status => {
            let engine = Engine::load(paths)?;
            println!("{}", serde_json::to_string_pretty(&engine.status())?);
        }
        Command::Config => {
            let engine = Engine::load(paths)?;
            println!("{}", serde_json::to_string_pretty(&json!(engine.config()))?);
        }
        Command::Discover {
            timeout,
            interface,
            dev_loopback,
            json,
        } => discover(paths, timeout, interface, dev_loopback, json).await?,
        Command::Serve {
            listen,
            dev_loopback,
            once,
            interface,
            address,
            accept_files,
        } => {
            serve(
                paths,
                listen,
                dev_loopback,
                once,
                interface,
                address,
                accept_files,
            )
            .await?
        }
        Command::Interfaces { json } => interfaces(json)?,
        Command::Pair {
            addr,
            fingerprint,
            dev_loopback,
        } => pair_cmd(paths, addr, &fingerprint, dev_loopback).await?,
        Command::Chat {
            addr,
            device,
            message,
            dev_loopback,
        } => chat(paths, addr, device, message, dev_loopback).await?,
        Command::Clipboard {
            addr,
            device,
            text,
            dev_loopback,
        } => clipboard(paths, addr, device, text, dev_loopback).await?,
        Command::TransferSend {
            addr,
            device,
            file,
            transfer_id,
            block_size,
            prepare_only,
            dev_loopback,
        } => {
            transfer_send(
                paths,
                addr,
                device,
                file,
                transfer_id,
                block_size,
                prepare_only,
                dev_loopback,
            )
            .await?
        }
        Command::TransferRecv {
            listen,
            dev_loopback,
            timeout,
        } => transfer_recv(paths, listen, dev_loopback, timeout).await?,
        Command::Connect {
            addr,
            fingerprint,
            resume,
            dev_loopback,
            message,
        } => {
            connect_cmd(
                paths,
                addr,
                fingerprint.as_deref(),
                resume,
                dev_loopback,
                message,
            )
            .await?
        }
    }
    Ok(())
}

async fn discover(
    paths: AppPaths,
    timeout: u64,
    interface: Option<String>,
    dev_loopback: bool,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if timeout == 0 {
        return Err("--timeout must be at least 1 second".into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let config = DiscoveryConfig {
        interface,
        allow_loopback: dev_loopback,
        ..DiscoveryConfig::default()
    };
    let mut discovery = MdnsDiscovery::browse(identity.device_id(), config).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let mut peers: HashMap<Uuid, PeerRecord> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, discovery.next_event()).await {
            Ok(Some(DiscoveryEvent::Found(peer))) => {
                let is_new = peers.insert(peer.device_id, (*peer).clone()).is_none();
                if is_new {
                    order.push(peer.device_id);
                }
                if !json_output && is_new {
                    println!("{}", format_peer(&peer));
                    std::io::stdout().flush()?;
                }
            }
            Ok(Some(DiscoveryEvent::Lost(device_id))) => {
                peers.remove(&device_id);
                order.retain(|candidate| candidate != &device_id);
                if !json_output {
                    println!("lost device={device_id}");
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    discovery.shutdown();

    if json_output {
        let output = order
            .iter()
            .filter_map(|device_id| peers.get(device_id))
            .map(|peer| {
                json!({
                    "deviceId": peer.device_id,
                    "name": peer.name,
                    "platform": peer.platform,
                    "addresses": peer.addresses.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "port": peer.port,
                    "fingerprint": hex::encode(peer.cert_fingerprint),
                    "protocolVersion": peer.protocol_version,
                    "features": peer.features,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if peers.is_empty() {
        println!("no peers found");
    }
    Ok(())
}

fn format_peer(peer: &PeerRecord) -> String {
    let addresses = peer
        .addresses
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "peer device={} name={} platform={} addr={} port={} version={}",
        peer.device_id, peer.name, peer.platform, addresses, peer.port, peer.protocol_version
    )
}

async fn chat(
    paths: AppPaths,
    addr: SocketAddr,
    device: Uuid,
    message: String,
    dev_loopback: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if addr.ip().is_loopback() && !dev_loopback {
        return Err("loopback target requires --dev-loopback".into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let client_tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    let mut session = connect_trusted(addr, device, &identity, &client_tls, &mut trust).await?;
    let peer = session.peer();
    println!("resumed device={} name={}", peer.device_id, peer.name);
    chat_once(&mut session, Some(message)).await?;
    close_session(session).await;
    Ok(())
}

async fn clipboard(
    paths: AppPaths,
    addr: SocketAddr,
    device: Uuid,
    text: String,
    dev_loopback: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if addr.ip().is_loopback() && !dev_loopback {
        return Err("loopback target requires --dev-loopback".into());
    }
    if text.is_empty() {
        return Err("clipboard text cannot be empty".into());
    }
    if text.len() > fluxy_net::MAX_CLIPBOARD_BYTES {
        return Err(format!(
            "clipboard text exceeds {} bytes",
            fluxy_net::MAX_CLIPBOARD_BYTES
        )
        .into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let client_tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    let mut session = connect_trusted(addr, device, &identity, &client_tls, &mut trust).await?;
    let peer = session.peer();
    println!("resumed device={} name={}", peer.device_id, peer.name);

    let digest = *blake3::hash(text.as_bytes()).as_bytes();
    let offer_id = Uuid::now_v7();
    let preview: String = text.chars().take(160).collect();
    let origin_id = identity.device_id();
    let offer = ClipboardShareOffer {
        offer_id,
        size_bytes: text.len() as u64,
        digest,
        preview,
        ttl_ms: 120_000,
        origin_id,
        hop: 0,
    };
    session.send_clipboard_offer(offer).await?;
    println!(
        "clipboard-offer id={offer_id} size={} waiting-for-consent",
        text.len()
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    let mut sent_text = false;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            session.close().await;
            return Err("timed out waiting for the receiver to confirm".into());
        }
        let event = tokio::time::timeout(remaining, session.next_event())
            .await
            .map_err(|_| "timed out waiting for the receiver to confirm")?
            .ok_or("session closed before the receiver confirmed")?;
        match event {
            SessionEvent::ClipboardAck {
                offer_id: ack_id,
                accepted,
                applied,
                code,
                message,
            } if ack_id == offer_id => {
                if applied {
                    println!("clipboard-applied id={offer_id}");
                    session.close().await;
                    return Ok(());
                }
                if !accepted {
                    println!("clipboard-rejected id={offer_id} code={code} message={message}");
                    session.close().await;
                    return Ok(());
                }
                if !code.is_empty() {
                    session.close().await;
                    return Err(
                        format!("receiver could not apply clipboard: {code} {message}").into(),
                    );
                }
                if !sent_text {
                    session
                        .send_clipboard_text(offer_id, text.clone(), digest)
                        .await?;
                    sent_text = true;
                    println!("clipboard-text-sent id={offer_id}");
                }
            }
            SessionEvent::ClipboardAck {
                offer_id: ack_id, ..
            } => {
                eprintln!("ignoring clipboard acknowledgement for unknown offer {ack_id}");
            }
            SessionEvent::Disconnected { reason } => {
                return Err(format!("disconnected before clipboard completed: {reason}").into());
            }
            SessionEvent::Reconnected { .. } => {}
            SessionEvent::Message { .. } | SessionEvent::Ack { .. } => {}
            SessionEvent::ClipboardOffer { .. } | SessionEvent::ClipboardText { .. } => {}
            SessionEvent::TransferOffer { .. }
            | SessionEvent::TransferAccept { .. }
            | SessionEvent::TransferReject { .. }
            | SessionEvent::TransferProgress { .. }
            | SessionEvent::TransferComplete { .. }
            | SessionEvent::TransferError { .. }
            | SessionEvent::TransferPause { .. }
            | SessionEvent::TransferCancel { .. }
            | SessionEvent::TransferResume { .. } => {}
        }
    }
}

/// Milliseconds since the Unix epoch, used in machine-readable output lines.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Throughput in MiB/s, guarding a zero-length interval.
fn mib_per_second(bytes: u64, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return 0.0;
    }
    (bytes as f64 / (1024.0 * 1024.0)) / seconds
}

/// Emit a progress line at most once every two seconds.
struct ProgressPrinter {
    role: &'static str,
    last_ms: AtomicU64,
}

impl ProgressPrinter {
    fn new(role: &'static str) -> Self {
        Self {
            role,
            last_ms: AtomicU64::new(0),
        }
    }

    fn report(&self, done: u64, total: u64) {
        let now = now_ms();
        let previous = self.last_ms.load(Ordering::Relaxed);
        if done < total && now.saturating_sub(previous) < 2_000 {
            return;
        }
        self.last_ms.store(now, Ordering::Relaxed);
        println!("FLUXY_XFER role={} progress={done}/{total}", self.role);
        let _ = std::io::stdout().flush();
    }
}

#[allow(clippy::too_many_arguments)]
async fn transfer_send(
    paths: AppPaths,
    addr: SocketAddr,
    device: Uuid,
    file: PathBuf,
    transfer_id: Option<Uuid>,
    block_size: u32,
    prepare_only: bool,
    dev_loopback: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if addr.ip().is_loopback() && !dev_loopback {
        return Err("loopback target requires --dev-loopback".into());
    }
    if block_size == 0 {
        return Err("--block-size must be at least one byte".into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    let driver = HeadlessTransfer::from_paths(&paths, TransferConfig::default())?;

    let mut request = SendRequest::new(file.clone()).with_block_size(block_size);
    if let Some(id) = transfer_id {
        request = request.with_transfer_id(id);
    }

    let hashing = ProgressPrinter::new("send-hash");
    let prepared_at = std::time::Instant::now();
    let cached = driver.manifest_with_progress(&file, block_size, &mut |progress| {
        hashing.report(progress.bytes_done, progress.total_bytes);
    })?;
    let manifest = cached.manifest;
    println!(
        "FLUXY_XFER role=send prepared file={} size={} block_size={} blocks={} root_hash={} manifest_digest={} manifest={} hash_ms={}",
        request.wire_name()?,
        manifest.file_size,
        manifest.block_size,
        manifest.block_count(),
        manifest.root_hash_hex(),
        manifest.manifest_digest_hex(),
        match cached.status {
            CacheStatus::Hit => "hit",
            CacheStatus::Miss => "miss",
        },
        prepared_at.elapsed().as_millis(),
    );
    std::io::stdout().flush()?;
    if prepare_only {
        return Ok(());
    }

    let mut session = connect_trusted(addr, device, &identity, &tls, &mut trust).await?;
    println!(
        "FLUXY_XFER role=send peer={} transfer={}",
        session.peer().device_id,
        request.transfer_id
    );
    std::io::stdout().flush()?;

    let started = std::time::Instant::now();
    let mut on_event = |event: SessionEvent| {
        if let SessionEvent::Message { text, .. } = event {
            println!("message: {text}");
            let _ = std::io::stdout().flush();
        }
    };
    let result = driver
        .send(&mut session, &identity, request.clone(), &mut on_event)
        .await;
    let elapsed = started.elapsed();
    match result {
        Ok(outcome) => {
            println!(
                "FLUXY_XFER role=send status=ok transfer={} file={} size={} root_hash={} bytes_served={} manifest_reused={} elapsed_ms={} throughput_mib_s={:.1}",
                outcome.transfer_id,
                outcome.file_name,
                outcome.file_size,
                outcome.root_hash,
                outcome.bytes_served,
                outcome.manifest_reused,
                elapsed.as_millis(),
                mib_per_second(outcome.bytes_served, elapsed),
            );
            std::io::stdout().flush()?;
            close_session(session).await;
            Ok(())
        }
        Err(error) => {
            println!(
                "FLUXY_XFER role=send status=failed transfer={} code={} code_number={} retryable={} message={}",
                request.transfer_id,
                error.code_name(),
                error.code_number(),
                error.is_retryable(),
                error,
            );
            std::io::stdout().flush()?;
            session.close().await;
            Err(error.into())
        }
    }
}

async fn transfer_recv(
    paths: AppPaths,
    listen: SocketAddr,
    dev_loopback: bool,
    timeout_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    if listen.ip().is_loopback() && !dev_loopback {
        return Err("loopback listener requires --dev-loopback".into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    let endpoint = server_endpoint(&tls, listen).await?;
    let actual = endpoint.local_addr()?;
    let driver = HeadlessTransfer::from_paths(&paths, TransferConfig::default())?;
    println!(
        "FLUXY_XFER role=recv listen={actual} device={} fingerprint={} root={}",
        identity.device_id(),
        hex::encode(tls.fingerprint),
        driver.received_dir().display(),
    );
    std::io::stdout().flush()?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs.max(1));
    let mut limiter = PairingLimiter::default();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("no transfer arrived before --timeout".into());
        }
        let connection = match tokio::time::timeout(remaining, accept(&endpoint)).await {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => return Err("no transfer arrived before --timeout".into()),
        };
        let accepted = accept_session(
            &connection,
            &identity,
            &tls,
            &mut trust,
            &mut limiter,
            confirm_sas,
        )
        .await;
        trust.save(&paths.trust_file)?;
        let mut session = match accepted {
            Ok(session) => session,
            Err(error) => {
                eprintln!("connection rejected: {error}");
                continue;
            }
        };
        let peer = session.peer();
        println!(
            "FLUXY_XFER role=recv peer={} resumed={}",
            peer.device_id, peer.resumed
        );
        std::io::stdout().flush()?;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                session.close().await;
                return Err("no transfer arrived before --timeout".into());
            }
            let event = match tokio::time::timeout(remaining, session.next_event()).await {
                Ok(Some(event)) => event,
                Ok(None) => {
                    session.close().await;
                    return Err("the peer closed the session before sending a file".into());
                }
                Err(_) => {
                    session.close().await;
                    return Err("no transfer arrived before --timeout".into());
                }
            };
            match event {
                SessionEvent::TransferOffer { .. } => {
                    let Some(offer) = OfferView::from_event(&event) else {
                        continue;
                    };
                    println!(
                        "FLUXY_XFER role=recv offered transfer={} file={} size={}",
                        offer.transfer_id, offer.file_name, offer.file_size
                    );
                    std::io::stdout().flush()?;
                    let printer = ProgressPrinter::new("recv");
                    let started = std::time::Instant::now();
                    let connection = session.connection();
                    let sender = session.sender();
                    let session_id = session.session_id();
                    let result = driver
                        .receive(
                            &connection,
                            &sender,
                            session_id,
                            &identity,
                            offer.clone(),
                            Some(&|done: u64, total: u64| printer.report(done, total)),
                        )
                        .await;
                    let elapsed = started.elapsed();
                    match result {
                        Ok(outcome) => {
                            println!(
                                "FLUXY_XFER role=recv status=ok transfer={} file={} size={} root_hash={} path={} bytes_written={} missing_bytes_before={} resumed={} elapsed_ms={} throughput_mib_s={:.1}",
                                outcome.transfer_id,
                                outcome.file_name,
                                outcome.file_size,
                                outcome.root_hash,
                                outcome.output_path.display(),
                                outcome.bytes_written,
                                outcome.missing_bytes_before,
                                outcome.resumed,
                                elapsed.as_millis(),
                                mib_per_second(outcome.bytes_written, elapsed),
                            );
                            std::io::stdout().flush()?;
                            session.close().await;
                            return Ok(());
                        }
                        Err(error) => {
                            println!(
                                "FLUXY_XFER role=recv status=failed transfer={} code={} code_number={} retryable={} partial={} message={}",
                                offer.transfer_id,
                                error.code_name(),
                                error.code_number(),
                                error.is_retryable(),
                                driver.partial_path(offer.transfer_id).display(),
                                error,
                            );
                            std::io::stdout().flush()?;
                            session.close().await;
                            return Err(error.into());
                        }
                    }
                }
                SessionEvent::Message { text, .. } => {
                    println!("message: {text}");
                    let _ = std::io::stdout().flush();
                }
                SessionEvent::Disconnected { reason } => {
                    println!("FLUXY_XFER role=recv disconnected reason={reason}");
                    session.close().await;
                    break;
                }
                _ => {}
            }
        }
    }
}

/// List every local interface, best candidate first, and say which one the
/// automatic pick would use. This is the command the T-11 manual check reads.
fn interfaces(json_output: bool) -> Result<(), Box<dyn std::error::Error>> {
    let interfaces = enumerate().map_err(|error| error.to_string())?;
    let default_route = default_route_interface();
    let recommended = select_auto(&interfaces, default_route.as_deref(), false).ok();
    if json_output {
        let payload = interfaces
            .iter()
            .map(|interface| {
                json!({
                    "name": interface.name,
                    "label": interface.label,
                    "kind": interface.kind.as_str(),
                    "index": interface.index,
                    "up": interface.up,
                    "virtual": interface.is_virtual(),
                    "default_route": default_route
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&interface.name)),
                    "recommended": recommended
                        .as_ref()
                        .is_some_and(|selection| selection.interface.name == interface.name),
                    "addresses": interface
                        .addresses
                        .iter()
                        .map(|address| address.display(&interface.name))
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "default_route": default_route,
                "selected": recommended.as_ref().map(ToString::to_string),
                "interfaces": payload,
            }))?
        );
        return Ok(());
    }
    println!(
        "default_route={}",
        default_route.as_deref().unwrap_or("(none)")
    );
    for interface in &interfaces {
        let mark = if recommended
            .as_ref()
            .is_some_and(|selection| selection.interface.name == interface.name)
        {
            " <- automatic"
        } else {
            ""
        };
        let flags = [
            if interface.up { "up" } else { "down" },
            interface.kind.as_str(),
            if interface.is_virtual() {
                "virtual"
            } else {
                "physical"
            },
        ]
        .join(",");
        // The platform name is what --interface accepts, but on Windows it is an
        // adapter GUID, so the friendly name leads and the raw name follows.
        println!(
            "{} (name={}) [{}] index={} addresses={}{}",
            interface.label,
            interface.name,
            flags,
            interface.index,
            interface
                .addresses
                .iter()
                .map(|address| address.display(&interface.label))
                .collect::<Vec<_>>()
                .join(", "),
            mark
        );
    }
    match recommended {
        Some(selection) => println!("automatic={selection}"),
        None => println!("automatic=(unavailable)"),
    }
    Ok(())
}

/// Turn the CLI flags into the address to bind and the interface to pin.
///
/// An explicit @--listen@ wins and keeps the old behaviour; otherwise the
/// interface selection decides, which is what makes the CLI a usable T-11 tool
/// on a machine with several adapters.
fn pick_selection(
    interface: Option<&str>,
    address: Option<IpAddr>,
    dev_loopback: bool,
) -> Result<Selection, SelectionError> {
    let interfaces = enumerate()?;
    let default_route = default_route_interface();
    match (interface, address) {
        (Some(name), Some(address)) => {
            select_manual(&interfaces, name, &address.to_string(), dev_loopback)
        }
        (Some(name), None) => {
            let wanted = interfaces
                .iter()
                .find(|candidate| candidate.matches(name))
                .and_then(|candidate| candidate.preferred_address())
                .ok_or_else(|| SelectionError::MissingInterface(name.to_string()))?;
            select_manual(&interfaces, name, &wanted.address.to_string(), dev_loopback)
        }
        (None, Some(address)) => {
            let owner = interfaces
                .iter()
                .find(|candidate| candidate.address(address).is_some())
                .ok_or_else(|| {
                    SelectionError::Unavailable(format!(
                        "{address} is not an address of any local interface"
                    ))
                })?;
            select_manual(&interfaces, &owner.name, &address.to_string(), dev_loopback)
        }
        (None, None) => select_auto(&interfaces, default_route.as_deref(), dev_loopback),
    }
}

/// The interface a pin should target for an explicitly bound address.
fn pin_for(address: IpAddr, interface: Option<&str>) -> Result<Option<InterfaceBinding>, String> {
    let interfaces = enumerate().map_err(|error| error.to_string())?;
    let owner = match interface {
        Some(name) => interfaces
            .iter()
            .find(|candidate| candidate.matches(name))
            .ok_or_else(|| format!("interface {name} is not present"))?,
        None => interfaces
            .iter()
            .find(|candidate| candidate.address(address).is_some())
            .ok_or_else(|| format!("{address} is not an address of any local interface"))?,
    };
    Ok(Some(InterfaceBinding {
        index: owner.index,
        name: owner.name.clone(),
        ipv6: address.is_ipv6(),
    }))
}

/// Close the session and give the close frame a moment to reach the peer. A
/// process that exits right after `close` never sends it, and the server then
/// waits for its idle timeout before accepting the next connection.
async fn close_session(session: ControlSession) {
    session.close().await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

/// How long the CLI server waits for the current peer before closing the
/// session so the next one can connect.
const IDLE_SESSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

async fn serve(
    paths: AppPaths,
    listen: Option<SocketAddr>,
    dev_loopback: bool,
    once: bool,
    interface: Option<String>,
    address: Option<IpAddr>,
    accept_files: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (listen, binding, selection) = match listen {
        Some(listen) => {
            if listen.ip().is_loopback() && !dev_loopback {
                return Err("loopback listener requires --dev-loopback".into());
            }
            let binding = pin_for(listen.ip(), interface.as_deref())?;
            (listen, binding, None)
        }
        None => {
            let selection = pick_selection(interface.as_deref(), address, dev_loopback)
                .map_err(|error| error.to_string())?;
            (
                selection.socket_addr(0),
                Some(selection.binding()),
                Some(selection),
            )
        }
    };
    let interface = match (&selection, &interface) {
        // The mDNS layer (if-addrs/mdns-sd) names interfaces its own way: the
        // friendly name on Windows, the platform name elsewhere.
        (Some(selection), _) => Some(selection.interface.discovery_name().to_string()),
        (None, Some(name)) => Some(name.clone()),
        (None, None) => None,
    };
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    let (endpoint, pinned) = server_endpoint_bound(&tls, listen, binding.as_ref()).await?;
    let actual = endpoint.local_addr()?;
    println!("listen={actual}");
    if let Some(selection) = &selection {
        println!("selection={selection}");
    }
    println!("interface_pinned={pinned}");
    println!("device={}", identity.device_id());
    println!("fingerprint={}", hex::encode(tls.fingerprint));

    let _discovery = if actual.ip().is_loopback() {
        println!("discovery=disabled-loopback");
        None
    } else {
        let config = AppConfig::load_or_default(&paths.config_file)?;
        let discovery_config = DiscoveryConfig {
            interface,
            allow_loopback: false,
            ..DiscoveryConfig::default()
        };
        match MdnsDiscovery::advertise(
            identity.device_id(),
            &config.device_name,
            std::env::consts::OS,
            actual.port(),
            tls.fingerprint,
            discovery_config,
        )
        .await
        {
            Ok(discovery) => {
                println!("discovery=mdns service={SERVICE_TYPE}");
                Some(discovery)
            }
            Err(error) => {
                println!("discovery=unavailable error={error}");
                None
            }
        }
    };
    std::io::stdout().flush()?;

    // The long-running server needs its own driver when it is asked to accept
    // files; a plain serve only prints the offer.
    let driver = if accept_files {
        Some(HeadlessTransfer::from_paths(
            &paths,
            TransferConfig::default(),
        )?)
    } else {
        None
    };

    let mut limiter = PairingLimiter::default();
    loop {
        let connection = accept(&endpoint).await?;
        let accepted = accept_session(
            &connection,
            &identity,
            &tls,
            &mut trust,
            &mut limiter,
            confirm_sas,
        )
        .await;
        // Persist trust as soon as pairing committed. The control stream can
        // still fail (for example when the peer closes it right after
        // pairing); losing the trust entry would make that a silent failure.
        trust.save(&paths.trust_file)?;
        let mut session = match accepted {
            Ok(session) => session,
            Err(error) => {
                eprintln!("connection rejected: {error}");
                if once {
                    return Err(error.into());
                }
                continue;
            }
        };
        let peer = session.peer();
        if peer.resumed {
            println!("resumed device={} name={}", peer.device_id, peer.name);
        } else {
            println!(
                "paired device={} name={} sas={}",
                peer.device_id, peer.name, peer.sas
            );
        }
        println!("control ready");
        std::io::stdout().flush()?;

        // A scripted peer may close without waiting for the QUIC close handshake,
        // and the loop can only accept the next connection once this session
        // ends. An idle session is therefore closed by the server rather than
        // left to the transport timeout, which would block every later peer.
        loop {
            let event = match tokio::time::timeout(IDLE_SESSION_TIMEOUT, session.next_event()).await
            {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    println!("idle timeout; closing the session");
                    std::io::stdout().flush()?;
                    session.close().await;
                    break;
                }
            };
            match event {
                SessionEvent::Message { text, .. } => {
                    println!("message: {text}");
                    std::io::stdout().flush()?;
                }
                SessionEvent::Ack { message_id, .. } => println!("ack: {message_id}"),
                SessionEvent::ClipboardOffer {
                    offer_id,
                    size_bytes,
                    preview,
                    ttl_ms,
                    ..
                } => {
                    println!(
                        "clipboard-offer id={offer_id} size={size_bytes} ttl_ms={ttl_ms} preview={preview:?}"
                    );
                    std::io::stdout().flush()?;
                    // The headless CLI has no secure way to ask the user for
                    // consent, so it never writes an incoming clipboard.
                    let _ = session
                        .sender()
                        .send_clipboard_ack(
                            offer_id,
                            false,
                            false,
                            "cli_no_consent",
                            "headless CLI cannot confirm clipboard writes",
                        )
                        .await;
                }
                SessionEvent::ClipboardText { text, .. } => {
                    println!("clipboard-text bytes={} (not displayed)", text.len());
                    std::io::stdout().flush()?;
                }
                SessionEvent::ClipboardAck {
                    offer_id,
                    accepted,
                    applied,
                    code,
                    ..
                } => println!(
                    "clipboard-ack id={offer_id} accepted={accepted} applied={applied} code={code}"
                ),
                SessionEvent::Reconnected { reason } => {
                    println!("reconnected after: {reason}")
                }
                SessionEvent::Disconnected { reason } => {
                    println!("disconnected: {reason}");
                    break;
                }
                SessionEvent::TransferOffer { .. } => {
                    let Some(offer) = OfferView::from_event(&event) else {
                        continue;
                    };
                    println!(
                        "transfer-offer file={} size={}",
                        offer.file_name, offer.file_size
                    );
                    std::io::stdout().flush()?;
                    let Some(driver) = driver.as_ref() else {
                        continue;
                    };
                    let printer = ProgressPrinter::new("recv");
                    let started = std::time::Instant::now();
                    let connection = session.connection();
                    let sender = session.sender();
                    let session_id = session.session_id();
                    let result = driver
                        .receive(
                            &connection,
                            &sender,
                            session_id,
                            &identity,
                            offer.clone(),
                            Some(&|done: u64, total: u64| printer.report(done, total)),
                        )
                        .await;
                    match result {
                        Ok(outcome) => println!(
                            "FLUXY_XFER role=recv status=ok transfer={} file={} size={} bytes={} elapsed_ms={}",
                            outcome.transfer_id,
                            outcome.file_name,
                            outcome.file_size,
                            outcome.bytes_written,
                            started.elapsed().as_millis(),
                        ),
                        Err(error) => {
                            println!(
                                "FLUXY_XFER role=recv status=error transfer={} error={error}",
                                offer.transfer_id
                            );
                            std::io::stdout().flush()?;
                        }
                    }
                    std::io::stdout().flush()?;
                }
                SessionEvent::TransferProgress {
                    bytes_done,
                    bytes_total,
                    ..
                } => println!("transfer-progress {bytes_done}/{bytes_total}"),
                SessionEvent::TransferComplete { transfer_id } => {
                    println!("transfer-complete id={transfer_id}")
                }
                SessionEvent::TransferError { code, message, .. } => {
                    println!("transfer-error code={code} message={message}")
                }
                SessionEvent::TransferAccept { .. }
                | SessionEvent::TransferReject { .. }
                | SessionEvent::TransferPause { .. }
                | SessionEvent::TransferCancel { .. }
                | SessionEvent::TransferResume { .. } => {}
            }
        }
        if once {
            break;
        }
        eprintln!("waiting for a reconnect");
    }
    Ok(())
}

async fn pair_cmd(
    paths: AppPaths,
    addr: SocketAddr,
    fingerprint: &str,
    dev_loopback: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if addr.ip().is_loopback() && !dev_loopback {
        return Err("loopback target requires --dev-loopback".into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let client_tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    let expected = parse_fingerprint(fingerprint)?;
    let endpoint = client_endpoint(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        &client_tls,
        expected,
    )
    .await?;
    let connection = connect(&endpoint, addr).await?;
    let peer = client_pair(&connection, &identity, &client_tls, &mut trust, confirm_sas).await?;
    trust.save(&paths.trust_file)?;
    println!(
        "paired device={} name={} sas={}",
        peer.device_id, peer.name, peer.sas
    );
    connection.close(0u32.into(), b"paired");
    endpoint.wait_idle().await;
    Ok(())
}

async fn connect_cmd(
    paths: AppPaths,
    addr: SocketAddr,
    fingerprint: Option<&str>,
    resume: Option<Uuid>,
    dev_loopback: bool,
    message: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if addr.ip().is_loopback() && !dev_loopback {
        return Err("loopback target requires --dev-loopback".into());
    }
    paths.ensure()?;
    let identity = IdentityStore::load_or_create(&paths.identity_file)?;
    let mut trust = TrustStore::load_or_default(&paths.trust_file)?;
    let client_tls =
        TlsIdentity::load_or_create(&paths.tls_cert_file, &paths.tls_key_file, "fluxy-desktop")?;
    if let Some(server_id) = resume {
        let mut session =
            connect_trusted(addr, server_id, &identity, &client_tls, &mut trust).await?;
        let peer = session.peer();
        println!("resumed device={} name={}", peer.device_id, peer.name);
        chat_once(&mut session, message).await?;
        session.close().await;
        return Ok(());
    }

    let fingerprint = fingerprint.ok_or("--fingerprint is required for first pairing")?;
    let expected = parse_fingerprint(fingerprint)?;
    let endpoint = client_endpoint(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        &client_tls,
        expected,
    )
    .await?;
    let connection = connect(&endpoint, addr).await?;
    let peer = client_pair(&connection, &identity, &client_tls, &mut trust, confirm_sas).await?;
    trust.save(&paths.trust_file)?;
    println!(
        "paired device={} name={} sas={}",
        peer.device_id, peer.name, peer.sas
    );
    let mut session = ControlSession::open(&connection, &identity, &peer).await?;
    chat_once(&mut session, message).await?;
    session.close().await;
    connection.close(0u32.into(), b"paired");
    endpoint.wait_idle().await;
    Ok(())
}

async fn chat_once(
    session: &mut ControlSession,
    message: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("control ready");
    let Some(message) = message else {
        return Ok(());
    };
    let message_id = session.send_text(message).await?;
    let ack = tokio::time::timeout(std::time::Duration::from_secs(5), session.next_event())
        .await
        .map_err(|_| "timed out waiting for ACK")?
        .ok_or("session closed before ACK")?;
    match ack {
        SessionEvent::Ack {
            message_id: ack_id,
            delivered,
        } => println!("ack message={ack_id} delivered={delivered} id={message_id}"),
        SessionEvent::Disconnected { reason } => return Err(reason.into()),
        SessionEvent::Reconnected { .. } => return Err("unexpected reconnect before ACK".into()),
        SessionEvent::Message { .. } => return Err("unexpected message before ACK".into()),
        SessionEvent::ClipboardOffer { .. }
        | SessionEvent::ClipboardText { .. }
        | SessionEvent::ClipboardAck { .. }
        | SessionEvent::TransferOffer { .. }
        | SessionEvent::TransferAccept { .. }
        | SessionEvent::TransferReject { .. }
        | SessionEvent::TransferProgress { .. }
        | SessionEvent::TransferComplete { .. }
        | SessionEvent::TransferError { .. }
        | SessionEvent::TransferPause { .. }
        | SessionEvent::TransferCancel { .. }
        | SessionEvent::TransferResume { .. } => {
            return Err("unexpected event before ACK".into());
        }
    }
    Ok(())
}

fn confirm_sas(sas: &str) -> bool {
    eprintln!("SAS: {sas}");
    eprintln!("Compare this on both devices, then type yes to confirm:");
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim(), "yes" | "y")
}

fn parse_fingerprint(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let bytes = hex::decode(value.trim())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "fingerprint must be 32 bytes (64 hex characters)")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_m007_commands() {
        let cli = Cli::try_parse_from([
            "fluxy",
            "discover",
            "--timeout",
            "5",
            "--interface",
            "Wi-Fi",
            "--json",
        ])
        .unwrap();
        match cli.command {
            Command::Discover {
                timeout,
                interface,
                json,
                ..
            } => {
                assert_eq!(timeout, 5);
                assert_eq!(interface.as_deref(), Some("Wi-Fi"));
                assert!(json);
            }
            _ => panic!("expected discover command"),
        }

        let cli = Cli::try_parse_from(["fluxy", "pair", "127.0.0.1:4242", "--fingerprint", "00"])
            .unwrap();
        assert!(matches!(cli.command, Command::Pair { .. }));

        let cli = Cli::try_parse_from([
            "fluxy",
            "chat",
            "127.0.0.1:4242",
            "--device",
            "018f2b7c-3e4d-7a6b-8c9d-0e1f2a3b4c5d",
            "--message",
            "hello",
        ])
        .unwrap();
        assert!(matches!(cli.command, Command::Chat { .. }));
    }
}
