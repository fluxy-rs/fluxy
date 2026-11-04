//! M2-09 failure injection for the transfer path (matrix T-02/T-03/T-04/T-06).
//!
//! Every test drives the real driver from `fluxy-app` over real UDP loopback
//! through the chaos proxy: a cut link, a restarted receiver, a source that
//! changes under the sender, and chat that must stay responsive while the link
//! is saturated. The 10 GiB benchmark (T-01) is the ignored test at the bottom.

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fluxy_app::{
    HeadlessError, HeadlessTransfer, OfferView, ReceiveOutcome, SendOutcome, SendRequest,
};
use fluxy_core::{Identity, TrustStore};
use fluxy_net::{
    ControlSession, PairingLimiter, SessionConfig, SessionEvent, TlsIdentity, TransferConfig,
    TransferError, accept, client_endpoint, client_pair, connect, server_endpoint, server_pair,
};
use udp_chaos::{ChaosConfig, UdpChaosProxy, start};
use uuid::Uuid;

/// Append a timestamped marker to a file when FLUXY_FAULT_TRACE is set. A hung
/// test cannot flush its own output, so this is the only way to see how far it
/// got; it stays silent and cheap otherwise.
fn mark(stage: &str) {
    use std::io::Write as _;
    if std::env::var_os("FLUXY_FAULT_TRACE").is_none() {
        return;
    }
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let path = std::env::temp_dir().join("fluxy-fault-trace.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{:>8}ms {stage}", start.elapsed().as_millis());
    }
}

const BLOCK_SIZE: u32 = 4 * 1024 * 1024;
const OFFER_WAIT: Duration = Duration::from_secs(30);
const JOIN_WAIT: Duration = Duration::from_secs(60);

fn loopback() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

fn session_config() -> SessionConfig {
    SessionConfig {
        heartbeat_interval: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(5),
        max_text_bytes: 32 * 1024,
        max_clipboard_bytes: fluxy_net::MAX_CLIPBOARD_BYTES,
        seen_limit: 32,
    }
}

/// Tuning with a short chunk timeout so a simulated outage fails fast.
fn transfer_config(chunk_timeout_ms: u64, max_retries: u32) -> TransferConfig {
    let mut config = TransferConfig::default();
    config.scheduler.chunk_timeout = Duration::from_millis(chunk_timeout_ms);
    config.scheduler.max_retries = max_retries;
    config
}

/// Write a deterministic pseudo-random file without holding it all in memory.
fn write_pattern_file(path: &Path, size: u64) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut block = vec![0_u8; 1 << 20];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut written = 0_u64;
    while written < size {
        for chunk in block.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            chunk.copy_from_slice(&state.to_le_bytes()[..chunk.len()]);
        }
        let take = ((size - written) as usize).min(block.len());
        file.write_all(&block[..take]).unwrap();
        written += take as u64;
    }
    file.sync_all().unwrap();
}

fn append_byte(path: &Path) {
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(b"x").unwrap();
    file.sync_all().unwrap();
}

/// Fresh identities for one attempt.
struct Peers {
    server_identity: Identity,
    server_tls: TlsIdentity,
    client_identity: Identity,
    client_tls: TlsIdentity,
    fingerprint: [u8; 32],
}

impl Peers {
    fn new() -> Self {
        let server_tls = TlsIdentity::generate("receiver").unwrap();
        let client_tls = TlsIdentity::generate("sender").unwrap();
        Self {
            server_identity: Identity::generate(),
            fingerprint: server_tls.fingerprint,
            server_tls,
            client_identity: Identity::generate(),
            client_tls,
        }
    }
}

/// A live attempt: an optional fault proxy plus one control session per side.
struct Link {
    proxy: Option<UdpChaosProxy>,
    client: Option<ControlSession>,
    server: Option<ControlSession>,
}

impl Link {
    fn take_client(&mut self) -> ControlSession {
        self.client
            .take()
            .expect("the sender session was already taken")
    }

    fn take_server(&mut self) -> ControlSession {
        self.server
            .take()
            .expect("the receiver session was already taken")
    }

    fn dropped(&self) -> u64 {
        self.proxy
            .as_ref()
            .map(|proxy| proxy.handle.stats().dropped)
            .unwrap_or(0)
    }

