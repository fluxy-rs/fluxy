use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use fluxy_core::{Identity, TrustStore};
use fluxy_net::{
    ControlSession, PairingLimiter, SessionConfig, SessionEvent, TlsIdentity, accept,
    client_endpoint, client_pair, connect, server_endpoint,
};
use tokio::time::timeout;
use udp_chaos::{ChaosConfig, ChaosStatsSnapshot, start};

fn loopback() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

fn test_config() -> SessionConfig {
    SessionConfig {
        heartbeat_interval: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(2),
        max_text_bytes: 32 * 1024,
        max_clipboard_bytes: fluxy_net::MAX_CLIPBOARD_BYTES,
        seen_limit: 32,
    }
}

async fn exchange(control: &mut ControlSession, text: &str) {
    let message_id = control.send_text(text).await.unwrap();
    loop {
        let event = timeout(Duration::from_secs(5), control.next_event())
            .await
            .expect("timeout waiting for ACK")
            .expect("session closed before ACK");
        match event {
            SessionEvent::Ack {
                message_id: ack_id,
                delivered,
            } => {
                assert_eq!(ack_id, message_id);
                assert!(delivered);
                return;
            }
            SessionEvent::Reconnected { .. } => continue,
            SessionEvent::Message { .. } => continue,
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
            | SessionEvent::TransferResume { .. } => continue,
            SessionEvent::Disconnected { reason } => panic!("unexpected disconnect: {reason}"),
        }
    }
}

async fn run_session(config: ChaosConfig, outage: bool) -> ChaosStatsSnapshot {
    let server_identity = Identity::generate();
    let server_tls = TlsIdentity::generate("server").unwrap();
    let server_fingerprint = server_tls.fingerprint;
    let server = server_endpoint(&server_tls, loopback()).await.unwrap();
    let server_addr = server.local_addr().unwrap();

    let server_identity_task = server_identity.clone();
    let server_tls_task = server_tls.clone();
    let server_task = tokio::spawn(async move {
        let connection = accept(&server).await.unwrap();
        let mut trust = TrustStore::default();
        let peer = fluxy_net::server_pair(
            &connection,
            &server_identity_task,
            &server_tls_task,
            &mut trust,
            &mut PairingLimiter::default(),
            |_| true,
        )
        .await
        .unwrap();
        let mut control = ControlSession::accept_with_config(
            &connection,
            &server_identity_task,
            &peer,
            test_config(),
            None,
        )
        .await
        .unwrap();
        let mut messages = Vec::new();
        for _ in 0..2 {
            let event = timeout(Duration::from_secs(5), control.next_event())
                .await
                .expect("server timed out waiting for chat")
                .expect("server session closed");
            match event {
                SessionEvent::Message { text, .. } => messages.push(text),
                other => panic!("expected message, got {other:?}"),
            }
        }
        timeout(Duration::from_secs(5), async {
            loop {
                match control.next_event().await {
                    None | Some(SessionEvent::Disconnected { .. }) => break,
                    Some(_) => {}
                }
            }
        })
        .await
        .expect("client did not close the control stream");
        control.close().await;
        connection.close(0u32.into(), b"done");
        messages
    });

    let proxy = start(server_addr, loopback(), config).await.unwrap();
    let client_identity = Identity::generate();
    let client_tls = TlsIdentity::generate("client").unwrap();
    let endpoint = client_endpoint(loopback(), &client_tls, server_fingerprint)
        .await
        .unwrap();
    let connection = connect(&endpoint, proxy.addr).await.unwrap();
    let mut trust = TrustStore::default();
    let peer = client_pair(
        &connection,
        &client_identity,
        &client_tls,
        &mut trust,
        |_| true,
    )
    .await
    .unwrap();
    let mut control =
        ControlSession::open_with_config(&connection, &client_identity, &peer, test_config(), None)
            .await
            .unwrap();

    exchange(&mut control, "before-outage").await;
    if outage {
        let restore = {
            let handle = proxy.handle.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                handle.set_outage(false);
            })
        };
        proxy.handle.set_outage(true);
        exchange(&mut control, "after-outage").await;
        restore.await.unwrap();
    } else {
        exchange(&mut control, "after-outage").await;
    }

    control.close().await;
    connection.close(0u32.into(), b"done");
    endpoint.wait_idle().await;

    let messages = timeout(Duration::from_secs(5), server_task)
        .await
        .expect("server did not finish")
        .expect("server task failed");
    assert_eq!(messages, vec!["before-outage", "after-outage"]);

    let stats = proxy.handle.stats();
    proxy.shutdown().await;
    stats
}

#[tokio::test(flavor = "multi_thread")]
async fn session_survives_a_packet_outage() {
    let stats = run_session(ChaosConfig::default(), true).await;
    assert!(stats.forwarded > 0);
    assert!(stats.dropped > 0, "outage did not drop any datagrams");
}

#[tokio::test(flavor = "multi_thread")]
async fn session_survives_delayed_packets() {
    let stats = run_session(
        ChaosConfig {
            delay: Duration::from_millis(20),
            ..ChaosConfig::default()
        },
        false,
    )
    .await;
    assert!(stats.forwarded > 0);
    assert!(stats.delayed > 0, "delay was not applied");
}

#[tokio::test(flavor = "multi_thread")]
async fn session_survives_duplicate_and_reordered_packets() {
    let stats = run_session(
        ChaosConfig {
            duplicate: 1.0,
            reorder: 1.0,
            delay: Duration::from_millis(5),
            ..ChaosConfig::default()
        },
        false,
    )
    .await;
    assert!(stats.forwarded > 0);
    assert!(stats.duplicated > 0);
    assert!(stats.reordered > 0);
}