    fn outage(&self, enabled: bool) {
        if let Some(proxy) = &self.proxy {
            proxy.handle.set_outage(enabled);
        }
    }

    async fn shutdown(&self) {
        if let Some(proxy) = &self.proxy {
            proxy.shutdown().await;
        }
    }
}

/// Pair two fresh peers and open a control session on each. A chaos config
/// inserts the fault proxy in front of the receiver; `None` connects straight
/// to it, which is what the chat test needs so that only the transfer itself
/// loads the link.
async fn open_link(peers: &Peers, chaos: Option<ChaosConfig>) -> Link {
    let server = server_endpoint(&peers.server_tls, loopback())
        .await
        .unwrap();
    let server_addr = server.local_addr().unwrap();
    let server_identity = peers.server_identity.clone();
    let server_tls = peers.server_tls.clone();
    let server_task = tokio::spawn(async move {
        let connection = accept(&server).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = server_pair(
            &connection,
            &server_identity,
            &server_tls,
            &mut trust,
            &mut PairingLimiter::default(),
            |_| true,
        )
        .await
        .unwrap();
        ControlSession::accept_with_config(
            &connection,
            &server_identity,
            &peer,
            session_config(),
            None,
        )
        .await
        .unwrap()
    });

    let proxy = match chaos {
        Some(config) => Some(start(server_addr, loopback(), config).await.unwrap()),
        None => None,
    };
    let target = proxy
        .as_ref()
        .map(|proxy| proxy.addr)
        .unwrap_or(server_addr);
    let endpoint = client_endpoint(loopback(), &peers.client_tls, peers.fingerprint)
        .await
        .unwrap();
    let connection = connect(&endpoint, target).await.unwrap();
    let mut trust = TrustStore::default();
    let peer = client_pair(
        &connection,
        &peers.client_identity,
        &peers.client_tls,
        &mut trust,
        |_| true,
    )
    .await
    .unwrap();
    let client = ControlSession::open_with_config(
        &connection,
        &peers.client_identity,
        &peer,
        session_config(),
        None,
    )
    .await
    .unwrap();
    let server = tokio::time::timeout(JOIN_WAIT, server_task)
        .await
        .expect("the receiver never accepted the connection")
        .unwrap();

    Link {
        proxy,
        client: Some(client),
        server: Some(server),
    }
}

/// Wait for the first offer and pull it, optionally aborting mid-flight.
async fn receive_one(
    driver: Arc<HeadlessTransfer>,
    mut session: ControlSession,
    identity: Identity,
    abort_after: Option<Duration>,
) -> Result<ReceiveOutcome, String> {
    let deadline = Instant::now() + OFFER_WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("no offer arrived".into());
        }
        let event = match tokio::time::timeout(remaining, session.next_event()).await {
            Ok(Some(event)) => event,
            Ok(None) => return Err("the session closed before an offer arrived".into()),
            Err(_) => return Err("no offer arrived".into()),
        };
        let Some(offer) = OfferView::from_event(&event) else {
            continue;
        };
        let connection = session.connection();
        let sender = session.sender();
        let session_id = session.session_id();
        let receive = driver.receive(&connection, &sender, session_id, &identity, offer, None);
        let result = match abort_after {
            Some(delay) => tokio::select! {
                result = receive => result,
                _ = tokio::time::sleep(delay) => return Err("receiver aborted".into()),
            },
            None => receive.await,
        };
        return result.map_err(|error| format!("{error} [{}]", error.code_name()));
    }
}

/// Send one file and return the outcome or a printable error.
async fn send_one(
    driver: Arc<HeadlessTransfer>,
    mut session: ControlSession,
    identity: Identity,
    request: SendRequest,
) -> Result<SendOutcome, String> {
    let mut on_event = |_event: SessionEvent| {};
    driver
        .send(&mut session, &identity, request, &mut on_event)
        .await
        .map_err(|error| format!("{error} [{}]", error.code_name()))
}

/// A source file plus a sender and a receiver rooted in separate data dirs.
struct Fixture {
    _dir: tempfile::TempDir,
    source: PathBuf,
    sender: Arc<HeadlessTransfer>,
    receiver: Arc<HeadlessTransfer>,
}

impl Fixture {
    fn new(size: u64, config: TransferConfig) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("payload.bin");
        write_pattern_file(&source, size);
        let sender = Arc::new(HeadlessTransfer::new(dir.path().join("sender"), config).unwrap());
        let receiver =
            Arc::new(HeadlessTransfer::new(dir.path().join("receiver"), config).unwrap());
        Self {
            _dir: dir,
            source,
            sender,
            receiver,
        }
    }

    fn request(&self, transfer_id: Uuid) -> SendRequest {
        SendRequest::new(self.source.clone())
            .with_block_size(BLOCK_SIZE)
            .with_transfer_id(transfer_id)
    }

    fn received_path(&self) -> PathBuf {
        self.receiver.received_dir().join("payload.bin")
    }

    /// Poll the receiver's checkpoint until it has recorded some blocks.
    async fn wait_for_blocks(&self, transfer_id: Uuid, blocks: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let done = self
                .receiver
                .checkpoints()
                .load_progress(transfer_id)
                .unwrap()
                .map(|checkpoint| checkpoint.blocks_done())
                .unwrap_or(0);
            if done >= blocks {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the receiver never checkpointed {blocks} blocks (saw {done})"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_short_outage_recovers_without_losing_blocks() {
    let fixture = Fixture::new(24 * 1024 * 1024, transfer_config(2_000, 5));
    let transfer_id = Uuid::now_v7();
    let peers = Peers::new();
    let mut link = open_link(&peers, Some(ChaosConfig::default())).await;

    let receiver = tokio::spawn(receive_one(
        fixture.receiver.clone(),
        link.take_server(),
        peers.server_identity.clone(),
        None,
    ));
    let sender = tokio::spawn(send_one(
        fixture.sender.clone(),
        link.take_client(),
        peers.client_identity.clone(),
        fixture.request(transfer_id),
    ));

    fixture.wait_for_blocks(transfer_id, 1).await;
    link.outage(true);
    tokio::time::sleep(Duration::from_millis(900)).await;
    link.outage(false);

    let sent = tokio::time::timeout(JOIN_WAIT, sender)
        .await
        .expect("the sender never finished")
        .unwrap();
    let received = tokio::time::timeout(JOIN_WAIT, receiver)
        .await
        .expect("the receiver never finished")
        .unwrap();

    let sent = sent.expect("the sender failed after a short outage");
    let received = received.expect("the receiver failed after a short outage");
    assert_eq!(sent.root_hash, received.root_hash);
    assert_eq!(received.file_size, 24 * 1024 * 1024);
    assert!(link.dropped() > 0, "the outage dropped no datagrams");
    assert!(fixture.received_path().is_file());
    link.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_receiver_restart_resumes_from_its_checkpoint() {
    let fixture = Fixture::new(32 * 1024 * 1024, transfer_config(800, 1));
    let transfer_id = Uuid::now_v7();
    let peers = Peers::new();

    // Phase 1: the link is cut once the first blocks landed and stays down, so
    // the receiver gives up after its retry budget and keeps its checkpoint.
    let mut link = open_link(&peers, Some(ChaosConfig::default())).await;
    let receiver = tokio::spawn(receive_one(
        fixture.receiver.clone(),
        link.take_server(),
        peers.server_identity.clone(),
        None,
    ));
    let sender = tokio::spawn(send_one(
        fixture.sender.clone(),
        link.take_client(),
        peers.client_identity.clone(),
        fixture.request(transfer_id),
    ));

    fixture.wait_for_blocks(transfer_id, 1).await;
    link.outage(true);
    let first = tokio::time::timeout(JOIN_WAIT, receiver)
        .await
        .expect("the receiver never gave up")
        .unwrap();
    assert!(
        first.is_err(),
        "the receiver survived a dead link: {first:?}"
    );
    let _ = tokio::time::timeout(Duration::from_secs(5), sender).await;
    let checkpointed = fixture
        .receiver
        .checkpoints()
        .load_progress(transfer_id)
        .unwrap()
        .expect("the receiver lost its checkpoint");
    let blocks_before = checkpointed.blocks_done();
    assert!(blocks_before > 0, "no blocks were checkpointed");
    assert!(
        blocks_before < 8,
        "the whole file arrived before the link was cut"
    );
    link.shutdown().await;
    drop(link);

    // Phase 2: a restarted receiver (fresh driver, same data directory) plus a
    // retry that reuses the transfer id must only pull the missing blocks.
    let restart = Peers::new();
    let mut link = open_link(&restart, Some(ChaosConfig::default())).await;
    let receiver = tokio::spawn(receive_one(
        fixture.receiver.clone(),
        link.take_server(),
        restart.server_identity.clone(),
        None,
    ));
    let sender = tokio::spawn(send_one(
        fixture.sender.clone(),
        link.take_client(),
        restart.client_identity.clone(),
        fixture.request(transfer_id),
    ));

    let sent = tokio::time::timeout(JOIN_WAIT, sender)
        .await
        .expect("the sender never finished")
        .unwrap()
        .expect("the sender failed on resume");
    let received = tokio::time::timeout(JOIN_WAIT, receiver)
        .await
        .expect("the receiver never finished")
        .unwrap()
        .expect("the receiver failed on resume");

    assert!(received.resumed, "the receiver did not resume a checkpoint");
    assert_eq!(sent.root_hash, received.root_hash);
    assert!(
        received.missing_bytes_before < received.file_size,
        "the resume planned the whole file again: {}",
        received.missing_bytes_before
    );
    assert_eq!(
        received.bytes_written, received.missing_bytes_before,
        "the resumed pull wrote more than the missing bytes"
    );
    assert!(fixture.received_path().is_file());
    link.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_source_is_refused_with_3002() {
    let fixture = Fixture::new(16 * 1024 * 1024, transfer_config(500, 1));
    let transfer_id = Uuid::now_v7();
    let peers = Peers::new();
    let mut link = open_link(&peers, Some(ChaosConfig::default())).await;

    let receiver = tokio::spawn(receive_one(
        fixture.receiver.clone(),
        link.take_server(),
        peers.server_identity.clone(),
        None,
    ));

    // Offer first, so the sender holds a source identity from before the change.
    let request = fixture.request(transfer_id);
    let mut client = link.take_client();
    let prepared = fixture
        .sender
        .offer(&mut client, &request, &mut |_| {})
        .await
        .expect("the offer was not accepted");
    append_byte(&fixture.source);

    let connection = client.connection();
    let sender_handle = client.sender();
    let session_id = client.session_id();
    let result = fixture
        .sender
        .serve(
            &connection,
            &sender_handle,
            session_id,
            &peers.client_identity,
            prepared,
        )
        .await;
    let error = result.expect_err("the sender served a changed source");
    assert_eq!(error.code_name(), "source_changed");
    assert_eq!(error.code_number(), 3002);
    assert!(!error.is_retryable());
    assert!(matches!(
        error,
        HeadlessError::Transfer(TransferError::File(_))
    ));

    // The receiver must not leave a complete file behind.
    let first = tokio::time::timeout(JOIN_WAIT, receiver)
        .await
        .expect("the receiver never finished")
        .unwrap();
    assert!(first.is_err(), "the receiver completed a refused transfer");
    assert!(
        !fixture.received_path().exists(),
        "a refused transfer produced {}",
        fixture.received_path().display()
    );
    link.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_stays_responsive_while_the_link_is_saturated() {
    const MESSAGES: usize = 100;
    const P95_BUDGET: Duration = Duration::from_millis(250);

    mark("T-06 start");
    let fixture = Fixture::new(64 * 1024 * 1024, TransferConfig::default());
    let transfer_id = Uuid::now_v7();
    let peers = Peers::new();
    mark("fixture ready");
    let mut link = open_link(&peers, None).await;
    mark("link open");

    let mut receiver_session = link.take_server();
    let client = link.take_client();
    let request = fixture.request(transfer_id);

    // The sender offers from its own task: waiting for the decision keeps its
    // control stream drained, exactly like the desktop app does.
    let offering = tokio::spawn({
        let driver = fixture.sender.clone();
        async move {
            let mut session = client;
            let mut on_event = |_event: SessionEvent| {};
            let prepared = driver.offer(&mut session, &request, &mut on_event).await;
            (session, prepared)
        }
    });

    // The receiver pulls on a background task while its control stream stays
    // drained here, so a full event channel never delays an acknowledgement.
    let started = Instant::now();
    let offer_deadline = Instant::now() + OFFER_WAIT;
    let offer = loop {
        let remaining = offer_deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "no offer arrived");
        let event = tokio::time::timeout(remaining, receiver_session.next_event())
            .await
            .expect("no offer arrived")
            .expect("the receiver session closed before the offer");
        if let Some(offer) = OfferView::from_event(&event) {
            break offer;
        }
    };
    let receiver_connection = receiver_session.connection();
    let receiver_sender = receiver_session.sender();
    let receiver_session_id = receiver_session.session_id();
    let pulling = tokio::spawn({
        let driver = fixture.receiver.clone();
        let identity = peers.server_identity.clone();
        async move {
            driver
                .receive(
                    &receiver_connection,
                    &receiver_sender,
                    receiver_session_id,
                    &identity,
                    offer,
                    None,
                )
                .await
                .map_err(|error| format!("{error} [{}]", error.code_name()))
        }
    });

    mark("receiver offer seen");
    let (mut client_session, prepared) = offering
        .await
        .expect("the sender task panicked while offering");
    mark("offer accepted");
    let prepared = prepared.expect("the offer was not accepted");
    let connection = client_session.connection();
    let sender_handle = client_session.sender();
    let chat_sender = client_session.sender();
    let session_id = client_session.session_id();
    let serving = tokio::spawn({
        let driver = fixture.sender.clone();
        let identity = peers.client_identity.clone();
        async move {
            driver
                .serve(&connection, &sender_handle, session_id, &identity, prepared)
                .await
                .map_err(|error| format!("{error} [{}]", error.code_name()))
        }
    });
    // The sender keeps draining its own control stream for the whole transfer.
    let draining = tokio::spawn(async move {
        let mut count = 0usize;
        while let Some(event) = client_session.next_event().await {
            count += 1;
            if let SessionEvent::Disconnected { reason } = &event {
                eprintln!("T-06: sender disconnected after {count} events: {reason}");
                break;
            }
            if count.is_multiple_of(25) {
                eprintln!("T-06: sender drained {count} events");
            }
        }
        eprintln!("T-06: sender drain loop ended after {count} events");
    });

    // Chat is measured one way: the sender stamps each message and the receiver
    // records how long it took to arrive. Sending never waits for delivery, so
    // a slow receive path cannot throttle the burst the way the real app's
    // fire-and-forget send does not either.
    let receiving = tokio::spawn(async move {
        let mut session = receiver_session;
        let mut arrived = vec![false; MESSAGES];
        let mut latencies: Vec<Duration> = Vec::with_capacity(MESSAGES);
        let deadline = Instant::now() + Duration::from_secs(60);
        while latencies.len() < MESSAGES {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, session.next_event()).await {
                Ok(Some(SessionEvent::Message { text, .. })) => {
                    let mut parts = text.split(' ');
                    let index = parts
                        .next()
                        .and_then(|name| name.rsplit('-').next())
                        .and_then(|value| value.parse::<usize>().ok())
                        .expect("the chat text carried no index");
                    let stamp = parts
                        .next()
                        .and_then(|value| value.parse::<u128>().ok())
                        .expect("the chat text carried no send stamp");
                    assert!(index < MESSAGES, "unexpected chat index {index}");
                    assert!(!arrived[index], "chat message {index} arrived twice");
                    arrived[index] = true;
                    let now = started.elapsed().as_micros();
                    latencies.push(Duration::from_micros((now - stamp) as u64));
                    if latencies.len().is_multiple_of(10) {
                        mark(&format!("received {} messages", latencies.len()));
                    }
                }
                Ok(Some(SessionEvent::Disconnected { reason })) => {
                    mark(&format!("receiver session disconnected: {reason}"));
                    break;
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    mark("receiver session closed");
                    break;
                }
                Err(_) => break,
            }
        }
        (arrived, latencies)
    });

    let mut send_failed = false;
    for index in 0..MESSAGES {
        let stamp = started.elapsed().as_micros();
        let send_started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            chat_sender.send_text(format!("during-transfer-{index} {stamp}")),
        )
        .await;
        let took = send_started.elapsed();
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                mark(&format!("send {index} failed: {error}"));
                send_failed = true;
                break;
            }
            Err(_) => {
                mark(&format!("send {index} timed out after {took:?}"));
                send_failed = true;
                break;
            }
        }
        if took > Duration::from_millis(200) || index % 25 == 0 {
            mark(&format!("sent {index} in {took:?}"));
        }
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
    mark("all messages sent");
    let (arrived, mut latencies) = receiving.await.unwrap();
    mark(&format!("{} messages arrived", latencies.len()));
    assert!(
        !send_failed,
        "the sender could not hand every chat message to its session"
    );

    let served = tokio::time::timeout(JOIN_WAIT, serving)
        .await
        .expect("the sender never finished")
        .unwrap()
        .expect("the sender failed");
    let received = tokio::time::timeout(JOIN_WAIT, pulling)
        .await
        .expect("the receiver never finished")
        .unwrap()
        .expect("the receiver failed");
    let transfer_time = started.elapsed();
    draining.abort();

    assert_eq!(latencies.len(), MESSAGES, "not every chat message arrived");
    assert!(
        arrived.iter().all(|seen| *seen),
        "a chat message never arrived"
    );
    latencies.sort();
    let p95 = latencies[(MESSAGES * 95 / 100) - 1];
    assert_eq!(served.root_hash, received.root_hash);
    assert!(
        p95 <= P95_BUDGET,
        "chat p95 was {p95:?} during a {transfer_time:?} transfer (budget {P95_BUDGET:?})"
    );
    eprintln!(
        "T-06: {MESSAGES} messages, p95 {p95:?}, transfer {transfer_time:?}, {} bytes",
        received.file_size
    );
    link.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "10 GiB benchmark; set FLUXY_BENCH_FILE to a file of at least 10 GiB"]
async fn a_ten_gib_transfer_matches_its_root_hash() {
    let Some(path) = std::env::var_os("FLUXY_BENCH_FILE").map(PathBuf::from) else {
        panic!("FLUXY_BENCH_FILE must point at a file of at least 10 GiB");
    };
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(
        size >= 10 * 1024 * 1024 * 1024,
        "FLUXY_BENCH_FILE is only {size} bytes"
    );

    let dir = tempfile::tempdir().unwrap();
    let config = TransferConfig::default();
    let sender = Arc::new(HeadlessTransfer::new(dir.path().join("sender"), config).unwrap());
    let receiver = Arc::new(HeadlessTransfer::new(dir.path().join("receiver"), config).unwrap());
    let transfer_id = Uuid::now_v7();
    let peers = Peers::new();
    let mut link = open_link(&peers, Some(ChaosConfig::default())).await;

    let hash_started = Instant::now();
    let cached = sender.manifest_for(&path, BLOCK_SIZE).unwrap();
    let hash_time = hash_started.elapsed();
    let manifest = cached.manifest;
    eprintln!(
        "T-01: {} bytes, {} blocks, prehash {hash_time:?} ({:.0} MiB/s)",
        manifest.file_size,
        manifest.block_count(),
        (manifest.file_size as f64 / (1024.0 * 1024.0)) / hash_time.as_secs_f64()
    );

    let request = SendRequest::new(path.clone())
        .with_block_size(BLOCK_SIZE)
        .with_transfer_id(transfer_id);
    let receiver_task = tokio::spawn(receive_one(
        receiver.clone(),
        link.take_server(),
        peers.server_identity.clone(),
        None,
    ));
    let sender_task = tokio::spawn(send_one(
        sender.clone(),
        link.take_client(),
        peers.client_identity.clone(),
        request,
    ));

    let started = Instant::now();
    let sent = tokio::time::timeout(Duration::from_secs(3600), sender_task)
        .await
        .expect("the sender timed out")
        .unwrap()
        .expect("the sender failed");
    let received = tokio::time::timeout(Duration::from_secs(3600), receiver_task)
        .await
        .expect("the receiver timed out")
        .unwrap()
        .expect("the receiver failed");
    let elapsed = started.elapsed();

    assert_eq!(sent.root_hash, received.root_hash);
    assert_eq!(received.file_size, size);
    eprintln!(
        "T-01: transfer {elapsed:?} ({:.0} MiB/s), root hash {}",
        (size as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64(),
        received.root_hash
    );
    link.shutdown().await;
}
